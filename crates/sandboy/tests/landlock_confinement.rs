//! VB-2 real-Landlock acceptance for the filesystem confinement + DIFFERENTIAL effect-based
//! self-check, mirroring the frozen Vertical B oracles (`o7-worker/tests/sandbox_confinement.rs`:
//! `writes_are_confined_to_the_worktree`, `exec_of_a_non_allowed_binary_is_denied_by_the_kernel`,
//! `a_sealed_proc_fd_target_executes_under_confinement`) AND proving BOTH directions: an allowed
//! directory / exact-file / sealed-`/proc/<pid>/fd/<n>` executable runs; a non-allowed one is denied;
//! and every fail-closed verdict (install stages + the four self-check verdicts) reports
//! `not_enforced` and never runs the op.
//!
//! The capability guard is an INDEPENDENT raw kernel ABI probe — NOT the system-under-test — so a
//! broken backend fails RED, never a silent SKIP. With `O7_REQUIRE_LANDLOCK` set (the designated
//! runner), an incapable host is a TEST FAILURE, never a skip.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::collections::BTreeMap;
use std::ffi::CString;
use std::fs::File;
use std::io::Write as _;
use std::os::fd::{AsRawFd as _, FromRawFd as _};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

fn backend_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_sandboy"))
}

/// INDEPENDENT raw kernel Landlock ABI probe — the documented `create_ruleset(NULL,0,VERSION)`
/// syscall, issued directly (NOT via the backend under test). Returns the ABI version or -1.
fn kernel_landlock_abi() -> i32 {
    // SAFETY: the documented ABI probe — NULL attr, 0 size, the VERSION flag (1). The kernel reads no
    // user memory in this mode and returns the ABI version (>=1) or -1 with errno set.
    let r = unsafe {
        libc::syscall(
            libc::SYS_landlock_create_ruleset,
            std::ptr::null::<u8>(),
            0_usize,
            1_u32,
        )
    };
    if r < 0 {
        -1
    } else {
        r as i32
    }
}

/// Gate a test: `true` to proceed. When `O7_REQUIRE_LANDLOCK` is set, an incapable host is a FAILURE
/// (never a skip); otherwise an incapable host skips. The predicate is the independent probe above,
/// so a backend regression can never turn a test into a green early return.
fn require_or_skip() -> bool {
    let abi = kernel_landlock_abi();
    let capable = abi >= 3;
    if std::env::var_os("O7_REQUIRE_LANDLOCK").is_some() {
        assert!(
            capable,
            "O7_REQUIRE_LANDLOCK set but the kernel Landlock ABI is {abi} (< 3, needs TRUNCATE)"
        );
        return true;
    }
    if !capable {
        eprintln!("SKIP: kernel Landlock ABI {abi} < 3 (independent probe)");
        return false;
    }
    true
}

fn parse_result(path: &Path) -> BTreeMap<String, String> {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter_map(|l| l.split_once('=').map(|(k, v)| (k.to_owned(), v.to_owned())))
        .collect()
}

fn field<'a>(res: &'a BTreeMap<String, String>, key: &str) -> &'a str {
    res.get(key).map(String::as_str).unwrap_or("")
}

/// Existing system directories a dynamically-linked executable needs to run under Landlock (the
/// binary dirs + the dynamic loader / libraries).
fn system_exec_roots() -> Vec<PathBuf> {
    ["/usr", "/lib", "/lib64", "/bin", "/sbin"]
        .into_iter()
        .map(PathBuf::from)
        .filter(|p| p.exists())
        .collect()
}

fn touch_bin() -> Option<PathBuf> {
    for cand in ["/usr/bin/touch", "/bin/touch"] {
        let p = PathBuf::from(cand);
        if p.exists() {
            return Some(p);
        }
    }
    None
}

struct RunOpts<'a> {
    worktree: &'a Path,
    outside_probe: Option<&'a Path>,
    allow_exec: &'a [&'a Path],
    fault: Option<&'a str>,
}

/// Run `sandboy __landlock-run`. Returns (exit_code, parsed result).
fn run_landlock(result: &Path, opts: RunOpts<'_>, op: &[&str]) -> (i32, BTreeMap<String, String>) {
    let mut cmd = Command::new(backend_bin());
    cmd.arg("__landlock-run")
        .arg("--result")
        .arg(result)
        .arg("--worktree")
        .arg(opts.worktree);
    if let Some(p) = opts.outside_probe {
        cmd.arg("--outside-probe").arg(p);
    }
    for p in opts.allow_exec {
        cmd.arg("--allow-exec").arg(p);
    }
    cmd.arg("--");
    for a in op {
        cmd.arg(a);
    }
    if let Some(f) = opts.fault {
        cmd.env("O7_LL_FAULT", f);
    }
    let status = cmd.status().expect("run sandboy __landlock-run");
    (status.code().unwrap_or(-1), parse_result(result))
}

// --- filesystem confinement (mirrors the frozen oracle) ---

#[test]
#[ignore = "Vertical B: real Landlock ABI >= 3 required; RED against the non-confining stand-in"]
fn writes_are_confined_to_the_worktree() {
    if !require_or_skip() {
        return;
    }
    const OVERWRITE_ORIGINAL: &[u8] = b"ORIGINAL-CONTENT";
    const TRUNCATE_ORIGINAL: &[u8] = b"MUST-STAY-SIZED";

    let wt = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let allow = tempfile::tempdir().unwrap();
    let create = outside.path().join("created.txt");
    let overwrite = outside.path().join("existing-overwrite.txt");
    let truncate = outside.path().join("existing-truncate.txt");
    std::fs::write(&overwrite, OVERWRITE_ORIGINAL).unwrap();
    std::fs::write(&truncate, TRUNCATE_ORIGINAL).unwrap();
    let result = wt.path().join("fs.result");

    let (code, res) = run_landlock(
        &result,
        RunOpts {
            worktree: wt.path(),
            outside_probe: None,
            allow_exec: &[allow.path()],
            fault: None,
        },
        &[
            "fs",
            &create.to_string_lossy(),
            &overwrite.to_string_lossy(),
            &truncate.to_string_lossy(),
        ],
    );

    assert_eq!(code, 0, "a fully-enforced fs run exits 0; result: {res:?}");
    assert_eq!(field(&res, "filesystem"), "enforced");
    assert!(
        wt.path().join("inside.txt").exists() && field(&res, "inside") == "OK",
        "an allowed write inside the worktree must succeed; result: {res:?}"
    );
    assert!(
        !create.exists() && matches!(field(&res, "create"), "ERR:13" | "ERR:1"),
        "creating a file OUTSIDE the worktree must be DENIED; result: {res:?}"
    );
    assert!(
        matches!(field(&res, "overwrite"), "ERR:13" | "ERR:1"),
        "a write to an existing outside file must be DENIED; result: {res:?}"
    );
    assert_eq!(
        std::fs::read(&overwrite).unwrap_or_default(),
        OVERWRITE_ORIGINAL,
        "the existing outside file was modified despite the deny"
    );
    assert!(
        matches!(field(&res, "truncate"), "ERR:13" | "ERR:1"),
        "truncating an existing outside file must be DENIED (ABI-3 TRUNCATE); result: {res:?}"
    );
    assert_eq!(
        std::fs::metadata(&truncate).map(|m| m.len()).unwrap_or(0),
        TRUNCATE_ORIGINAL.len() as u64,
        "the existing outside file was truncated despite the deny"
    );
}

// --- execute restriction: BOTH directions ---

#[test]
#[ignore = "Vertical B: real Landlock required; positive directory-rule exec"]
fn an_allowed_directory_executable_runs_and_writes_inside_the_worktree() {
    if !require_or_skip() {
        return;
    }
    let Some(touch) = touch_bin() else {
        eprintln!("SKIP: no touch binary");
        return;
    };
    let wt = tempfile::tempdir().unwrap();
    let roots = system_exec_roots();
    let allow: Vec<&Path> = roots.iter().map(PathBuf::as_path).collect();
    let marker = wt.path().join("ran-marker");
    let result = wt.path().join("exec.result");

    let (code, res) = run_landlock(
        &result,
        RunOpts {
            worktree: wt.path(),
            outside_probe: None,
            allow_exec: &allow,
            fault: None,
        },
        &["exec", &touch.to_string_lossy(), &marker.to_string_lossy()],
    );

    assert_eq!(code, 0, "result: {res:?}");
    assert_eq!(field(&res, "filesystem"), "enforced");
    assert_eq!(
        field(&res, "exec"),
        "OK",
        "an allowed executable must run; result: {res:?}"
    );
    assert!(
        marker.exists(),
        "the allowed executable must run and write its marker INSIDE the worktree"
    );
}

/// A memfd holding `path`'s bytes, plus the `/proc/<pid>/fd/<n>` path that names it. With
/// `seal_full`, it is fully sealed (WRITE|GROW|SHRINK|SEAL) — the frozen sealed-launch-image shape;
/// otherwise it is an unsealed memfd. The returned `File` must be kept alive for the run.
fn memfd_of(path: &Path, seal_full: bool) -> (File, PathBuf) {
    let bytes = std::fs::read(path).expect("read binary");
    let name = CString::new("o7-sealed-landlock-probe").unwrap();
    // SAFETY: `memfd_create` with a valid NUL-terminated name and MFD_ALLOW_SEALING returns a fresh
    // owned fd or -1; we check the result before using it.
    let fd = unsafe { libc::memfd_create(name.as_ptr(), libc::MFD_ALLOW_SEALING) };
    assert!(
        fd >= 0,
        "memfd_create failed: {}",
        std::io::Error::last_os_error()
    );
    // SAFETY: `fd` is a fresh, exclusively-owned fd from memfd_create; `File` takes sole ownership.
    let mut file = unsafe { File::from_raw_fd(fd) };
    file.write_all(&bytes).expect("write memfd bytes");
    if seal_full {
        let seals =
            libc::F_SEAL_WRITE | libc::F_SEAL_GROW | libc::F_SEAL_SHRINK | libc::F_SEAL_SEAL;
        // SAFETY: F_ADD_SEALS on our own memfd with a valid seal bitmask; returns 0 or -1.
        let r = unsafe { libc::fcntl(file.as_raw_fd(), libc::F_ADD_SEALS, seals) };
        assert_eq!(
            r,
            0,
            "F_ADD_SEALS failed: {}",
            std::io::Error::last_os_error()
        );
    }
    let proc_path = PathBuf::from(format!(
        "/proc/{}/fd/{}",
        std::process::id(),
        file.as_raw_fd()
    ));
    (file, proc_path)
}

// VB-4 authority-model correction: the following two VB-2 harness tests exec'd the TARGET directly
// and asserted it was denied unless present in `allow_exec` — the old semantics where the launch
// target was itself an `allow_exec` member. Under the corrected model the launch target is always the
// sanctioned execution-object capability (a ruleable private inode), and `allow_exec` governs the
// running target's SUBPROCESS execs. Those subprocess-denial properties (a non-allowlisted binary and
// a writable-worktree binary must be denied when exec'd BY the target) now live in the o7-worker
// confinement matrix, which runs a real target that attempts the sub-exec. Removed here:
//   - exec_of_a_non_allowed_binary_is_denied_by_the_kernel
//   - an_executable_inside_the_worktree_is_not_executable_unless_allowlisted

// VB-4 authority-model correction (see docs/architecture/vb4-exec-authority-contract-correction.md):
// a pathless anonymous `memfd` can NOT be a Landlock execution-rule object
// (`landlock_add_rule` → EBADFD), so it is never offered to `install_filesystem` directly. In
// production the sealed source memfd is materialized into a private, ruleable execution INODE (the
// `staging` module) which IS the exec object. This harness therefore asserts the kernel truth: a
// sealed memfd handed straight to `install_filesystem` as the execution object FAILS CLOSED at the
// rule stage. (The positive "the sealed source's bytes execute under confinement" property now lives
// in the o7-worker matrix via the staged private-inode path.)
#[test]
#[ignore = "Vertical B / VB-4: a sealed memfd offered directly as a Landlock exec object fails closed"]
fn a_sealed_memfd_offered_as_execution_object_fails_closed() {
    if !require_or_skip() {
        return;
    }
    let Some(touch) = touch_bin() else {
        eprintln!("SKIP: no touch binary");
        return;
    };
    let (_sealed, sealed_path) = memfd_of(&touch, true);

    let wt = tempfile::tempdir().unwrap();
    let mut roots = system_exec_roots();
    roots.push(sealed_path.clone());
    let allow: Vec<&Path> = roots.iter().map(PathBuf::as_path).collect();
    let marker = wt.path().join("sealed-ran");
    let result = wt.path().join("exec.result");

    let (_code, res) = run_landlock(
        &result,
        RunOpts {
            worktree: wt.path(),
            outside_probe: None,
            allow_exec: &allow,
            fault: None,
        },
        &[
            "exec",
            &sealed_path.to_string_lossy(),
            &marker.to_string_lossy(),
        ],
    );

    assert_eq!(
        field(&res, "filesystem"),
        "not_enforced",
        "a sealed memfd offered as the Landlock exec object must fail closed (EBADFD); result: {res:?}"
    );
    assert_eq!(
        field(&res, "stage"),
        "add_rule",
        "the failure must be at the rule-attach stage (anonymous inode is unruleable); result: {res:?}"
    );
    assert!(
        !marker.exists(),
        "no target may run once install fails closed"
    );
}

/// Run the exec op and return (exit, result) — for the sealed-target negative cases.
fn run_exec(
    result: &Path,
    worktree: &Path,
    allow: &[&Path],
    target: &Path,
    marker: &Path,
) -> (i32, BTreeMap<String, String>) {
    run_landlock(
        result,
        RunOpts {
            worktree,
            outside_probe: None,
            allow_exec: allow,
            fault: None,
        },
        &["exec", &target.to_string_lossy(), &marker.to_string_lossy()],
    )
}

#[test]
#[ignore = "Vertical B: real Landlock required; an UNSEALED memfd allowance fails closed"]
fn an_unsealed_memfd_allowance_fails_closed() {
    if !require_or_skip() {
        return;
    }
    let Some(touch) = touch_bin() else {
        eprintln!("SKIP: no touch binary");
        return;
    };
    // The launch target is an UNSEALED memfd — not the sanctioned exception, and unruleable.
    let (_mem, path) = memfd_of(&touch, false);
    let wt = tempfile::tempdir().unwrap();
    let mut roots = system_exec_roots();
    roots.push(path.clone());
    let allow: Vec<&Path> = roots.iter().map(PathBuf::as_path).collect();
    let marker = wt.path().join("ran");
    let result = wt.path().join("res");

    let (_code, res) = run_exec(&result, wt.path(), &allow, &path, &marker);
    assert_eq!(
        field(&res, "filesystem"),
        "not_enforced",
        "an unsealed memfd allowance must fail closed; result: {res:?}"
    );
    assert!(
        !marker.exists(),
        "the target must not run when install failed closed"
    );
}

#[test]
#[ignore = "Vertical B: real Landlock required; a sealed memfd that is NOT the launch target fails closed"]
fn a_sealed_memfd_allowance_that_is_not_the_launch_target_fails_closed() {
    if !require_or_skip() {
        return;
    }
    let Some(touch) = touch_bin() else {
        eprintln!("SKIP: no touch binary");
        return;
    };
    // A fully-sealed memfd is allow-listed, but the LAUNCH TARGET is an ordinary file — so the sealed
    // memfd cannot claim the detached exception and must be ruled (EBADFD) → fail closed.
    let (_mem, sealed_path) = memfd_of(&touch, true);
    let wt = tempfile::tempdir().unwrap();
    let mut roots = system_exec_roots();
    roots.push(sealed_path.clone());
    let allow: Vec<&Path> = roots.iter().map(PathBuf::as_path).collect();
    let marker = wt.path().join("ran");
    let result = wt.path().join("res");

    let (_code, res) = run_exec(&result, wt.path(), &allow, &touch, &marker);
    assert_eq!(
        field(&res, "filesystem"),
        "not_enforced",
        "a sealed memfd that is not the launch target must fail closed; result: {res:?}"
    );
    assert!(
        !marker.exists(),
        "the target must not run when install failed closed"
    );
}

#[test]
#[ignore = "Vertical B: real Landlock required; deliberate worktree∩allow_exec overlap"]
fn an_exact_worktree_file_added_to_allow_exec_executes() {
    if !require_or_skip() {
        return;
    }
    let Some(touch) = touch_bin() else {
        eprintln!("SKIP: no touch binary");
        return;
    };
    let wt = tempfile::tempdir().unwrap();
    let tool = wt.path().join("tool");
    std::fs::copy(&touch, &tool).unwrap();
    let marker = wt.path().join("ran");
    // The EXACT worktree file, explicitly allow-listed (the deliberate overlap): writable AND now
    // executable. Plus system roots for the dynamic loader/libraries.
    let mut roots = system_exec_roots();
    roots.push(tool.clone());
    let allow: Vec<&Path> = roots.iter().map(PathBuf::as_path).collect();
    let result = wt.path().join("exec.result");

    let (code, res) = run_landlock(
        &result,
        RunOpts {
            worktree: wt.path(),
            outside_probe: None,
            allow_exec: &allow,
            fault: None,
        },
        &["exec", &tool.to_string_lossy(), &marker.to_string_lossy()],
    );

    assert_eq!(code, 0, "result: {res:?}");
    assert_eq!(field(&res, "filesystem"), "enforced");
    assert_eq!(
        field(&res, "exec"),
        "OK",
        "an exact worktree file added to allow_exec must execute (deliberate overlap); result: {res:?}"
    );
    assert!(
        marker.exists(),
        "the allow-listed worktree file must run and write its marker"
    );
}

#[test]
#[ignore = "Vertical B: real Landlock required; TOCTOU authority-widening race"]
fn a_pathname_swap_during_rule_setup_cannot_widen_authority_outside_the_worktree() {
    if !require_or_skip() {
        return;
    }
    let wt = tempfile::tempdir().unwrap();
    // A writable-sans-Landlock directory OUTSIDE the worktree; the swap tries to grant it authority.
    let outside = tempfile::tempdir().unwrap();
    // The allow_exec target is a symlink INSIDE the worktree pointing at an inside dir. The harness
    // opens it (resolving to the inside object) then pauses at the barrier.
    let inside_dir = wt.path().join("inside_allow");
    std::fs::create_dir(&inside_dir).unwrap();
    let alias = wt.path().join("alias");
    std::os::unix::fs::symlink(&inside_dir, &alias).unwrap();

    let create = outside.path().join("created");
    let overwrite = outside.path().join("ow");
    std::fs::write(&overwrite, b"KEEP").unwrap();
    let truncate = outside.path().join("tr");
    std::fs::write(&truncate, b"KEEPSIZED").unwrap();

    let ready = wt.path().join("ready");
    let go = wt.path().join("go");
    let result = wt.path().join("res");

    // Spawn the harness; it opens `alias` (-> inside_dir) then blocks at the barrier for GO.
    let mut child = Command::new(backend_bin())
        .arg("__landlock-run")
        .arg("--result")
        .arg(&result)
        .arg("--worktree")
        .arg(wt.path())
        .arg("--allow-exec")
        .arg(&alias)
        .arg("--")
        .arg("fs")
        .arg(&create)
        .arg(&overwrite)
        .arg(&truncate)
        .env("O7_REQUIRE_LANDLOCK", "1")
        .env("O7_LL_RACE_READY", &ready)
        .env("O7_LL_RACE_GO", &go)
        .spawn()
        .expect("spawn harness");

    // Wait until every rule object is OPEN, then ATOMICALLY swap `alias` -> the OUTSIDE dir, then GO.
    let start = Instant::now();
    while !ready.exists() {
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "harness never signalled READY"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    std::fs::remove_file(&alias).unwrap();
    std::os::unix::fs::symlink(outside.path(), &alias).unwrap(); // now points OUTSIDE the worktree
    std::fs::write(&go, b"1").unwrap();

    let _ = child.wait().expect("wait harness");
    let res = parse_result(&result);

    // The rule bound to the inside object opened BEFORE the swap; the outside object got nothing.
    assert_eq!(field(&res, "filesystem"), "enforced", "result: {res:?}");
    assert!(
        matches!(field(&res, "create"), "ERR:13" | "ERR:1"),
        "outside create must be DENIED despite the swap; result: {res:?}"
    );
    assert!(
        matches!(field(&res, "overwrite"), "ERR:13" | "ERR:1"),
        "outside overwrite must be DENIED despite the swap; result: {res:?}"
    );
    assert!(
        matches!(field(&res, "truncate"), "ERR:13" | "ERR:1"),
        "outside truncate must be DENIED despite the swap; result: {res:?}"
    );
    assert!(
        !create.exists(),
        "outside file created — a swap widened authority"
    );
    assert_eq!(
        std::fs::read(&overwrite).unwrap(),
        b"KEEP",
        "outside file modified — a swap widened authority"
    );
    assert_eq!(
        std::fs::metadata(&truncate).unwrap().len(),
        b"KEEPSIZED".len() as u64,
        "outside file truncated — a swap widened authority"
    );
}

// --- fail-closed matrix: install stages + self-check verdicts + object type ---

/// Assert a fail-closed install: `filesystem=not_enforced`, the expected stage + exit code, and the
/// probe op NEVER ran (no `inside.txt`).
fn assert_fails_closed(opts: RunOpts<'_>, op: &[&str], want_stage: &str, want_code: i32) {
    let wt_owned;
    let worktree = opts.worktree;
    let result = {
        // Put the result outside the worktree so a would-be op never masks it.
        wt_owned = tempfile::tempdir().unwrap();
        wt_owned.path().join("res")
    };
    let (code, res) = run_landlock(&result, opts, op);
    assert_eq!(
        field(&res, "filesystem"),
        "not_enforced",
        "[{want_stage}] must be not_enforced"
    );
    assert_eq!(
        field(&res, "stage"),
        want_stage,
        "[{want_stage}] wrong stage"
    );
    assert_eq!(
        code, want_code,
        "[{want_stage}] wrong exit code; result: {res:?}"
    );
    assert!(
        !worktree.join("inside.txt").exists(),
        "[{want_stage}] op ran despite not_enforced"
    );
}

#[test]
#[ignore = "Vertical B: real Landlock required"]
fn install_stage_failures_report_not_enforced_and_never_launch() {
    if !require_or_skip() {
        return;
    }
    let fs_op = |c: &Path, o: &Path, t: &Path| {
        vec![
            "fs".to_string(),
            c.to_string_lossy().into_owned(),
            o.to_string_lossy().into_owned(),
            t.to_string_lossy().into_owned(),
        ]
    };
    for (fault, stage, exit) in [
        ("abi_enosys", "unsupported", 81),
        ("abi_eopnotsupp", "unsupported", 81),
        ("abi_low", "abi_too_low", 82),
        ("create", "create_ruleset", 80),
        ("add_rule", "add_rule", 84),
        ("add_rule_partial", "add_rule", 84),
        ("no_new_privs", "no_new_privs", 85),
        ("restrict_self", "restrict_self", 86),
    ] {
        let wt = tempfile::tempdir().unwrap();
        let out = tempfile::tempdir().unwrap();
        let allow = tempfile::tempdir().unwrap();
        let op = fs_op(
            &out.path().join("n"),
            &out.path().join("o"),
            &out.path().join("t"),
        );
        let op_ref: Vec<&str> = op.iter().map(String::as_str).collect();
        assert_fails_closed(
            RunOpts {
                worktree: wt.path(),
                outside_probe: None,
                allow_exec: &[allow.path()],
                fault: Some(fault),
            },
            &op_ref,
            stage,
            exit,
        );
    }
}

#[test]
#[ignore = "Vertical B: real Landlock required; the four self-check verdicts + object type"]
fn self_check_and_object_type_verdicts_are_fail_closed() {
    if !require_or_skip() {
        return;
    }
    let out = tempfile::tempdir().unwrap();
    let (c, o, t) = (
        out.path().join("n"),
        out.path().join("o"),
        out.path().join("t"),
    );
    let op = [
        "fs",
        &c.to_string_lossy(),
        &o.to_string_lossy(),
        &t.to_string_lossy(),
    ];

    // 87 — outside probe IS the worktree, so the post-restrict "outside" write is allowed.
    let wt = tempfile::tempdir().unwrap();
    let allow = tempfile::tempdir().unwrap();
    assert_fails_closed(
        RunOpts {
            worktree: wt.path(),
            outside_probe: Some(wt.path()),
            allow_exec: &[allow.path()],
            fault: None,
        },
        &op,
        "self_check_outside",
        87,
    );

    // 88 — a misbuilt ruleset (worktree rule omitted): the inside write is denied post-restrict.
    let wt = tempfile::tempdir().unwrap();
    assert_fails_closed(
        RunOpts {
            worktree: wt.path(),
            outside_probe: None,
            allow_exec: &[allow.path()],
            fault: Some("omit_worktree_rule"),
        },
        &op,
        "self_check_inside",
        88,
    );

    // 89 — the outside write fails post-restrict but NOT with a Landlock deny.
    let wt = tempfile::tempdir().unwrap();
    assert_fails_closed(
        RunOpts {
            worktree: wt.path(),
            outside_probe: None,
            allow_exec: &[allow.path()],
            fault: Some("selfcheck_outside_notdenied"),
        },
        &op,
        "self_check_outside_inconclusive",
        89,
    );

    // 91 — a read-only outside probe dir: the UNCONFINED baseline write already fails, so a later
    // denial could not be attributed to Landlock. (Real condition, no fault.)
    let wt = tempfile::tempdir().unwrap();
    let ro = tempfile::tempdir().unwrap();
    std::fs::set_permissions(
        ro.path(),
        std::os::unix::fs::PermissionsExt::from_mode(0o555),
    )
    .unwrap();
    assert_fails_closed(
        RunOpts {
            worktree: wt.path(),
            outside_probe: Some(ro.path()),
            allow_exec: &[allow.path()],
            fault: None,
        },
        &op,
        "baseline_outside",
        91,
    );
    // restore perms so tempdir cleanup succeeds
    std::fs::set_permissions(
        ro.path(),
        std::os::unix::fs::PermissionsExt::from_mode(0o755),
    )
    .unwrap();

    // 83 — an allow_exec path that does not exist: the O_PATH open fails before any rule.
    let wt = tempfile::tempdir().unwrap();
    let missing = PathBuf::from("/nonexistent/o7-landlock-allow-path");
    assert_fails_closed(
        RunOpts {
            worktree: wt.path(),
            outside_probe: None,
            allow_exec: &[&missing],
            fault: None,
        },
        &op,
        "open_parent",
        83,
    );

    // 90 — a read-only worktree: the UNCONFINED inside baseline write already fails.
    let wt = tempfile::tempdir().unwrap();
    std::fs::set_permissions(
        wt.path(),
        std::os::unix::fs::PermissionsExt::from_mode(0o555),
    )
    .unwrap();
    assert_fails_closed(
        RunOpts {
            worktree: wt.path(),
            outside_probe: None,
            allow_exec: &[allow.path()],
            fault: None,
        },
        &op,
        "baseline_inside",
        90,
    );
    std::fs::set_permissions(
        wt.path(),
        std::os::unix::fs::PermissionsExt::from_mode(0o755),
    )
    .unwrap();

    // 92 — an allow_exec object that is neither a directory nor a regular file (a FIFO).
    let wt = tempfile::tempdir().unwrap();
    let fifo_dir = tempfile::tempdir().unwrap();
    let fifo = fifo_dir.path().join("a-fifo");
    let fifo_c = CString::new(fifo.to_string_lossy().as_bytes()).unwrap();
    // SAFETY: mkfifo with a valid NUL-terminated path and mode 0644; returns 0 or -1.
    let r = unsafe { libc::mkfifo(fifo_c.as_ptr(), 0o644) };
    assert_eq!(r, 0, "mkfifo failed: {}", std::io::Error::last_os_error());
    assert_fails_closed(
        RunOpts {
            worktree: wt.path(),
            outside_probe: None,
            allow_exec: &[&fifo],
            fault: None,
        },
        &op,
        "unsupported_object_type",
        92,
    );
}
