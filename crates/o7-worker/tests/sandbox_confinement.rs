//! Vertical B — REAL kernel confinement acceptance matrix (RED).
//!
//! These pin what a REAL Landlock + seccomp + cgroup-v2 backend must enforce. They run against a
//! NON-confining stand-in — `confinement_backend()` currently returns the frozen fake, which
//! reports every dimension `enforced` but installs nothing — so a launched target ESCAPES, and each
//! RED assertion observes the escape with a CONCRETE, INDEPENDENTLY non-vacuous oracle:
//!
//! - a write OUTSIDE the worktree, captured with the exact denial errno;
//! - an `execve` of a non-allowed binary observed KERNEL-side (a secondary marker it must never
//!   create), NOT the lexical parent gate (that is Vertical A — see `sandbox_contract.rs`);
//! - IPv4 AND IPv6 socket creation denied with the EXACT `EPERM`/`EACCES`, gated by an UNCONFINED
//!   baseline (all three families must work unconfined, else the host is not the designated env);
//! - a fail-closed inherited-descriptor oracle with a PLANTED non-CLOEXEC socket sentinel;
//! - `setsid` and `setpgid` each denied with the exact `EPERM`, in SEPARATE processes so neither
//!   result depends on the other having run first;
//! - a monitor-owned cgroup that CONTAINS the tree (monitor + target + ordinary child + a
//!   double-forked descendant), is a DEDICATED non-root leaf, and is REMOVED on teardown;
//! - a deadline that kills the target AND its descendant inside ONE bounded window and leaves no
//!   cgroup, checked AFTER the monitor is waited (never racing a reap-then-remove monitor);
//! - a four-mode setup-failure/report-truthfulness matrix that runs NO target;
//! - a live sealed `/proc/<pid>/fd/<n>` source that EXECUTES under confinement with the exact
//!   outside-denial errno.
//!
//! Teardown oracles compare PID + start-time IDENTITY (not a bare PID number), so PID reuse can
//! never read as a successful teardown. Every test is `#[ignore]`d: the PORTABLE workspace suite
//! stays host-agnostic, and the DESIGNATED Linux confinement CI job runs them with
//! `--include-ignored` after PROVING the kernel features — a missing capability is a job FAILURE
//! there, never a skip. GREEN makes the real backend enforce and points `confinement_backend()` at
//! it, without rewriting a single assertion.
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
use o7_worker::{
    BackendConfig, BackendImage, ProcessBoundary, ProcessIdentity, SandboyBoundary, StdinMode,
};

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

/// `/bin` and `/usr/bin` — enough for a shell target to exec `dash`/`sleep` under a GREEN Landlock
/// ruleset (the process-tree tests are about ownership, not the exec allowlist).
fn shell_exec_allow() -> Vec<PathBuf> {
    vec![PathBuf::from("/bin"), PathBuf::from("/usr/bin")]
}

/// Whether a captured PID+start-time identity is GONE — the PID is absent, or reused by a DIFFERENT
/// process (a different start time). A bare `/proc/<pid>` check cannot tell teardown from PID reuse.
fn identity_gone(id: &ProcessIdentity) -> bool {
    match ProcessIdentity::read(id.pid) {
        None => true,
        Some(cur) => cur.start_time_ticks != id.start_time_ticks,
    }
}

async fn wait_identity_gone(id: &ProcessIdentity, bound: Duration) -> bool {
    let start = tokio::time::Instant::now();
    while start.elapsed() < bound {
        if identity_gone(id) {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    identity_gone(id)
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

/// The captured live identity of the process that wrote its pid to `path`.
async fn read_identity_bounded(path: &Path, bound: Duration) -> Option<ProcessIdentity> {
    let pid = read_pid_bounded(path, bound).await?;
    ProcessIdentity::read(pid)
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

/// Run the probe DIRECTLY (no boundary, no confinement) and return its marker — the UNCONFINED
/// baseline the confined run is measured against.
fn unconfined_probe(args: &[&str]) -> String {
    let dir = tempfile::tempdir().unwrap();
    let marker = dir.path().join("baseline");
    let mut full: Vec<OsString> = args.iter().map(OsString::from).collect();
    full.push(OsString::from(&marker));
    let status = std::process::Command::new(probe_bin())
        .args(&full)
        .status()
        .expect("run unconfined probe");
    assert!(
        status.success(),
        "unconfined probe {args:?} failed: {status:?}"
    );
    std::fs::read_to_string(&marker).unwrap_or_default()
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
    assert!(
        wt.path().join("inside.txt").exists() && report.contains("inside=OK"),
        "an allowed write inside the worktree must succeed; report: {report:?}"
    );
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
    // runs, an outside-the-worktree write must be denied with the EXACT errno. RED: the stand-in
    // runs it but does not confine, so the outside file appears.
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
    assert!(
        report.contains("inside=OK"),
        "the sealed /proc/<pid>/fd source must EXECUTE under confinement; report: {report:?}"
    );
    assert!(
        !outside.exists(),
        "the sealed target must be confined; an outside write was allowed"
    );
    assert!(
        report.contains("outside=ERR:13") || report.contains("outside=ERR:1"),
        "the outside write must report the exact EACCES/EPERM; report: {report:?}"
    );
}

// --- Network / seccomp ---

#[tokio::test(flavor = "multi_thread")]
#[ignore = "Vertical B: real seccomp backend required; RED against the non-confining stand-in"]
async fn network_sockets_are_denied() {
    // MANDATORY baseline: the designated environment MUST support IPv4 AND IPv6 unconfined — a
    // runner missing IPv6 is an environment gate failure here, not a reason to drop the IPv6 leg.
    let baseline = unconfined_probe(&["net"]);
    for family in ["udp4", "tcp4", "udp6"] {
        assert!(
            baseline.contains(&format!("{family}=OK")),
            "designated env must support {family} unconfined; baseline: {baseline:?}"
        );
    }

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
    // RED: every family must be denied with the EXACT EPERM(1)/EACCES(13); the stand-in installs no
    // seccomp, so each bind succeeds (`OK`).
    for family in ["udp4", "tcp4", "udp6"] {
        assert!(
            report.contains(&format!("{family}=ERR:1"))
                || report.contains(&format!("{family}=ERR:13")),
            "{family}: socket creation must be DENIED by seccomp with EPERM/EACCES; report: {report:?}"
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "Vertical B confinement job (portable-clean; scrub proven with a planted socket)"]
async fn the_confined_target_inherits_no_planted_socket() {
    use std::os::unix::io::AsRawFd as _;

    use nix::fcntl::{fcntl, FcntlArg, FdFlag};

    // PLANT a real socket and CLEAR its CLOEXEC, so it WOULD be inherited across the exec into the
    // backend unless the monitor scrubs the descriptor set. (std sockets are CLOEXEC by default.)
    let sentinel = std::net::UdpSocket::bind("127.0.0.1:0").expect("bind sentinel");
    let raw = sentinel.as_raw_fd();
    fcntl(raw, FcntlArg::F_SETFD(FdFlag::empty())).expect("clear CLOEXEC");
    let flags = fcntl(raw, FcntlArg::F_GETFD).expect("get flags");
    assert!(
        flags & FdFlag::FD_CLOEXEC.bits() == 0,
        "the sentinel must be inheritable for this test to mean anything"
    );

    let wt = tempfile::tempdir().unwrap();
    let marker = wt.path().join("fds.result");
    let b = boundary(
        wt.path(),
        vec![probe_dir()],
        vec![],
        Duration::from_secs(30),
    );
    let mut launch = b
        .spawn(probe_target(
            &["fds", &marker.to_string_lossy()],
            wt.path(),
            BTreeMap::new(),
        ))
        .await
        .expect("launch");
    let exit = launch.process.wait().await.expect("waited");
    drop(sentinel);

    // The probe fails CLOSED: a success marker exists ONLY if enumeration succeeded (exit 0).
    assert_eq!(
        exit,
        o7_worker::boundary::BoundaryExit::Code(0),
        "the fail-closed inherited-fd probe must exit 0 on a clean enumeration"
    );
    let report = std::fs::read_to_string(&marker).unwrap_or_default();
    assert!(
        report.contains("inherited_sockets=0"),
        "the confined target must inherit NO socket descriptors, even a planted one; report: {report:?}"
    );
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

// --- Process tree / seccomp escape denial (independent) ---

#[tokio::test(flavor = "multi_thread")]
#[ignore = "Vertical B: real seccomp filter required; RED against the non-confining stand-in"]
async fn setsid_is_denied_by_seccomp() {
    // Baseline: unconfined, `setsid` succeeds — so a confined `ERR:1` is meaningful.
    assert!(
        unconfined_probe(&["setsid"]).contains("setsid=OK"),
        "unconfined setsid must succeed on the test host"
    );
    let wt = tempfile::tempdir().unwrap();
    let marker = wt.path().join("setsid.result");
    let b = boundary(
        wt.path(),
        vec![probe_dir()],
        vec![],
        Duration::from_secs(30),
    );
    let mut launch = b
        .spawn(probe_target(
            &["setsid", &marker.to_string_lossy()],
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
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "Vertical B: real seccomp filter required; RED against the non-confining stand-in"]
async fn setpgid_is_denied_by_seccomp() {
    // Baseline: unconfined, a FRESH process (no prior setsid) can `setpgid(0,0)` — so a confined
    // `ERR:1` proves the FILTER, not the ordinary "a session leader cannot change its pgid" rule.
    assert!(
        unconfined_probe(&["setpgid"]).contains("setpgid=OK"),
        "unconfined setpgid must succeed on the test host"
    );
    let wt = tempfile::tempdir().unwrap();
    let marker = wt.path().join("setpgid.result");
    let b = boundary(
        wt.path(),
        vec![probe_dir()],
        vec![],
        Duration::from_secs(30),
    );
    let mut launch = b
        .spawn(probe_target(
            &["setpgid", &marker.to_string_lossy()],
            wt.path(),
            BTreeMap::new(),
        ))
        .await
        .expect("launch");
    let _ = launch.process.wait().await;

    let report = std::fs::read_to_string(&marker).unwrap_or_default();
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
    // child + descendant in ONE dedicated non-root cgroup, and that cgroup directory must be
    // REMOVED on teardown. A killpg-only backend never creates a dedicated cgroup, so the members
    // share the harness's own cgroup, which does not disappear.
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

    let monitor = launch.process.identity();
    let target = read_identity_bounded(&tpid, Duration::from_secs(3))
        .await
        .expect("target identity");
    let child = read_identity_bounded(&cpid, Duration::from_secs(3))
        .await
        .expect("child identity");
    let descendant = read_identity_bounded(&dpid, Duration::from_secs(3))
        .await
        .expect("descendant identity");

    let target_cg = cgroup_of(target.pid).expect("target cgroup");
    let harness_cg = cgroup_of(std::process::id() as i32).expect("harness cgroup");

    // RED: the owned cgroup must be a DEDICATED leaf, distinct from the harness's own cgroup.
    let dedicated = target_cg != harness_cg;
    let members = cgroup_procs(&target_cg);
    let all_in_one = [&monitor, &target, &child, &descendant]
        .iter()
        .all(|p| cgroup_of(p.pid).as_deref() == Some(target_cg.as_str()));
    let procs_complete = [&monitor, &target, &child, &descendant]
        .iter()
        .all(|p| members.contains(&p.pid));

    launch.process.force_stop().await.expect("force_stop");
    let _ = launch.process.wait().await;

    // RED: after drain the exact IDENTITIES must be gone AND the owned cgroup directory removed.
    let identities_gone = wait_identity_gone(&target, Duration::from_secs(3)).await
        && wait_identity_gone(&child, Duration::from_secs(3)).await
        && wait_identity_gone(&descendant, Duration::from_secs(3)).await;
    let cgroup_removed = !cgroup_dir(&target_cg).exists();

    for id in [&child, &descendant, &target] {
        if !identity_gone(id) {
            best_effort_kill(id.pid);
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
    assert!(identities_gone, "the whole tree identity must be torn down");
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
    let script = format!(
        "echo $$ > {tp}; \
         ( /bin/dash -c 'echo $$ > {dp}; exec sleep 5' & ); \
         sleep 5; touch {s}",
        tp = tpid.display(),
        dp = dpid.display(),
        s = survived.display(),
    );
    let deadline = Duration::from_millis(500);
    let grace = Duration::from_millis(2500);
    let b = boundary(wt.path(), shell_exec_allow(), vec![], deadline);
    let mut launch = b
        .spawn(dash_target(script, wt.path()))
        .await
        .expect("launch");
    let monitor = launch.process.identity();
    let target = read_identity_bounded(&tpid, Duration::from_secs(2))
        .await
        .expect("target identity");
    let descendant = read_identity_bounded(&dpid, Duration::from_secs(2))
        .await
        .expect("descendant identity");
    let target_cg = cgroup_of(target.pid).expect("target cgroup");

    // ONE bounded window: within deadline + grace the MONITOR itself must finish (kill the tree,
    // reap, remove the cgroup, exit). We wait the monitor, THEN read the teardown oracle — never
    // racing a monitor that reaps before it removes the cgroup.
    let _ = tokio::time::timeout(deadline + grace, launch.process.wait()).await;

    let target_gone = identity_gone(&target);
    let descendant_gone = identity_gone(&descendant);
    let monitor_gone = identity_gone(&monitor);
    let survived_absent = !survived.exists();
    let cgroup_removed = !cgroup_dir(&target_cg).exists();

    // Cleanup happens ONLY after the oracle is captured, never before it.
    let _ = launch.process.force_stop().await;
    let _ = launch.process.wait().await;
    for id in [&target, &descendant] {
        if !identity_gone(id) {
            best_effort_kill(id.pid);
        }
    }

    assert!(
        target_gone,
        "the target ({}) past the deadline must be killed by the monitor",
        target.pid
    );
    assert!(
        descendant_gone,
        "the descendant ({}) must be killed with the target",
        descendant.pid
    );
    assert!(
        monitor_gone,
        "the monitor must exit after the deadline teardown"
    );
    assert!(
        survived_absent,
        "the target ran past the deadline (the `survived` marker was written)"
    );
    assert!(
        cgroup_removed,
        "the owned cgroup {target_cg:?} must be removed after the deadline kill"
    );
}

// --- Report truthfulness / setup failure (four stages) ---

/// A forced setup failure at `fault` must run NO target: the future backend fails that stage,
/// reports NOT enforced (or dies before GO), so the parent fails closed and the target-ran marker
/// is absent. RED: the stand-in does not recognise the fault, reports `enforced`, GOes, and runs
/// the target unconfined — so `spawn` succeeds AND the marker appears.
async fn assert_setup_failure_runs_no_target(fault: &str) {
    let wt = tempfile::tempdir().unwrap();
    let marker = wt.path().join("fs.result");
    let outside_dir = tempfile::tempdir().unwrap();
    let outside = outside_dir.path().join("x.txt");
    let b = boundary_mode(
        wt.path(),
        vec![probe_dir()],
        vec![],
        Duration::from_secs(30),
        &format!("ok;fault={fault}"),
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
    let failed_closed = spawn.is_err();
    if let Ok(mut launch) = spawn {
        let _ = launch.process.wait().await;
    }
    tokio::time::sleep(Duration::from_millis(100)).await;

    assert!(
        !marker.exists(),
        "fault={fault}: a forced setup failure must run NO target; the target-ran marker exists \
         (the stand-in ignored the injected fault and ran the target unconfined)"
    );
    assert!(
        failed_closed,
        "fault={fault}: spawn must fail closed on a setup failure; the stand-in returned a \
         successful launch"
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "Vertical B: real backend fault injection required; RED against the non-confining stand-in"]
async fn a_landlock_setup_failure_runs_no_target() {
    assert_setup_failure_runs_no_target("landlock").await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "Vertical B: real backend fault injection required; RED against the non-confining stand-in"]
async fn a_seccomp_setup_failure_runs_no_target() {
    assert_setup_failure_runs_no_target("seccomp").await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "Vertical B: real backend fault injection required; RED against the non-confining stand-in"]
async fn a_cgroup_setup_failure_runs_no_target() {
    assert_setup_failure_runs_no_target("cgroup").await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "Vertical B: real backend self-check downgrade required; RED against the non-confining stand-in"]
async fn a_self_check_downgrade_runs_no_target() {
    // Distinct from the other three: the backend installs, then a POST-INSTALL self-check finds a
    // dimension not fully enforced and emits a syntactically valid report with that dimension
    // `partial`/`not_enforced`. The parent must reject it and send no GO. RED: the stand-in reports
    // every dimension `enforced` (it never self-checks), so the target runs.
    assert_setup_failure_runs_no_target("self-check").await;
}
