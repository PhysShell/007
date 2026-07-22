//! Acceptance (8, 15): reopening preserves committed events; a recovery scan
//! finds running runs/attempts, opening does NOT auto-change them, and the
//! caller marks them interrupted explicitly.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use o7_ledger::{Ledger, NewRun, RunStatus, SqliteLedger};

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
