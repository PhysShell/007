//! The Sandboy boundary: a [`ProcessBoundary`] that confines the child with real Linux
//! isolation (Landlock + seccomp) and attests [`EnforcementLevel::FullyEnforced`].
//!
//! The confinement runs in an EXTERNAL `sandboy` backend binary (every 007 crate forbids
//! `unsafe`; see `docs/architecture/sandboy-boundary.md`). This boundary launches the backend
//! through the spawn seam. The backend, in a MONITOR topology, installs the confinement, forks
//! a confined child that `exec`s the real target IN PLACE (same PID namespace, so PR 3's
//! sealed-memfd exec via `/proc/<verifier_pid>/fd/<n>` still resolves), and remains as the
//! cgroup-owning monitor enforcing the deadline + process-tree kill.
//!
//! Trust is bound to OBJECTS, not paths:
//! - the BACKEND is a [`BackendImage`] — a HELD immutable descriptor + its digest + the
//!   expected [`BackendIdentity`]. The launch execs the held descriptor (not a re-resolved
//!   path, so a post-construction swap cannot substitute a different binary), and the report
//!   must echo the expected identity;
//! - the TARGET's digest is derived from the exact held bytes the backend confined, and the
//!   report must echo it.
//!
//! Authorization is a BARRIER, not an afterthought: the two-phase handshake (`--report-fd` for
//! the report, `--control-fd` for the parent GO) means the backend holds the confined target
//! BEFORE `exec` until the parent has verified the report and sent GO. A malformed / mis-bound
//! / downgraded report, a NACK, EOF, or a cancel makes the backend kill its cgroup and reap —
//! the target never runs on an unverified report.
//!
//! LIVE + GREEN here: policy/backend validation, nonce minting (fail-closed on RNG failure),
//! the confined-backend argv, the exec-permission gate, and the pure fail-closed verifiers
//! ([`SandboyBoundary::verify_backend_object`], [`SandboyBoundary::verify_report`]). RED: the
//! confined live launch itself.

use std::os::unix::io::RawFd;
use std::path::PathBuf;
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

/// A backend bound to a specific immutable OBJECT, not just a path. `descriptor` is a HELD,
/// read-only descriptor (e.g. `/proc/<verifier_pid>/fd/<n>` of a sealed staging object) that
/// the boundary execs directly, so a substitution of the on-disk path after construction
/// cannot change what runs; `digest` is the digest of that object's bytes (re-checked before
/// launch); `identity` is the backend the report must echo. Producing the sealed/held object
/// is the caller's acquisition step (PR 3's mechanism) — this type only carries the proof.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackendImage {
    pub descriptor: PathBuf,
    pub digest: Digest256,
    pub identity: BackendIdentity,
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
    /// The backend descriptor is not an absolute (held) path.
    #[error("sandboy backend descriptor must be an absolute held path: {0}")]
    RelativeBackend(PathBuf),
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
    /// Build a Sandboy boundary from an acquisition-bound backend and a confinement policy,
    /// using the OS CSPRNG for launch nonces.
    ///
    /// # Errors
    /// [`SandboyLaunchError::RelativeBackend`] if the backend descriptor is not absolute;
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
        if !backend.descriptor.is_absolute() {
            return Err(SandboyLaunchError::RelativeBackend(backend.descriptor));
        }
        policy.validate()?;
        Ok(Self {
            backend,
            policy,
            nonce_source,
        })
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

    /// Verify the held backend object's bytes against the trusted image digest, or fail closed.
    ///
    /// # Errors
    /// [`SandboyLaunchError::BackendObjectMismatch`] on any mismatch.
    pub fn verify_backend_object(&self, bytes: &[u8]) -> Result<(), SandboyLaunchError> {
        if Digest256::of_bytes(bytes) == self.backend.digest {
            Ok(())
        } else {
            Err(SandboyLaunchError::BackendObjectMismatch)
        }
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
        arguments.push(OsString::from("--"));
        arguments.push(spec.executable.clone().into_os_string());
        arguments.extend(spec.arguments.iter().cloned());

        Ok(BoundarySpawnSpec {
            // Exec the HELD backend object, not a re-resolved path.
            executable: self.backend.descriptor.clone(),
            arguments,
            working_directory: spec.working_directory.clone(),
            environment: spec.environment.clone(),
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
        if report.backend != self.backend.identity {
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
