//! `o7-worker` — a generic worker runtime for 007.
//!
//! It launches ONE external process through a [`ProcessBoundary`], owns its whole
//! process GROUP (the members that stay in the host group — not a tree/cgroup; a
//! descendant that starts its own group/session escapes it), streams typed
//! [`WorkerObservation`]s to an [`ObservationSink`],
//! cancels idempotently, tears the process group down deterministically, and
//! yields exactly one [`WorkerResult`]. It knows NOTHING about Claude, Codex, MCP,
//! worktrees, verifiers, or the ledger — those live in other crates/PRs.
//!
//! PR 2 ships only the [`UnconfinedHostBoundary`], attested as
//! [`EnforcementLevel::None`]: it provides lifecycle control, NOT isolation. A
//! real sandbox (Sandboy) is a separate, mandatory boundary implementation
//! required before any live provider run. Unix-only by construction.
//!
//! See `docs/architecture/worker-lifecycle.md` and
//! `docs/architecture/process-boundary.md`.

pub mod boundary;
pub mod cancellation;
pub mod heartbeat;
pub mod host_boundary;
pub mod observation;
pub mod output;
pub mod process_identity;
pub mod spec;
pub mod state;
pub mod supervisor;

pub use boundary::{
    BoundaryAttestation, BoundaryError, BoundaryExit, BoundaryKind, BoundaryProcess,
    BoundaryRequirement, BoundarySpawnSpec, BoundaryStream, EnforcementLevel, ProcessBoundary,
};
pub use cancellation::CancellationPolicy;
pub use heartbeat::HeartbeatPolicy;
pub use host_boundary::UnconfinedHostBoundary;
pub use observation::{ObservationError, ObservationSink, WorkerObservation};
pub use output::{OutputChunk, OutputPolicy, OutputStream};
pub use process_identity::ProcessIdentity;
pub use spec::{EnvironmentPolicy, SpecError, StdinMode, WorkerId, WorkerSpec};
pub use state::WorkerState;
pub use supervisor::{WorkerHandle, WorkerJoin, WorkerResult, WorkerSupervisor};
