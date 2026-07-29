//! VB-1 — a dedicated, monitor-owned cgroup v2 leaf with wall-clock timeout and `cgroup.kill`
//! teardown. SAFE: only filesystem writes to cgroup control files + `std::process` — NO `unsafe`,
//! no kernel syscall crate. Placement is by INHERITANCE: the monitor moves ITSELF into the leaf,
//! then spawns the target, so the target and every descendant (ordinary child AND a double-forked,
//! reparented one) inherit the leaf; teardown moves the monitor back out and writes `cgroup.kill`,
//! killing the whole tree at once without killing the monitor, which then proves the drain and
//! removes the leaf directory.
//!
//! Compiled ONLY under the `test-harness` feature at VB-1: the production `run` path does not use it
//! yet (the backend cannot report `FullyEnforced` until Landlock/seccomp/env land in VB-2/VB-3), so
//! wiring this into the live launch is VB-4. It is exercised here by the `#[ignore]`d confinement
//! tests, which need real delegated cgroup v2 with a writable `cgroup.kill`.

use std::io::{self, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// The unified (v2) cgroup a pid belongs to: the path after `0::` in `/proc/<pid>/cgroup`.
pub(crate) fn cgroup_of(pid: i32) -> Option<String> {
    let raw = std::fs::read_to_string(format!("/proc/{pid}/cgroup")).ok()?;
    raw.lines()
        .find_map(|l| l.strip_prefix("0::").map(str::to_owned))
}

/// The absolute filesystem directory for a cgroup path (`/sys/fs/cgroup<path>`).
fn cgroup_dir(path: &str) -> PathBuf {
    PathBuf::from(format!("/sys/fs/cgroup{path}"))
}

/// This process's own delegated cgroup directory — the parent under which a leaf is created.
fn own_cgroup_dir() -> io::Result<PathBuf> {
    let pid = std::process::id() as i32;
    let path = cgroup_of(pid).ok_or_else(|| {
        io::Error::other("could not read own cgroup (0::) from /proc/self/cgroup")
    })?;
    Ok(cgroup_dir(&path))
}

/// Write `pid` into `<cgroup_dir>/cgroup.procs`, moving that process (and its threads) into the
/// cgroup. A single write per the kernel interface.
fn move_pid_into(cgroup_dir: &Path, pid: i32) -> io::Result<()> {
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .open(cgroup_dir.join("cgroup.procs"))?;
    write!(f, "{pid}")
}

/// The pids currently in a cgroup (`cgroup.procs`).
fn members(cgroup_dir: &Path) -> Vec<i32> {
    std::fs::read_to_string(cgroup_dir.join("cgroup.procs"))
        .map(|s| s.lines().filter_map(|l| l.trim().parse().ok()).collect())
        .unwrap_or_default()
}

/// Kill EVERY process in `leaf` at once via `cgroup.kill`, then poll `cgroup.procs` to empty within
/// `bound`. Returns whether the leaf drained. The monitor must already have moved ITSELF out, or it
/// would kill itself too.
fn kill_and_drain(leaf: &Path, bound: Duration) -> bool {
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .write(true)
        .open(leaf.join("cgroup.kill"))
    {
        let _ = f.write_all(b"1");
    }
    let start = Instant::now();
    loop {
        if members(leaf).is_empty() {
            return true;
        }
        if start.elapsed() >= bound {
            return members(leaf).is_empty();
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Best-effort teardown of a leaf the monitor still owns: move the monitor OUT (to `parent`), kill +
/// drain the tree, then remove the empty leaf directory. Used on the normal path AND on every setup
/// failure, so a partially-created leaf never leaks.
fn teardown(leaf: &Path, parent: &Path, drain_bound: Duration) -> (bool, bool) {
    // Move the monitor out of the leaf first (idempotent if it is not in the leaf).
    let _ = move_pid_into(parent, std::process::id() as i32);
    let drained = kill_and_drain(leaf, drain_bound);
    let dir_removed = std::fs::remove_dir(leaf).is_ok() || !leaf.exists();
    (drained, dir_removed)
}

/// The result of one monitored run, written to the harness `--result` file as `key=value` lines so
/// the confinement test can verify FACTS the monitor observed from inside the cgroup.
struct RunResult {
    leaf: PathBuf,
    monitor_pid: i32,
    members: Vec<i32>,
    timed_out: bool,
    drained: bool,
    dir_removed: bool,
    elapsed_ms: u128,
}

impl RunResult {
    fn write(&self, path: &Path) -> io::Result<()> {
        let members = self
            .members
            .iter()
            .map(i32::to_string)
            .collect::<Vec<_>>()
            .join(",");
        let body = format!(
            "leaf={}\nmonitor_pid={}\nmembers={}\ntimed_out={}\ndrained={}\ndir_removed={}\nelapsed_ms={}\n",
            self.leaf.display(),
            self.monitor_pid,
            members,
            u8::from(self.timed_out),
            u8::from(self.drained),
            u8::from(self.dir_removed),
            self.elapsed_ms,
        );
        std::fs::write(path, body)
    }
}

/// Parsed `__cgroup-run` arguments.
struct HarnessArgs {
    result: PathBuf,
    deadline: Duration,
    settle: Duration,
    target: PathBuf,
    target_args: Vec<std::ffi::OsString>,
}

fn parse_harness_args() -> Option<HarnessArgs> {
    let mut it = std::env::args_os().skip(2); // skip program name + "__cgroup-run"
    let mut result = None;
    let mut deadline_ms: Option<u64> = None;
    let mut settle_ms: Option<u64> = Some(200);
    loop {
        let flag = it.next()?;
        match flag.to_str()? {
            "--result" => result = Some(PathBuf::from(it.next()?)),
            "--deadline-ms" => deadline_ms = Some(it.next()?.to_str()?.parse().ok()?),
            "--settle-ms" => settle_ms = Some(it.next()?.to_str()?.parse().ok()?),
            "--" => break,
            _ => return None,
        }
    }
    let target = PathBuf::from(it.next()?);
    let target_args: Vec<std::ffi::OsString> = it.collect();
    Some(HarnessArgs {
        result: result?,
        deadline: Duration::from_millis(deadline_ms?),
        settle: Duration::from_millis(settle_ms?),
        target,
        target_args,
    })
}

/// TEST-HARNESS ENTRY (`sandboy __cgroup-run …`): create a dedicated leaf, become its owner, run the
/// target inside it under a wall-clock deadline, tear the tree down via `cgroup.kill`, and record
/// what was observed. Returns a non-zero exit on a setup failure — and NEVER leaves a cgroup behind.
pub(crate) fn harness_main() -> i32 {
    let Some(args) = parse_harness_args() else {
        eprintln!("sandboy __cgroup-run: bad arguments");
        return 64;
    };

    let parent = match own_cgroup_dir() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("sandboy __cgroup-run: {e}");
            return 65;
        }
    };
    // A dedicated, non-root leaf under our delegated subtree.
    let leaf = parent.join(format!("o7cg-{}", std::process::id()));
    if let Err(e) = std::fs::create_dir(&leaf) {
        eprintln!(
            "sandboy __cgroup-run: cannot create leaf {}: {e}",
            leaf.display()
        );
        return 66;
    }
    // Record the leaf path immediately, so even a SETUP-FAILURE test can assert it was removed.
    let mut result = RunResult {
        leaf: leaf.clone(),
        monitor_pid: std::process::id() as i32,
        members: Vec::new(),
        timed_out: false,
        drained: false,
        dir_removed: false,
        elapsed_ms: 0,
    };
    let _ = result.write(&args.result);

    // Become the leaf's owner: move the MONITOR in, so the target (spawned next) inherits the leaf.
    if let Err(e) = move_pid_into(&leaf, std::process::id() as i32) {
        eprintln!("sandboy __cgroup-run: cannot join leaf: {e}");
        let (drained, dir_removed) = teardown(&leaf, &parent, Duration::from_secs(3));
        result.drained = drained;
        result.dir_removed = dir_removed;
        let _ = result.write(&args.result);
        return 67;
    }

    let started = Instant::now();
    let child = Command::new(&args.target)
        .args(&args.target_args)
        .stdin(Stdio::null())
        .spawn();
    let mut child = match child {
        Ok(c) => c,
        Err(e) => {
            // SETUP FAILURE: no target ran. Tear the leaf down — it must not leak.
            eprintln!("sandboy __cgroup-run: could not spawn target: {e}");
            let (drained, dir_removed) = teardown(&leaf, &parent, Duration::from_secs(3));
            result.drained = drained;
            result.dir_removed = dir_removed;
            let _ = result.write(&args.result);
            return 68;
        }
    };

    // Let the tree establish, then capture the leaf membership the target/descendants inherited.
    std::thread::sleep(args.settle);
    result.members = members(&leaf);
    let _ = result.write(&args.result);

    // Enforce the wall-clock deadline: wait for the target, but never past `deadline`.
    let mut timed_out = false;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => {}
            Err(_) => break,
        }
        if started.elapsed() >= args.deadline {
            timed_out = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }

    // Teardown: move the monitor out, `cgroup.kill` the tree, prove the drain, remove the directory.
    let (drained, dir_removed) = teardown(&leaf, &parent, Duration::from_secs(3));
    // Reap the (now-killed) direct child so it does not linger as a zombie.
    let _ = child.wait();

    result.timed_out = timed_out;
    result.drained = drained;
    result.dir_removed = dir_removed;
    result.elapsed_ms = started.elapsed().as_millis();
    let _ = result.write(&args.result);

    if drained && dir_removed {
        0
    } else {
        69
    }
}
