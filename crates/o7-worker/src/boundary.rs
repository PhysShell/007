//! The process-boundary abstraction. A boundary OWNS a set of processes (a whole
//! process group / tree), not just the leader PID — so a later Sandboy
//! implementation can use cgroups/namespaces/etc. instead of a POSIX process
//! group without the generic supervisor ever knowing the difference.
//!
//! PR 2 ships only [`crate::host_boundary::UnconfinedHostBoundary`], attested as
//! [`EnforcementLevel::None`]. It provides lifecycle control, NOT security
//! isolation.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::PathBuf;

use async_trait::async_trait;
use tokio::process::{ChildStderr, ChildStdout};

use crate::process_identity::ProcessIdentity;
use crate::spec::StdinMode;

/// Which boundary implementation is in use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoundaryKind {
    /// Bare host process group — no confinement.
    UnconfinedHost,
    /// The Sandboy sandbox (not implemented in PR 2).
    Sandboy,
}

/// How much isolation the boundary actually enforces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnforcementLevel {
    None,
    Partial,
    FullyEnforced,
}

/// A boundary's honest self-description. Never inferred — asserted by the
/// implementation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BoundaryAttestation {
    pub implementation: BoundaryKind,
    pub enforcement: EnforcementLevel,
}

/// What a caller demands of the boundary. There is no default: a spec must state
/// it explicitly, and an unconfined boundary is usable ONLY under
/// [`BoundaryRequirement::AllowUnconfined`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoundaryRequirement {
    /// Permits an unconfined (host) boundary.
    AllowUnconfined,
    /// Requires `EnforcementLevel::FullyEnforced`; anything less fails closed
    /// BEFORE spawn. No silent fallback.
    RequireFullyEnforced,
}

impl BoundaryRequirement {
    /// Whether a boundary with the given attestation may be used under this
    /// requirement.
    #[must_use]
    pub fn is_satisfied_by(self, attestation: &BoundaryAttestation) -> bool {
        match self {
            Self::AllowUnconfined => true,
            Self::RequireFullyEnforced => {
                attestation.enforcement == EnforcementLevel::FullyEnforced
            }
        }
    }
}

/// How a boundary-owned process finished.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoundaryExit {
    Code(i32),
    Signal(i32),
}

/// The OS-level launch request handed to a boundary (the subset of a `WorkerSpec`
/// the boundary needs).
#[derive(Debug, Clone)]
pub struct BoundarySpawnSpec {
    pub executable: PathBuf,
    pub arguments: Vec<OsString>,
    pub working_directory: PathBuf,
    /// The COMPLETE child environment (the boundary clears the host env first).
    pub environment: BTreeMap<OsString, OsString>,
    pub stdin: StdinMode,
}

/// Errors from a boundary mechanism.
#[derive(Debug, thiserror::Error)]
pub enum BoundaryError {
    #[error("spawn failed: {0}")]
    Spawn(#[source] std::io::Error),
    #[error("signal delivery failed: {0}")]
    Signal(String),
    #[error("waiting for the process failed: {0}")]
    Wait(#[source] std::io::Error),
    #[error("boundary is only supported on Unix")]
    UnsupportedPlatform,
}

/// Spawns processes inside a boundary it owns.
#[async_trait]
pub trait ProcessBoundary: Send + Sync {
    /// Launch the process. On success the returned [`BoundaryProcess`] owns the
    /// entire process set.
    async fn spawn(
        &self,
        spec: BoundarySpawnSpec,
    ) -> Result<Box<dyn BoundaryProcess>, BoundaryError>;

    /// The boundary's honest attestation.
    fn attestation(&self) -> BoundaryAttestation;
}

/// A live, boundary-owned process set. The generic supervisor never needs to know
/// whether `force_stop` means `killpg`, a cgroup kill, or a Sandboy shutdown.
#[async_trait]
pub trait BoundaryProcess: Send {
    /// Identity of the leader process.
    fn identity(&self) -> ProcessIdentity;

    /// Take ownership of the child's stdout pipe (once).
    fn take_stdout(&mut self) -> Option<ChildStdout>;
    /// Take ownership of the child's stderr pipe (once).
    fn take_stderr(&mut self) -> Option<ChildStderr>;

    /// Ask the whole set to stop gracefully (e.g. SIGTERM to the group).
    async fn request_graceful_stop(&mut self) -> Result<(), BoundaryError>;

    /// Force the whole set to stop (e.g. SIGKILL to the group).
    async fn force_stop(&mut self) -> Result<(), BoundaryError>;

    /// Wait for the leader to exit.
    async fn wait(&mut self) -> Result<BoundaryExit, BoundaryError>;

    /// Which processes of the owned set are still alive.
    async fn remaining_members(&self) -> Result<Vec<ProcessIdentity>, BoundaryError>;
}
