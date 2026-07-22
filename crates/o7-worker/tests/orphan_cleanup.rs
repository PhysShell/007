//! Acceptance 21, 33, 34: in-supervisor descendant cleanup, no leaked owned
//! descendants, no zombie direct child. (Post-daemon-crash orphan RECOVERY is
//! explicitly out of scope for PR 2 — see docs/architecture/worker-lifecycle.md.)
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod common;

use common::*;

// (21) The leader exits but a grandchild remains: the supervisor detects and
// cleans it.
#[tokio::test]
async fn supervisor_detects_and_cleans_remaining_grandchild() {
    let sink = RecordingSink::new();
    let result = run_to_completion(child_spec("orph21", "grandchild_then_exit"), &sink).await;
    assert_eq!(result.kind(), "EXITED_NORMALLY", "got {result:?}");
    assert!(
        sink.has("descendants_remaining"),
        "supervisor must notice the orphan"
    );

    let pgid = sink.spawned_identity().unwrap().process_group;
    assert!(group_is_empty(pgid), "the orphan must be cleaned up");
}

// (33) A normal run leaves no live owned descendants.
#[tokio::test]
async fn no_live_owned_descendants_after_run() {
    let sink = RecordingSink::new();
    run_to_completion(child_spec("orph33", "grandchild_then_exit"), &sink).await;
    let pgid = sink.spawned_identity().unwrap().process_group;
    assert!(group_is_empty(pgid));
}

// (34) The direct child is reaped — no zombie remains in the owned group.
#[tokio::test]
async fn no_zombie_direct_child() {
    let sink = RecordingSink::new();
    let result = run_to_completion(child_spec("orph34", "exit0"), &sink).await;
    assert_eq!(result.kind(), "EXITED_NORMALLY");
    // A zombie would still appear in /proc with the leader's pgid; the group being
    // empty proves the direct child was reaped.
    let pgid = sink.spawned_identity().unwrap().process_group;
    assert!(group_is_empty(pgid));
}
