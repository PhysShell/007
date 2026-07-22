//! Re-gate blocker: a membership query error/timeout during the GRACEFUL DRAIN loop must
//! not vanish into a clean `CancelledForcefully`. The authoritative membership mechanism
//! failed, so the fault is preserved: routed through the manage fault path, it becomes a
//! `BoundaryFailure` when cleanup later recovers, or a composed `CleanupFailure` when
//! cleanup stays unprovable.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod common;

use std::time::Duration;

use common::mock::MockBoundary;
use common::*;
use o7_worker::WorkerResult;

const OUTER_BOUND: Duration = Duration::from_secs(8);

fn message(result: &WorkerResult) -> String {
    match result {
        WorkerResult::CleanupFailure(m)
        | WorkerResult::BoundaryFailure(m)
        | WorkerResult::ObservationFailure(m)
        | WorkerResult::OutputFailure(m)
        | WorkerResult::FailedToStart(m) => m.clone(),
        other => format!("{other:?}"),
    }
}

async fn cancel_then_join(
    boundary: MockBoundary,
    worker_id: &str,
    sink: &RecordingSink,
) -> WorkerResult {
    let mut spec = child_spec(worker_id, "unused");
    spec.cancellation.graceful_timeout = Duration::from_millis(500);
    let (handle, join) = start_with(spec, boundary.boxed(), sink);
    // Cancel BEFORE the leader's ~150ms exit deadline, so cancellation runs and then the
    // leader exits within the grace → the graceful DRAIN loop is reached.
    tokio::time::sleep(Duration::from_millis(60)).await;
    tokio::time::timeout(OUTER_BOUND, handle.cancel())
        .await
        .expect("cancel bounded");
    tokio::time::timeout(OUTER_BOUND, join.join())
        .await
        .expect("terminal bounded")
}

// TRANSIENT membership failure during graceful drain, then cleanup RECOVERS (empty):
// the terminal is BoundaryFailure preserving the membership fault — NOT a clean cancel.
#[tokio::test]
async fn transient_drain_membership_failure_then_recovery_is_boundary_failure_not_clean_cancel() {
    let boundary = MockBoundary::new()
        .with_leader_exit_after(Duration::from_millis(150))
        .with_membership_error_then_empty("transient proc failure");
    let sink = RecordingSink::new();

    let result = cancel_then_join(boundary, "drain-recover", &sink).await;

    assert_eq!(
        result.kind(),
        "BOUNDARY_FAILURE",
        "a failed drain membership query must not read as a clean cancel: {result:?}"
    );
    assert!(
        !matches!(
            result,
            WorkerResult::CancelledForcefully | WorkerResult::CancelledGracefully
        ),
        "must never be a clean cancellation: {result:?}"
    );
    let msg = message(&result);
    assert!(
        msg.contains("transient proc failure") && msg.contains("graceful drain"),
        "the original membership fault must be preserved: {msg:?}"
    );
}

// PERSISTENT membership failure during graceful drain AND cleanup: the terminal is a
// CleanupFailure that COMPOSES both the drain membership fault and the cleanup fault.
#[tokio::test]
async fn persistent_drain_membership_failure_is_cleanup_failure_composing_both() {
    let boundary = MockBoundary::new()
        .with_leader_exit_after(Duration::from_millis(150))
        .with_membership_error("proc read failed");
    let sink = RecordingSink::new();

    let result = cancel_then_join(boundary, "drain-persist", &sink).await;

    assert_eq!(
        result.kind(),
        "CLEANUP_FAILURE",
        "an unprovable drain+cleanup membership must be CleanupFailure: {result:?}"
    );
    let msg = message(&result);
    // The primary drain fault (marked "graceful drain") AND the later cleanup fault are
    // both present — the membership error string appears at least twice.
    assert!(
        msg.contains("graceful drain"),
        "the drain membership fault must be preserved: {msg:?}"
    );
    assert!(
        msg.matches("proc read failed").count() >= 2,
        "both the drain and cleanup membership faults must be composed: {msg:?}"
    );
}
