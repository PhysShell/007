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

use std::time::Duration;

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

// (34) The direct child is reaped — its `/proc/<pid>` entry is GONE, not a zombie.
#[tokio::test]
async fn no_zombie_direct_child() {
    let sink = RecordingSink::new();
    let result = run_to_completion(child_spec("orph34", "exit0"), &sink).await;
    assert_eq!(result.kind(), "EXITED_NORMALLY");

    let identity = sink.spawned_identity().unwrap();
    // Prove reaping with a RAW `/proc/<pid>` check, NOT the live-members scan: the scan
    // treats a zombie as gone, so it could not tell "reaped" from "still a zombie". A
    // truly reaped direct child has no `/proc/<pid>` entry at all.
    assert!(
        proc_pid_gone_within(identity.pid, Duration::from_secs(2)).await,
        "the direct child's /proc/{} entry must disappear (reaped, not zombified)",
        identity.pid
    );
    // And the owned group is empty by the authoritative membership scan.
    assert!(group_is_empty(identity.process_group));
}
