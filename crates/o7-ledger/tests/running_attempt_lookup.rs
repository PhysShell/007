//! Q-Deck R0.7 (`docs/q-deck/r07-live-ingress.md`): `running_attempt` is how
//! a resumed live-ingress projector (a fresh process, after a crash) finds
//! the attempt it should keep projecting into, since the crashed process's
//! in-memory `AttemptId` is gone.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use o7_ledger::{NewRun, SqliteLedger};

#[tokio::test]
async fn finds_the_one_running_attempt_for_a_run() {
    let ledger = SqliteLedger::open_in_memory().unwrap();
    let conv = ledger.create_conversation(None).await.unwrap();
    let run = ledger
        .create_run(
            NewRun {
                conversation_id: conv.conversation_id,
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

    let found = ledger
        .running_attempt(run.run_id.clone())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(found.attempt_id, attempt.attempt_id);
    assert_eq!(found.status, o7_ledger::AttemptStatus::Running);
}

#[tokio::test]
async fn none_for_a_run_with_no_running_attempt() {
    let ledger = SqliteLedger::open_in_memory().unwrap();
    let conv = ledger.create_conversation(None).await.unwrap();
    let run = ledger
        .create_run(
            NewRun {
                conversation_id: conv.conversation_id,
                parent_run_id: None,
                agent: "claude".to_owned(),
                role: "implementer".to_owned(),
            },
            None,
        )
        .await
        .unwrap();
    // Queued: no attempt exists at all yet.
    assert!(ledger
        .running_attempt(run.run_id.clone())
        .await
        .unwrap()
        .is_none());

    ledger.start_run(run.run_id.clone()).await.unwrap();
    ledger.create_attempt(run.run_id.clone()).await.unwrap();
    ledger.complete_run(run.run_id.clone()).await.unwrap();
    // Terminal: the attempt is finished, not running, anymore.
    assert!(ledger.running_attempt(run.run_id).await.unwrap().is_none());
}

/// Q-Deck R0.7 second re-gate, blocker #2: unlike `running_attempt`,
/// `latest_attempt` must still find the attempt of an already-terminal run —
/// a catch-up attaching to a sealed run needs the SAME `attempt_id` its
/// events were originally projected under (part of `append_system_note`'s
/// idempotency digest), not `None`.
#[tokio::test]
async fn latest_attempt_finds_a_terminal_runs_attempt_too() {
    let ledger = SqliteLedger::open_in_memory().unwrap();
    let conv = ledger.create_conversation(None).await.unwrap();
    let run = ledger
        .create_run(
            NewRun {
                conversation_id: conv.conversation_id,
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
    ledger.complete_run(run.run_id.clone()).await.unwrap();

    let found = ledger
        .latest_attempt(run.run_id.clone())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(found.attempt_id, attempt.attempt_id);
    assert_eq!(found.status, o7_ledger::AttemptStatus::Completed);
}

#[tokio::test]
async fn latest_attempt_is_none_before_any_attempt_exists() {
    let ledger = SqliteLedger::open_in_memory().unwrap();
    let conv = ledger.create_conversation(None).await.unwrap();
    let run = ledger
        .create_run(
            NewRun {
                conversation_id: conv.conversation_id,
                parent_run_id: None,
                agent: "claude".to_owned(),
                role: "implementer".to_owned(),
            },
            None,
        )
        .await
        .unwrap();
    assert!(ledger.latest_attempt(run.run_id).await.unwrap().is_none());
}
