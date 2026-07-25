//! Contract tests for the Sandboy boundary's PURE, fail-closed surface: policy/backend
//! validation, the confined-backend argv (report + control descriptors), the exec-permission
//! gate, honest attestation, and — the load-bearing part — the object/report VERIFICATION
//! matrix: a backend object is trusted only if its bytes match the pinned digest, and a report
//! only if it echoes the expected backend AND is bound to this policy/launch/target AND fully
//! enforced. All GREEN. The confined LIVE LAUNCH matrix is pinned (RED) in `sandboy_launch.rs`.
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
use o7_worker::sandbox_protocol::policy::{
    Enforcement, NetworkPolicy, SandboxPolicy, SandboxPolicyError,
};
use o7_worker::sandbox_protocol::report::SandboxReport;
use o7_worker::sandbox_protocol::SCHEMA_VERSION;
use o7_worker::{
    BackendImage, BoundaryEvidence, BoundaryKind, BoundaryRequirement, EnforcementLevel,
    ProcessBoundary, SandboyBoundary, SandboyLaunchError, StdinMode,
};

const BACKEND_BYTES: &[u8] = b"the trusted sandboy backend object bytes";

fn backend_identity() -> BackendIdentity {
    BackendIdentity::new("sandboy-linux", "0.1.0").unwrap()
}

fn backend_image() -> BackendImage {
    BackendImage {
        descriptor: PathBuf::from("/usr/bin/sandboy"),
        digest: Digest256::of_bytes(BACKEND_BYTES),
        identity: backend_identity(),
    }
}

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
    SandboyBoundary::new(backend_image(), full_policy()).expect("valid boundary")
}

fn a_nonce() -> LaunchNonce {
    LaunchNonce::from_bytes([0x11; 16])
}

fn a_target() -> Digest256 {
    Digest256::of_bytes(b"the confined target bytes")
}

/// A report frame with the given backend identity/policy/nonce/target/dimensions — so a test
/// can forge a wrong backend, a mis-binding, or a downgrade on purpose.
fn report_frame(
    backend: BackendIdentity,
    policy_digest: Digest256,
    nonce: &LaunchNonce,
    target: &Digest256,
    dims: [Enforcement; 5],
) -> Vec<u8> {
    let [filesystem, network, env, process_tree, timeout] = dims;
    let report = SandboxReport {
        schema_version: SCHEMA_VERSION,
        backend,
        policy_digest,
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

/// A fully-bound, fully-enforced frame for `boundary()`.
fn good_frame(b: &SandboyBoundary, nonce: &LaunchNonce, target: &Digest256) -> Vec<u8> {
    report_frame(
        b.backend().identity.clone(),
        b.policy().digest(),
        nonce,
        target,
        ALL_ENFORCED,
    )
}

const ALL_ENFORCED: [Enforcement; 5] = [Enforcement::Enforced; 5];

// --- construction is fail-closed ---

#[test]
fn a_relative_backend_descriptor_is_rejected() {
    let mut image = backend_image();
    image.descriptor = PathBuf::from("sandboy");
    let err = SandboyBoundary::new(image, full_policy())
        .expect_err("a relative backend descriptor must be refused");
    assert!(matches!(err, SandboyLaunchError::RelativeBackend(_)));
}

#[test]
fn an_invalid_policy_is_rejected() {
    let mut policy = full_policy();
    policy.timeout = Duration::ZERO;
    assert!(SandboyBoundary::new(backend_image(), policy).is_err());
}

#[test]
fn attestation_is_sandboy_fully_enforced_and_satisfies_the_requirement() {
    let attestation = boundary().attestation();
    assert_eq!(attestation.implementation, BoundaryKind::Sandboy);
    assert_eq!(attestation.enforcement, EnforcementLevel::FullyEnforced);
    assert!(BoundaryRequirement::RequireFullyEnforced.is_satisfied_by(&attestation));
}

// --- backend object binding: bytes, not path ---

#[test]
fn the_backend_object_is_bound_to_its_digest() {
    let b = boundary();
    assert!(b.verify_backend_object(BACKEND_BYTES).is_ok());
    // A substituted object (a swapped binary at the same path) must fail closed.
    assert!(matches!(
        b.verify_backend_object(b"a substituted backend binary"),
        Err(SandboyLaunchError::BackendObjectMismatch)
    ));
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
fn the_backend_invocation_carries_report_and_control_fds_and_wraps_the_target() {
    let b = boundary();
    let target = BoundarySpawnSpec {
        executable: PathBuf::from("/proc/4242/fd/7"),
        arguments: vec![OsString::from("--flag"), OsString::from("value")],
        working_directory: PathBuf::from("/work/run-1"),
        environment: Default::default(),
        stdin: StdinMode::Null,
    };
    let wrapped = b
        .backend_spawn_spec(&target, 3, 4, &a_nonce())
        .expect("a permitted target is wrapped");

    // The launched program is the HELD backend descriptor.
    assert_eq!(wrapped.executable, PathBuf::from("/usr/bin/sandboy"));
    let arg_after = |flag: &str| -> OsString {
        let i = wrapped.arguments.iter().position(|a| a == flag).unwrap();
        wrapped.arguments[i + 1].clone()
    };
    assert_eq!(arg_after("--report-fd"), OsString::from("3"));
    assert_eq!(arg_after("--control-fd"), OsString::from("4"));
    assert_eq!(
        arg_after("--launch-nonce"),
        OsString::from(a_nonce().as_str())
    );
    let sep = wrapped.arguments.iter().position(|a| a == "--").unwrap();
    assert!(wrapped.arguments[..sep].iter().any(|a| a == "--deny-net"));
    assert_eq!(
        wrapped.arguments[sep + 1],
        OsString::from("/proc/4242/fd/7")
    );
    assert_eq!(wrapped.arguments[sep + 2], OsString::from("--flag"));
    assert_eq!(wrapped.arguments[sep + 3], OsString::from("value"));
}

#[test]
fn wrapping_an_unpermitted_target_fails_closed() {
    let mut policy = full_policy();
    policy.allow_exec = vec![PathBuf::from("/usr/bin")];
    let b = SandboyBoundary::new(backend_image(), policy).unwrap();
    let target = BoundarySpawnSpec {
        executable: PathBuf::from("/proc/4242/fd/7"),
        arguments: vec![],
        working_directory: PathBuf::from("/work/run-1"),
        environment: Default::default(),
        stdin: StdinMode::Null,
    };
    assert!(b.backend_spawn_spec(&target, 3, 4, &a_nonce()).is_err());
}

// --- the report VERIFICATION matrix (pure, fail-closed) ---

#[test]
fn a_fully_bound_full_report_verifies_to_reported_sandboy_evidence() {
    let b = boundary();
    let nonce = a_nonce();
    let target = a_target();
    let frame = good_frame(&b, &nonce, &target);
    let evidence = b
        .verify_report(&frame, &nonce, &target)
        .expect("a fully bound, fully enforced report is trusted");
    match evidence {
        BoundaryEvidence::Reported {
            attestation,
            report,
        } => {
            assert_eq!(attestation.implementation, BoundaryKind::Sandboy);
            assert_eq!(attestation.enforcement, EnforcementLevel::FullyEnforced);
            assert_eq!(report, frame, "the raw frame is carried for persistence");
        }
        BoundaryEvidence::Unconfined => panic!("a verified report must be Reported evidence"),
    }
}

#[test]
fn a_report_from_the_wrong_backend_fails_closed() {
    let b = boundary();
    let nonce = a_nonce();
    let target = a_target();
    let frame = report_frame(
        BackendIdentity::new("totally-secure-trust-me", "9.9").unwrap(),
        b.policy().digest(),
        &nonce,
        &target,
        ALL_ENFORCED,
    );
    assert!(matches!(
        b.verify_report(&frame, &nonce, &target),
        Err(SandboyLaunchError::BackendMismatch)
    ));
}

#[test]
fn a_downgraded_report_fails_closed() {
    let b = boundary();
    let nonce = a_nonce();
    let target = a_target();
    for i in 0..5 {
        let mut dims = ALL_ENFORCED;
        dims[i] = Enforcement::Partial;
        let frame = report_frame(
            b.backend().identity.clone(),
            b.policy().digest(),
            &nonce,
            &target,
            dims,
        );
        assert!(
            matches!(
                b.verify_report(&frame, &nonce, &target),
                Err(SandboyLaunchError::NotFullyEnforced)
            ),
            "dimension {i} downgraded must fail closed"
        );
    }
}

#[test]
fn a_report_bound_to_a_different_nonce_fails_closed() {
    let b = boundary();
    let target = a_target();
    let frame = good_frame(&b, &a_nonce(), &target);
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
    let frame = good_frame(&b, &nonce, &a_target());
    let other = Digest256::of_bytes(b"a different target");
    assert!(matches!(
        b.verify_report(&frame, &nonce, &other),
        Err(SandboyLaunchError::TargetMismatch)
    ));
}

#[test]
fn a_report_bound_to_a_different_policy_fails_closed() {
    let b = boundary();
    let nonce = a_nonce();
    let target = a_target();
    // A report minted against ANOTHER policy digest.
    let other_policy = {
        let mut p = full_policy();
        p.timeout = Duration::from_secs(601);
        p
    };
    let frame = report_frame(
        b.backend().identity.clone(),
        other_policy.digest(),
        &nonce,
        &target,
        ALL_ENFORCED,
    );
    assert!(matches!(
        b.verify_report(&frame, &nonce, &target),
        Err(SandboyLaunchError::PolicyDigestMismatch)
    ));
}

#[test]
fn a_malformed_or_truncated_frame_fails_closed() {
    let b = boundary();
    let nonce = a_nonce();
    let target = a_target();
    let frame = good_frame(&b, &nonce, &target);
    let cut = &frame[..frame.len() - 4];
    assert!(matches!(
        b.verify_report(cut, &nonce, &target),
        Err(SandboyLaunchError::Frame(_))
    ));
    assert!(matches!(
        b.verify_report(b"not a frame", &nonce, &target),
        Err(SandboyLaunchError::Frame(_))
    ));
}

// --- P1: policy allowances/env are SETS ---

#[test]
fn duplicate_exec_allowances_are_rejected() {
    let mut policy = full_policy();
    policy.allow_exec = vec![PathBuf::from("/bin"), PathBuf::from("/bin")];
    assert!(matches!(
        policy.validate(),
        Err(SandboxPolicyError::DuplicateExecAllowance(_))
    ));
}

#[test]
fn duplicate_env_names_are_rejected() {
    let mut policy = full_policy();
    policy.env_allowlist = vec![OsString::from("PATH"), OsString::from("PATH")];
    assert!(matches!(
        policy.validate(),
        Err(SandboxPolicyError::DuplicateEnvName(_))
    ));
}
