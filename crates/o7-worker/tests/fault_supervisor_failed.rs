//! Re-gate blocker: the sink is authoritative through the FINAL publication. The
//! terminal result must be computed AFTER the `SupervisorFailed` publish, so a sink lost
//! on THAT publish still becomes an `ObservationFailure` (preserving the run/output/
//! boundary fault), and when `CleanupFailure` dominates the concurrently-lost sink is
//! preserved too. The primary sink fault on a failed `Spawned` publish is likewise kept.
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

const OUTER_BOUND: Duration = Duration::from_secs(8);

fn message(result: &WorkerResult) -> String {
    match result {
        WorkerResult::CleanupFailure(m)
        | WorkerResult::BoundaryFailure(m)
        | WorkerResult::ObservationFailure(m)
        | WorkerResult::OutputFailure(m)
        | WorkerResult::FailedToStart(m) => m.clone(),
        other => format!("{other:?}"),
    }
}

fn assert_ordered(msg: &str, fragments: &[&str]) {
    let mut from = 0usize;
    for f in fragments {
        match msg.get(from..).and_then(|t| t.find(f)) {
            Some(rel) => from += rel + f.len(),
            None => panic!("missing fragment {f:?} at/after {from} in {msg:?}"),
        }
    }
}

// (1) OUTPUT fault + sink fails on SupervisorFailed → ObservationFailure preserving the
// output fault. The sink loss happens on the FINAL publish, after the result is first
// computed — the old code returned OutputFailure and ignored it.
#[tokio::test]
async fn output_fault_then_sink_fails_on_supervisor_failed_is_observation_failure() {
    let boundary = MockBoundary::new()
        .with_stdout_then_read_error(vec![b"x".to_vec()], "injected output error");
    let sink = RecordingSink::failing_on_kind("supervisor_failed");

    let result = tokio::time::timeout(
        OUTER_BOUND,
        run_with(child_spec("sf-output", "unused"), boundary.boxed(), &sink),
    )
    .await
    .expect("must resolve within a bound");

    assert_eq!(result.kind(), "OBSERVATION_FAILURE", "got {result:?}");
    assert!(
        message(&result).contains("injected output error"),
        "the output fault must be preserved: {}",
        message(&result)
    );
    assert!(sink.attempted_kinds().contains(&"supervisor_failed"));
}

// (2) BOUNDARY fault + sink fails on SupervisorFailed → ObservationFailure preserving it.
#[tokio::test]
async fn boundary_fault_then_sink_fails_on_supervisor_failed_is_observation_failure() {
    let boundary = MockBoundary::new().with_graceful_stop_error("SIGTERM delivery failed");
    let sink = RecordingSink::failing_on_kind("supervisor_failed");

    let mut spec = child_spec("sf-boundary", "unused");
    spec.cancellation.graceful_timeout = Duration::from_millis(200);
    let (handle, join) = start_with(spec, boundary.boxed(), &sink);
    tokio::time::sleep(Duration::from_millis(100)).await;
    tokio::time::timeout(OUTER_BOUND, handle.cancel())
        .await
        .expect("cancel bounded");
    let result = tokio::time::timeout(OUTER_BOUND, join.join())
        .await
        .expect("terminal bounded");

    assert_eq!(result.kind(), "OBSERVATION_FAILURE", "got {result:?}");
    assert!(
        message(&result).contains("SIGTERM delivery failed"),
        "the boundary fault must be preserved: {}",
        message(&result)
    );
}

// (3) CLEANUP failure + sink fails on SupervisorFailed → CleanupFailure containing BOTH.
#[tokio::test]
async fn cleanup_failure_then_sink_fails_on_supervisor_failed_contains_both() {
    let boundary = MockBoundary::new().with_membership_error("proc read failed");
    let sink = RecordingSink::failing_on_kind("supervisor_failed");

    let result = tokio::time::timeout(
        OUTER_BOUND,
        run_with(child_spec("sf-cleanup", "unused"), boundary.boxed(), &sink),
    )
    .await
    .expect("must resolve within a bound");

    assert_eq!(result.kind(), "CLEANUP_FAILURE", "got {result:?}");
    let msg = message(&result);
    assert!(
        msg.contains("proc read failed"),
        "cleanup fault preserved: {msg}"
    );
    assert!(
        msg.contains("observation sink lost"),
        "the sink loss on SupervisorFailed must be preserved: {msg}"
    );
}

// (4) Failed Spawned publish + cleanup failure → primary sink fault preserved FIRST.
#[tokio::test]
async fn failed_spawned_publish_plus_cleanup_failure_preserves_primary_sink_fault_first() {
    let boundary = MockBoundary::new().with_membership_error("proc read failed");
    let sink = RecordingSink::failing_on_kind("spawned");

    let result = tokio::time::timeout(
        OUTER_BOUND,
        run_with(child_spec("sf-spawn", "unused"), boundary.boxed(), &sink),
    )
    .await
    .expect("must resolve within a bound");

    assert_eq!(result.kind(), "CLEANUP_FAILURE", "got {result:?}");
    assert_ordered(
        &message(&result),
        &["primary sink fault", "proc read failed"],
    );
}

// (5) Sink failure on a cancellation/cleanup observation + cleanup failure → both kept.
// The sink dies on `GracefulStopSent`; a descendant never drains, so cleanup fails; the
// terminal CleanupFailure preserves the survivor fault AND the lost-sink fault.
#[tokio::test]
async fn cancellation_observation_sink_loss_plus_cleanup_failure_preserves_both() {
    let boundary = MockBoundary::new()
        .with_live_leader()
        .with_present_members(1);
    let sink = RecordingSink::failing_on_kind("graceful_stop_sent");

    let mut spec = child_spec("sf-cancel-obs", "unused");
    spec.cancellation.graceful_timeout = Duration::from_millis(200);
    let (handle, join) = start_with(spec, boundary.boxed(), &sink);
    tokio::time::sleep(Duration::from_millis(100)).await;
    tokio::time::timeout(OUTER_BOUND, handle.cancel())
        .await
        .expect("cancel bounded");
    let result = tokio::time::timeout(OUTER_BOUND, join.join())
        .await
        .expect("terminal bounded");

    assert_eq!(result.kind(), "CLEANUP_FAILURE", "got {result:?}");
    let msg = message(&result);
    assert!(
        msg.contains("survived cleanup"),
        "cleanup (survivor) fault preserved: {msg}"
    );
    assert!(
        msg.contains("observation sink lost"),
        "the cancellation-observation sink loss must be preserved: {msg}"
    );
    assert!(sink.attempted_kinds().contains(&"graceful_stop_sent"));
}
