//! Re-gate blocker: EVERY boundary control/query op used during cancellation and
//! cleanup is bounded — `request_graceful_stop`, `force_stop`, and `remaining_members`
//! each have a timeout, so a boundary whose op never resolves cannot hang the supervisor.
//!
//!   * a graceful-stop timeout escalates to force IMMEDIATELY;
//!   * a force-stop or membership timeout is an unprovable teardown → bounded
//!     `CleanupFailure`.
//!
//! Covered across the call sites the gate listed: run_cancellation (graceful, membership
//! drain), force_after_grace (force), the manage fault path (force), abandon_and_verify
//! (force, membership), and cleanup_group (membership, force).
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

const OUTER_BOUND: Duration = Duration::from_secs(8);

async fn cancel_after_running(
    boundary: MockBoundary,
    worker_id: &str,
    graceful_timeout: Duration,
    sink: &RecordingSink,
) -> o7_worker::WorkerResult {
    let mut spec = child_spec(worker_id, "unused");
    spec.cancellation.graceful_timeout = graceful_timeout;
    let (handle, join) = start_with(spec, boundary.boxed(), sink);
    tokio::time::sleep(Duration::from_millis(100)).await;
    tokio::time::timeout(OUTER_BOUND, handle.cancel())
        .await
        .expect("cancel must be bounded even with a hung boundary op");
    tokio::time::timeout(OUTER_BOUND, join.join())
        .await
        .expect("the terminal result must be produced within a bound")
}

// ---- run_cancellation: request_graceful_stop ----

#[tokio::test]
async fn pending_graceful_stop_escalates_immediately_not_after_grace() {
    // A LONG graceful timeout (10s): if the hung graceful stop were awaited, or the grace
    // were waited out, cancel would take ~10s. Bounding + immediate escalation makes it
    // fast, and the leader dies on the escalated force → CancelledForcefully.
    let boundary = MockBoundary::new().with_pending_graceful_stop();
    let state = boundary.state();
    let sink = RecordingSink::new();

    let result =
        cancel_after_running(boundary, "bop-graceful", Duration::from_secs(10), &sink).await;

    assert_eq!(
        result.kind(),
        "CANCELLED_FORCEFULLY",
        "a hung graceful stop must escalate to force immediately: {result:?}"
    );
    assert!(state.graceful_stops() >= 1, "graceful stop was attempted");
    assert!(state.force_stops() >= 1, "escalated to force");
    assert!(sink.attempted_kinds().contains(&"force_stop_sent"));
}

// ---- force_after_grace: force_stop ----

#[tokio::test]
async fn pending_force_stop_in_cancellation_is_bounded_cleanup_failure() {
    // Graceful succeeds, the leader survives the grace, escalation force-stops — and the
    // force hangs. Bounded → unprovable teardown → CleanupFailure.
    let boundary = MockBoundary::new()
        .with_live_leader()
        .with_pending_force_stop();
    let sink = RecordingSink::new();

    let result =
        cancel_after_running(boundary, "bop-force-cx", Duration::from_millis(200), &sink).await;

    assert_eq!(result.kind(), "CLEANUP_FAILURE", "got {result:?}");
}

// ---- run_cancellation: membership during graceful draining ----

#[tokio::test]
async fn pending_membership_during_graceful_drain_is_bounded_cleanup_failure() {
    // Leader exits at ~150ms (within the 500ms grace) so cancellation reaches the graceful
    // DRAIN loop; the membership query there hangs. Bounded → escalate → cleanup also
    // hangs on membership → bounded CleanupFailure.
    let boundary = MockBoundary::new()
        .with_leader_exit_after(Duration::from_millis(150))
        .with_pending_membership();
    let sink = RecordingSink::new();

    let result =
        cancel_after_running(boundary, "bop-mem-drain", Duration::from_millis(500), &sink).await;

    assert_eq!(result.kind(), "CLEANUP_FAILURE", "got {result:?}");
}

// ---- manage fault path: force_stop ----

#[tokio::test]
async fn pending_force_stop_in_manage_fault_path_is_bounded_cleanup_failure() {
    // A graceful-stop ERROR routes to the manage fault path (BoundaryFailed), whose
    // emergency force-stop then hangs. Bounded → CleanupFailure.
    let boundary = MockBoundary::new()
        .with_graceful_stop_error("SIGTERM delivery failed")
        .with_pending_force_stop();
    let sink = RecordingSink::new();

    let result = cancel_after_running(
        boundary,
        "bop-force-manage",
        Duration::from_millis(200),
        &sink,
    )
    .await;

    assert_eq!(result.kind(), "CLEANUP_FAILURE", "got {result:?}");
}

// ---- abandon_and_verify (failed Spawned publish): force_stop ----

#[tokio::test]
async fn pending_force_stop_after_failed_spawned_is_bounded_cleanup_failure() {
    let boundary = MockBoundary::new().with_pending_force_stop();
    let sink = RecordingSink::failing_on_kind("spawned");
    let result = tokio::time::timeout(
        OUTER_BOUND,
        run_with(
            child_spec("bop-force-sp", "unused"),
            boundary.boxed(),
            &sink,
        ),
    )
    .await
    .expect("a hung force in the abandon path must be bounded");
    assert_eq!(result.kind(), "CLEANUP_FAILURE", "got {result:?}");
}

// ---- abandon_and_verify (failed Spawned publish): membership ----

#[tokio::test]
async fn pending_membership_after_failed_spawned_is_bounded_cleanup_failure() {
    // Force succeeds, leader reaps immediately, but the verification membership hangs.
    let boundary = MockBoundary::new().with_pending_membership();
    let sink = RecordingSink::failing_on_kind("spawned");
    let result = tokio::time::timeout(
        OUTER_BOUND,
        run_with(child_spec("bop-mem-sp", "unused"), boundary.boxed(), &sink),
    )
    .await
    .expect("a hung membership in the abandon path must be bounded");
    assert_eq!(result.kind(), "CLEANUP_FAILURE", "got {result:?}");
}

// ---- cleanup_group: membership (Natural exit path) ----

#[tokio::test]
async fn pending_membership_in_cleanup_after_clean_exit_is_bounded_cleanup_failure() {
    // The leader exits cleanly (Natural), but the cleanup membership query hangs. A clean
    // exit must NOT mask an unprovable cleanup, and it must be bounded.
    let boundary = MockBoundary::new().with_pending_membership();
    let sink = RecordingSink::new();
    let result = tokio::time::timeout(
        OUTER_BOUND,
        run_with(
            child_spec("bop-mem-clean", "unused"),
            boundary.boxed(),
            &sink,
        ),
    )
    .await
    .expect("a hung cleanup membership must be bounded");
    assert_eq!(result.kind(), "CLEANUP_FAILURE", "got {result:?}");
}

// ---- cleanup_group: force_stop (present members that never drain) ----

#[tokio::test]
async fn pending_force_stop_in_cleanup_with_survivors_is_bounded_cleanup_failure() {
    // The leader exits cleanly but a descendant is still present, so cleanup_group reaches
    // its force-stop step — which hangs. Bounded → CleanupFailure.
    let boundary = MockBoundary::new()
        .with_present_members(1)
        .with_pending_force_stop();
    let sink = RecordingSink::new();
    let result = tokio::time::timeout(
        OUTER_BOUND,
        run_with(
            child_spec("bop-force-clean", "unused"),
            boundary.boxed(),
            &sink,
        ),
    )
    .await
    .expect("a hung cleanup force-stop must be bounded");
    assert_eq!(result.kind(), "CLEANUP_FAILURE", "got {result:?}");
}
