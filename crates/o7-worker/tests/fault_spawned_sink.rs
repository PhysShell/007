//! Gate re-gate blocker #5: a failed `Spawned` publish still owns a live process, so
//! it must go through VERIFIED cleanup — not a best-effort kill. If cleanup
//! verification or the force-stop ALSO fails, `CleanupFailure` (possible leaked
//! processes) dominates the `ObservationFailure`. If cleanup succeeds, it is an
//! `ObservationFailure`.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod common;

use common::mock::MockBoundary;
use common::*;

// Sink dies on Spawned AND the force-stop fails → cleanup unprovable → CleanupFailure.
#[tokio::test]
async fn spawned_sink_failure_with_force_stop_failure_is_cleanup_failure() {
    let boundary = MockBoundary::new().with_force_stop_error("SIGKILL delivery failed");
    let sink = RecordingSink::failing_on_kind("spawned");
    let result = run_with(child_spec("sp5a", "unused"), boundary.boxed(), &sink).await;

    assert_eq!(
        result.kind(),
        "CLEANUP_FAILURE",
        "an unprovable cleanup must dominate the sink failure: {result:?}"
    );
}

// Sink dies on Spawned AND the membership query fails → group unprovable → CleanupFailure.
#[tokio::test]
async fn spawned_sink_failure_with_unprovable_membership_is_cleanup_failure() {
    let boundary = MockBoundary::new().with_membership_error("/proc read failed");
    let sink = RecordingSink::failing_on_kind("spawned");
    let result = run_with(child_spec("sp5b", "unused"), boundary.boxed(), &sink).await;

    assert_eq!(result.kind(), "CLEANUP_FAILURE", "got {result:?}");
}

// Sink dies on Spawned but cleanup is provably clean → ObservationFailure (the process
// was still verified-cleaned, not leaked).
#[tokio::test]
async fn spawned_sink_failure_with_clean_cleanup_is_observation_failure() {
    let boundary = MockBoundary::new();
    let sink = RecordingSink::failing_on_kind("spawned");
    let result = run_with(child_spec("sp5c", "unused"), boundary.boxed(), &sink).await;

    assert_eq!(result.kind(), "OBSERVATION_FAILURE", "got {result:?}");
}
