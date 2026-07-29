//! VB-1 real-cgroup acceptance for the monitor-owned cgroup v2 leaf, mirroring the frozen Vertical B
//! oracles (`o7-worker/tests/sandbox_confinement.rs`) AND proving the fail-closed teardown: the
//! monitor + target + an ordinary child + a DOUBLE-FORKED descendant live in ONE dedicated non-root
//! leaf; teardown is `cgroup.kill` after the monitor is proven OUT, drains the tree, and removes the
//! directory; the deadline is ABSOLUTE (a large settle cannot let the target outlive it); and every
//! teardown step fails closed — a forced move-out / kill-write / drain-read failure never invokes a
//! later step, never self-kills the monitor, and never reports `drained`.
//!
//! Every test is `#[ignore]`d and needs REAL delegated cgroup v2 with a writable `cgroup.kill`: the
//! hosted gate cannot delegate cgroups, so these run on the confinement runner (or a delegated host)
//! via `cargo test -p sandboy --features test-harness -- --include-ignored`. A capability guard skips
//! (does not fail) if delegation is unavailable.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

fn backend_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_sandboy"))
}

/// Whether this host provides a delegated cgroup v2 leaf with a writable `cgroup.kill`. Skips
/// (returns false) rather than failing when it does not, so `--include-ignored` is safe anywhere.
fn cgroup_delegation_available() -> bool {
    let Ok(own) = std::fs::read_to_string("/proc/self/cgroup") else {
        return false;
    };
    let Some(path) = own.lines().find_map(|l| l.strip_prefix("0::")) else {
        return false;
    };
    let base = PathBuf::from(format!("/sys/fs/cgroup{}", path.trim()));
    let probe = base.join(format!("o7cg-cap-{}", std::process::id()));
    if std::fs::create_dir(&probe).is_err() {
        return false;
    }
    let writable = std::fs::OpenOptions::new()
        .write(true)
        .open(probe.join("cgroup.kill"))
        .is_ok();
    let _ = std::fs::remove_dir(&probe);
    writable
}

fn parse_result(path: &Path) -> BTreeMap<String, String> {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter_map(|l| l.split_once('=').map(|(k, v)| (k.to_owned(), v.to_owned())))
        .collect()
}

fn members_of(result: &BTreeMap<String, String>) -> Vec<i32> {
    result
        .get("members")
        .map(|s| s.split(',').filter_map(|p| p.trim().parse().ok()).collect())
        .unwrap_or_default()
}

fn field<'a>(result: &'a BTreeMap<String, String>, key: &str) -> &'a str {
    result.get(key).map(String::as_str).unwrap_or("")
}

fn read_pid(path: &Path) -> Option<i32> {
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(3) {
        if let Ok(s) = std::fs::read_to_string(path) {
            if let Ok(pid) = s.trim().parse() {
                return Some(pid);
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    None
}

fn pid_alive(pid: i32) -> bool {
    Path::new(&format!("/proc/{pid}")).exists()
}

/// Best-effort cleanup of a leaf a fail-closed test intentionally left behind (its tree may still be
/// alive). We are not in the leaf, so `cgroup.kill` is safe.
fn cleanup_leaf(leaf: &Path) {
    let _ = std::fs::write(leaf.join("cgroup.kill"), "1");
    for _ in 0..100 {
        let empty = std::fs::read_to_string(leaf.join("cgroup.procs"))
            .map(|s| s.trim().is_empty())
            .unwrap_or(true);
        if empty {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let _ = std::fs::remove_dir(leaf);
}

/// Run `sandboy __cgroup-run` synchronously; `fault` sets the test-only `O7_CG_FAULT` control knob.
/// Returns (exit_code, parsed result).
fn run_harness(
    result: &Path,
    deadline_ms: u64,
    settle_ms: u64,
    fault: Option<&str>,
    target: &[&str],
) -> (i32, BTreeMap<String, String>) {
    let mut cmd = Command::new(backend_bin());
    cmd.arg("__cgroup-run")
        .arg("--result")
        .arg(result)
        .arg("--deadline-ms")
        .arg(deadline_ms.to_string())
        .arg("--settle-ms")
        .arg(settle_ms.to_string())
        .arg("--");
    for a in target {
        cmd.arg(a);
    }
    if let Some(f) = fault {
        cmd.env("O7_CG_FAULT", f);
    }
    let status = cmd.status().expect("run sandboy __cgroup-run");
    (status.code().unwrap_or(-1), parse_result(result))
}

/// A target that records its pid and sleeps (a stable tree to observe/tear down).
fn sleeper(tpid: &Path) -> String {
    format!("echo $$ > '{}'; exec sleep 30", tpid.display())
}

#[test]
#[ignore = "Vertical B: real delegated cgroup v2 + writable cgroup.kill required"]
fn the_owned_leaf_contains_the_whole_tree_and_is_removed_on_teardown() {
    if !cgroup_delegation_available() {
        eprintln!("SKIP: no delegated cgroup v2 with a writable cgroup.kill");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let (tpid, cpid, dpid, result) = (
        dir.path().join("target.pid"),
        dir.path().join("child.pid"),
        dir.path().join("desc.pid"),
        dir.path().join("result"),
    );
    // target: record own pid, fork an ORDINARY child and a DOUBLE-FORKED (reparented) descendant —
    // both inherit the leaf — then sleep.
    let script = format!(
        "echo $$ > '{tp}'; \
         /bin/sh -c 'echo $$ > \"{cp}\"; exec sleep 30' & \
         ( /bin/sh -c 'echo $$ > \"{dp}\"; exec sleep 30' & ); \
         sleep 30",
        tp = tpid.display(),
        cp = cpid.display(),
        dp = dpid.display(),
    );
    let (code, res) = run_harness(&result, 2000, 400, None, &["/bin/sh", "-c", &script]);

    let target = read_pid(&tpid).expect("target pid");
    let child = read_pid(&cpid).expect("child pid");
    let descendant = read_pid(&dpid).expect("descendant pid");
    let monitor: i32 = field(&res, "monitor_pid").parse().expect("monitor pid");
    let members = members_of(&res);
    let leaf = PathBuf::from(field(&res, "leaf"));

    assert!(
        leaf.file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with("o7cg-")),
        "leaf must be a dedicated non-root leaf, got {leaf:?}"
    );
    for (label, pid) in [
        ("monitor", monitor),
        ("target", target),
        ("child", child),
        ("descendant", descendant),
    ] {
        assert!(
            members.contains(&pid),
            "{label} ({pid}) must be in the one owned leaf (members {members:?})"
        );
    }
    assert_eq!(
        field(&res, "drained"),
        "1",
        "the leaf must drain via cgroup.kill"
    );
    assert_eq!(
        field(&res, "dir_removed"),
        "1",
        "the leaf directory must be removed"
    );
    assert_eq!(field(&res, "teardown_stage"), "ok");
    assert!(!leaf.exists(), "the leaf directory {leaf:?} must be gone");
    assert_eq!(code, 0, "a clean teardown exits 0");
}

#[test]
#[ignore = "Vertical B: real delegated cgroup v2 + writable cgroup.kill required"]
fn a_target_outliving_the_deadline_is_killed_with_its_descendants_within_the_window() {
    if !cgroup_delegation_available() {
        eprintln!("SKIP: no delegated cgroup v2 with a writable cgroup.kill");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let (tpid, dpid, survived, result) = (
        dir.path().join("target.pid"),
        dir.path().join("desc.pid"),
        dir.path().join("survived"),
        dir.path().join("result"),
    );
    let script = format!(
        "echo $$ > '{tp}'; \
         ( /bin/sh -c 'echo $$ > \"{dp}\"; exec sleep 10' & ); \
         sleep 10; touch '{s}'",
        tp = tpid.display(),
        dp = dpid.display(),
        s = survived.display(),
    );
    let (deadline, grace) = (Duration::from_millis(800), Duration::from_secs(3));
    let started = Instant::now();
    let (_code, res) = run_harness(
        &result,
        deadline.as_millis() as u64,
        300,
        None,
        &["/bin/sh", "-c", &script],
    );
    let within_window = started.elapsed() <= deadline + grace;

    let target = read_pid(&tpid).expect("target pid");
    let descendant = read_pid(&dpid).expect("descendant pid");
    let leaf = PathBuf::from(field(&res, "leaf"));

    assert_eq!(
        field(&res, "timed_out"),
        "1",
        "the target outlived the deadline"
    );
    assert!(
        !survived.exists(),
        "the target must NOT run past the deadline"
    );
    assert!(
        within_window,
        "teardown must finish inside deadline + grace ({:?})",
        started.elapsed()
    );
    assert!(
        !pid_alive(target),
        "the target ({target}) must be killed at the deadline"
    );
    assert!(
        !pid_alive(descendant),
        "the descendant ({descendant}) must be killed with the target"
    );
    assert_eq!(
        field(&res, "drained"),
        "1",
        "the tree drained via cgroup.kill"
    );
    assert_eq!(
        field(&res, "dir_removed"),
        "1",
        "the leaf directory removed"
    );
    assert!(!leaf.exists(), "the owned leaf {leaf:?} must be removed");
}

#[test]
#[ignore = "Vertical B: real delegated cgroup v2 + writable cgroup.kill required"]
fn a_deadline_shorter_than_the_settle_kills_before_the_settle_marker() {
    // The ABSOLUTE-deadline regression: settle (3s) >> deadline (200ms). The target would write a
    // marker at ~500ms (past the deadline, before the settle). A monitor that sleeps the whole settle
    // before its first deadline check lets that marker be written; an absolute deadline must not.
    if !cgroup_delegation_available() {
        eprintln!("SKIP: no delegated cgroup v2 with a writable cgroup.kill");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let (tpid, dpid, marker, result) = (
        dir.path().join("target.pid"),
        dir.path().join("desc.pid"),
        dir.path().join("past-deadline"),
        dir.path().join("result"),
    );
    let script = format!(
        "echo $$ > '{tp}'; \
         ( /bin/sh -c 'echo $$ > \"{dp}\"; exec sleep 10' & ); \
         sleep 0.5; touch '{m}'",
        tp = tpid.display(),
        dp = dpid.display(),
        m = marker.display(),
    );
    let (deadline, settle, grace) = (Duration::from_millis(200), 3000u64, Duration::from_secs(3));
    let started = Instant::now();
    let (_code, res) = run_harness(
        &result,
        deadline.as_millis() as u64,
        settle,
        None,
        &["/bin/sh", "-c", &script],
    );
    let elapsed = started.elapsed();

    let target = read_pid(&tpid).expect("target pid");
    let descendant = read_pid(&dpid).expect("descendant pid");
    let leaf = PathBuf::from(field(&res, "leaf"));

    assert!(
        !marker.exists(),
        "settle > deadline must NOT let the post-deadline marker be written"
    );
    assert_eq!(
        field(&res, "timed_out"),
        "1",
        "the target was killed at the absolute deadline"
    );
    assert!(
        elapsed <= deadline + grace,
        "must finish inside deadline + grace, took {elapsed:?}"
    );
    assert!(
        !pid_alive(target),
        "target ({target}) killed at the absolute deadline"
    );
    assert!(
        !pid_alive(descendant),
        "descendant ({descendant}) killed with the target"
    );
    assert_eq!(field(&res, "dir_removed"), "1", "the leaf must be removed");
    assert!(!leaf.exists());
}

#[test]
#[ignore = "Vertical B: real delegated cgroup v2 + writable cgroup.kill required"]
fn a_target_spawn_failure_leaves_no_cgroup() {
    if !cgroup_delegation_available() {
        eprintln!("SKIP: no delegated cgroup v2 with a writable cgroup.kill");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let result = dir.path().join("result");
    let (code, res) = run_harness(
        &result,
        2000,
        200,
        None,
        &["/nonexistent/o7-no-such-target"],
    );
    let leaf = PathBuf::from(field(&res, "leaf"));
    assert_eq!(code, 68, "a target spawn failure fails closed");
    assert_eq!(
        field(&res, "dir_removed"),
        "1",
        "the leaf must be removed on setup failure"
    );
    assert!(!leaf.exists(), "a setup failure must leave NO cgroup");
}

// --- fail-closed teardown (forced faults) ---

#[test]
#[ignore = "Vertical B: real delegated cgroup v2 + writable cgroup.kill required"]
fn a_forced_move_out_failure_does_not_kill_the_monitor_or_the_tree() {
    if !cgroup_delegation_available() {
        eprintln!("SKIP: no delegated cgroup v2 with a writable cgroup.kill");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let (tpid, result) = (dir.path().join("target.pid"), dir.path().join("result"));
    // Short deadline so the monitor reaches teardown quickly; the fault aborts BEFORE cgroup.kill.
    let (code, res) = run_harness(
        &result,
        300,
        200,
        Some("move_out"),
        &["/bin/sh", "-c", &sleeper(&tpid)],
    );
    let target = read_pid(&tpid).expect("target pid");
    let leaf = PathBuf::from(field(&res, "leaf"));

    // A DISTINCT failure code (not a signal): the monitor RAN TO COMPLETION — it did not SIGKILL
    // itself (a self-killed process has no exit code).
    assert_eq!(
        code, 72,
        "move-out failure must return its distinct code (monitor not self-killed)"
    );
    assert_eq!(field(&res, "teardown_stage"), "move_out");
    assert_eq!(
        field(&res, "drained"),
        "0",
        "no drain may be claimed after a move-out failure"
    );
    // cgroup.kill was NOT invoked — the tree is still alive.
    assert!(
        pid_alive(target),
        "the target must NOT be killed when move-out failed (no cgroup.kill)"
    );

    cleanup_leaf(&leaf);
}

#[test]
#[ignore = "Vertical B: real delegated cgroup v2 + writable cgroup.kill required"]
fn a_forced_cgroup_kill_write_failure_cannot_report_drained() {
    if !cgroup_delegation_available() {
        eprintln!("SKIP: no delegated cgroup v2 with a writable cgroup.kill");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let (tpid, result) = (dir.path().join("target.pid"), dir.path().join("result"));
    let (code, res) = run_harness(
        &result,
        300,
        200,
        Some("kill"),
        &["/bin/sh", "-c", &sleeper(&tpid)],
    );
    let target = read_pid(&tpid).expect("target pid");
    let leaf = PathBuf::from(field(&res, "leaf"));

    assert_eq!(
        code, 74,
        "a cgroup.kill write failure returns its distinct code"
    );
    assert_eq!(field(&res, "teardown_stage"), "kill");
    assert_eq!(
        field(&res, "drained"),
        "0",
        "drained must NOT be reported when the kill write failed"
    );
    // The kill was never written, so the tree is still alive.
    assert!(
        pid_alive(target),
        "the target is still alive when cgroup.kill failed to write"
    );

    cleanup_leaf(&leaf);
}

#[test]
#[ignore = "Vertical B: real delegated cgroup v2 + writable cgroup.kill required"]
fn a_forced_drain_observation_failure_cannot_report_drained() {
    if !cgroup_delegation_available() {
        eprintln!("SKIP: no delegated cgroup v2 with a writable cgroup.kill");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let (tpid, result) = (dir.path().join("target.pid"), dir.path().join("result"));
    // move-out + placement + kill SUCCEED (the tree really dies), but the drain OBSERVATION is forced
    // to fail — so `drained` must stay false: an unobserved drain is never a proven one.
    let (code, res) = run_harness(
        &result,
        300,
        200,
        Some("drain"),
        &["/bin/sh", "-c", &sleeper(&tpid)],
    );
    let leaf = PathBuf::from(field(&res, "leaf"));

    assert_eq!(
        code, 75,
        "a drain-observation failure returns its distinct code"
    );
    assert_eq!(field(&res, "teardown_stage"), "drain");
    assert_eq!(
        field(&res, "drained"),
        "0",
        "drained must NOT be reported without an observed drain"
    );

    cleanup_leaf(&leaf);
}
