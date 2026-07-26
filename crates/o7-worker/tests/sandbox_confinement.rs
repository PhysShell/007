//! Vertical B — REAL kernel confinement acceptance matrix (RED).
//!
//! These pin what a REAL Landlock + seccomp + cgroup-v2 backend must enforce. They run against a
//! NON-confining stand-in — `confinement_backend()` currently returns the frozen fake, which
//! reports every dimension `enforced` but installs nothing — so a launched target ESCAPES, and each
//! RED assertion observes the escape with a CONCRETE oracle rather than a vacuous `is_err()`:
//!
//! - a write OUTSIDE the worktree, captured with the exact denial errno;
//! - an `execve` of a non-allowed binary observed KERNEL-side (a secondary marker it must never
//!   create), NOT the lexical parent gate (that is Vertical A — see `sandbox_contract.rs`);
//! - IPv4/IPv6 socket creation denied with the EXACT `EPERM`/`EACCES`, gated by an UNCONFINED
//!   baseline so a runner lacking IPv6 cannot masquerade as a working seccomp filter;
//! - `setsid`/`setpgid` denied with the exact `EPERM` (a real GREEN seccomp filter makes the old
//!   "the escape survives" assertion impossible, so it is split out here);
//! - a monitor-owned cgroup that provably CONTAINS the tree (monitor + target + a double-forked
//!   descendant), is a DEDICATED non-root leaf, and is REMOVED on teardown — a killpg-only backend
//!   cannot pass this;
//! - a deadline that kills the target AND its descendants and leaves no cgroup;
//! - a forced setup failure that runs NO target;
//! - a live sealed `/proc/<pid>/fd/<n>` source that EXECUTES under confinement.
//!
//! Every test is `#[ignore]`d: the PORTABLE workspace suite stays host-agnostic, and the DESIGNATED
//! Linux confinement CI job runs them with `--include-ignored` after PROVING the kernel features
//! (Landlock ABI + ruleset, delegated cgroup v2, a real seccomp filter) — a missing capability is a
//! job FAILURE there, never a skip. GREEN makes the real backend enforce and points
//! `confinement_backend()` at it, without rewriting a single assertion.
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
use o7_worker::{BackendConfig, BackendImage, ProcessBoundary, SandboyBoundary, StdinMode};

/// The backend UNDER TEST for the confinement matrix. RED: the frozen fake, which claims
/// `enforced` but installs no Landlock/seccomp/cgroup. GREEN (Vertical B): swap this one line for
/// the real external confinement backend and the whole matrix turns green unchanged.
fn confinement_backend() -> BackendImage {
    let path = PathBuf::from(env!("CARGO_BIN_EXE_sandboy_fake"));
    let bytes = std::fs::read(&path).expect("read backend binary");
    BackendImage::acquire(
        &path,
        Digest256::of_bytes(&bytes),
        BackendIdentity::new("sandboy-linux", "0.1.0").unwrap(),
    )
    .expect("acquire backend")
}

/// A boundary over the stand-in with an explicit fake-backend `mode` (control-plane only).
fn boundary_mode(
    worktree: &Path,
    allow_exec: Vec<PathBuf>,
    env_allowlist: Vec<OsString>,
    timeout: Duration,
    mode: &str,
) -> SandboyBoundary {
    SandboyBoundary::new(
        confinement_backend(),
        SandboxPolicy {
            worktree: worktree.to_path_buf(),
            allow_exec,
            network: NetworkPolicy::DenyAll,
            env_allowlist,
            timeout,
        },
    )
    .expect("valid boundary")
    .with_backend_config(BackendConfig {
        fake_mode: Some(mode.to_owned()),
    })
}

fn boundary(
    worktree: &Path,
    allow_exec: Vec<PathBuf>,
    env_allowlist: Vec<OsString>,
    timeout: Duration,
) -> SandboyBoundary {
    boundary_mode(worktree, allow_exec, env_allowlist, timeout, "ok")
}

fn probe_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_sandbox_probe"))
}

fn probe_dir() -> PathBuf {
    probe_bin().parent().expect("probe dir").to_path_buf()
}

/// `/bin` and `/usr/bin` — enough for a shell target to exec `dash`/`sleep`/`setsid` under a GREEN
/// Landlock ruleset (the process-tree tests are about ownership, not the exec allowlist).
fn shell_exec_allow() -> Vec<PathBuf> {
    vec![PathBuf::from("/bin"), PathBuf::from("/usr/bin")]
}

fn pid_alive(pid: i32) -> bool {
    Path::new(&format!("/proc/{pid}")).exists()
}

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

/// The unified cgroup-v2 path a pid belongs to (`0::<path>` in `/proc/<pid>/cgroup`).
fn cgroup_of(pid: i32) -> Option<String> {
    let raw = std::fs::read_to_string(format!("/proc/{pid}/cgroup")).ok()?;
    raw.lines()
        .find_map(|l| l.strip_prefix("0::").map(str::to_owned))
}

fn cgroup_dir(path: &str) -> PathBuf {
    PathBuf::from(format!("/sys/fs/cgroup{path}"))
}

/// The pids listed in a cgroup's `cgroup.procs`.
fn cgroup_procs(path: &str) -> Vec<i32> {
    std::fs::read_to_string(cgroup_dir(path).join("cgroup.procs"))
        .map(|s| {
            s.lines()
                .filter_map(|l| l.trim().parse::<i32>().ok())
                .collect()
        })
        .unwrap_or_default()
}

fn best_effort_kill(pid: i32) {
    let _ = nix::sys::signal::kill(
        nix::unistd::Pid::from_raw(pid),
        nix::sys::signal::Signal::SIGKILL,
    );
}

/// A `/bin/dash -c <script>` target (dash is a regular file, unlike the `/bin/sh` symlink).
fn dash_target(script: String, worktree: &Path) -> BoundarySpawnSpec {
    BoundarySpawnSpec {
        executable: PathBuf::from("/bin/dash"),
        arguments: vec![OsString::from("-c"), OsString::from(script)],
        working_directory: worktree.to_path_buf(),
        environment: BTreeMap::new(),
        stdin: StdinMode::Null,
    }
}

fn probe_target(
    args: &[&str],
    worktree: &Path,
    env: BTreeMap<OsString, OsString>,
) -> BoundarySpawnSpec {
    BoundarySpawnSpec {
        executable: probe_bin(),
        arguments: args.iter().map(OsString::from).collect(),
        working_directory: worktree.to_path_buf(),
        environment: env,
        stdin: StdinMode::Null,
    }
}

// --- Filesystem / Landlock ---

#[tokio::test(flavor = "multi_thread")]
#[ignore = "Vertical B: real Landlock backend required; RED against the non-confining stand-in"]
async fn writes_are_confined_to_the_worktree() {
    let wt = tempfile::tempdir().unwrap();
    let outside_dir = tempfile::tempdir().unwrap();
    let outside = outside_dir.path().join("escaped.txt");
    let marker = wt.path().join("fs.result");
    let b = boundary(
        wt.path(),
        vec![probe_dir()],
        vec![],
        Duration::from_secs(30),
    );
    let mut launch = b
        .spawn(probe_target(
            &[
                "fs",
                &wt.path().to_string_lossy(),
                &outside.to_string_lossy(),
                &marker.to_string_lossy(),
            ],
            wt.path(),
            BTreeMap::new(),
        ))
        .await
        .expect("launch");
    let _ = launch.process.wait().await;

    let report = std::fs::read_to_string(&marker).unwrap_or_default();
    // The allowed write inside the worktree must succeed (positive half).
    assert!(
        wt.path().join("inside.txt").exists() && report.contains("inside=OK"),
        "an allowed write inside the worktree must succeed; report: {report:?}"
    );
    // RED: a write OUTSIDE the worktree must be denied with EACCES(13)/EPERM(1). The stand-in
    // lets it through, so the file exists and the probe recorded OK.
    assert!(
        !outside.exists(),
        "a write OUTSIDE the worktree must be DENIED by Landlock; the file was created"
    );
    assert!(
        report.contains("outside=ERR:13") || report.contains("outside=ERR:1"),
        "the outside write must report EACCES/EPERM; report: {report:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "Vertical B: real Landlock execute restriction required; RED against the stand-in"]
async fn exec_of_a_non_allowed_binary_is_denied_by_the_kernel() {
    // KERNEL-side oracle (the lexical parent-gate refusal is Vertical A —
    // `sandbox_contract::wrapping_an_unpermitted_target_fails_closed`). The confined probe runs,
    // then attempts to `execve` `/usr/bin/env` (NOT under `allow_exec`). Under real Landlock the
    // exec is denied (EACCES/EPERM) and the `secondary` marker is never created; the stand-in
    // installs no ruleset, so the exec succeeds and the executed command creates `secondary`.
    let wt = tempfile::tempdir().unwrap();
    let secondary = wt.path().join("escaped-exec");
    let primary = wt.path().join("exec.result");
    let b = boundary(
        wt.path(),
        vec![probe_dir()],
        vec![],
        Duration::from_secs(30),
    );
    let mut launch = b
        .spawn(probe_target(
            &[
                "exec",
                "/usr/bin/env",
                &secondary.to_string_lossy(),
                &primary.to_string_lossy(),
            ],
            wt.path(),
            BTreeMap::new(),
        ))
        .await
        .expect("launch");
    let _ = launch.process.wait().await;
    // Give the escaped exec chain a moment to create its marker on the RED path.
    tokio::time::sleep(Duration::from_millis(200)).await;

    assert!(
        !secondary.exists(),
        "execve of a non-allowed binary must be DENIED by the kernel; it ran (secondary marker)"
    );
    let report = std::fs::read_to_string(&primary).unwrap_or_default();
    assert!(
        report.contains("exec=ERR:13") || report.contains("exec=ERR:1"),
        "the denied exec must report EACCES/EPERM; report: {report:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "Vertical B: real Landlock backend required; RED against the non-confining stand-in"]
async fn a_sealed_proc_fd_target_executes_under_confinement() {
    // PR-3 contract composed with Vertical B: a LIVE sealed `/proc/<pid>/fd/<n>` source (a sealed
    // memfd held by THIS process) must EXECUTE under confinement with no path-copy — and while it
    // runs, an outside-the-worktree write must be denied. RED: the stand-in runs it but does not
    // confine, so the outside file appears.
    use std::io::Write as _;
    use std::os::unix::io::AsRawFd as _;

    use nix::fcntl::{fcntl, FcntlArg, SealFlag};
    use nix::sys::memfd::{memfd_create, MemFdCreateFlag};

    let wt = tempfile::tempdir().unwrap();
    let outside_dir = tempfile::tempdir().unwrap();
    let outside = outside_dir.path().join("escaped.txt");
    let marker = wt.path().join("fs.result");

    let probe_bytes = std::fs::read(probe_bin()).expect("read probe");
    let name = std::ffi::CString::new("o7-sealed-sandbox-probe").unwrap();
    let owned = memfd_create(&name, MemFdCreateFlag::MFD_ALLOW_SEALING).expect("memfd_create");
    let mut file = std::fs::File::from(owned);
    file.write_all(&probe_bytes).expect("write sealed bytes");
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
    let sealed_path = PathBuf::from(format!("/proc/{}/fd/{}", std::process::id(), raw));

    let b = boundary(
        wt.path(),
        vec![sealed_path.clone(), PathBuf::from("/bin")],
        vec![],
        Duration::from_secs(30),
    );
    let spec = BoundarySpawnSpec {
        executable: sealed_path,
        arguments: vec![
            OsString::from("fs"),
            OsString::from(wt.path()),
            OsString::from(&outside),
            OsString::from(&marker),
        ],
        working_directory: wt.path().to_path_buf(),
        environment: BTreeMap::new(),
        stdin: StdinMode::Null,
    };
    let mut launch = b.spawn(spec).await.expect("the sealed memfd must execute");
    let _ = launch.process.wait().await;
    drop(file);

    let report = std::fs::read_to_string(&marker).unwrap_or_default();
    // The sealed proc-fd source really ran (positive: the exact sealed bytes executed).
    assert!(
        report.contains("inside=OK"),
        "the sealed /proc/<pid>/fd source must EXECUTE under confinement; report: {report:?}"
    );
    // RED: while it ran, an outside write must be denied.
    assert!(
        !outside.exists(),
        "the sealed target must be confined; an outside write was allowed"
    );
}

// --- Network / seccomp ---

#[tokio::test(flavor = "multi_thread")]
#[ignore = "Vertical B: real seccomp backend required; RED against the non-confining stand-in"]
async fn network_sockets_are_denied() {
    // UNCONFINED baseline: prove THIS host can actually create each socket family, so a runner
    // without (say) IPv6 cannot let `udp6` pass as a "seccomp denial" when it was really
    // EAFNOSUPPORT. Only families the host supports are held to the confined denial.
    let baseline = unconfined_baseline_net();
    assert!(
        baseline.contains("udp4=OK") && baseline.contains("tcp4=OK"),
        "unusable test host: the unconfined baseline cannot create IPv4 sockets: {baseline:?}"
    );

    let wt = tempfile::tempdir().unwrap();
    let marker = wt.path().join("net.result");
    let b = boundary(
        wt.path(),
        vec![probe_dir()],
        vec![],
        Duration::from_secs(30),
    );
    let mut launch = b
        .spawn(probe_target(
            &["net", &marker.to_string_lossy()],
            wt.path(),
            BTreeMap::new(),
        ))
        .await
        .expect("launch");
    let _ = launch.process.wait().await;

    let report = std::fs::read_to_string(&marker).unwrap_or_default();
    // RED: every family the host DEMONSTRABLY supports must now be denied with the EXACT EPERM(1)/
    // EACCES(13) — not merely "some error" (an EAFNOSUPPORT(97) is not a seccomp denial). The
    // stand-in installs no seccomp, so each bind succeeds (`OK`).
    for family in ["udp4", "tcp4", "udp6"] {
        if !baseline.contains(&format!("{family}=OK")) {
            continue; // host cannot do it unconfined — not a seccomp signal
        }
        assert!(
            report.contains(&format!("{family}=ERR:1"))
                || report.contains(&format!("{family}=ERR:13")),
            "{family}: socket creation must be DENIED by seccomp with EPERM/EACCES; report: {report:?}"
        );
    }
    // Inherited descriptors: the confined target must inherit NO network sockets (fd scrub).
    assert!(
        report.contains("inherited_sockets=0"),
        "the confined target must inherit no network descriptors; report: {report:?}"
    );
}

/// Run the probe DIRECTLY (no boundary, no confinement) to learn which socket families this host
/// supports at all — the baseline the confined run is measured against.
fn unconfined_baseline_net() -> String {
    let dir = tempfile::tempdir().unwrap();
    let marker = dir.path().join("baseline.net");
    let status = std::process::Command::new(probe_bin())
        .arg("net")
        .arg(&marker)
        .status()
        .expect("run unconfined probe");
    assert!(status.success(), "unconfined probe failed: {status:?}");
    std::fs::read_to_string(&marker).unwrap_or_default()
}

// --- Environment (already enforced by plane separation) ---

#[tokio::test(flavor = "multi_thread")]
#[ignore = "Vertical B confinement job (portable-clean; already enforced by plane separation)"]
async fn the_target_env_is_exactly_the_allowlist() {
    let wt = tempfile::tempdir().unwrap();
    let marker = wt.path().join("env.result");
    let mut env = BTreeMap::new();
    env.insert(OsString::from("PATH"), OsString::from("/usr/bin:/bin"));
    let b = boundary(
        wt.path(),
        vec![probe_dir()],
        vec![OsString::from("PATH")],
        Duration::from_secs(30),
    );
    let mut launch = b
        .spawn(probe_target(
            &["env", &marker.to_string_lossy()],
            wt.path(),
            env,
        ))
        .await
        .expect("launch");
    let _ = launch.process.wait().await;

    let names = std::fs::read_to_string(&marker).unwrap_or_default();
    assert_eq!(
        names.trim(),
        "PATH",
        "the target must receive EXACTLY the allowlisted env names, got: {names:?}"
    );
}

// --- Process tree / seccomp escape denial ---

#[tokio::test(flavor = "multi_thread")]
#[ignore = "Vertical B: real seccomp filter required; RED against the non-confining stand-in"]
async fn setsid_and_setpgid_are_denied_by_seccomp() {
    // A GREEN seccomp filter forbids leaving the owned set via a new session/process group, so
    // BOTH `setsid` and `setpgid` must return EPERM(1). (This replaces the old "the setsid escape
    // survives teardown" assertion, which a correct filter makes impossible.) RED: the stand-in
    // installs no filter, so the confined target — a non-leader child — succeeds at both.
    let wt = tempfile::tempdir().unwrap();
    let marker = wt.path().join("seccomp.result");
    let b = boundary(
        wt.path(),
        vec![probe_dir()],
        vec![],
        Duration::from_secs(30),
    );
    let mut launch = b
        .spawn(probe_target(
            &["seccomp", &marker.to_string_lossy()],
            wt.path(),
            BTreeMap::new(),
        ))
        .await
        .expect("launch");
    let _ = launch.process.wait().await;

    let report = std::fs::read_to_string(&marker).unwrap_or_default();
    assert!(
        report.contains("setsid=ERR:1"),
        "setsid must be DENIED by seccomp with EPERM; report: {report:?}"
    );
    assert!(
        report.contains("setpgid=ERR:1"),
        "setpgid must be DENIED by seccomp with EPERM; report: {report:?}"
    );
}

// --- Process tree / cgroup v2 ownership ---

#[tokio::test(flavor = "multi_thread")]
#[ignore = "Vertical B: monitor-owned cgroup v2 required; RED against process-group ownership"]
async fn the_owned_cgroup_contains_the_tree_and_is_removed_on_teardown() {
    // The target records its pid, spawns an ORDINARY child, and a DOUBLE-FORKED (reparented, no
    // setsid) descendant, all sleeping. A monitor-owned cgroup v2 must contain monitor + target +
    // descendant in ONE dedicated non-root cgroup, and that cgroup directory must be REMOVED on
    // teardown. A killpg-only backend cannot pass this: it never creates a dedicated cgroup, so the
    // members share the harness's own cgroup, which does not disappear.
    let wt = tempfile::tempdir().unwrap();
    let tpid = wt.path().join("target.pid");
    let cpid = wt.path().join("child.pid");
    let dpid = wt.path().join("desc.pid");
    let script = format!(
        "echo $$ > {tp}; \
         /bin/dash -c 'echo $$ > {cp}; exec sleep 30' & \
         ( /bin/dash -c 'echo $$ > {dp}; exec sleep 30' & ); \
         sleep 30",
        tp = tpid.display(),
        cp = cpid.display(),
        dp = dpid.display(),
    );
    let b = boundary(
        wt.path(),
        shell_exec_allow(),
        vec![],
        Duration::from_secs(30),
    );
    let mut launch = b
        .spawn(dash_target(script, wt.path()))
        .await
        .expect("launch");

    let monitor = launch.process.identity().pid;
    let target = read_pid_bounded(&tpid, Duration::from_secs(3))
        .await
        .expect("target pid");
    let child = read_pid_bounded(&cpid, Duration::from_secs(3))
        .await
        .expect("child pid");
    let descendant = read_pid_bounded(&dpid, Duration::from_secs(3))
        .await
        .expect("descendant pid");

    let target_cg = cgroup_of(target).expect("target cgroup");
    let harness_cg = cgroup_of(std::process::id() as i32).expect("harness cgroup");

    // RED: the owned cgroup must be a DEDICATED leaf, distinct from the harness's own cgroup. A
    // killpg-only stand-in leaves everyone in the harness cgroup.
    let dedicated = target_cg != harness_cg;
    // Every member shares that one cgroup, and it lists them all.
    let members = cgroup_procs(&target_cg);
    let all_in_one = [monitor, target, child, descendant]
        .iter()
        .all(|p| cgroup_of(*p).as_deref() == Some(target_cg.as_str()));
    let procs_complete = [monitor, target, child, descendant]
        .iter()
        .all(|p| members.contains(p));

    launch.process.force_stop().await.expect("force_stop");
    let _ = launch.process.wait().await;

    let gone = wait_pid_gone(target, Duration::from_secs(3)).await
        && wait_pid_gone(child, Duration::from_secs(3)).await
        && wait_pid_gone(descendant, Duration::from_secs(3)).await;
    // RED: after drain the owned cgroup directory must be gone. The harness cgroup never vanishes.
    let cgroup_removed = !cgroup_dir(&target_cg).exists();

    // Never leak survivors on the RED path.
    for pid in [child, descendant, target] {
        if pid_alive(pid) {
            best_effort_kill(pid);
        }
    }

    assert!(
        dedicated,
        "the tree must live in a DEDICATED monitor-owned cgroup, not the harness cgroup {harness_cg:?}"
    );
    assert!(
        all_in_one && procs_complete,
        "monitor+target+child+descendant must all be in the one owned cgroup {target_cg:?} \
         (members: {members:?})"
    );
    assert!(gone, "the whole tree must be torn down");
    assert!(
        cgroup_removed,
        "the owned cgroup directory {target_cg:?} must be REMOVED after drain"
    );
}

// --- Timeout ---

#[tokio::test(flavor = "multi_thread")]
#[ignore = "Vertical B: monitor-enforced deadline + owned cgroup required; RED against the stand-in"]
async fn a_target_outliving_the_deadline_is_killed_with_its_descendants() {
    let wt = tempfile::tempdir().unwrap();
    let tpid = wt.path().join("target.pid");
    let dpid = wt.path().join("desc.pid");
    let survived = wt.path().join("survived");
    // The target spawns a double-forked descendant, then both sleep past the deadline; the target
    // would `touch survived` if it ever completed.
    let script = format!(
        "echo $$ > {tp}; \
         ( /bin/dash -c 'echo $$ > {dp}; exec sleep 5' & ); \
         sleep 5; touch {s}",
        tp = tpid.display(),
        dp = dpid.display(),
        s = survived.display(),
    );
    // 500ms deadline; the tree sleeps 5s.
    let b = boundary(
        wt.path(),
        shell_exec_allow(),
        vec![],
        Duration::from_millis(500),
    );
    let mut launch = b
        .spawn(dash_target(script, wt.path()))
        .await
        .expect("launch");
    let target = read_pid_bounded(&tpid, Duration::from_secs(2))
        .await
        .expect("target pid");
    let descendant = read_pid_bounded(&dpid, Duration::from_secs(2))
        .await
        .expect("descendant pid");
    let target_cg = cgroup_of(target).expect("target cgroup");

    // Within the deadline + a bounded grace, target AND descendant must be dead.
    let target_killed = wait_pid_gone(target, Duration::from_millis(2500)).await;
    let descendant_killed = wait_pid_gone(descendant, Duration::from_millis(2500)).await;
    let cgroup_removed = !cgroup_dir(&target_cg).exists();

    // Clean up any lingering process on the RED path.
    let _ = launch.process.force_stop().await;
    let _ = launch.process.wait().await;
    for pid in [target, descendant] {
        if pid_alive(pid) {
            best_effort_kill(pid);
        }
    }

    assert!(
        target_killed,
        "the target (pid {target}) past the deadline must be killed by the monitor"
    );
    assert!(
        descendant_killed,
        "the descendant (pid {descendant}) must be killed with the target"
    );
    assert!(
        !survived.exists(),
        "the target ran past the deadline (the `survived` marker was written)"
    );
    assert!(
        cgroup_removed,
        "the owned cgroup {target_cg:?} must be removed after the deadline kill"
    );
}

// --- Report truthfulness / setup failure ---

#[tokio::test(flavor = "multi_thread")]
#[ignore = "Vertical B: real backend fault injection required; RED against the non-confining stand-in"]
async fn a_forced_setup_failure_runs_no_target() {
    // The future backend honours a TEST-ONLY fault-injection param on its control-plane
    // `O7_FAKE_MODE` (`ok;fault=landlock`): it fails the Landlock install, reports NOT enforced (or
    // dies before GO), so the parent fails closed and the target NEVER runs. RED: the stand-in does
    // not recognise the fault, reports `enforced`, GOes, and runs the target — the marker appears.
    let wt = tempfile::tempdir().unwrap();
    let marker = wt.path().join("fs.result");
    let outside_dir = tempfile::tempdir().unwrap();
    let outside = outside_dir.path().join("x.txt");
    let b = boundary_mode(
        wt.path(),
        vec![probe_dir()],
        vec![],
        Duration::from_secs(30),
        "ok;fault=landlock",
    );
    let spawn = b
        .spawn(probe_target(
            &[
                "fs",
                &wt.path().to_string_lossy(),
                &outside.to_string_lossy(),
                &marker.to_string_lossy(),
            ],
            wt.path(),
            BTreeMap::new(),
        ))
        .await;
    // GREEN fails the launch closed; the stand-in succeeds. Either way, drain if a process exists.
    if let Ok(mut launch) = spawn {
        let _ = launch.process.wait().await;
    }
    tokio::time::sleep(Duration::from_millis(100)).await;

    assert!(
        !marker.exists(),
        "a forced setup failure must run NO target; the target-ran marker exists (the stand-in \
         ignored the injected fault and ran the target unconfined)"
    );
}
