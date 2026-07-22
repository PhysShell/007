//! Re-gate blocker: every leader reap performed AFTER a force-stop must be BOUNDED.
//! A boundary whose `force_stop()` fails while the leader stays alive, or whose
//! `wait()` never completes even after a successful force-stop, must not hang the
//! supervisor forever — the promised failure terminal (`CleanupFailure`, an unprovable
//! teardown) must still be returned within a known bound.
//!
//! Two fault shapes × two contexts:
//!   * shapes: force-stop error + permanently pending leader; force-stop success +
//!     permanently pending `wait()`.
//!   * contexts: a failed `Spawned` publish (abandon-and-verify), and cancellation
//!     escalation (`run_cancellation` → `force_after_grace`).
//!
//! The mock deliberately does NOT wake `wait()` on a failed force-stop, so the bound
//! is what prevents the hang — not a helpful mock.
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

// If a reap were unbounded these would hang; the timeout catches that as a failure
// instead of an infinite test.
const OUTER_BOUND: Duration = Duration::from_secs(8);

// ---- context 1: failed `Spawned` publish → abandon_and_verify ----

#[tokio::test]
async fn spawned_publish_fail_with_force_error_and_pending_leader_is_bounded_cleanup_failure() {
    // The `Spawned` publish fails (owns a live process), force-stop errors, and the
    // leader's wait() never resolves. abandon_and_verify must bound its reap and return
    // CleanupFailure (unprovable teardown) — never hang.
    let boundary = MockBoundary::new()
        .with_force_stop_error("SIGKILL delivery failed")
        .with_pending_wait();
    let sink = RecordingSink::failing_on_kind("spawned");

    let result = tokio::time::timeout(
        OUTER_BOUND,
        run_with(child_spec("reap-1a", "unused"), boundary.boxed(), &sink),
    )
    .await
    .expect("a failed force + pending wait must NOT hang the abandon path");

    assert_eq!(result.kind(), "CLEANUP_FAILURE", "got {result:?}");
}

#[tokio::test]
async fn spawned_publish_fail_with_force_success_and_pending_wait_is_bounded_cleanup_failure() {
    // Force-stop SUCCEEDS but the leader's wait() never resolves — the reap cannot be
    // proven within the bound, so teardown is unprovable → CleanupFailure, bounded.
    let boundary = MockBoundary::new().with_pending_wait();
    let sink = RecordingSink::failing_on_kind("spawned");

    let result = tokio::time::timeout(
        OUTER_BOUND,
        run_with(child_spec("reap-1b", "unused"), boundary.boxed(), &sink),
    )
    .await
    .expect("a pending wait after a successful force must NOT hang the abandon path");

    assert_eq!(result.kind(), "CLEANUP_FAILURE", "got {result:?}");
}

// ---- context 2: cancellation escalation → force_after_grace + manage fault reap ----

async fn cancel_after_running(
    boundary: MockBoundary,
    worker_id: &str,
    sink: &RecordingSink,
) -> o7_worker::WorkerResult {
    // Short graceful timeout so escalation to force happens quickly.
    let mut spec = child_spec(worker_id, "unused");
    spec.cancellation.graceful_timeout = Duration::from_millis(200);
    let (handle, join) = start_with(spec, boundary.boxed(), sink);
    // Let the run loop reach Running (leader stays alive: wait() is pending).
    tokio::time::sleep(Duration::from_millis(100)).await;
    tokio::time::timeout(OUTER_BOUND, handle.cancel())
        .await
        .expect("cancel must be bounded even with a pending/failed reap");
    tokio::time::timeout(OUTER_BOUND, join.join())
        .await
        .expect("the terminal result must be produced within a bound")
}

#[tokio::test]
async fn cancellation_with_force_error_and_pending_leader_is_bounded_cleanup_failure() {
    // Cancel a live worker; graceful stop succeeds, the leader survives the grace,
    // escalation force-stops — which ERRORS — and the leader's wait() never resolves.
    let boundary = MockBoundary::new()
        .with_force_stop_error("SIGKILL delivery failed")
        .with_pending_wait();
    let sink = RecordingSink::new();

    let result = cancel_after_running(boundary, "reap-2a", &sink).await;

    assert_eq!(
        result.kind(),
        "CLEANUP_FAILURE",
        "a failed force during escalation is an unprovable teardown: {result:?}"
    );
    // The escalation to SIGKILL was published to the authoritative stream.
    assert!(
        sink.attempted_kinds().contains(&"force_stop_sent"),
        "attempted: {:?}",
        sink.attempted_kinds()
    );
}

#[tokio::test]
async fn cancellation_with_force_success_and_pending_wait_is_bounded_cleanup_failure() {
    // Graceful succeeds, leader survives the grace, force-stop SUCCEEDS, but the
    // leader's wait() never resolves — the reap times out (bounded) → CleanupFailure.
    let boundary = MockBoundary::new().with_pending_wait();
    let sink = RecordingSink::new();

    let result = cancel_after_running(boundary, "reap-2b", &sink).await;

    assert_eq!(
        result.kind(),
        "CLEANUP_FAILURE",
        "a pending reap after a successful force is an unprovable teardown: {result:?}"
    );
    assert!(
        sink.attempted_kinds().contains(&"force_stop_sent"),
        "attempted: {:?}",
        sink.attempted_kinds()
    );
}
