//! RED transition-table tests for the pure reducer.
//!
//! Every test asserts the TARGET semantics of `reduce`/`reduce_all`. Because this commit
//! ships the reducer as contract-only (`ReduceError::Unimplemented`), these tests FAIL by
//! construction — they are the executable specification a following commit turns green. They
//! must NEVER be satisfied by an unimplemented reducer returning a plausible answer.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod common;

use common::*;
use o7_run::event::{
    AgentObligation, AgentOutcome, ArtifactKind, ExecutionSubject, GateOutcome, PolicyOutcome,
    PolicyProtectedSubject, PolicyRequirement, RunContract, SandboxEvidenceOutcome,
    SandboxRequirement,
};
use o7_run::reduce::ReduceError;
use o7_run::{reduce_all, Digest256, PolicyObligation, RunEvent, RunEventKind, RunId, Verdict};

fn verdict_of(events: &[RunEvent]) -> Verdict {
    reduce_all(events)
        .expect("well-formed stream reduces")
        .verdict
        .expect("a sealed run has a verdict")
}

// ============================ verdict reduction: gates ============================

#[test]
fn all_required_gates_pass_is_pass() {
    let events = single_required_gate_stream("build", GateOutcome::Pass);
    assert_eq!(verdict_of(&events), Verdict::Pass);
}

#[test]
fn a_required_gate_that_did_not_execute_is_blocked_never_pass() {
    let events = chained(vec![
        run_started(contract(vec![req("build"), req("lint")])),
        RunEventKind::GateStarted {
            gate: gate("build"),
        },
        RunEventKind::GateFinished {
            gate: gate("build"),
            outcome: GateOutcome::Pass,
            log: None,
        },
        RunEventKind::RunSealed,
    ]);
    assert_eq!(
        verdict_of(&events),
        Verdict::Blocked,
        "a required-but-unexecuted gate must never yield PASS"
    );
}

#[test]
fn a_required_applicable_gate_reported_not_applicable_is_blocked() {
    let events = single_required_gate_stream("win-only", GateOutcome::NotApplicable);
    assert_eq!(verdict_of(&events), Verdict::Blocked);
}

#[test]
fn a_gate_waived_for_this_environment_may_be_skipped_and_still_pass() {
    let events = chained(vec![
        run_started(contract(vec![
            req("build"),
            waived("win-only", "linux", "windows-only, not run on linux"),
        ])),
        RunEventKind::GateStarted {
            gate: gate("build"),
        },
        RunEventKind::GateFinished {
            gate: gate("build"),
            outcome: GateOutcome::Pass,
            log: None,
        },
        RunEventKind::RunSealed,
    ]);
    assert_eq!(verdict_of(&events), Verdict::Pass);
}

#[test]
fn a_required_gate_returning_failure_is_fail() {
    let events = single_required_gate_stream("build", GateOutcome::Fail);
    assert_eq!(verdict_of(&events), Verdict::Fail);
}

#[test]
fn a_gate_that_could_not_run_is_error_not_fail() {
    let events = single_required_gate_stream("build", GateOutcome::Error);
    assert_eq!(verdict_of(&events), Verdict::Error);
}

#[test]
fn an_optional_gate_failure_never_blocks() {
    let events = chained(vec![
        run_started(contract(vec![req("build"), opt("bench")])),
        RunEventKind::GateStarted {
            gate: gate("build"),
        },
        RunEventKind::GateFinished {
            gate: gate("build"),
            outcome: GateOutcome::Pass,
            log: None,
        },
        RunEventKind::GateStarted {
            gate: gate("bench"),
        },
        RunEventKind::GateFinished {
            gate: gate("bench"),
            outcome: GateOutcome::Fail,
            log: None,
        },
        RunEventKind::RunSealed,
    ]);
    assert_eq!(verdict_of(&events), Verdict::Pass);
}

#[test]
fn a_warning_only_required_gate_still_passes() {
    let events = single_required_gate_stream("build", GateOutcome::Warn);
    assert_eq!(verdict_of(&events), Verdict::Pass);
}

#[test]
fn an_unfinished_required_gate_at_seal_is_error() {
    let events = chained(vec![
        run_started(contract(vec![req("build")])),
        RunEventKind::GateStarted {
            gate: gate("build"),
        },
        RunEventKind::RunSealed,
    ]);
    assert_eq!(verdict_of(&events), Verdict::Error);
}

// ============================ verdict reduction: agent ============================

/// A required-agent stream that exits with `outcome`, with a passing gate, then seals.
fn agent_outcome_stream(outcome: AgentOutcome) -> Vec<RunEvent> {
    chained(vec![
        run_started(contract_agent(vec![req("build")])),
        RunEventKind::AgentStarted,
        RunEventKind::AgentExited { outcome },
        RunEventKind::GateStarted {
            gate: gate("build"),
        },
        RunEventKind::GateFinished {
            gate: gate("build"),
            outcome: GateOutcome::Pass,
            log: None,
        },
        RunEventKind::RunSealed,
    ])
}

#[test]
fn a_required_agent_never_started_is_error() {
    let events = chained(vec![
        run_started(contract_agent(vec![req("build")])),
        RunEventKind::GateStarted {
            gate: gate("build"),
        },
        RunEventKind::GateFinished {
            gate: gate("build"),
            outcome: GateOutcome::Pass,
            log: None,
        },
        RunEventKind::RunSealed,
    ]);
    assert_eq!(
        verdict_of(&events),
        Verdict::Error,
        "a required agent that never ran is a harness Error"
    );
}

#[test]
fn a_required_agent_started_but_not_terminal_at_seal_is_error() {
    let events = chained(vec![
        run_started(contract_agent(vec![req("build")])),
        RunEventKind::AgentStarted,
        RunEventKind::GateStarted {
            gate: gate("build"),
        },
        RunEventKind::GateFinished {
            gate: gate("build"),
            outcome: GateOutcome::Pass,
            log: None,
        },
        RunEventKind::RunSealed,
    ]);
    assert_eq!(verdict_of(&events), Verdict::Error);
}

#[test]
fn an_agent_clean_exit_zero_is_neutral() {
    // The agent exited 0 and the gate passed → the agent contributes nothing; PASS.
    assert_eq!(
        verdict_of(&agent_outcome_stream(AgentOutcome::ExitedNormally {
            code: 0
        })),
        Verdict::Pass
    );
}

#[test]
fn an_agent_nonzero_exit_is_error() {
    assert_eq!(
        verdict_of(&agent_outcome_stream(AgentOutcome::ExitedNormally {
            code: 1
        })),
        Verdict::Error
    );
}

#[test]
fn an_agent_that_failed_to_start_is_error() {
    let events = chained(vec![
        run_started(contract_agent(vec![req("build")])),
        RunEventKind::AgentExited {
            outcome: AgentOutcome::FailedToStart {
                reason: "no such executable".to_owned(),
            },
        },
        RunEventKind::RunSealed,
    ]);
    assert_eq!(verdict_of(&events), Verdict::Error);
}

#[test]
fn agent_faults_and_cancellations_are_all_error() {
    for outcome in [
        AgentOutcome::ExitedBySignal { signal: 9 },
        AgentOutcome::CancelledGracefully,
        AgentOutcome::CancelledForcefully,
        AgentOutcome::BoundaryFailure {
            reason: "boundary".to_owned(),
        },
        AgentOutcome::ObservationFailure {
            reason: "sink".to_owned(),
        },
        AgentOutcome::OutputFailure {
            reason: "pipe".to_owned(),
        },
        AgentOutcome::CleanupFailure {
            reason: "group not proven gone".to_owned(),
        },
    ] {
        let events = agent_outcome_stream(outcome.clone());
        assert_eq!(
            verdict_of(&events),
            Verdict::Error,
            "agent outcome {outcome:?} must be Error"
        );
    }
}

#[test]
fn a_cleanup_failure_is_error_even_when_every_gate_passes() {
    // Emphasis: clean nothing else, gate passes, but the owned set was not proven gone.
    assert_eq!(
        verdict_of(&agent_outcome_stream(AgentOutcome::CleanupFailure {
            reason: "process group not proven gone".to_owned(),
        })),
        Verdict::Error
    );
}

// ============================ verdict reduction: policy ============================

fn policy_stream(checks: Vec<RunEventKind>) -> Vec<RunEvent> {
    let mut kinds = vec![run_started(contract_policy(
        vec![req("build")],
        vec![required_policy(policy_artifact(), Vec::new())],
    ))];
    kinds.extend(checks);
    kinds.push(RunEventKind::GateStarted {
        gate: gate("build"),
    });
    kinds.push(RunEventKind::GateFinished {
        gate: gate("build"),
        outcome: GateOutcome::Pass,
        log: None,
    });
    kinds.push(RunEventKind::RunSealed);
    chained(kinds)
}

#[test]
fn a_required_policy_not_checked_is_blocked() {
    assert_eq!(verdict_of(&policy_stream(vec![])), Verdict::Blocked);
}

#[test]
fn a_required_policy_allowed_is_discharged() {
    let events = policy_stream(vec![RunEventKind::PolicyChecked {
        policy: policy_artifact(),
        outcome: PolicyOutcome::Allowed,
    }]);
    assert_eq!(verdict_of(&events), Verdict::Pass);
}

#[test]
fn a_policy_denial_is_blocked() {
    let events = policy_stream(vec![RunEventKind::PolicyChecked {
        policy: policy_artifact(),
        outcome: PolicyOutcome::Denied,
    }]);
    assert_eq!(verdict_of(&events), Verdict::Blocked);
}

#[test]
fn a_policy_engine_error_is_error() {
    let events = policy_stream(vec![RunEventKind::PolicyChecked {
        policy: policy_artifact(),
        outcome: PolicyOutcome::Error,
    }]);
    assert_eq!(verdict_of(&events), Verdict::Error);
}

// ============================ verdict reduction: sandbox ============================

fn sandbox_stream(evidence: Vec<RunEventKind>) -> Vec<RunEvent> {
    // A GATE subject (build is required+applicable), so the requirement is valid even though
    // this fixture declares no agent.
    let mut kinds = vec![run_started(contract_sandboxed(
        vec![req("build")],
        vec![gate_sandbox_requirement("build")],
    ))];
    kinds.extend(evidence);
    kinds.push(RunEventKind::GateStarted {
        gate: gate("build"),
    });
    kinds.push(RunEventKind::GateFinished {
        gate: gate("build"),
        outcome: GateOutcome::Pass,
        log: None,
    });
    kinds.push(RunEventKind::RunSealed);
    chained(kinds)
}

fn evidence(outcome: SandboxEvidenceOutcome, policy_digest: Digest256) -> RunEventKind {
    RunEventKind::SandboxEvidenceCaptured {
        subject: ExecutionSubject::Gate {
            gate: gate("build"),
        },
        policy_digest,
        report: artifact(ArtifactKind::SandboxReport, "sandbox.json", b"{}"),
        outcome,
    }
}

#[test]
fn a_required_sandbox_without_evidence_is_blocked() {
    assert_eq!(verdict_of(&sandbox_stream(vec![])), Verdict::Blocked);
}

#[test]
fn a_required_sandbox_with_satisfying_evidence_can_pass() {
    let events = sandbox_stream(vec![evidence(
        SandboxEvidenceOutcome::Satisfied,
        sandbox_policy_digest(),
    )]);
    assert_eq!(verdict_of(&events), Verdict::Pass);
}

#[test]
fn a_violated_sandbox_is_blocked() {
    let events = sandbox_stream(vec![evidence(
        SandboxEvidenceOutcome::Violated,
        sandbox_policy_digest(),
    )]);
    assert_eq!(verdict_of(&events), Verdict::Blocked);
}

#[test]
fn an_errored_sandbox_report_is_error() {
    let events = sandbox_stream(vec![evidence(
        SandboxEvidenceOutcome::Error,
        sandbox_policy_digest(),
    )]);
    assert_eq!(verdict_of(&events), Verdict::Error);
}

// ============================ precedence ============================

#[test]
fn precedence_is_error_over_fail_over_blocked() {
    let events = chained(vec![
        run_started(contract(vec![req("a"), req("b"), req("c")])),
        RunEventKind::GateStarted { gate: gate("a") },
        RunEventKind::GateFinished {
            gate: gate("a"),
            outcome: GateOutcome::Error,
            log: None,
        },
        RunEventKind::GateStarted { gate: gate("b") },
        RunEventKind::GateFinished {
            gate: gate("b"),
            outcome: GateOutcome::Fail,
            log: None,
        },
        RunEventKind::RunSealed,
    ]);
    assert_eq!(verdict_of(&events), Verdict::Error);
}

#[test]
fn fail_outranks_blocked() {
    let events = chained(vec![
        run_started(contract(vec![req("a"), req("b")])),
        RunEventKind::GateStarted { gate: gate("a") },
        RunEventKind::GateFinished {
            gate: gate("a"),
            outcome: GateOutcome::Fail,
            log: None,
        },
        RunEventKind::RunSealed,
    ]);
    assert_eq!(verdict_of(&events), Verdict::Fail);
}

// ============================ byte-stable replay of state ============================

#[test]
fn reducing_the_same_stream_twice_is_byte_stable() {
    let events = single_required_gate_stream("build", GateOutcome::Pass);
    let a = reduce_all(&events).expect("reduces");
    let b = reduce_all(&events).expect("reduces");
    assert_eq!(
        serde_json::to_string(&a).unwrap(),
        serde_json::to_string(&b).unwrap(),
        "the normalized state must be byte-stable across replays"
    );
}

// ============================ structural: chain/sequence/schema ============================

fn structural_err(events: &[RunEvent]) -> ReduceError {
    reduce_all(events).expect_err("a malformed stream must fail loudly")
}

#[test]
fn an_event_before_run_started_fails_loudly() {
    let events = chained(vec![RunEventKind::AgentStarted, RunEventKind::RunSealed]);
    assert!(
        matches!(
            structural_err(&events),
            ReduceError::MissingRunStarted { .. }
        ),
        "{:?}",
        structural_err(&events)
    );
}

#[test]
fn a_duplicate_run_started_fails_loudly() {
    let events = chained(vec![
        run_started(contract(vec![req("build")])),
        run_started(contract(vec![req("build")])),
        RunEventKind::RunSealed,
    ]);
    assert!(matches!(
        structural_err(&events),
        ReduceError::DuplicateStart { .. }
    ));
}

#[test]
fn an_out_of_order_sequence_fails_loudly() {
    let run = run_id();
    let e0 = make_event(
        &run,
        0,
        &Digest256::genesis(),
        run_started(contract(vec![req("build")])),
    );
    let e1 = make_event(&run, 5, &e0.event_digest, RunEventKind::RunSealed);
    assert!(matches!(
        structural_err(&[e0, e1]),
        ReduceError::OutOfOrder { .. }
    ));
}

#[test]
fn a_duplicate_sequence_fails_loudly() {
    let run = run_id();
    let e0 = make_event(
        &run,
        0,
        &Digest256::genesis(),
        run_started(contract(vec![req("build")])),
    );
    // A valid evidence event at sequence 1, then a SECOND event re-using sequence 1 — the
    // only defect is the repeated sequence.
    let e1 = make_event(
        &run,
        1,
        &e0.event_digest,
        RunEventKind::WorktreeCreated {
            worktree: artifact(ArtifactKind::Worktree, "worktree", b"wt"),
        },
    );
    let e1_dup = make_event(&run, 1, &e1.event_digest, RunEventKind::RunSealed);
    assert!(matches!(
        structural_err(&[e0, e1, e1_dup]),
        ReduceError::DuplicateSequence { .. }
    ));
}

#[test]
fn a_broken_chain_link_fails_loudly() {
    let run = run_id();
    let e0 = make_event(
        &run,
        0,
        &Digest256::genesis(),
        run_started(contract(vec![req("build")])),
    );
    let e1 = make_event(&run, 1, &Digest256::genesis(), RunEventKind::RunSealed);
    assert!(matches!(
        structural_err(&[e0, e1]),
        ReduceError::BrokenChain { .. }
    ));
}

#[test]
fn a_run_id_mismatch_fails_loudly() {
    let run_a = RunId::new("run-A").expect("id");
    let run_b = RunId::new("run-B").expect("id");
    let e0 = make_event(
        &run_a,
        0,
        &Digest256::genesis(),
        run_started(contract(vec![req("build")])),
    );
    let e1 = make_event(&run_b, 1, &e0.event_digest, RunEventKind::RunSealed);
    assert!(matches!(
        structural_err(&[e0, e1]),
        ReduceError::RunIdMismatch { .. }
    ));
}

#[test]
fn a_duplicate_event_id_fails_loudly() {
    let run = run_id();
    let e0 = event_with(
        &run,
        "same-id",
        1,
        0,
        &Digest256::genesis(),
        run_started(contract(vec![req("build")])),
    );
    let e1 = event_with(
        &run,
        "same-id",
        1,
        1,
        &e0.event_digest,
        RunEventKind::RunSealed,
    );
    assert!(matches!(
        structural_err(&[e0, e1]),
        ReduceError::DuplicateEventId { .. }
    ));
}

#[test]
fn an_unsupported_schema_fails_loudly() {
    let run = run_id();
    let e0 = event_with(
        &run,
        "ev-0",
        0,
        0,
        &Digest256::genesis(),
        run_started(contract(vec![req("build")])),
    );
    assert!(matches!(
        structural_err(&[e0]),
        ReduceError::UnsupportedSchema { .. }
    ));
}

// ============================ structural: gates ============================

#[test]
fn a_duplicate_gate_in_the_contract_fails_loudly() {
    let events = chained(vec![
        run_started(contract(vec![req("build"), req("build")])),
        RunEventKind::RunSealed,
    ]);
    assert!(matches!(
        structural_err(&events),
        ReduceError::DuplicateGateInContract { .. }
    ));
}

#[test]
fn an_unknown_gate_during_the_run_fails_loudly() {
    let events = chained(vec![
        run_started(contract(vec![req("build")])),
        RunEventKind::GateStarted {
            gate: gate("surprise"),
        },
        RunEventKind::RunSealed,
    ]);
    assert!(matches!(
        structural_err(&events),
        ReduceError::UnknownGate { .. }
    ));
}

#[test]
fn a_duplicate_gate_started_fails_loudly() {
    let events = chained(vec![
        run_started(contract(vec![req("build")])),
        RunEventKind::GateStarted {
            gate: gate("build"),
        },
        RunEventKind::GateStarted {
            gate: gate("build"),
        },
        RunEventKind::RunSealed,
    ]);
    assert!(matches!(
        structural_err(&events),
        ReduceError::DuplicateGateStarted { .. }
    ));
}

#[test]
fn a_gate_finished_without_started_fails_loudly() {
    let events = chained(vec![
        run_started(contract(vec![req("build")])),
        RunEventKind::GateFinished {
            gate: gate("build"),
            outcome: GateOutcome::Pass,
            log: None,
        },
        RunEventKind::RunSealed,
    ]);
    assert!(matches!(
        structural_err(&events),
        ReduceError::GateFinishedWithoutStart { .. }
    ));
}

#[test]
fn a_duplicate_gate_finished_fails_loudly() {
    let events = chained(vec![
        run_started(contract(vec![req("build")])),
        RunEventKind::GateStarted {
            gate: gate("build"),
        },
        RunEventKind::GateFinished {
            gate: gate("build"),
            outcome: GateOutcome::Pass,
            log: None,
        },
        RunEventKind::GateFinished {
            gate: gate("build"),
            outcome: GateOutcome::Pass,
            log: None,
        },
        RunEventKind::RunSealed,
    ]);
    assert!(matches!(
        structural_err(&events),
        ReduceError::DuplicateGateFinished { .. }
    ));
}

// ============================ structural: agent ============================

#[test]
fn an_agent_exit_before_start_fails_loudly() {
    let events = chained(vec![
        run_started(contract_agent(vec![req("build")])),
        RunEventKind::AgentExited {
            outcome: agent_ok(),
        },
        RunEventKind::RunSealed,
    ]);
    assert!(matches!(
        structural_err(&events),
        ReduceError::AgentExitBeforeStart { .. }
    ));
}

#[test]
fn a_duplicate_agent_start_fails_loudly() {
    let events = chained(vec![
        run_started(contract_agent(vec![req("build")])),
        RunEventKind::AgentStarted,
        RunEventKind::AgentStarted,
        RunEventKind::RunSealed,
    ]);
    assert!(matches!(
        structural_err(&events),
        ReduceError::DuplicateAgentStart { .. }
    ));
}

#[test]
fn a_duplicate_agent_exit_fails_loudly() {
    let events = chained(vec![
        run_started(contract_agent(vec![req("build")])),
        RunEventKind::AgentStarted,
        RunEventKind::AgentExited {
            outcome: agent_ok(),
        },
        RunEventKind::AgentExited {
            outcome: agent_ok(),
        },
        RunEventKind::RunSealed,
    ]);
    assert!(matches!(
        structural_err(&events),
        ReduceError::DuplicateAgentExit { .. }
    ));
}

#[test]
fn an_agent_event_when_none_declared_fails_loudly() {
    // contract() declares AgentObligation::NotUsed; an agent event contradicts it.
    let events = chained(vec![
        run_started(contract(vec![req("build")])),
        RunEventKind::AgentStarted,
        RunEventKind::RunSealed,
    ]);
    assert!(matches!(
        structural_err(&events),
        ReduceError::DisallowedAgentEvent { .. }
    ));
}

// ============================ structural: policy ============================

#[test]
fn a_duplicate_policy_obligation_fails_loudly() {
    let events = chained(vec![
        run_started(contract_policy(
            vec![req("build")],
            vec![
                required_policy(policy_artifact(), Vec::new()),
                required_policy(policy_artifact(), Vec::new()),
            ],
        )),
        RunEventKind::RunSealed,
    ]);
    assert!(matches!(
        structural_err(&events),
        ReduceError::DuplicatePolicyInContract { .. }
    ));
}

#[test]
fn a_check_for_an_undeclared_policy_fails_loudly() {
    let other = artifact(ArtifactKind::Policy, "other.json", b"{\"other\":true}");
    let events = chained(vec![
        run_started(contract_policy(
            vec![req("build")],
            vec![required_policy(policy_artifact(), Vec::new())],
        )),
        RunEventKind::PolicyChecked {
            policy: other,
            outcome: PolicyOutcome::Allowed,
        },
        RunEventKind::RunSealed,
    ]);
    assert!(matches!(
        structural_err(&events),
        ReduceError::UnknownPolicy { .. }
    ));
}

#[test]
fn a_duplicate_policy_check_fails_loudly() {
    let events = chained(vec![
        run_started(contract_policy(
            vec![req("build")],
            vec![required_policy(policy_artifact(), Vec::new())],
        )),
        RunEventKind::PolicyChecked {
            policy: policy_artifact(),
            outcome: PolicyOutcome::Allowed,
        },
        RunEventKind::PolicyChecked {
            policy: policy_artifact(),
            outcome: PolicyOutcome::Allowed,
        },
        RunEventKind::RunSealed,
    ]);
    assert!(matches!(
        structural_err(&events),
        ReduceError::DuplicatePolicyCheck { .. }
    ));
}

#[test]
fn a_protected_subject_starting_before_its_policy_is_allowed_fails_loudly() {
    // The agent is protected by a required policy; it must not start before an Allowed check.
    let contract = RunContract {
        gate_obligations: vec![req("build")],
        policy_obligations: vec![PolicyObligation {
            policy: policy_artifact(),
            requirement: PolicyRequirement::Required,
            protects: vec![PolicyProtectedSubject::Agent],
        }],
        sandbox_requirements: Vec::new(),
        agent: AgentObligation::Required,
        runner_environment: "linux".to_owned(),
    };
    let events = chained(vec![
        run_started(contract),
        RunEventKind::AgentStarted, // before the guarding policy is checked
        RunEventKind::PolicyChecked {
            policy: policy_artifact(),
            outcome: PolicyOutcome::Allowed,
        },
        RunEventKind::RunSealed,
    ]);
    assert!(matches!(
        structural_err(&events),
        ReduceError::ProtectedStartBeforePolicyAllowed { .. }
    ));
}

// ============================ structural: sandbox ============================

#[test]
fn a_duplicate_sandbox_requirement_fails_loudly() {
    let events = chained(vec![
        run_started(contract_sandboxed(
            vec![req("build")],
            vec![
                gate_sandbox_requirement("build"),
                gate_sandbox_requirement("build"),
            ],
        )),
        RunEventKind::RunSealed,
    ]);
    assert!(matches!(
        structural_err(&events),
        ReduceError::DuplicateSandboxRequirement { .. }
    ));
}

#[test]
fn a_sandbox_requirement_for_an_undeclared_gate_fails_loudly() {
    let req_unknown = SandboxRequirement {
        subject: ExecutionSubject::Gate { gate: gate("nope") },
        policy_digest: sandbox_policy_digest(),
    };
    let events = chained(vec![
        run_started(contract_sandboxed(vec![req("build")], vec![req_unknown])),
        RunEventKind::RunSealed,
    ]);
    assert!(matches!(
        structural_err(&events),
        ReduceError::SandboxSubjectUnknownGate { .. }
    ));
}

#[test]
fn a_duplicate_sandbox_evidence_fails_loudly() {
    let events = chained(vec![
        run_started(contract_sandboxed(
            vec![req("build")],
            vec![gate_sandbox_requirement("build")],
        )),
        evidence(SandboxEvidenceOutcome::Satisfied, sandbox_policy_digest()),
        evidence(SandboxEvidenceOutcome::Satisfied, sandbox_policy_digest()),
        RunEventKind::RunSealed,
    ]);
    assert!(matches!(
        structural_err(&events),
        ReduceError::DuplicateSandboxEvidence { .. }
    ));
}

#[test]
fn sandbox_evidence_without_a_matching_requirement_fails_loudly() {
    let events = chained(vec![
        run_started(contract_sandboxed(vec![req("build")], Vec::new())),
        evidence(SandboxEvidenceOutcome::Satisfied, sandbox_policy_digest()),
        RunEventKind::RunSealed,
    ]);
    assert!(matches!(
        structural_err(&events),
        ReduceError::UnrequiredSandboxEvidence { .. }
    ));
}

// ============================ structural: singleton evidence, kinds, locators ==========

#[test]
fn a_duplicate_worktree_fails_loudly() {
    let wt = artifact(ArtifactKind::Worktree, "worktree", b"wt");
    let events = chained(vec![
        run_started(contract(vec![req("build")])),
        RunEventKind::WorktreeCreated {
            worktree: wt.clone(),
        },
        RunEventKind::WorktreeCreated { worktree: wt },
        RunEventKind::RunSealed,
    ]);
    assert!(matches!(
        structural_err(&events),
        ReduceError::DuplicateWorktree { .. }
    ));
}

#[test]
fn a_duplicate_patch_fails_loudly() {
    let p = artifact(ArtifactKind::Diff, "diff.patch", b"p");
    let events = chained(vec![
        run_started(contract(vec![req("build")])),
        RunEventKind::PatchCaptured { patch: p.clone() },
        RunEventKind::PatchCaptured { patch: p },
        RunEventKind::RunSealed,
    ]);
    assert!(matches!(
        structural_err(&events),
        ReduceError::DuplicatePatch { .. }
    ));
}

#[test]
fn every_artifact_slot_enforces_its_kind() {
    // Task slot with a Diff-kind artifact.
    let bad_task = chained(vec![
        RunEventKind::RunStarted {
            contract: contract(vec![req("build")]),
            task: artifact(ArtifactKind::Diff, "task.json", b"x"),
        },
        RunEventKind::RunSealed,
    ]);
    assert!(matches!(
        structural_err(&bad_task),
        ReduceError::WrongArtifactKind { .. }
    ));

    // Worktree slot with a Task-kind artifact.
    let bad_wt = chained(vec![
        run_started(contract(vec![req("build")])),
        RunEventKind::WorktreeCreated {
            worktree: artifact(ArtifactKind::Task, "worktree", b"x"),
        },
        RunEventKind::RunSealed,
    ]);
    assert!(matches!(
        structural_err(&bad_wt),
        ReduceError::WrongArtifactKind { .. }
    ));

    // Patch slot with a Task-kind artifact.
    let bad_patch = chained(vec![
        run_started(contract(vec![req("build")])),
        RunEventKind::PatchCaptured {
            patch: artifact(ArtifactKind::Task, "diff.patch", b"x"),
        },
        RunEventKind::RunSealed,
    ]);
    assert!(matches!(
        structural_err(&bad_patch),
        ReduceError::WrongArtifactKind { .. }
    ));

    // Gate-log slot with a Task-kind artifact.
    let bad_log = chained(vec![
        run_started(contract(vec![req("build")])),
        RunEventKind::GateStarted {
            gate: gate("build"),
        },
        RunEventKind::GateFinished {
            gate: gate("build"),
            outcome: GateOutcome::Pass,
            log: Some(artifact(ArtifactKind::Task, "gate/build.log", b"x")),
        },
        RunEventKind::RunSealed,
    ]);
    assert!(matches!(
        structural_err(&bad_log),
        ReduceError::WrongArtifactKind { .. }
    ));

    // Sandbox-report slot with a Task-kind artifact (matching a declared requirement).
    let bad_report = chained(vec![
        run_started(contract_sandboxed(
            vec![req("build")],
            vec![gate_sandbox_requirement("build")],
        )),
        RunEventKind::SandboxEvidenceCaptured {
            subject: ExecutionSubject::Gate {
                gate: gate("build"),
            },
            policy_digest: sandbox_policy_digest(),
            report: artifact(ArtifactKind::Task, "sandbox.json", b"x"),
            outcome: SandboxEvidenceOutcome::Satisfied,
        },
        RunEventKind::RunSealed,
    ]);
    assert!(matches!(
        structural_err(&bad_report),
        ReduceError::WrongArtifactKind { .. }
    ));

    // Policy-obligation slot with a Diff-kind artifact.
    let bad_policy = chained(vec![
        run_started(contract_policy(
            vec![req("build")],
            vec![required_policy(
                artifact(ArtifactKind::Diff, "policy.json", b"x"),
                Vec::new(),
            )],
        )),
        RunEventKind::RunSealed,
    ]);
    assert!(matches!(
        structural_err(&bad_policy),
        ReduceError::WrongArtifactKind { .. }
    ));
}

#[test]
fn an_empty_artifact_locator_fails_loudly() {
    let events = chained(vec![
        run_started(contract(vec![req("build")])),
        RunEventKind::WorktreeCreated {
            worktree: artifact(ArtifactKind::Worktree, "", b"wt"),
        },
        RunEventKind::RunSealed,
    ]);
    assert!(matches!(
        structural_err(&events),
        ReduceError::EmptyArtifactLocator { .. }
    ));
}

#[test]
fn an_event_after_seal_fails_loudly() {
    let run = run_id();
    let e0 = make_event(
        &run,
        0,
        &Digest256::genesis(),
        run_started(contract(vec![req("build")])),
    );
    let e1 = make_event(&run, 1, &e0.event_digest, RunEventKind::RunSealed);
    let e2 = make_event(&run, 2, &e1.event_digest, RunEventKind::AgentStarted);
    assert!(matches!(
        structural_err(&[e0, e1, e2]),
        ReduceError::EventAfterSeal { .. }
    ));
}

// ============================ structural: subject/obligation consistency =============

#[test]
fn a_waiver_for_a_different_environment_fails_loudly() {
    // Waived for windows, but the run environment is linux — a self-contradictory contract.
    let events = chained(vec![
        run_started(contract(vec![
            req("build"),
            waived("win-only", "windows", "windows-only check"),
        ])),
        RunEventKind::RunSealed,
    ]);
    assert!(matches!(
        structural_err(&events),
        ReduceError::WaiverEnvironmentMismatch { .. }
    ));
}

#[test]
fn an_agent_sandbox_requirement_without_an_agent_fails_loudly() {
    let events = chained(vec![
        run_started(contract_sandboxed(
            vec![req("build")],
            vec![agent_sandbox_requirement()], // Agent subject, but contract is NotUsed
        )),
        RunEventKind::RunSealed,
    ]);
    assert!(matches!(
        structural_err(&events),
        ReduceError::SandboxAgentSubjectWithoutAgent
    ));
}

#[test]
fn a_gate_sandbox_requirement_for_a_waived_gate_fails_loudly() {
    let events = chained(vec![
        run_started(contract_sandboxed(
            vec![waived("build", "linux", "waived here")],
            vec![gate_sandbox_requirement("build")],
        )),
        RunEventKind::RunSealed,
    ]);
    assert!(matches!(
        structural_err(&events),
        ReduceError::SandboxSubjectGateIneligible { .. }
    ));
}

#[test]
fn a_gate_sandbox_requirement_for_an_optional_gate_fails_loudly() {
    // Otherwise an optional gate becomes implicitly required via its sandbox obligation.
    let events = chained(vec![
        run_started(contract_sandboxed(
            vec![opt("build")],
            vec![gate_sandbox_requirement("build")],
        )),
        RunEventKind::RunSealed,
    ]);
    assert!(matches!(
        structural_err(&events),
        ReduceError::SandboxSubjectGateIneligible { .. }
    ));
}

#[test]
fn a_policy_protecting_an_unknown_gate_fails_loudly() {
    let events = chained(vec![
        run_started(contract_policy(
            vec![req("build")],
            vec![required_policy(
                policy_artifact(),
                vec![PolicyProtectedSubject::Gate { gate: gate("nope") }],
            )],
        )),
        RunEventKind::RunSealed,
    ]);
    assert!(matches!(
        structural_err(&events),
        ReduceError::PolicyProtectsUnknownGate { .. }
    ));
}

#[test]
fn a_policy_protecting_the_agent_without_an_agent_fails_loudly() {
    let events = chained(vec![
        run_started(contract_policy(
            vec![req("build")],
            vec![required_policy(
                policy_artifact(),
                vec![PolicyProtectedSubject::Agent],
            )],
        )),
        RunEventKind::RunSealed,
    ]);
    assert!(matches!(
        structural_err(&events),
        ReduceError::PolicyProtectsAgentWithoutAgent
    ));
}

#[test]
fn a_duplicate_protected_subject_fails_loudly() {
    let contract = RunContract {
        gate_obligations: vec![req("build")],
        policy_obligations: vec![PolicyObligation {
            policy: policy_artifact(),
            requirement: PolicyRequirement::Required,
            protects: vec![PolicyProtectedSubject::Agent, PolicyProtectedSubject::Agent],
        }],
        sandbox_requirements: Vec::new(),
        agent: AgentObligation::Required,
        runner_environment: "linux".to_owned(),
    };
    let events = chained(vec![run_started(contract), RunEventKind::RunSealed]);
    assert!(matches!(
        structural_err(&events),
        ReduceError::DuplicateProtectedSubject { .. }
    ));
}

#[test]
fn an_optional_policy_that_protects_a_subject_fails_loudly() {
    let contract = RunContract {
        gate_obligations: vec![req("build")],
        policy_obligations: vec![PolicyObligation {
            policy: policy_artifact(),
            requirement: PolicyRequirement::Optional,
            protects: vec![PolicyProtectedSubject::Agent],
        }],
        sandbox_requirements: Vec::new(),
        agent: AgentObligation::Required,
        runner_environment: "linux".to_owned(),
    };
    let events = chained(vec![run_started(contract), RunEventKind::RunSealed]);
    assert!(matches!(
        structural_err(&events),
        ReduceError::OptionalPolicyWithProtects { .. }
    ));
}

// ============================ optional policy / optional gate tables ================

fn optional_policy_stream(checks: Vec<RunEventKind>) -> Vec<RunEvent> {
    let mut kinds = vec![run_started(contract_policy(
        vec![req("build")],
        vec![optional_policy(policy_artifact())],
    ))];
    kinds.extend(checks);
    kinds.push(RunEventKind::GateStarted {
        gate: gate("build"),
    });
    kinds.push(RunEventKind::GateFinished {
        gate: gate("build"),
        outcome: GateOutcome::Pass,
        log: None,
    });
    kinds.push(RunEventKind::RunSealed);
    chained(kinds)
}

#[test]
fn an_optional_policy_never_blocks_for_any_outcome() {
    // Absent.
    assert_eq!(verdict_of(&optional_policy_stream(vec![])), Verdict::Pass);
    // Present, for every outcome.
    for outcome in [
        PolicyOutcome::Allowed,
        PolicyOutcome::Denied,
        PolicyOutcome::Error,
    ] {
        let events = optional_policy_stream(vec![RunEventKind::PolicyChecked {
            policy: policy_artifact(),
            outcome,
        }]);
        assert_eq!(
            verdict_of(&events),
            Verdict::Pass,
            "optional policy outcome {outcome:?} must be neutral"
        );
    }
}

#[test]
fn an_optional_gate_never_blocks_for_any_outcome() {
    // Absent (declared optional, never run).
    let absent = chained(vec![
        run_started(contract(vec![req("build"), opt("bench")])),
        RunEventKind::GateStarted {
            gate: gate("build"),
        },
        RunEventKind::GateFinished {
            gate: gate("build"),
            outcome: GateOutcome::Pass,
            log: None,
        },
        RunEventKind::RunSealed,
    ]);
    assert_eq!(verdict_of(&absent), Verdict::Pass);

    for outcome in [
        GateOutcome::Pass,
        GateOutcome::Fail,
        GateOutcome::Warn,
        GateOutcome::Blocked,
        GateOutcome::NotApplicable,
        GateOutcome::Error,
    ] {
        let events = chained(vec![
            run_started(contract(vec![req("build"), opt("bench")])),
            RunEventKind::GateStarted {
                gate: gate("build"),
            },
            RunEventKind::GateFinished {
                gate: gate("build"),
                outcome: GateOutcome::Pass,
                log: None,
            },
            RunEventKind::GateStarted {
                gate: gate("bench"),
            },
            RunEventKind::GateFinished {
                gate: gate("bench"),
                outcome,
                log: None,
            },
            RunEventKind::RunSealed,
        ]);
        assert_eq!(
            verdict_of(&events),
            Verdict::Pass,
            "optional gate outcome {outcome:?} must be neutral"
        );
    }
}

// ============================ cross-obligation: policy-blocked agent non-start =======

/// A contract with a required agent and a required policy that PROTECTS the agent.
fn agent_protected_by_policy() -> RunContract {
    RunContract {
        gate_obligations: vec![req("build")],
        policy_obligations: vec![PolicyObligation {
            policy: policy_artifact(),
            requirement: PolicyRequirement::Required,
            protects: vec![PolicyProtectedSubject::Agent],
        }],
        sandbox_requirements: Vec::new(),
        agent: AgentObligation::Required,
        runner_environment: "linux".to_owned(),
    }
}

#[test]
fn required_policy_denial_protecting_agent_blocks_instead_of_errors() {
    // The agent correctly did NOT start because its protecting policy denied it. A legitimate
    // policy denial must not be promoted to a harness Error just because the forbidden
    // execution did not occur.
    let events = chained(vec![
        run_started(agent_protected_by_policy()),
        RunEventKind::PolicyChecked {
            policy: policy_artifact(),
            outcome: PolicyOutcome::Denied,
        },
        RunEventKind::RunSealed,
    ]);
    assert_eq!(
        verdict_of(&events),
        Verdict::Blocked,
        "a policy-denied, correctly-unstarted agent is Blocked, not Error"
    );
}

#[test]
fn required_policy_absence_protecting_agent_blocks_instead_of_errors() {
    // The protecting policy was never checked, so the agent could not start. Blocked (the
    // unmet policy obligation), not Error for the resulting non-start.
    let events = chained(vec![
        run_started(agent_protected_by_policy()),
        RunEventKind::RunSealed,
    ]);
    assert_eq!(
        verdict_of(&events),
        Verdict::Blocked,
        "an absent protecting policy that prevents the agent start is Blocked, not Error"
    );
}

#[test]
fn allowed_protecting_policy_followed_by_missing_agent_is_error() {
    // Nothing blocked the agent — its protecting policy was Allowed — yet the obligated agent
    // never ran. That is a genuine harness Error.
    let events = chained(vec![
        run_started(agent_protected_by_policy()),
        RunEventKind::PolicyChecked {
            policy: policy_artifact(),
            outcome: PolicyOutcome::Allowed,
        },
        RunEventKind::RunSealed,
    ]);
    assert_eq!(
        verdict_of(&events),
        Verdict::Error,
        "an allowed agent that still never ran is a harness Error"
    );
}

#[test]
fn errored_protecting_policy_with_missing_agent_is_error() {
    // Derivable from the policy table (Error → Error), pinned explicitly for the matrix.
    let events = chained(vec![
        run_started(agent_protected_by_policy()),
        RunEventKind::PolicyChecked {
            policy: policy_artifact(),
            outcome: PolicyOutcome::Error,
        },
        RunEventKind::RunSealed,
    ]);
    assert_eq!(verdict_of(&events), Verdict::Error);
}
