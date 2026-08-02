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
//! ## Status
//!
//! The versioned contracts ([`event`], [`state`]), the pure reducer ([`reduce::reduce`] /
//! [`reduce::reduce_all`]), and independent replay ([`replay::replay`] /
//! [`replay::replay_verify`]) are all implemented, with behavior specified exhaustively by
//! the `tests/` transition table and replay-acceptance fixtures (all green). The contract
//! was frozen through a RED→GREEN sequence before these were implemented, so the code holds
//! the contract rather than the reverse. Not yet wired here (later PRs): the
//! `WorkerObservation → RunEvent` adapter, the `o7-ledger` append path, and the `o7 replay`
//! CLI.

pub mod candidate;
pub mod event;
pub mod ids;
pub mod reduce;
pub mod replay;
pub mod state;

pub use candidate::{
    CandidateMaterializationCheck, CandidateStateReceiptV1, CandidateVerifyError,
    CANDIDATE_STATE_RECEIPT_SCHEMA_V1,
};
pub use event::{
    AgentObligation, AgentOutcome, ArtifactKind, ArtifactRef, CandidatePatchKind,
    CandidateStateContractV1, Digest256, DigestFormatError, ExecutionSubject, GateApplicability,
    GateObligation, GateOutcome, GateRequirement, PolicyObligation, PolicyOutcome,
    PolicyProtectedSubject, PolicyRequirement, RepositoryIdentity, RunContract, RunEvent,
    RunEventKind, SandboxEvidenceOutcome, SandboxRequirement, CANDIDATE_STATE_CONTRACT_SCHEMA_V1,
    RUN_EVENT_SCHEMA_VERSION,
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
