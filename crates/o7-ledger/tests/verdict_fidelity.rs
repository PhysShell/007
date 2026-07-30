//! Q-Deck R0.6 (`docs/q-deck/r06-verdict-fidelity.md`): direct transition,
//! migration, reopen, and recovery proof for the new sealed `Blocked`/`Error`
//! run outcomes, and for the v1 -> v2 schema migration that gives
//! `run.status`/`run_attempt.status` real `CHECK` constraints.
//!
//! Invariant for the restriction-lint allowance below: every `unwrap`/
//! `expect`/index here operates on this test's own controlled fixtures (a
//! just-created ledger, a row this test itself inserted the line before) —
//! a panic here is this test's own setup failing loudly, not a runtime
//! condition. Matches the precedent in this crate's other test files.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use o7_ledger::migrations::CURRENT_SCHEMA_VERSION;
use o7_ledger::{AttemptStatus, NewRun, RunStatus, SqliteLedger};
use rusqlite::Connection;

fn run_req(conversation_id: o7_ledger::ConversationId) -> NewRun {
    NewRun {
        conversation_id,
        parent_run_id: None,
        agent: "claude".to_owned(),
        role: "implementer".to_owned(),
    }
}

// --- direct transitions ----------------------------------------------------

#[tokio::test]
async fn running_can_transition_to_blocked_and_error() {
    let ledger = SqliteLedger::open_in_memory().unwrap();
    let conv = ledger.create_conversation(None).await.unwrap();

    let blocked_run = ledger
        .create_run(run_req(conv.conversation_id.clone()), None)
        .await
        .unwrap();
    ledger.start_run(blocked_run.run_id.clone()).await.unwrap();
    let blocked = ledger.block_run(blocked_run.run_id.clone()).await.unwrap();
    assert_eq!(blocked.status, RunStatus::Blocked);
    assert!(blocked.finished_at.is_some(), "blocked is sealed");

    let errored_run = ledger
        .create_run(run_req(conv.conversation_id.clone()), None)
        .await
        .unwrap();
    ledger.start_run(errored_run.run_id.clone()).await.unwrap();
    let errored = ledger.error_run(errored_run.run_id.clone()).await.unwrap();
    assert_eq!(errored.status, RunStatus::Error);
    assert!(errored.finished_at.is_some(), "error is sealed");
}

#[tokio::test]
async fn blocked_and_error_are_sealed_dead_ends() {
    let ledger = SqliteLedger::open_in_memory().unwrap();
    let conv = ledger.create_conversation(None).await.unwrap();

    // queued -> blocked (skipping running) is forbidden, same shape as
    // queued -> completed already being forbidden.
    let run = ledger
        .create_run(run_req(conv.conversation_id.clone()), None)
        .await
        .unwrap();
    let err = ledger.block_run(run.run_id.clone()).await.unwrap_err();
    assert_eq!(err.code(), "FORBIDDEN_TRANSITION");

    ledger.start_run(run.run_id.clone()).await.unwrap();
    ledger.block_run(run.run_id.clone()).await.unwrap();
    // blocked -> running is forbidden: sealed means sealed, no resurrection
    // path the way `interrupted` deliberately has one.
    let err = ledger.start_run(run.run_id.clone()).await.unwrap_err();
    assert_eq!(err.code(), "FORBIDDEN_TRANSITION");
    let err = ledger.error_run(run.run_id.clone()).await.unwrap_err();
    assert_eq!(err.code(), "FORBIDDEN_TRANSITION");
}

#[tokio::test]
async fn blocked_and_error_never_collapse_into_completed_at_the_attempt_level() {
    // The regression this vertical exists to prevent: `terminal_attempt_status`
    // used to catch-all anything that wasn't Failed/Cancelled/Interrupted into
    // `Completed` -- which would have silently mislabeled a blocked/errored
    // run's attempt as having succeeded.
    let ledger = SqliteLedger::open_in_memory().unwrap();
    let conv = ledger.create_conversation(None).await.unwrap();

    let run = ledger
        .create_run(run_req(conv.conversation_id.clone()), None)
        .await
        .unwrap();
    ledger.start_run(run.run_id.clone()).await.unwrap();
    let attempt = ledger.create_attempt(run.run_id.clone()).await.unwrap();
    ledger.block_run(run.run_id.clone()).await.unwrap();

    let reloaded = ledger
        .attempt(attempt.attempt_id.clone())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(reloaded.status, AttemptStatus::Blocked);
    assert_ne!(reloaded.status, AttemptStatus::Completed);
}

// --- reopen -----------------------------------------------------------------

#[tokio::test]
async fn blocked_and_error_survive_a_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ledger.sqlite3");

    let (run_id, conv_id) = {
        let ledger = SqliteLedger::open(&path).unwrap();
        let conv = ledger.create_conversation(None).await.unwrap();
        let run = ledger
            .create_run(run_req(conv.conversation_id.clone()), None)
            .await
            .unwrap();
        ledger.start_run(run.run_id.clone()).await.unwrap();
        ledger.error_run(run.run_id.clone()).await.unwrap();
        (run.run_id.clone(), conv.conversation_id.clone())
    };

    let reopened = SqliteLedger::open(&path).unwrap();
    let run = reopened.run(run_id).await.unwrap().unwrap();
    assert_eq!(run.status, RunStatus::Error);
    assert!(run.finished_at.is_some());
    assert_eq!(run.conversation_id, conv_id);
}

// --- recovery ----------------------------------------------------------------

#[tokio::test]
async fn recovery_scan_does_not_touch_an_already_sealed_blocked_or_error_run() {
    // recover_scan only ever looks for rows still `running` at open — a run
    // already sealed as blocked/error must never be reported as needing
    // recovery, exactly as an already-completed run isn't today.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ledger.sqlite3");
    let run_id = {
        let ledger = SqliteLedger::open(&path).unwrap();
        let conv = ledger.create_conversation(None).await.unwrap();
        let run = ledger
            .create_run(run_req(conv.conversation_id.clone()), None)
            .await
            .unwrap();
        ledger.start_run(run.run_id.clone()).await.unwrap();
        ledger.block_run(run.run_id.clone()).await.unwrap();
        run.run_id.clone()
    };

    let reopened = SqliteLedger::open(&path).unwrap();
    let recovery = reopened.recover_scan().await.unwrap();
    assert!(
        !recovery.interrupted_runs.contains(&run_id),
        "a sealed blocked run must not be reported by recovery scan"
    );
}

// --- migration ---------------------------------------------------------------

/// Build a v1-only database by hand (bypassing `SqliteLedger::open`, which
/// would migrate it), insert a run through raw SQL, then confirm opening it
/// for real migrates to v2 and preserves the row byte-for-byte.
#[tokio::test]
async fn migrating_v1_database_preserves_existing_rows() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("v1.sqlite3");

    {
        let conn = Connection::open(&path).unwrap();
        // The real v1 SQL, verbatim — never hand-duplicated, so this test
        // can't silently drift from what `apply`/`validate_schema` actually
        // produce.
        conn.execute_batch(o7_ledger::migrations::migration_sql(1).unwrap())
            .unwrap();
        conn.pragma_update(None, "user_version", 1).unwrap();
        conn.execute(
            "INSERT INTO conversation (conversation_id, created_at, status) VALUES ('c1', 1000, 'open')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO run (run_id, conversation_id, parent_run_id, agent, role, status, created_at, finished_at) \
             VALUES ('r1', 'c1', NULL, 'claude', 'implementer', 'completed', 1000, 1005)",
            [],
        )
        .unwrap();
    }

    assert_eq!(CURRENT_SCHEMA_VERSION, 2, "this test assumes v1 -> v2");
    let ledger = SqliteLedger::open(&path).unwrap();
    let run = ledger
        .run(o7_ledger::RunId::from_raw("r1".to_owned()))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(run.conversation_id.as_str(), "c1");
    assert_eq!(run.agent, "claude");
    assert_eq!(run.role, "implementer");
    assert_eq!(run.status, RunStatus::Completed);
    assert_eq!(run.created_at, 1000);
    assert_eq!(run.finished_at, Some(1005));

    let conv = ledger
        .conversation(o7_ledger::ConversationId::from_raw("c1".to_owned()))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(conv.created_at, 1000);
}

#[tokio::test]
async fn v2_check_constraint_rejects_an_invalid_run_status_via_raw_sql() {
    // Defense-in-depth proof: the CHECK constraint added in v2 rejects a
    // direct-SQL write with an invalid status even though every production
    // write path only ever writes a valid RunStatus::as_str() value anyway.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ledger.sqlite3");
    let run_id = {
        let ledger = SqliteLedger::open(&path).unwrap();
        let conv = ledger.create_conversation(None).await.unwrap();
        let run = ledger
            .create_run(run_req(conv.conversation_id.clone()), None)
            .await
            .unwrap();
        run.run_id.clone()
    };

    let conn = Connection::open(&path).unwrap();
    let result = conn.execute(
        "UPDATE run SET status = 'banana' WHERE run_id = ?1",
        [run_id.as_str()],
    );
    assert!(
        result.is_err(),
        "the v2 CHECK constraint must reject an unrecognized run status"
    );
}

#[tokio::test]
async fn schema_v3_would_still_fail_closed() {
    // Same guard as tests/migrations.rs's schema_too_new_fails_closed, run
    // again here to confirm it still holds at the NEW CURRENT_SCHEMA_VERSION
    // (2), not just conceptually.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("too-new.sqlite3");
    SqliteLedger::open(&path).unwrap();
    {
        let conn = Connection::open(&path).unwrap();
        conn.pragma_update(None, "user_version", CURRENT_SCHEMA_VERSION + 1)
            .unwrap();
    }
    let err = SqliteLedger::open(&path).unwrap_err();
    assert_eq!(err.code(), "SCHEMA_TOO_NEW");
}
