//! Gate blocker #5: graceful-vs-forceful is decided by the whole owned SET, not
//! the leader. If the leader exits on SIGTERM but a same-group descendant ignores
//! it and only dies to SIGKILL, the result must be `CancelledForcefully` and the
//! group must end up empty — not a false `CancelledGracefully` just because the
//! leader went quietly.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod common;

use std::time::Duration;

use common::*;
use o7_worker::WorkerResult;

#[tokio::test]
async fn descendant_ignoring_sigterm_forces_the_group() {
    let sink = RecordingSink::new();
    let (handle, join) = start(
        child_spec("force5", "leader_dies_grandchild_ignores_sigterm"),
        &sink,
    );

    // Give the descendant time to install its SIGTERM-ignoring handler; otherwise
    // the default disposition would let it die gracefully (a race, not the
    // behaviour under test).
    tokio::time::sleep(Duration::from_millis(300)).await;
    handle.cancel().await;
    let result = join.join().await;

    assert_eq!(
        result,
        WorkerResult::CancelledForcefully,
        "leader went quietly but a descendant survived SIGTERM — must be forceful. obs: {:?}",
        sink.kinds()
    );
    assert!(
        sink.has("force_stop_sent"),
        "escalation to SIGKILL must be observed: {:?}",
        sink.kinds()
    );

    let pgid = sink.spawned_identity().unwrap().process_group;
    assert!(
        group_is_empty(pgid),
        "the whole owned group must be gone after a forceful cancel"
    );
}
