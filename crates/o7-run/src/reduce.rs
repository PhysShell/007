//! The pure run-state reducer: `reduce(state, event) -> Result<state, ReduceError>`.
//!
//! The reducer is PURE: same input state + event ⇒ same output, no I/O, no clock, no panics.
//! It validates the event ENVELOPE (schema, self-consistent digest, dense sequence, chain
//! link, run id, unique event id), fully validates the pre-execution CONTRACT at
//! `RunStarted`, folds lifecycle/evidence events left to right, and at `RunSealed` collapses
//! the accumulated obligations to a [`crate::state::Verdict`] under `Error > Fail > Blocked >
//! Pass`. Any structural violation of the stream fails loudly; a domain outcome never does.
//!
//! Artifact-CONTENT validation is not the reducer's job — it folds only the event payloads
//! and checks each artifact's declared KIND and locator; resolving and hashing artifact
//! bytes belongs to [`crate::replay`](mod@crate::replay). Its behavior is specified exhaustively by
//! `tests/reducer_transitions.rs`.

use std::collections::BTreeSet;

use crate::event::{
    AgentObligation, AgentOutcome, ArtifactKind, ArtifactRef, Digest256, ExecutionSubject,
    GateApplicability, GateObligation, GateOutcome, GateRequirement, PolicyObligation,
    PolicyOutcome, PolicyProtectedSubject, PolicyRequirement, RunContract, RunEvent, RunEventKind,
    SandboxEvidenceOutcome, RUN_EVENT_SCHEMA_VERSION,
};
use crate::ids::{GateId, RunEventId, RunId};
use crate::state::{
    AgentLifecycle, GateProgress, PolicyResult, RunPhase, RunState, SandboxEvidenceEntry,
    SandboxEvidenceKey, Verdict,
};

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

    /// A gate waiver carried a blank reason. A waiver is the ONLY way a required gate may be
    /// skipped, and it must be auditable — an empty reason would be an unexplained
    /// false-green.
    #[error("gate {gate} is waived with a blank reason")]
    EmptyWaiverReason { gate: GateId },

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

    /// A second `ProviderSessionCaptured` (the singleton evidence slot cannot be
    /// overwritten).
    #[error("duplicate ProviderSessionCaptured at sequence {sequence}")]
    DuplicateProviderSessionReceipt { sequence: u64 },

    /// A second `CommandBindingCaptured` (the singleton evidence slot cannot be
    /// overwritten).
    #[error("duplicate CommandBindingCaptured at sequence {sequence}")]
    DuplicateCommandBinding { sequence: u64 },
}

/// Fold one event into the run state.
///
/// Processing order: (1) reject post-seal events; (2) validate the envelope — schema,
/// self-consistent digest, dense sequence, chain link, run id, unique event id; (3) require
/// the first event to be `RunStarted`; (4) apply the event, validating the contract at
/// `RunStarted` and folding lifecycle/evidence otherwise; (5) at `RunSealed`, compute the
/// verdict and freeze the run.
///
/// # Errors
/// [`ReduceError`] for any structural violation of the stream. A domain outcome (pass /
/// fail / blocked / error) is NEVER an error here — it is recorded in the returned state and
/// finalized into [`crate::state::Verdict`] at `RunSealed`.
pub fn reduce(mut state: RunState, event: &RunEvent) -> Result<RunState, ReduceError> {
    let seq = event.sequence;

    // 1. Nothing may follow a seal.
    if state.phase == RunPhase::Sealed {
        return Err(ReduceError::EventAfterSeal { sequence: seq });
    }

    // 2. Envelope validation.
    if event.schema_version != RUN_EVENT_SCHEMA_VERSION {
        return Err(ReduceError::UnsupportedSchema {
            sequence: seq,
            found: event.schema_version,
            supported: RUN_EVENT_SCHEMA_VERSION,
        });
    }
    if !event.digest_is_consistent() {
        return Err(ReduceError::InconsistentDigest { sequence: seq });
    }
    match state.last_sequence {
        None if seq != 0 => {
            return Err(ReduceError::OutOfOrder {
                expected: 0,
                found: seq,
            })
        }
        Some(last) if seq == last => return Err(ReduceError::DuplicateSequence { sequence: seq }),
        Some(last) if seq != last + 1 => {
            return Err(ReduceError::OutOfOrder {
                expected: last + 1,
                found: seq,
            })
        }
        _ => {}
    }
    if event.previous_event_digest != state.last_event_digest {
        return Err(ReduceError::BrokenChain { sequence: seq });
    }
    if let Some(expected) = &state.run_id {
        if &event.run_id != expected {
            return Err(ReduceError::RunIdMismatch {
                sequence: seq,
                expected: expected.clone(),
                found: event.run_id.clone(),
            });
        }
    }
    if state.seen_event_ids.contains(&event.event_id) {
        return Err(ReduceError::DuplicateEventId {
            sequence: seq,
            event_id: event.event_id.clone(),
        });
    }

    // 3. The first event must be `RunStarted`.
    if state.phase == RunPhase::NotStarted && !matches!(event.kind, RunEventKind::RunStarted { .. })
    {
        return Err(ReduceError::MissingRunStarted {
            found: event.kind.name(),
            sequence: seq,
        });
    }

    // 4/5. Apply the event.
    apply_kind(&mut state, event, seq)?;

    state.seen_event_ids.insert(event.event_id.clone());
    state.last_sequence = Some(seq);
    state.last_event_digest = event.event_digest.clone();
    Ok(state)
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

/// Apply one event's payload (the envelope is already validated). `RunStarted` binds the
/// contract; every other event borrows it — moved out for the duration and restored — so the
/// reduce hot path never re-clones the contract.
fn apply_kind(state: &mut RunState, event: &RunEvent, seq: u64) -> Result<(), ReduceError> {
    if let RunEventKind::RunStarted { contract, task } = &event.kind {
        if state.phase != RunPhase::NotStarted {
            return Err(ReduceError::DuplicateStart { sequence: seq });
        }
        validate_contract(contract, seq)?;
        validate_artifact(task, ArtifactKind::Task, seq)?;
        state.run_id = Some(event.run_id.clone());
        state.contract = Some(contract.clone());
        state.task = Some(task.clone());
        state.phase = RunPhase::Running;
        return Ok(());
    }

    let contract = match state.contract.take() {
        Some(contract) => contract,
        None => {
            return Err(ReduceError::MissingRunStarted {
                found: event.kind.name(),
                sequence: seq,
            })
        }
    };
    let result = apply_with_contract(state, &contract, event, seq);
    state.contract = Some(contract);
    result
}

/// Apply a non-`RunStarted` event against the already-bound `contract` (borrowed, not cloned).
fn apply_with_contract(
    state: &mut RunState,
    contract: &RunContract,
    event: &RunEvent,
    seq: u64,
) -> Result<(), ReduceError> {
    match &event.kind {
        // Handled in `apply_kind`; a second RunStarted is a duplicate.
        RunEventKind::RunStarted { .. } => Err(ReduceError::DuplicateStart { sequence: seq }),
        RunEventKind::WorktreeCreated { worktree } => {
            validate_artifact(worktree, ArtifactKind::Worktree, seq)?;
            if state.worktree.is_some() {
                return Err(ReduceError::DuplicateWorktree { sequence: seq });
            }
            state.worktree = Some(worktree.clone());
            Ok(())
        }
        RunEventKind::PatchCaptured { patch } => {
            validate_artifact(patch, ArtifactKind::Diff, seq)?;
            if state.patch.is_some() {
                return Err(ReduceError::DuplicatePatch { sequence: seq });
            }
            state.patch = Some(patch.clone());
            Ok(())
        }
        RunEventKind::AgentStarted => {
            if matches!(contract.agent, AgentObligation::NotUsed { .. }) {
                return Err(ReduceError::DisallowedAgentEvent { sequence: seq });
            }
            if !matches!(state.agent, AgentLifecycle::NotObserved) {
                return Err(ReduceError::DuplicateAgentStart { sequence: seq });
            }
            check_protected_start(state, contract, &PolicyProtectedSubject::Agent, seq)?;
            state.agent = AgentLifecycle::Started;
            Ok(())
        }
        RunEventKind::AgentExited { outcome } => {
            if matches!(contract.agent, AgentObligation::NotUsed { .. }) {
                return Err(ReduceError::DisallowedAgentEvent { sequence: seq });
            }
            match &state.agent {
                AgentLifecycle::NotObserved => {
                    // Only `FailedToStart` legitimately needs no prior `AgentStarted`.
                    if !matches!(outcome, AgentOutcome::FailedToStart { .. }) {
                        return Err(ReduceError::AgentExitBeforeStart { sequence: seq });
                    }
                }
                AgentLifecycle::Started => {}
                AgentLifecycle::Exited { .. } => {
                    return Err(ReduceError::DuplicateAgentExit { sequence: seq })
                }
            }
            state.agent = AgentLifecycle::Exited {
                outcome: outcome.clone(),
            };
            Ok(())
        }
        RunEventKind::PolicyChecked { policy, outcome } => {
            validate_artifact(policy, ArtifactKind::Policy, seq)?;
            if !contract
                .policy_obligations
                .iter()
                .any(|o| o.policy.digest == policy.digest)
            {
                return Err(ReduceError::UnknownPolicy {
                    sequence: seq,
                    policy_digest: policy.digest.clone(),
                });
            }
            if state
                .policy_results
                .iter()
                .any(|r| r.policy.digest == policy.digest)
            {
                return Err(ReduceError::DuplicatePolicyCheck {
                    sequence: seq,
                    policy_digest: policy.digest.clone(),
                });
            }
            state.policy_results.push(PolicyResult {
                policy: policy.clone(),
                outcome: *outcome,
            });
            Ok(())
        }
        RunEventKind::GateStarted { gate } => {
            if gate_obligation(contract, gate).is_none() {
                return Err(ReduceError::UnknownGate {
                    sequence: seq,
                    gate: gate.clone(),
                });
            }
            if state.gates.contains_key(gate) {
                return Err(ReduceError::DuplicateGateStarted {
                    sequence: seq,
                    gate: gate.clone(),
                });
            }
            check_protected_start(
                state,
                contract,
                &PolicyProtectedSubject::Gate { gate: gate.clone() },
                seq,
            )?;
            state.gates.insert(gate.clone(), GateProgress::Started);
            Ok(())
        }
        RunEventKind::GateFinished { gate, outcome, log } => {
            if gate_obligation(contract, gate).is_none() {
                return Err(ReduceError::UnknownGate {
                    sequence: seq,
                    gate: gate.clone(),
                });
            }
            if let Some(log) = log {
                validate_artifact(log, ArtifactKind::GateLog, seq)?;
            }
            match state.gates.get(gate) {
                None => {
                    return Err(ReduceError::GateFinishedWithoutStart {
                        sequence: seq,
                        gate: gate.clone(),
                    })
                }
                Some(GateProgress::Finished { .. }) => {
                    return Err(ReduceError::DuplicateGateFinished {
                        sequence: seq,
                        gate: gate.clone(),
                    })
                }
                Some(GateProgress::Started) => {}
            }
            state
                .gates
                .insert(gate.clone(), GateProgress::Finished { outcome: *outcome });
            Ok(())
        }
        RunEventKind::SandboxEvidenceCaptured {
            subject,
            policy_digest,
            report,
            outcome,
        } => {
            validate_artifact(report, ArtifactKind::SandboxReport, seq)?;
            let matches_requirement = contract
                .sandbox_requirements
                .iter()
                .any(|r| &r.subject == subject && &r.policy_digest == policy_digest);
            if !matches_requirement {
                return Err(ReduceError::UnrequiredSandboxEvidence { sequence: seq });
            }
            let key = SandboxEvidenceKey {
                subject: subject.clone(),
                policy_digest: policy_digest.clone(),
            };
            if state.sandbox_evidence.iter().any(|e| e.key == key) {
                return Err(ReduceError::DuplicateSandboxEvidence { sequence: seq });
            }
            let entry = SandboxEvidenceEntry {
                key,
                outcome: *outcome,
            };
            let pos = state
                .sandbox_evidence
                .binary_search_by(|e| e.key.cmp(&entry.key))
                .unwrap_or_else(|p| p);
            state.sandbox_evidence.insert(pos, entry);
            Ok(())
        }
        RunEventKind::ProviderSessionCaptured { receipt } => {
            validate_artifact(receipt, ArtifactKind::ProviderSession, seq)?;
            if state.provider_session_receipt.is_some() {
                return Err(ReduceError::DuplicateProviderSessionReceipt { sequence: seq });
            }
            state.provider_session_receipt = Some(receipt.clone());
            Ok(())
        }
        RunEventKind::CommandBindingCaptured { binding } => {
            validate_artifact(binding, ArtifactKind::CommandBinding, seq)?;
            if state.command_binding.is_some() {
                return Err(ReduceError::DuplicateCommandBinding { sequence: seq });
            }
            state.command_binding = Some(binding.clone());
            Ok(())
        }
        RunEventKind::RunSealed => {
            state.verdict = Some(compute_verdict(state, contract));
            state.phase = RunPhase::Sealed;
            Ok(())
        }
    }
}

fn gate_obligation<'a>(contract: &'a RunContract, gate: &GateId) -> Option<&'a GateObligation> {
    contract.gate_obligations.iter().find(|o| &o.gate == gate)
}

/// A gate that is required AND applicable (not waived) — the only kind that can be a sandbox
/// subject or that blocks the verdict when unrun.
fn is_required_applicable(obligation: &GateObligation) -> bool {
    matches!(obligation.requirement, GateRequirement::Required)
        && matches!(obligation.applicability, GateApplicability::Applicable)
}

/// Validate an artifact reference's locator and kind.
fn validate_artifact(
    artifact: &ArtifactRef,
    expected: ArtifactKind,
    seq: u64,
) -> Result<(), ReduceError> {
    if artifact.locator.is_empty() {
        return Err(ReduceError::EmptyArtifactLocator { sequence: seq });
    }
    if artifact.kind != expected {
        return Err(ReduceError::WrongArtifactKind {
            sequence: seq,
            expected,
            found: artifact.kind,
        });
    }
    Ok(())
}

/// Fully validate the pre-execution contract at `RunStarted`.
fn validate_contract(contract: &RunContract, seq: u64) -> Result<(), ReduceError> {
    let agent_declared = matches!(contract.agent, AgentObligation::Required);

    // Gates: unique ids, and any waiver must match the run environment.
    let mut seen_gates: BTreeSet<&GateId> = BTreeSet::new();
    for o in &contract.gate_obligations {
        if !seen_gates.insert(&o.gate) {
            return Err(ReduceError::DuplicateGateInContract {
                gate: o.gate.clone(),
            });
        }
        if let GateApplicability::Waived {
            environment,
            reason,
        } = &o.applicability
        {
            if environment != &contract.runner_environment {
                return Err(ReduceError::WaiverEnvironmentMismatch {
                    gate: o.gate.clone(),
                    waiver_environment: environment.clone(),
                    runner_environment: contract.runner_environment.clone(),
                });
            }
            if reason.trim().is_empty() {
                return Err(ReduceError::EmptyWaiverReason {
                    gate: o.gate.clone(),
                });
            }
        }
    }

    // Policy obligations: valid policy artifact, unique digest, protects well-formed.
    let mut seen_policies: BTreeSet<&Digest256> = BTreeSet::new();
    for p in &contract.policy_obligations {
        validate_artifact(&p.policy, ArtifactKind::Policy, seq)?;
        if !seen_policies.insert(&p.policy.digest) {
            return Err(ReduceError::DuplicatePolicyInContract {
                policy_digest: p.policy.digest.clone(),
            });
        }
        if !p.protects.is_empty() && !matches!(p.requirement, PolicyRequirement::Required) {
            return Err(ReduceError::OptionalPolicyWithProtects {
                policy_digest: p.policy.digest.clone(),
            });
        }
        let mut seen_protected: Vec<&PolicyProtectedSubject> = Vec::new();
        for s in &p.protects {
            if seen_protected.contains(&s) {
                return Err(ReduceError::DuplicateProtectedSubject { subject: s.clone() });
            }
            seen_protected.push(s);
            match s {
                PolicyProtectedSubject::Agent => {
                    if !agent_declared {
                        return Err(ReduceError::PolicyProtectsAgentWithoutAgent);
                    }
                }
                PolicyProtectedSubject::Gate { gate } => {
                    if gate_obligation(contract, gate).is_none() {
                        return Err(ReduceError::PolicyProtectsUnknownGate { gate: gate.clone() });
                    }
                }
            }
        }
    }

    // Sandbox requirements: unique, and each subject must be eligible.
    let mut seen_sandbox: Vec<(&ExecutionSubject, &Digest256)> = Vec::new();
    for r in &contract.sandbox_requirements {
        let key = (&r.subject, &r.policy_digest);
        if seen_sandbox.contains(&key) {
            return Err(ReduceError::DuplicateSandboxRequirement {
                subject: r.subject.clone(),
                policy_digest: r.policy_digest.clone(),
            });
        }
        seen_sandbox.push(key);
        match &r.subject {
            ExecutionSubject::Agent => {
                if !agent_declared {
                    return Err(ReduceError::SandboxAgentSubjectWithoutAgent);
                }
            }
            ExecutionSubject::Gate { gate } => match gate_obligation(contract, gate) {
                None => return Err(ReduceError::SandboxSubjectUnknownGate { gate: gate.clone() }),
                Some(o) if !is_required_applicable(o) => {
                    return Err(ReduceError::SandboxSubjectGateIneligible { gate: gate.clone() })
                }
                Some(_) => {}
            },
            ExecutionSubject::Verifier => {}
        }
    }

    Ok(())
}

/// A subject protected by a required policy may not start before that policy is `Allowed`.
fn check_protected_start(
    state: &RunState,
    contract: &RunContract,
    subject: &PolicyProtectedSubject,
    seq: u64,
) -> Result<(), ReduceError> {
    for p in &contract.policy_obligations {
        if !matches!(p.requirement, PolicyRequirement::Required) {
            continue;
        }
        if !p.protects.iter().any(|s| s == subject) {
            continue;
        }
        let allowed = state.policy_results.iter().any(|r| {
            r.policy.digest == p.policy.digest && matches!(r.outcome, PolicyOutcome::Allowed)
        });
        if !allowed {
            return Err(ReduceError::ProtectedStartBeforePolicyAllowed {
                sequence: seq,
                subject: protected_to_execution(subject),
            });
        }
    }
    Ok(())
}

fn protected_to_execution(subject: &PolicyProtectedSubject) -> ExecutionSubject {
    match subject {
        PolicyProtectedSubject::Agent => ExecutionSubject::Agent,
        PolicyProtectedSubject::Gate { gate } => ExecutionSubject::Gate { gate: gate.clone() },
    }
}

/// Compute the final verdict at `RunSealed` from the folded state and the contract, applying
/// `Error > Fail > Blocked > Pass`. The state retains the failed AND blocked obligations;
/// this only collapses them to the winning label.
fn compute_verdict(state: &RunState, contract: &RunContract) -> Verdict {
    let mut error = false;
    let mut fail = false;
    let mut blocked = false;

    // Agent obligation.
    if matches!(contract.agent, AgentObligation::Required) {
        match &state.agent {
            AgentLifecycle::Exited { outcome } => {
                if !matches!(outcome, AgentOutcome::ExitedNormally { code: 0 }) {
                    error = true;
                }
            }
            AgentLifecycle::Started => error = true, // never reached a terminal outcome
            AgentLifecycle::NotObserved => match agent_nonstart_disposition(state, contract) {
                NonStart::Error => error = true,
                NonStart::Blocked => blocked = true,
            },
        }
    }

    // Gates: only required+applicable gates can move the verdict.
    for o in &contract.gate_obligations {
        if !is_required_applicable(o) {
            continue;
        }
        match state.gates.get(&o.gate) {
            None => blocked = true,
            Some(GateProgress::Started) => error = true,
            Some(GateProgress::Finished { outcome }) => match outcome {
                GateOutcome::Pass | GateOutcome::Warn => {}
                GateOutcome::Fail => fail = true,
                GateOutcome::Error => error = true,
                GateOutcome::Blocked | GateOutcome::NotApplicable => blocked = true,
            },
        }
    }

    // Policies: only required policies move the verdict.
    for p in &contract.policy_obligations {
        if !matches!(p.requirement, PolicyRequirement::Required) {
            continue;
        }
        match state
            .policy_results
            .iter()
            .find(|r| r.policy.digest == p.policy.digest)
        {
            None => blocked = true,
            Some(r) => match r.outcome {
                PolicyOutcome::Allowed => {}
                PolicyOutcome::Denied => blocked = true,
                PolicyOutcome::Error => error = true,
            },
        }
    }

    // Sandbox requirements (all are mandatory obligations).
    for req in &contract.sandbox_requirements {
        match state
            .sandbox_evidence
            .iter()
            .find(|e| e.key.subject == req.subject && e.key.policy_digest == req.policy_digest)
        {
            None => blocked = true,
            Some(e) => match e.outcome {
                SandboxEvidenceOutcome::Satisfied => {}
                SandboxEvidenceOutcome::Violated => blocked = true,
                SandboxEvidenceOutcome::Error => error = true,
            },
        }
    }

    if error {
        Verdict::Error
    } else if fail {
        Verdict::Fail
    } else if blocked {
        Verdict::Blocked
    } else {
        Verdict::Pass
    }
}

/// Disposition of a required agent that never started, per the cross-obligation rule.
enum NonStart {
    Error,
    Blocked,
}

fn agent_nonstart_disposition(state: &RunState, contract: &RunContract) -> NonStart {
    let protectors: Vec<&PolicyObligation> = contract
        .policy_obligations
        .iter()
        .filter(|o| matches!(o.requirement, PolicyRequirement::Required))
        .filter(|o| {
            o.protects
                .iter()
                .any(|s| matches!(s, PolicyProtectedSubject::Agent))
        })
        .collect();
    if protectors.is_empty() {
        // Nothing blocked it, yet the obligated agent never ran.
        return NonStart::Error;
    }
    let mut any_error = false;
    let mut any_denied_or_absent = false;
    for o in &protectors {
        match state
            .policy_results
            .iter()
            .find(|r| r.policy.digest == o.policy.digest)
            .map(|r| r.outcome)
        {
            Some(PolicyOutcome::Error) => any_error = true,
            Some(PolicyOutcome::Denied) | None => any_denied_or_absent = true,
            Some(PolicyOutcome::Allowed) => {}
        }
    }
    if any_error {
        NonStart::Error
    } else if any_denied_or_absent {
        NonStart::Blocked
    } else {
        // Every protecting policy was Allowed, so nothing blocked it → Error.
        NonStart::Error
    }
}
