//! The process-boundary abstraction. A boundary OWNS a set of processes (the
//! members of a host process GROUP — not just the leader PID, but also not a
//! whole tree/cgroup: a descendant that leaves the group by starting its own
//! group/session is no longer owned) — so a later Sandboy
//! implementation can use cgroups/namespaces/etc. instead of a POSIX process
//! group without the generic supervisor ever knowing the difference.
//!
//! PR 2 ships only [`crate::host_boundary::UnconfinedHostBoundary`], attested as
//! [`EnforcementLevel::None`]. It provides lifecycle control, NOT security
//! isolation.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::PathBuf;
use std::pin::Pin;

use async_trait::async_trait;
use tokio::io::AsyncRead;

use crate::process_identity::ProcessIdentity;
use crate::spec::StdinMode;

/// An owned output stream (stdout or stderr) of a boundary process. Boxed +
/// pinned so a boundary can back it with a real child pipe or, in tests, an
/// injected reader.
pub type BoundaryStream = Pin<Box<dyn AsyncRead + Send>>;

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
    #[error("membership query failed: {0}")]
    Membership(String),
    #[error("boundary is only supported on Linux")]
    UnsupportedPlatform,
}

/// Spawns processes inside a boundary it owns.
#[async_trait]
pub trait ProcessBoundary: Send + Sync {
    /// Launch the process. On success the returned [`BoundaryProcess`] owns the
    /// entire process set.
    ///
    /// CANCEL-SAFETY CONTRACT: the returned future MUST be cancel-safe. If it is
    /// dropped before completing, the boundary must not leak a process — any
    /// partially-created process must be terminated/cleaned up as the future
    /// drops (so a cancel racing with a spawn can never leave an ownerless
    /// process). The supervisor relies on this to cancel during `Starting`.
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

    /// Take ownership of the stdout stream (once).
    fn take_stdout(&mut self) -> Option<BoundaryStream>;
    /// Take ownership of the stderr stream (once).
    fn take_stderr(&mut self) -> Option<BoundaryStream>;

    /// Ask the whole set to stop gracefully (e.g. SIGTERM to the group).
    ///
    /// CANCEL-SAFETY CONTRACT: the supervisor BOUNDS this call with a timeout and drops
    /// the future if it does not complete in time (a hung boundary must not stall
    /// cancellation). Dropping a partially-run `request_graceful_stop` must not corrupt
    /// the boundary or leak a signal — after a drop the set is left in a well-defined
    /// state and a subsequent `force_stop()` remains valid. A graceful-stop timeout is
    /// treated as "graceful did not take" and escalates immediately to a forceful stop.
    async fn request_graceful_stop(&mut self) -> Result<(), BoundaryError>;

    /// Force the whole set to stop (e.g. SIGKILL to the group).
    ///
    /// CANCEL-SAFETY CONTRACT: the supervisor BOUNDS this call with a timeout and drops
    /// the future if it does not complete in time. Dropping a partially-run `force_stop`
    /// must not corrupt the boundary; a force-stop that times out is treated as an
    /// UNPROVABLE teardown (the set may still be alive), yielding a bounded
    /// `CleanupFailure` rather than a hang.
    async fn force_stop(&mut self) -> Result<(), BoundaryError>;

    /// Wait for the leader to exit.
    ///
    /// CANCEL-SAFETY CONTRACT: the returned future MUST be cancel-safe. The supervisor
    /// polls `wait()` inside a `tokio::select!` and DROPS the pending future whenever
    /// another branch (output, heartbeat, cancellation) wins, then calls `wait()` again
    /// on the next iteration. Dropping a pending `wait()` therefore must lose no
    /// progress: a leader exit that became ready while the future was not being polled
    /// must still be observed by a subsequent `wait()` call, and the exit status must
    /// never be consumed-and-lost by a dropped future. Concretely, model the exit on an
    /// ABSOLUTE deadline / a re-checkable readiness flag, not a relative timer restarted
    /// on each poll. Tokio's `Child::wait` satisfies this; a custom boundary (e.g.
    /// Sandboy) must uphold it too. After a `force_stop()`, callers additionally BOUND
    /// each reap, so a `wait()` that never completes cannot hang the supervisor — but a
    /// bounded-out reap is reported as an unprovable teardown, so a compliant boundary
    /// should still complete `wait()` promptly once the leader is gone.
    async fn wait(&mut self) -> Result<BoundaryExit, BoundaryError>;

    /// Which processes of the owned set are still alive.
    ///
    /// CANCEL-SAFETY CONTRACT: the supervisor BOUNDS this query with a timeout and drops
    /// the future if it does not complete in time. Dropping a partially-run
    /// `remaining_members` must not corrupt the boundary and a subsequent call must
    /// still return an accurate set. A membership query that times out is treated as an
    /// UNPROVABLE teardown (the set's state is unknown), yielding a bounded
    /// `CleanupFailure` — never "unknown means empty".
    async fn remaining_members(&self) -> Result<Vec<ProcessIdentity>, BoundaryError>;
}
