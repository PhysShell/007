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
//! - a setup/lifecycle FAULT matrix (Phase 7b.3) driving one typed `O7_FAULT_POINT` into the REAL
//!   fault-injection artifact via the committed `matrix_backend()` seam, each proving a DISTINCT stage
//!   via a MONITOR-owned witness (`O7_FAULT_WITNESS`): pre-GO faults run no target and fail closed
//!   as NotFullyEnforced; a self-check downgrade is rejected on its merits (not a crash); post-GO
//!   release/execveat run no target; and kill/drain prove teardown-failure dominance;
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

/// TEST-ONLY pre-flip backend SELECTION SEAM. This is the ONLY backend every boundary in this matrix
/// binds, so the 17 non-fault oracles are reproducible FROM THE COMMITTED TREE — no manual binary
/// swap, no scratch file. With `O7_SANDBOY_BIN` set (the designated confinement job), it binds the
/// PINNED fault-injection artifact as the REAL backend; with it unset, it falls back to the frozen
/// fake (`confinement_backend()`), so the RED semantics against the stand-in are unchanged.
///
/// `confinement_backend()` itself is UNTOUCHED until Phase 8 (the production selector flip). This
/// seam does NOT flip production — it only lets the amended oracles exercise the real backend through
/// an explicit, committed env selection:
///   `O7_SANDBOY_BIN=<fault artifact> cargo test -p o7-worker --test sandbox_confinement -- --ignored`
fn matrix_backend() -> BackendImage {
    match std::env::var_os("O7_SANDBOY_BIN") {
        Some(path) => acquire_expected_fault_artifact(PathBuf::from(path)),
        None => confinement_backend(),
    }
}

/// Acquire the real backend at `path`, binding the HARD-EXPECTED fault-artifact identity
/// (`sandboy-linux` / `0.1.0+faultinject`). The identity is a fixed constant — NEVER derived from the
/// environment — so `O7_SANDBOY_BIN` can only select the designated fault artifact and can never
/// smuggle in an arbitrary backend under a chosen identity. A production build is `0.1.0`, so a
/// production binary can never be bound here.
fn acquire_expected_fault_artifact(path: PathBuf) -> BackendImage {
    let bytes = std::fs::read(&path).expect("read O7_SANDBOY_BIN backend binary");
    BackendImage::acquire(
        &path,
        Digest256::of_bytes(&bytes),
        BackendIdentity::new("sandboy-linux", "0.1.0+faultinject").unwrap(),
    )
    .expect("acquire fault-injection backend")
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
        matrix_backend(),
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
        staging_probe: None,
        fault_point: None,
        fault_witness: None,
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

/// A boundary that hands the REAL backend a STAGING-BARRIER witness path (test-only control plane, not
/// the target env) so the proc-isolation reopen oracle can probe the staging fd mid-materialize.
fn boundary_with_staging_probe(
    worktree: &Path,
    allow_exec: Vec<PathBuf>,
    witness: &Path,
) -> SandboyBoundary {
    SandboyBoundary::new(
        matrix_backend(),
        SandboxPolicy {
            worktree: worktree.to_path_buf(),
            allow_exec,
            network: NetworkPolicy::DenyAll,
            env_allowlist: vec![],
            timeout: Duration::from_secs(30),
        },
    )
    .expect("valid boundary")
    .with_backend_config(BackendConfig {
        fake_mode: None,
        staging_probe: Some(witness.to_path_buf()),
        fault_point: None,
        fault_witness: None,
    })
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
    // REQUIRED-HOST + DIFFERENTIAL. Find an executable inside an ACTUAL PROFILE_V1 runtime read root
    // (`/usr/lib`, `/usr/lib64`, `/lib`, `/lib64`). `libc.so.6` is itself a runnable ELF that prints
    // its version and exits 0. This is NOT a skippable test: on the designated confinement host such a
    // candidate MUST exist — its absence is a FAILURE (the matrix header: a missing designated-host
    // capability is a job failure, never a skip).
    let lib = ["/usr/lib", "/usr/lib64", "/lib", "/lib64"]
        .into_iter()
        .map(|r| Path::new(r).join("libc.so.6"))
        .find(|p| p.is_file())
        .expect(
            "designated host must carry libc.so.6 under a PROFILE_V1 runtime read root \
             (/usr/lib{,64}, /lib{,64}) — a missing candidate is a job FAILURE, not a skip",
        );

    // UNCONFINED BASELINE: the EXACT candidate must execve successfully with NO confinement, so the
    // confined denial below is attributable specifically to Landlock's READ_FILE-not-EXECUTE split —
    // not to DAC, a missing execute bit, or ENOEXEC. `status().is_ok()` means the kernel exec'd it.
    let baseline = std::process::Command::new(&lib)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
    assert!(
        baseline.is_ok(),
        "unconfined baseline: {lib:?} must be kernel-executable (else a confined denial proves \
         nothing about Landlock): {baseline:?}"
    );

    let wt = tempfile::tempdir().unwrap();
    let secondary = wt.path().join("escaped");
    let primary = wt.path().join("exec.result");
    // allow_exec = probe_dir + /usr/bin (the probe target + loader); the runtime read root (/usr/lib…)
    // is granted READ_FILE only, NEVER EXECUTE.
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
        "the SAME candidate that execs unconfined must be DENIED under confinement (runtime read root \
         is READ_FILE only, EXECUTE denied); report: {report:?}"
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
// source-identity object; the backend materializes it into a PRIVATE, ruleable execution inode
// proven byte-equal to the source, which is what executes under a NARROW ruleset. Launch-source
// authority is SEPARATE from `allow_exec` — the sealed source is authorized by its seals, never by a
// `/proc/<pid>/fd` grant (see `a_second_sealed_memfd_is_not_executable_by_the_target`).
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

    // LAUNCH-SOURCE authority is SEPARATE from the subprocess `allow_exec` allowlist. The sealed
    // `/proc/<pid>/fd/<n>` source is authorized by its FULL SEAL SET at acquisition (the boundary's
    // `source_authorized` recognizes a `/proc/fd` source and defers to the seal check) — NOT by
    // placing `/proc/<pid>/fd` in `allow_exec` (which would grant EXECUTE over every fd the owner
    // holds; see the adversarial oracle below). `allow_exec` is therefore ORDINARY dirs only; the
    // sealed source runs via its private execution inode and its interpreter/libraries via the
    // resolved runtime read policy.
    let b = boundary(wt.path(), shell_exec_allow(), vec![], Duration::from_secs(30));
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

/// Create a LIVE fully-sealed memfd holding `bytes`, returning the owning `File` (keep it alive for
/// the launch) and its `/proc/<self>/fd/<n>` descriptor path — a caller-held sealed launch source.
fn sealed_memfd(bytes: &[u8], name: &str) -> (std::fs::File, PathBuf) {
    use std::io::Write as _;
    use std::os::unix::io::AsRawFd as _;

    use nix::fcntl::{fcntl, FcntlArg, SealFlag};
    use nix::sys::memfd::{memfd_create, MemFdCreateFlag};

    let cname = std::ffi::CString::new(name).unwrap();
    let owned = memfd_create(&cname, MemFdCreateFlag::MFD_ALLOW_SEALING).expect("memfd_create");
    let mut file = std::fs::File::from(owned);
    file.write_all(bytes).expect("write sealed bytes");
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
    let path = PathBuf::from(format!("/proc/{}/fd/{}", std::process::id(), raw));
    (file, path)
}

/// VB-4 (new, correction 2): the sealed launch target is the ONE exact execution capability. The
/// owner holds a SECOND fully-sealed executable memfd; the confined target attempts to exec it via
/// `/proc/<owner>/fd/<other>`. Because launch-source authority is SEPARATE from `allow_exec` and
/// `/proc/<pid>/fd` is NEVER in the Landlock exec allowlist, the second memfd is not executable — the
/// exec is kernel-denied. Proves "and nothing other than the target runs", which the old
/// `/proc/<pid>/fd`-in-`allow_exec` workaround silently violated.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "Vertical B / VB-4: a second sealed memfd is NOT executable — only the launch target runs"]
async fn a_second_sealed_memfd_is_not_executable_by_the_target() {
    let wt = tempfile::tempdir().unwrap();
    let secondary = wt.path().join("escaped-second");
    let primary = wt.path().join("exec.result");
    // memfd #1 = the sealed launch TARGET (the probe). memfd #2 = a SECOND sealed executable the owner
    // ALSO holds (`touch`) — if the target could exec it, `touch` would create `secondary`.
    let (probe_file, probe_path) =
        sealed_memfd(&std::fs::read(probe_bin()).unwrap(), "o7-sealed-target");
    let (touch_file, touch_path) =
        sealed_memfd(&std::fs::read(touch_bin()).unwrap(), "o7-sealed-second");
    // `allow_exec` is ORDINARY dirs only — NEVER `/proc/fd`. The sealed target is authorized by its
    // seals; the second memfd is authorized by NOTHING and must be kernel-denied.
    let b = boundary(wt.path(), shell_exec_allow(), vec![], Duration::from_secs(30));
    let spec = BoundarySpawnSpec {
        executable: probe_path,
        arguments: [
            OsString::from("exec"),
            OsString::from(&touch_path),
            OsString::from(&secondary),
            OsString::from(&primary),
        ]
        .to_vec(),
        working_directory: wt.path().to_path_buf(),
        environment: BTreeMap::new(),
        stdin: StdinMode::Null,
    };
    let mut launch = b.spawn(spec).await.expect("the sealed target must launch");
    let exit = launch.process.wait().await.expect("waited");
    drop(probe_file);
    drop(touch_file);
    tokio::time::sleep(Duration::from_millis(200)).await;

    assert!(
        !secondary.exists(),
        "a SECOND sealed memfd must NOT be executable by the confined target (only the launch target runs)"
    );
    let report = consume_marker_after_probe_success(exit, || {
        std::fs::read_to_string(&primary).expect("a successful probe must publish its marker")
    });
    assert!(
        report.contains("exec=ERR:13") || report.contains("exec=ERR:1"),
        "exec of the second sealed memfd via /proc/<owner>/fd must be kernel-DENIED; report: {report:?}"
    );
}

/// VB-4 (new, re-gate: object-identity procfs rejection). The `/proc` allowlist ban is decided by the
/// OPENED object's superblock (`fstatfs`), not its pathname — so a SYMLINK ALIAS whose lexical path is
/// NOT under `/proc` but which resolves to the owner's `/proc/<pid>/fd` directory must STILL fail the
/// install closed. Otherwise authority over the owner's whole fd table returns through the alias.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "Vertical B / VB-4: a symlink alias to /proc/fd in allow_exec fails closed (object identity)"]
async fn a_proc_fd_symlink_alias_in_allow_exec_fails_closed() {
    let wt = tempfile::tempdir().unwrap();
    let alias_dir = tempfile::tempdir().unwrap();
    // An innocuously-NAMED symlink (lexical path NOT under /proc) that resolves to the owner's
    // /proc/<pid>/fd directory — the exact bypass the object-identity check must catch.
    let alias = alias_dir.path().join("harmless-name");
    std::os::unix::fs::symlink(format!("/proc/{}/fd", std::process::id()), &alias).unwrap();
    let marker = wt.path().join("fs.result");

    // allow_exec = probe_dir (authorizes the target) + the procfs alias (must be rejected by identity).
    let b = boundary(
        wt.path(),
        vec![probe_dir(), alias.clone()],
        vec![],
        Duration::from_secs(30),
    );
    let spawn = b
        .spawn(probe_target(
            &[
                "fs",
                &wt.path().to_string_lossy(),
                &wt.path().join("c.txt").to_string_lossy(),
                &wt.path().join("o.txt").to_string_lossy(),
                &wt.path().join("t.txt").to_string_lossy(),
            ],
            wt.path(),
            BTreeMap::new(),
        ))
        .await;

    // Setup must FAIL CLOSED: the install rejects the procfs alias, so there is no FullyEnforced
    // launch and the target never runs.
    assert!(
        spawn.is_err(),
        "a procfs-alias allow_exec entry (symlink → /proc/<pid>/fd) must fail the launch CLOSED; got Ok"
    );
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(
        !marker.exists(),
        "the target must NOT run when a procfs alias is present in allow_exec"
    );
}

/// VB-4 (new, correction 3): the DIRECT proof of staging proc-isolation. `PR_SET_DUMPABLE(0)` is set
/// BEFORE the writable staging inode is opened, so while the launch child holds that inode a SAME-UID
/// external process must NOT be able to reopen `/proc/<child>/fd/<staging-fd>` for read OR write.
/// (`ETXTBSY`-after-exec is only supplementary — it would hold even without `PR_SET_DUMPABLE`.) The
/// backend's compile-gated staging barrier publishes `<pid> <fd>` and blocks until we release it.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "Vertical B / VB-4: the staging inode is not reopenable by a same-UID external process"]
async fn the_staging_inode_is_not_reopenable_during_materialize() {
    let wt = tempfile::tempdir().unwrap();
    let witness = wt.path().join("staging.witness");
    let go = wt.path().join("staging.witness.go");
    let b = boundary_with_staging_probe(wt.path(), vec![probe_dir()], &witness);
    let spec = probe_target(
        &["fs", &wt.path().to_string_lossy()],
        wt.path(),
        BTreeMap::new(),
    );

    // The child BLOCKS inside materialize at the barrier, so `spawn` stays pending until we release —
    // run the reopen probe CONCURRENTLY with the spawn, never awaiting spawn first (that would
    // deadlock against our own release).
    let probe = async {
        // BOUNDED witness acquisition (`<child-pid> <staging-fd>`), published once the staging inode is
        // open and the child is already non-dumpable. A missing witness must not hang the test.
        let acquired = tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                if let Ok(s) = std::fs::read_to_string(&witness) {
                    let mut it = s.split_whitespace();
                    if let (Some(p), Some(f)) = (it.next(), it.next()) {
                        if let (Ok(p), Ok(f)) = (p.parse::<i32>(), f.parse::<i32>()) {
                            break (p, f);
                        }
                    }
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await;
        let result = acquired.ok().map(|(pid, fd)| {
            let path = format!("/proc/{pid}/fd/{fd}");
            // Same-UID external reopen attempts — BOTH must be denied while the child is non-dumpable.
            let rd = std::fs::OpenOptions::new()
                .read(true)
                .open(&path)
                .err()
                .and_then(|e| e.raw_os_error());
            let wr = std::fs::OpenOptions::new()
                .write(true)
                .open(&path)
                .err()
                .and_then(|e| e.raw_os_error());
            (rd, wr)
        });
        // ALWAYS release the child, on EVERY exit path (probe done OR witness timeout) — the child must
        // never be left blocked at the barrier.
        let _ = std::fs::write(&go, b"go");
        result
    };

    let (launch, probe_result) = tokio::join!(b.spawn(spec), probe);

    // DISTINCT failures: the barrier must have published its witness...
    let (rd_err, wr_err) =
        probe_result.expect("staging barrier never published its witness within 10s");
    // ...and the production-shaped launch MUST succeed after release — proving the reopen denial was
    // observed inside a REAL enforced launch, not a broken one that merely happened to deny.
    let mut launch = launch.expect("the launch must complete FullyEnforced after the barrier release");
    launch
        .process
        .force_stop()
        .await
        .expect("force_stop the released launch");
    launch.process.wait().await.expect("reap the released launch");

    assert_eq!(
        rd_err,
        Some(nix::errno::Errno::EACCES as i32),
        "a same-UID external READ reopen of the staging inode must be denied (EACCES); got {rd_err:?}"
    );
    assert_eq!(
        wr_err,
        Some(nix::errno::Errno::EACCES as i32),
        "a same-UID external WRITE reopen of the staging inode must be denied (EACCES); got {wr_err:?}"
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

// --- Setup + lifecycle faults driven through the REAL fault-injection artifact (Phase 7b.3) ---
//
// These replace the frozen `O7_FAKE_MODE` setup oracles. Each drives EXACTLY ONE typed `FaultId`
// (`O7_FAULT_POINT`) into the real composed launch state machine via the committed `matrix_backend()`
// seam, and reads a MONITOR-owned stage witness (`O7_FAULT_WITNESS`, written by the unconfined
// monitor/pre-exec child — never by the target, which never runs on a setup fault). The witness names
// the EXACT stage reached, so a generic/malformed failure cannot satisfy a stage-specific oracle.

/// A boundary that injects ONE typed fault + a monitor-owned witness path into the real artifact,
/// both through the trusted control plane (never the target env). `matrix_backend()` selects the real
/// fault artifact when `O7_SANDBOY_BIN` is set; `confinement_backend()` stays the fake and is untouched.
fn boundary_with_fault(
    worktree: &Path,
    allow_exec: Vec<PathBuf>,
    fault: &str,
    witness: &Path,
) -> SandboyBoundary {
    SandboyBoundary::new(
        matrix_backend(),
        SandboxPolicy {
            worktree: worktree.to_path_buf(),
            allow_exec,
            network: NetworkPolicy::DenyAll,
            env_allowlist: vec![],
            timeout: Duration::from_secs(30),
        },
    )
    .expect("valid boundary")
    .with_backend_config(BackendConfig {
        fake_mode: None,
        staging_probe: None,
        fault_point: Some(fault.to_owned()),
        fault_witness: Some(witness.to_path_buf()),
    })
}

/// Count the monitor-owned `o7cg-*` cgroup leaves currently under the harness's own cgroup subtree. A
/// fault that creates a leaf must leave NONE behind (the monitor's teardown removes it on fail-close);
/// a leak is a cleanup failure.
fn owned_cgroup_leaves() -> usize {
    let Some(cg) = cgroup_of(std::process::id() as i32) else {
        return 0;
    };
    let dir = cgroup_dir(&cg);
    std::fs::read_dir(&dir)
        .map(|rd| {
            rd.flatten()
                .filter(|e| {
                    e.file_name()
                        .to_string_lossy()
                        .starts_with("o7cg-")
                })
                .count()
        })
        .unwrap_or(0)
}

/// The observation of a driven fault: whether spawn failed closed (and with what error), whether the
/// target started, the monitor-owned witness, and how many owned cgroup leaves LEAKED.
struct FaultRun {
    spawn_err: Option<String>,
    target_ran: bool,
    witness: Option<String>,
    leaked_cgroups: usize,
}

/// Drive a PRE-GO fault: spawn is expected to fail closed (the report is downgraded, GO withheld), so
/// the target never runs. Bounded throughout; always reaps any (unexpected) live launch.
async fn run_pre_go_fault(fault: &str) -> FaultRun {
    let wt = tempfile::tempdir().unwrap();
    let ran = wt.path().join("ran.marker");
    let witness = wt.path().join("fault.witness");
    let before = owned_cgroup_leaves();
    let b = boundary_with_fault(wt.path(), vec![probe_dir()], fault, &witness);
    let spawn = b
        .spawn(probe_target(
            &["ran", &ran.to_string_lossy()],
            wt.path(),
            BTreeMap::new(),
        ))
        .await;
    let spawn_err = spawn.as_ref().err().map(std::string::ToString::to_string);
    // A pre-GO fault must NOT return a live launch; if it (wrongly) did, reap it so nothing leaks.
    if let Ok(mut l) = spawn {
        let _ = l.process.force_stop().await;
        let _ = l.process.wait().await;
    }
    tokio::time::sleep(Duration::from_millis(200)).await;
    FaultRun {
        spawn_err,
        target_ran: ran.exists(),
        witness: std::fs::read_to_string(&witness).ok(),
        leaked_cgroups: owned_cgroup_leaves().saturating_sub(before),
    }
}

/// Every PRE-GO fault shares this shape: NO target runs; the launch is rejected as `NotFullyEnforced`
/// (a report-VERIFICATION rejection — NOT a cleanup `Signal` error, which would mean an owned member
/// survived, so this also proves the monitor + child were reaped); the monitor-owned witness names
/// EXACTLY the injected stage (a generic error cannot satisfy this); and no owned cgroup leaf leaks.
fn assert_pre_go_fault(fault: &str, expected_stage: &str, r: &FaultRun) {
    assert!(
        !r.target_ran,
        "fault={fault}: NO target may run before GO; the `ran` marker exists (the fault was ignored \
         and the target ran)"
    );
    let err = r.spawn_err.as_deref().unwrap_or("");
    assert!(
        err.contains("full enforcement"),
        "fault={fault}: must be rejected as NotFullyEnforced (report-verification), NOT a cleanup \
         Signal error (an owned member surviving) nor a generic setup error; spawn error: {err:?}"
    );
    assert_eq!(
        r.witness.as_deref(),
        Some(expected_stage),
        "fault={fault}: the MONITOR-owned witness must name EXACTLY stage `{expected_stage}` (proving \
         the launch failed at that stage, not by a generic error); witness: {:?}",
        r.witness
    );
    assert_eq!(
        r.leaked_cgroups, 0,
        "fault={fault}: no owned cgroup leaf may leak — the monitor's fail-close teardown removes it"
    );
}

// --- Class A: pre-GO setup faults (cgroup / staging / runtime / Landlock / seccomp / fd/env/placement
//     / READY). One representative typed fault per stage class; each proves a DISTINCT witness stage. ---

#[tokio::test(flavor = "multi_thread")]
#[ignore = "Vertical B / VB-4 7b.3: real cgroup-create fault fails closed with no target"]
async fn fault_cgroup_create_runs_no_target() {
    assert_pre_go_fault(
        "cgroup_create",
        "cgroup_create",
        &run_pre_go_fault("cgroup_create").await,
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "Vertical B / VB-4 7b.3: real staging (O_TMPFILE) fault fails closed with no target"]
async fn fault_staging_target_tmpfile_runs_no_target() {
    assert_pre_go_fault(
        "target_tmpfile",
        "target_tmpfile",
        &run_pre_go_fault("target_tmpfile").await,
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "Vertical B / VB-4 7b.3: real runtime-interpreter fault fails closed with no target"]
async fn fault_runtime_interpreter_runs_no_target() {
    assert_pre_go_fault(
        "runtime_interpreter",
        "runtime_interpreter",
        &run_pre_go_fault("runtime_interpreter").await,
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "Vertical B / VB-4 7b.3: real Landlock restrict_self fault fails closed with no target"]
async fn fault_landlock_restrict_runs_no_target() {
    assert_pre_go_fault(
        "landlock_restrict",
        "restrict_self",
        &run_pre_go_fault("landlock_restrict").await,
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "Vertical B / VB-4 7b.3: real seccomp apply fault fails closed with no target"]
async fn fault_seccomp_apply_runs_no_target() {
    assert_pre_go_fault("seccomp_apply", "apply", &run_pre_go_fault("seccomp_apply").await);
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "Vertical B / VB-4 7b.3: real fd-scrub verify fault fails closed with no target"]
async fn fault_fd_verify_runs_no_target() {
    assert_pre_go_fault("fd_verify", "fd_verify", &run_pre_go_fault("fd_verify").await);
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "Vertical B / VB-4 7b.3: real env verify fault fails closed with no target"]
async fn fault_env_verify_runs_no_target() {
    assert_pre_go_fault("env_verify", "env_verify", &run_pre_go_fault("env_verify").await);
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "Vertical B / VB-4 7b.3: real cgroup-placement verify fault fails closed with no target"]
async fn fault_cgroup_verify_runs_no_target() {
    assert_pre_go_fault(
        "cgroup_verify",
        "cgroup_verify",
        &run_pre_go_fault("cgroup_verify").await,
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "Vertical B / VB-4 7b.3: child dying before READY (ready_write) fails closed with no target"]
async fn fault_ready_write_runs_no_target() {
    // The child exits before writing its proof → the monitor sees premature EOF (`ready_eof`).
    assert_pre_go_fault("ready_write", "ready_eof", &run_pre_go_fault("ready_write").await);
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "Vertical B / VB-4 7b.3: monitor READY-validation fault fails closed with no target"]
async fn fault_ready_validate_runs_no_target() {
    assert_pre_go_fault(
        "ready_validate",
        "ready_validate",
        &run_pre_go_fault("ready_validate").await,
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "Vertical B / VB-4 7b.3: an UNKNOWN fault selection aborts the launch (no target)"]
async fn fault_invalid_selection_runs_no_target() {
    // A malformed/unknown `O7_FAULT_POINT` must abort BEFORE the child — never run the target.
    assert_pre_go_fault(
        "no_such_fault_xyz",
        "invalid_fault",
        &run_pre_go_fault("no_such_fault_xyz").await,
    );
}

// --- Class B: report-verification downgrade (the special self-check property) ---

#[tokio::test(flavor = "multi_thread")]
#[ignore = "Vertical B / VB-4 7b.3: a self-check downgrade is rejected as NotFullyEnforced on its merits"]
async fn fault_seccomp_self_check_downgrade_is_rejected_on_merits() {
    // DISTINCT from a crash (`seccomp_apply`): the seccomp filter INSTALLS, then the differential
    // effect self-check finds the network not actually denied and HONESTLY downgrades the report. The
    // witness `seccomp_self_check` (not `apply`) proves the launch reached the self-check. The report
    // is correctly bound (assemble binds identity/nonce/digests from the monitor's trusted bindings),
    // so the parent rejects it PURELY on enforcement merits (`NotFullyEnforced`) — not on a binding
    // mismatch or an early crash — with GO withheld, no target, and teardown complete.
    let r = run_pre_go_fault("seccomp_self_check").await;
    assert_pre_go_fault("seccomp_self_check", "seccomp_self_check", &r);
}

// --- Class C: post-GO / lifecycle faults. Target-start is contractually possible only where noted;
//     the release/execveat legs still run NO target (release withheld / exec fails). ---

/// Drive a POST-GO fault where the report was fully enforced (spawn SUCCEEDS + GO is sent), but the
/// fault fires after GO and before the target image runs, so no target executes. Returns whether the
/// target ran + the witness + leaked cgroups.
async fn run_post_go_no_target(fault: &str) -> FaultRun {
    let wt = tempfile::tempdir().unwrap();
    let ran = wt.path().join("ran.marker");
    let witness = wt.path().join("fault.witness");
    let before = owned_cgroup_leaves();
    let b = boundary_with_fault(wt.path(), vec![probe_dir()], fault, &witness);
    let spawn = b
        .spawn(probe_target(
            &["ran", &ran.to_string_lossy()],
            wt.path(),
            BTreeMap::new(),
        ))
        .await;
    // The report was fully enforced, so the parent SENT GO and spawn returned a live launch. Await the
    // monitor's own exit (the post-GO fault ends it), then observe.
    let spawn_err = spawn.as_ref().err().map(std::string::ToString::to_string);
    if let Ok(mut l) = spawn {
        let _ = l.process.wait().await;
        let _ = l.process.force_stop().await;
    }
    tokio::time::sleep(Duration::from_millis(200)).await;
    FaultRun {
        spawn_err,
        target_ran: ran.exists(),
        witness: std::fs::read_to_string(&witness).ok(),
        leaked_cgroups: owned_cgroup_leaves().saturating_sub(before),
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "Vertical B / VB-4 7b.3: a post-GO release fault runs no target and tears down"]
async fn fault_release_runs_no_target() {
    // Post-GO but PRE-execution: the child never receives the release token, so the target never runs.
    let r = run_post_go_no_target("release").await;
    assert!(
        r.spawn_err.is_none(),
        "fault=release: the report was fully enforced, so spawn must SUCCEED (the fault is post-GO); \
         got {:?}",
        r.spawn_err
    );
    assert!(!r.target_ran, "fault=release: the target must NOT run (release withheld)");
    assert_eq!(
        r.witness.as_deref(),
        Some("release"),
        "fault=release: monitor witness must be `release`; got {:?}",
        r.witness
    );
    assert_eq!(r.leaked_cgroups, 0, "fault=release: the monitor's teardown must remove the leaf");
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "Vertical B / VB-4 7b.3: a post-GO execveat fault runs no target and tears down"]
async fn fault_execveat_runs_no_target() {
    // Post-GO, at exec: the execveat is aborted in the pre-exec child, so the target image never runs.
    let r = run_post_go_no_target("execveat").await;
    assert!(!r.target_ran, "fault=execveat: the target image must NOT run (execveat aborted)");
    assert_eq!(
        r.witness.as_deref(),
        Some("execveat"),
        "fault=execveat: monitor/pre-exec witness must be `execveat`; got {:?}",
        r.witness
    );
    assert_eq!(r.leaked_cgroups, 0, "fault=execveat: the monitor's teardown must remove the leaf");
}

// --- Class C teardown-failure dominance: the target RAN (a real tree), then the monitor's teardown is
//     faulted. Cleanup failure DOMINATES; the boundary then fail-closed-reaps the surviving tree. ---

/// Drive a TEARDOWN fault (kill/drain) against the hermetic tree fixture with a short deadline. The
/// tree runs, the deadline fires, and the monitor's `cgroup.kill`/drain is faulted so teardown FAILS.
/// Asserts: the injected teardown stage is witnessed, the monitor's exit is teardown-dominated (the
/// boundary surfaces the cleanup failure, NOT a clean exit), and the boundary ultimately reaps the
/// tree (no owned process survives the test).
async fn assert_teardown_fault_dominates(fault: &str, expected_stage: &str) {
    let wt = tempfile::tempdir().unwrap();
    let (tpid, cpid, dpid, survived) = (
        wt.path().join("t.pid"),
        wt.path().join("c.pid"),
        wt.path().join("d.pid"),
        wt.path().join("survived"),
    );
    let witness = wt.path().join("fault.witness");
    let b = SandboyBoundary::new(
        matrix_backend(),
        SandboxPolicy {
            worktree: wt.path().to_path_buf(),
            allow_exec: fixture_exec_allow(),
            network: NetworkPolicy::DenyAll,
            env_allowlist: vec![],
            timeout: Duration::from_secs(6),
        },
    )
    .expect("valid boundary")
    .with_backend_config(BackendConfig {
        fake_mode: None,
        staging_probe: None,
        fault_point: Some(fault.to_owned()),
        fault_witness: Some(witness.to_path_buf()),
    });
    let mut launch = b
        .spawn(fixture_target(&tpid, &cpid, &dpid, &survived, wt.path()))
        .await
        .expect("the report is fully enforced, so spawn succeeds; the fault is in TEARDOWN");
    let target = read_identity_bounded(&tpid, Duration::from_secs(5))
        .await
        .expect("the target tree must run before the deadline teardown");
    // The monitor's teardown is faulted; wait for the launch to end (bounded). The witness is the
    // dominance proof: `write_fault_witness` runs ONLY on the teardown Err → `exit::TEARDOWN` path, so
    // a witnessed teardown stage means the teardown failure OVERRODE the target's own status. A KILL
    // fault also leaves the tree alive → the boundary additionally surfaces a cleanup `Err`; a DRAIN
    // fault kills the tree but faults the drain OBSERVATION, so the boundary may return the TEARDOWN
    // exit as `Ok`. Either way, the reaping below must leave nothing owned alive.
    let waited = tokio::time::timeout(Duration::from_secs(20), launch.process.wait()).await;
    // A KILL fault leaves the tree alive → the boundary surfaces a cleanup `Err` (extra evidence); a
    // DRAIN fault kills the tree but faults the OBSERVATION, so the boundary may return `Ok`.
    let boundary_flagged_cleanup = matches!(waited, Ok(Err(_)));
    let got_witness = std::fs::read_to_string(&witness).ok();

    // Whatever the monitor left behind, the boundary + test must reap the whole owned tree.
    let _ = launch.process.force_stop().await;
    let _ = launch.process.wait().await;
    if !identity_gone(&target) {
        best_effort_kill(target.pid);
    }
    // The monitor faults the teardown, so the leaf may leak — sweep any residue so the suite stays clean.
    if let Some(cg) = cgroup_of(std::process::id() as i32) {
        if let Ok(rd) = std::fs::read_dir(cgroup_dir(&cg)) {
            for e in rd.flatten() {
                if e.file_name().to_string_lossy().starts_with("o7cg-") {
                    let _ = std::fs::remove_dir(e.path());
                }
            }
        }
    }

    // DOMINANCE: the witness is written ONLY on the teardown `Err` → `exit::TEARDOWN` path, so a
    // witnessed teardown stage proves the teardown failure OVERRODE the target's own status (the
    // triggering deadline-kill). This is the robust dominance evidence for BOTH kill and drain.
    assert_eq!(
        got_witness.as_deref(),
        Some(expected_stage),
        "fault={fault}: the monitor-owned witness must name the teardown stage `{expected_stage}` \
         (written only on the teardown-dominated exit path); got {got_witness:?}"
    );
    // A KILL fault must ALSO surface a boundary cleanup error (the tree survived the faulted kill).
    if fault == "kill" {
        assert!(
            boundary_flagged_cleanup,
            "fault=kill: a faulted `cgroup.kill` leaves the tree alive, so the boundary must surface a \
             cleanup failure; waited: {waited:?}"
        );
    }
    assert!(
        identity_gone(&target),
        "fault={fault}: the owned tree must ultimately be reaped (boundary fail-closed cleanup)"
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "Vertical B / VB-4 7b.3: a faulted cgroup.kill teardown dominates; tree still reaped"]
async fn fault_teardown_kill_dominates() {
    assert_teardown_fault_dominates("kill", "kill").await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "Vertical B / VB-4 7b.3: a faulted drain-observation teardown dominates; tree still reaped"]
async fn fault_teardown_drain_dominates() {
    assert_teardown_fault_dominates("drain", "drain").await;
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
