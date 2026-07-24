//! RED transition-table tests for the pure reducer.
//!
//! Every test here asserts the TARGET semantics of `reduce`/`reduce_all`. Because this
//! commit ships the reducer as contract-only (`ReduceError::Unimplemented`), these tests
//! FAIL by construction — they are the executable specification a following commit turns
//! green. They must NEVER be satisfied by an unimplemented reducer returning a plausible
//! answer.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod common;

use common::*;
use o7_run::event::{AgentExit, ArtifactKind, GateOutcome, RunContract};
use o7_run::reduce::ReduceError;
use o7_run::{reduce_all, Digest256, RunEventKind, Verdict};

// ---- verdict reduction ----

#[test]
fn all_required_gates_pass_is_pass() {
    let events = single_gate_stream(contract(&["build"]), "build", GateOutcome::Pass);
    let state = reduce_all(&events).expect("well-formed stream reduces");
    assert_eq!(state.verdict, Some(Verdict::Pass));
}

#[test]
fn a_required_gate_that_did_not_execute_is_blocked_never_pass() {
    // The contract requires `lint`, but only `build` runs. `lint` never executed.
    let events = chained(vec![
        RunEventKind::RunStarted {
            contract: contract(&["build", "lint"]),
        },
        RunEventKind::GateStarted {
            gate: gate("build"),
        },
        RunEventKind::GateFinished {
            gate: gate("build"),
            outcome: GateOutcome::Pass,
            supported_in_env: true,
            log: None,
        },
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
fn a_required_gate_unsupported_in_this_environment_is_blocked() {
    // A required Windows-only gate finishes NotApplicable / unsupported on a Linux runner.
    let events = chained(vec![
        RunEventKind::RunStarted {
            contract: contract(&["win-only"]),
        },
        RunEventKind::GateStarted {
            gate: gate("win-only"),
        },
        RunEventKind::GateFinished {
            gate: gate("win-only"),
            outcome: GateOutcome::NotApplicable,
            supported_in_env: false,
            log: None,
        },
        RunEventKind::RunSealed,
    ]);
    let state = reduce_all(&events).expect("well-formed stream reduces");
    assert_eq!(state.verdict, Some(Verdict::Blocked));
}

#[test]
fn a_gate_declared_optional_in_env_may_be_skipped_and_still_pass() {
    // `win-only` is required, but explicitly declared optional-in-env BEFORE execution, so
    // its NotApplicable is allowed and visible — the run still passes on `build`.
    let contract = RunContract {
        required_gates: vec![gate("build"), gate("win-only")],
        optional_in_env: vec![gate("win-only")],
        requires_sandbox_evidence: false,
        runner_environment: "linux".to_owned(),
    };
    let events = chained(vec![
        RunEventKind::RunStarted { contract },
        RunEventKind::GateStarted {
            gate: gate("build"),
        },
        RunEventKind::GateFinished {
            gate: gate("build"),
            outcome: GateOutcome::Pass,
            supported_in_env: true,
            log: None,
        },
        RunEventKind::GateStarted {
            gate: gate("win-only"),
        },
        RunEventKind::GateFinished {
            gate: gate("win-only"),
            outcome: GateOutcome::NotApplicable,
            supported_in_env: false,
            log: None,
        },
        RunEventKind::RunSealed,
    ]);
    let state = reduce_all(&events).expect("well-formed stream reduces");
    assert_eq!(state.verdict, Some(Verdict::Pass));
}

#[test]
fn a_required_gate_returning_failure_is_fail() {
    let events = single_gate_stream(contract(&["build"]), "build", GateOutcome::Fail);
    let state = reduce_all(&events).expect("well-formed stream reduces");
    assert_eq!(state.verdict, Some(Verdict::Fail));
}

#[test]
fn a_gate_that_could_not_run_is_error_not_fail() {
    let events = single_gate_stream(contract(&["build"]), "build", GateOutcome::Error);
    let state = reduce_all(&events).expect("well-formed stream reduces");
    assert_eq!(
        state.verdict,
        Some(Verdict::Error),
        "a harness Error must be distinguishable from a domain Fail"
    );
}

#[test]
fn an_agent_that_failed_to_start_is_error() {
    let events = chained(vec![
        RunEventKind::RunStarted {
            contract: contract(&["build"]),
        },
        RunEventKind::AgentExited {
            exit: AgentExit::FailedToStart {
                reason: "no such executable".to_owned(),
            },
        },
        RunEventKind::RunSealed,
    ]);
    let state = reduce_all(&events).expect("well-formed stream reduces");
    assert_eq!(state.verdict, Some(Verdict::Error));
}

#[test]
fn a_required_not_applicable_without_optout_is_blocked() {
    let events = single_gate_stream(contract(&["build"]), "build", GateOutcome::NotApplicable);
    let state = reduce_all(&events).expect("well-formed stream reduces");
    assert_eq!(state.verdict, Some(Verdict::Blocked));
}

#[test]
fn a_run_requiring_sandbox_evidence_without_it_is_blocked() {
    let contract = RunContract {
        required_gates: vec![gate("build")],
        optional_in_env: Vec::new(),
        requires_sandbox_evidence: true, // required, but no SandboxEvidenceCaptured event
        runner_environment: "linux".to_owned(),
    };
    let events = chained(vec![
        RunEventKind::RunStarted { contract },
        RunEventKind::GateStarted {
            gate: gate("build"),
        },
        RunEventKind::GateFinished {
            gate: gate("build"),
            outcome: GateOutcome::Pass,
            supported_in_env: true,
            log: None,
        },
        RunEventKind::RunSealed,
    ]);
    let state = reduce_all(&events).expect("well-formed stream reduces");
    assert_eq!(state.verdict, Some(Verdict::Blocked));
}

#[test]
fn a_run_requiring_sandbox_evidence_with_valid_evidence_can_pass() {
    let contract = RunContract {
        required_gates: vec![gate("build")],
        optional_in_env: Vec::new(),
        requires_sandbox_evidence: true,
        runner_environment: "linux".to_owned(),
    };
    let report = artifact(
        ArtifactKind::SandboxReport,
        "sandbox.json",
        b"{\"confined\":true}",
    );
    let events = chained(vec![
        RunEventKind::RunStarted { contract },
        RunEventKind::SandboxEvidenceCaptured {
            report,
            satisfies_policy: true,
        },
        RunEventKind::GateStarted {
            gate: gate("build"),
        },
        RunEventKind::GateFinished {
            gate: gate("build"),
            outcome: GateOutcome::Pass,
            supported_in_env: true,
            log: None,
        },
        RunEventKind::RunSealed,
    ]);
    let state = reduce_all(&events).expect("well-formed stream reduces");
    assert_eq!(state.verdict, Some(Verdict::Pass));
}

#[test]
fn precedence_is_error_over_fail_over_blocked() {
    // One required gate errors, one fails, one never runs. ERROR must win.
    let events = chained(vec![
        RunEventKind::RunStarted {
            contract: contract(&["a", "b", "c"]),
        },
        RunEventKind::GateStarted { gate: gate("a") },
        RunEventKind::GateFinished {
            gate: gate("a"),
            outcome: GateOutcome::Error,
            supported_in_env: true,
            log: None,
        },
        RunEventKind::GateStarted { gate: gate("b") },
        RunEventKind::GateFinished {
            gate: gate("b"),
            outcome: GateOutcome::Fail,
            supported_in_env: true,
            log: None,
        },
        // gate "c" never runs → would be Blocked on its own
        RunEventKind::RunSealed,
    ]);
    let state = reduce_all(&events).expect("well-formed stream reduces");
    assert_eq!(state.verdict, Some(Verdict::Error));
}

#[test]
fn fail_outranks_blocked() {
    // One required gate fails, one never runs. FAIL outranks the incompleteness of BLOCKED.
    let events = chained(vec![
        RunEventKind::RunStarted {
            contract: contract(&["a", "b"]),
        },
        RunEventKind::GateStarted { gate: gate("a") },
        RunEventKind::GateFinished {
            gate: gate("a"),
            outcome: GateOutcome::Fail,
            supported_in_env: true,
            log: None,
        },
        RunEventKind::RunSealed,
    ]);
    let state = reduce_all(&events).expect("well-formed stream reduces");
    assert_eq!(state.verdict, Some(Verdict::Fail));
}

#[test]
fn a_warning_only_required_gate_still_passes() {
    let events = single_gate_stream(contract(&["build"]), "build", GateOutcome::Warn);
    let state = reduce_all(&events).expect("well-formed stream reduces");
    assert_eq!(state.verdict, Some(Verdict::Pass));
}

// ---- byte-stable replay of state ----

#[test]
fn reducing_the_same_stream_twice_is_byte_stable() {
    let events = single_gate_stream(contract(&["build"]), "build", GateOutcome::Pass);
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
    let events = chained(vec![
        RunEventKind::AgentStarted, // not RunStarted first
        RunEventKind::RunSealed,
    ]);
    let err = reduce_all(&events).expect_err("a stream not beginning with RunStarted must fail");
    assert!(
        matches!(err, ReduceError::MissingRunStarted { .. }),
        "got {err:?}"
    );
}

#[test]
fn a_duplicate_run_started_fails_loudly() {
    let events = chained(vec![
        RunEventKind::RunStarted {
            contract: contract(&["build"]),
        },
        RunEventKind::RunStarted {
            contract: contract(&["build"]),
        },
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
    // Hand-build: seq 0 (RunStarted) then seq 5 (should be 1).
    let run = run_id();
    let e0 = make_event(
        &run,
        0,
        &Digest256::genesis(),
        RunEventKind::RunStarted {
            contract: contract(&["build"]),
        },
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
        RunEventKind::RunStarted {
            contract: contract(&["build"]),
        },
    );
    let e1 = make_event(&run, 1, &e0.event_digest, RunEventKind::AgentStarted);
    // Re-use sequence 1, correctly chained to e1 so the ONLY defect is the duplicate seq.
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
        RunEventKind::RunStarted {
            contract: contract(&["build"]),
        },
    );
    // e1's previous-digest points at genesis instead of e0 — the chain is severed.
    let e1 = make_event(&run, 1, &Digest256::genesis(), RunEventKind::RunSealed);
    let err = reduce_all(&[e0, e1]).expect_err("a severed chain link must fail");
    assert!(
        matches!(err, ReduceError::BrokenChain { .. }),
        "got {err:?}"
    );
}

#[test]
fn a_gate_finished_without_started_fails_loudly() {
    let events = chained(vec![
        RunEventKind::RunStarted {
            contract: contract(&["build"]),
        },
        RunEventKind::GateFinished {
            gate: gate("build"),
            outcome: GateOutcome::Pass,
            supported_in_env: true,
            log: None,
        },
        RunEventKind::RunSealed,
    ]);
    let err = reduce_all(&events).expect_err("GateFinished without GateStarted must fail");
    assert!(
        matches!(err, ReduceError::ImpossibleTransition { .. }),
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
        RunEventKind::RunStarted {
            contract: contract(&["build"]),
        },
    );
    let e1 = make_event(&run, 1, &e0.event_digest, RunEventKind::RunSealed);
    let e2 = make_event(&run, 2, &e1.event_digest, RunEventKind::AgentStarted);
    let err = reduce_all(&[e0, e1, e2]).expect_err("an event after seal must fail");
    assert!(
        matches!(err, ReduceError::EventAfterSeal { .. }),
        "got {err:?}"
    );
}
