//! Gate blocker #6: `Starting` cancellation must be a real, bounded boundary
//! contract. A slow/hung `boundary.spawn()` must not make `cancel()` wait for it,
//! and dropping the spawn future (the cancel-safety contract) must not leave a
//! process ownerless.
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
async fn cancel_during_slow_spawn_is_bounded_and_leak_free() {
    // A boundary whose spawn takes "forever". If cancellation waited on the spawn
    // future, this test would hang for 30s.
    let boundary = MockBoundary::new().with_spawn_delay(Duration::from_secs(30));
    let state = boundary.state();
    let sink = RecordingSink::new();

    let (handle, join) = start_with(child_spec("slow-spawn", "unused"), boundary.boxed(), &sink);

    // Let the supervisor pass its pre-spawn checks and actually enter the spawn
    // future, so the cancel below lands DURING the spawn (not before it).
    tokio::time::sleep(Duration::from_millis(150)).await;

    // Both the cancel and the join must resolve promptly — far under the 30s spawn.
    tokio::time::timeout(Duration::from_secs(5), handle.cancel())
        .await
        .expect("cancel must not wait for the hung spawn");
    let result = tokio::time::timeout(Duration::from_secs(5), join.join())
        .await
        .expect("join must not wait for the hung spawn");

    assert_eq!(result, WorkerResult::CancelledGracefully, "got {result:?}");

    // The spawn future ran, was cancelled before it produced a process, and its
    // Drop performed the cancel-safe cleanup — so nothing was left ownerless.
    assert!(state.entered_spawn(), "the spawn future must have started");
    assert!(
        !state.committed_spawn(),
        "a cancelled slow spawn must not have handed over a process"
    );
    assert!(
        state.dropped_before_commit(),
        "dropping the spawn future must clean up (no ownerless process)"
    );

    // Nothing was ever `Spawned`, so the run never entered `Running`.
    assert!(!sink.has("spawned"), "obs: {:?}", sink.kinds());
    assert!(
        sink.has("cancellation_requested"),
        "obs: {:?}",
        sink.kinds()
    );
}
