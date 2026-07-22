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
use o7_worker::{ProcessIdentity, WorkerResult};

fn is_cancelled(result: &WorkerResult) -> bool {
    matches!(
        result,
        WorkerResult::CancelledGracefully | WorkerResult::CancelledForcefully
    )
}

// (16) cancel() is idempotent.
#[tokio::test]
async fn cancel_is_idempotent() {
    let sink = RecordingSink::new();
    let (handle, join) = start(child_spec("c16", "sleep"), &sink);
    handle.cancel().await;
    handle.cancel().await; // second time is a no-op that still resolves
    let result = join.join().await;
    assert!(is_cancelled(&result), "got {result:?}");
    assert!(sink.count("cleanup_completed") <= 1);
}

// (17) Several concurrent cancels yield exactly one terminal result.
#[tokio::test]
async fn concurrent_cancels_yield_one_result() {
    let sink = RecordingSink::new();
    let (handle, join) = start(child_spec("c17", "sleep"), &sink);
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
    let (handle, join) = start(child_spec("c19", "ignore_sigterm"), &sink);
    // Let the child install its SIGTERM handler before we signal it, otherwise
    // the default SIGTERM disposition would kill it gracefully (a race, not the
    // behaviour under test).
    tokio::time::sleep(Duration::from_millis(250)).await;
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
            ProcessIdentity::enumerate_group(identity.process_group).is_empty(),
            "no owned process may survive"
        );
    }
}
