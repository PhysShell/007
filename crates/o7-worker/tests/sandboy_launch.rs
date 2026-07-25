//! The confined LIVE LAUNCH / MONITOR acceptance matrix, driven through a CONTROLLED fake
//! `sandboy` backend (`CARGO_BIN_EXE_sandboy_fake`) that respects the monitor topology (it
//! stays alive and owns the target's group, never `exec`s into it). Every case asserts a
//! SPECIFIC error variant or lifecycle fact — never a bare `is_err()` — so none is vacuous:
//! against the current fail-closed stub they are RED, and the GREEN launch turns them green
//! WITHOUT rewriting the acceptance contract. The backend's misbehavior mode is configured via
//! its TRUSTED control-plane environment, never the untrusted target env.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::io::Write as _;
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

/// Acquire the controlled fake backend (a real held descriptor over its binary).
fn fake_backend() -> BackendImage {
    let path = PathBuf::from(env!("CARGO_BIN_EXE_sandboy_fake"));
    let bytes = std::fs::read(&path).expect("read the fake backend binary");
    BackendImage::acquire(&path, Digest256::of_bytes(&bytes), backend_identity())
        .expect("acquire fake backend")
}

fn backend_identity() -> BackendIdentity {
    BackendIdentity::new("sandboy-linux", "0.1.0").unwrap()
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

/// A boundary whose backend runs in the given fake `mode`, configured via the TRUSTED
/// control-plane env — NOT the target environment.
fn boundary(allow_exec: Vec<PathBuf>, mode: &str) -> SandboyBoundary {
    let mut backend_env = BTreeMap::new();
    backend_env.insert(OsString::from("O7_FAKE_MODE"), OsString::from(mode));
    SandboyBoundary::new(fake_backend(), launch_policy(allow_exec))
        .expect("valid boundary")
        .with_backend_env(backend_env)
}

/// A target that touches `marker` then exits 0 — the target env is EMPTY (mode lives on the
/// backend's control plane, not here).
fn touch_target(marker: &std::path::Path) -> BoundarySpawnSpec {
    BoundarySpawnSpec {
        executable: PathBuf::from("/bin/sh"),
        arguments: vec![
            OsString::from("-c"),
            OsString::from(format!("touch {}; exit 0", marker.display())),
        ],
        working_directory: std::env::temp_dir(),
        environment: BTreeMap::new(),
        stdin: StdinMode::Null,
    }
}

/// RED matrix helper: a misbehaving backend `mode` must fail closed with a protocol/evidence
/// error AND leave the target un-run.
async fn assert_backend_misbehavior_fails_closed(mode: &str) {
    let dir = tempfile::tempdir().unwrap();
    let marker = dir.path().join("target-ran");
    let b = boundary(vec![PathBuf::from("/bin")], mode);
    let err = b
        .spawn(touch_target(&marker))
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
async fn a_valid_report_plus_go_runs_the_target_under_a_distinct_monitor() {
    // The positive path AND the monitor-topology invariant, PROVEN: the target records its OWN
    // pid, and it must differ from the owned leader (the monitor). A backend that `exec`d into
    // the target would make the two pids equal. The target also stays alive briefly so the
    // monitor and child provably co-exist (not posthumous pid numerology).
    let dir = tempfile::tempdir().unwrap();
    let marker = dir.path().join("target-ran");
    let target_pid_file = dir.path().join("target.pid");
    let b = boundary(vec![PathBuf::from("/bin")], "ok");
    let target = BoundarySpawnSpec {
        executable: PathBuf::from("/bin/sh"),
        arguments: vec![
            OsString::from("-c"),
            OsString::from(format!(
                "echo $$ > {}; touch {}; sleep 0.3; exit 0",
                target_pid_file.display(),
                marker.display()
            )),
        ],
        working_directory: std::env::temp_dir(),
        environment: BTreeMap::new(),
        stdin: StdinMode::Null,
    };
    let mut launch = b
        .spawn(target)
        .await
        .expect("a valid report + GO launches the confined target under a monitor");
    let monitor_pid = launch.process.identity().pid;
    match &launch.evidence {
        BoundaryEvidence::Reported { attestation, .. } => {
            assert_eq!(attestation.enforcement, EnforcementLevel::FullyEnforced);
        }
        BoundaryEvidence::Unconfined => panic!("a confined launch must report evidence"),
    }
    let exit = launch.process.wait().await.expect("monitor waited");
    assert_eq!(exit, BoundaryExit::Code(0));
    assert!(marker.exists(), "the confined target must have run");
    let target_pid: i32 = std::fs::read_to_string(&target_pid_file)
        .expect("target recorded its pid")
        .trim()
        .parse()
        .expect("pid parses");
    assert_ne!(
        target_pid, monitor_pid,
        "the monitor must be a DISTINCT process from the target (no exec-in-place)"
    );
}

#[tokio::test]
async fn the_confined_target_has_a_clean_fd_table() {
    // fd non-inheritance is a SECURITY contract. Rather than probe fixed fictional numbers
    // (which leak if the OS assigns the report/control/request descriptors elsewhere), the
    // target asserts its ENTIRE inherited fd table is clean: only stdio (0,1,2). The launch
    // must normalise/close every control-plane descriptor (report, control, request, and any
    // backend-internal fd) so nothing beyond stdio survives into the confined target.
    let dir = tempfile::tempdir().unwrap();
    let leaked = dir.path().join("leaked-fd");
    let b = boundary(vec![PathBuf::from("/bin")], "ok");
    let target = BoundarySpawnSpec {
        executable: PathBuf::from("/bin/sh"),
        arguments: vec![
            OsString::from("-c"),
            // Count this shell's own fds; anything beyond 0,1,2 is an inherited leak. `ls` reads
            // /proc/<shell pid>/fd (the `$$` shell), not its own.
            OsString::from(format!(
                "n=$(ls /proc/$$/fd | wc -l); if [ \"$n\" -gt 3 ]; then touch {}; fi; exit 0",
                leaked.display()
            )),
        ],
        working_directory: std::env::temp_dir(),
        environment: BTreeMap::new(),
        stdin: StdinMode::Null,
    };
    let mut launch = b.spawn(target).await.expect("the confined target launches");
    let _ = launch.process.wait().await;
    assert!(
        !leaked.exists(),
        "the confined target must inherit ONLY stdio — no control-plane descriptors"
    );
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
    let mut backend_env = BTreeMap::new();
    backend_env.insert(OsString::from("O7_FAKE_MODE"), OsString::from("ok"));
    let b = SandboyBoundary::with_nonce_source(
        fake_backend(),
        launch_policy(vec![PathBuf::from("/bin")]),
        Arc::new(FailingNonce),
    )
    .unwrap()
    .with_backend_env(backend_env);
    let dir = tempfile::tempdir().unwrap();
    let marker = dir.path().join("target-ran");
    let err = b
        .spawn(touch_target(&marker))
        .await
        .expect_err("an RNG failure must fail the launch");
    assert!(matches!(err, BoundaryError::Spawn(_)), "got {err:?}");
    assert!(!marker.exists());
}

// --- the exact sealed-memfd probe (RED) ---

#[cfg(target_os = "linux")]
#[tokio::test]
async fn the_exact_sealed_memfd_executes_and_the_report_binds_its_bytes() {
    // The load-bearing PR-3 invariant, proven LIVE with a REAL sealed memfd held by THIS
    // (parent) process. The target writes a UNIQUE marker so we know the exact bytes ran, waits
    // to a clean exit, and the returned evidence's report must bind the exact sealed bytes.
    use std::os::unix::io::AsRawFd as _;

    use nix::fcntl::{fcntl, FcntlArg, SealFlag};
    use nix::sys::memfd::{memfd_create, MemFdCreateFlag};

    let dir = tempfile::tempdir().unwrap();
    let marker = dir.path().join("sealed-ran");
    let script = format!("#!/bin/sh\ntouch {}\nexit 0\n", marker.display());
    let name = std::ffi::CString::new("o7-sealed-probe").unwrap();
    let owned = memfd_create(&name, MemFdCreateFlag::MFD_ALLOW_SEALING).expect("memfd_create");
    let mut file = std::fs::File::from(owned);
    file.write_all(script.as_bytes())
        .expect("write sealed bytes");
    let raw = file.as_raw_fd();
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

    // The exact object path is /proc/<THIS parent pid>/fd/<n> — not /proc/self (which inside
    // the confined child is the child).
    let sealed_path = PathBuf::from(format!("/proc/{}/fd/{}", std::process::id(), raw));
    let expected_target_digest = SandboyBoundary::target_digest_of(script.as_bytes());

    let b = boundary(vec![sealed_path.clone(), PathBuf::from("/bin")], "ok");
    let target = BoundarySpawnSpec {
        executable: sealed_path,
        arguments: vec![],
        working_directory: std::env::temp_dir(),
        environment: BTreeMap::new(),
        stdin: StdinMode::Null,
    };

    let mut launch = b
        .spawn(target)
        .await
        .expect("the exact sealed memfd must execute under the final policy");
    let exit = launch.process.wait().await.expect("waited");
    assert_eq!(exit, BoundaryExit::Code(0));
    assert!(marker.exists(), "the EXACT sealed bytes must have run");
    match &launch.evidence {
        BoundaryEvidence::Reported { report, .. } => {
            let decoded = o7_worker::sandbox_protocol::frame::decode(report).expect("report");
            assert_eq!(
                decoded.target_digest, expected_target_digest,
                "the report must bind the exact sealed bytes"
            );
        }
        BoundaryEvidence::Unconfined => panic!("must report evidence"),
    }
    drop(file);
}
