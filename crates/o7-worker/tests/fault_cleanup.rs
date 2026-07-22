//! Gate blocker #1: cleanup must fail CLOSED. A boundary whose membership query
//! fails can never prove the owned set is gone, so the supervisor must yield
//! `CleanupFailure` — never `CleanupCompleted`, never a success — even though the
//! leader itself exited cleanly.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod common;

use common::mock::MockBoundary;
use common::*;
use o7_worker::WorkerResult;

#[tokio::test]
async fn membership_query_failure_is_a_cleanup_failure_not_success() {
    // Leader exits with code 0, but every membership query errors.
    let boundary = MockBoundary::new().with_membership_error("simulated /proc read failure");
    let state = boundary.state();
    let sink = RecordingSink::new();

    let result = run_with(child_spec("clean-fail", "unused"), boundary.boxed(), &sink).await;

    // The clean exit must NOT be reported as success: an unprovable cleanup wins.
    assert_eq!(result.kind(), "CLEANUP_FAILURE", "got {result:?}");
    assert!(
        !matches!(result, WorkerResult::ExitedNormally(_)),
        "a clean exit must not mask an unprovable cleanup: {result:?}"
    );

    // The membership query was actually attempted (and failed) — not skipped.
    assert!(
        state.membership_queries() >= 1,
        "cleanup must query the set"
    );

    let kinds = sink.kinds();
    assert!(
        !kinds.contains(&"cleanup_completed"),
        "cleanup must never be declared complete when it could not be proven: {kinds:?}"
    );
    // The sink is still alive, so the supervisor announces its own failure.
    assert!(
        kinds.contains(&"supervisor_failed"),
        "a live sink must be told the supervisor failed: {kinds:?}"
    );
}
