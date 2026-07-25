//! `o7-run` — the canonical run-event protocol, the pure run-state reducer, and
//! independent replay.
//!
//! This crate turns a run's forensic archive into a *replayable execution model*. It is
//! the layer that binds the already-built pieces:
//!
//! ```text
//! o7-worker observations
//!         ↓  (canonical mapping — a later commit)
//! canonical RunEvent           [`event`]
//!         ↓  (append — o7-ledger, a later commit)
//! o7-ledger append
//!         ↓
//! pure RunState reducer         [`reduce`] over [`state`]
//!         ↓
//! replayable verdict            [`replay`]
//! ```
//!
//! Two invariants define the crate:
//!
//! 1. **A green verdict means every required obligation was actually discharged.** A
//!    required gate that never executed can never reduce to `Pass` — it is `Blocked`. This
//!    closes the false-green hole where an environment-specific required gate could be
//!    skipped yet the run still passed.
//! 2. **Run truth is independently reproducible.** Events are chained by digest and
//!    reference their artifacts by digest, so replay re-derives the verdict byte-for-byte
//!    and detects any post-run modification NOT accompanied by a consistent recomputation
//!    of every downstream digest — truncation, reordering, substitution, in-place edits —
//!    or fails loudly. This is tamper-EVIDENCE, not authenticity: an actor who can rewrite
//!    the whole stream and recompute the chain is out of scope (no remote attestation). The
//!    replay report exposes the final event digest and normalized-state digest so a stream
//!    can be anchored externally against that. A pre-protocol run stays readable but is
//!    explicitly `LegacyNonReplayable`, never silently "verified".
//!
//! The core is PURE and dependency-light on purpose (serde + a hash): the semantics can be
//! recomputed anywhere, by anyone, without the ledger, a runtime, or a database.
//!
//! ## Status: contract-only (RED)
//!
//! This first commit fixes the versioned contracts ([`event`], [`state`]) and the
//! reducer/replay SURFACES, with the required behavior specified exhaustively by the
//! `tests/` transition table and replay-acceptance fixtures. The transition and
//! verification LOGIC is intentionally not implemented here — [`reduce::reduce`] and
//! [`replay::replay`] return an explicit `Unimplemented` error rather than a plausible
//! wrong answer, so the acceptance tests are RED by construction and a later commit turns
//! them GREEN. Nothing here can be mistaken for a working verdict.

pub mod event;
pub mod ids;
pub mod reduce;
pub mod replay;
pub mod state;

pub use event::{
    AgentObligation, AgentOutcome, ArtifactKind, ArtifactRef, Digest256, DigestFormatError,
    ExecutionSubject, GateApplicability, GateObligation, GateOutcome, GateRequirement,
    PolicyObligation, PolicyOutcome, PolicyRequirement, RunContract, RunEvent, RunEventKind,
    SandboxEvidenceOutcome, SandboxRequirement, RUN_EVENT_SCHEMA_VERSION,
};
pub use ids::{GateId, IdError, RunEventId, RunId};
pub use reduce::{reduce, reduce_all, ReduceError};
pub use replay::{
    classify, replay, replay_verify, ArtifactError, ArtifactResolver, ReplayError, ReplayReport,
    Replayability,
};
pub use state::{
    AgentLifecycle, GateProgress, PolicyResult, RunPhase, RunState, SandboxEvidenceEntry,
    SandboxEvidenceKey, Verdict, RUN_STATE_SCHEMA_VERSION,
};
