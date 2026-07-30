//! Q-Deck R0.5 "live-readiness" proof, ledger half: replay the canonical
//! golden synthetic-run transcript (`tests/support`) through a real
//! file-backed SQLite database using only `o7-ledger`'s production write
//! API, and prove exactly the properties the R0.5 downstream contract
//! depends on: events read back in original order, sequence never lost,
//! repeated reads deterministic, run metadata matches what was written,
//! terminal status is a ledger fact (never recomputed by a caller), and a
//! close+reopen of the same database file preserves everything.
//!
//! Invariant for the restriction-lint allowance below: every `unwrap`/
//! `expect`/index here is on a value this same test just constructed or a
//! response this same test just asserted the shape of — a controlled
//! fixture, not runtime input. A panic means this test's own setup broke,
//! which should fail the test loudly. Matches the precedent in this crate's
//! other test files (e.g. `tests/append_replay.rs`).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod support;

use o7_ledger::{Ledger as _, RunStatus, SqliteLedger};
use support::{apply_golden_transcript, outcome_event_type, GoldenOutcome, EXPECTED_EVENT_TYPES};

fn expected_types(outcome: GoldenOutcome) -> Vec<&'static str> {
    let mut types = EXPECTED_EVENT_TYPES.to_vec();
    let last = types.len() - 1;
    types[last] = outcome_event_type(outcome);
    types
}

#[tokio::test]
async fn golden_transcript_events_read_back_in_original_order() {
    let ledger = SqliteLedger::open_in_memory().unwrap();
    let transcript = apply_golden_transcript(&ledger, GoldenOutcome::Pass)
        .await
        .unwrap();

    let events = ledger
        .read_events(&transcript.conversation.conversation_id, None, 100)
        .await
        .unwrap();

    let sequences: Vec<u64> = events.iter().map(|e| e.sequence).collect();
    assert_eq!(
        sequences,
        vec![1, 2, 3, 4, 5, 6, 7],
        "sequence must be gapless and start at 1"
    );

    let types: Vec<&str> = events.iter().map(|e| e.event_type.as_str()).collect();
    assert_eq!(
        types,
        expected_types(GoldenOutcome::Pass),
        "events must read back in exactly the order they were appended"
    );
}

#[tokio::test]
async fn golden_transcript_repeated_read_is_deterministic() {
    let ledger = SqliteLedger::open_in_memory().unwrap();
    let transcript = apply_golden_transcript(&ledger, GoldenOutcome::Pass)
        .await
        .unwrap();

    let first = ledger
        .read_events(&transcript.conversation.conversation_id, None, 100)
        .await
        .unwrap();
    let second = ledger
        .read_events(&transcript.conversation.conversation_id, None, 100)
        .await
        .unwrap();

    assert_eq!(
        first, second,
        "reading the same conversation twice must return byte-identical results"
    );
}

#[tokio::test]
async fn golden_transcript_run_metadata_matches_transcript_pass() {
    let ledger = SqliteLedger::open_in_memory().unwrap();
    let transcript = apply_golden_transcript(&ledger, GoldenOutcome::Pass)
        .await
        .unwrap();

    let run = ledger
        .run(transcript.run.run_id.clone())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(run.agent, "claude");
    assert_eq!(run.role, "implementer");
    assert_eq!(run.conversation_id, transcript.conversation.conversation_id);
    assert_eq!(run.status, o7_ledger::RunStatus::Completed);
    assert!(
        run.finished_at.is_some(),
        "PASS (completed) is a closed terminal state — finished_at must be set"
    );
}

#[tokio::test]
async fn golden_transcript_run_metadata_matches_transcript_fail() {
    let ledger = SqliteLedger::open_in_memory().unwrap();
    let transcript = apply_golden_transcript(&ledger, GoldenOutcome::Fail)
        .await
        .unwrap();

    let run = ledger
        .run(transcript.run.run_id.clone())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(run.status, o7_ledger::RunStatus::Failed);
    assert!(
        run.finished_at.is_some(),
        "FAIL (failed) is a closed terminal state — finished_at must be set"
    );
}

#[tokio::test]
async fn golden_transcript_run_metadata_matches_transcript_blocked() {
    let ledger = SqliteLedger::open_in_memory().unwrap();
    let transcript = apply_golden_transcript(&ledger, GoldenOutcome::Blocked)
        .await
        .unwrap();

    let run = ledger
        .run(transcript.run.run_id.clone())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(run.status, o7_ledger::RunStatus::Blocked);
    assert!(
        run.finished_at.is_some(),
        "BLOCKED is a sealed/closed terminal state — finished_at must be set"
    );
}

#[tokio::test]
async fn golden_transcript_run_metadata_matches_transcript_error() {
    let ledger = SqliteLedger::open_in_memory().unwrap();
    let transcript = apply_golden_transcript(&ledger, GoldenOutcome::Error)
        .await
        .unwrap();

    let run = ledger
        .run(transcript.run.run_id.clone())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(run.status, o7_ledger::RunStatus::Error);
    assert!(
        run.finished_at.is_some(),
        "ERROR is a sealed/closed terminal state — finished_at must be set (and it is NOT the same thing as interrupted)"
    );
}

#[tokio::test]
async fn golden_transcript_run_metadata_matches_transcript_interrupted() {
    let ledger = SqliteLedger::open_in_memory().unwrap();
    let transcript = apply_golden_transcript(&ledger, GoldenOutcome::Interrupted)
        .await
        .unwrap();

    let run = ledger
        .run(transcript.run.run_id.clone())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(run.status, o7_ledger::RunStatus::Interrupted);
    // Real, existing ledger behavior, not a bug this fixture works around:
    // `RunStatus::is_terminal()` deliberately excludes `Interrupted` (it is
    // resumable via `resume_interrupted_run`), so `finished_at` stays unset
    // even though the run is no longer active. This is NOT a terminal/sealed
    // outcome and must never be reshaped into looking like one.
    assert!(
        run.finished_at.is_none(),
        "interrupted is resumable, not a sealed terminal state — finished_at stays unset"
    );
}

#[tokio::test]
async fn golden_transcript_interrupted_run_can_resume_to_running() {
    // The regression this transcript must prove: an interrupted run is not
    // a dead end. `resume_interrupted_run` is the ONLY production path that
    // can move a run `interrupted -> running` (the general `start_run` path
    // deliberately cannot revive one — see `start_run_cannot_revive_interrupted_run`
    // in tests/attempt_lifecycle.rs); this test proves it on THIS transcript's
    // own run, not just in the abstract.
    let ledger = SqliteLedger::open_in_memory().unwrap();
    let transcript = apply_golden_transcript(&ledger, GoldenOutcome::Interrupted)
        .await
        .unwrap();

    ledger
        .resume_interrupted_run(transcript.run.run_id.clone())
        .await
        .unwrap();

    let resumed = ledger
        .run(transcript.run.run_id.clone())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(resumed.status, RunStatus::Running);
    assert!(
        resumed.finished_at.is_none(),
        "a resumed run is active again, not finished"
    );
}

#[tokio::test]
async fn golden_transcript_terminal_status_is_a_ledger_fact_not_recomputed() {
    // The status stored on `Run` is exactly the enum the transition method
    // wrote — there is no separate reducer/derivation step anywhere in this
    // crate that could disagree with it. Proven here by asserting the value
    // returned directly from `fail_run` equals the value read back
    // independently via `ledger.run(...)`.
    let ledger = SqliteLedger::open_in_memory().unwrap();
    let transcript = apply_golden_transcript(&ledger, GoldenOutcome::Fail)
        .await
        .unwrap();

    let read_back = ledger
        .run(transcript.run.run_id.clone())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(transcript.run.status, read_back.status);
    assert_eq!(transcript.run.finished_at, read_back.finished_at);
    assert_eq!(transcript.outcome, GoldenOutcome::Fail);
}

#[tokio::test]
async fn golden_transcript_restart_preserves_state_for_every_outcome() {
    for outcome in [
        GoldenOutcome::Pass,
        GoldenOutcome::Fail,
        GoldenOutcome::Interrupted,
        GoldenOutcome::Blocked,
        GoldenOutcome::Error,
    ] {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("ledger.sqlite3");

        let (conv_id, run_id, before_events) = {
            let ledger = SqliteLedger::open(&db_path).unwrap();
            let transcript = apply_golden_transcript(&ledger, outcome).await.unwrap();
            let events = ledger
                .read_events(&transcript.conversation.conversation_id, None, 100)
                .await
                .unwrap();
            (
                transcript.conversation.conversation_id.clone(),
                transcript.run.run_id.clone(),
                events,
            )
            // `ledger` (and its underlying connection) drops here.
        };

        // Reopen the SAME file as a brand-new `SqliteLedger` instance — this
        // is what a real `o7d serve --ledger <path>` restart looks like, not
        // just a fresh in-memory database that happens to be empty.
        let reopened = SqliteLedger::open(&db_path).unwrap();

        let after_events = reopened.read_events(&conv_id, None, 100).await.unwrap();
        assert_eq!(
            before_events, after_events,
            "every event must read back identically after a restart (outcome={outcome:?})"
        );

        let run = reopened.run(run_id).await.unwrap().unwrap();
        let expected_finished_at_is_some = outcome != GoldenOutcome::Interrupted;
        assert_eq!(
            run.finished_at.is_some(),
            expected_finished_at_is_some,
            "finished_at sealed-ness must survive a restart (outcome={outcome:?})"
        );

        let conversation = reopened.conversation(conv_id).await.unwrap().unwrap();
        assert_eq!(conversation.status, o7_ledger::ConversationStatus::Open);
    }
}
