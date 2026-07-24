//! Acceptance 15: losing the authoritative observation sink is fatal — the worker
//! is cancelled, its process set cleaned up, and the terminal result reflects it.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod common;

use common::*;

// (15) A sink failure cancels the worker and cleans up.
#[tokio::test]
async fn sink_failure_cancels_and_cleans_up() {
    let sink = RecordingSink::failing_on_output();
    // The child prints (triggering the sink failure) and would otherwise sleep
    // forever — so the supervisor must actively kill it.
    let (handle, join) = start(child_spec("s15", "print_then_sleep"), &sink);
    let result = join.join().await;

    assert_eq!(result.kind(), "OBSERVATION_FAILURE", "got {result:?}");
    // The still-sleeping child must have been cleaned up.
    let identity = sink
        .spawned_identity()
        .expect("spawned recorded before the failure");
    assert!(
        group_is_empty(identity.process_group),
        "the worker process must be cleaned up after a sink failure"
    );
    drop(handle);
}
