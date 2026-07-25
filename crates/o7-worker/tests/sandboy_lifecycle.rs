//! A2 (reworked): LIVE lifecycle invariants for the Sandboy monitor, pinned against the now-live
//! engine (A1) with NO false-green paths. The backend really exists, the target really runs, and
//! a misbehaving fake can stall or misbehave in a specific phase. Several are RED against the
//! current happy-path-only implementation (A3 will enforce them); each RED asserts the FULL
//! contract, not a partial symptom, so A3 cannot make it green cheaply:
//!
//! - `every_report_failure_reaps_the_backend_and_never_runs_the_target` — for EVERY report
//!   failure (malformed / truncated-exit / each binding mismatch / downgrade / TRAILING byte):
//!   a specific fail-closed error, the target never runs, and the backend's exact pid is reaped
//!   within a bound. RED for `trailing` (A1 authorizes the target and the backend lives on).
//! - `the_monitor_owns_the_target_tree_for_membership_and_teardown` — the target forks a live
//!   descendant; the boundary identity is the monitor leader, the LIVE owned set lists the whole
//!   tree (monitor + target + descendant), and `force_stop` reaps all of it, leaving an empty
//!   membership. RED: the fake starts the target in its own group, so the monitor owns none of it.
//! - `a_monitor_that_exits_while_the_target_lives_is_not_a_clean_run` — losing the monitor while
//!   an owned target is alive must make `wait()` return `Err`, tear the target down, and leave an
//!   empty membership. RED: A1 reports a clean `Code(0)`.
//! - `a_high_inherited_descriptor_does_not_leak_into_the_target` — an inherited non-cloexec fd at
//!   a dynamically chosen high number must be scrubbed. RED: only the control-plane fds are.
//! - `a_blocking_fifo_target_fails_closed_within_a_bound` — a target whose acquisition BLOCKS must
//!   fail closed within a bound (probed in a helper subprocess so it cannot wedge this runtime).
//!   RED: A1's blocking open never returns.
//! - `a_symlinked_target_final_component_fails_closed` — a symlinked target must be refused
//!   (O_NOFOLLOW + regular-file check, as the verifier already does). RED: A1 follows it.
//!
//! Expected GREEN (A1 already does the right thing): cancelling a launch mid-report reaps the
//! backend, and a live launch executes the SEALED target even after its source path is swapped.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
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
    Path::new(&format!("/proc/{pid}")).exists()
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

/// Bounded-poll for a pid recorded to `path` (not a single read tied to another process's exit
/// schedule). Returns the parsed pid, or `None` if it never appeared within `bound`.
async fn read_pid_bounded(path: &Path, bound: Duration) -> Option<i32> {
    let start = tokio::time::Instant::now();
    loop {
        if let Ok(s) = std::fs::read_to_string(path) {
            if let Ok(pid) = s.trim().parse::<i32>() {
                return Some(pid);
            }
        }
        if start.elapsed() >= bound {
            return None;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// Bounded-poll for a marker file's appearance.
async fn wait_file(path: &Path, bound: Duration) -> bool {
    let start = tokio::time::Instant::now();
    while start.elapsed() < bound {
        if path.exists() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    path.exists()
}

/// Closes a raw descriptor on drop — RAII so a planted sentinel is reclaimed even if the test
/// panics mid-way.
struct FdGuard(i32);
impl Drop for FdGuard {
    fn drop(&mut self) {
        let _ = nix::unistd::close(self.0);
    }
}

// --- every report failure fails closed, runs nothing, and reaps the backend ---

/// One case of the report-failure matrix. Returns `Err(reason)` (rather than panicking) so the
/// caller can run EVERY case and report all gaps at once. Each case proves three things together:
/// a specific fail-closed error, the target never ran, and the backend's exact pid was reaped.
async fn one_report_failure(mode: &str, needle: &str) -> Result<(), String> {
    let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
    let bpid = dir.path().join("backend.pid");
    let marker = dir.path().join("target-ran");
    let b = boundary(
        vec![PathBuf::from("/bin")],
        &format!("{mode};pid={}", bpid.display()),
    );
    // If the target ever starts, it leaves a marker; a fail-closed report must leave none.
    let target = sh_target(format!("touch {}; exit 0", marker.display()));

    let err = match b.spawn(target).await {
        Ok(mut launch) => {
            let _ = launch.process.force_stop().await; // never leak on the RED path
            return Err("spawn SUCCEEDED — the target was authorized on a bad report".to_owned());
        }
        Err(e) => e,
    };
    let msg = match &err {
        BoundaryError::Evidence(m) => m.clone(),
        other => {
            return Err(format!(
                "expected a fail-closed Evidence error, got {other:?}"
            ))
        }
    };
    if !msg.contains(needle) {
        return Err(format!("error {msg:?} does not mention {needle:?}"));
    }
    if marker.exists() {
        return Err("the target RAN despite the fail-closed report".to_owned());
    }
    // The backend recorded its pid before misbehaving; it must be reaped within the bound — not
    // left blocked on GO or lingering as a monitor.
    let backend_pid = read_pid_bounded(&bpid, Duration::from_secs(1))
        .await
        .ok_or_else(|| "backend never recorded its pid".to_owned())?;
    if !wait_pid_gone(backend_pid, Duration::from_secs(3)).await {
        return Err(format!("backend pid {backend_pid} was not reaped"));
    }
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn every_report_failure_reaps_the_backend_and_never_runs_the_target() {
    // (mode, a substring the fail-closed error must mention). `trailing` has no needle: A1 does
    // not reject the tail at all, so its RED is the "spawn SUCCEEDED" gap; the eventual A3 error
    // message is A3's to choose.
    let cases: [(&str, &str); 9] = [
        ("malformed", "did not deliver"),
        ("exit_before_report", "did not deliver"),
        ("wrong_nonce", "launch_nonce"),
        ("wrong_policy", "policy_digest"),
        ("wrong_target", "launch_spec_digest"),
        ("wrong_backend", "backend identity"),
        ("wrong_backend_digest", "backend digest"),
        ("partial", "full enforcement"),
        ("trailing", ""),
    ];
    let mut failures = Vec::new();
    for (mode, needle) in cases {
        if let Err(why) = one_report_failure(mode, needle).await {
            failures.push(format!("[{mode}] {why}"));
        }
    }
    assert!(
        failures.is_empty(),
        "report-failure matrix has {} gap(s) of {}:\n{}",
        failures.len(),
        cases.len(),
        failures.join("\n")
    );
}

// --- the monitor OWNS the whole target tree, for membership AND teardown ---

#[tokio::test(flavor = "multi_thread")]
async fn the_monitor_owns_the_target_tree_for_membership_and_teardown() {
    // The target forks a LIVE descendant and records both pids, then waits. Ownership must cover
    // the whole tree: the owned set includes the target, and force_stop reaps the monitor, the
    // target, and the descendant, leaving an empty membership. RED: the fake starts the target in
    // its OWN process group, so the monitor's group neither lists nor kills any of it.
    let dir = tempfile::tempdir().unwrap();
    let mpid = dir.path().join("monitor.pid");
    let tpid = dir.path().join("target.pid");
    let cpid = dir.path().join("child.pid");
    let b = boundary(
        vec![PathBuf::from("/bin")],
        &format!("ok;pid={}", mpid.display()),
    );
    let script = format!(
        "sleep 5 & echo $! > {}; echo $$ > {}; wait",
        cpid.display(),
        tpid.display()
    );
    let mut launch = b.spawn(sh_target(script)).await.expect("launch");

    let monitor_pid = read_pid_bounded(&mpid, Duration::from_secs(3))
        .await
        .expect("monitor pid");
    let target_pid = read_pid_bounded(&tpid, Duration::from_secs(3))
        .await
        .expect("target pid");
    let child_pid = read_pid_bounded(&cpid, Duration::from_secs(3))
        .await
        .expect("descendant pid");

    // The boundary's identity must BE the monitor leader, and the LIVE owned set must list the
    // whole tree — monitor + target + descendant — not just the target. Otherwise A3 could return
    // membership for the target alone and separately hunt the descendant, passing this test on an
    // incomplete `remaining_members`. Snapshot both before teardown.
    let leader_pid = launch.process.identity().pid;
    let members = launch.process.remaining_members().await.expect("members");

    // Tear down BEFORE asserting, so a RED membership check never leaks the tree.
    launch.process.force_stop().await.expect("force_stop");
    let _ = launch.process.wait().await; // reap the monitor

    let target_gone = wait_pid_gone(target_pid, Duration::from_secs(3)).await;
    let child_gone = wait_pid_gone(child_pid, Duration::from_secs(3)).await;
    let monitor_gone = wait_pid_gone(monitor_pid, Duration::from_secs(3)).await;
    let members_after = launch
        .process
        .remaining_members()
        .await
        .expect("members after");

    assert_eq!(
        leader_pid, monitor_pid,
        "the boundary's identity must be the monitor leader"
    );
    for expected in [monitor_pid, target_pid, child_pid] {
        assert!(
            members.iter().any(|member| member.pid == expected),
            "owned membership is missing pid {expected}: {members:?}"
        );
    }
    assert!(target_gone, "force_stop must tear the target down");
    assert!(
        child_gone,
        "force_stop must tear the target's DESCENDANT down too (pid {child_pid})"
    );
    assert!(monitor_gone, "the monitor must be reaped after teardown");
    assert!(
        members_after.is_empty(),
        "the owned set must be empty after teardown (got {members_after:?})"
    );
}

// --- monitor death with a live owned target is a boundary FAILURE ---

#[tokio::test(flavor = "multi_thread")]
async fn a_monitor_that_exits_while_the_target_lives_is_not_a_clean_run() {
    // The fake abandons the target (exits without waiting it). Losing the monitor while an owned
    // target is still alive must surface as a boundary FAILURE — not a clean `Code(0)` — AND the
    // abandoned target must be torn down, not orphaned alive. RED: A1 reports a clean exit.
    let dir = tempfile::tempdir().unwrap();
    let tpid = dir.path().join("target.pid");
    let b = boundary(vec![PathBuf::from("/bin")], "monitor_exit_early");
    let mut launch = b
        .spawn(sh_target(format!("echo $$ > {}; sleep 5", tpid.display())))
        .await
        .expect("launch");

    let target_pid = read_pid_bounded(&tpid, Duration::from_secs(3))
        .await
        .expect("target pid");

    let outcome = launch.process.wait().await;
    assert!(
        outcome.is_err(),
        "monitor ownership loss must NOT be reported as a clean exit, got {outcome:?}"
    );
    assert!(
        wait_pid_gone(target_pid, Duration::from_secs(3)).await,
        "the abandoned target must be torn down, not orphaned alive"
    );
    let members = launch.process.remaining_members().await.expect("members");
    assert!(
        members.is_empty(),
        "no owned members may remain after monitor loss (got {members:?})"
    );
}

// --- a high inherited descriptor is scrubbed from the target ---

#[tokio::test(flavor = "multi_thread")]
async fn a_high_inherited_descriptor_does_not_leak_into_the_target() {
    use std::os::fd::AsRawFd as _;
    // Plant a NON-cloexec sentinel at a dynamically chosen high fd: F_DUPFD returns the lowest
    // free fd >= 100 with cloexec CLEARED on the copy, so we never clobber a number the harness or
    // a concurrent test is using. The copy inherits into the backend and, unless the target's fds
    // are fully scrubbed, into the target. A RAII guard reclaims it even on panic. (No `unsafe`:
    // pipe + fcntl are nix's safe API; the sentinel is closed by the guard.)
    let (r, _w) = nix::unistd::pipe().unwrap();
    let sentinel = nix::fcntl::fcntl(r.as_raw_fd(), nix::fcntl::FcntlArg::F_DUPFD(100)).unwrap();
    let _guard = FdGuard(sentinel);

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
    assert!(
        !leaked.exists(),
        "a high inherited descriptor (fd {sentinel}) must be scrubbed from the target"
    );
}

// --- cancel mid-report reaps the backend (expected GREEN) ---

#[tokio::test(flavor = "multi_thread")]
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
        // Poll the spawn future until the backend has recorded its pid (now stalled mid-report),
        // then DROP it (cancel).
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

// --- a blocking-acquisition target fails closed within a bound (probed out-of-process) ---

#[tokio::test(flavor = "multi_thread")]
async fn a_blocking_fifo_target_fails_closed_within_a_bound() {
    use nix::sys::stat::Mode;
    // A FIFO with no writer BLOCKS on open. Acquisition must fail closed within a bound instead of
    // blocking forever BEFORE the first `.await`. We drive one spawn in a helper SUBPROCESS so a
    // blocking acquisition wedges the helper, not this runtime; the parent bounds the helper and
    // kills it. RED today: the helper never returns and we must kill it. GREEN once acquisition is
    // non-blocking: the helper exits 3 (fail-closed).
    let dir = tempfile::tempdir().unwrap();
    let fifo = dir.path().join("fifo");
    nix::unistd::mkfifo(&fifo, Mode::from_bits_truncate(0o644)).unwrap();

    let helper = env!("CARGO_BIN_EXE_spawn_probe");
    let backend = env!("CARGO_BIN_EXE_sandboy_fake");
    let mut child = tokio::process::Command::new(helper)
        .arg(backend)
        .arg(dir.path())
        .arg(&fifo)
        .kill_on_drop(true)
        .spawn()
        .expect("spawn probe helper");

    match tokio::time::timeout(Duration::from_secs(5), child.wait()).await {
        Ok(Ok(status)) => assert_eq!(
            status.code(),
            Some(3),
            "a blocking FIFO target must fail closed (probe exited {status:?})"
        ),
        Ok(Err(e)) => panic!("probe helper wait failed: {e}"),
        Err(_) => {
            let _ = child.kill().await;
            panic!("acquisition of a blocking FIFO target did not return within the bound");
        }
    }
}

// --- a symlinked target's final component fails closed ---

#[tokio::test(flavor = "multi_thread")]
async fn a_symlinked_target_final_component_fails_closed() {
    // Hardened acquisition must open the target with O_NOFOLLOW + a regular-file check (as the
    // verifier already does), so a symlink in the final component is refused even when it points
    // at a real executable. RED today: acquisition follows the link and launches. Both the link's
    // directory and the resolved target are on the exec allowlist so the LEXICAL exec gate cannot
    // mask the symlink refusal we are pinning.
    let dir = tempfile::tempdir().unwrap();
    let link = dir.path().join("link");
    std::os::unix::fs::symlink("/bin/sh", &link).unwrap();
    let b = boundary(vec![dir.path().to_path_buf(), PathBuf::from("/bin")], "ok");
    let spec = BoundarySpawnSpec {
        executable: link,
        arguments: vec![],
        working_directory: std::env::temp_dir(),
        environment: BTreeMap::new(),
        stdin: StdinMode::Null,
    };
    match tokio::time::timeout(Duration::from_secs(5), b.spawn(spec)).await {
        Ok(Ok(mut launch)) => {
            let _ = launch.process.force_stop().await;
            panic!("a symlinked target must fail closed (O_NOFOLLOW), but the launch succeeded");
        }
        Ok(Err(_)) => {} // fail closed — the desired hardened behavior
        Err(_) => panic!("symlinked-target acquisition did not complete within the bound"),
    }
}

// --- a live launch executes the SEALED target, not a swapped source (expected GREEN) ---

#[tokio::test(flavor = "multi_thread")]
async fn a_live_launch_executes_the_sealed_target_not_a_swapped_source() {
    // End-to-end hash->exec through the LIVE engine (not the sealed-object unit alone): seal a
    // script that writes marker A, HOLD the backend just before it starts the target, rewrite the
    // SOURCE path to write marker B, then release. The sealed memfd must run the ORIGINAL bytes
    // (marker A) — the swapped source (marker B) must never execute.
    use std::os::unix::fs::PermissionsExt as _;
    let dir = tempfile::tempdir().unwrap();
    let prog = dir.path().join("prog");
    let marker_a = dir.path().join("ran-A");
    let marker_b = dir.path().join("ran-B");
    std::fs::write(&prog, format!("#!/bin/sh\ntouch {}\n", marker_a.display())).unwrap();
    std::fs::set_permissions(&prog, std::fs::Permissions::from_mode(0o755)).unwrap();

    let ready = dir.path().join("ready");
    let release = dir.path().join("release");
    let b = boundary(
        vec![dir.path().to_path_buf()],
        &format!(
            "hold_before_target;ready={};release={}",
            ready.display(),
            release.display()
        ),
    );
    let spec = BoundarySpawnSpec {
        executable: prog.clone(),
        arguments: vec![],
        working_directory: std::env::temp_dir(),
        environment: BTreeMap::new(),
        stdin: StdinMode::Null,
    };

    // spawn() seals `prog` up front, then the backend blocks on `ready`. Concurrently: wait for
    // that hold, swap the source to marker B, release. Both branches share the one task.
    let driver = async {
        assert!(
            wait_file(&ready, Duration::from_secs(5)).await,
            "backend never reached the pre-target hold"
        );
        std::fs::write(&prog, format!("#!/bin/sh\ntouch {}\n", marker_b.display())).unwrap();
        std::fs::write(&release, b"go").unwrap();
    };
    let (spawned, ()) = tokio::join!(b.spawn(spec), driver);
    let mut launch = spawned.expect("launch");
    let _ = launch.process.wait().await;

    assert!(
        marker_a.exists(),
        "the SEALED original (marker A) must have executed"
    );
    assert!(
        !marker_b.exists(),
        "the swapped source (marker B) must NOT have executed"
    );
}
