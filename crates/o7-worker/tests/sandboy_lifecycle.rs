//! LIVE lifecycle invariants for the Sandboy monitor, ENFORCED (all GREEN) against the live engine
//! with no false-green paths. The backend really exists, the target really runs, and a misbehaving
//! fake can stall or misbehave in a specific phase — each test asserts the FULL contract, not a
//! partial symptom:
//!
//! - `every_report_failure_reaps_the_backend_and_never_runs_the_target` — for EVERY report failure
//!   (malformed / truncated-exit / each binding mismatch / downgrade / TRAILING byte): a specific
//!   fail-closed error, the target never runs, and the backend's exact pid is reaped within a bound.
//! - `a_backend_that_forks_before_a_bad_report_is_fully_reaped` — cleanup drains the whole owned
//!   GROUP (leader + a pre-report descendant), not merely the leader.
//! - `no_control_descriptor_leaks_to_a_concurrent_sibling` — the CLOEXEC-from-creation control
//!   socket never leaks into a process spawned concurrently with a launch (no inheritable window).
//! - `the_monitor_owns_the_target_tree_for_membership_and_teardown` — the boundary identity is the
//!   monitor leader, the LIVE owned set lists the whole tree (monitor + target + descendant), and
//!   `force_stop` reaps all of it, leaving an empty membership.
//! - `a_monitor_that_exits_while_the_target_lives_is_not_a_clean_run` — losing the monitor while an
//!   owned target is alive makes `wait()` return `Err`, tears the target down, empties membership.
//! - `a_high_inherited_descriptor_does_not_leak_into_the_target` — an inherited non-cloexec fd at a
//!   dynamically chosen high number is scrubbed.
//! - `a_blocking_fifo_target_fails_closed_within_a_bound` — a target whose acquisition would block
//!   fails closed within a bound (probed in a helper subprocess so it cannot wedge this runtime).
//! - `a_symlinked_target_final_component_fails_closed` — a symlinked target is refused (O_NOFOLLOW
//!   + regular-file check), while the frozen `/proc/<pid>/fd/<n>` sealed target stays runnable.
//! - `cancelling_a_launch_mid_report_reaps_the_backend`, and
//!   `a_live_launch_executes_the_sealed_target_not_a_swapped_source` (hash→exec through the live
//!   engine).
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
        // A REGULAR-file shell (dash), not the /bin/sh symlink — hardened acquisition rejects a
        // symlink final component (O_NOFOLLOW). Still under /bin for the lexical exec gate.
        executable: PathBuf::from("/bin/dash"),
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

// --- the control transport never leaks into a concurrently-spawned sibling ---

/// The SOCKET descriptors (`socket:[inode]`) this process currently holds. During a launch the
/// control transport is the only AF_UNIX socket in play (tokio's I/O driver uses epoll+eventfd,
/// `anon_inode:[…]`, not sockets), so this set IS the set of control-socket inodes the sibling must
/// never inherit — identified by object, not by a bare fd count that a non-CLOEXEC pipe would trip.
fn parent_socket_targets() -> std::collections::BTreeSet<String> {
    let mut set = std::collections::BTreeSet::new();
    if let Ok(entries) = std::fs::read_dir("/proc/self/fd") {
        for entry in entries.flatten() {
            if let Ok(target) = std::fs::read_link(entry.path()) {
                let target = target.to_string_lossy().into_owned();
                if target.starts_with("socket:") {
                    set.insert(target);
                }
            }
        }
    }
    set
}

#[tokio::test(flavor = "multi_thread")]
async fn no_control_descriptor_leaks_to_a_concurrent_sibling() {
    // The control transport is a SOCKET that is CLOEXEC FROM CREATION (`UnixStream::pair`) and mapped
    // onto the backend's stdin by the child's own dup — so there is no window in which it is
    // inheritable by an unrelated process. Prove it CONTROL-TRANSPORT-SPECIFICALLY: race a stream of
    // launches against a stream of sibling spawns and assert no sibling inherits ANY socket the
    // parent holds (the control socket is the only one in play). This does NOT count unrelated
    // inherited plumbing — e.g. the non-CLOEXEC Cargo jobserver `pipe:[…]` present on CI runners —
    // which a bare "zero fd above stdio" oracle wrongly flagged. A regression to a non-CLOEXEC
    // transport would let a sibling inherit a `socket:[inode]` the parent also holds.
    //
    // A separate deterministic test (`a_fresh_control_socket_pair_is_cloexec_on_both_ends`) pins the
    // CLOEXEC-from-creation property directly; this one pins the live no-inheritable-window race.
    let probe = env!("CARGO_BIN_EXE_fd_probe");
    let launches = tokio::spawn(async {
        for _ in 0..30 {
            let dir = tempfile::tempdir().unwrap();
            let marker = dir.path().join("m");
            let b = boundary(vec![PathBuf::from("/bin")], "ok");
            if let Ok(mut launch) = b
                .spawn(sh_target(format!("touch {}; exit 0", marker.display())))
                .await
            {
                let _ = launch.process.wait().await;
            }
        }
    });

    let mut leaks: Vec<String> = Vec::new();
    for _ in 0..120 {
        // Snapshot the control-socket inodes the parent holds RIGHT NOW (a launch may be mid-
        // handshake), then fork the sibling and read the sockets it inherited.
        let parent_sockets = parent_socket_targets();
        let out = tokio::process::Command::new(probe)
            .output()
            .await
            .expect("probe runs");
        // The probe MUST succeed: an enumeration/parse/readlink failure must never masquerade as
        // "no leak".
        assert!(
            out.status.success(),
            "fd_probe failed to enumerate its descriptors: {:?}",
            String::from_utf8_lossy(&out.stderr)
        );
        let inherited_sockets: std::collections::BTreeSet<String> =
            String::from_utf8_lossy(&out.stdout)
                .lines()
                .filter(|l| l.starts_with("socket:"))
                .map(str::to_owned)
                .collect();
        // Any socket the sibling inherited is a leak — the parent's only socket during a launch is
        // the control transport; `parent-held` records whether it matched a live parent socket.
        for s in inherited_sockets {
            let record = format!("{s} (parent-held={})", parent_sockets.contains(&s));
            if !leaks.contains(&record) {
                leaks.push(record);
            }
        }
        if launches.is_finished() {
            break;
        }
    }
    let _ = launches.await;
    assert!(
        leaks.is_empty(),
        "a concurrently-spawned sibling inherited the parent's control socket(s) — the control \
         transport must be CLOEXEC with no inheritable window; leaked: {leaks:?}"
    );
}

// --- the freshly created control socket pair is CLOEXEC on both ends (deterministic) ---

#[test]
fn a_fresh_control_socket_pair_is_cloexec_on_both_ends() {
    use std::os::fd::AsRawFd as _;
    // The transport's race-freedom rests on both ends being CLOEXEC FROM CREATION. Prove that
    // structural property deterministically (the concurrent race above is the stress complement).
    let (a, b) = std::os::unix::net::UnixStream::pair().expect("socketpair");
    for end in [a.as_raw_fd(), b.as_raw_fd()] {
        let flags = nix::fcntl::fcntl(end, nix::fcntl::FcntlArg::F_GETFD).expect("F_GETFD");
        let cloexec =
            nix::fcntl::FdFlag::from_bits_truncate(flags).contains(nix::fcntl::FdFlag::FD_CLOEXEC);
        assert!(
            cloexec,
            "a freshly created control socket end must be CLOEXEC from creation"
        );
    }
}

// --- a broken backend that forks before a bad report is fully reaped (group drain) ---

#[tokio::test(flavor = "multi_thread")]
async fn a_backend_that_forks_before_a_bad_report_is_fully_reaped() {
    // Cleanup must prove the whole owned GROUP drained, not merely that the leader was reaped. A
    // broken backend forks a live descendant (in its own owned group) BEFORE delivering a malformed
    // report; the fail-closed path must kill the leader AND the descendant and leave an empty group.
    let dir = tempfile::tempdir().unwrap();
    let bpid = dir.path().join("backend.pid");
    let dpid = dir.path().join("desc.pid");
    let marker = dir.path().join("target-ran");
    let b = boundary(
        vec![PathBuf::from("/bin")],
        &format!(
            "fork_before_bad_report;pid={};desc={}",
            bpid.display(),
            dpid.display()
        ),
    );
    let err = b
        .spawn(sh_target(format!("touch {}; exit 0", marker.display())))
        .await
        .expect_err("a malformed report must fail closed");
    assert!(matches!(err, BoundaryError::Evidence(_)), "got {err:?}");

    let leader = read_pid_bounded(&bpid, Duration::from_secs(3))
        .await
        .expect("backend leader pid");
    let descendant = read_pid_bounded(&dpid, Duration::from_secs(3))
        .await
        .expect("backend descendant pid");
    assert!(
        wait_pid_gone(leader, Duration::from_secs(3)).await,
        "the backend leader must be reaped"
    );
    assert!(
        wait_pid_gone(descendant, Duration::from_secs(3)).await,
        "the pre-report descendant must be reaped too (group drain, not just leader)"
    );
    assert!(!marker.exists(), "the target must never run");
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

#[tokio::test(flavor = "multi_thread")]
async fn cancelling_a_launch_after_a_fork_reaps_the_whole_group() {
    // A broken backend forks a live descendant, then stalls mid-report. Dropping the spawn future
    // (cancellation) must reap the whole owned GROUP — the monitor leader AND the pre-report
    // descendant — not merely the leader (kill_on_drop only signals the leader PID).
    let dir = tempfile::tempdir().unwrap();
    let bpid = dir.path().join("backend.pid");
    let dpid = dir.path().join("desc.pid");
    let b = boundary(
        vec![PathBuf::from("/bin")],
        &format!(
            "fork_then_stall_mid_report;pid={};desc={}",
            bpid.display(),
            dpid.display()
        ),
    );
    {
        let fut = b.spawn(sh_target("exit 0".to_owned()));
        tokio::pin!(fut);
        let deadline = tokio::time::Instant::now() + Duration::from_secs(6);
        loop {
            tokio::select! {
                _ = &mut fut => break,
                _ = tokio::time::sleep(Duration::from_millis(25)) => {}
            }
            if (bpid.exists() && dpid.exists()) || tokio::time::Instant::now() > deadline {
                break;
            }
        }
    } // fut dropped -> cancellation with a live descendant
    let leader = read_pid_bounded(&bpid, Duration::from_secs(3))
        .await
        .expect("leader pid");
    let descendant = read_pid_bounded(&dpid, Duration::from_secs(3))
        .await
        .expect("descendant pid");
    assert!(
        wait_pid_gone(leader, Duration::from_secs(3)).await,
        "a cancelled launch must reap the monitor leader"
    );
    assert!(
        wait_pid_gone(descendant, Duration::from_secs(3)).await,
        "a cancelled launch must reap the pre-report descendant too (group, not just leader)"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn cancelling_a_launch_after_go_reaps_the_live_target_tree() {
    // The frozen cancel-safety contract's hardest window: GO is delivered and the backend has
    // started the target (which forks a live descendant), but the spawn future is cancelled BEFORE
    // BoundaryLaunch is returned. The drop-path guard must reap the monitor + target + descendant.
    // A test-only post-GO barrier holds the future at exactly that instant so the cancel is
    // deterministic.
    let dir = tempfile::tempdir().unwrap();
    let bpid = dir.path().join("backend.pid");
    let tpid = dir.path().join("target.pid");
    let cpid = dir.path().join("child.pid");
    let barrier = std::sync::Arc::new(tokio::sync::Notify::new());
    let b = boundary(
        vec![PathBuf::from("/bin")],
        &format!("ok;pid={}", bpid.display()),
    )
    .with_post_go_barrier(barrier.clone());
    let script = format!(
        "echo $$ > {}; sleep 30 & echo $! > {}; sleep 30",
        tpid.display(),
        cpid.display()
    );
    {
        let fut = b.spawn(sh_target(script));
        tokio::pin!(fut);
        // Drive the future to the post-GO barrier (it blocks there) and wait until the target tree
        // is live, then drop it.
        let deadline = tokio::time::Instant::now() + Duration::from_secs(8);
        loop {
            tokio::select! {
                _ = &mut fut => break, // guards against an unexpected non-blocking completion
                _ = tokio::time::sleep(Duration::from_millis(25)) => {}
            }
            if (tpid.exists() && cpid.exists()) || tokio::time::Instant::now() > deadline {
                break;
            }
        }
    } // fut dropped AT the barrier -> cancellation with a live target tree
    let leader = read_pid_bounded(&bpid, Duration::from_secs(3))
        .await
        .expect("monitor pid");
    let target = read_pid_bounded(&tpid, Duration::from_secs(3))
        .await
        .expect("target pid");
    let descendant = read_pid_bounded(&cpid, Duration::from_secs(3))
        .await
        .expect("descendant pid");
    assert!(
        wait_pid_gone(leader, Duration::from_secs(3)).await,
        "cancel after GO must reap the monitor"
    );
    assert!(
        wait_pid_gone(target, Duration::from_secs(3)).await,
        "cancel after GO must reap the live target"
    );
    assert!(
        wait_pid_gone(descendant, Duration::from_secs(3)).await,
        "cancel after GO must reap the target's descendant"
    );
}

// --- a blocking-acquisition target fails closed within a bound (probed out-of-process) ---

#[tokio::test(flavor = "multi_thread")]
async fn a_blocking_fifo_target_fails_closed_within_a_bound() {
    use nix::sys::stat::Mode;
    // A FIFO with no writer BLOCKS on open. Acquisition must fail closed within a bound instead of
    // blocking forever BEFORE the first `.await`. We drive one spawn in a helper SUBPROCESS so a
    // blocking acquisition wedges the helper, not this runtime; the parent bounds the helper and
    // kills it. Exit 0 is the helper's PASS: the target was refused exactly as a
    // `BoundaryError::TargetAcquisition` (AGENTS.md §2 — 0 for PASS). A launched target would be
    // FAIL (1) and an infrastructure fault ERROR (4, see
    // `a_non_target_boundary_error_is_not_reported_as_a_target_refusal`); only the hardened refusal
    // earns the PASS here.
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
            Some(0),
            "a blocking FIFO target must fail closed as a target refusal — the helper's PASS (0); \
             probe exited {status:?}"
        ),
        Ok(Err(e)) => panic!("probe helper wait failed: {e}"),
        Err(_) => {
            let _ = child.kill().await;
            panic!("acquisition of a blocking FIFO target did not return within the bound");
        }
    }
}

// --- a NON-target boundary error is not laundered into a target-acquisition refusal ---

#[tokio::test(flavor = "multi_thread")]
async fn a_non_target_boundary_error_is_not_reported_as_a_target_refusal() {
    // The helper's PASS (0) means ONE thing: the hardened target acquisition refused an unacceptable
    // target (a FIFO/symlink/non-regular object). Prove that meaning is real by forcing a DIFFERENT
    // failure — a fully acquirable, permitted, regular-file target driven against a bogus "backend"
    // (`/bin/true`) that spawns, exits 0, and never delivers a report → an infrastructure
    // `BoundaryError::Evidence`, NOT a target refusal. The original `Err(_) => 3` reported EVERY error
    // alike, so this false green was indistinguishable from a genuine FIFO/symlink refusal and the
    // `a_blocking_fifo_target_...` assertion did not actually pin target acquisition. The classified
    // helper must exit 4 (infrastructure ERROR), never 0 (the refusal PASS).
    let dir = tempfile::tempdir().unwrap();
    // A REGULAR file (not a symlink like /bin/sh often is) so acquisition SUCCEEDS and the only
    // failure left is the bogus backend. Its contents are irrelevant — acquisition only proves
    // S_ISREG; the launch dies at the report stage before any exec.
    let target = dir.path().join("acquirable-target");
    std::fs::write(
        &target,
        b"a regular, acquirable file; never actually exec'd\n",
    )
    .unwrap();

    let helper = env!("CARGO_BIN_EXE_spawn_probe");
    let mut child = tokio::process::Command::new(helper)
        .arg("/bin/true") // bogus backend: spawns, exits 0, never delivers a report → Evidence error
        .arg(dir.path()) // allow-exec dir — the target is under it, so the permit check passes
        .arg(&target) // regular + permitted + acquirable → acquisition SUCCEEDS
        .kill_on_drop(true)
        .spawn()
        .expect("spawn probe helper");

    // The bound comfortably exceeds the backend report-wait window (a silent backend makes the
    // launch wait for its report before failing with Evidence); we assert the CLASSIFICATION, not
    // latency.
    match tokio::time::timeout(Duration::from_secs(25), child.wait()).await {
        Ok(Ok(status)) => assert_eq!(
            status.code(),
            Some(4),
            "a non-target (infrastructure) boundary error must be classified ERROR (4), not the \
             target-acquisition refusal PASS (0); got {status:?}"
        ),
        Ok(Err(e)) => panic!("probe helper wait failed: {e}"),
        Err(_) => {
            let _ = child.kill().await;
            panic!("spawn_probe did not return within the bound for a non-target error");
        }
    }
}

// --- an unexpectedly launched target is a FAIL, distinct from the refusal PASS ---

#[tokio::test(flavor = "multi_thread")]
async fn an_unexpectedly_launched_target_is_a_fail_not_the_refusal_pass() {
    // The third semantic outcome: a target that should have been refused but actually LAUNCHES is a
    // FAIL (1) — never conflated with the refusal PASS (0) or an infrastructure ERROR (4). Drive a
    // fully valid launch with the REAL fake backend (the helper hardcodes mode `ok`) and a
    // Cargo-provided compiled binary as the target: guaranteed to exist and be a regular file (unlike
    // /bin/dash, absent on some minimal systems). Exec'd with no arguments and StdinMode::Null it
    // exits immediately (usage) without reading stdin, so `spawn` SUCCEEDS, the helper force-stops the
    // launch and reports FAIL (1). Its parent directory is the allow-exec dir. This pins that 0 is
    // reserved for the refusal PASS and cannot be reached by a target that ran.
    let helper = env!("CARGO_BIN_EXE_spawn_probe");
    let backend = env!("CARGO_BIN_EXE_sandboy_fake");
    let target = env!("CARGO_BIN_EXE_spawn_probe");
    let allow_dir = Path::new(target)
        .parent()
        .expect("the compiled target binary has a parent directory");
    let mut child = tokio::process::Command::new(helper)
        .arg(backend)
        .arg(allow_dir)
        .arg(target)
        .kill_on_drop(true)
        .spawn()
        .expect("spawn probe helper");

    match tokio::time::timeout(Duration::from_secs(25), child.wait()).await {
        Ok(Ok(status)) => assert_eq!(
            status.code(),
            Some(1),
            "a target that unexpectedly launched must be FAIL (1), not the refusal PASS (0) nor an \
             ERROR (4); got {status:?}"
        ),
        Ok(Err(e)) => panic!("probe helper wait failed: {e}"),
        Err(_) => {
            let _ = child.kill().await;
            panic!("spawn_probe did not return within the bound for a launched target");
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

// --- the live backend argv names the SEALED descriptor (the same construction the contract inspects) ---

#[tokio::test(flavor = "multi_thread")]
async fn the_backend_receives_the_sealed_descriptor_as_its_exec_argv() {
    // The GREEN half of the argv-construction contract: the REAL launch passes the SEALED target
    // descriptor (`/proc/<pid>/fd/<n>`) as the backend's exec argument, NOT the caller's source path
    // (`/bin/dash`). Together with the contract test that pins `backend_spawn_spec` naming the seal,
    // and the production fact that `spawn` builds its argv through that same `backend_spawn_spec`,
    // this proves both surfaces consume ONE construction — not two implementations coincidentally
    // equal. The fake records the exact argument it received after `--`.
    let dir = tempfile::tempdir().unwrap();
    let arg_file = dir.path().join("target-arg");
    let b = boundary(
        vec![PathBuf::from("/bin")],
        &format!("ok;target_arg={}", arg_file.display()),
    );
    let mut launch = b
        .spawn(sh_target("exit 0".to_owned()))
        .await
        .expect("launch");
    let _ = launch.process.wait().await;

    assert!(
        wait_file(&arg_file, Duration::from_secs(3)).await,
        "the backend never recorded the exec argument it received"
    );
    let recorded = std::fs::read_to_string(&arg_file).expect("read recorded exec arg");
    let recorded = recorded.trim();
    assert!(
        recorded.starts_with("/proc/") && recorded.contains("/fd/"),
        "the backend's exec argument must be the SEALED /proc/<pid>/fd/<n> descriptor, got {recorded:?}"
    );
    assert_ne!(
        recorded, "/bin/dash",
        "the caller's SOURCE path must never be the backend's exec argument"
    );
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
