//! Re-gate blocker: the EMERGENCY force-stop taken on a generic fault path (e.g. a
//! failed graceful stop) is a real teardown action, so it must be published to the
//! authoritative stream as `ForceStopSent` BEFORE it happens — never an invisible
//! SIGKILL. This test drives a failed graceful stop (→ `BoundaryFailure`) and makes
//! the sink die on that emergency `force_stop_sent` observation. Combined-fault
//! precedence must then hold: `ObservationFailure` dominates the boundary fault, and
//! the ObservationFailure message must PRESERVE the original boundary fault.
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
use o7_worker::{WorkerObservation, WorkerResult};

#[tokio::test]
async fn sink_failure_on_emergency_force_stop_is_observation_failure_preserving_boundary_fault() {
    // Graceful stop fails → the fault path force-stops as an emergency. The sink is
    // rigged to die exactly on that emergency `force_stop_sent` observation.
    let boundary = MockBoundary::new().with_graceful_stop_error("SIGTERM delivery failed");
    let state = boundary.state();
    let sink = RecordingSink::failing_on_kind("force_stop_sent");

    let mut spec = child_spec("fss", "unused");
    spec.cancellation.graceful_timeout = Duration::from_secs(10);

    let (handle, join) = start_with(spec, boundary.boxed(), &sink);
    // Reach Running (the leader is live until force-stopped).
    tokio::time::sleep(Duration::from_millis(150)).await;

    tokio::time::timeout(Duration::from_secs(3), handle.cancel())
        .await
        .expect("cancel must be bounded even when the emergency force-stop obs sink dies");
    let result = join.join().await;

    // Precedence: CleanupFailure > ObservationFailure > Boundary/Output. Cleanup proves
    // the (empty mock) set gone, so the lost sink dominates the boundary fault.
    assert_eq!(
        result.kind(),
        "OBSERVATION_FAILURE",
        "a lost sink on the emergency force-stop obs must dominate the boundary fault: {result:?}"
    );
    // The original boundary fault must not be erased — it is preserved in the message.
    match &result {
        WorkerResult::ObservationFailure(message) => assert!(
            message.contains("graceful stop failed"),
            "the ObservationFailure must preserve the underlying boundary fault: {message:?}"
        ),
        other => panic!("expected ObservationFailure, got {other:?}"),
    }

    // The emergency force-stop was actually attempted (it was published, then performed).
    assert!(
        state.force_stops() >= 1,
        "the emergency force-stop must still be performed"
    );
    // The force-stop observation was published to the authoritative stream (it is what
    // the sink failed on) — i.e. the teardown action was never invisible.
    let attempted_force_obs = sink.attempted_kinds().contains(&"force_stop_sent");
    assert!(
        attempted_force_obs,
        "the emergency SIGKILL must be published as force_stop_sent before it happens: {:?}",
        sink.attempted_kinds()
    );
}

// Control: with a HEALTHY sink, the same emergency force-stop path publishes
// `ForceStopSent` and the terminal remains the boundary fault (not masked).
#[tokio::test]
async fn emergency_force_stop_is_observed_and_result_stays_boundary_failure() {
    let boundary = MockBoundary::new().with_graceful_stop_error("SIGTERM delivery failed");
    let sink = RecordingSink::new();

    let mut spec = child_spec("fss-ok", "unused");
    spec.cancellation.graceful_timeout = Duration::from_secs(10);

    let (handle, join) = start_with(spec, boundary.boxed(), &sink);
    tokio::time::sleep(Duration::from_millis(150)).await;
    tokio::time::timeout(Duration::from_secs(3), handle.cancel())
        .await
        .expect("cancel must be bounded");
    let result = join.join().await;

    assert_eq!(result.kind(), "BOUNDARY_FAILURE", "got {result:?}");
    // Exactly one ForceStopSent is recorded on the authoritative stream (the emergency
    // teardown), so PR-4's adapter can map it to a canonical event.
    let force_stops = sink
        .observations()
        .into_iter()
        .filter(|o| matches!(o, WorkerObservation::ForceStopSent))
        .count();
    assert_eq!(
        force_stops,
        1,
        "the emergency SIGKILL must appear exactly once on the stream: {:?}",
        sink.kinds()
    );
}
