//! VB-2 real-Landlock acceptance for the filesystem confinement + effect-based self-check, mirroring
//! the frozen Vertical B filesystem/exec oracles (`o7-worker/tests/sandbox_confinement.rs`:
//! `writes_are_confined_to_the_worktree`, `exec_of_a_non_allowed_binary_is_denied_by_the_kernel`)
//! and proving the fail-closed install: an unsupported / disabled / insufficient-ABI Landlock, or a
//! failure at any install stage, reports `filesystem=not_enforced` and NEVER runs the probe op.
//!
//! Every test is `#[ignore]`d and needs a REAL kernel with Landlock ABI >= 3 (TRUNCATE). A capability
//! guard SKIPS (does not fail) when Landlock is unavailable, so `--include-ignored` is safe anywhere.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

fn backend_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_sandboy"))
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

/// Run `sandboy __landlock-run`; `fault` sets the test-only `O7_LL_FAULT` control knob. Returns
/// (exit_code, parsed result).
fn run_landlock(
    result: &Path,
    worktree: &Path,
    allow_exec: &[&Path],
    fault: Option<&str>,
    op: &[&str],
) -> (i32, BTreeMap<String, String>) {
    let mut cmd = Command::new(backend_bin());
    cmd.arg("__landlock-run")
        .arg("--result")
        .arg(result)
        .arg("--worktree")
        .arg(worktree);
    for p in allow_exec {
        cmd.arg("--allow-exec").arg(p);
    }
    cmd.arg("--");
    for a in op {
        cmd.arg(a);
    }
    if let Some(f) = fault {
        cmd.env("O7_LL_FAULT", f);
    }
    let status = cmd.status().expect("run sandboy __landlock-run");
    (status.code().unwrap_or(-1), parse_result(result))
}

/// Whether this host can install and PROVE a Landlock filesystem policy (ABI >= 3 present + enabled).
/// Probes by actually running an fs op in a throwaway dir and requiring `filesystem=enforced`.
fn landlock_available() -> bool {
    let wt = match tempfile::tempdir() {
        Ok(d) => d,
        Err(_) => return false,
    };
    let outside = match tempfile::tempdir() {
        Ok(d) => d,
        Err(_) => return false,
    };
    let result = wt.path().join("probe.result");
    let (code, res) = run_landlock(
        &result,
        wt.path(),
        &[wt.path()],
        None,
        &[
            "fs",
            &outside.path().join("c").to_string_lossy(),
            &outside.path().join("o").to_string_lossy(),
            &outside.path().join("t").to_string_lossy(),
        ],
    );
    code == 0 && field(&res, "filesystem") == "enforced"
}

/// A `touch`-like binary that creates its first argument when executed. Used as the NON-allowed exec
/// target; if the kernel lets it run, it leaves an observable `secondary` marker.
fn touch_bin() -> Option<PathBuf> {
    for cand in ["/usr/bin/touch", "/bin/touch"] {
        let p = PathBuf::from(cand);
        if p.exists() {
            return Some(p);
        }
    }
    None
}

#[test]
#[ignore = "Vertical B: real Landlock ABI >= 3 required; RED against the non-confining stand-in"]
fn writes_are_confined_to_the_worktree() {
    if !landlock_available() {
        eprintln!("SKIP: no Landlock ABI >= 3 on this host");
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
    // The overwrite/truncate targets already EXIST — Landlock governs modifying (WRITE_FILE) and
    // emptying (TRUNCATE, ABI 3) them with rights DISTINCT from creating a new file (MAKE_REG).
    std::fs::write(&overwrite, OVERWRITE_ORIGINAL).unwrap();
    std::fs::write(&truncate, TRUNCATE_ORIGINAL).unwrap();
    let result = wt.path().join("fs.result");

    let (code, res) = run_landlock(
        &result,
        wt.path(),
        &[allow.path()],
        None,
        &[
            "fs",
            &create.to_string_lossy(),
            &overwrite.to_string_lossy(),
            &truncate.to_string_lossy(),
        ],
    );

    assert_eq!(code, 0, "a fully-enforced fs run exits 0; result: {res:?}");
    assert_eq!(field(&res, "filesystem"), "enforced");
    // An allowed write INSIDE the worktree must succeed.
    assert!(
        wt.path().join("inside.txt").exists() && field(&res, "inside") == "OK",
        "an allowed write inside the worktree must succeed; result: {res:?}"
    );
    // Creating a NEW outside file must be denied (EACCES/EPERM) and must not exist.
    assert!(
        !create.exists() && matches!(field(&res, "create"), "ERR:13" | "ERR:1"),
        "creating a file OUTSIDE the worktree must be DENIED; result: {res:?}"
    );
    // Overwriting a PRE-EXISTING outside file must be denied AND leave its bytes untouched.
    assert!(
        matches!(field(&res, "overwrite"), "ERR:13" | "ERR:1"),
        "a write to an existing outside file must be DENIED; result: {res:?}"
    );
    assert_eq!(
        std::fs::read(&overwrite).unwrap_or_default(),
        OVERWRITE_ORIGINAL,
        "the existing outside file was modified despite the deny"
    );
    // Truncating a PRE-EXISTING outside file must be denied AND leave its size unchanged.
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

#[test]
#[ignore = "Vertical B: real Landlock execute restriction required; RED against the stand-in"]
fn exec_of_a_non_allowed_binary_is_denied_by_the_kernel() {
    if !landlock_available() {
        eprintln!("SKIP: no Landlock ABI >= 3 on this host");
        return;
    }
    let Some(touch) = touch_bin() else {
        eprintln!("SKIP: no touch binary to use as the non-allowed exec target");
        return;
    };
    let wt = tempfile::tempdir().unwrap();
    let allow = tempfile::tempdir().unwrap(); // deliberately does NOT contain `touch`
    let secondary = wt.path().join("escaped-exec");
    let result = wt.path().join("exec.result");

    let (code, res) = run_landlock(
        &result,
        wt.path(),
        &[allow.path()],
        None,
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

/// Shared assertion for a fail-closed install: `filesystem=not_enforced`, the expected stage + exit
/// code, and — the point — the probe op NEVER ran (no `inside.txt`, outside targets untouched).
fn assert_fails_closed(fault: &str, want_stage: &str, want_code: i32) {
    let wt = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let allow = tempfile::tempdir().unwrap();
    let overwrite = outside.path().join("ow.txt");
    let truncate = outside.path().join("tr.txt");
    std::fs::write(&overwrite, b"KEEP").unwrap();
    std::fs::write(&truncate, b"KEEPSIZED").unwrap();
    let result = wt.path().join("res");

    let (code, res) = run_landlock(
        &result,
        wt.path(),
        &[allow.path()],
        Some(fault),
        &[
            "fs",
            &outside.path().join("new.txt").to_string_lossy(),
            &overwrite.to_string_lossy(),
            &truncate.to_string_lossy(),
        ],
    );

    assert_eq!(
        field(&res, "filesystem"),
        "not_enforced",
        "[{fault}] must be not_enforced"
    );
    assert_eq!(field(&res, "stage"), want_stage, "[{fault}] wrong stage");
    assert_eq!(code, want_code, "[{fault}] wrong exit code");
    // "Never launch": the op did not run.
    assert!(
        !wt.path().join("inside.txt").exists(),
        "[{fault}] op ran despite not_enforced"
    );
    assert_eq!(
        std::fs::read(&overwrite).unwrap_or_default(),
        b"KEEP",
        "[{fault}] outside file touched"
    );
    assert_eq!(
        std::fs::metadata(&truncate).map(|m| m.len()).unwrap_or(0),
        b"KEEPSIZED".len() as u64,
        "[{fault}] outside file truncated"
    );
}

#[test]
#[ignore = "Vertical B: real Landlock required"]
fn an_unsupported_or_disabled_landlock_reports_not_enforced_and_never_launches() {
    if !landlock_available() {
        eprintln!("SKIP: no Landlock ABI >= 3 on this host");
        return;
    }
    // ENOSYS (no Landlock) and EOPNOTSUPP (disabled) both map to the same fail-closed outcome.
    assert_fails_closed("abi_enosys", "unsupported", 81);
    assert_fails_closed("abi_eopnotsupp", "unsupported", 81);
}

#[test]
#[ignore = "Vertical B: real Landlock required"]
fn an_insufficient_abi_reports_not_enforced_and_never_launches() {
    if !landlock_available() {
        eprintln!("SKIP: no Landlock ABI >= 3 on this host");
        return;
    }
    // ABI < 3 cannot enforce TRUNCATE, which the frozen oracle requires.
    assert_fails_closed("abi_low", "abi_too_low", 82);
}

#[test]
#[ignore = "Vertical B: real Landlock required"]
fn a_failed_install_stage_reports_not_enforced_and_never_launches() {
    if !landlock_available() {
        eprintln!("SKIP: no Landlock ABI >= 3 on this host");
        return;
    }
    // Every stage from ruleset creation through the point-of-no-return fails closed, and a PARTIAL
    // ruleset (worktree rule added, an allow_exec rule fails) never reaches restrict_self.
    assert_fails_closed("create", "create_ruleset", 80);
    assert_fails_closed("add_rule", "add_rule", 84);
    assert_fails_closed("add_rule_partial", "add_rule", 84);
    assert_fails_closed("no_new_privs", "no_new_privs", 85);
    assert_fails_closed("restrict_self", "restrict_self", 86);
}
