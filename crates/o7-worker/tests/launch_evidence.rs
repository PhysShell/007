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

/// A `Reported` evidence attesting `attestation`, carrying a placeholder verified frame.
fn reported(attestation: BoundaryAttestation) -> BoundaryEvidence {
    BoundaryEvidence::Reported {
        attestation,
        report: b"verified-frame".to_vec(),
    }
}

fn host_full_attestation() -> BoundaryAttestation {
    // A NONSENSE run-time claim used only to prove the implementation-consistency check:
    // "fully enforced" but by the UnconfinedHost implementation.
    BoundaryAttestation {
        implementation: BoundaryKind::UnconfinedHost,
        enforcement: EnforcementLevel::FullyEnforced,
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

/// Drive a boundary that attests `full_attestation()` pre-spawn but returns `evidence` at
/// launch, under RequireFullyEnforced, and assert it fails closed with verified teardown and
/// no trusted publish.
async fn assert_fails_closed(evidence: BoundaryEvidence, id: &str) {
    let boundary = MockBoundary::new()
        .with_attestation(full_attestation())
        .with_launch_evidence(evidence);
    let state = boundary.state();

    let mut spec = child_spec(id, "exit0");
    spec.boundary_requirement = BoundaryRequirement::RequireFullyEnforced;
    let sink = RecordingSink::new();
    let result = run_with(spec, Box::new(boundary), &sink).await;

    assert_eq!(
        result.kind(),
        "FAILED_TO_START",
        "must fail closed: {result:?}"
    );
    assert!(
        !sink.has("launch_evidence"),
        "un-satisfying evidence must not be published as established: {:?}",
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
async fn attests_full_but_establishes_no_report_fails_closed() {
    // Full + NO report (Unconfined). The pre-spawn attestation passes, so the process is owned;
    // the missing report must tear it down and fail closed — there is no "fully enforced with
    // no report" path.
    assert_fails_closed(BoundaryEvidence::Unconfined, "no-report").await;
}

#[tokio::test]
async fn reported_evidence_with_a_mismatched_implementation_fails_closed() {
    // A report that claims FullyEnforced but by the WRONG implementation (Host, while the
    // boundary is configured Sandboy) must fail the configured/established consistency check.
    assert_fails_closed(reported(host_full_attestation()), "impl-mismatch").await;
}

#[tokio::test]
async fn reported_but_downgraded_evidence_fails_closed() {
    // A Reported evidence that is only Partially enforced must fail closed too.
    let downgraded = BoundaryAttestation {
        implementation: BoundaryKind::Sandboy,
        enforcement: EnforcementLevel::Partial,
    };
    assert_fails_closed(reported(downgraded), "downgraded").await;
}

#[tokio::test]
async fn full_reported_evidence_under_require_fully_enforced_runs_normally() {
    // The honest confining boundary: attests and REPORTS FullyEnforced by the matching
    // implementation → the run proceeds and the evidence is published.
    let boundary = MockBoundary::new()
        .with_attestation(full_attestation())
        .with_launch_evidence(reported(full_attestation()));

    let mut spec = child_spec("honest-full", "exit0");
    spec.boundary_requirement = BoundaryRequirement::RequireFullyEnforced;
    let sink = RecordingSink::new();
    let result = run_with(spec, Box::new(boundary), &sink).await;

    assert_eq!(result.kind(), "EXITED_NORMALLY", "got {result:?}");
    assert!(sink.has("launch_evidence"));
    assert!(sink.has("spawned"));
}
