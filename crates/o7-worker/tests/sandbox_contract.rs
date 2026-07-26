//! Contract tests for the Sandboy boundary's PURE, fail-closed surface: acquisition-bound
//! backend, policy validation, the confined-backend argv (report + control descriptors, TRUSTED
//! backend cwd/env — never the target's), the exec-permission gate, honest attestation, and the
//! report VERIFICATION matrix (a report is trusted only if it echoes the expected backend AND is
//! bound to this policy/launch/target AND fully enforced). All GREEN. The confined LIVE LAUNCH /
//! monitor matrix is pinned (RED) in `sandboy_launch.rs`.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::ffi::OsString;
use std::io::Write as _;
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

/// Acquire a real held-descriptor backend image over a temp file with `bytes`. The temp file is
/// unlinked on drop, but the acquired image holds its OWN open fd, so it stays valid.
fn acquired_backend(bytes: &[u8], digest: Digest256) -> Result<BackendImage, SandboyLaunchError> {
    let mut tmp = tempfile::NamedTempFile::new().unwrap();
    tmp.write_all(bytes).unwrap();
    tmp.flush().unwrap();
    BackendImage::acquire(tmp.path(), digest, backend_identity())
}

fn backend_image() -> BackendImage {
    acquired_backend(BACKEND_BYTES, Digest256::of_bytes(BACKEND_BYTES)).expect("acquire backend")
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

#[allow(clippy::too_many_arguments)]
fn report_frame(
    backend: BackendIdentity,
    backend_digest: Digest256,
    policy_digest: Digest256,
    nonce: &LaunchNonce,
    launch_spec: &Digest256,
    dims: [Enforcement; 5],
) -> Vec<u8> {
    let [filesystem, network, env, process_tree, timeout] = dims;
    let report = SandboxReport {
        schema_version: SCHEMA_VERSION,
        backend,
        backend_digest,
        policy_digest,
        launch_nonce: nonce.clone(),
        launch_spec_digest: launch_spec.clone(),
        filesystem,
        network,
        env,
        process_tree,
        timeout,
    };
    encode(&report).expect("report encodes")
}

fn good_frame(b: &SandboyBoundary, nonce: &LaunchNonce, launch_spec: &Digest256) -> Vec<u8> {
    report_frame(
        b.backend().identity().clone(),
        b.backend().digest().clone(),
        b.policy().digest(),
        nonce,
        launch_spec,
        ALL_ENFORCED,
    )
}

const ALL_ENFORCED: [Enforcement; 5] = [Enforcement::Enforced; 5];

// --- acquisition is fail-closed and object-bound ---

#[test]
fn acquiring_a_backend_whose_bytes_mismatch_the_digest_fails_closed() {
    // A pinned digest that does not match the held object's bytes must be rejected — no
    // boundary is built around an object we did not verify.
    let wrong = Digest256::of_bytes(b"some other backend");
    assert!(matches!(
        acquired_backend(BACKEND_BYTES, wrong),
        Err(SandboyLaunchError::BackendObjectMismatch)
    ));
}

#[test]
fn the_acquired_backend_exec_path_is_a_held_proc_fd() {
    // The launch execs the HELD descriptor, addressed as /proc/<owner_pid>/fd/<n> — not a
    // re-resolvable source path.
    let image = backend_image();
    let path = image.descriptor_path();
    let s = path.to_string_lossy();
    assert!(s.starts_with("/proc/"), "held path: {s}");
    assert!(s.contains("/fd/"), "held path: {s}");
}

#[test]
fn a_sealed_backend_survives_an_in_place_source_rewrite() {
    // The load-bearing "held != immutable" fix: acquisition stages the bytes into a SEALED
    // memfd, so a same-UID in-place rewrite of the SOURCE inode after acquire() cannot change
    // what the held descriptor exposes — the sealed bytes are still the original.
    let mut tmp = tempfile::NamedTempFile::new().unwrap();
    tmp.write_all(BACKEND_BYTES).unwrap();
    tmp.flush().unwrap();
    let image = BackendImage::acquire(
        tmp.path(),
        Digest256::of_bytes(BACKEND_BYTES),
        backend_identity(),
    )
    .expect("acquire seals the object");

    // Rewrite the SAME inode in place with attacker bytes.
    std::fs::write(tmp.path(), b"attacker-substituted backend bytes here!").unwrap();

    // The held (sealed) descriptor still exposes the ORIGINAL bytes, and the pinned digest is
    // unchanged.
    let held = std::fs::read(image.descriptor_path()).expect("read the sealed descriptor");
    assert_eq!(held, BACKEND_BYTES, "the sealed object must be immutable");
    assert_eq!(*image.digest(), Digest256::of_bytes(BACKEND_BYTES));
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

// --- the confined-backend invocation: descriptors + PLANE SEPARATION ---

#[test]
fn the_backend_invocation_runs_the_held_backend_with_trusted_cwd_and_env() {
    let b = boundary();
    let mut target_env = std::collections::BTreeMap::new();
    target_env.insert(OsString::from("LD_PRELOAD"), OsString::from("/evil.so"));
    let target = BoundarySpawnSpec {
        // The caller's SOURCE path (permitted by the policy), DISTINCT from the sealed descriptor.
        executable: PathBuf::from("/usr/bin/dash"),
        arguments: vec![OsString::from("--flag"), OsString::from("value")],
        working_directory: PathBuf::from("/work/target-cwd"),
        environment: target_env,
        stdin: StdinMode::Null,
    };
    // The backend must exec the SEALED descriptor (what the live launch passes), not the source.
    let sealed = PathBuf::from("/proc/4242/fd/7");
    let wrapped = b
        .backend_spawn_spec(&target, &sealed, &a_nonce())
        .expect("a permitted target is wrapped");

    // The launched program is the HELD backend descriptor, in a TRUSTED cwd, with a TRUSTED
    // (here empty) env — NEVER the target's cwd or env.
    assert!(wrapped.executable.to_string_lossy().starts_with("/proc/"));
    assert_eq!(wrapped.working_directory, PathBuf::from("/"));
    assert!(
        !wrapped
            .environment
            .contains_key(OsString::from("LD_PRELOAD").as_os_str()),
        "the untrusted target env must NOT leak into the backend"
    );

    let arg_after = |flag: &str| -> OsString {
        let i = wrapped.arguments.iter().position(|a| a == flag).unwrap();
        wrapped.arguments[i + 1].clone()
    };
    // The control transport is a single socket on the backend's STDIN, so NO control descriptor is
    // passed by number on the /proc-visible argv — only the per-launch nonce and policy flags.
    assert!(
        !wrapped
            .arguments
            .iter()
            .any(|a| a == "--report-fd" || a == "--control-fd" || a == "--request-fd"),
        "no control descriptor may appear on the /proc-visible argv"
    );
    assert_eq!(
        arg_after("--launch-nonce"),
        OsString::from(a_nonce().as_str())
    );
    // NONE of the target's argv/cwd/env appears on the /proc-visible backend argv — only the
    // target executable descriptor follows the separator; everything else is in the request.
    let flat = wrapped.arguments.join(&OsString::from("\u{1f}"));
    let flat = flat.to_string_lossy();
    assert!(
        !flat.contains("LD_PRELOAD"),
        "target env must not be on the argv"
    );
    assert!(
        !flat.contains("--flag"),
        "target args must not be on the argv"
    );
    assert!(
        !flat.contains("/work/target-cwd"),
        "target cwd must not be on the argv"
    );
    let sep = wrapped.arguments.iter().position(|a| a == "--").unwrap();
    assert_eq!(
        wrapped.arguments[sep + 1],
        OsString::from("/proc/4242/fd/7")
    );
    assert_eq!(
        wrapped.arguments.len(),
        sep + 2,
        "only the target exec follows --"
    );
}

#[test]
fn the_backend_argv_names_the_sealed_descriptor_not_the_source() {
    // The single most consequential A-lane defect: the contract helper must build the SAME argv the
    // live launch passes. The live launch execs the SEALED descriptor (`target.descriptor_path()`,
    // a `/proc/<pid>/fd/<n>`), so the helper must name that sealed path after `--`, NOT the caller's
    // mutable SOURCE path. Prior to the fix, the helper echoed `spec.executable` (the source),
    // disagreeing with the launch — a fiction the old test hid by making the source itself look like
    // a `/proc` path. Here the source and the seal are DISTINCT, so an echo of the source is caught.
    let b = boundary();
    let source = PathBuf::from("/usr/bin/dash"); // a real SOURCE path, permitted by full_policy
    let sealed = PathBuf::from("/proc/4242/fd/7"); // stands in for target.descriptor_path()
    let spec = BoundarySpawnSpec {
        executable: source.clone(),
        arguments: vec![OsString::from("-c"), OsString::from("exit 0")],
        working_directory: PathBuf::from("/work/target-cwd"),
        environment: Default::default(),
        stdin: StdinMode::Null,
    };
    let wrapped = b
        .backend_spawn_spec(&spec, &sealed, &a_nonce())
        .expect("a permitted target is wrapped");
    let sep = wrapped.arguments.iter().position(|a| a == "--").unwrap();
    assert_eq!(
        wrapped.arguments[sep + 1],
        OsString::from("/proc/4242/fd/7"),
        "the backend argv must name the SEALED descriptor the live launch execs"
    );
    assert_ne!(
        wrapped.arguments[sep + 1],
        OsString::from(source.as_os_str()),
        "the caller's mutable SOURCE path must never be the backend's exec target"
    );
    assert_eq!(
        wrapped.arguments.len(),
        sep + 2,
        "only the sealed target follows --"
    );
}

#[test]
fn wrapping_an_unpermitted_target_fails_closed() {
    let mut policy = full_policy();
    policy.allow_exec = vec![PathBuf::from("/usr/bin")];
    let b = SandboyBoundary::new(backend_image(), policy).unwrap();
    let target = BoundarySpawnSpec {
        // A SOURCE path the narrowed policy does not permit; the gate is on this path, not the seal.
        executable: PathBuf::from("/bin/dash"),
        arguments: vec![],
        working_directory: PathBuf::from("/work/run-1"),
        environment: Default::default(),
        stdin: StdinMode::Null,
    };
    let sealed = PathBuf::from("/proc/4242/fd/7");
    assert!(b.backend_spawn_spec(&target, &sealed, &a_nonce()).is_err());
}

// --- the out-of-band launch request: env allowlist ---

#[test]
fn the_launch_request_carries_allowlisted_env_byte_exact() {
    let mut policy = full_policy();
    policy.env_allowlist = vec![OsString::from("PATH"), OsString::from("HOME")];
    let b = SandboyBoundary::new(backend_image(), policy).unwrap();
    let mut env = std::collections::BTreeMap::new();
    env.insert(OsString::from("PATH"), OsString::from("/usr/bin"));
    env.insert(OsString::from("HOME"), OsString::from("/root"));
    let target = BoundarySpawnSpec {
        executable: PathBuf::from("/proc/4242/fd/7"),
        arguments: vec![OsString::from("-c"), OsString::from("exit 0")],
        working_directory: PathBuf::from("/work"),
        environment: env,
        stdin: StdinMode::Null,
    };
    let req = b
        .launch_request(&target, a_target(), &a_nonce())
        .expect("allowlisted env is accepted");
    // argv/cwd/env are carried in the request, byte-exact.
    assert_eq!(req.argv, vec![b"-c".to_vec(), b"exit 0".to_vec()]);
    assert_eq!(req.cwd, b"/work".to_vec());
    let mut names: Vec<Vec<u8>> = req.env.iter().map(|e| e.name.clone()).collect();
    names.sort();
    assert_eq!(names, vec![b"HOME".to_vec(), b"PATH".to_vec()]);
}

#[test]
fn a_disallowed_target_env_var_fails_closed_before_launch() {
    // The target asks to keep a variable the policy does not allowlist → fail closed BEFORE the
    // backend runs anything.
    let b = boundary(); // allowlist = [PATH]
    let mut env = std::collections::BTreeMap::new();
    env.insert(OsString::from("LD_PRELOAD"), OsString::from("/evil.so"));
    let target = BoundarySpawnSpec {
        executable: PathBuf::from("/proc/4242/fd/7"),
        arguments: vec![],
        working_directory: PathBuf::from("/work"),
        environment: env,
        stdin: StdinMode::Null,
    };
    assert!(matches!(
        b.launch_request(&target, a_target(), &a_nonce()),
        Err(SandboyLaunchError::DisallowedEnv(_))
    ));
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
            assert_eq!(report, frame);
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
        b.backend().digest().clone(),
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
            b.backend().identity().clone(),
            b.backend().digest().clone(),
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
fn a_report_bound_to_a_different_launch_spec_fails_closed() {
    let b = boundary();
    let nonce = a_nonce();
    let frame = good_frame(&b, &nonce, &a_target());
    let other = Digest256::of_bytes(b"a different launch spec");
    assert!(matches!(
        b.verify_report(&frame, &nonce, &other),
        Err(SandboyLaunchError::LaunchSpecMismatch)
    ));
}

#[test]
fn a_report_with_the_wrong_backend_digest_fails_closed() {
    // Even with the right backend NAME, a report whose backend_digest is not the trusted
    // sealed object's digest must fail closed — identity is not enough.
    let b = boundary();
    let nonce = a_nonce();
    let target = a_target();
    let frame = report_frame(
        b.backend().identity().clone(),
        Digest256::of_bytes(b"an untrusted backend binary"),
        b.policy().digest(),
        &nonce,
        &target,
        ALL_ENFORCED,
    );
    assert!(matches!(
        b.verify_report(&frame, &nonce, &target),
        Err(SandboyLaunchError::BackendDigestMismatch)
    ));
}

#[test]
fn a_report_bound_to_a_different_policy_fails_closed() {
    let b = boundary();
    let nonce = a_nonce();
    let target = a_target();
    let other_policy = {
        let mut p = full_policy();
        p.timeout = Duration::from_secs(601);
        p
    };
    let frame = report_frame(
        b.backend().identity().clone(),
        b.backend().digest().clone(),
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
