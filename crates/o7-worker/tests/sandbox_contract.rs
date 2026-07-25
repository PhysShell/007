//! Contract tests for the Sandboy confinement types: the per-dimension report (no boolean
//! `secure`, every dimension explicit), its honest mapping to a [`BoundaryAttestation`],
//! policy validation, and the fail-closed exec-permission gate that keeps PR 3's
//! sealed-memfd exec working. These are all PURE and GREEN; the confined LIVE LAUNCH is
//! pinned (RED) in the sibling `sandboy_launch.rs`.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

use o7_worker::{
    BoundaryKind, BoundaryRequirement, Enforcement, EnforcementLevel, NetworkPolicy,
    ProcessBoundary, ReportError, SandboxPolicy, SandboxPolicyError, SandboxReport,
    SandboyBoundary,
};

fn full_report_json(timeout: &str) -> String {
    format!(
        r#"{{"backend":"landlock-seccomp","filesystem":"enforced","network":"enforced","env":"enforced","process_tree":"enforced","timeout":"{timeout}"}}"#
    )
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

// --- The report rejects the shapes that let software lie about confinement ---

#[test]
fn a_boolean_secure_report_is_rejected() {
    // issue #53: "Avoid boolean `secure: true`." An unknown field must fail to parse, not be
    // silently ignored.
    let bytes = br#"{"backend":"x","filesystem":"enforced","network":"enforced","env":"enforced","process_tree":"enforced","timeout":"enforced","secure":true}"#;
    assert!(matches!(
        SandboxReport::parse(bytes),
        Err(ReportError::Malformed(_))
    ));
}

#[test]
fn a_report_missing_a_dimension_is_rejected() {
    // Drop `timeout` — a missing dimension must be a parse error, never a silent "enforced".
    let bytes = br#"{"backend":"x","filesystem":"enforced","network":"enforced","env":"enforced","process_tree":"enforced"}"#;
    assert!(matches!(
        SandboxReport::parse(bytes),
        Err(ReportError::Malformed(_))
    ));
}

#[test]
fn a_report_with_no_backend_named_is_rejected() {
    let bytes = full_report_json("enforced").replace("landlock-seccomp", "");
    assert_eq!(
        SandboxReport::parse(bytes.as_bytes()),
        Err(ReportError::EmptyBackend)
    );
}

#[test]
fn a_well_formed_report_round_trips() {
    let bytes = full_report_json("enforced");
    let report = SandboxReport::parse(bytes.as_bytes()).expect("valid report parses");
    let json = serde_json::to_vec(&report).unwrap();
    let back = SandboxReport::parse(&json).expect("re-parses");
    assert_eq!(report, back);
}

// --- The honest mapping: a downgrade can never present as full confinement ---

#[test]
fn an_all_enforced_report_is_fully_enforced_and_attests_sandboy() {
    let report = SandboxReport::parse(full_report_json("enforced").as_bytes()).unwrap();
    assert_eq!(report.enforcement_level(), EnforcementLevel::FullyEnforced);
    let attestation = report.attestation();
    assert_eq!(attestation.implementation, BoundaryKind::Sandboy);
    assert_eq!(attestation.enforcement, EnforcementLevel::FullyEnforced);
}

#[test]
fn any_downgraded_dimension_is_never_fully_enforced() {
    // A single non-enforced dimension must drop the whole report below FullyEnforced, so it
    // can never satisfy RequireFullyEnforced.
    for downgraded in [
        r#"{"backend":"x","filesystem":"partial","network":"enforced","env":"enforced","process_tree":"enforced","timeout":"enforced"}"#,
        r#"{"backend":"x","filesystem":"enforced","network":"not_enforced","env":"enforced","process_tree":"enforced","timeout":"enforced"}"#,
    ] {
        let report = SandboxReport::parse(downgraded.as_bytes()).unwrap();
        assert_ne!(
            report.enforcement_level(),
            EnforcementLevel::FullyEnforced,
            "a downgraded dimension must not read as fully enforced: {downgraded}"
        );
        assert!(
            !BoundaryRequirement::RequireFullyEnforced.is_satisfied_by(&report.attestation()),
            "a downgraded report must not satisfy RequireFullyEnforced: {downgraded}"
        );
    }
}

#[test]
fn an_all_not_enforced_report_is_none() {
    let bytes = br#"{"backend":"x","filesystem":"not_enforced","network":"not_enforced","env":"not_enforced","process_tree":"not_enforced","timeout":"not_enforced"}"#;
    let report = SandboxReport::parse(bytes).unwrap();
    assert_eq!(report.enforcement_level(), EnforcementLevel::None);
}

#[test]
fn a_mixed_report_is_partial() {
    let bytes = br#"{"backend":"x","filesystem":"enforced","network":"not_enforced","env":"partial","process_tree":"enforced","timeout":"enforced"}"#;
    let report = SandboxReport::parse(bytes).unwrap();
    assert_eq!(report.enforcement_level(), EnforcementLevel::Partial);
    assert_eq!(
        report.dimension(o7_worker::SandboxDimension::Env),
        Enforcement::Partial
    );
}

// --- Policy validation: a degenerate policy never becomes a FullyEnforced boundary ---

#[test]
fn a_relative_worktree_is_rejected() {
    let mut policy = full_policy();
    policy.worktree = PathBuf::from("relative/dir");
    assert!(matches!(
        policy.validate(),
        Err(SandboxPolicyError::RelativeWorktree(_))
    ));
}

#[test]
fn a_policy_with_no_exec_allowance_is_rejected() {
    let mut policy = full_policy();
    policy.allow_exec.clear();
    assert_eq!(policy.validate(), Err(SandboxPolicyError::NoExecAllowance));
}

#[test]
fn a_relative_exec_allowance_is_rejected() {
    let mut policy = full_policy();
    policy.allow_exec = vec![PathBuf::from("bin")];
    assert!(matches!(
        policy.validate(),
        Err(SandboxPolicyError::RelativeExecAllowance(_))
    ));
}

#[test]
fn a_zero_timeout_is_rejected() {
    let mut policy = full_policy();
    policy.timeout = Duration::ZERO;
    assert_eq!(policy.validate(), Err(SandboxPolicyError::ZeroTimeout));
}

#[test]
fn new_rejects_an_invalid_policy_fail_closed() {
    let mut policy = full_policy();
    policy.timeout = Duration::ZERO;
    assert!(SandboyBoundary::new(PathBuf::from("/usr/bin/sandboy"), policy).is_err());
}

// --- The load-bearing PR-3 invariant: the sealed-memfd exec path must be permitted ---

#[test]
fn the_sealed_memfd_exec_path_must_be_permitted_by_the_policy() {
    // PR 3 execs a sealed memfd via /proc/<verifier_pid>/fd/<n>. Granting /proc permits it;
    // granting nothing under /proc must fail closed.
    let sealed = Path::new("/proc/4242/fd/7");

    let mut permits = full_policy();
    permits.allow_exec = vec![PathBuf::from("/proc")];
    assert!(
        permits.permits_exec(sealed),
        "granting /proc must permit the sealed-memfd exec target"
    );

    let mut denies = full_policy();
    denies.allow_exec = vec![PathBuf::from("/usr/bin")];
    assert!(
        !denies.permits_exec(sealed),
        "a policy not covering /proc/<pid>/fd must NOT permit the sealed-memfd exec"
    );
}

#[test]
fn attestation_of_a_valid_boundary_is_sandboy_fully_enforced() {
    let boundary = SandboyBoundary::new(PathBuf::from("/usr/bin/sandboy"), full_policy())
        .expect("a full-confinement policy is accepted");
    let attestation = boundary.attestation();
    assert_eq!(attestation.implementation, BoundaryKind::Sandboy);
    assert_eq!(attestation.enforcement, EnforcementLevel::FullyEnforced);
    assert!(
        BoundaryRequirement::RequireFullyEnforced.is_satisfied_by(&attestation),
        "a configured Sandboy boundary must satisfy RequireFullyEnforced"
    );
}

#[test]
fn the_backend_invocation_wraps_the_target_after_a_separator() {
    let boundary = SandboyBoundary::new(PathBuf::from("/usr/bin/sandboy"), full_policy()).unwrap();
    let target = o7_worker::boundary::BoundarySpawnSpec {
        executable: PathBuf::from("/proc/4242/fd/7"),
        arguments: vec![OsString::from("--flag"), OsString::from("value")],
        working_directory: PathBuf::from("/work/run-1"),
        environment: Default::default(),
        stdin: o7_worker::StdinMode::Null,
    };
    let wrapped = boundary
        .backend_spawn_spec(&target)
        .expect("a permitted target is wrapped");

    // The launched program is the backend, not the target.
    assert_eq!(wrapped.executable, PathBuf::from("/usr/bin/sandboy"));
    // The confinement flags precede the separator; the real target follows it.
    let sep = wrapped
        .arguments
        .iter()
        .position(|a| a == "--")
        .expect("a `--` separator delimits the target");
    let before: Vec<&OsString> = wrapped.arguments[..sep].iter().collect();
    assert!(before.iter().any(|a| *a == "--deny-net"));
    assert!(before.iter().any(|a| *a == "--report"));
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
    let boundary = SandboyBoundary::new(PathBuf::from("/usr/bin/sandboy"), policy).unwrap();
    let target = o7_worker::boundary::BoundarySpawnSpec {
        executable: PathBuf::from("/proc/4242/fd/7"),
        arguments: vec![],
        working_directory: PathBuf::from("/work/run-1"),
        environment: Default::default(),
        stdin: o7_worker::StdinMode::Null,
    };
    assert!(
        boundary.backend_spawn_spec(&target).is_err(),
        "an exec target the policy does not permit must refuse to build a launch"
    );
}
