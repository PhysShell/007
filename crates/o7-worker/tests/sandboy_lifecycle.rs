//! A2: LIVE lifecycle invariants for the Sandboy monitor, pinned against the now-live engine
//! (A1). These are NOT vacuous — the backend really exists, the target really runs, and a
//! misbehaving fake can stall in a specific phase. Several are RED against the current
//! happy-path-only implementation (A3 will enforce them):
//!
//! - the target is started in its OWN process group, so the monitor boundary does NOT own it
//!   (`remaining_members` misses it; `force_stop` does not kill it);
//! - a trailing byte after the single report frame is not detected;
//! - a monitor that exits while the target lives is reported as a clean exit;
//! - a high inherited descriptor is not scrubbed from the target;
//! - a non-regular / blocking target is not fail-closed within a bound.
//!
//! Two are expected GREEN (A1 already does the right thing): a report error reaps the backend,
//! and a sealed target survives a source rewrite.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::PathBuf;
use std::time::Duration;

use o7_worker::boundary::BoundarySpawnSpec;
use o7_worker::sandbox_protocol::ids::{BackendIdentity, Digest256};
use o7_worker::sandbox_protocol::policy::{NetworkPolicy, SandboxPolicy};
use o7_worker::{BackendImage, BoundaryError, ProcessBoundary, SandboyBoundary, StdinMode};

fn fake_backend() -> BackendImage {
    let path = PathBuf::from(env!("CARGO_BIN_EXE_sandboy_fake"));
    let bytes = std::fs::read(&path).expect("read the fake backend binary");
    BackendImage::acquire(
        &path,
        Digest256::of_bytes(&bytes),
        BackendIdentity::new("sandboy-linux", "0.1.0").unwrap(),
    )
    .expect("acquire fake backend")
}

fn boundary(allow_exec: Vec<PathBuf>, mode: &str) -> SandboyBoundary {
    SandboyBoundary::new(
        fake_backend(),
        SandboxPolicy {
            worktree: std::env::temp_dir(),
            allow_exec,
            network: NetworkPolicy::DenyAll,
            env_allowlist: vec![OsString::from("PATH")],
            timeout: Duration::from_secs(30),
        },
    )
    .expect("valid boundary")
    .with_backend_config(o7_worker::BackendConfig {
        fake_mode: Some(mode.to_owned()),
    })
}

fn sh_target(script: String) -> BoundarySpawnSpec {
    BoundarySpawnSpec {
        executable: PathBuf::from("/bin/sh"),
        arguments: vec![OsString::from("-c"), OsString::from(script)],
        working_directory: std::env::temp_dir(),
        environment: BTreeMap::new(),
        stdin: StdinMode::Null,
    }
}

fn pid_alive(pid: i32) -> bool {
    std::path::Path::new(&format!("/proc/{pid}")).exists()
}

/// Poll until `pid` is gone (reaped), or the bound elapses; returns whether it went away.
async fn wait_pid_gone(pid: i32, bound: Duration) -> bool {
    let start = tokio::time::Instant::now();
    while start.elapsed() < bound {
        if !pid_alive(pid) {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    !pid_alive(pid)
}

// --- report framing ---

#[tokio::test]
async fn a_valid_report_with_a_trailing_byte_fails_closed() {
    // The protocol is exactly ONE frame. A trailing byte after it must be detected and the target
    // must never be authorized. RED: the parent reads only the declared frame and misses the tail.
    let dir = tempfile::tempdir().unwrap();
    let marker = dir.path().join("target-ran");
    let b = boundary(vec![PathBuf::from("/bin")], "trailing");
    let result = b
        .spawn(sh_target(format!("touch {}; exit 0", marker.display())))
        .await;
    assert!(
        matches!(result, Err(BoundaryError::Evidence(_))),
        "a trailing byte must fail closed, got {result:?}"
    );
    assert!(
        !marker.exists(),
        "the target must not run on a malformed stream"
    );
}

// --- monitor OWNERSHIP of the target ---

#[tokio::test]
async fn the_monitor_owns_the_target_for_membership_and_teardown() {
    // The target sleeps and records its pid. The owned set must INCLUDE it, and force_stop must
    // tear it down. RED: the fake starts the target in its own process group, so the monitor's
    // group neither lists nor kills it.
    let dir = tempfile::tempdir().unwrap();
    let tpid = dir.path().join("target.pid");
    let b = boundary(vec![PathBuf::from("/bin")], "ok");
    let mut launch = b
        .spawn(sh_target(format!("echo $$ > {}; sleep 5", tpid.display())))
        .await
        .expect("launch");
    // Wait for the target to record its pid.
    let mut target_pid = 0;
    for _ in 0..80 {
        if let Ok(s) = std::fs::read_to_string(&tpid) {
            if let Ok(p) = s.trim().parse::<i32>() {
                target_pid = p;
                break;
            }
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert!(target_pid > 0, "target must record its pid");
    assert!(pid_alive(target_pid), "target must be alive");

    let members = launch.process.remaining_members().await.expect("members");
    assert!(
        members.iter().any(|m| m.pid == target_pid),
        "the owned set must INCLUDE the live target (got {members:?})"
    );

    launch.process.force_stop().await.expect("force_stop");
    assert!(
        wait_pid_gone(target_pid, Duration::from_secs(3)).await,
        "force_stop must tear the TARGET down too"
    );
}

#[tokio::test]
async fn a_monitor_that_exits_while_the_target_lives_is_not_a_clean_run() {
    // The fake abandons the target (exits without waiting it). The target keeps running unowned.
    // A correct boundary must NOT report a clean exit with a live target orphaned. RED.
    let dir = tempfile::tempdir().unwrap();
    let tpid = dir.path().join("target.pid");
    let b = boundary(vec![PathBuf::from("/bin")], "monitor_exit_early");
    let mut launch = b
        .spawn(sh_target(format!("echo $$ > {}; sleep 5", tpid.display())))
        .await
        .expect("launch");
    let _ = launch.process.wait().await;
    let target_pid: i32 = std::fs::read_to_string(&tpid)
        .expect("target pid")
        .trim()
        .parse()
        .expect("pid");
    assert!(
        !pid_alive(target_pid),
        "the target must not be orphaned alive after the monitor 'completes'"
    );
}

// --- descriptor scrubbing at a HIGH fd number ---

#[tokio::test]
async fn a_high_inherited_descriptor_does_not_leak_into_the_target() {
    // Plant a non-CLOEXEC sentinel descriptor at a HIGH number in THIS process, so it inherits
    // into the backend and (if not scrubbed) into the target. The confined target must not see
    // it. RED: only report/control/request are closed; an arbitrary inherited fd survives.
    use std::os::fd::AsRawFd as _;

    let (r, _w) = nix::unistd::pipe().unwrap();
    // dup the read end to a fixed high number and CLEAR cloexec so it inherits (no `unsafe`:
    // dup2 + fcntl are nix's safe API; the dup'd number is closed explicitly below).
    let sentinel: i32 = 60;
    nix::unistd::dup2(r.as_raw_fd(), sentinel).unwrap();
    nix::fcntl::fcntl(
        sentinel,
        nix::fcntl::FcntlArg::F_SETFD(nix::fcntl::FdFlag::empty()),
    )
    .unwrap();

    let dir = tempfile::tempdir().unwrap();
    let leaked = dir.path().join("leaked");
    let b = boundary(vec![PathBuf::from("/bin")], "ok");
    let mut launch = b
        .spawn(sh_target(format!(
            "if [ -e /proc/self/fd/{sentinel} ]; then touch {}; fi; exit 0",
            leaked.display()
        )))
        .await
        .expect("launch");
    let _ = launch.process.wait().await;
    let _ = nix::unistd::close(sentinel);
    assert!(
        !leaked.exists(),
        "a high inherited descriptor (fd {sentinel}) must be scrubbed from the target"
    );
}

// --- backend reap on a report error (expected GREEN) ---

#[tokio::test]
async fn a_report_error_reaps_the_backend() {
    let dir = tempfile::tempdir().unwrap();
    let bpid = dir.path().join("backend.pid");
    let b = boundary(
        vec![PathBuf::from("/bin")],
        &format!("malformed;pid={}", bpid.display()),
    );
    let err = b
        .spawn(sh_target("exit 0".to_owned()))
        .await
        .expect_err("malformed report fails closed");
    assert!(matches!(err, BoundaryError::Evidence(_)));
    let backend_pid: i32 = std::fs::read_to_string(&bpid)
        .expect("backend recorded its pid")
        .trim()
        .parse()
        .expect("pid");
    assert!(
        wait_pid_gone(backend_pid, Duration::from_secs(3)).await,
        "the backend must be reaped after a report error (pid {backend_pid} still present)"
    );
}

// --- cancel mid-report reaps the backend ---

#[tokio::test]
async fn cancelling_a_launch_mid_report_reaps_the_backend() {
    // The fake stalls after a PARTIAL report frame. Dropping the spawn future (cancellation) must
    // leave no backend behind.
    let dir = tempfile::tempdir().unwrap();
    let bpid = dir.path().join("backend.pid");
    let b = boundary(
        vec![PathBuf::from("/bin")],
        &format!("stall_mid_report;pid={}", bpid.display()),
    );
    {
        let fut = b.spawn(sh_target("exit 0".to_owned()));
        tokio::pin!(fut);
        // Poll the spawn future until the backend has recorded its pid (it is now stalled
        // mid-report), then DROP it (cancel).
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        loop {
            tokio::select! {
                _ = &mut fut => break,
                _ = tokio::time::sleep(Duration::from_millis(25)) => {}
            }
            if bpid.exists() || tokio::time::Instant::now() > deadline {
                break;
            }
        }
    } // fut dropped here -> cancellation
    let backend_pid: i32 = std::fs::read_to_string(&bpid)
        .expect("backend recorded its pid")
        .trim()
        .parse()
        .expect("pid");
    assert!(
        wait_pid_gone(backend_pid, Duration::from_secs(3)).await,
        "a cancelled launch must reap the backend (pid {backend_pid} still present)"
    );
}

// --- non-regular / blocking target fails closed within a bound ---

#[tokio::test]
async fn a_non_regular_target_fails_closed() {
    // A non-regular target (here a DIRECTORY) must fail closed at acquisition. This is the
    // testable, non-hanging half of the non-regular contract; the specifically-BLOCKING variant
    // (a FIFO with no writer, whose open() blocks) cannot be RED-tested without wedging a thread
    // and is pinned once A3's non-blocking hardened open lands.
    let dir = tempfile::tempdir().unwrap();
    let allow = dir.path().to_path_buf();
    let b = boundary(vec![allow.clone()], "ok");
    let target = BoundarySpawnSpec {
        executable: allow, // a directory, not a regular executable
        arguments: vec![],
        working_directory: std::env::temp_dir(),
        environment: BTreeMap::new(),
        stdin: StdinMode::Null,
    };
    let res = tokio::time::timeout(Duration::from_secs(3), b.spawn(target)).await;
    match res {
        Ok(r) => assert!(
            r.is_err(),
            "a non-regular target must fail closed, got {r:?}"
        ),
        Err(_) => panic!("acquisition of a non-regular target did not complete within the bound"),
    }
}

// --- sealed target survives a source rewrite (expected GREEN) ---

#[test]
fn a_sealed_target_survives_a_source_rewrite() {
    use std::io::Write as _;
    let mut tmp = tempfile::NamedTempFile::new().unwrap();
    tmp.write_all(b"#!/bin/sh\nexit 0\n").unwrap();
    tmp.flush().unwrap();
    let original = Digest256::of_bytes(b"#!/bin/sh\nexit 0\n");
    let sealed = o7_worker::sealed::SealedObject::stage(tmp.path()).expect("stage");
    assert_eq!(*sealed.digest(), original);
    // Rewrite the source inode in place.
    std::fs::write(tmp.path(), b"#!/bin/sh\necho pwned\n").unwrap();
    let held = std::fs::read(sealed.descriptor_path()).expect("read sealed");
    assert_eq!(
        held, b"#!/bin/sh\nexit 0\n",
        "the sealed target must be immutable"
    );
}
