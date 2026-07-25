//! The pure run-state reducer: `reduce(state, event) -> Result<state, ReduceError>`.
//!
//! CONTRACT-ONLY (RED). This commit fixes the reducer's SIGNATURE and its required behavior
//! (encoded exhaustively in `tests/reducer_transitions.rs`) but does NOT yet implement the
//! transition logic. Until the following commit implements it, [`reduce`] returns
//! [`ReduceError::Unimplemented`] so that no caller can ever mistake an unimplemented
//! reducer for a real verdict — the tests are RED by construction, never silently green.
//!
//! The reducer is PURE: same input state + event ⇒ same output, no I/O, no clock, no
//! panics. Artifact-CONTENT validation is not the reducer's job (it folds only the event
//! payloads and checks each artifact's declared KIND); resolving and hashing artifact bytes
//! belongs to [`crate::replay`].

use crate::event::{ArtifactKind, Digest256, ExecutionSubject, PolicyProtectedSubject, RunEvent};
use crate::ids::{GateId, RunEventId, RunId};
use crate::state::RunState;

/// A structural violation of the event STREAM (as opposed to a domain verdict). These make
/// reduction fail loudly — they are never folded into a `Verdict`.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ReduceError {
    /// The first folded event was not `RunStarted`.
    #[error("stream does not begin with RunStarted (found {found} at sequence {sequence})")]
    MissingRunStarted { found: &'static str, sequence: u64 },

    /// A second `RunStarted`.
    #[error("duplicate RunStarted at sequence {sequence}")]
    DuplicateStart { sequence: u64 },

    /// `sequence` did not increase by exactly one from the previous event.
    #[error("out-of-order event: expected sequence {expected}, found {found}")]
    OutOfOrder { expected: u64, found: u64 },

    /// The same sequence number appeared twice.
    #[error("duplicate sequence {sequence}")]
    DuplicateSequence { sequence: u64 },

    /// `previous_event_digest` did not equal the prior event's `event_digest` (or genesis
    /// for the first event) — the chain is broken.
    #[error("broken chain at sequence {sequence}: previous-digest link does not match")]
    BrokenChain { sequence: u64 },

    /// The event's stored `event_digest` does not match a recomputation of its own fields.
    #[error("inconsistent event digest at sequence {sequence}")]
    InconsistentDigest { sequence: u64 },

    /// An event carries a different `run_id` than the run's first event.
    #[error("run id mismatch at sequence {sequence}: expected {expected}, found {found}")]
    RunIdMismatch {
        sequence: u64,
        expected: RunId,
        found: RunId,
    },

    /// The same `event_id` appeared more than once.
    #[error("duplicate event id {event_id} at sequence {sequence}")]
    DuplicateEventId { sequence: u64, event_id: RunEventId },

    /// The event's schema version is not the one this build understands (too old, unknown,
    /// or too new) — refuse rather than mis-reduce.
    #[error("unsupported event schema v{found} at sequence {sequence} (this build supports v{supported})")]
    UnsupportedSchema {
        sequence: u64,
        found: u32,
        supported: u32,
    },

    /// The `RunStarted` contract declared the same gate id in more than one obligation.
    #[error("duplicate gate {gate} in the run contract")]
    DuplicateGateInContract { gate: GateId },

    /// A gate event referenced a gate that was never declared as an obligation.
    #[error("unknown gate {gate} at sequence {sequence}: not a declared obligation")]
    UnknownGate { sequence: u64, gate: GateId },

    /// A second `GateStarted` for a gate already started.
    #[error("duplicate GateStarted for {gate} at sequence {sequence}")]
    DuplicateGateStarted { sequence: u64, gate: GateId },

    /// `GateFinished` for a gate that was never `GateStarted`.
    #[error("GateFinished without GateStarted for {gate} at sequence {sequence}")]
    GateFinishedWithoutStart { sequence: u64, gate: GateId },

    /// A second `GateFinished` for a gate already finished.
    #[error("duplicate GateFinished for {gate} at sequence {sequence}")]
    DuplicateGateFinished { sequence: u64, gate: GateId },

    /// `AgentExited` before any `AgentStarted` (except the `FailedToStart` outcome, which
    /// legitimately needs no prior start).
    #[error("AgentExited before AgentStarted at sequence {sequence}")]
    AgentExitBeforeStart { sequence: u64 },

    /// A second `AgentStarted`.
    #[error("duplicate AgentStarted at sequence {sequence}")]
    DuplicateAgentStart { sequence: u64 },

    /// A second `AgentExited`.
    #[error("duplicate AgentExited at sequence {sequence}")]
    DuplicateAgentExit { sequence: u64 },

    /// An artifact slot carried the wrong `ArtifactKind` (e.g. a patch slot with a `task`
    /// artifact).
    #[error("wrong artifact kind at sequence {sequence}: expected {expected:?}, found {found:?}")]
    WrongArtifactKind {
        sequence: u64,
        expected: ArtifactKind,
        found: ArtifactKind,
    },

    /// An artifact reference carried an empty locator.
    #[error("empty artifact locator at sequence {sequence}")]
    EmptyArtifactLocator { sequence: u64 },

    /// A gate is waived for a DIFFERENT environment than the run's — a self-contradictory
    /// pre-run contract.
    #[error("gate {gate} waived for {waiver_environment:?} but the run environment is {runner_environment:?}")]
    WaiverEnvironmentMismatch {
        gate: GateId,
        waiver_environment: String,
        runner_environment: String,
    },

    /// A sandbox requirement names the `Agent` subject, but the contract declares no agent.
    #[error(
        "sandbox requirement names the agent, but the contract declares AgentObligation::NotUsed"
    )]
    SandboxAgentSubjectWithoutAgent,

    /// A sandbox requirement names a `Gate` subject that is not a required, applicable gate
    /// (it is optional or waived) — which would make that gate implicitly required.
    #[error("sandbox requirement references gate {gate}, which is not required+applicable")]
    SandboxSubjectGateIneligible { gate: GateId },

    /// A policy `protects` an unknown gate.
    #[error("policy protects unknown gate {gate}: not a declared obligation")]
    PolicyProtectsUnknownGate { gate: GateId },

    /// A policy `protects` the agent, but the contract declares no agent.
    #[error("policy protects the agent, but the contract declares AgentObligation::NotUsed")]
    PolicyProtectsAgentWithoutAgent,

    /// The same protected subject appears twice within one policy's `protects`.
    #[error("duplicate protected subject in a policy obligation: {subject:?}")]
    DuplicateProtectedSubject { subject: PolicyProtectedSubject },

    /// A non-empty `protects` on an `Optional` policy — an optional pre-check cannot gate a
    /// start.
    #[error(
        "optional policy {policy_digest} declares protected subjects; only a required policy may"
    )]
    OptionalPolicyWithProtects { policy_digest: Digest256 },

    /// A second `WorktreeCreated` (the singleton evidence slot cannot be overwritten).
    #[error("duplicate WorktreeCreated at sequence {sequence}")]
    DuplicateWorktree { sequence: u64 },

    /// A second `PatchCaptured` (the singleton evidence slot cannot be overwritten).
    #[error("duplicate PatchCaptured at sequence {sequence}")]
    DuplicatePatch { sequence: u64 },

    /// An agent event appeared although the contract declared `AgentObligation::NotUsed`.
    #[error("disallowed agent event at sequence {sequence}: the contract declares no agent")]
    DisallowedAgentEvent { sequence: u64 },

    /// The contract declared the same policy digest in more than one obligation.
    #[error("duplicate policy obligation for digest {policy_digest}")]
    DuplicatePolicyInContract { policy_digest: Digest256 },

    /// A `PolicyChecked` referenced a policy that was never declared as an obligation.
    #[error("unknown policy {policy_digest} at sequence {sequence}: not a declared obligation")]
    UnknownPolicy {
        sequence: u64,
        policy_digest: Digest256,
    },

    /// A second `PolicyChecked` for a policy already checked (no aggregation contract).
    #[error("duplicate policy check for {policy_digest} at sequence {sequence}")]
    DuplicatePolicyCheck {
        sequence: u64,
        policy_digest: Digest256,
    },

    /// A subject protected by a required policy started before that policy was checked
    /// `Allowed`.
    #[error("protected subject started before its policy was allowed at sequence {sequence}: {subject:?}")]
    ProtectedStartBeforePolicyAllowed {
        sequence: u64,
        subject: ExecutionSubject,
    },

    /// The contract declared the same sandbox requirement (subject + policy) twice.
    #[error("duplicate sandbox requirement: {subject:?} under {policy_digest}")]
    DuplicateSandboxRequirement {
        subject: ExecutionSubject,
        policy_digest: Digest256,
    },

    /// A sandbox requirement named a `Gate` subject whose gate is not a declared obligation.
    #[error("sandbox requirement references unknown gate {gate}")]
    SandboxSubjectUnknownGate { gate: GateId },

    /// A second `SandboxEvidenceCaptured` for the same (subject, policy) key (no aggregation
    /// contract).
    #[error("duplicate sandbox evidence at sequence {sequence}")]
    DuplicateSandboxEvidence { sequence: u64 },

    /// `SandboxEvidenceCaptured` for a (subject, policy) key that is not a declared
    /// requirement.
    #[error("sandbox evidence at sequence {sequence} does not match any declared requirement")]
    UnrequiredSandboxEvidence { sequence: u64 },

    /// A transition that cannot happen (e.g. any non-`RunStarted` event before the run
    /// began) not covered by a more specific variant.
    #[error("impossible transition at sequence {sequence}: {detail}")]
    ImpossibleTransition { sequence: u64, detail: String },

    /// Any event after `RunSealed`.
    #[error("event after seal at sequence {sequence}")]
    EventAfterSeal { sequence: u64 },

    /// The transition logic is not implemented in this (contract-only) commit.
    #[error("reducer transition logic is not implemented in this contract-only build")]
    Unimplemented,
}

/// Fold one event into the run state.
///
/// # Errors
/// [`ReduceError`] for any structural violation of the stream. A domain outcome (pass /
/// fail / blocked / error) is NEVER an error here — it is recorded in the returned state and
/// finalized into [`crate::state::Verdict`] at `RunSealed`.
pub fn reduce(state: RunState, event: &RunEvent) -> Result<RunState, ReduceError> {
    // CONTRACT-ONLY: see the module doc. The behavior is specified by the tests in this
    // commit and implemented next; until then reduction is explicitly unavailable.
    let _ = (&state, event);
    Err(ReduceError::Unimplemented)
}

/// Fold an entire stream from the initial state, returning the terminal state.
///
/// # Errors
/// The first [`ReduceError`] encountered (reduction is fail-fast and left-to-right).
pub fn reduce_all(events: &[RunEvent]) -> Result<RunState, ReduceError> {
    let mut state = RunState::initial();
    for event in events {
        state = reduce(state, event)?;
    }
    Ok(state)
}
