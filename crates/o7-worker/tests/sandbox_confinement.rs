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
//! - a deadline whose teardown must COMPLETE inside one ABSOLUTE window (`timeout_at` from
//!   target-start, so backend setup is excluded, result asserted — a late monitor fails), with
//!   identities captured in parallel so acquisition never eats the window;
//! - a four-stage setup-failure/report-truthfulness matrix where each stage proves a DISTINCT
//!   failure path via a stage-specific control-plane witness (and self-check proves a valid
//!   downgraded report was REJECTED, not a crash);
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

/// The HERMETIC process-tree fixture binary (VB-4): the real sandboy artifact, whose `__tree-fixture`
/// mode forks a leader + ordinary child + reparented double-fork descendant with NO host-shell
/// dependency. Same `O7_SANDBOY_BIN` the designated confinement job builds and points at the backend;
/// an unset path is a job failure, never a silent skip.
fn tree_fixture_bin() -> PathBuf {
    PathBuf::from(
        std::env::var_os("O7_SANDBOY_BIN")
            .expect("O7_SANDBOY_BIN must point at the sandboy artifact (its __tree-fixture mode)"),
    )
}

/// A `__tree-fixture` target that writes leader/child/descendant pids to `tpid`/`cpid`/`dpid` and,
/// only if it is never killed, `survived`. Replaces the old `/bin/dash -c <script>` tree target.
fn fixture_target(
    tpid: &Path,
    cpid: &Path,
    dpid: &Path,
    survived: &Path,
    worktree: &Path,
) -> BoundarySpawnSpec {
    BoundarySpawnSpec {
        executable: tree_fixture_bin(),
        arguments: [
            "__tree-fixture",
            &tpid.to_string_lossy(),
            &cpid.to_string_lossy(),
            &dpid.to_string_lossy(),
            &survived.to_string_lossy(),
        ]
        .iter()
        .map(OsString::from)
        .collect(),
        working_directory: worktree.to_path_buf(),
        environment: BTreeMap::new(),
        stdin: StdinMode::Null,
    }
}

/// `allow_exec` for a fixture-target launch: the fixture's own directory (the exec-permission gate is
/// on the SOURCE path) plus the loader/library dirs. The fixture forks — it execs no subprocess — so
/// this is only what the exec-permission gate and the runtime read policy require.
fn fixture_exec_allow() -> Vec<PathBuf> {
    let mut v = shell_exec_allow();
    if let Some(dir) = tree_fixture_bin().parent() {
        v.push(dir.to_path_buf());
    }
    v
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

/// Require the confined probe to have exited cleanly (`Code(0)`) BEFORE its marker is read/parsed. A
/// probe that returns a non-zero code — e.g. `sandbox_probe` exiting 2 after a failed or partial
/// marker write — signals an infrastructure ERROR, not a confinement RESULT. Reading its marker
/// anyway could accept a stale, truncated, or leftover oracle as if the kernel had just produced it
/// (a parseable marker plus a non-zero exit must be REJECTED, never parsed). This mirrors the
/// fail-closed `fds` gate, where a success marker is trusted only on `Code(0)`.
fn require_probe_exit_zero(exit: o7_worker::boundary::BoundaryExit) {
    assert_eq!(
        exit,
        o7_worker::boundary::BoundaryExit::Code(0),
        "the confined probe must exit 0 before its marker is read; a non-zero exit is an \
         infrastructure ERROR (e.g. sandbox_probe returning 2 on a failed marker write), not a \
         confinement result; got {exit:?}"
    );
}

/// Consume a confined probe's marker ONLY after the probe exited cleanly. This is the single seam the
/// seven positive-marker oracles (fs, exec, sealed-fs, net, env, setsid, setpgid) and the non-ignored
/// ordering regression both call: it gates on the exit via `require_probe_exit_zero` FIRST and only
/// then runs `consume` (the marker read/parse). Sharing one seam is what makes the regression a real
/// guard — the Vertical B oracles are `#[ignore]`d (compiled but not executed on the hosted gate), so
/// a reorder to "read the marker, then check the exit" would be invisible if the regression exercised
/// a private copy of this logic. Because both go through here, reordering these two statements makes
/// the regression fail on every gate.
fn consume_marker_after_probe_success<T>(
    exit: o7_worker::boundary::BoundaryExit,
    consume: impl FnOnce() -> T,
) -> T {
    require_probe_exit_zero(exit);
    consume()
}

// --- Filesystem / Landlock ---

#[tokio::test(flavor = "multi_thread")]
#[ignore = "Vertical B: real Landlock backend required; RED against the non-confining stand-in"]
async fn writes_are_confined_to_the_worktree() {
    const OVERWRITE_ORIGINAL: &[u8] = b"ORIGINAL-CONTENT";
    const TRUNCATE_ORIGINAL: &[u8] = b"MUST-STAY-SIZED";

    let wt = tempfile::tempdir().unwrap();
    let outside_dir = tempfile::tempdir().unwrap();
    let create = outside_dir.path().join("created.txt");
    let overwrite = outside_dir.path().join("existing-overwrite.txt");
    let truncate = outside_dir.path().join("existing-truncate.txt");
    // The overwrite/truncate targets already EXIST — Landlock governs modifying them (WRITE_FILE)
    // and emptying them (TRUNCATE, ABI 3) with rights DISTINCT from creating a new file (MAKE_REG),
    // so a partial ruleset could deny creation while still leaking modification/truncation.
    std::fs::write(&overwrite, OVERWRITE_ORIGINAL).unwrap();
    std::fs::write(&truncate, TRUNCATE_ORIGINAL).unwrap();
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
                &create.to_string_lossy(),
                &overwrite.to_string_lossy(),
                &truncate.to_string_lossy(),
            ],
            wt.path(),
            BTreeMap::new(),
        ))
        .await
        .expect("launch");
    let exit = launch.process.wait().await.expect("waited");
    let report = consume_marker_after_probe_success(exit, || {
        std::fs::read_to_string(&marker).expect("a successful probe must publish its marker")
    });
    assert!(
        wt.path().join("inside.txt").exists() && report.contains("inside=OK"),
        "an allowed write inside the worktree must succeed; report: {report:?}"
    );
    // RED: creating a new outside file must be denied with the exact errno, and it must not exist.
    assert!(
        !create.exists() && (report.contains("create=ERR:13") || report.contains("create=ERR:1")),
        "creating a file OUTSIDE the worktree must be DENIED (EACCES/EPERM); report: {report:?}"
    );
    // RED: overwriting a PRE-EXISTING outside file must be denied AND leave its bytes untouched — a
    // wrong errno cannot mask actual corruption.
    assert!(
        report.contains("overwrite=ERR:13") || report.contains("overwrite=ERR:1"),
        "a non-truncating write to an existing outside file must be DENIED; report: {report:?}"
    );
    assert_eq!(
        std::fs::read(&overwrite).unwrap_or_default(),
        OVERWRITE_ORIGINAL,
        "the existing outside file was modified despite the deny"
    );
    // RED: truncating a PRE-EXISTING outside file must be denied AND leave its size unchanged.
    assert!(
        report.contains("truncate=ERR:13") || report.contains("truncate=ERR:1"),
        "truncating an existing outside file must be DENIED (ABI-3 TRUNCATE); report: {report:?}"
    );
    assert_eq!(
        std::fs::metadata(&truncate).map(|m| m.len()).unwrap_or(0),
        TRUNCATE_ORIGINAL.len() as u64,
        "the existing outside file was truncated despite the deny"
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
    let exit = launch.process.wait().await.expect("waited");
    tokio::time::sleep(Duration::from_millis(200)).await;

    assert!(
        !secondary.exists(),
        "execve of a non-allowed binary must be DENIED by the kernel; it ran (secondary marker)"
    );
    let report = consume_marker_after_probe_success(exit, || {
        std::fs::read_to_string(&primary).expect("a successful probe must publish its marker")
    });
    assert!(
        report.contains("exec=ERR:13") || report.contains("exec=ERR:1"),
        "the denied exec must report EACCES/EPERM; report: {report:?}"
    );
}

/// A real `touch` binary — the exec-oracle subprocess target (running it creates the `secondary`
/// marker, so an ALLOWED exec is observable without a shell).
fn touch_bin() -> PathBuf {
    for c in ["/usr/bin/touch", "/bin/touch"] {
        let p = PathBuf::from(c);
        if p.exists() {
            return p;
        }
    }
    PathBuf::from("/usr/bin/touch")
}

/// VB-4 subprocess-exec authority (new; the reviewer's assertion set). The confined target execs a
/// subprocess INSIDE `allow_exec` — it must RUN. Complements `exec_of_a_non_allowed_binary…` (the
/// denial direction). RED against the non-confining stand-in only in that it needs the real backend
/// to prove the allowance is honored UNDER confinement.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "Vertical B / VB-4: an allowlisted subprocess must execute under real Landlock"]
async fn an_explicitly_allowlisted_subprocess_executes() {
    let wt = tempfile::tempdir().unwrap();
    let secondary = wt.path().join("ran-allowed");
    let primary = wt.path().join("exec.result");
    let touch = touch_bin();
    // `/usr/bin` is in allow_exec, so `touch` may be exec'd by the target.
    let b = boundary(
        wt.path(),
        vec![probe_dir(), PathBuf::from("/usr/bin"), PathBuf::from("/bin")],
        vec![],
        Duration::from_secs(30),
    );
    let mut launch = b
        .spawn(probe_target(
            &[
                "exec",
                &touch.to_string_lossy(),
                &secondary.to_string_lossy(),
                &primary.to_string_lossy(),
            ],
            wt.path(),
            BTreeMap::new(),
        ))
        .await
        .expect("launch");
    let _ = launch.process.wait().await.expect("waited");
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(
        secondary.exists(),
        "an allowlisted subprocess ({touch:?}) must EXECUTE under confinement and create its marker",
    );
}

/// VB-4 (new): a binary in the WRITABLE worktree is NOT executable unless separately allowlisted —
/// the worktree root carries write rights but NOT `EXECUTE`, so a target that drops a binary there
/// and execs it is denied by the kernel.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "Vertical B / VB-4: a worktree executable is denied unless allowlisted"]
async fn a_worktree_executable_is_denied_unless_allowlisted() {
    let wt = tempfile::tempdir().unwrap();
    let evil = wt.path().join("evil");
    std::fs::copy(touch_bin(), &evil).unwrap();
    let secondary = wt.path().join("escaped");
    let primary = wt.path().join("exec.result");
    // allow_exec is /usr/bin (loader/libraries), NOT the worktree — so the ONLY thing missing is an
    // exec grant for the worktree binary.
    let b = boundary(
        wt.path(),
        vec![probe_dir(), PathBuf::from("/usr/bin"), PathBuf::from("/bin")],
        vec![],
        Duration::from_secs(30),
    );
    let mut launch = b
        .spawn(probe_target(
            &[
                "exec",
                &evil.to_string_lossy(),
                &secondary.to_string_lossy(),
                &primary.to_string_lossy(),
            ],
            wt.path(),
            BTreeMap::new(),
        ))
        .await
        .expect("launch");
    let exit = launch.process.wait().await.expect("waited");
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(
        !secondary.exists(),
        "a writable-worktree binary must NOT be kernel-executable when absent from allow_exec"
    );
    let report = consume_marker_after_probe_success(exit, || {
        std::fs::read_to_string(&primary).expect("a successful probe must publish its marker")
    });
    assert!(
        report.contains("exec=ERR:13") || report.contains("exec=ERR:1"),
        "the denied worktree exec must report EACCES/EPERM; report: {report:?}"
    );
}

/// VB-4 (new): a file in a RUNTIME READ ROOT (`/usr/lib`, granted `READ_FILE` only so the loader can
/// map shared objects) is NOT executable — loading a shared object is a read, never an execute. A
/// target that execs such a file is denied by the kernel.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "Vertical B / VB-4: a runtime-read-root file is denied EXECUTE (READ_FILE only)"]
async fn a_runtime_read_root_executable_is_denied_unless_allowlisted() {
    let lib = ["/usr/lib/libc.so.6", "/lib/x86_64-linux-gnu/libc.so.6"]
        .into_iter()
        .map(PathBuf::from)
        .find(|p| p.exists());
    let Some(lib) = lib else {
        eprintln!("SKIP: no /usr/lib libc to probe");
        return;
    };
    let wt = tempfile::tempdir().unwrap();
    let secondary = wt.path().join("escaped");
    let primary = wt.path().join("exec.result");
    // allow_exec is /usr/bin only; /usr/lib is a runtime READ root (READ_FILE, no EXECUTE).
    let b = boundary(
        wt.path(),
        vec![probe_dir(), PathBuf::from("/usr/bin"), PathBuf::from("/bin")],
        vec![],
        Duration::from_secs(30),
    );
    let mut launch = b
        .spawn(probe_target(
            &[
                "exec",
                &lib.to_string_lossy(),
                &secondary.to_string_lossy(),
                &primary.to_string_lossy(),
            ],
            wt.path(),
            BTreeMap::new(),
        ))
        .await
        .expect("launch");
    let exit = launch.process.wait().await.expect("waited");
    let report = consume_marker_after_probe_success(exit, || {
        std::fs::read_to_string(&primary).expect("a successful probe must publish its marker")
    });
    assert!(
        report.contains("exec=ERR:13") || report.contains("exec=ERR:1"),
        "executing a runtime-read-root file (READ_FILE only) must be DENIED; report: {report:?}"
    );
}

/// VB-4 (new): the RUNNING private execution inode is immutable — a SAME-UID external open of it for
/// write is denied with `ETXTBSY` (the kernel forbids writing a file that is being executed). The
/// target (the tree fixture) exposes its executable via `/proc/<pid>/exe`.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "Vertical B / VB-4: the running execution inode cannot be reopened for write (ETXTBSY)"]
async fn the_running_execution_inode_is_immutable_to_write_reopen() {
    let wt = tempfile::tempdir().unwrap();
    let (tpid, cpid, dpid, survived) = (
        wt.path().join("t.pid"),
        wt.path().join("c.pid"),
        wt.path().join("d.pid"),
        wt.path().join("survived"),
    );
    let b = boundary(wt.path(), fixture_exec_allow(), vec![], Duration::from_secs(30));
    let mut launch = b
        .spawn(fixture_target(&tpid, &cpid, &dpid, &survived, wt.path()))
        .await
        .expect("launch");
    let leader = read_pid_bounded(&tpid, Duration::from_secs(5))
        .await
        .expect("leader pid");
    // Open the running executable inode for WRITE from THIS (same-UID) process → must be ETXTBSY.
    let attempt = std::fs::OpenOptions::new()
        .write(true)
        .open(format!("/proc/{leader}/exe"));
    let err = attempt.err().and_then(|e| e.raw_os_error());
    launch.process.force_stop().await.ok();
    let _ = launch.process.wait().await;
    assert_eq!(
        err,
        Some(nix::errno::Errno::ETXTBSY as i32),
        "reopening the RUNNING execution inode for write must fail with ETXTBSY; got {err:?}"
    );
}

// VB-4 authority-model correction (see docs/architecture/vb4-exec-authority-contract-correction.md
// §4, §9). REPLACES the old `a_sealed_proc_fd_target_executes_under_confinement`, which required a
// sealed `memfd` to EXECUTE under Landlock with the sealed `/proc/fd` path INSIDE `allow_exec` — an
// impossible shape (an anonymous inode is not a Landlock rule object: `landlock_add_rule` → EBADFD,
// and only a root grant would run it). The corrected contract: the sealed source is the transport +
// sealed `memfd` to EXECUTE under Landlock with the sealed `/proc/fd` path INSIDE `allow_exec` — an
// impossible shape (an anonymous inode is not a Landlock rule object: `landlock_add_rule` → EBADFD,
// and only a root grant would run it). The corrected contract: the sealed source is the transport +
// source-identity object; the backend materializes it into a PRIVATE, ruleable execution inode
// proven byte-equal to the source, which is what executes under a NARROW ruleset. The `allow_exec`
// list is ordinary program dirs — NEVER the sealed path.
//
// The internal source→inode bindings (source seals + digest, destination-digest equality, exact
// execution-inode identity, complete resolved-runtime binding) are NOT matrix-visible; they are
// ATTESTED by the `FullyEnforced` verdict — `spawn` succeeds only if the backend reported every
// dimension Enforced, and (per the launch monitor) `filesystem` is Enforced only when the child
// bound the execution-inode identity + runtime-policy digest and the execution digest matched the
// launch-spec target digest. Their FAILURE modes get dedicated forced-failure oracles in Phase 7b.3.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "Vertical B: real Landlock backend required; RED against the non-confining stand-in"]
async fn a_sealed_source_runs_as_private_execution_inode() {
    use std::io::Write as _;
    use std::os::unix::io::AsRawFd as _;

    use nix::fcntl::{fcntl, FcntlArg, SealFlag};
    use nix::sys::memfd::{memfd_create, MemFdCreateFlag};

    let wt = tempfile::tempdir().unwrap();
    let outside_dir = tempfile::tempdir().unwrap();
    let create = outside_dir.path().join("created.txt");
    let overwrite = outside_dir.path().join("existing-overwrite.txt");
    let truncate = outside_dir.path().join("existing-truncate.txt");
    std::fs::write(&overwrite, b"ORIGINAL").unwrap();
    std::fs::write(&truncate, b"SIZED").unwrap();
    let marker = wt.path().join("fs.result");

    // A LIVE fully-sealed memfd source holding the probe.
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

    // The FROZEN Vertical-A exec-permission gate (`SandboxPolicy::permits_exec`) authorizes the
    // launch TARGET by path: a sealed `/proc/<pid>/fd/<n>` source is authorized by granting its
    // `/proc/<pid>/fd` directory (policy.rs §"grant that path or its `/proc/<pid>/fd` directory").
    // That directory is a REAL procfs inode — unlike the anonymous memfd it lists — so the backend
    // can Landlock-rule it; the sealed source itself still runs via its PRIVATE execution inode
    // (materialized + ruled internally), and its interpreter/libraries via the resolved runtime read
    // policy. Ordinary subprocess-exec dirs (`/bin`, `/usr/bin`) round out the allowlist.
    let proc_fd_dir = PathBuf::from(format!("/proc/{}/fd", std::process::id()));
    let mut allow = shell_exec_allow();
    allow.push(proc_fd_dir);
    let b = boundary(wt.path(), allow, vec![], Duration::from_secs(30));
    let spec = BoundarySpawnSpec {
        executable: sealed_path,
        arguments: vec![
            OsString::from("fs"),
            OsString::from(wt.path()),
            OsString::from(&create),
            OsString::from(&overwrite),
            OsString::from(&truncate),
        ],
        working_directory: wt.path().to_path_buf(),
        environment: BTreeMap::new(),
        stdin: StdinMode::Null,
    };
    // `spawn` succeeding IS the attestation: FullyEnforced ⇒ the backend bound the execution-inode
    // identity + runtime-policy digest and matched the execution digest to the launch-spec target.
    let mut launch = b
        .spawn(spec)
        .await
        .expect("the sealed source must run as a FullyEnforced private execution inode");
    let exit = launch.process.wait().await.expect("waited");
    drop(file);

    let report = consume_marker_after_probe_success(exit, || {
        std::fs::read_to_string(&marker).expect("a successful probe must publish its marker")
    });
    assert!(
        report.contains("inside=OK"),
        "the private execution inode derived from the sealed source must EXECUTE; report: {report:?}"
    );
    assert!(
        !create.exists(),
        "the executing inode must be confined; an outside write was allowed"
    );
    assert!(
        report.contains("create=ERR:13") || report.contains("create=ERR:1"),
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
    let exit = launch.process.wait().await.expect("waited");
    let report = consume_marker_after_probe_success(exit, || {
        std::fs::read_to_string(&marker).expect("a successful probe must publish its marker")
    });
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
    // Raise the plant to a HIGH fd (VB-4): the target-side census scans `[3, RLIMIT_NOFILE)`, so a
    // high-numbered inherited socket must be caught. A low-fd-only plant could pass merely because
    // every other test fd happens to sit below some constant — this forces the whole range.
    const PLANTED_HIGH_FD: std::os::fd::RawFd = 900;
    nix::unistd::dup2(raw, PLANTED_HIGH_FD).expect("dup the sentinel to a high fd");
    // dup2's target has CLOEXEC clear by default → inheritable.
    let flags = fcntl(PLANTED_HIGH_FD, FcntlArg::F_GETFD).expect("get flags");
    assert!(
        flags & FdFlag::FD_CLOEXEC.bits() == 0,
        "the high-fd sentinel must be inheritable for this test to mean anything"
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
    // The sentinel must still be OPEN in the harness, so `inherited_sockets=0` proves the monitor's
    // scrub — not a parent-side close that would make ANY backend look clean.
    assert!(
        sentinel.local_addr().is_ok(),
        "the planted sentinel must still be open in the harness for this test to mean anything"
    );
    let _ = nix::unistd::close(PLANTED_HIGH_FD);
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
    let exit = launch.process.wait().await.expect("waited");
    let names = consume_marker_after_probe_success(exit, || {
        std::fs::read_to_string(&marker).expect("a successful probe must publish its marker")
    });
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
    let exit = launch.process.wait().await.expect("waited");
    let report = consume_marker_after_probe_success(exit, || {
        std::fs::read_to_string(&marker).expect("a successful probe must publish its marker")
    });
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
    let exit = launch.process.wait().await.expect("waited");
    let report = consume_marker_after_probe_success(exit, || {
        std::fs::read_to_string(&marker).expect("a successful probe must publish its marker")
    });
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
    let survived = wt.path().join("survived");
    // A SHORT absolute deadline: long enough to capture the live tree's membership, short enough that
    // the MONITOR's OWN teardown (move-out → cgroup.kill → drain → rmdir) — the only path that removes
    // the cgroup DIRECTORY — runs promptly. `force_stop` would SIGKILL the monitor before it can
    // rmdir, so the removal is observed from the monitor's deadline teardown, never from an external
    // kill. The grace bounds that teardown after the deadline fires.
    let deadline = Duration::from_secs(8);
    // Generous teardown grace: on a slow debug host the monitor's kill+drain (DRAIN_BOUND=3s)+rmdir,
    // plus init reaping the reparented descendant and the boundary proving the group drained, carries
    // real jitter. The SEMANTICS need only that the monitor's OWN teardown completes and removes the
    // leaf; the wide grace bounds that without racing scheduler jitter on a 2GB VPS.
    let grace = Duration::from_secs(12);
    let b = boundary(wt.path(), fixture_exec_allow(), vec![], deadline);
    let mut launch = b
        .spawn(fixture_target(&tpid, &cpid, &dpid, &survived, wt.path()))
        .await
        .expect("launch");
    // The monitor arms its deadline at target-start (≈ here); the teardown must land within
    // `deadline + grace` of this instant, backend SETUP correctly excluded.
    let bound_started = tokio::time::Instant::now();

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

    // The MONITOR itself must finish its deadline teardown (kill tree, drain, rmdir, exit) inside the
    // absolute window — the only path that removes the cgroup directory. A late monitor is a FAILURE.
    let completed_in_bound = matches!(
        tokio::time::timeout_at(bound_started + deadline + grace, launch.process.wait()).await,
        Ok(Ok(_))
    );

    // Captured from the monitor's OWN teardown, BEFORE any best-effort cleanup below.
    let identities_gone = wait_identity_gone(&target, Duration::from_secs(3)).await
        && wait_identity_gone(&child, Duration::from_secs(3)).await
        && wait_identity_gone(&descendant, Duration::from_secs(3)).await;
    let cgroup_removed = !cgroup_dir(&target_cg).exists();

    let _ = launch.process.force_stop().await;
    let _ = launch.process.wait().await;
    for id in [&child, &descendant, &target] {
        if !identity_gone(id) {
            best_effort_kill(id.pid);
        }
    }

    assert!(
        completed_in_bound,
        "the monitor must complete its deadline teardown INSIDE deadline + grace"
    );

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
    let cpid = wt.path().join("child.pid");
    let dpid = wt.path().join("desc.pid");
    let survived = wt.path().join("survived");
    // The tree sleeps 3600s — far past the deadline — so the monitor MUST kill the whole tree and
    // remove its cgroup. The deadline is the TARGET's absolute lifetime, armed by the monitor at
    // target-start; the grace bounds the monitor's own kill+reap+cgroup-removal+exit after it fires.
    let deadline = Duration::from_secs(8);
    let grace = Duration::from_secs(12);
    let b = boundary(wt.path(), fixture_exec_allow(), vec![], deadline);

    let mut launch = b
        .spawn(fixture_target(&tpid, &cpid, &dpid, &survived, wt.path()))
        .await
        .expect("launch");
    // The ABSOLUTE window is measured from HERE — the monitor arms its deadline at target-start
    // (i.e. as `spawn` returns), NOT at the `spawn` CALL, so backend SETUP (materialize + narrow
    // Landlock + handshake), which the deadline never governs, is correctly excluded. Everything the
    // monitor still owes — identity capture, kill, reap, cgroup removal, exit — must land inside
    // `deadline + grace` of this instant.
    let bound_started = tokio::time::Instant::now();
    let monitor = launch.process.identity();
    // Capture the two live identities IN PARALLEL (not two sequential 2s reads), so identity
    // acquisition cannot itself consume the window.
    let (target, descendant) = tokio::join!(
        read_identity_bounded(&tpid, Duration::from_secs(2)),
        read_identity_bounded(&dpid, Duration::from_secs(2)),
    );
    let target = target.expect("target identity");
    let descendant = descendant.expect("descendant identity");
    let target_cg = cgroup_of(target.pid).expect("target cgroup");

    // The MONITOR must itself FINISH (kill the tree, reap, remove the cgroup, exit) within the
    // absolute bound. We assert the completion RESULT, never discard it — a monitor that exits one
    // millisecond late is a FAILURE, not a pass.
    let completed_in_bound = matches!(
        tokio::time::timeout_at(bound_started + deadline + grace, launch.process.wait()).await,
        Ok(Ok(_))
    );

    // Capture the teardown oracle BEFORE any cleanup, so `force_stop` never masks a monitor failure.
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
        completed_in_bound,
        "the monitor must complete the deadline teardown INSIDE deadline + grace (it did not finish \
         within the absolute window)"
    );
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

/// What a forced-fault launch produced. The GREEN backend honours a TEST-ONLY control-plane fault
/// on `O7_FAKE_MODE` (`ok;fault=<stage>;witness=<path>`): it fails at exactly `<stage>` and writes a
/// STAGE-SPECIFIC token to `<path>` from that stage's own code path, so each leg proves a DIFFERENT
/// failure path — a backend that returns one generic error for every fault passes none of them.
struct FaultObservation {
    /// `spawn` returned an error (fail-closed) rather than a live launch.
    failed_closed: bool,
    /// The `spawn` error text (present iff `failed_closed`).
    error: Option<String>,
    /// The immediate `ran` marker exists — the target STARTED.
    target_ran: bool,
    /// The backend's stage witness, if any.
    witness: Option<String>,
}

async fn observe_setup_fault(fault: &str) -> FaultObservation {
    let wt = tempfile::tempdir().unwrap();
    let ran = wt.path().join("ran.marker");
    // The GREEN backend writes the stage it reached here; the stand-in ignores the param.
    let witness = wt.path().join("witness");
    let mode = format!("ok;fault={fault};witness={}", witness.display());
    let b = boundary_mode(
        wt.path(),
        vec![probe_dir()],
        vec![],
        Duration::from_secs(30),
        &mode,
    );
    let spawn = b
        .spawn(probe_target(
            &["ran", &ran.to_string_lossy()],
            wt.path(),
            BTreeMap::new(),
        ))
        .await;
    let failed_closed = spawn.is_err();
    let error = spawn.as_ref().err().map(|e| e.to_string());
    if let Ok(mut launch) = spawn {
        let _ = launch.process.wait().await;
    }
    tokio::time::sleep(Duration::from_millis(150)).await;
    FaultObservation {
        failed_closed,
        error,
        target_ran: ran.exists(),
        witness: std::fs::read_to_string(&witness).ok(),
    }
}

/// The three INSTALL-stage faults share a shape: no target runs, spawn fails closed, and the
/// backend witnesses reaching EXACTLY `stage` (so the three legs are not one oracle in a trench
/// coat). RED: the stand-in runs the target, returns a live launch, and writes no witness.
fn assert_install_fault(stage: &str, obs: &FaultObservation) {
    assert!(
        !obs.target_ran,
        "fault={stage}: a forced setup failure must run NO target; the immediate `ran` marker \
         exists (the stand-in ignored the injected fault and ran the target unconfined)"
    );
    assert!(
        obs.failed_closed,
        "fault={stage}: spawn must fail closed; the stand-in returned a live launch"
    );
    assert!(
        obs.witness.as_deref().is_some_and(|w| w.contains(stage)),
        "fault={stage}: the backend must witness failing at the {stage} install stage \
         (stage-specific, not a generic error); witness: {:?}",
        obs.witness
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "Vertical B: real backend Landlock-install fault required; RED against the non-confining stand-in"]
async fn a_landlock_setup_failure_runs_no_target() {
    let obs = observe_setup_fault("landlock").await;
    assert_install_fault("landlock", &obs);
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "Vertical B: real backend seccomp-install fault required; RED against the non-confining stand-in"]
async fn a_seccomp_setup_failure_runs_no_target() {
    let obs = observe_setup_fault("seccomp").await;
    assert_install_fault("seccomp", &obs);
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "Vertical B: real backend cgroup-setup fault required; RED against the non-confining stand-in"]
async fn a_cgroup_setup_failure_runs_no_target() {
    let obs = observe_setup_fault("cgroup").await;
    assert_install_fault("cgroup", &obs);
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "Vertical B: real backend self-check downgrade required; RED against the non-confining stand-in"]
async fn a_self_check_downgrade_runs_no_target() {
    // DISTINCT from the three install faults: the backend installs, then a POST-INSTALL self-check
    // finds a dimension not fully enforced and emits a SYNTACTICALLY VALID report with that
    // dimension downgraded. The parent must reject that report ON ITS MERITS — the failure is a
    // report-VERIFICATION rejection (`NotFullyEnforced`), NOT an early crash / generic setup error.
    let obs = observe_setup_fault("self-check").await;
    assert!(
        !obs.target_ran,
        "fault=self-check: no target may run; the immediate `ran` marker exists"
    );
    assert!(
        obs.error
            .as_deref()
            .is_some_and(|e| e.contains("full enforcement")),
        "fault=self-check: the parent must reject a VALID downgraded report (a NotFullyEnforced \
         verification failure), not fail on a crash/generic error; spawn error: {:?}",
        obs.error
    );
    assert!(
        obs.witness
            .as_deref()
            .is_some_and(|w| w.contains("self-check")),
        "fault=self-check: the backend must witness emitting a downgraded report after its \
         self-check; witness: {:?}",
        obs.witness
    );
}

// --- Consumer-side exit gate (non-vacuous regression) ---

/// Locks the ORDERING the seven positive-marker oracles enforce: the confined probe's exit code is
/// checked BEFORE its marker is read. Every oracle previously did `let _ = wait().await;` and then
/// asserted on the marker CONTENTS, so a probe that left a fully parseable denial marker on disk yet
/// exited 2 (an ERROR — e.g. `sandbox_probe` failing a partial marker write over stale content, per
/// commit `91cb703`) would have been ACCEPTED as a genuine kernel denial.
///
/// Crucially, this guard drives the SAME `consume_marker_after_probe_success` seam the seven oracles
/// use — not a private copy of its logic. That coupling is the whole point: the Vertical B oracles
/// are `#[ignore]`d (compiled but never executed on the hosted gate), so a reorder to "read the
/// marker, then validate the exit" inside the shared seam would be invisible to them. Because this
/// non-ignored test consumes through the same seam and flips `read_reached` only when control reaches
/// the read callback, that reorder makes THIS test fail on every gate. On `Code(2)` the seam must
/// panic in `require_probe_exit_zero` BEFORE running the callback, so `read_reached` stays false —
/// ordering, not mere rejection: a read-then-check seam would run the callback first, flip the flag,
/// and fail the assertion below. On `Code(0)` the same seam runs the callback and returns the
/// parseable marker, which satisfies the real setsid oracle assertion, proving the `Code(2)` case
/// rejected an otherwise fully acceptable marker rather than a straw man. No real Landlock/seccomp
/// backend is needed, so it runs on every hosted gate.
#[test]
fn a_parseable_marker_paired_with_a_nonzero_exit_is_rejected_before_parsing() {
    use std::sync::atomic::{AtomicBool, Ordering};

    let dir = tempfile::tempdir().unwrap();
    let marker = dir.path().join("setsid.result");
    // A leftover, fully parseable denial marker — exactly the bytes a GREEN confined run produces.
    std::fs::write(&marker, "setsid=ERR:1\n").unwrap();

    // Consume the planted marker through the REAL shared seam, exactly as the seven oracles do. The
    // read callback flips `read_reached` iff the seam lets control reach it, so the assertions below
    // observe the seam's own ordering — not a private reimplementation of it.
    let read_reached = AtomicBool::new(false);
    let consume_via_seam = |exit: o7_worker::boundary::BoundaryExit| -> String {
        consume_marker_after_probe_success(exit, || {
            read_reached.store(true, Ordering::SeqCst);
            std::fs::read_to_string(&marker).unwrap_or_default()
        })
    };

    // Code(2): the shared seam must panic in the exit gate BEFORE the callback runs. `catch_unwind`
    // (no process-global panic-hook mutation — that would race with other tests in this process)
    // confirms the panic; `read_reached == false` confirms the ORDERING through the real seam.
    let rejected = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = consume_via_seam(o7_worker::boundary::BoundaryExit::Code(2));
    }))
    .is_err();
    assert!(
        rejected,
        "a non-zero probe exit must be REJECTED by the shared seam before the marker is parsed"
    );
    assert!(
        !read_reached.load(Ordering::SeqCst),
        "the stale marker must NOT be read when the probe exited non-zero — the shared seam's exit \
         gate must PRECEDE the marker read, not follow it"
    );

    // Code(0): the SAME seam runs the callback and returns content that satisfies the real setsid
    // oracle assertion — so the Code(2) case above rejected an otherwise fully acceptable marker.
    let report = consume_via_seam(o7_worker::boundary::BoundaryExit::Code(0));
    assert!(
        read_reached.load(Ordering::SeqCst),
        "a clean exit must let the shared seam run the marker-read callback"
    );
    assert!(
        report.contains("setsid=ERR:1"),
        "the planted marker must be parseable so the pre-gate accept-path is genuinely exercised; \
         report: {report:?}"
    );
}
