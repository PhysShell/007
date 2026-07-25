//! The Sandboy boundary: a [`ProcessBoundary`] that confines the child with real Linux
//! isolation (Landlock + seccomp) and attests [`EnforcementLevel::FullyEnforced`].
//!
//! The confinement runs in an EXTERNAL `sandboy` backend binary — every 007 crate forbids
//! `unsafe`, and installing Landlock/seccomp on a child requires an `unsafe` `pre_exec` hook
//! we will not add (see `docs/architecture/sandboy-boundary.md`). So this boundary launches
//! the backend through the evolved spawn seam. The backend, in a MONITOR topology, installs
//! the confinement, forks a confined child that `exec`s the real target in place (same PID
//! namespace, so PR 3's sealed-memfd exec via `/proc/<verifier_pid>/fd/<n>` still resolves),
//! and remains as the cgroup-owning monitor that enforces the wall-clock timeout and the
//! process-tree kill. It emits a single [`o7_sandbox_protocol::SandboxReport`] frame on a
//! dedicated inherited descriptor, BOUND to the policy, launch nonce, and target.
//!
//! What is LIVE here (pure, GREEN):
//! - policy validation and an ABSOLUTE backend path (no PATH search);
//! - the pre-spawn exec-permission gate (PR-3 sealed memfd);
//! - the confined-backend argv (`--report-fd`, `--launch-nonce`, policy flags, `-- <target>`);
//! - [`SandboyBoundary::verify_report`]: decode the report frame and FAIL CLOSED unless it is
//!   bound to this policy/launch/target AND attests full enforcement across every dimension.
//!
//! What is RED (the live launch): wiring the report pipe, spawning + reaping the backend,
//! reading the frame, and returning a live [`BoundaryProcess`]. [`SandboyBoundary::spawn`]
//! fails closed until it lands.

use std::os::unix::io::RawFd;
use std::path::PathBuf;
use std::{ffi::OsString, io};

use async_trait::async_trait;

use o7_sandbox_protocol::frame::{self, FrameError};
use o7_sandbox_protocol::ids::{Digest256, LaunchNonce};
use o7_sandbox_protocol::policy::{NetworkPolicy, SandboxPolicy, SandboxPolicyError};

use crate::boundary::{
    BoundaryAttestation, BoundaryError, BoundaryEvidence, BoundaryKind, BoundaryLaunch,
    BoundarySpawnSpec, EnforcementLevel, ProcessBoundary,
};

/// A boundary that confines the child through an external `sandboy` backend and attests full
/// enforcement. Constructed only with a validated full-confinement policy and an ABSOLUTE
/// backend path, so a constructed instance always intends `FullyEnforced` — there is no
/// weaker mode and no silent fallback.
#[derive(Debug, Clone)]
pub struct SandboyBoundary {
    /// The `sandboy` backend executable — absolute, never a PATH lookup.
    backend: PathBuf,
    /// The confinement to install.
    policy: SandboxPolicy,
}

/// Why a Sandboy launch cannot be trusted — at construction OR when verifying a report.
#[derive(Debug, thiserror::Error)]
pub enum SandboyLaunchError {
    /// The confinement policy is not capable of full enforcement.
    #[error("sandbox policy is not fully confining: {0}")]
    Policy(#[from] SandboxPolicyError),
    /// The backend path is not absolute — a PATH search must never resolve the confiner.
    #[error("sandboy backend must be an absolute path: {0}")]
    RelativeBackend(PathBuf),
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
    /// Build a Sandboy boundary from an ABSOLUTE backend binary and a confinement policy.
    ///
    /// # Errors
    /// [`SandboyLaunchError::RelativeBackend`] if the backend path is not absolute;
    /// [`SandboyLaunchError::Policy`] if the policy cannot fully confine. Failing here is the
    /// first fail-closed gate: an untrustworthy backend/policy never becomes a boundary.
    pub fn new(backend: PathBuf, policy: SandboxPolicy) -> Result<Self, SandboyLaunchError> {
        if !backend.is_absolute() {
            return Err(SandboyLaunchError::RelativeBackend(backend));
        }
        policy.validate()?;
        Ok(Self { backend, policy })
    }

    /// The policy this boundary installs.
    #[must_use]
    pub fn policy(&self) -> &SandboxPolicy {
        &self.policy
    }

    /// Construct the confined backend invocation for a target `spec`, or fail closed.
    ///
    /// The backend is launched with the confinement flags plus the report descriptor and the
    /// launch nonce; the real target follows `--`. The target's `executable` MUST be
    /// permitted to execute by the policy — in particular PR 3's `/proc/<verifier_pid>/fd/<n>`
    /// sealed memfd — otherwise this refuses rather than launching a doomed backend.
    ///
    /// # Errors
    /// [`BoundaryError::Spawn`] if the target executable is not permitted by the policy.
    pub fn backend_spawn_spec(
        &self,
        spec: &BoundarySpawnSpec,
        report_fd: RawFd,
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
            // The dedicated report descriptor (not stdout/stderr) the backend writes ONE
            // versioned, length-bounded frame to and then closes.
            OsString::from("--report-fd"),
            OsString::from(report_fd.to_string()),
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
            executable: self.backend.clone(),
            arguments,
            working_directory: spec.working_directory.clone(),
            environment: spec.environment.clone(),
            stdin: spec.stdin,
        })
    }

    /// Verify a report frame and turn it into trusted [`BoundaryEvidence`], or FAIL CLOSED.
    ///
    /// The report is trusted only if it is bound to THIS policy (`policy_digest`), THIS launch
    /// (`launch_nonce`), and the exact `target` that was confined (`target_digest`), AND it
    /// attests full enforcement on every dimension. A stale, forged, or downgraded report
    /// yields an error — never a live, "trusted" process. The raw frame bytes are carried on
    /// the evidence for a downstream adapter to persist as a content-addressed artifact.
    ///
    /// # Errors
    /// [`SandboyLaunchError`] on a bad frame, a binding mismatch, or a non-full report.
    pub fn verify_report(
        &self,
        frame_bytes: &[u8],
        launch_nonce: &LaunchNonce,
        target_digest: &Digest256,
    ) -> Result<BoundaryEvidence, SandboyLaunchError> {
        let report = frame::decode(frame_bytes)?;
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
        Ok(BoundaryEvidence {
            attestation: BoundaryAttestation {
                implementation: BoundaryKind::Sandboy,
                enforcement: EnforcementLevel::FullyEnforced,
            },
            report: Some(frame_bytes.to_vec()),
        })
    }
}

#[async_trait]
impl ProcessBoundary for SandboyBoundary {
    async fn spawn(&self, spec: BoundarySpawnSpec) -> Result<BoundaryLaunch, BoundaryError> {
        if !cfg!(target_os = "linux") {
            return Err(BoundaryError::UnsupportedPlatform);
        }
        // Fail closed BEFORE any launch if the target cannot execute under the policy. (The
        // real launch mints a nonce and a report pipe; a fixed placeholder fd is used only to
        // reuse the argv builder's permission gate here.)
        let probe_nonce = LaunchNonce::from_bytes([0u8; 16]);
        let _backend_spec = self.backend_spawn_spec(&spec, -1, &probe_nonce)?;
        // RED: the confined live launch (mint a nonce, open the report pipe, spawn + reap the
        // backend, read and verify the frame, wrap it as a live BoundaryProcess with the
        // established evidence) is not implemented yet. Fail closed rather than pretend to
        // confine.
        Err(BoundaryError::Spawn(io::Error::other(
            "sandboy backend launch not yet implemented",
        )))
    }

    fn attestation(&self) -> BoundaryAttestation {
        // A constructed boundary was built from a validated full-confinement policy and an
        // absolute backend, so it INTENDS full enforcement. The run-time report (verified by
        // `verify_report`) must independently re-prove this; a downgraded report fails closed.
        BoundaryAttestation {
            implementation: BoundaryKind::Sandboy,
            enforcement: EnforcementLevel::FullyEnforced,
        }
    }
}
