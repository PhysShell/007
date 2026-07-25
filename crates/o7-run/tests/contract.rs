//! GREEN contract tests: the parts that ARE implemented in this contract-only commit — the
//! versioned types, the tamper-evident digest chain, form-validation of untrusted inputs,
//! three-valued replay classification, and serde stability. The reducer/replay SEMANTICS
//! are exercised (RED) in the sibling test files.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod common;

use common::*;
use o7_run::event::{Digest256, GateOutcome, RunEventKind};
use o7_run::ids::RunId;
use o7_run::replay::{classify, Replayability};
use o7_run::{RunEvent, RUN_EVENT_SCHEMA_VERSION, RUN_STATE_SCHEMA_VERSION};

#[test]
fn a_built_stream_is_internally_consistent() {
    let events = single_required_gate_stream("build", GateOutcome::Pass);
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
    let events = single_required_gate_stream("build", GateOutcome::Pass);
    // events[4] is the GateFinished event; flip its outcome without recomputing the digest.
    let mut tampered = events[4].clone();
    tampered.kind = RunEventKind::GateFinished {
        gate: gate("build"),
        outcome: GateOutcome::Fail,
        log: None,
    };
    assert!(
        !tampered.digest_is_consistent(),
        "a mutated payload must not still match its stored digest"
    );
}

#[test]
fn the_digest_is_byte_stable_across_recomputation() {
    let events = single_required_gate_stream("build", GateOutcome::Pass);
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
    let events = single_required_gate_stream("build", GateOutcome::Pass);
    for e in &events {
        let json = serde_json::to_string(e).unwrap();
        let back: RunEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(*e, back, "event must survive a JSON round-trip unchanged");
    }
}

#[test]
fn classification_is_three_valued() {
    // Empty → legacy.
    assert_eq!(classify(&[]), Replayability::LegacyNonReplayable);
    // A genesis RunStarted → replay candidate.
    let good = single_required_gate_stream("build", GateOutcome::Pass);
    assert_eq!(classify(&good), Replayability::CanonicalReplayable);
    // Non-empty but not starting with a genesis RunStarted → malformed, NOT replayable.
    let malformed = chained(vec![RunEventKind::AgentStarted, RunEventKind::RunSealed]);
    assert_eq!(classify(&malformed), Replayability::CanonicalMalformed);
}

#[test]
fn a_digest_only_accepts_64_lowercase_hex() {
    assert!(Digest256::parse(&"a".repeat(64)).is_ok());
    assert!(Digest256::parse(&"0".repeat(64)).is_ok());
    assert!(Digest256::parse("abc").is_err(), "too short");
    assert!(
        Digest256::parse(&"A".repeat(64)).is_err(),
        "uppercase is not canonical"
    );
    assert!(
        Digest256::parse(&"g".repeat(64)).is_err(),
        "non-hex is rejected"
    );
    assert!(
        Digest256::parse(&"a".repeat(65)).is_err(),
        "too long is rejected"
    );
}

#[test]
fn a_malformed_digest_is_rejected_on_deserialization() {
    // A stored value is untrusted: an out-of-form digest must not deserialize.
    let bad = "\"not-a-real-digest\"";
    assert!(serde_json::from_str::<Digest256>(bad).is_err());
    let good = format!("\"{}\"", "a".repeat(64));
    assert!(serde_json::from_str::<Digest256>(&good).is_ok());
}

#[test]
fn an_empty_id_is_rejected() {
    assert!(RunId::new("").is_err(), "empty id via constructor");
    assert!(RunId::new("run-1").is_ok());
    // And on the untrusted deserialization path.
    assert!(serde_json::from_str::<RunId>("\"\"").is_err());
    assert!(serde_json::from_str::<RunId>("\"run-1\"").is_ok());
}

#[test]
fn the_event_and_state_schema_versions_are_pinned() {
    assert_eq!(RUN_EVENT_SCHEMA_VERSION, 1);
    assert_eq!(RUN_STATE_SCHEMA_VERSION, 1);
}
