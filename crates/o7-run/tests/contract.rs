//! GREEN contract tests: the parts that ARE implemented in this contract-only commit —
//! the versioned types, the tamper-evident digest chain, `classify`, and serde stability.
//! The reducer/replay SEMANTICS are exercised (RED) in the sibling test files.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod common;

use common::*;
use o7_run::event::{ArtifactKind, Digest256, GateOutcome};
use o7_run::replay::{classify, Replayability};
use o7_run::{RunEvent, RunEventKind, RUN_EVENT_SCHEMA_VERSION, RUN_STATE_SCHEMA_VERSION};

#[test]
fn a_built_stream_is_internally_consistent() {
    let events = single_gate_stream(contract(&["build"]), "build", GateOutcome::Pass);
    // Every event's stored digest recomputes, and each links to its predecessor.
    let mut prev = Digest256::genesis();
    for (i, e) in events.iter().enumerate() {
        assert_eq!(e.sequence, i as u64, "sequence must be dense from 0");
        assert!(e.digest_is_consistent(), "event {i} digest inconsistent");
        assert_eq!(
            e.previous_event_digest, prev,
            "event {i} previous-digest link is broken"
        );
        prev = e.event_digest.clone();
    }
}

#[test]
fn tampering_one_field_breaks_that_events_digest() {
    let events = single_gate_stream(contract(&["build"]), "build", GateOutcome::Pass);
    let mut tampered = events[4].clone(); // the GateFinished event
                                          // Flip the outcome without recomputing the digest — the in-place edit is detectable.
    tampered.kind = RunEventKind::GateFinished {
        gate: gate("build"),
        outcome: GateOutcome::Fail,
        supported_in_env: true,
        log: None,
    };
    assert!(
        !tampered.digest_is_consistent(),
        "a mutated payload must not still match its stored digest"
    );
}

#[test]
fn the_digest_is_byte_stable_across_recomputation() {
    let events = single_gate_stream(contract(&["build"]), "build", GateOutcome::Pass);
    for e in &events {
        assert_eq!(
            e.compute_digest(),
            e.compute_digest(),
            "digest computation must be deterministic"
        );
    }
}

#[test]
fn an_event_round_trips_through_json() {
    let events = single_gate_stream(contract(&["build"]), "build", GateOutcome::Pass);
    for e in &events {
        let json = serde_json::to_string(e).unwrap();
        let back: RunEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(*e, back, "event must survive a JSON round-trip unchanged");
    }
}

#[test]
fn an_empty_run_is_legacy_and_a_populated_run_is_replayable() {
    assert_eq!(classify(&[]), Replayability::LegacyNonReplayable);
    let events = single_gate_stream(contract(&["build"]), "build", GateOutcome::Pass);
    assert_eq!(classify(&events), Replayability::Replayable);
}

#[test]
fn a_diff_artifact_reference_matches_its_own_bytes() {
    let bytes = b"--- a\n+++ b\n@@ -1 +1 @@\n-old\n+new\n";
    let r = artifact(ArtifactKind::Diff, "diff.patch", bytes);
    assert_eq!(r.digest, Digest256::of_bytes(bytes));
    // A different byte string hashes differently — the anchor for tamper detection.
    assert_ne!(r.digest, Digest256::of_bytes(b"other"));
}

#[test]
fn the_event_and_state_schema_versions_are_pinned() {
    assert_eq!(RUN_EVENT_SCHEMA_VERSION, 1);
    assert_eq!(RUN_STATE_SCHEMA_VERSION, 1);
}
