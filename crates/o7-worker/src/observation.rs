//! The worker's **internal lifecycle observation model** and the sink it is
//! published to.
//!
//! IMPORTANT: this is NOT the canonical 007 event protocol and NOT a stable
//! persistence schema — PR 1 froze the ledger's event set and PR 4 owns the
//! canonical event protocol. PR 2 must not write these into the ledger or use
//! final event names. In PR 4 an adapter maps `WorkerObservation` →
//! canonical 007 event → append-only ledger.
//!
//! The `ObservationSink` is authoritative: losing it is a FATAL supervisor error
//! that stops the worker (a UI disconnect is unrelated — the UI is not a sink).

use std::time::Duration;

use async_trait::async_trait;

use crate::boundary::{BoundaryAttestation, BoundaryExit};
use crate::output::OutputChunk;
use crate::process_identity::ProcessIdentity;
use crate::spec::WorkerId;

/// A single observation of the worker lifecycle.
#[derive(Debug, Clone)]
pub enum WorkerObservation {
    /// The boundary's attestation, emitted before anything is spawned.
    BoundaryAttested(BoundaryAttestation),
    /// About to ask the boundary to spawn.
    SpawnRequested,
    /// The leader process was spawned.
    Spawned(ProcessIdentity),
    /// A chunk of stdout/stderr.
    OutputChunk(OutputChunk),
    /// A supervisor-liveness heartbeat (independent of output).
    Heartbeat {
        worker_id: WorkerId,
        sequence: u64,
        uptime: Duration,
        identity: ProcessIdentity,
    },
    /// Cancellation was requested.
    CancellationRequested,
    /// A graceful stop (SIGTERM to the group) was sent.
    GracefulStopSent,
    /// A forceful stop (SIGKILL to the group) was sent.
    ForceStopSent,
    /// After a stop, these owned processes were still alive.
    DescendantsRemaining(Vec<ProcessIdentity>),
    /// The leader exited.
    Exited(BoundaryExit),
    /// The owned process set is confirmed gone.
    CleanupCompleted,
    /// The supervisor itself failed (e.g. the sink was lost, cleanup could not be
    /// proven). Terminal.
    SupervisorFailed(String),
}

/// A publish failure. Treated by the supervisor as fatal.
#[derive(Debug, thiserror::Error)]
#[error("observation sink failure: {0}")]
pub struct ObservationError(pub String);

/// The authoritative destination for [`WorkerObservation`]s.
#[async_trait]
pub trait ObservationSink: Send + Sync {
    /// Publish one observation. Returning `Err` is fatal: the supervisor cancels
    /// the worker and cleans up.
    async fn publish(&self, observation: WorkerObservation) -> Result<(), ObservationError>;
}
