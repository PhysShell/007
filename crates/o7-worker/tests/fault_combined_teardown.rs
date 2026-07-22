//! Re-gate blocker: when several teardown operations fail, the terminal `CleanupFailure`
//! must COMPOSE every applicable fault (force, reap, then membership/cleanup) in
//! execution order — not keep only one. A single fault masking the others hides
//! diagnostic signal the reported guarantee promises to preserve.
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

/// Assert every fragment is present AND appears in the given order (execution order).
fn assert_ordered_fragments(msg: &str, fragments: &[&str]) {
    let mut search_from = 0usize;
    for fragment in fragments {
        match msg.get(search_from..).and_then(|tail| tail.find(fragment)) {
            Some(rel) => search_from += rel + fragment.len(),
            None => panic!(
                "expected fragment {fragment:?} at/after offset {search_from} in message: {msg:?}"
            ),
        }
    }
}

// abandon_and_verify (failed Spawned publish): force FAILS, reap TIMES OUT, and the
// verification membership FAILS. All three must appear, in execution order.
#[tokio::test]
async fn abandon_path_composes_force_reap_and_membership_faults() {
    let boundary = MockBoundary::new()
        .with_force_stop_error("SIGKILL delivery failed")
        .with_membership_error("proc read failed")
        .with_pending_wait();
    let sink = RecordingSink::failing_on_kind("spawned");

    let result = tokio::time::timeout(
        OUTER_BOUND,
        run_with(
            child_spec("comb-abandon", "unused"),
            boundary.boxed(),
            &sink,
        ),
    )
    .await
    .expect("must resolve within a bound");

    assert_eq!(result.kind(), "CLEANUP_FAILURE", "got {result:?}");
    let msg = message(&result);
    assert_ordered_fragments(
        &msg,
        &[
            "SIGKILL delivery failed", // the force fault
            "leader did not exit",     // the reap fault
            "proc read failed",        // the membership/cleanup fault
        ],
    );
}

// manage fault path (a graceful-stop ERROR → BoundaryFailed): the emergency force FAILS,
// the reap TIMES OUT, and cleanup membership FAILS. All three composed, in order.
#[tokio::test]
async fn manage_fault_path_composes_force_reap_and_membership_faults() {
    let boundary = MockBoundary::new()
        .with_graceful_stop_error("SIGTERM delivery failed")
        .with_force_stop_error("SIGKILL delivery failed")
        .with_membership_error("proc read failed")
        .with_pending_wait();
    let sink = RecordingSink::new();

    let mut spec = child_spec("comb-manage", "unused");
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
    assert_ordered_fragments(
        &msg,
        &[
            "SIGKILL delivery failed", // the force fault
            "leader did not exit",     // the reap fault
            "proc read failed",        // the membership/cleanup fault
        ],
    );
}
