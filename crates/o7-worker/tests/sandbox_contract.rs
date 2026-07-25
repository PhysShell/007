//! Contract tests for the Sandboy boundary's PURE, fail-closed surface: policy/backend
//! validation, the confined-backend argv, the exec-permission gate that keeps PR 3's
//! sealed-memfd exec working, honest attestation, and — the load-bearing part — the report
//! VERIFICATION matrix: a report is trusted only when bound to this policy, launch, and
//! target AND fully enforced; everything else fails closed. These are all GREEN. The confined
//! LIVE LAUNCH matrix is pinned (RED) in the sibling `sandboy_launch.rs`.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

use o7_worker::boundary::BoundarySpawnSpec;
use o7_worker::sandbox_protocol::frame::encode;
use o7_worker::sandbox_protocol::ids::{BackendIdentity, Digest256, LaunchNonce};
use o7_worker::sandbox_protocol::policy::{Enforcement, NetworkPolicy, SandboxPolicy};
use o7_worker::sandbox_protocol::report::SandboxReport;
use o7_worker::sandbox_protocol::SCHEMA_VERSION;
use o7_worker::{
    BoundaryKind, BoundaryRequirement, EnforcementLevel, ProcessBoundary, SandboyBoundary,
    SandboyLaunchError, StdinMode,
};

const BACKEND: &str = "/usr/bin/sandboy";

fn full_policy() -> SandboxPolicy {
    SandboxPolicy {
        worktree: PathBuf::from("/work/run-1"),
        allow_exec: vec![PathBuf::from("/proc"), PathBuf::from("/usr/bin")],
        network: NetworkPolicy::DenyAll,
        env_allowlist: vec![OsString::from("PATH")],
        timeout: Duration::from_secs(600),
    }
}

fn boundary() -> SandboyBoundary {
    SandboyBoundary::new(PathBuf::from(BACKEND), full_policy()).expect("valid boundary")
}

fn a_nonce() -> LaunchNonce {
    LaunchNonce::from_bytes([0x11; 16])
}

fn a_target() -> Digest256 {
    Digest256::of_bytes(b"the confined target bytes")
}

/// A report frame bound to `boundary`'s policy, `nonce`, and `target`, with each dimension set
/// as given — so a test can forge a downgrade or a mis-binding on purpose.
fn report_frame(
    b: &SandboyBoundary,
    nonce: &LaunchNonce,
    target: &Digest256,
    dims: [Enforcement; 5],
) -> Vec<u8> {
    let [filesystem, network, env, process_tree, timeout] = dims;
    let report = SandboxReport {
        schema_version: SCHEMA_VERSION,
        backend: BackendIdentity::new("sandboy-linux", "0.1.0").unwrap(),
        policy_digest: b.policy().digest(),
        launch_nonce: nonce.clone(),
        target_digest: target.clone(),
        filesystem,
        network,
        env,
        process_tree,
        timeout,
    };
    encode(&report).expect("report encodes")
}

const ALL_ENFORCED: [Enforcement; 5] = [Enforcement::Enforced; 5];

// --- construction is fail-closed ---

#[test]
fn a_relative_backend_is_rejected() {
    let err = SandboyBoundary::new(PathBuf::from("sandboy"), full_policy())
        .expect_err("a PATH-relative backend must be refused");
    assert!(matches!(err, SandboyLaunchError::RelativeBackend(_)));
}

#[test]
fn an_invalid_policy_is_rejected() {
    let mut policy = full_policy();
    policy.timeout = Duration::ZERO;
    assert!(SandboyBoundary::new(PathBuf::from(BACKEND), policy).is_err());
}

#[test]
fn attestation_is_sandboy_fully_enforced_and_satisfies_the_requirement() {
    let attestation = boundary().attestation();
    assert_eq!(attestation.implementation, BoundaryKind::Sandboy);
    assert_eq!(attestation.enforcement, EnforcementLevel::FullyEnforced);
    assert!(BoundaryRequirement::RequireFullyEnforced.is_satisfied_by(&attestation));
}

// --- the exec gate: PR-3 sealed memfd ---

#[test]
fn the_sealed_memfd_exec_path_must_be_permitted() {
    let sealed = Path::new("/proc/4242/fd/7");
    assert!(
        full_policy().permits_exec(sealed),
        "granting /proc permits it"
    );

    let mut denies = full_policy();
    denies.allow_exec = vec![PathBuf::from("/usr/bin")];
    assert!(
        !denies.permits_exec(sealed),
        "a policy not covering /proc/<pid>/fd must NOT permit the sealed-memfd exec"
    );
}

#[test]
fn the_backend_invocation_wraps_the_target_after_a_separator() {
    let b = boundary();
    let target = BoundarySpawnSpec {
        executable: PathBuf::from("/proc/4242/fd/7"),
        arguments: vec![OsString::from("--flag"), OsString::from("value")],
        working_directory: PathBuf::from("/work/run-1"),
        environment: Default::default(),
        stdin: StdinMode::Null,
    };
    let wrapped = b
        .backend_spawn_spec(&target, 3, &a_nonce())
        .expect("a permitted target is wrapped");

    assert_eq!(wrapped.executable, PathBuf::from(BACKEND));
    let before_sep = |flag: &str| {
        let sep = wrapped.arguments.iter().position(|a| a == "--").unwrap();
        wrapped.arguments[..sep].iter().any(|a| a == flag)
    };
    assert!(before_sep("--deny-net"));
    assert!(before_sep("--report-fd"));
    assert!(before_sep("--launch-nonce"));
    let sep = wrapped.arguments.iter().position(|a| a == "--").unwrap();
    assert_eq!(
        wrapped.arguments[sep + 1],
        OsString::from("/proc/4242/fd/7")
    );
    assert_eq!(wrapped.arguments[sep + 2], OsString::from("--flag"));
    assert_eq!(wrapped.arguments[sep + 3], OsString::from("value"));
    // The nonce we passed is the one placed on the argv.
    let np = wrapped
        .arguments
        .iter()
        .position(|a| a == "--launch-nonce")
        .unwrap();
    assert_eq!(
        wrapped.arguments[np + 1],
        OsString::from(a_nonce().as_str())
    );
}

#[test]
fn wrapping_an_unpermitted_target_fails_closed() {
    let mut policy = full_policy();
    policy.allow_exec = vec![PathBuf::from("/usr/bin")];
    let b = SandboyBoundary::new(PathBuf::from(BACKEND), policy).unwrap();
    let target = BoundarySpawnSpec {
        executable: PathBuf::from("/proc/4242/fd/7"),
        arguments: vec![],
        working_directory: PathBuf::from("/work/run-1"),
        environment: Default::default(),
        stdin: StdinMode::Null,
    };
    assert!(b.backend_spawn_spec(&target, 3, &a_nonce()).is_err());
}

// --- the report VERIFICATION matrix (pure, fail-closed) ---

#[test]
fn a_fully_bound_full_report_verifies_to_sandboy_evidence() {
    let b = boundary();
    let nonce = a_nonce();
    let target = a_target();
    let frame = report_frame(&b, &nonce, &target, ALL_ENFORCED);
    let evidence = b
        .verify_report(&frame, &nonce, &target)
        .expect("a fully bound, fully enforced report is trusted");
    assert_eq!(evidence.attestation.implementation, BoundaryKind::Sandboy);
    assert_eq!(
        evidence.attestation.enforcement,
        EnforcementLevel::FullyEnforced
    );
    assert_eq!(
        evidence.report.as_deref(),
        Some(frame.as_slice()),
        "the raw frame is carried for content-addressed persistence"
    );
}

#[test]
fn a_downgraded_report_fails_closed() {
    let b = boundary();
    let nonce = a_nonce();
    let target = a_target();
    // Every single-dimension downgrade must be rejected.
    for i in 0..5 {
        let mut dims = ALL_ENFORCED;
        dims[i] = Enforcement::Partial;
        let frame = report_frame(&b, &nonce, &target, dims);
        assert!(
            matches!(
                b.verify_report(&frame, &nonce, &target),
                Err(SandboyLaunchError::NotFullyEnforced)
            ),
            "dimension {i} downgraded to Partial must fail closed"
        );
    }
}

#[test]
fn a_report_bound_to_a_different_nonce_fails_closed() {
    let b = boundary();
    let target = a_target();
    let frame = report_frame(&b, &a_nonce(), &target, ALL_ENFORCED);
    let other = LaunchNonce::from_bytes([0x22; 16]);
    assert!(matches!(
        b.verify_report(&frame, &other, &target),
        Err(SandboyLaunchError::NonceMismatch)
    ));
}

#[test]
fn a_report_bound_to_a_different_target_fails_closed() {
    let b = boundary();
    let nonce = a_nonce();
    let frame = report_frame(&b, &nonce, &a_target(), ALL_ENFORCED);
    let other = Digest256::of_bytes(b"a different target");
    assert!(matches!(
        b.verify_report(&frame, &nonce, &other),
        Err(SandboyLaunchError::TargetMismatch)
    ));
}

#[test]
fn a_report_bound_to_a_different_policy_fails_closed() {
    // A report minted against ANOTHER policy must not verify against this boundary.
    let other_boundary = {
        let mut p = full_policy();
        p.timeout = Duration::from_secs(601);
        SandboyBoundary::new(PathBuf::from(BACKEND), p).unwrap()
    };
    let nonce = a_nonce();
    let target = a_target();
    let foreign_frame = report_frame(&other_boundary, &nonce, &target, ALL_ENFORCED);
    assert!(matches!(
        boundary().verify_report(&foreign_frame, &nonce, &target),
        Err(SandboyLaunchError::PolicyDigestMismatch)
    ));
}

#[test]
fn a_malformed_or_truncated_frame_fails_closed() {
    let b = boundary();
    let nonce = a_nonce();
    let target = a_target();
    let frame = report_frame(&b, &nonce, &target, ALL_ENFORCED);
    // Truncate the body.
    let cut = &frame[..frame.len() - 4];
    assert!(matches!(
        b.verify_report(cut, &nonce, &target),
        Err(SandboyLaunchError::Frame(_))
    ));
    // Not even a frame.
    assert!(matches!(
        b.verify_report(b"not a frame", &nonce, &target),
        Err(SandboyLaunchError::Frame(_))
    ));
}
