//! VB-1 — a dedicated, monitor-owned cgroup v2 leaf with an ABSOLUTE wall-clock timeout and a
//! FAIL-CLOSED `cgroup.kill` teardown. SAFE: only filesystem writes to cgroup control files +
//! `std::process` — NO `unsafe`, no kernel syscall crate.
//!
//! Placement is by INHERITANCE: the monitor moves ITSELF into the leaf, then spawns the target, so
//! the target and every descendant (ordinary child AND a double-forked, reparented one) inherit the
//! leaf. Teardown is fail-closed and ORDERED: move the monitor back out, PROVE it is out (its own
//! cgroup is the expected parent) BEFORE writing `cgroup.kill` — otherwise `cgroup.kill` would
//! SIGKILL the monitor along with the tree — then observe the drain from a SUCCESSFUL `cgroup.procs`
//! read (a read failure is a failure, never an "empty"), and only then remove the directory. Every
//! step returns a typed error; `drained = true` means the drain was actually observed.
//!
//! Compiled ONLY under the `test-harness` feature at VB-1: the production `run` path does not use it
//! yet (the backend cannot report `FullyEnforced` until Landlock/seccomp/env land in VB-2/VB-3), so
//! wiring this into the live launch is VB-4. Exercised by the `#[ignore]`d confinement tests, which
//! need real delegated cgroup v2 with a writable `cgroup.kill`. A TEST-ONLY control-plane fault knob
//! (`O7_CG_FAULT`) forces individual teardown steps to fail so the fail-closed paths are provable
//! without a real kernel failure; production never compiles this module, so it never reads it.

use std::io::{self, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// The unified (v2) cgroup PATH a pid belongs to: the text after `0::` in `/proc/<pid>/cgroup`.
pub(crate) fn cgroup_path_of(pid: i32) -> io::Result<String> {
    let raw = std::fs::read_to_string(format!("/proc/{pid}/cgroup"))?;
    raw.lines()
        .find_map(|l| l.strip_prefix("0::").map(str::to_owned))
        .ok_or_else(|| io::Error::other("no unified (0::) cgroup line in /proc/<pid>/cgroup"))
}

/// The absolute filesystem directory for a cgroup path (`/sys/fs/cgroup<path>`).
fn cgroup_dir(path: &str) -> PathBuf {
    PathBuf::from(format!("/sys/fs/cgroup{path}"))
}

/// Write `pid` into `<cgroup_dir>/cgroup.procs`, moving that process into the cgroup.
fn move_pid_into(cgroup_dir: &Path, pid: i32) -> io::Result<()> {
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .open(cgroup_dir.join("cgroup.procs"))?;
    write!(f, "{pid}")
}

/// The pids in a cgroup's `cgroup.procs`. FALLIBLE: a read failure is an error, never an empty set —
/// the teardown drain check depends on distinguishing "read succeeded and empty" from "read failed".
fn read_members(cgroup_dir: &Path) -> io::Result<Vec<i32>> {
    let raw = std::fs::read_to_string(cgroup_dir.join("cgroup.procs"))?;
    Ok(raw.lines().filter_map(|l| l.trim().parse().ok()).collect())
}

/// TEST-ONLY forced-fault knob, read from the control-plane env. Lets the confinement tests force a
/// specific teardown step to fail and prove the fail-closed path, with no external dependency.
#[derive(Debug, Clone, Copy, Default)]
struct Faults {
    move_out: bool,
    kill: bool,
    drain: bool,
}

fn faults_from_env() -> Faults {
    match std::env::var("O7_CG_FAULT").ok().as_deref() {
        Some("move_out") => Faults {
            move_out: true,
            ..Faults::default()
        },
        Some("kill") => Faults {
            kill: true,
            ..Faults::default()
        },
        Some("drain") => Faults {
            drain: true,
            ..Faults::default()
        },
        _ => Faults::default(),
    }
}

/// A teardown step that failed — named so the harness can surface WHICH stage refused and so a
/// downgrade at any step never masquerades as a clean teardown.
#[derive(Debug)]
enum TeardownError {
    MoveOut(String),
    Placement(String),
    Kill(String),
    Drain(String),
    Rmdir(String),
}

impl TeardownError {
    fn stage(&self) -> &'static str {
        match self {
            TeardownError::MoveOut(_) => "move_out",
            TeardownError::Placement(_) => "verify_placement",
            TeardownError::Kill(_) => "kill",
            TeardownError::Drain(_) => "drain",
            TeardownError::Rmdir(_) => "rmdir",
        }
    }
    /// A distinct non-zero exit per teardown stage.
    fn exit_code(&self) -> i32 {
        match self {
            TeardownError::MoveOut(_) => 72,
            TeardownError::Placement(_) => 73,
            TeardownError::Kill(_) => 74,
            TeardownError::Drain(_) => 75,
            TeardownError::Rmdir(_) => 76,
        }
    }
    fn message(&self) -> &str {
        match self {
            TeardownError::MoveOut(m)
            | TeardownError::Placement(m)
            | TeardownError::Kill(m)
            | TeardownError::Drain(m)
            | TeardownError::Rmdir(m) => m,
        }
    }
}

/// Move the monitor back to `parent` with a bounded retry.
fn move_out_with_retry(parent: &Path, faults: Faults) -> Result<(), String> {
    if faults.move_out {
        return Err("forced move-out fault (test)".to_owned());
    }
    let self_pid = std::process::id() as i32;
    let mut last = String::from("move-out failed");
    for _ in 0..5 {
        match move_pid_into(parent, self_pid) {
            Ok(()) => return Ok(()),
            Err(e) => {
                last = e.to_string();
                std::thread::sleep(Duration::from_millis(20));
            }
        }
    }
    Err(last)
}

/// PROVE the monitor is out of the leaf: its own unified cgroup must now be exactly `expected_parent`
/// (not the leaf). Writing `cgroup.kill` before this is proven would SIGKILL the monitor.
fn verify_monitor_out(expected_parent: &str) -> Result<(), String> {
    let self_pid = std::process::id() as i32;
    let own = cgroup_path_of(self_pid).map_err(|e| e.to_string())?;
    if own == expected_parent {
        Ok(())
    } else {
        Err(format!(
            "monitor is still in {own}, expected to be in the parent {expected_parent}"
        ))
    }
}

/// Write `1` to `<leaf>/cgroup.kill`, SIGKILLing every process in the leaf tree at once. Fallible.
fn write_cgroup_kill(leaf: &Path, faults: Faults) -> Result<(), String> {
    if faults.kill {
        return Err("forced cgroup.kill fault (test)".to_owned());
    }
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .open(leaf.join("cgroup.kill"))
        .map_err(|e| format!("open cgroup.kill: {e}"))?;
    f.write_all(b"1")
        .map_err(|e| format!("write cgroup.kill: {e}"))
}

/// OBSERVE the drain: poll `cgroup.procs` (a read failure is a failure) until it is empty within
/// `bound`. Returns `Ok` ONLY when a SUCCESSFUL read showed the leaf empty.
fn observe_drain(leaf: &Path, bound: Duration, faults: Faults) -> Result<(), String> {
    if faults.drain {
        return Err("forced drain-observation fault (test)".to_owned());
    }
    let start = Instant::now();
    loop {
        let m = read_members(leaf).map_err(|e| format!("read cgroup.procs: {e}"))?;
        if m.is_empty() {
            return Ok(());
        }
        if start.elapsed() >= bound {
            return Err(format!(
                "leaf did not drain: {} process(es) remain",
                m.len()
            ));
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// FAIL-CLOSED teardown of the leaf the monitor owns: move out, PROVE placement, `cgroup.kill`,
/// OBSERVE the drain, then remove the directory. Any step's failure aborts BEFORE the next and is
/// returned typed — in particular, `cgroup.kill` is never written until the monitor is proven out.
fn teardown(
    leaf: &Path,
    parent: &Path,
    parent_path: &str,
    drain_bound: Duration,
    faults: Faults,
) -> Result<(), TeardownError> {
    move_out_with_retry(parent, faults).map_err(TeardownError::MoveOut)?;
    verify_monitor_out(parent_path).map_err(TeardownError::Placement)?;
    write_cgroup_kill(leaf, faults).map_err(TeardownError::Kill)?;
    observe_drain(leaf, drain_bound, faults).map_err(TeardownError::Drain)?;
    std::fs::remove_dir(leaf).map_err(|e| TeardownError::Rmdir(e.to_string()))?;
    Ok(())
}

/// The result of one monitored run, written to the harness `--result` file as `key=value` lines so
/// the confinement test can verify FACTS the monitor observed. `drained`/`dir_removed` are true only
/// when actually OBSERVED; `teardown_stage` is `ok` or the stage that refused.
struct RunResult {
    leaf: PathBuf,
    monitor_pid: i32,
    members: Vec<i32>,
    timed_out: bool,
    drained: bool,
    dir_removed: bool,
    teardown_stage: String,
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
            "leaf={}\nmonitor_pid={}\nmembers={}\ntimed_out={}\ndrained={}\ndir_removed={}\nteardown_stage={}\nelapsed_ms={}\n",
            self.leaf.display(),
            self.monitor_pid,
            members,
            u8::from(self.timed_out),
            u8::from(self.drained),
            u8::from(self.dir_removed),
            self.teardown_stage,
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
/// target inside it under an ABSOLUTE wall-clock deadline, tear the tree down fail-closed via
/// `cgroup.kill`, and record what was observed. Never leaves a cgroup behind on a setup failure.
pub(crate) fn harness_main() -> i32 {
    let Some(args) = parse_harness_args() else {
        eprintln!("sandboy __cgroup-run: bad arguments");
        return 64;
    };
    let faults = faults_from_env();

    // The monitor's ORIGINAL cgroup is the parent it returns to; capture it BEFORE moving in.
    let parent_path = match cgroup_path_of(std::process::id() as i32) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("sandboy __cgroup-run: {e}");
            return 65;
        }
    };
    let parent = cgroup_dir(&parent_path);
    let leaf = parent.join(format!("o7cg-{}", std::process::id()));
    if let Err(e) = std::fs::create_dir(&leaf) {
        eprintln!(
            "sandboy __cgroup-run: cannot create leaf {}: {e}",
            leaf.display()
        );
        return 66;
    }
    let mut result = RunResult {
        leaf: leaf.clone(),
        monitor_pid: std::process::id() as i32,
        members: Vec::new(),
        timed_out: false,
        drained: false,
        dir_removed: false,
        teardown_stage: String::from("not_reached"),
        elapsed_ms: 0,
    };
    // Record the leaf path immediately, so even a SETUP-FAILURE test can assert it was removed.
    let _ = result.write(&args.result);

    // Own the leaf: move the MONITOR in, so the target (spawned next) inherits it.
    if let Err(e) = move_pid_into(&leaf, std::process::id() as i32) {
        eprintln!("sandboy __cgroup-run: cannot join leaf: {e}");
        finish_teardown(
            &mut result,
            &leaf,
            &parent,
            &parent_path,
            faults,
            &args.result,
        );
        return 67;
    }

    // ABSOLUTE deadline from spawn — every subsequent wait is capped by the remaining time to it, so
    // a large `settle` can never let the target outlive the deadline.
    let started = Instant::now();
    let deadline_at = started + args.deadline;
    let child = Command::new(&args.target)
        .args(&args.target_args)
        .stdin(Stdio::null())
        .spawn();
    let mut child = match child {
        Ok(c) => c,
        Err(e) => {
            // SETUP FAILURE: no target ran. Tear the leaf down — it must not leak.
            eprintln!("sandboy __cgroup-run: could not spawn target: {e}");
            finish_teardown(
                &mut result,
                &leaf,
                &parent,
                &parent_path,
                faults,
                &args.result,
            );
            return 68;
        }
    };

    // Capture membership, but NEVER sleep past the deadline: cap the settle by the deadline.
    let settle_at = (started + args.settle).min(deadline_at);
    sleep_until(settle_at);
    result.members = read_members(&leaf).unwrap_or_default();
    let _ = result.write(&args.result);

    // Enforce the ABSOLUTE deadline: wait for the target, capping every sleep by the remaining time.
    let mut timed_out = false;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => {}
            Err(_) => break,
        }
        let now = Instant::now();
        if now >= deadline_at {
            timed_out = true;
            break;
        }
        let remaining = deadline_at.saturating_duration_since(now);
        std::thread::sleep(remaining.min(Duration::from_millis(20)));
    }
    result.timed_out = timed_out;

    let outcome = teardown(&leaf, &parent, &parent_path, Duration::from_secs(3), faults);
    // Reap the (now-killed) direct child so it does not linger as a zombie — but ONLY when the kill
    // actually landed (Ok, or a failure AFTER the kill: drain/rmdir). Before the kill (move-out /
    // placement / kill-write failures) the tree is still ALIVE, so a blocking `wait` would hang;
    // leave it for the test's cleanup (it is reparented to init on our exit).
    match &outcome {
        Ok(()) | Err(TeardownError::Drain(_)) | Err(TeardownError::Rmdir(_)) => {
            let _ = child.wait();
        }
        Err(TeardownError::MoveOut(_))
        | Err(TeardownError::Placement(_))
        | Err(TeardownError::Kill(_)) => {}
    }

    result.elapsed_ms = started.elapsed().as_millis();
    match outcome {
        Ok(()) => {
            result.teardown_stage = String::from("ok");
            result.drained = true;
            result.dir_removed = true;
            let _ = result.write(&args.result);
            0
        }
        Err(e) => {
            eprintln!(
                "sandboy __cgroup-run: teardown failed at {}: {}",
                e.stage(),
                e.message()
            );
            result.teardown_stage = e.stage().to_owned();
            // `drained`/`dir_removed` stay false — they were not observed.
            let _ = result.write(&args.result);
            e.exit_code()
        }
    }
}

/// Sleep until `at` (no-op if already past), in small steps.
fn sleep_until(at: Instant) {
    while Instant::now() < at {
        let remaining = at.saturating_duration_since(Instant::now());
        std::thread::sleep(remaining.min(Duration::from_millis(20)));
    }
}

/// Teardown on a SETUP-failure path (no target running yet, or leaf-join failed) and fold the
/// observed drain/dir-removal + stage into `result`.
fn finish_teardown(
    result: &mut RunResult,
    leaf: &Path,
    parent: &Path,
    parent_path: &str,
    faults: Faults,
    result_path: &Path,
) {
    match teardown(leaf, parent, parent_path, Duration::from_secs(3), faults) {
        Ok(()) => {
            result.teardown_stage = String::from("ok");
            result.drained = true;
            result.dir_removed = true;
        }
        Err(e) => {
            result.teardown_stage = e.stage().to_owned();
        }
    }
    let _ = result.write(result_path);
}
