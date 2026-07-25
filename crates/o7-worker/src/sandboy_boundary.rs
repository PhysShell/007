//! The Sandboy boundary: a [`ProcessBoundary`] that confines the child with real Linux
//! isolation (Landlock + seccomp) and attests [`EnforcementLevel::FullyEnforced`].
//!
//! The confinement runs in an EXTERNAL `sandboy` backend binary (every 007 crate forbids
//! `unsafe`; see `docs/architecture/sandboy-boundary.md`). This boundary launches the backend
//! through the spawn seam. In a MONITOR topology the backend installs the confinement, starts
//! the target as a CHILD (same PID namespace, so PR 3's sealed-memfd exec via
//! `/proc/<verifier_pid>/fd/<n>` still resolves), and REMAINS ALIVE as the cgroup-owning
//! monitor enforcing the deadline + process-tree kill — it never `exec`s into the target.
//!
//! Trust is bound to OBJECTS, not paths:
//! - the BACKEND is a [`BackendImage`] that OWNS a held read-only descriptor + its digest + the
//!   expected [`BackendIdentity`]. The launch execs the held descriptor via
//!   [`BackendImage::descriptor_path`] (not a re-resolved path, so a post-construction swap
//!   cannot substitute a different binary), and the report must echo the expected identity;
//! - the TARGET's digest is derived from the exact held bytes the backend confined, and the
//!   report must echo it.
//!
//! Planes are SEPARATE: the backend is a TRUSTED launcher and runs from a fixed trusted cwd
//! (`/`) with a trusted control-plane environment — never the untrusted target's cwd/env
//! (LD_PRELOAD, …). The target's own cwd rides the (non-sensitive) argv; its environment
//! travels out-of-band to the confined target, never through the backend's process env.
//!
//! Authorization is a BARRIER: the two-phase handshake (`--report-fd` for the report,
//! `--control-fd` for the parent GO) means the backend holds the confined target BEFORE it
//! starts until the parent has verified the report and sent GO. A malformed / mis-bound /
//! downgraded report, a NACK, EOF, or a cancel makes the backend kill its cgroup and reap — the
//! target never runs on an unverified report.
//!
//! LIVE + GREEN here: backend acquisition (fail-closed on a digest mismatch), policy
//! validation, nonce minting (fail-closed on RNG failure), the confined-backend argv with plane
//! separation, the exec-permission gate, and the pure fail-closed [`SandboyBoundary::verify_report`].
//! RED: the confined live launch itself.

use std::collections::BTreeMap;
use std::fs::File;
use std::os::unix::io::{AsRawFd as _, RawFd};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::{ffi::OsString, io};

use async_trait::async_trait;

use o7_sandbox_protocol::frame::{self, FrameError};
use o7_sandbox_protocol::ids::{BackendIdentity, Digest256, LaunchNonce};
use o7_sandbox_protocol::policy::{NetworkPolicy, SandboxPolicy, SandboxPolicyError};

use crate::boundary::{
    BoundaryAttestation, BoundaryError, BoundaryEvidence, BoundaryKind, BoundaryLaunch,
    BoundarySpawnSpec, EnforcementLevel, ProcessBoundary,
};

/// A backend bound to a specific immutable OBJECT, not a re-resolvable path. It OWNS an open,
/// read-only descriptor to the backend binary for the lifetime of the boundary: the digest is
/// re-read back THROUGH that held descriptor (`/proc/self/fd/<n>`) and compared at construction,
/// and the launch execs the held object via [`BackendImage::descriptor_path`]
/// (`/proc/<owner_pid>/fd/<n>`) — so a substitution of the source path after construction cannot
/// change what runs. The fields are private; the only way to build one is
/// [`BackendImage::acquire`], which fails closed on a digest mismatch.
#[derive(Debug, Clone)]
pub struct BackendImage {
    /// The HELD descriptor. Retained (via `Arc`) for the whole boundary lifetime; dropping the
    /// original acquirer does not invalidate it.
    descriptor: Arc<File>,
    digest: Digest256,
    identity: BackendIdentity,
}

impl BackendImage {
    /// Acquire the backend object: open it, read its bytes back THROUGH the held descriptor,
    /// verify they hash to `expected_digest`, and RETAIN the descriptor. After this returns,
    /// the object is pinned to the held fd — a swap of `path` on disk cannot change it.
    ///
    /// # Errors
    /// [`SandboyLaunchError::BackendOpen`] if the object cannot be opened/read;
    /// [`SandboyLaunchError::BackendObjectMismatch`] if the held bytes do not match the digest.
    pub fn acquire(
        path: &Path,
        expected_digest: Digest256,
        identity: BackendIdentity,
    ) -> Result<Self, SandboyLaunchError> {
        let file = File::open(path).map_err(|e| SandboyLaunchError::BackendOpen(e.to_string()))?;
        // Read back THROUGH the held fd, not the path — binds the digest to the object we hold.
        let held_path = format!("/proc/self/fd/{}", file.as_raw_fd());
        let bytes = std::fs::read(&held_path)
            .map_err(|e| SandboyLaunchError::BackendOpen(e.to_string()))?;
        if Digest256::of_bytes(&bytes) != expected_digest {
            return Err(SandboyLaunchError::BackendObjectMismatch);
        }
        Ok(Self {
            descriptor: Arc::new(file),
            digest: expected_digest,
            identity,
        })
    }

    /// The exec path of the HELD object: `/proc/<owner_pid>/fd/<n>`, valid while the boundary
    /// (and thus this held descriptor) lives.
    #[must_use]
    pub fn descriptor_path(&self) -> PathBuf {
        PathBuf::from(format!(
            "/proc/{}/fd/{}",
            std::process::id(),
            self.descriptor.as_raw_fd()
        ))
    }

    /// The expected backend identity a report must echo.
    #[must_use]
    pub fn identity(&self) -> &BackendIdentity {
        &self.identity
    }

    /// The digest of the held backend object.
    #[must_use]
    pub fn digest(&self) -> &Digest256 {
        &self.digest
    }
}

/// Mints a 128-bit launch nonce. Abstracted so tests can inject a deterministic source and a
/// forced RNG failure; production uses [`OsNonceSource`] (the OS CSPRNG, no fallback).
pub trait NonceSource: Send + Sync {
    /// Mint a fresh nonce, or fail (an RNG failure must fail the launch, never fall back).
    ///
    /// # Errors
    /// [`NonceError`] if randomness could not be obtained.
    fn mint(&self) -> Result<LaunchNonce, NonceError>;
}

/// The OS CSPRNG nonce source: 128 bits straight from `getrandom`, no timestamp/counter
/// fallback.
#[derive(Debug, Clone, Copy, Default)]
pub struct OsNonceSource;

impl NonceSource for OsNonceSource {
    fn mint(&self) -> Result<LaunchNonce, NonceError> {
        let mut bytes = [0u8; 16];
        getrandom::getrandom(&mut bytes).map_err(|e| NonceError(e.to_string()))?;
        Ok(LaunchNonce::from_bytes(bytes))
    }
}

/// A failure obtaining randomness for a launch nonce.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("launch nonce RNG failure: {0}")]
pub struct NonceError(pub String);

/// A boundary that confines the child through an external `sandboy` backend and attests full
/// enforcement. Constructed only with a validated full-confinement policy and an
/// acquisition-bound backend, so a constructed instance always intends `FullyEnforced` — there
/// is no weaker mode and no silent fallback.
#[derive(Clone)]
pub struct SandboyBoundary {
    backend: BackendImage,
    policy: SandboxPolicy,
    nonce_source: Arc<dyn NonceSource>,
    /// The TRUSTED control-plane environment handed to the backend (never the target's). Empty
    /// in production; tests set backend configuration (e.g. a fake mode) here — NOT via the
    /// untrusted target environment.
    backend_env: BTreeMap<OsString, OsString>,
}

impl std::fmt::Debug for SandboyBoundary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The nonce source is an opaque trait object; the backend + policy are the identity.
        f.debug_struct("SandboyBoundary")
            .field("backend", &self.backend)
            .field("policy", &self.policy)
            .finish_non_exhaustive()
    }
}

/// Why a Sandboy launch cannot be trusted — at construction OR when verifying a report/object.
#[derive(Debug, thiserror::Error)]
pub enum SandboyLaunchError {
    /// The confinement policy is not capable of full confinement.
    #[error("sandbox policy is not fully confining: {0}")]
    Policy(#[from] SandboxPolicyError),
    /// The backend object could not be opened/read for acquisition.
    #[error("sandboy backend object could not be acquired: {0}")]
    BackendOpen(String),
    /// The held backend object's bytes do not match its pinned digest (substitution / TOCTOU).
    #[error("backend object digest does not match the trusted image")]
    BackendObjectMismatch,
    /// The report names a different backend than the expected identity.
    #[error("report backend identity does not match the expected backend")]
    BackendMismatch,
    /// The report frame was malformed / oversized / truncated / trailing.
    #[error("report frame invalid: {0}")]
    Frame(#[from] FrameError),
    /// The report is bound to a different policy than the one installed.
    #[error("report policy_digest does not match the installed policy")]
    PolicyDigestMismatch,
    /// The report is bound to a different (stale/replayed) launch.
    #[error("report launch_nonce does not match this launch")]
    NonceMismatch,
    /// The report is bound to a different target than the one confined.
    #[error("report target_digest does not match the confined target")]
    TargetMismatch,
    /// The backend did not establish full enforcement on every dimension.
    #[error("backend did not establish full enforcement (a dimension was downgraded)")]
    NotFullyEnforced,
}

impl SandboyBoundary {
    /// Build a Sandboy boundary from an acquired (held) backend and a confinement policy,
    /// using the OS CSPRNG for launch nonces.
    ///
    /// # Errors
    /// [`SandboyLaunchError::Policy`] if the policy cannot fully confine.
    pub fn new(backend: BackendImage, policy: SandboxPolicy) -> Result<Self, SandboyLaunchError> {
        Self::with_nonce_source(backend, policy, Arc::new(OsNonceSource))
    }

    /// As [`SandboyBoundary::new`], with an injected nonce source (deterministic tests / forced
    /// RNG failure).
    ///
    /// # Errors
    /// As [`SandboyBoundary::new`].
    pub fn with_nonce_source(
        backend: BackendImage,
        policy: SandboxPolicy,
        nonce_source: Arc<dyn NonceSource>,
    ) -> Result<Self, SandboyLaunchError> {
        policy.validate()?;
        Ok(Self {
            backend,
            policy,
            nonce_source,
            backend_env: BTreeMap::new(),
        })
    }

    /// Set the TRUSTED control-plane environment for the backend (default empty). This is the
    /// backend's OWN environment, distinct from the untrusted target's — used for backend
    /// configuration, never for passing target-controlled variables.
    #[must_use]
    pub fn with_backend_env(mut self, env: BTreeMap<OsString, OsString>) -> Self {
        self.backend_env = env;
        self
    }

    /// The policy this boundary installs.
    #[must_use]
    pub fn policy(&self) -> &SandboxPolicy {
        &self.policy
    }

    /// The trusted backend image.
    #[must_use]
    pub fn backend(&self) -> &BackendImage {
        &self.backend
    }

    /// The digest of a target object's exact bytes — the value the report's `target_digest`
    /// must equal. The launch computes this from the HELD bytes it hands the backend, binding
    /// the report to the object that actually ran.
    #[must_use]
    pub fn target_digest_of(bytes: &[u8]) -> Digest256 {
        Digest256::of_bytes(bytes)
    }

    /// Construct the confined backend invocation for a target `spec`, or fail closed. The
    /// launched executable is the HELD backend descriptor; the report and control descriptors
    /// and the launch nonce precede the target after `--`.
    ///
    /// # Errors
    /// [`BoundaryError::Spawn`] if the target executable is not permitted by the policy.
    pub fn backend_spawn_spec(
        &self,
        spec: &BoundarySpawnSpec,
        report_fd: RawFd,
        control_fd: RawFd,
        launch_nonce: &LaunchNonce,
    ) -> Result<BoundarySpawnSpec, BoundaryError> {
        if !self.policy.permits_exec(&spec.executable) {
            return Err(BoundaryError::Spawn(io::Error::other(format!(
                "sandbox policy does not permit executing {}; refusing to launch",
                spec.executable.display()
            ))));
        }

        let mut arguments: Vec<OsString> = vec![
            OsString::from("run"),
            // The dedicated report descriptor (one versioned length-bounded frame).
            OsString::from("--report-fd"),
            OsString::from(report_fd.to_string()),
            // The control descriptor: the backend holds the target until it reads the parent's
            // GO here; a NACK/EOF makes it kill the cgroup and reap.
            OsString::from("--control-fd"),
            OsString::from(control_fd.to_string()),
            // The per-launch nonce the backend must echo, binding the report to THIS spawn.
            OsString::from("--launch-nonce"),
            OsString::from(launch_nonce.as_str()),
        ];
        match self.policy.network {
            NetworkPolicy::DenyAll => arguments.push(OsString::from("--deny-net")),
        }
        arguments.push(OsString::from("--worktree"));
        arguments.push(self.policy.worktree.clone().into_os_string());
        // Enforced `timeout >= 1ms` by policy validation, so this is never `0`.
        let timeout_ms = self.policy.timeout.as_millis();
        arguments.push(OsString::from("--timeout-ms"));
        arguments.push(OsString::from(timeout_ms.to_string()));
        for path in &self.policy.allow_exec {
            arguments.push(OsString::from("--allow-exec"));
            arguments.push(path.clone().into_os_string());
        }
        for name in &self.policy.env_allowlist {
            arguments.push(OsString::from("--allow-env"));
            arguments.push(name.clone());
        }
        // The target's cwd is not secret (unlike its env), so it may ride the argv; the
        // target's ENVIRONMENT does not — it travels out-of-band to the confined target.
        arguments.push(OsString::from("--target-cwd"));
        arguments.push(spec.working_directory.clone().into_os_string());
        arguments.push(OsString::from("--"));
        arguments.push(spec.executable.clone().into_os_string());
        arguments.extend(spec.arguments.iter().cloned());

        Ok(BoundarySpawnSpec {
            // Exec the HELD backend object via its owned descriptor path, not a re-resolved
            // path — a swap of the source path after acquisition cannot change what runs.
            executable: self.backend.descriptor_path(),
            arguments,
            // PLANE SEPARATION: the backend is a TRUSTED launcher and must NOT inherit the
            // untrusted target's environment or cwd (LD_PRELOAD, LD_LIBRARY_PATH, attacker cwd,
            // …). It runs from a fixed trusted directory with a trusted control-plane
            // environment. The target's own cwd/env travel to the backend out-of-band (a launch
            // request), never through the backend's process environment or a /proc-visible argv.
            working_directory: PathBuf::from("/"),
            environment: self.backend_env.clone(),
            stdin: spec.stdin,
        })
    }

    /// Verify a report frame and turn it into trusted [`BoundaryEvidence::Reported`], or FAIL
    /// CLOSED. The report is trusted only if it echoes the expected BACKEND identity, and is
    /// bound to THIS policy (`policy_digest`), THIS launch (`launch_nonce`), and the exact
    /// `target` confined (`target_digest`), AND attests full enforcement on every dimension.
    ///
    /// # Errors
    /// [`SandboyLaunchError`] on a bad frame, a wrong backend, a binding mismatch, or a
    /// non-full report.
    pub fn verify_report(
        &self,
        frame_bytes: &[u8],
        launch_nonce: &LaunchNonce,
        target_digest: &Digest256,
    ) -> Result<BoundaryEvidence, SandboyLaunchError> {
        let report = frame::decode(frame_bytes)?;
        if report.backend != *self.backend.identity() {
            return Err(SandboyLaunchError::BackendMismatch);
        }
        if report.policy_digest != self.policy.digest() {
            return Err(SandboyLaunchError::PolicyDigestMismatch);
        }
        if report.launch_nonce != *launch_nonce {
            return Err(SandboyLaunchError::NonceMismatch);
        }
        if report.target_digest != *target_digest {
            return Err(SandboyLaunchError::TargetMismatch);
        }
        if !report.is_fully_enforced() {
            return Err(SandboyLaunchError::NotFullyEnforced);
        }
        Ok(BoundaryEvidence::Reported {
            attestation: BoundaryAttestation {
                implementation: BoundaryKind::Sandboy,
                enforcement: EnforcementLevel::FullyEnforced,
            },
            report: frame_bytes.to_vec(),
        })
    }
}

#[async_trait]
impl ProcessBoundary for SandboyBoundary {
    async fn spawn(&self, spec: BoundarySpawnSpec) -> Result<BoundaryLaunch, BoundaryError> {
        if !cfg!(target_os = "linux") {
            return Err(BoundaryError::UnsupportedPlatform);
        }
        // Mint the launch nonce from the OS CSPRNG BEFORE any backend spawn; an RNG failure
        // fails closed here — never a timestamp/counter fallback.
        let nonce = self
            .nonce_source
            .mint()
            .map_err(|e| BoundaryError::Spawn(io::Error::other(e.to_string())))?;
        // Fail closed BEFORE any launch if the target cannot execute under the policy. (Fixed
        // placeholder fds reuse the argv builder's permission gate; the live launch assigns the
        // real report/control descriptors.)
        let _backend_spec = self.backend_spawn_spec(&spec, -1, -1, &nonce)?;
        // RED: the confined live launch (verify the held backend object, open the report +
        // control pipes, spawn the monitor backend, read + verify the report, send GO, and wrap
        // it as a live BoundaryProcess with Reported evidence) is not implemented yet. Fail
        // closed rather than pretend to confine.
        Err(BoundaryError::Spawn(io::Error::other(
            "sandboy backend launch not yet implemented",
        )))
    }

    fn attestation(&self) -> BoundaryAttestation {
        BoundaryAttestation {
            implementation: BoundaryKind::Sandboy,
            enforcement: EnforcementLevel::FullyEnforced,
        }
    }
}
