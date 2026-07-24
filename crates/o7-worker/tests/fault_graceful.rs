//! Gate re-gate blocker #2: a FAILED graceful stop must not wait the grace period
//! and then return `CancelledGracefully`. It force-closes immediately, preserves the
//! boundary fault in the terminal result, and still verifies the group is gone.
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

#[tokio::test]
async fn failed_graceful_stop_forces_immediately_and_preserves_fault() {
    let boundary = MockBoundary::new().with_graceful_stop_error("SIGTERM delivery failed");
    let state = boundary.state();
    let sink = RecordingSink::new();

    // A LONG grace: if cancellation waited it out instead of force-closing on the
    // graceful-stop error, the cancel below would block for 10s.
    let mut spec = child_spec("gstop", "unused");
    spec.cancellation.graceful_timeout = Duration::from_secs(10);

    let (handle, join) = start_with(spec, boundary.boxed(), &sink);
    // Reach Running (the leader is live until force-stopped).
    tokio::time::sleep(Duration::from_millis(150)).await;

    tokio::time::timeout(Duration::from_secs(3), handle.cancel())
        .await
        .expect("cancel must NOT wait the graceful interval after a failed graceful stop");
    let result = join.join().await;

    assert_eq!(result.kind(), "BOUNDARY_FAILURE", "got {result:?}");
    assert!(
        !matches!(result, WorkerResult::CancelledGracefully),
        "a failed graceful stop must never read as a graceful cancel: {result:?}"
    );
    // It escalated to force and (via the empty mock set) proved the group gone.
    assert!(
        state.force_stops() >= 1,
        "must force-stop after a failed graceful stop"
    );
    assert!(sink.has("supervisor_failed"), "obs: {:?}", sink.kinds());
}
