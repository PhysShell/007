//! RED replay-acceptance tests: independent verdict re-derivation plus tamper detection.
//!
//! These assert the TARGET behavior of `replay`/`replay_verify`. This contract-only commit
//! ships replay as `ReplayError::Unimplemented`, so they FAIL by construction — a legacy or
//! tampered run must NEVER be reported as verified, and an unimplemented replay must never
//! be mistaken for a passing one. A following commit implements verification and turns them
//! green.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod common;

use common::*;
use o7_run::event::{ArtifactKind, GateOutcome};
use o7_run::replay::{replay, replay_verify, ReplayError};
use o7_run::{Digest256, RunEventKind, Verdict};

/// A well-formed passing stream that references a diff and a gate log, plus a resolver
/// seeded with matching content. The baseline for the tamper tests.
fn passing_run_with_artifacts() -> (Vec<o7_run::RunEvent>, MapResolver) {
    let diff_bytes = b"--- a/x\n+++ b/x\n@@ -1 +1 @@\n-old\n+new\n";
    let log_bytes = b"build ok\n";
    let diff = artifact(ArtifactKind::Diff, "diff.patch", diff_bytes);
    let log = artifact(ArtifactKind::GateLog, "gate/build.log", log_bytes);

    let events = chained(vec![
        RunEventKind::RunStarted {
            contract: contract(&["build"]),
        },
        RunEventKind::PatchCaptured { patch: diff },
        RunEventKind::GateStarted {
            gate: gate("build"),
        },
        RunEventKind::GateFinished {
            gate: gate("build"),
            outcome: GateOutcome::Pass,
            supported_in_env: true,
            log: Some(log),
        },
        RunEventKind::RunSealed,
    ]);

    let mut resolver = MapResolver::new();
    resolver.insert("diff.patch", diff_bytes);
    resolver.insert("gate/build.log", log_bytes);
    (events, resolver)
}

#[test]
fn a_clean_run_replays_to_its_verdict_and_verifies_artifacts() {
    let (events, resolver) = passing_run_with_artifacts();
    let report = replay(&events, &resolver).expect("a clean run replays");
    assert_eq!(report.verdict, Verdict::Pass);
    assert_eq!(report.events_verified, events.len() as u64);
    assert_eq!(
        report.artifacts_verified, 2,
        "the diff and the gate log must both be digest-verified"
    );
}

#[test]
fn replay_verify_agrees_with_a_correct_stored_verdict() {
    let (events, resolver) = passing_run_with_artifacts();
    let report = replay_verify(&events, &resolver, Verdict::Pass).expect("stored verdict matches");
    assert_eq!(report.verdict, Verdict::Pass);
}

#[test]
fn replay_verify_rejects_a_wrong_stored_verdict() {
    let (events, resolver) = passing_run_with_artifacts();
    let err = replay_verify(&events, &resolver, Verdict::Fail)
        .expect_err("a stored verdict that disagrees with the recomputation must be rejected");
    assert!(
        matches!(err, ReplayError::VerdictMismatch { .. }),
        "got {err:?}"
    );
}

#[test]
fn a_diff_changed_after_sealing_fails_replay() {
    let (events, mut resolver) = passing_run_with_artifacts();
    // The stored diff bytes are edited after sealing; the event's digest no longer matches.
    resolver.insert("diff.patch", b"totally different patch bytes");
    let err = replay(&events, &resolver).expect_err("an altered diff must fail replay");
    assert!(
        matches!(err, ReplayError::ArtifactDigestMismatch { .. }),
        "got {err:?}"
    );
}

#[test]
fn a_gate_log_changed_after_sealing_fails_replay() {
    let (events, mut resolver) = passing_run_with_artifacts();
    resolver.insert("gate/build.log", b"build FAILED (forged)\n");
    let err = replay(&events, &resolver).expect_err("an altered gate log must fail replay");
    assert!(
        matches!(err, ReplayError::ArtifactDigestMismatch { .. }),
        "got {err:?}"
    );
}

#[test]
fn a_missing_artifact_fails_replay() {
    let (events, _seeded) = passing_run_with_artifacts();
    let empty = MapResolver::new(); // resolves nothing
    let err = replay(&events, &empty).expect_err("an unresolved artifact must fail replay");
    assert!(
        matches!(err, ReplayError::ArtifactUnresolved(_)),
        "got {err:?}"
    );
}

#[test]
fn deleting_an_event_breaks_the_chain() {
    let (mut events, resolver) = passing_run_with_artifacts();
    events.remove(2); // drop the GateStarted; successors' previous-digest links dangle
    let err = replay(&events, &resolver).expect_err("a deleted event must break the chain");
    assert!(
        matches!(
            err,
            ReplayError::ChainBroken { .. } | ReplayError::Reduce(_)
        ),
        "got {err:?}"
    );
}

#[test]
fn reordering_events_breaks_the_chain() {
    let (mut events, resolver) = passing_run_with_artifacts();
    events.swap(1, 2); // swap PatchCaptured and GateStarted
    let err = replay(&events, &resolver).expect_err("reordered events must break the chain");
    assert!(
        matches!(
            err,
            ReplayError::ChainBroken { .. } | ReplayError::Reduce(_)
        ),
        "got {err:?}"
    );
}

#[test]
fn inserting_a_foreign_event_breaks_the_chain() {
    let (mut events, resolver) = passing_run_with_artifacts();
    // Splice in a correctly-self-consistent but unlinked event at position 2.
    let foreign = make_event(
        &run_id(),
        2,
        &Digest256::genesis(),
        RunEventKind::AgentStarted,
    );
    events.insert(2, foreign);
    let err = replay(&events, &resolver).expect_err("an inserted event must break the chain");
    assert!(
        matches!(
            err,
            ReplayError::ChainBroken { .. } | ReplayError::Reduce(_)
        ),
        "got {err:?}"
    );
}

#[test]
fn tampering_an_event_payload_in_place_fails_replay() {
    let (mut events, resolver) = passing_run_with_artifacts();
    // Flip the gate outcome to Fail WITHOUT recomputing the stored event_digest.
    events[3].kind = RunEventKind::GateFinished {
        gate: gate("build"),
        outcome: GateOutcome::Fail,
        supported_in_env: true,
        log: None,
    };
    let err = replay(&events, &resolver).expect_err("an in-place payload edit must fail replay");
    assert!(
        matches!(
            err,
            ReplayError::EventDigestMismatch { .. } | ReplayError::Reduce(_)
        ),
        "got {err:?}"
    );
}

#[test]
fn an_unsealed_run_has_no_verdict_to_verify() {
    // A stream with no RunSealed: replay must refuse rather than invent a verdict.
    let events = chained(vec![
        RunEventKind::RunStarted {
            contract: contract(&["build"]),
        },
        RunEventKind::GateStarted {
            gate: gate("build"),
        },
    ]);
    let resolver = MapResolver::new();
    let err = replay(&events, &resolver).expect_err("an unsealed run has no fixed verdict");
    assert!(matches!(err, ReplayError::NotSealed), "got {err:?}");
}

#[test]
fn a_legacy_run_without_events_is_non_replayable_never_verified() {
    let resolver = MapResolver::new();
    let err = replay(&[], &resolver)
        .expect_err("a run with no canonical events cannot be replay-verified");
    assert!(
        matches!(err, ReplayError::LegacyNonReplayable),
        "a legacy run must be explicitly non-replayable, not silently verified: got {err:?}"
    );
}
