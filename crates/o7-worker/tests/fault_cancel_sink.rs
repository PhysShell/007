//! Gate re-gate blocker #4: cancel-before-spawn and cancel-during-spawn must honor a
//! failed authoritative sink. A lost sink on `CancellationRequested` is an
//! `ObservationFailure`, never a `CancelledGracefully`.
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

// Cancel BEFORE spawn: on the single-threaded test runtime the cancel request is set
// before the supervisor reaches its pre-spawn check, so the cancel is observed there.
// The sink fails on that observation.
#[tokio::test]
async fn cancel_before_spawn_sink_failure_is_observation_failure() {
    let sink = RecordingSink::failing_on_kind("cancellation_requested");
    let (handle, join) = start(child_spec("cbs", "sleep"), &sink);
    handle.cancel().await;
    let result = join.join().await;

    assert_eq!(result.kind(), "OBSERVATION_FAILURE", "got {result:?}");
    assert!(
        !sink.has("spawned"),
        "nothing may be spawned: {:?}",
        sink.kinds()
    );
}

// Cancel DURING spawn: a slow mock spawn means the cancel lands in the spawn `select`.
// The sink fails on the cancellation observation → ObservationFailure, and the spawn
// future is dropped without committing a process.
#[tokio::test]
async fn cancel_during_spawn_sink_failure_is_observation_failure() {
    let boundary = MockBoundary::new().with_spawn_delay(Duration::from_secs(30));
    let state = boundary.state();
    let sink = RecordingSink::failing_on_kind("cancellation_requested");

    let (handle, join) = start_with(child_spec("cds", "unused"), boundary.boxed(), &sink);
    tokio::time::sleep(Duration::from_millis(150)).await;
    tokio::time::timeout(Duration::from_secs(5), handle.cancel())
        .await
        .expect("cancel must be bounded");
    let result = join.join().await;

    assert_eq!(result.kind(), "OBSERVATION_FAILURE", "got {result:?}");
    assert!(
        !state.committed_spawn(),
        "a cancelled slow spawn must not have committed a process"
    );
    assert!(!sink.has("spawned"), "obs: {:?}", sink.kinds());
}
