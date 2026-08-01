//! Replay-acceptance specification: independent verdict re-derivation plus tamper detection.
//!
//! These pin the behavior of `replay`/`replay_verify`. The load-bearing invariant: a legacy
//! run (no canonical events) or a tampered run (a broken chain, an in-place edit, or an
//! altered artifact) must NEVER be reported as verified — replay fails loudly instead.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod common;

use common::*;
use o7_run::event::{ArtifactKind, GateOutcome};
use o7_run::replay::{classify_record, replay, replay_verify, RecordVerdict, ReplayError};
use o7_run::{Digest256, RunEventKind, Verdict};

/// A well-formed passing stream referencing a task, a diff, and a gate log, plus a resolver
/// seeded with matching content. The baseline for the tamper tests.
fn passing_run_with_artifacts() -> (Vec<o7_run::RunEvent>, MapResolver) {
    let diff_bytes = b"--- a/x\n+++ b/x\n@@ -1 +1 @@\n-old\n+new\n";
    let log_bytes = b"build ok\n";
    let diff = artifact(ArtifactKind::Diff, "diff.patch", diff_bytes);
    let log = artifact(ArtifactKind::GateLog, "gate/build.log", log_bytes);

    let events = chained(vec![
        run_started(contract(vec![req("build")])),
        RunEventKind::PatchCaptured { patch: diff },
        RunEventKind::GateStarted {
            gate: gate("build"),
        },
        RunEventKind::GateFinished {
            gate: gate("build"),
            outcome: GateOutcome::Pass,
            log: Some(log),
        },
        RunEventKind::RunSealed,
    ]);

    let mut resolver = MapResolver::new();
    resolver.insert("task.json", TASK_BYTES);
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
        report.artifacts_verified, 3,
        "the task, the diff, and the gate log must all be digest-verified"
    );
}

#[test]
fn a_clean_replay_reports_anchor_digests() {
    let (events, resolver) = passing_run_with_artifacts();
    let report = replay(&events, &resolver).expect("a clean run replays");
    // The final event digest anchors the chain; the normalized-state digest anchors the
    // reduced state. Both must be present and stable for external anchoring.
    assert_eq!(
        report.final_event_digest,
        events.last().unwrap().event_digest
    );
    let again = replay(&events, &resolver).expect("replays again");
    assert_eq!(
        report.normalized_state_digest, again.normalized_state_digest,
        "the normalized-state digest must be stable across replays"
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
    let empty = MapResolver::new();
    let err = replay(&events, &empty).expect_err("an unresolved artifact must fail replay");
    assert!(
        matches!(err, ReplayError::ArtifactUnresolved(_)),
        "got {err:?}"
    );
}

#[test]
fn deleting_an_event_breaks_the_chain() {
    let (mut events, resolver) = passing_run_with_artifacts();
    events.remove(2); // drop GateStarted; successors' previous-digest links dangle
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
    events.swap(1, 2);
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
    // Flip the gate outcome WITHOUT recomputing the stored event_digest.
    events[3].kind = RunEventKind::GateFinished {
        gate: gate("build"),
        outcome: GateOutcome::Fail,
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
    let events = chained(vec![
        run_started(contract(vec![req("build")])),
        RunEventKind::GateStarted {
            gate: gate("build"),
        },
    ]);
    let mut resolver = MapResolver::new();
    resolver.insert("task.json", TASK_BYTES);
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

// Q-Deck R1 fourth corrective round: `classify_record` is the shared
// primitive `o7d`'s pre-redrive decision is built on (as well as the root
// `o7` binary's `catch_up`) — these pin its four outcomes against the SAME
// fixtures the `replay`/`verify_prefix` acceptance tests above already use,
// so its behavior can never silently drift from what a sealed replay itself
// would report.

#[test]
fn classify_record_reports_no_canonical_stream_for_an_empty_slice() {
    let resolver = MapResolver::new();
    assert_eq!(
        classify_record(&[], &resolver),
        RecordVerdict::NoCanonicalStream
    );
}

#[test]
fn classify_record_reports_valid_unsealed_for_a_genuine_in_progress_prefix() {
    let events = chained(vec![
        run_started(contract(vec![req("build")])),
        RunEventKind::GateStarted {
            gate: gate("build"),
        },
    ]);
    let mut resolver = MapResolver::new();
    resolver.insert("task.json", TASK_BYTES);
    assert_eq!(
        classify_record(&events, &resolver),
        RecordVerdict::ValidUnsealed
    );
}

#[test]
fn classify_record_reports_valid_sealed_with_the_same_evidence_replay_would() {
    let (events, resolver) = passing_run_with_artifacts();
    let RecordVerdict::ValidSealed(report) = classify_record(&events, &resolver) else {
        panic!("a clean, sealed record must classify as ValidSealed");
    };
    let direct = replay(&events, &resolver).expect("the same record replays cleanly");
    assert_eq!(
        report, direct,
        "classify_record must not report weaker evidence than replay"
    );
}

#[test]
fn classify_record_reports_invalid_for_a_tampered_sealed_record_never_no_canonical_stream() {
    let (events, mut resolver) = passing_run_with_artifacts();
    resolver.insert("diff.patch", b"totally different patch bytes");
    let RecordVerdict::Invalid(err) = classify_record(&events, &resolver) else {
        panic!("a tampered artifact must classify as Invalid, never as absent or unsealed");
    };
    assert!(
        matches!(err, ReplayError::ArtifactDigestMismatch { .. }),
        "got {err:?}"
    );
}

#[test]
fn classify_record_reports_invalid_for_a_broken_chain_never_valid_unsealed() {
    let (mut events, resolver) = passing_run_with_artifacts();
    events.remove(2);
    let RecordVerdict::Invalid(_) = classify_record(&events, &resolver) else {
        panic!("a broken chain must never classify as a genuine unsealed prefix");
    };
}
