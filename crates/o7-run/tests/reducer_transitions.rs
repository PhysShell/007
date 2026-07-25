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
    AgentOutcome, ArtifactKind, ExecutionSubject, GateOutcome, PolicyOutcome,
    SandboxEvidenceOutcome, SandboxRequirement,
};
use o7_run::reduce::ReduceError;
use o7_run::{reduce_all, Digest256, RunEventKind, RunId, Verdict};

fn policy_ref() -> o7_run::ArtifactRef {
    artifact(ArtifactKind::Policy, "policy.json", b"{\"net\":\"deny\"}")
}

fn sandbox_policy_digest() -> Digest256 {
    Digest256::of_bytes(b"agent-confinement-policy-v1")
}

// ---- verdict reduction ----

#[test]
fn all_required_gates_pass_is_pass() {
    let events = single_required_gate_stream("build", GateOutcome::Pass);
    let state = reduce_all(&events).expect("well-formed stream reduces");
    assert_eq!(state.verdict, Some(Verdict::Pass));
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
        // `lint` never runs.
        RunEventKind::RunSealed,
    ]);
    let state = reduce_all(&events).expect("well-formed stream reduces");
    assert_eq!(
        state.verdict,
        Some(Verdict::Blocked),
        "a required-but-unexecuted gate must never yield PASS"
    );
}

#[test]
fn a_required_applicable_gate_reported_not_applicable_is_blocked() {
    // Not waived up front, so a run-time NotApplicable cannot excuse it.
    let events = single_required_gate_stream("win-only", GateOutcome::NotApplicable);
    let state = reduce_all(&events).expect("well-formed stream reduces");
    assert_eq!(state.verdict, Some(Verdict::Blocked));
}

#[test]
fn a_gate_waived_for_this_environment_may_be_skipped_and_still_pass() {
    let events = chained(vec![
        run_started(contract(vec![
            req("build"),
            waived("win-only", "linux", "windows-only check, not run on linux"),
        ])),
        RunEventKind::GateStarted {
            gate: gate("build"),
        },
        RunEventKind::GateFinished {
            gate: gate("build"),
            outcome: GateOutcome::Pass,
            log: None,
        },
        // `win-only` is waived for linux and legitimately never runs.
        RunEventKind::RunSealed,
    ]);
    let state = reduce_all(&events).expect("well-formed stream reduces");
    assert_eq!(state.verdict, Some(Verdict::Pass));
}

#[test]
fn a_required_gate_returning_failure_is_fail() {
    let events = single_required_gate_stream("build", GateOutcome::Fail);
    let state = reduce_all(&events).expect("well-formed stream reduces");
    assert_eq!(state.verdict, Some(Verdict::Fail));
}

#[test]
fn a_gate_that_could_not_run_is_error_not_fail() {
    let events = single_required_gate_stream("build", GateOutcome::Error);
    let state = reduce_all(&events).expect("well-formed stream reduces");
    assert_eq!(
        state.verdict,
        Some(Verdict::Error),
        "a harness Error must be distinguishable from a domain Fail"
    );
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
    let state = reduce_all(&events).expect("well-formed stream reduces");
    assert_eq!(state.verdict, Some(Verdict::Pass));
}

#[test]
fn a_warning_only_required_gate_still_passes() {
    let events = single_required_gate_stream("build", GateOutcome::Warn);
    let state = reduce_all(&events).expect("well-formed stream reduces");
    assert_eq!(state.verdict, Some(Verdict::Pass));
}

// ---- agent terminal outcomes ----

#[test]
fn an_agent_that_failed_to_start_is_error() {
    let events = chained(vec![
        run_started(contract(vec![req("build")])),
        RunEventKind::AgentExited {
            outcome: AgentOutcome::FailedToStart {
                reason: "no such executable".to_owned(),
            },
        },
        RunEventKind::RunSealed,
    ]);
    let state = reduce_all(&events).expect("well-formed stream reduces");
    assert_eq!(state.verdict, Some(Verdict::Error));
}

#[test]
fn a_cleanup_failure_is_error_even_when_every_gate_passes() {
    // The leader exited 0 and the gate passed, but the owned set was not proven gone.
    let events = chained(vec![
        run_started(contract(vec![req("build")])),
        RunEventKind::AgentStarted,
        RunEventKind::GateStarted {
            gate: gate("build"),
        },
        RunEventKind::GateFinished {
            gate: gate("build"),
            outcome: GateOutcome::Pass,
            log: None,
        },
        RunEventKind::AgentExited {
            outcome: AgentOutcome::CleanupFailure {
                reason: "process group not proven gone".to_owned(),
            },
        },
        RunEventKind::RunSealed,
    ]);
    let state = reduce_all(&events).expect("well-formed stream reduces");
    assert_eq!(
        state.verdict,
        Some(Verdict::Error),
        "an unproven-cleanup run must be Error even with a clean exit and a passing gate"
    );
}

// ---- policy ----

#[test]
fn a_policy_denial_is_blocked() {
    let events = chained(vec![
        run_started(contract(vec![req("build")])),
        RunEventKind::PolicyChecked {
            policy: policy_ref(),
            outcome: PolicyOutcome::Denied,
        },
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
    let state = reduce_all(&events).expect("well-formed stream reduces");
    assert_eq!(state.verdict, Some(Verdict::Blocked));
}

#[test]
fn a_policy_engine_error_is_error() {
    let events = chained(vec![
        run_started(contract(vec![req("build")])),
        RunEventKind::PolicyChecked {
            policy: policy_ref(),
            outcome: PolicyOutcome::Error,
        },
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
    let state = reduce_all(&events).expect("well-formed stream reduces");
    assert_eq!(
        state.verdict,
        Some(Verdict::Error),
        "a broken policy engine is an Error, not a denial"
    );
}

// ---- sandbox evidence, bound to subject + policy ----

fn agent_sandbox_requirement() -> SandboxRequirement {
    SandboxRequirement {
        subject: ExecutionSubject::Agent,
        policy_digest: sandbox_policy_digest(),
    }
}

fn passing_gate_tail() -> Vec<RunEventKind> {
    vec![
        RunEventKind::GateStarted {
            gate: gate("build"),
        },
        RunEventKind::GateFinished {
            gate: gate("build"),
            outcome: GateOutcome::Pass,
            log: None,
        },
        RunEventKind::RunSealed,
    ]
}

#[test]
fn a_required_sandbox_without_evidence_is_blocked() {
    let mut kinds = vec![run_started(contract_sandboxed(
        vec![req("build")],
        vec![agent_sandbox_requirement()],
    ))];
    kinds.extend(passing_gate_tail());
    let state = reduce_all(&chained(kinds)).expect("well-formed stream reduces");
    assert_eq!(state.verdict, Some(Verdict::Blocked));
}

#[test]
fn a_required_sandbox_with_satisfying_evidence_can_pass() {
    let mut kinds = vec![
        run_started(contract_sandboxed(
            vec![req("build")],
            vec![agent_sandbox_requirement()],
        )),
        RunEventKind::SandboxEvidenceCaptured {
            subject: ExecutionSubject::Agent,
            policy_digest: sandbox_policy_digest(),
            report: artifact(
                ArtifactKind::SandboxReport,
                "sandbox.json",
                b"{\"ok\":true}",
            ),
            outcome: SandboxEvidenceOutcome::Satisfied,
        },
    ];
    kinds.extend(passing_gate_tail());
    let state = reduce_all(&chained(kinds)).expect("well-formed stream reduces");
    assert_eq!(state.verdict, Some(Verdict::Pass));
}

#[test]
fn a_violated_sandbox_is_blocked() {
    let mut kinds = vec![
        run_started(contract_sandboxed(
            vec![req("build")],
            vec![agent_sandbox_requirement()],
        )),
        RunEventKind::SandboxEvidenceCaptured {
            subject: ExecutionSubject::Agent,
            policy_digest: sandbox_policy_digest(),
            report: artifact(
                ArtifactKind::SandboxReport,
                "sandbox.json",
                b"{\"ok\":false}",
            ),
            outcome: SandboxEvidenceOutcome::Violated,
        },
    ];
    kinds.extend(passing_gate_tail());
    let state = reduce_all(&chained(kinds)).expect("well-formed stream reduces");
    assert_eq!(state.verdict, Some(Verdict::Blocked));
}

#[test]
fn an_errored_sandbox_report_is_error() {
    let mut kinds = vec![
        run_started(contract_sandboxed(
            vec![req("build")],
            vec![agent_sandbox_requirement()],
        )),
        RunEventKind::SandboxEvidenceCaptured {
            subject: ExecutionSubject::Agent,
            policy_digest: sandbox_policy_digest(),
            report: artifact(ArtifactKind::SandboxReport, "sandbox.json", b"garbage"),
            outcome: SandboxEvidenceOutcome::Error,
        },
    ];
    kinds.extend(passing_gate_tail());
    let state = reduce_all(&chained(kinds)).expect("well-formed stream reduces");
    assert_eq!(state.verdict, Some(Verdict::Error));
}

#[test]
fn evidence_for_the_wrong_policy_does_not_satisfy_the_requirement() {
    // Satisfying evidence, but under a DIFFERENT policy digest than required → still Blocked.
    let mut kinds = vec![
        run_started(contract_sandboxed(
            vec![req("build")],
            vec![agent_sandbox_requirement()],
        )),
        RunEventKind::SandboxEvidenceCaptured {
            subject: ExecutionSubject::Agent,
            policy_digest: Digest256::of_bytes(b"some-other-policy"),
            report: artifact(
                ArtifactKind::SandboxReport,
                "sandbox.json",
                b"{\"ok\":true}",
            ),
            outcome: SandboxEvidenceOutcome::Satisfied,
        },
    ];
    kinds.extend(passing_gate_tail());
    let state = reduce_all(&chained(kinds)).expect("well-formed stream reduces");
    assert_eq!(
        state.verdict,
        Some(Verdict::Blocked),
        "evidence bound to a different policy must not satisfy the requirement"
    );
}

// ---- precedence ----

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
        // `c` never runs → would be Blocked on its own.
        RunEventKind::RunSealed,
    ]);
    let state = reduce_all(&events).expect("well-formed stream reduces");
    assert_eq!(state.verdict, Some(Verdict::Error));
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
        // `b` never runs.
        RunEventKind::RunSealed,
    ]);
    let state = reduce_all(&events).expect("well-formed stream reduces");
    assert_eq!(state.verdict, Some(Verdict::Fail));
}

#[test]
fn an_unfinished_required_gate_at_seal_is_error() {
    // GateStarted but never GateFinished before sealing — the harness could not complete it.
    let events = chained(vec![
        run_started(contract(vec![req("build")])),
        RunEventKind::GateStarted {
            gate: gate("build"),
        },
        RunEventKind::RunSealed,
    ]);
    let state = reduce_all(&events).expect("well-formed stream reduces");
    assert_eq!(state.verdict, Some(Verdict::Error));
}

// ---- byte-stable replay of state ----

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

// ---- structural violations fail loudly (never a verdict) ----

#[test]
fn an_event_before_run_started_fails_loudly() {
    let events = chained(vec![RunEventKind::AgentStarted, RunEventKind::RunSealed]);
    let err = reduce_all(&events).expect_err("a stream not beginning with RunStarted must fail");
    assert!(
        matches!(err, ReduceError::MissingRunStarted { .. }),
        "got {err:?}"
    );
}

#[test]
fn a_duplicate_run_started_fails_loudly() {
    let events = chained(vec![
        run_started(contract(vec![req("build")])),
        run_started(contract(vec![req("build")])),
        RunEventKind::RunSealed,
    ]);
    let err = reduce_all(&events).expect_err("a second RunStarted must fail");
    assert!(
        matches!(err, ReduceError::DuplicateStart { .. }),
        "got {err:?}"
    );
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
    let err = reduce_all(&[e0, e1]).expect_err("a sequence jump must fail");
    assert!(matches!(err, ReduceError::OutOfOrder { .. }), "got {err:?}");
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
    let e1 = make_event(&run, 1, &e0.event_digest, RunEventKind::AgentStarted);
    let e1_dup = make_event(&run, 1, &e1.event_digest, RunEventKind::RunSealed);
    let err = reduce_all(&[e0, e1, e1_dup]).expect_err("a repeated sequence must fail");
    assert!(
        matches!(err, ReduceError::DuplicateSequence { .. }),
        "got {err:?}"
    );
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
    let err = reduce_all(&[e0, e1]).expect_err("a severed chain link must fail");
    assert!(
        matches!(err, ReduceError::BrokenChain { .. }),
        "got {err:?}"
    );
}

#[test]
fn a_run_id_mismatch_fails_loudly() {
    let run_a = RunId::from_raw("run-A");
    let run_b = RunId::from_raw("run-B");
    let e0 = make_event(
        &run_a,
        0,
        &Digest256::genesis(),
        run_started(contract(vec![req("build")])),
    );
    let e1 = make_event(&run_b, 1, &e0.event_digest, RunEventKind::RunSealed);
    let err = reduce_all(&[e0, e1]).expect_err("a differing run id must fail");
    assert!(
        matches!(err, ReduceError::RunIdMismatch { .. }),
        "got {err:?}"
    );
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
    let err = reduce_all(&[e0, e1]).expect_err("a repeated event id must fail");
    assert!(
        matches!(err, ReduceError::DuplicateEventId { .. }),
        "got {err:?}"
    );
}

#[test]
fn an_unsupported_schema_fails_loudly() {
    let run = run_id();
    let e0 = event_with(
        &run,
        "ev-0",
        0, // schema 0 — not the supported version
        0,
        &Digest256::genesis(),
        run_started(contract(vec![req("build")])),
    );
    let err = reduce_all(&[e0]).expect_err("an unsupported schema must fail");
    assert!(
        matches!(err, ReduceError::UnsupportedSchema { .. }),
        "got {err:?}"
    );
}

#[test]
fn a_duplicate_gate_in_the_contract_fails_loudly() {
    let events = chained(vec![
        run_started(contract(vec![req("build"), req("build")])),
        RunEventKind::RunSealed,
    ]);
    let err = reduce_all(&events).expect_err("a duplicate gate obligation must fail");
    assert!(
        matches!(err, ReduceError::DuplicateGateInContract { .. }),
        "got {err:?}"
    );
}

#[test]
fn an_unknown_gate_during_the_run_fails_loudly() {
    let events = chained(vec![
        run_started(contract(vec![req("build")])),
        RunEventKind::GateStarted {
            gate: gate("surprise"),
        }, // not a declared obligation
        RunEventKind::RunSealed,
    ]);
    let err = reduce_all(&events).expect_err("a gate not in the contract must fail");
    assert!(
        matches!(err, ReduceError::UnknownGate { .. }),
        "got {err:?}"
    );
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
    let err = reduce_all(&events).expect_err("a second GateStarted must fail");
    assert!(
        matches!(err, ReduceError::DuplicateGateStarted { .. }),
        "got {err:?}"
    );
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
    let err = reduce_all(&events).expect_err("GateFinished without GateStarted must fail");
    assert!(
        matches!(err, ReduceError::GateFinishedWithoutStart { .. }),
        "got {err:?}"
    );
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
    let err = reduce_all(&events).expect_err("a second GateFinished must fail");
    assert!(
        matches!(err, ReduceError::DuplicateGateFinished { .. }),
        "got {err:?}"
    );
}

#[test]
fn an_agent_exit_before_start_fails_loudly() {
    // A normal exit with no prior AgentStarted (FailedToStart is the only legitimate
    // no-start exit and is tested separately).
    let events = chained(vec![
        run_started(contract(vec![req("build")])),
        RunEventKind::AgentExited {
            outcome: agent_ok(),
        },
        RunEventKind::RunSealed,
    ]);
    let err = reduce_all(&events).expect_err("AgentExited before AgentStarted must fail");
    assert!(
        matches!(err, ReduceError::AgentExitBeforeStart { .. }),
        "got {err:?}"
    );
}

#[test]
fn a_duplicate_agent_start_fails_loudly() {
    let events = chained(vec![
        run_started(contract(vec![req("build")])),
        RunEventKind::AgentStarted,
        RunEventKind::AgentStarted,
        RunEventKind::RunSealed,
    ]);
    let err = reduce_all(&events).expect_err("a second AgentStarted must fail");
    assert!(
        matches!(err, ReduceError::DuplicateAgentStart { .. }),
        "got {err:?}"
    );
}

#[test]
fn a_duplicate_agent_exit_fails_loudly() {
    let events = chained(vec![
        run_started(contract(vec![req("build")])),
        RunEventKind::AgentStarted,
        RunEventKind::AgentExited {
            outcome: agent_ok(),
        },
        RunEventKind::AgentExited {
            outcome: agent_ok(),
        },
        RunEventKind::RunSealed,
    ]);
    let err = reduce_all(&events).expect_err("a second AgentExited must fail");
    assert!(
        matches!(err, ReduceError::DuplicateAgentExit { .. }),
        "got {err:?}"
    );
}

#[test]
fn a_wrong_artifact_kind_fails_loudly() {
    // PatchCaptured references a Task-kind artifact instead of a Diff.
    let events = chained(vec![
        run_started(contract(vec![req("build")])),
        RunEventKind::PatchCaptured {
            patch: artifact(ArtifactKind::Task, "diff.patch", b"not really a diff"),
        },
        RunEventKind::RunSealed,
    ]);
    let err = reduce_all(&events).expect_err("a mis-kinded artifact must fail");
    assert!(
        matches!(err, ReduceError::WrongArtifactKind { .. }),
        "got {err:?}"
    );
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
    let err = reduce_all(&[e0, e1, e2]).expect_err("an event after seal must fail");
    assert!(
        matches!(err, ReduceError::EventAfterSeal { .. }),
        "got {err:?}"
    );
}
