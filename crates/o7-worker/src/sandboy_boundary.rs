//! The Sandboy boundary: a [`ProcessBoundary`] that confines the child with real Linux
//! isolation (Landlock + seccomp) and attests [`EnforcementLevel::FullyEnforced`].
//!
//! The confinement itself runs in an EXTERNAL `sandboy` backend binary — every 007 crate
//! forbids `unsafe`, and installing Landlock/seccomp on a child requires an `unsafe`
//! `pre_exec` hook we will not add (see `docs/architecture/sandboy-boundary.md`). So this
//! boundary launches the backend through the UNCHANGED frozen PATH-spawn seam: the
//! backend self-confines, emits a machine-readable [`SandboxReport`], then `exec`s the
//! real target IN PLACE (same PID namespace) so PR 3's sealed-memfd exec via
//! `/proc/<verifier_pid>/fd/<n>` still resolves.
//!
//! This module's PURE, fail-closed parts are live: policy validation, the honest
//! attestation, backend argv construction, and the pre-spawn exec-permission gate. The
//! confined LIVE LAUNCH (wiring the report channel, reading the report, and mapping it
//! onto a live [`BoundaryProcess`]) is not implemented yet — [`SandboyBoundary::spawn`]
//! fails closed until it lands.

use std::ffi::OsString;
use std::io;
use std::path::PathBuf;

use async_trait::async_trait;

use crate::boundary::{
    BoundaryAttestation, BoundaryError, BoundaryKind, BoundaryProcess, BoundarySpawnSpec,
    EnforcementLevel, ProcessBoundary,
};
use crate::sandbox::{NetworkPolicy, SandboxPolicy, SandboxPolicyError};
use crate::spec::StdinMode;

/// A boundary that confines the child through an external `sandboy` backend and attests
/// full enforcement. Constructed only with a validated, full-confinement policy, so a
/// constructed instance always intends `FullyEnforced` — there is no weaker mode and no
/// silent fallback.
#[derive(Debug, Clone)]
pub struct SandboyBoundary {
    /// The `sandboy` backend executable, launched via the frozen spawn seam.
    backend: PathBuf,
    /// The confinement to install.
    policy: SandboxPolicy,
}

impl SandboyBoundary {
    /// Build a Sandboy boundary from a backend binary and a confinement policy.
    ///
    /// # Errors
    /// [`SandboxPolicyError`] if the policy cannot be trusted to fully confine (a relative
    /// root/allowance, no exec allowance, or a zero timeout). Failing here is the first
    /// fail-closed gate: an untrustworthy policy never becomes a `FullyEnforced` boundary.
    pub fn new(backend: PathBuf, policy: SandboxPolicy) -> Result<Self, SandboxPolicyError> {
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
    /// The returned spec launches the `sandboy` backend with the confinement flags, then
    /// the original target after `--`. The target's `executable` MUST be permitted to
    /// execute by the policy — in particular PR 3's `/proc/<verifier_pid>/fd/<n>` sealed
    /// memfd — otherwise this refuses rather than launching a backend doomed to a denied
    /// `exec`.
    ///
    /// # Errors
    /// [`BoundaryError::Spawn`] if the target executable is not permitted by the policy.
    pub fn backend_spawn_spec(
        &self,
        spec: &BoundarySpawnSpec,
    ) -> Result<BoundarySpawnSpec, BoundaryError> {
        if !self.policy.permits_exec(&spec.executable) {
            return Err(BoundaryError::Spawn(io::Error::other(format!(
                "sandbox policy does not permit executing {}; refusing to launch",
                spec.executable.display()
            ))));
        }

        let mut arguments: Vec<OsString> = vec![OsString::from("run"), OsString::from("--report")];
        match self.policy.network {
            NetworkPolicy::DenyAll => arguments.push(OsString::from("--deny-net")),
        }
        arguments.push(OsString::from("--worktree"));
        arguments.push(self.policy.worktree.clone().into_os_string());
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
        // The real target and its arguments follow the separator.
        arguments.push(OsString::from("--"));
        arguments.push(spec.executable.clone().into_os_string());
        arguments.extend(spec.arguments.iter().cloned());

        Ok(BoundarySpawnSpec {
            executable: self.backend.clone(),
            arguments,
            working_directory: spec.working_directory.clone(),
            environment: spec.environment.clone(),
            stdin: match spec.stdin {
                StdinMode::Null => StdinMode::Null,
            },
        })
    }
}

#[async_trait]
impl ProcessBoundary for SandboyBoundary {
    async fn spawn(
        &self,
        spec: BoundarySpawnSpec,
    ) -> Result<Box<dyn BoundaryProcess>, BoundaryError> {
        if !cfg!(target_os = "linux") {
            return Err(BoundaryError::UnsupportedPlatform);
        }
        // Fail closed BEFORE any launch if the target cannot execute under the policy.
        let _backend_spec = self.backend_spawn_spec(&spec)?;
        // RED: the confined live launch (spawn the backend, wire the report channel, read
        // and re-prove the report, wrap it in a live BoundaryProcess) is not implemented
        // yet. Fail closed rather than pretend to confine.
        Err(BoundaryError::Spawn(io::Error::other(
            "sandboy backend launch not yet implemented",
        )))
    }

    fn attestation(&self) -> BoundaryAttestation {
        // A constructed boundary was built from a validated full-confinement policy, so it
        // INTENDS full enforcement. The run-time SandboxReport must independently re-prove
        // this; a downgraded report fails the run closed.
        BoundaryAttestation {
            implementation: BoundaryKind::Sandboy,
            enforcement: EnforcementLevel::FullyEnforced,
        }
    }
}
