//! Acceptance 30-32: boundary attestation and the fail-closed requirement.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod common;

use common::*;
use o7_worker::{
    BoundaryKind, BoundaryRequirement, EnforcementLevel, ProcessBoundary, UnconfinedHostBoundary,
};

// (30) The host boundary attests EnforcementLevel::None (and UnconfinedHost).
#[tokio::test]
async fn host_boundary_attests_none() {
    let attestation = UnconfinedHostBoundary.attestation();
    assert_eq!(attestation.implementation, BoundaryKind::UnconfinedHost);
    assert_eq!(attestation.enforcement, EnforcementLevel::None);
}

// (31) RequireFullyEnforced rejects the host boundary BEFORE spawning.
#[tokio::test]
async fn require_fully_enforced_rejects_host_before_spawn() {
    let mut spec = child_spec("req", "exit0");
    spec.boundary_requirement = BoundaryRequirement::RequireFullyEnforced;
    let sink = RecordingSink::new();
    let result = run_to_completion(spec, &sink).await;
    assert_eq!(result.kind(), "FAILED_TO_START", "got {result:?}");
    assert!(
        !sink.has("spawn_requested"),
        "must fail before requesting spawn"
    );
    assert!(!sink.has("spawned"));
}

// (32) There is no silent fallback: RequireFullyEnforced never quietly runs
// unconfined instead.
#[tokio::test]
async fn no_silent_fallback_to_unconfined() {
    let mut spec = child_spec("nofallback", "exit0");
    spec.boundary_requirement = BoundaryRequirement::RequireFullyEnforced;
    let sink = RecordingSink::new();
    let result = run_to_completion(spec, &sink).await;
    // Not a success of any kind — the process never ran.
    assert_eq!(result.kind(), "FAILED_TO_START");
    assert!(!matches!(
        result,
        o7_worker::WorkerResult::ExitedNormally(_) | o7_worker::WorkerResult::ExitedBySignal(_)
    ));
}
