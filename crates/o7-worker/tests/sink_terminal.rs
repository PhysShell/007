//! Gate blocker #3: the `ObservationSink` is authoritative in the TERMINAL and
//! cleanup phases too — not only for mid-run output. If a publish fails on
//! `Exited`, `CleanupCompleted`, `DescendantsRemaining`, or a cancellation
//! observation, the worker must report `ObservationFailure`, never a false
//! `ExitedNormally`/`Cancelled*`. The owned set is still cleaned up regardless.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod common;

use common::*;

// A sink that dies on `Exited` must not yield a successful exit.
#[tokio::test]
async fn sink_failure_on_exited_is_observation_failure() {
    let sink = RecordingSink::failing_on_kind("exited");
    let result = run_to_completion(child_spec("term-exit", "exit0"), &sink).await;

    assert_eq!(result.kind(), "OBSERVATION_FAILURE", "got {result:?}");
    let kinds = sink.kinds();
    assert!(
        !kinds.contains(&"cleanup_completed"),
        "a dead sink cannot have recorded a clean terminal: {kinds:?}"
    );
}

// A sink that dies on the very last observation (`CleanupCompleted`) still fails
// the worker rather than returning a success.
#[tokio::test]
async fn sink_failure_on_cleanup_completed_is_observation_failure() {
    let sink = RecordingSink::failing_on_kind("cleanup_completed");
    let result = run_to_completion(child_spec("term-cleanup", "exit0"), &sink).await;

    assert_eq!(result.kind(), "OBSERVATION_FAILURE", "got {result:?}");
    assert!(!sink.has("cleanup_completed"));
    // The exit was recorded before the failing publish.
    assert!(sink.has("exited"), "obs: {:?}", sink.kinds());
}

// A sink that dies on `DescendantsRemaining` fails the worker — but the orphan is
// still cleaned up (cleanup does not depend on the sink surviving).
#[tokio::test]
async fn sink_failure_on_descendants_remaining_still_cleans_up() {
    let sink = RecordingSink::failing_on_kind("descendants_remaining");
    let result = run_to_completion(child_spec("term-desc", "grandchild_then_exit"), &sink).await;

    assert_eq!(result.kind(), "OBSERVATION_FAILURE", "got {result:?}");
    let pgid = sink.spawned_identity().unwrap().process_group;
    assert!(
        group_is_empty(pgid),
        "the orphan must be cleaned up even after the sink dies"
    );
}

// The critical case: a sink failing on a CANCELLATION observation must not let the
// worker return `CancelledGracefully`. It is an `ObservationFailure`.
#[tokio::test]
async fn sink_failure_during_cancellation_is_observation_failure_not_cancelled() {
    let sink = RecordingSink::failing_on_kind("graceful_stop_sent");
    let (handle, join) = start(child_spec("term-cancel", "sleep"), &sink);
    // Reach Running first, so cancellation goes through `run_cancellation` and
    // actually publishes `GracefulStopSent` (which the sink then fails on). A
    // cancel before spawn would never send it.
    tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    handle.cancel().await;
    let result = join.join().await;

    assert_eq!(
        result.kind(),
        "OBSERVATION_FAILURE",
        "a lost sink during cancel must not read as a graceful cancel: {result:?}"
    );
    let pgid = sink.spawned_identity().unwrap().process_group;
    assert!(group_is_empty(pgid), "the process must still be cleaned up");
}
