//! Acceptance: attempt lifecycle is closed — no attempt on a non-running run, at
//! most one running attempt per run, and an atomic resume of an interrupted run
//! with a new attempt.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use o7_ledger::{AttemptStatus, NewRun, RunStatus, SqliteLedger};

async fn conv_and_run(ledger: &SqliteLedger) -> o7_ledger::RunId {
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
    run.run_id
}

// create_attempt is rejected on a queued run and on a terminal run.
#[tokio::test]
async fn cannot_create_attempt_on_non_running_run() {
    let ledger = SqliteLedger::open_in_memory().unwrap();
    let run_id = conv_and_run(&ledger).await;

    // queued run
    let err = ledger.create_attempt(run_id.clone()).await.unwrap_err();
    assert_eq!(err.code(), "INVALID_STATE");

    // terminal (completed) run
    ledger.start_run(run_id.clone()).await.unwrap();
    ledger.complete_run(run_id.clone()).await.unwrap();
    let err = ledger.create_attempt(run_id.clone()).await.unwrap_err();
    assert_eq!(err.code(), "INVALID_STATE");
}

// At most one running attempt per run.
#[tokio::test]
async fn cannot_create_two_running_attempts() {
    let ledger = SqliteLedger::open_in_memory().unwrap();
    let run_id = conv_and_run(&ledger).await;
    ledger.start_run(run_id.clone()).await.unwrap();

    let first = ledger.create_attempt(run_id.clone()).await.unwrap();
    assert_eq!(first.status, AttemptStatus::Running);

    let err = ledger.create_attempt(run_id.clone()).await.unwrap_err();
    assert_eq!(err.code(), "INVALID_STATE");
}

// Completing/interrupting a run finishes its running attempt (no dangling
// running attempt on a non-running run).
#[tokio::test]
async fn finishing_a_run_finishes_its_running_attempt() {
    let ledger = SqliteLedger::open_in_memory().unwrap();
    let run_id = conv_and_run(&ledger).await;
    ledger.start_run(run_id.clone()).await.unwrap();
    let attempt = ledger.create_attempt(run_id.clone()).await.unwrap();

    ledger.complete_run(run_id.clone()).await.unwrap();
    let after = ledger
        .attempt(attempt.attempt_id.clone())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(after.status, AttemptStatus::Completed);
    // No running attempts remain.
    assert!(ledger.recover_scan().await.unwrap().is_empty());
}

// interrupted → running via a NEW attempt, atomically.
#[tokio::test]
async fn resume_interrupted_run_is_atomic() {
    let ledger = SqliteLedger::open_in_memory().unwrap();
    let run_id = conv_and_run(&ledger).await;
    ledger.start_run(run_id.clone()).await.unwrap();
    let attempt1 = ledger.create_attempt(run_id.clone()).await.unwrap();

    ledger.interrupt_run(run_id.clone()).await.unwrap();
    // interrupt finished attempt 1 too.
    let a1 = ledger
        .attempt(attempt1.attempt_id.clone())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(a1.status, AttemptStatus::Interrupted);

    let attempt2 = ledger.resume_interrupted_run(run_id.clone()).await.unwrap();
    assert_eq!(attempt2.attempt_number, 2);
    assert_eq!(attempt2.status, AttemptStatus::Running);

    let run = ledger.run(run_id.clone()).await.unwrap().unwrap();
    assert_eq!(run.status, RunStatus::Running);

    // Resume on a non-interrupted (now running) run is rejected.
    let err = ledger
        .resume_interrupted_run(run_id.clone())
        .await
        .unwrap_err();
    assert_eq!(err.code(), "INVALID_STATE");
}

// start_run must NOT revive an interrupted run (that would bypass the atomic
// resume + new-attempt path). Only resume_interrupted_run may.
#[tokio::test]
async fn start_run_cannot_revive_interrupted_run() {
    let ledger = SqliteLedger::open_in_memory().unwrap();
    let run_id = conv_and_run(&ledger).await;
    ledger.start_run(run_id.clone()).await.unwrap(); // queued -> running
    ledger.interrupt_run(run_id.clone()).await.unwrap(); // running -> interrupted

    let err = ledger.start_run(run_id.clone()).await.unwrap_err();
    assert_eq!(
        err.code(),
        "FORBIDDEN_TRANSITION",
        "start_run must not make interrupted -> running"
    );

    // The only sanctioned path succeeds and creates a fresh attempt.
    let attempt = ledger.resume_interrupted_run(run_id.clone()).await.unwrap();
    assert_eq!(attempt.status, AttemptStatus::Running);
    let run = ledger.run(run_id).await.unwrap().unwrap();
    assert_eq!(run.status, RunStatus::Running);
}
