//! The run-time evidence contract at the supervisor seam: evidence is published before
//! `Spawned`, and a boundary that ATTESTS full enforcement but ESTABLISHES less at launch is
//! torn down and fails closed under `RequireFullyEnforced` — a live process is never trusted
//! on a claim its evidence does not back.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod common;

use common::mock::MockBoundary;
use common::*;
use o7_worker::{
    BoundaryAttestation, BoundaryEvidence, BoundaryKind, BoundaryRequirement, EnforcementLevel,
};

fn full_attestation() -> BoundaryAttestation {
    BoundaryAttestation {
        implementation: BoundaryKind::Sandboy,
        enforcement: EnforcementLevel::FullyEnforced,
    }
}

fn none_attestation() -> BoundaryAttestation {
    BoundaryAttestation {
        implementation: BoundaryKind::UnconfinedHost,
        enforcement: EnforcementLevel::None,
    }
}

#[tokio::test]
async fn launch_evidence_is_published_before_spawned() {
    // A boring successful run on the mock: LaunchEvidence must appear, and before Spawned.
    let spec = child_spec("evidence-order", "exit0");
    let sink = RecordingSink::new();
    let _ = run_with(spec, Box::new(MockBoundary::new()), &sink).await;

    let kinds = sink.kinds();
    let pos = |k: &str| kinds.iter().position(|x| *x == k);
    let evidence = pos("launch_evidence").expect("launch_evidence must be published");
    let spawned = pos("spawned").expect("spawned must be published");
    assert!(
        evidence < spawned,
        "launch_evidence must precede spawned; got {kinds:?}"
    );
}

#[tokio::test]
async fn a_boundary_that_attests_full_but_establishes_none_fails_closed() {
    // The lying boundary: it PASSES the pre-spawn requirement check (attests FullyEnforced)
    // but its launch evidence establishes None. Under RequireFullyEnforced the supervisor must
    // tear the live process down and fail closed — never run it.
    let boundary = MockBoundary::new()
        .with_attestation(full_attestation())
        .with_launch_evidence(BoundaryEvidence::unconfined(none_attestation()));
    let state = boundary.state();

    let mut spec = child_spec("lying-boundary", "exit0");
    spec.boundary_requirement = BoundaryRequirement::RequireFullyEnforced;
    let sink = RecordingSink::new();
    let result = run_with(spec, Box::new(boundary), &sink).await;

    assert_eq!(
        result.kind(),
        "FAILED_TO_START",
        "downgraded launch evidence must fail closed: {result:?}"
    );
    // It failed AFTER spawn (the pre-spawn attestation passed), so the process was owned and
    // must have been torn down — and the evidence was NOT published as trusted.
    assert!(
        !sink.has("launch_evidence"),
        "downgraded evidence must not be published as established: {:?}",
        sink.kinds()
    );
    assert!(
        !sink.has("spawned"),
        "the process must never be announced running"
    );
    assert!(
        state.force_stops() >= 1,
        "the owned process must be force-stopped (verified teardown) on a fail-closed launch"
    );
}

#[tokio::test]
async fn full_evidence_under_require_fully_enforced_runs_normally() {
    // The honest confining boundary: attests and establishes FullyEnforced → the run proceeds.
    let boundary = MockBoundary::new()
        .with_attestation(full_attestation())
        .with_launch_evidence(BoundaryEvidence::unconfined(full_attestation()));

    let mut spec = child_spec("honest-full", "exit0");
    spec.boundary_requirement = BoundaryRequirement::RequireFullyEnforced;
    let sink = RecordingSink::new();
    let result = run_with(spec, Box::new(boundary), &sink).await;

    assert_eq!(result.kind(), "EXITED_NORMALLY", "got {result:?}");
    assert!(sink.has("launch_evidence"));
    assert!(sink.has("spawned"));
}
