//! Acceptance 16-19, 24: cancellation is idempotent, concurrency-safe, escalates
//! SIGTERM→SIGKILL, and cannot lose a process started mid-cancel.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod common;

use std::sync::Arc;
use std::time::Duration;

use common::*;
use o7_worker::WorkerResult;

/// Generous but bounded ceiling for readiness waits — a spawn/marker arrives in
/// milliseconds; this only guards against a genuine hang, never a timing assumption.
const READY_TIMEOUT: Duration = Duration::from_secs(10);

fn is_cancelled(result: &WorkerResult) -> bool {
    matches!(
        result,
        WorkerResult::CancelledGracefully | WorkerResult::CancelledForcefully
    )
}

// (16) cancel() is idempotent on a STARTED worker: cancelling a running process twice
// still yields one cancellation and exactly one completed cleanup. (The no-process
// cancel paths are covered by the cancel-before/during-spawn tests, so this test does
// not need to preserve that ambiguity.)
#[tokio::test]
async fn cancel_is_idempotent() {
    let sink = RecordingSink::new();
    let (handle, join) = start(child_spec("c16", "sleep"), &sink);
    // Wait until the worker has actually started before cancelling.
    sink.wait_for_kind_count("spawned", 1, READY_TIMEOUT)
        .await
        .unwrap();
    handle.cancel().await;
    handle.cancel().await; // second time is a no-op that still resolves
    let result = join.join().await;
    assert!(is_cancelled(&result), "got {result:?}");
    assert_eq!(
        sink.count("cleanup_completed"),
        1,
        "exactly one cleanup_completed, obs: {:?}",
        sink.kinds()
    );
}

// (17) Several concurrent cancels yield exactly one terminal result.
#[tokio::test]
async fn concurrent_cancels_yield_one_result() {
    let sink = RecordingSink::new();
    let (handle, join) = start(child_spec("c17", "sleep"), &sink);
    // Reach Running first, so the concurrent cancels all race the SAME live process
    // (and produce exactly one CleanupCompleted). Cancelling before spawn is a
    // distinct path with no cleanup to complete — covered by (24).
    sink.wait_for_kind_count("spawned", 1, READY_TIMEOUT)
        .await
        .unwrap();
    let handle = Arc::new(handle);
    let mut tasks = Vec::new();
    for _ in 0..5 {
        let h = Arc::clone(&handle);
        tasks.push(tokio::spawn(async move { h.cancel().await }));
    }
    for t in tasks {
        t.await.unwrap();
    }
    let result = join.join().await;
    assert!(is_cancelled(&result), "got {result:?}");
    assert_eq!(sink.count("cleanup_completed"), 1, "exactly one terminal");
}

// (18) A cooperative process stops after SIGTERM (gracefully).
#[tokio::test]
async fn cooperative_process_stops_gracefully() {
    let sink = RecordingSink::new();
    let (handle, join) = start(child_spec("c18", "sleep"), &sink);
    // Wait for the spawn: this test exercises the graceful SIGTERM path of a LIVE
    // process. Cancelling before spawn is a separate (also-graceful) path with nothing
    // to signal — covered by (24).
    sink.wait_for_kind_count("spawned", 1, READY_TIMEOUT)
        .await
        .unwrap();
    handle.cancel().await;
    let result = join.join().await;
    assert_eq!(
        result,
        WorkerResult::CancelledGracefully,
        "obs: {:?}",
        sink.kinds()
    );
    assert!(sink.has("graceful_stop_sent"));
    assert!(!sink.has("force_stop_sent"));
}

// (19) A process that ignores SIGTERM is force-killed after the grace period.
#[tokio::test]
async fn sigterm_ignoring_process_is_force_killed() {
    let sink = RecordingSink::new();
    let (handle, join) = start(child_spec("c19", "ignore_sigterm_ready"), &sink);
    // Wait for the child's explicit readiness marker, emitted only AFTER it installs its
    // SIGTERM handler — otherwise the default SIGTERM disposition would kill it
    // gracefully (a race, not the behaviour under test).
    sink.wait_for_stdout_contains(READY_SIGTERM_HANDLER, READY_TIMEOUT)
        .await
        .unwrap();
    handle.cancel().await;
    let result = join.join().await;
    assert_eq!(
        result,
        WorkerResult::CancelledForcefully,
        "obs: {:?}",
        sink.kinds()
    );
    assert!(sink.has("force_stop_sent"));
}

// (24) Cancelling immediately does not leave a lost/leaked process.
#[tokio::test]
async fn cancel_during_start_leaves_no_lost_process() {
    let sink = RecordingSink::new();
    let (handle, join) = start(child_spec("c24", "sleep"), &sink);
    handle.cancel().await; // as early as we can
    let result = join.join().await;
    assert!(
        is_cancelled(&result) || matches!(result, WorkerResult::ExitedBySignal(_)),
        "got {result:?}"
    );
    if let Some(identity) = sink.spawned_identity() {
        assert!(
            group_is_empty(identity.process_group),
            "no owned process may survive"
        );
    }
}
