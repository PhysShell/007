//! Vertical B — REAL kernel confinement acceptance matrix (RED).
//!
//! These pin what a REAL Landlock + seccomp + cgroup-v2 backend must enforce. They run against a
//! NON-confining stand-in — `confinement_backend()` currently returns the frozen fake, which
//! reports every dimension `enforced` but installs nothing — so a launched target ESCAPES, and
//! each assertion observes the escape with a CONCRETE oracle (a file that exists, an exact errno,
//! a surviving pid, an env dump) rather than a vacuous `is_err()`. They are RED today; the GREEN
//! slice makes the real backend actually enforce and points `confinement_backend()` at it, without
//! rewriting a single assertion.
//!
//! Every test is `#[ignore]`d: the PORTABLE workspace suite stays host-agnostic, and the DESIGNATED
//! Linux confinement CI job runs them with `--include-ignored` after asserting the kernel features
//! are present (a missing ABI is a job FAILURE there, never a skip). `#[cfg(feature =
//! "test-harness")]`-gated by construction (the fake backend + probe bins are harness-only).
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
use o7_worker::{BackendImage, ProcessBoundary, SandboyBoundary, StdinMode};

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

fn boundary(
    worktree: &Path,
    allow_exec: Vec<PathBuf>,
    env_allowlist: Vec<OsString>,
    timeout: Duration,
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
    .with_backend_config(o7_worker::BackendConfig {
        fake_mode: Some("ok".to_owned()),
    })
}

fn probe_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_sandbox_probe"))
}

fn probe_dir() -> PathBuf {
    probe_bin().parent().expect("probe dir").to_path_buf()
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
#[ignore = "Vertical B: real Landlock backend required; RED against the non-confining stand-in"]
async fn an_executable_outside_allow_exec_is_refused() {
    // `/bin/dash` runs (allow_exec = [/bin]); `/usr/bin/env` is NOT under the allowance and must
    // not run. This is the LEXICAL gate today (GREEN); Vertical B additionally proves the KERNEL
    // refuses it, but the lexical refusal is already the fail-closed contract.
    let wt = tempfile::tempdir().unwrap();
    let b = boundary(
        wt.path(),
        vec![PathBuf::from("/bin")],
        vec![],
        Duration::from_secs(30),
    );
    let spec = BoundarySpawnSpec {
        executable: PathBuf::from("/usr/bin/env"),
        arguments: vec![],
        working_directory: wt.path().to_path_buf(),
        environment: BTreeMap::new(),
        stdin: StdinMode::Null,
    };
    let result = b.spawn(spec).await;
    assert!(
        result.is_err(),
        "an executable outside allow_exec must be refused before launch"
    );
}

// --- Network / seccomp ---

#[tokio::test(flavor = "multi_thread")]
#[ignore = "Vertical B: real seccomp backend required; RED against the non-confining stand-in"]
async fn network_sockets_are_denied() {
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
    // RED: every socket create must be denied (EPERM/EACCES). The stand-in installs no seccomp, so
    // each bind succeeds (`OK`).
    for family in ["udp4", "tcp4", "udp6"] {
        assert!(
            report.contains(&format!("{family}=ERR")),
            "{family}: socket creation must be DENIED by seccomp; report: {report:?}"
        );
    }
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

// --- Process tree / cgroup v2 ownership ---

#[tokio::test(flavor = "multi_thread")]
#[ignore = "Vertical B: monitor-owned cgroup required; RED against process-group ownership"]
async fn a_setsid_escape_does_not_survive_owned_teardown() {
    let wt = tempfile::tempdir().unwrap();
    let tpid = wt.path().join("target.pid");
    let epid = wt.path().join("escaped.pid");
    // The target records its pid, then starts a descendant in a NEW SESSION (setsid) that records
    // its own pid and sleeps. A cgroup owner contains the escape; process-group ownership does not.
    let script = format!(
        "echo $$ > {tp}; setsid /bin/dash -c 'echo $$ > {ep}; exec sleep 30' & sleep 30",
        tp = tpid.display(),
        ep = epid.display(),
    );
    let b = boundary(
        wt.path(),
        vec![PathBuf::from("/bin")],
        vec![],
        Duration::from_secs(30),
    );
    let mut launch = b
        .spawn(dash_target(script, wt.path()))
        .await
        .expect("launch");

    let target = read_pid_bounded(&tpid, Duration::from_secs(3))
        .await
        .expect("target pid");
    let escaped = read_pid_bounded(&epid, Duration::from_secs(3))
        .await
        .expect("escaped pid");
    assert!(
        pid_alive(target) && pid_alive(escaped),
        "both must be live before teardown"
    );

    launch.process.force_stop().await.expect("force_stop");
    let _ = launch.process.wait().await;

    let target_gone = wait_pid_gone(target, Duration::from_secs(3)).await;
    let escaped_gone = wait_pid_gone(escaped, Duration::from_secs(3)).await;
    // Do not leak the escaped process on the RED path.
    if !escaped_gone {
        let _ = nix::sys::signal::kill(
            nix::unistd::Pid::from_raw(escaped),
            nix::sys::signal::Signal::SIGKILL,
        );
    }
    assert!(target_gone, "the target must be torn down");
    assert!(
        escaped_gone,
        "a setsid escape (pid {escaped}) must NOT survive owned teardown — cgroup ownership required"
    );
}

// --- Timeout ---

#[tokio::test(flavor = "multi_thread")]
#[ignore = "Vertical B: monitor-enforced wall-clock deadline required; RED against the stand-in"]
async fn a_target_outliving_the_deadline_is_killed() {
    let wt = tempfile::tempdir().unwrap();
    let tpid = wt.path().join("target.pid");
    let survived = wt.path().join("survived");
    let script = format!(
        "echo $$ > {tp}; sleep 3; touch {s}",
        tp = tpid.display(),
        s = survived.display(),
    );
    // 500ms deadline; the target sleeps 3s.
    let b = boundary(
        wt.path(),
        vec![PathBuf::from("/bin")],
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

    // Within the deadline + a bounded grace, the target must be dead and never reach `survived`.
    let killed = wait_pid_gone(target, Duration::from_millis(2000)).await;
    // Clean up the lingering target on the RED path.
    let _ = launch.process.force_stop().await;
    let _ = launch.process.wait().await;

    assert!(
        killed,
        "a target past the wall-clock deadline (pid {target}) must be killed by the monitor"
    );
    assert!(
        !survived.exists(),
        "the target ran to completion past the deadline (the `survived` marker was written)"
    );
}
