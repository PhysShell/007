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

/// The run-time proof a boundary establishes AT launch — distinct from the pre-spawn
/// [`BoundaryAttestation`], which is only a configured claim.
///
/// A two-variant enum, not a struct with an `Option<report>`, so there is no way to express
/// the contradiction "fully enforced but with no report": a launch either established NO
/// confinement ([`BoundaryEvidence::Unconfined`], the host) or it established confinement and
/// MUST carry the raw report frame that proves it ([`BoundaryEvidence::Reported`]). There is
/// deliberately no constructor that pairs `FullyEnforced` with an absent report.
///
/// The supervisor publishes this before [`crate::WorkerObservation::Spawned`] and gates it
/// against the requirement via [`BoundaryEvidence::satisfies`]: `RequireFullyEnforced`
/// accepts ONLY a `Reported` evidence that is fully enforced AND whose implementation matches
/// the boundary's configured one — so missing evidence, a downgrade, or an implementation
/// swap all fail closed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BoundaryEvidence {
    /// No confinement was established (e.g. the host boundary). Never satisfies
    /// `RequireFullyEnforced`. It carries no attestation because the boundary's configured
    /// claim is already published as [`crate::WorkerObservation::BoundaryAttested`].
    Unconfined,
    /// Confinement was established, proven by the backend's raw report frame. The `report`
    /// bytes are opaque here — a downstream adapter stores them as a content-addressed
    /// artifact (o7-run's `SandboxEvidenceCaptured`) and re-derives their meaning.
    Reported {
        /// The attestation the report PROVES (re-derived from the verified report, not merely
        /// claimed).
        attestation: BoundaryAttestation,
        /// The verified raw report frame.
        report: Vec<u8>,
    },
}

impl BoundaryEvidence {
    /// The attestation this evidence actually establishes, or `None` for `Unconfined`.
    #[must_use]
    pub fn established(&self) -> Option<BoundaryAttestation> {
        match self {
            Self::Unconfined => None,
            Self::Reported { attestation, .. } => Some(*attestation),
        }
    }

    /// Whether this run-time evidence satisfies `requirement`, given the boundary's
    /// pre-spawn `configured` attestation.
    ///
    /// - `AllowUnconfined`: any evidence is acceptable.
    /// - `RequireFullyEnforced`: the evidence MUST be [`BoundaryEvidence::Reported`], fully
    ///   enforced, AND its implementation must match the configured one. `Unconfined`
    ///   (missing report), a downgrade, or an implementation swap all fail.
    #[must_use]
    pub fn satisfies(
        &self,
        requirement: BoundaryRequirement,
        configured: &BoundaryAttestation,
    ) -> bool {
        match requirement {
            BoundaryRequirement::AllowUnconfined => true,
            BoundaryRequirement::RequireFullyEnforced => match self {
                Self::Unconfined => false,
                Self::Reported { attestation, .. } => {
                    attestation.enforcement == EnforcementLevel::FullyEnforced
                        && attestation.implementation == configured.implementation
                }
            },
        }
    }
}

/// A successful launch: the owned process set PLUS the run-time [`BoundaryEvidence`] the
/// launch established. Returned by [`ProcessBoundary::spawn`] so evidence is available to
/// the supervisor and downstream BEFORE the process is trusted — a frozen interface that
/// could only return a `BoundaryProcess` could never carry the required proof.
pub struct BoundaryLaunch {
    pub process: Box<dyn BoundaryProcess>,
    pub evidence: BoundaryEvidence,
}

impl std::fmt::Debug for BoundaryLaunch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The process is an opaque trait object; the evidence is the interesting part.
        f.debug_struct("BoundaryLaunch")
            .field("evidence", &self.evidence)
            .field("process", &"<dyn BoundaryProcess>")
            .finish()
    }
}

/// Errors from a boundary mechanism.
#[derive(Debug, thiserror::Error)]
pub enum BoundaryError {
    #[error("spawn failed: {0}")]
    Spawn(#[source] std::io::Error),
    /// The TARGET executable could not be acquired under the hardened open: a symlink final
    /// component (`O_NOFOLLOW`), a FIFO/device/socket/directory, or another non-regular object.
    /// This is the intended fail-closed refusal of an unacceptable target — kept distinct from
    /// [`BoundaryError::Spawn`] so a genuine target refusal (the desired hardened behavior) cannot
    /// be confused with an infrastructure spawn failure (backend launch, control socket, RNG). A
    /// probe that treats "any error" as a target refusal would accept an infrastructure failure as
    /// proof of confinement; this variant lets the consumer demand exactly the refusal it tests.
    #[error("target acquisition failed closed: {0}")]
    TargetAcquisition(String),
    #[error("signal delivery failed: {0}")]
    Signal(String),
    #[error("waiting for the process failed: {0}")]
    Wait(#[source] std::io::Error),
    #[error("membership query failed: {0}")]
    Membership(String),
    /// A report/protocol failure at launch: a malformed frame, a mis-bound or downgraded
    /// report, a mismatched backend identity, or a backend that never delivered a report.
    /// Kept distinct from [`BoundaryError::Spawn`] so report failures do not dissolve into a
    /// generic I/O error.
    #[error("launch evidence/protocol failure: {0}")]
    Evidence(String),
    #[error("boundary is only supported on Linux")]
    UnsupportedPlatform,
}

/// Spawns processes inside a boundary it owns.
#[async_trait]
pub trait ProcessBoundary: Send + Sync {
    /// Launch the process. On success the returned [`BoundaryLaunch`] owns the entire
    /// process set AND carries the run-time [`BoundaryEvidence`] the launch established. A
    /// confining boundary MUST NOT return a live process it could not confine: if it cannot
    /// establish (and prove) the intended enforcement it fails closed here.
    ///
    /// CANCEL-SAFETY CONTRACT: the returned future MUST be cancel-safe. If it is
    /// dropped before completing, the boundary must not leak a process — any
    /// partially-created process (including a backend spawned to read a report from) must be
    /// terminated/cleaned up as the future drops (so a cancel racing with a spawn can never
    /// leave an ownerless process). The supervisor relies on this to cancel during
    /// `Starting`.
    async fn spawn(&self, spec: BoundarySpawnSpec) -> Result<BoundaryLaunch, BoundaryError>;

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
