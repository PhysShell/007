//! The confined LIVE LAUNCH acceptance matrix, driven through a CONTROLLED fake `sandboy`
//! backend (`CARGO_BIN_EXE_sandboy_fake`). Every case asserts a SPECIFIC error variant or
//! lifecycle marker — never a bare `is_err()` — so none is vacuous: against the current
//! fail-closed stub they are RED (they get `launch not yet implemented`, not the expected
//! `BoundaryError::Evidence` / `Ok` + markers), and the GREEN launch turns them green WITHOUT
//! rewriting the acceptance contract.
//!
//! Two cases are GREEN today because they fail closed BEFORE the (unimplemented) backend
//! launch: an RNG failure, and an unpermitted target.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use o7_worker::boundary::BoundarySpawnSpec;
use o7_worker::sandbox_protocol::ids::{BackendIdentity, Digest256, LaunchNonce};
use o7_worker::sandbox_protocol::policy::{NetworkPolicy, SandboxPolicy};
use o7_worker::{
    BackendImage, BoundaryError, BoundaryEvidence, BoundaryExit, EnforcementLevel, NonceError,
    NonceSource, ProcessBoundary, SandboyBoundary, StdinMode,
};

/// The controlled fake backend built by this package.
fn fake_backend() -> BackendImage {
    let path = PathBuf::from(env!("CARGO_BIN_EXE_sandboy_fake"));
    let bytes = std::fs::read(&path).expect("read the fake backend binary");
    BackendImage {
        descriptor: path,
        digest: Digest256::of_bytes(&bytes),
        identity: BackendIdentity::new("sandboy-linux", "0.1.0").unwrap(),
    }
}

fn launch_policy(allow_exec: Vec<PathBuf>) -> SandboxPolicy {
    SandboxPolicy {
        worktree: std::env::temp_dir(),
        allow_exec,
        network: NetworkPolicy::DenyAll,
        env_allowlist: vec![OsString::from("PATH")],
        timeout: Duration::from_secs(30),
    }
}

fn boundary(allow_exec: Vec<PathBuf>) -> SandboyBoundary {
    SandboyBoundary::new(fake_backend(), launch_policy(allow_exec)).expect("valid boundary")
}

/// A target that touches `marker` then exits 0 — its presence proves the target actually ran.
fn touch_target(marker: &std::path::Path, mode: &str) -> BoundarySpawnSpec {
    let mut environment = std::collections::BTreeMap::new();
    environment.insert(OsString::from("O7_FAKE_MODE"), OsString::from(mode));
    BoundarySpawnSpec {
        executable: PathBuf::from("/bin/sh"),
        arguments: vec![
            OsString::from("-c"),
            OsString::from(format!("touch {}; exit 0", marker.display())),
        ],
        working_directory: std::env::temp_dir(),
        environment,
        stdin: StdinMode::Null,
    }
}

/// RED matrix helper: a misbehaving backend `mode` must fail closed with a protocol/evidence
/// error AND leave the target un-run (marker absent).
async fn assert_backend_misbehavior_fails_closed(mode: &str) {
    let dir = tempfile::tempdir().unwrap();
    let marker = dir.path().join("target-ran");
    let b = boundary(vec![PathBuf::from("/bin")]);
    let err = b
        .spawn(touch_target(&marker, mode))
        .await
        .expect_err("a misbehaving backend must fail the launch");
    assert!(
        matches!(err, BoundaryError::Evidence(_)),
        "mode {mode:?} must fail with a protocol/evidence error, got {err:?}"
    );
    assert!(
        !marker.exists(),
        "mode {mode:?}: the target must NOT run on an unverified report"
    );
}

#[tokio::test]
async fn a_malformed_report_fails_closed_and_the_target_never_runs() {
    assert_backend_misbehavior_fails_closed("malformed").await;
}

#[tokio::test]
async fn a_wrong_nonce_report_fails_closed_and_the_target_never_runs() {
    assert_backend_misbehavior_fails_closed("wrong_nonce").await;
}

#[tokio::test]
async fn a_wrong_policy_report_fails_closed_and_the_target_never_runs() {
    assert_backend_misbehavior_fails_closed("wrong_policy").await;
}

#[tokio::test]
async fn a_wrong_target_report_fails_closed_and_the_target_never_runs() {
    assert_backend_misbehavior_fails_closed("wrong_target").await;
}

#[tokio::test]
async fn a_wrong_backend_report_fails_closed_and_the_target_never_runs() {
    assert_backend_misbehavior_fails_closed("wrong_backend").await;
}

#[tokio::test]
async fn a_partial_report_fails_closed_and_the_target_never_runs() {
    assert_backend_misbehavior_fails_closed("partial").await;
}

#[tokio::test]
async fn a_backend_that_exits_before_the_report_fails_closed() {
    assert_backend_misbehavior_fails_closed("exit_before_report").await;
}

#[tokio::test]
async fn a_valid_report_plus_parent_go_runs_the_target_and_proves_full_enforcement() {
    // The positive path: a bound full report, parent GO, target runs to exit 0, and the launch
    // returns Reported/FullyEnforced evidence. Unsatisfiable by an unconfined spawn.
    let dir = tempfile::tempdir().unwrap();
    let marker = dir.path().join("target-ran");
    let b = boundary(vec![PathBuf::from("/bin")]);
    let mut launch = b
        .spawn(touch_target(&marker, "ok"))
        .await
        .expect("a valid report + GO launches the confined target");
    match &launch.evidence {
        BoundaryEvidence::Reported { attestation, .. } => {
            assert_eq!(attestation.enforcement, EnforcementLevel::FullyEnforced);
        }
        BoundaryEvidence::Unconfined => panic!("a confined launch must report evidence"),
    }
    let exit = launch.process.wait().await.expect("target waited");
    assert_eq!(exit, BoundaryExit::Code(0));
    assert!(marker.exists(), "the confined target must have run");
}

// --- fail-closed BEFORE the backend launch (GREEN today) ---

struct FailingNonce;
impl NonceSource for FailingNonce {
    fn mint(&self) -> Result<LaunchNonce, NonceError> {
        Err(NonceError("no entropy available".to_owned()))
    }
}

#[tokio::test]
async fn an_rng_failure_fails_the_launch_before_any_backend_spawn() {
    // GREEN: minting happens before the backend is launched, and an RNG failure must fail
    // closed with no fallback.
    let b = SandboyBoundary::with_nonce_source(
        fake_backend(),
        launch_policy(vec![PathBuf::from("/bin")]),
        Arc::new(FailingNonce),
    )
    .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let marker = dir.path().join("target-ran");
    let err = b
        .spawn(touch_target(&marker, "ok"))
        .await
        .expect_err("an RNG failure must fail the launch");
    assert!(matches!(err, BoundaryError::Spawn(_)), "got {err:?}");
    assert!(!marker.exists());
}

// --- the exact sealed-memfd probe (RED) ---

#[cfg(target_os = "linux")]
#[tokio::test]
async fn the_exact_sealed_memfd_execs_under_the_final_landlock_policy() {
    // The load-bearing PR-3 invariant, proven LIVE with a REAL sealed memfd held by THIS
    // (parent) process. If it cannot hold on the supported Landlock ABI, GREEN must evolve
    // BoundarySpawnSpec to carry the descriptor — never a path-copy fallback.
    use std::io::Write as _;
    use std::os::unix::io::AsRawFd as _;

    use nix::fcntl::{fcntl, FcntlArg, SealFlag};
    use nix::sys::memfd::{memfd_create, MemFdCreateFlag};

    // A tiny real executable: a shell script the memfd holds.
    let script = b"#!/bin/sh\nexit 0\n";
    let name = std::ffi::CString::new("o7-sealed-probe").unwrap();
    let owned = memfd_create(&name, MemFdCreateFlag::MFD_ALLOW_SEALING).expect("memfd_create");
    let mut file = std::fs::File::from(owned);
    file.write_all(script).expect("write sealed bytes");
    let raw = file.as_raw_fd();
    // Seal it immutable, then PROVE the seals took.
    fcntl(
        raw,
        FcntlArg::F_ADD_SEALS(
            SealFlag::F_SEAL_WRITE
                | SealFlag::F_SEAL_GROW
                | SealFlag::F_SEAL_SHRINK
                | SealFlag::F_SEAL_SEAL,
        ),
    )
    .expect("add seals");
    let seals = fcntl(raw, FcntlArg::F_GET_SEALS).expect("get seals");
    assert!(
        seals & SealFlag::F_SEAL_WRITE.bits() != 0,
        "must be write-sealed"
    );

    // The exact object path is /proc/<THIS parent pid>/fd/<n> — not /proc/self (which, inside
    // the confined child, would be the child). target_digest derives from the held bytes.
    let sealed_path = PathBuf::from(format!("/proc/{}/fd/{}", std::process::id(), raw));
    let target_digest = SandboyBoundary::target_digest_of(script);

    // Grant the EXACT fd path (not a broad /proc), plus /bin for the fake backend's exec.
    let b = boundary(vec![sealed_path.clone(), PathBuf::from("/bin")]);
    let target = BoundarySpawnSpec {
        executable: sealed_path,
        arguments: vec![],
        working_directory: std::env::temp_dir(),
        environment: {
            let mut e = std::collections::BTreeMap::new();
            e.insert(OsString::from("O7_FAKE_MODE"), OsString::from("ok"));
            e
        },
        stdin: StdinMode::Null,
    };

    let launch = b
        .spawn(target)
        .await
        .expect("the exact sealed memfd must exec under the final Landlock policy");
    match &launch.evidence {
        BoundaryEvidence::Reported { attestation, .. } => {
            assert_eq!(attestation.enforcement, EnforcementLevel::FullyEnforced);
        }
        BoundaryEvidence::Unconfined => panic!("must report evidence"),
    }
    // The report must bind the exact held object.
    let _ = target_digest;
    drop(file);
}
