//! VB-1 real-cgroup acceptance for the monitor-owned cgroup v2 leaf, mirroring the frozen Vertical B
//! oracles (`o7-worker/tests/sandbox_confinement.rs`): the monitor + target + an ordinary child + a
//! DOUBLE-FORKED (reparented) descendant all live in ONE dedicated non-root leaf; teardown is via
//! `cgroup.kill`, drains the whole tree, and REMOVES the leaf directory; a target outliving the
//! deadline is killed with its descendants inside an ABSOLUTE window; and a setup failure leaves no
//! cgroup behind.
//!
//! Every test is `#[ignore]`d and needs REAL delegated cgroup v2 with a writable `cgroup.kill` — the
//! hosted gate cannot delegate cgroups, so these run only on the confinement runner (or locally on a
//! delegated host) via `cargo test -p sandboy --features test-harness -- --include-ignored`. A
//! defensive capability guard skips (does not fail) if delegation is unavailable.
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

/// Whether this host actually provides a delegated cgroup v2 leaf with a writable `cgroup.kill`.
/// Skips (returns false) rather than failing when it does not, so `--include-ignored` is safe on a
/// non-delegated host.
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

/// Parse the harness `--result` file (`key=value` lines).
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

fn read_pid(path: &Path) -> Option<i32> {
    // Bounded: the target writes its pid shortly after spawn.
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

/// Run `sandboy __cgroup-run` synchronously; return (exit_code, parsed result).
fn run_harness(
    result: &Path,
    deadline_ms: u64,
    settle_ms: u64,
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
    let status = cmd.status().expect("run sandboy __cgroup-run");
    (status.code().unwrap_or(-1), parse_result(result))
}

#[test]
#[ignore = "Vertical B: real delegated cgroup v2 + writable cgroup.kill required"]
fn the_owned_leaf_contains_the_whole_tree_and_is_removed_on_teardown() {
    if !cgroup_delegation_available() {
        eprintln!("SKIP: no delegated cgroup v2 with a writable cgroup.kill on this host");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let tpid = dir.path().join("target.pid");
    let cpid = dir.path().join("child.pid");
    let dpid = dir.path().join("desc.pid");
    let result = dir.path().join("result");

    // target: record own pid, fork an ORDINARY child and a DOUBLE-FORKED (reparented) descendant —
    // both inherit the leaf — then sleep. The deadline bounds the run; members are captured earlier.
    let script = format!(
        "echo $$ > '{tp}'; \
         /bin/sh -c 'echo $$ > \"{cp}\"; exec sleep 30' & \
         ( /bin/sh -c 'echo $$ > \"{dp}\"; exec sleep 30' & ); \
         sleep 30",
        tp = tpid.display(),
        cp = cpid.display(),
        dp = dpid.display(),
    );
    let (code, res) = run_harness(&result, 2000, 400, &["/bin/sh", "-c", &script]);

    let target = read_pid(&tpid).expect("target pid");
    let child = read_pid(&cpid).expect("child pid");
    let descendant = read_pid(&dpid).expect("descendant pid");
    let monitor: i32 = res
        .get("monitor_pid")
        .and_then(|s| s.parse().ok())
        .expect("monitor pid");
    let members = members_of(&res);
    let leaf = PathBuf::from(res.get("leaf").expect("leaf path"));

    // A DEDICATED leaf — the harness's monitor + the whole tree, all in ONE cgroup.
    assert!(
        leaf.file_name()
            .and_then(|n| n.to_str())
            .map(|n| n.starts_with("o7cg-"))
            .unwrap_or(false),
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
    // cgroup.kill drained the whole tree AND the directory was removed.
    assert_eq!(
        res.get("drained").map(String::as_str),
        Some("1"),
        "the leaf must drain via cgroup.kill"
    );
    assert_eq!(
        res.get("dir_removed").map(String::as_str),
        Some("1"),
        "the leaf directory must be removed"
    );
    assert!(!leaf.exists(), "the leaf directory {leaf:?} must be gone");
    assert_eq!(code, 0, "a clean teardown exits 0");
}

#[test]
#[ignore = "Vertical B: real delegated cgroup v2 + writable cgroup.kill required"]
fn a_target_outliving_the_deadline_is_killed_with_its_descendants_within_the_window() {
    if !cgroup_delegation_available() {
        eprintln!("SKIP: no delegated cgroup v2 with a writable cgroup.kill on this host");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let tpid = dir.path().join("target.pid");
    let dpid = dir.path().join("desc.pid");
    let survived = dir.path().join("survived");
    let result = dir.path().join("result");

    let script = format!(
        "echo $$ > '{tp}'; \
         ( /bin/sh -c 'echo $$ > \"{dp}\"; exec sleep 10' & ); \
         sleep 10; touch '{s}'",
        tp = tpid.display(),
        dp = dpid.display(),
        s = survived.display(),
    );
    let deadline = Duration::from_millis(800);
    let grace = Duration::from_secs(3);

    // The ABSOLUTE window starts before we launch the monitor: the whole kill+drain+rmdir+exit must
    // finish inside deadline + grace.
    let started = Instant::now();
    let (_code, res) = run_harness(
        &result,
        deadline.as_millis() as u64,
        300,
        &["/bin/sh", "-c", &script],
    );
    let within_window = started.elapsed() <= deadline + grace;

    let target = read_pid(&tpid).expect("target pid");
    let descendant = read_pid(&dpid).expect("descendant pid");
    let leaf = PathBuf::from(res.get("leaf").expect("leaf path"));

    assert_eq!(
        res.get("timed_out").map(String::as_str),
        Some("1"),
        "the target outlived the deadline"
    );
    assert!(
        !survived.exists(),
        "the target must NOT run past the deadline (survived marker written)"
    );
    assert!(
        within_window,
        "teardown must complete inside deadline + grace ({:?})",
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
        res.get("drained").map(String::as_str),
        Some("1"),
        "the tree drained via cgroup.kill"
    );
    assert_eq!(
        res.get("dir_removed").map(String::as_str),
        Some("1"),
        "the leaf directory removed"
    );
    assert!(
        !leaf.exists(),
        "the owned leaf {leaf:?} must be removed after the deadline kill"
    );
}

#[test]
#[ignore = "Vertical B: real delegated cgroup v2 + writable cgroup.kill required"]
fn a_target_spawn_failure_leaves_no_cgroup() {
    if !cgroup_delegation_available() {
        eprintln!("SKIP: no delegated cgroup v2 with a writable cgroup.kill on this host");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let result = dir.path().join("result");
    // A non-existent target executable → the monitor's spawn fails AFTER it created the leaf. It must
    // tear the leaf down; no cgroup may leak.
    let (code, res) = run_harness(&result, 2000, 200, &["/nonexistent/o7-no-such-target"]);
    let leaf = PathBuf::from(res.get("leaf").expect("leaf path recorded before spawn"));
    assert_eq!(code, 68, "a target spawn failure fails closed");
    assert_eq!(
        res.get("dir_removed").map(String::as_str),
        Some("1"),
        "the leaf must be removed on setup failure"
    );
    assert!(
        !leaf.exists(),
        "a setup failure must leave NO cgroup ({leaf:?} still exists)"
    );
}
