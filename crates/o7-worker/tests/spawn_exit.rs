//! Acceptance 1-5, 25: spawn + exit reporting and the exit-vs-cancel race.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod common;

use std::path::PathBuf;

use common::*;
use o7_worker::WorkerResult;

// (1) A simple process runs and returns exit code 0.
#[tokio::test]
async fn exit_zero() {
    let sink = RecordingSink::new();
    let result = run_to_completion(child_spec("exit0", "exit0"), &sink).await;
    assert_eq!(result, WorkerResult::ExitedNormally(0));
    assert!(sink.has("spawned"));
    assert!(sink.has("cleanup_completed"));
}

// (2) A non-zero exit code is preserved exactly.
#[tokio::test]
async fn non_zero_exit_code_preserved() {
    let mut spec = child_spec("exit42", "exit_code");
    set_env(&mut spec, ENV_CODE, "42");
    let sink = RecordingSink::new();
    let result = run_to_completion(spec, &sink).await;
    assert_eq!(result, WorkerResult::ExitedNormally(42));
}

// (3) A signal exit is distinct from an exit code.
#[tokio::test]
async fn signal_exit_is_distinct_from_code() {
    let sink = RecordingSink::new();
    let result = run_to_completion(child_spec("sig", "signal"), &sink).await;
    assert_eq!(result.kind(), "EXITED_BY_SIGNAL", "got {result:?}");
    assert!(!matches!(result, WorkerResult::ExitedNormally(_)));
}

// (4) A non-existent executable yields FailedToStart.
#[tokio::test]
async fn nonexistent_executable_fails_to_start() {
    let mut spec = child_spec("missing", "exit0");
    spec.executable = PathBuf::from("/nonexistent/o7-worker-definitely-not-here");
    let sink = RecordingSink::new();
    let result = run_to_completion(spec, &sink).await;
    assert_eq!(result.kind(), "FAILED_TO_START", "got {result:?}");
}

// (5) A spawn failure leaves no process state (nothing was spawned).
#[tokio::test]
async fn spawn_failure_leaves_no_process_state() {
    let mut spec = child_spec("missing2", "exit0");
    spec.executable = PathBuf::from("/nonexistent/o7-worker-nope");
    let sink = RecordingSink::new();
    let result = run_to_completion(spec, &sink).await;
    assert_eq!(result.kind(), "FAILED_TO_START");
    assert!(
        !sink.has("spawned"),
        "no Spawned observation on spawn failure"
    );
    assert!(!sink.has("cleanup_completed"));
}

// (25) A natural-exit-vs-cancel race yields exactly ONE deterministic terminal
// result (never two, never a panic).
#[tokio::test]
async fn natural_exit_vs_cancel_race_is_single_terminal() {
    let sink = RecordingSink::new();
    let (handle, join) = start(child_spec("race", "exit0"), &sink);
    // Cancel roughly concurrently with the process exiting.
    handle.cancel().await;
    let result = join.join().await;

    // Whichever won, it is one valid terminal, and at most one cleanup_completed.
    let valid = matches!(
        result,
        WorkerResult::ExitedNormally(_)
            | WorkerResult::CancelledGracefully
            | WorkerResult::CancelledForcefully
    );
    assert!(valid, "unexpected terminal: {result:?}");
    assert!(sink.count("cleanup_completed") <= 1);
}
