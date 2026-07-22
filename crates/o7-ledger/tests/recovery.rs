//! Acceptance (8, 15): reopening preserves committed events; a recovery scan
//! finds running runs/attempts, opening does NOT auto-change them, and the
//! caller marks them interrupted explicitly.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use o7_ledger::{AttemptStatus, Ledger, NewRun, RecoveryState, RunStatus, SqliteLedger};

// (8) Reopening the database preserves every committed event.
#[tokio::test]
async fn reopen_preserves_committed_events() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ledger.db");

    let conv_id;
    {
        let ledger = SqliteLedger::open(&path).unwrap();
        let conv = ledger.create_conversation(None).await.unwrap();
        conv_id = conv.conversation_id.clone();
        ledger
            .append_user_message(conv_id.clone(), serde_json::json!({"m":1}), None, None)
            .await
            .unwrap();
        ledger
            .append_user_message(conv_id.clone(), serde_json::json!({"m":2}), None, None)
            .await
            .unwrap();
        ledger
            .create_run(
                NewRun {
                    conversation_id: conv_id.clone(),
                    parent_run_id: None,
                    agent: "codex".to_owned(),
                    role: "implementer".to_owned(),
                },
                None,
            )
            .await
            .unwrap();
    } // dropped — connection closed

    let reopened = SqliteLedger::open(&path).unwrap();
    let events = reopened.read_events(&conv_id, None, 100).await.unwrap();
    // conversation.created + 2 user messages + run.created
    assert_eq!(events.len(), 4);
    assert_eq!(events[0].event_type, "conversation.created");
    assert_eq!(events[3].event_type, "run.created");
    assert_eq!(
        events.iter().map(|e| e.sequence).collect::<Vec<_>>(),
        vec![1, 2, 3, 4]
    );
}

// (15) A running run/attempt is found by the recovery scan; open() does not
// mutate it; the caller marks it interrupted explicitly.
#[tokio::test]
async fn recovery_scan_finds_running_work_without_mutating_it() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ledger.db");

    let run_id;
    let attempt_id;
    {
        let ledger = SqliteLedger::open(&path).unwrap();
        let conv = ledger.create_conversation(None).await.unwrap();
        let run = ledger
            .create_run(
                NewRun {
                    conversation_id: conv.conversation_id.clone(),
                    parent_run_id: None,
                    agent: "claude".to_owned(),
                    role: "implementer".to_owned(),
                },
                None,
            )
            .await
            .unwrap();
        ledger.start_run(run.run_id.clone()).await.unwrap();
        let attempt = ledger.create_attempt(run.run_id.clone()).await.unwrap();
        run_id = run.run_id.clone();
        attempt_id = attempt.attempt_id.clone();
        // dropped mid-flight: run + attempt are left `running` (a crash)
    }

    let reopened = SqliteLedger::open(&path).unwrap();

    // open() must NOT have changed status.
    let run_after_open = reopened.run(run_id.clone()).await.unwrap().unwrap();
    assert_eq!(run_after_open.status, RunStatus::Running);

    let recovery = reopened.recover_scan().await.unwrap();
    assert!(recovery.interrupted_runs.contains(&run_id));
    assert!(recovery.interrupted_attempts.contains(&attempt_id));

    // Explicit, caller-driven marking.
    reopened.mark_interrupted(recovery).await.unwrap();
    let run_final = reopened.run(run_id.clone()).await.unwrap().unwrap();
    assert_eq!(run_final.status, RunStatus::Interrupted);

    // Nothing left running.
    let after = reopened.recover_scan().await.unwrap();
    assert!(after.is_empty());
}

// mark_interrupted must not trust a stale/partial snapshot: closing a run closes
// EVERY running attempt of that run, even one the caller omitted — so a run is
// never left `interrupted` with a `running` attempt.
#[tokio::test]
async fn mark_interrupted_closes_all_running_attempts_despite_stale_snapshot() {
    let ledger = SqliteLedger::open_in_memory().unwrap();
    let conv = ledger.create_conversation(None).await.unwrap();
    let run = ledger
        .create_run(
            NewRun {
                conversation_id: conv.conversation_id.clone(),
                parent_run_id: None,
                agent: "claude".to_owned(),
                role: "implementer".to_owned(),
            },
            None,
        )
        .await
        .unwrap();
    ledger.start_run(run.run_id.clone()).await.unwrap();
    // A running run WITH a running attempt: the exact "crashed mid-run" state.
    let attempt = ledger.create_attempt(run.run_id.clone()).await.unwrap();

    let scan = ledger.recover_scan().await.unwrap();
    assert!(scan.interrupted_runs.contains(&run.run_id));
    assert!(scan.interrupted_attempts.contains(&attempt.attempt_id));

    // Deliberately STALE snapshot: run listed, its running attempt omitted.
    let stale = RecoveryState {
        interrupted_runs: vec![run.run_id.clone()],
        interrupted_attempts: vec![],
    };
    ledger.mark_interrupted(stale).await.unwrap();

    let run_after = ledger.run(run.run_id.clone()).await.unwrap().unwrap();
    assert_eq!(run_after.status, RunStatus::Interrupted);
    let attempt_after = ledger
        .attempt(attempt.attempt_id.clone())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        attempt_after.status,
        AttemptStatus::Interrupted,
        "the omitted running attempt must still be closed"
    );
    assert!(ledger.recover_scan().await.unwrap().is_empty());
}
