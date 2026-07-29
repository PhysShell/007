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
#[ignore = "Vertical B: real Landlock execute restriction required; RED against the stand-in"]
fn exec_of_a_non_allowed_binary_is_denied_by_the_kernel() {
    if !require_or_skip() {
        return;
    }
    let Some(touch) = touch_bin() else {
        eprintln!("SKIP: no touch binary");
        return;
    };
    let wt = tempfile::tempdir().unwrap();
    let allow = tempfile::tempdir().unwrap(); // deliberately does NOT contain `touch`
    let secondary = wt.path().join("escaped-exec");
    let result = wt.path().join("exec.result");

    let (code, res) = run_landlock(
        &result,
        RunOpts {
            worktree: wt.path(),
            outside_probe: None,
            allow_exec: &[allow.path()],
            fault: None,
        },
        &[
            "exec",
            &touch.to_string_lossy(),
            &secondary.to_string_lossy(),
        ],
    );

    assert_eq!(
        code, 0,
        "the confined harness itself exits 0; result: {res:?}"
    );
    assert_eq!(field(&res, "filesystem"), "enforced");
    assert!(
        !secondary.exists(),
        "execve of a non-allowed binary must be DENIED; it ran (secondary marker present)"
    );
    assert!(
        matches!(field(&res, "exec"), "ERR:13" | "ERR:1"),
        "the denied exec must report EACCES/EPERM; result: {res:?}"
    );
}

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

/// A sealed memfd holding `path`'s bytes, plus the `/proc/<pid>/fd/<n>` path that names it. Mirrors
/// the frozen `a_sealed_proc_fd_target_executes_under_confinement` setup. The returned `File` must be
/// kept alive for the duration of the run.
fn sealed_memfd_of(path: &Path) -> (File, PathBuf) {
    let bytes = std::fs::read(path).expect("read binary to seal");
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
    file.write_all(&bytes).expect("write sealed bytes");
    let seals = libc::F_SEAL_WRITE | libc::F_SEAL_GROW | libc::F_SEAL_SHRINK | libc::F_SEAL_SEAL;
    // SAFETY: F_ADD_SEALS on our own memfd with a valid seal bitmask; returns 0 or -1.
    let r = unsafe { libc::fcntl(file.as_raw_fd(), libc::F_ADD_SEALS, seals) };
    assert_eq!(
        r,
        0,
        "F_ADD_SEALS failed: {}",
        std::io::Error::last_os_error()
    );
    let proc_path = PathBuf::from(format!(
        "/proc/{}/fd/{}",
        std::process::id(),
        file.as_raw_fd()
    ));
    (file, proc_path)
}

#[test]
#[ignore = "Vertical B: real Landlock required; sealed /proc/<pid>/fd exec + exact file rule"]
fn a_sealed_proc_fd_executable_runs_under_landlock() {
    if !require_or_skip() {
        return;
    }
    let Some(touch) = touch_bin() else {
        eprintln!("SKIP: no touch binary");
        return;
    };
    // A LIVE sealed memfd holding a real executable. A memfd is an ANONYMOUS inode with no
    // filesystem-hierarchy path, so it cannot be named by a Landlock path_beneath rule (add_rule
    // would EBADFD) — and precisely because it is not path-reachable, executing it is not restricted,
    // while the resulting process stays confined. Kept alive here for the duration of the run.
    let (_sealed, sealed_path) = sealed_memfd_of(&touch);

    let wt = tempfile::tempdir().unwrap();
    let mut roots = system_exec_roots();
    // The EXACT regular-file allow rule (a real file, object-type masked to file rights) — proves a
    // file rule installs without EINVAL alongside directory rules. `touch` is also under a system
    // root, so this rule exists purely to exercise the exact-file path.
    roots.push(touch.clone());
    let allow: Vec<&Path> = roots.iter().map(PathBuf::as_path).collect();
    let marker = wt.path().join("sealed-ran");
    let result = wt.path().join("exec.result");

    // The target is named by its /proc/<pid>/fd/<n> path, exactly like the frozen oracle. The harness
    // opens it BEFORE restrict and executes it via the fd (execveat), so the anonymous memfd — which
    // no path rule can name — runs, while the surrounding filesystem stays confined.
    let (code, res) = run_landlock(
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

    assert_eq!(code, 0, "result: {res:?}");
    assert_eq!(
        field(&res, "filesystem"),
        "enforced",
        "an exact regular-file allow rule must install (object-type masked); result: {res:?}"
    );
    assert_eq!(
        field(&res, "exec"),
        "OK",
        "the sealed memfd must EXECUTE under Landlock; result: {res:?}"
    );
    assert!(
        marker.exists(),
        "the sealed executable must run and write its marker"
    );
}

#[test]
#[ignore = "Vertical B: real Landlock required; the writable worktree must NOT be an executable root"]
fn an_executable_inside_the_worktree_is_not_executable_unless_allowlisted() {
    if !require_or_skip() {
        return;
    }
    let Some(touch) = touch_bin() else {
        eprintln!("SKIP: no touch binary");
        return;
    };
    let wt = tempfile::tempdir().unwrap();
    // A REAL runnable executable copied INTO the writable worktree (mode preserved by copy).
    let evil = wt.path().join("evil");
    std::fs::copy(&touch, &evil).unwrap();
    let marker = wt.path().join("pwned");
    // Allow the system roots (so the ONLY thing missing is an exec grant for the worktree binary).
    let roots = system_exec_roots();
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
        &["exec", &evil.to_string_lossy(), &marker.to_string_lossy()],
    );

    assert_eq!(code, 0, "result: {res:?}");
    assert_eq!(field(&res, "filesystem"), "enforced");
    assert!(
        matches!(field(&res, "exec"), "ERR:13" | "ERR:1"),
        "an executable in the WRITABLE worktree must NOT be kernel-executable when absent from \
         allow_exec; result: {res:?}"
    );
    assert!(
        !marker.exists(),
        "the worktree executable RAN despite not being allow-listed — writable became executable"
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
