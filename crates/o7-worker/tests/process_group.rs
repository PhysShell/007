//! Acceptance 20, 22: the whole process GROUP is owned; the terminal completion
//! is not declared until the group is gone.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod common;

use common::*;
use o7_worker::ProcessIdentity;

// (20) A grandchild in the leader's group is terminated together with the leader.
#[tokio::test]
async fn grandchild_is_killed_with_the_group() {
    let sink = RecordingSink::new();
    let (handle, join) = start(child_spec("pg20", "grandchild_then_sleep"), &sink);
    // Let the grandchild appear.
    tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    handle.cancel().await;
    let result = join.join().await;

    assert_ne!(
        result.kind(),
        "CLEANUP_FAILURE",
        "group must be gone: {result:?}"
    );
    let pgid = sink.spawned_identity().unwrap().process_group;
    assert!(
        ProcessIdentity::enumerate_group(pgid).is_empty(),
        "no group member may survive cancellation"
    );
}

// (22) The terminal completion is only published after the group is confirmed gone.
#[tokio::test]
async fn terminal_not_published_until_group_gone() {
    let sink = RecordingSink::new();
    // Leader exits immediately, leaving a grandchild that the supervisor must clean.
    let result = run_to_completion(child_spec("pg22", "grandchild_then_exit"), &sink).await;
    assert_eq!(result.kind(), "EXITED_NORMALLY", "got {result:?}");

    let kinds = sink.kinds();
    assert!(kinds.contains(&"descendants_remaining"), "obs: {kinds:?}");
    // CleanupCompleted is the final lifecycle observation, emitted after the group
    // is confirmed empty.
    assert_eq!(kinds.last(), Some(&"cleanup_completed"), "obs: {kinds:?}");

    let pgid = sink.spawned_identity().unwrap().process_group;
    assert!(ProcessIdentity::enumerate_group(pgid).is_empty());
}
