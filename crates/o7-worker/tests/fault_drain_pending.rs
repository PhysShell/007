//! Re-gate blocker: the trailing-output drain must be BOUNDED. `out_rx.recv()` only
//! returns `None` once every reader task ends, which needs the pipes to close. A
//! descendant that escaped the owned group can hold an inherited pipe open forever,
//! so an unbounded drain would hang and the promised failure terminal would never be
//! produced. Two faults are covered:
//!
//!   1. cleanup FAILS (membership error) AND a stream is permanently pending — the
//!      supervisor must not wait on pipe closure; `CleanupFailure` dominates and is
//!      returned promptly.
//!   2. cleanup verifies the original group EMPTY but a stream is permanently pending
//!      — the bounded drain times out, which is itself a failure (never a clean
//!      `ExitedNormally`).
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

// A generous outer bound: the supervisor's own drain grace is ~500ms, so any of these
// runs must finish well under this. If the drain were unbounded these would hang.
const OUTER_BOUND: Duration = Duration::from_secs(8);

#[tokio::test]
async fn membership_error_with_pending_stream_is_cleanup_failure_not_a_hang() {
    // Leader exits code 0, every membership query errors, and stdout never closes.
    let boundary = MockBoundary::new()
        .with_membership_error("simulated /proc read failure")
        .with_pending_stdout();
    let state = boundary.state();
    let sink = RecordingSink::new();

    let result = tokio::time::timeout(
        OUTER_BOUND,
        run_with(
            child_spec("drain-pend-a", "unused"),
            boundary.boxed(),
            &sink,
        ),
    )
    .await
    .expect("cleanup failure must be returned promptly, never blocked on a pending pipe");

    assert_eq!(
        result.kind(),
        "CLEANUP_FAILURE",
        "an unprovable cleanup must dominate and must not wait on a pending pipe: {result:?}"
    );
    assert!(
        !matches!(result, WorkerResult::ExitedNormally(_)),
        "a clean exit must not mask an unprovable cleanup: {result:?}"
    );
    assert!(
        state.membership_queries() >= 1,
        "cleanup must actually query the set"
    );
}

#[tokio::test]
async fn verified_empty_group_with_pending_stream_times_out_as_output_failure() {
    // Leader exits code 0, the ORIGINAL owned group verifies empty (default mock), but
    // stdout is held open by a (modelled) escaped descendant, so the drain cannot see
    // EOF. The bounded drain must time out — a failure, never a clean exit.
    let boundary = MockBoundary::new().with_pending_stdout();
    let sink = RecordingSink::new();

    let result = tokio::time::timeout(
        OUTER_BOUND,
        run_with(
            child_spec("drain-pend-b", "unused"),
            boundary.boxed(),
            &sink,
        ),
    )
    .await
    .expect("a bounded drain must terminate; an unbounded one would hang here");

    assert_eq!(
        result.kind(),
        "OUTPUT_FAILURE",
        "a drain that never sees pipe closure must fail, never pass clean: {result:?}"
    );
    assert!(
        !matches!(result, WorkerResult::ExitedNormally(_)),
        "the drain timeout must not read as a clean exit: {result:?}"
    );
    // The sink is still alive, so the supervisor announces its own failure.
    assert!(sink.has("supervisor_failed"), "obs: {:?}", sink.kinds());
}
