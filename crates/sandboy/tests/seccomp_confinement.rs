//! VB-3 real-seccomp acceptance: network deny (IPv4/IPv6, AF_UNIX preserved), `setsid`/`setpgid`
//! deny, the x32-namespace anti-bypass, an inherited-fd scrub with a planted non-CLOEXEC socket, an
//! exact env allowlist, fork inheritance, a frozen compiled-BPF digest, and fail-closed install.
//!
//! Every test is `#[ignore]`d and needs a kernel with seccomp-BPF. The capability guard is an
//! INDEPENDENT raw `prctl(PR_GET_SECCOMP)` probe — NOT the backend under test — so a broken backend
//! fails RED, never a silent SKIP; with `O7_REQUIRE_SECCOMP` set the incapable host is a FAILURE.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::collections::BTreeMap;
use std::os::fd::AsRawFd as _;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The exact compiled-BPF digest of the VB-3 policy. FROZEN: a `seccompiler`/compiler change or a
/// rule edit changes this, forcing a deliberate review rather than a silent policy drift.
const FROZEN_BPF_DIGEST: &str = "9c8911957d2e3ad307a5c8b0fcc1464c1160df0973633348f9149bbaad0fc9a8";

fn backend_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_sandboy"))
}

/// INDEPENDENT raw seccomp capability probe: `prctl(PR_GET_SECCOMP)` succeeds (>= 0) iff the kernel
/// has seccomp compiled in. Not the system-under-test.
fn kernel_seccomp_available() -> bool {
    // SAFETY: PR_GET_SECCOMP takes no pointer args and returns the current seccomp mode (>=0) or -1.
    let r = unsafe { libc::prctl(libc::PR_GET_SECCOMP) };
    r >= 0
}

fn require_or_skip() -> bool {
    let ok = kernel_seccomp_available();
    if std::env::var_os("O7_REQUIRE_SECCOMP").is_some() {
        assert!(
            ok,
            "O7_REQUIRE_SECCOMP set but the kernel has no seccomp-BPF"
        );
        return true;
    }
    if !ok {
        eprintln!("SKIP: no seccomp-BPF on this host (independent prctl probe)");
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

#[derive(Default)]
struct Run {
    check_fd_closed: Option<i32>,
    env_allow: Vec<String>,
    fault: Option<&'static str>,
    /// An extra inherited (non-CLOEXEC) fd to keep open across the spawn.
    keep_open_listener: Option<std::net::TcpListener>,
}

fn run_seccomp(result: &Path, run: Run) -> (i32, BTreeMap<String, String>) {
    let mut cmd = Command::new(backend_bin());
    cmd.arg("__seccomp-run").arg("--result").arg(result);
    if let Some(fd) = run.check_fd_closed {
        cmd.arg("--check-fd-closed").arg(fd.to_string());
    }
    for name in &run.env_allow {
        cmd.arg("--env-allow").arg(name);
    }
    if let Some(f) = run.fault {
        cmd.env("O7_SC_FAULT", f);
    }
    let status = cmd.status().expect("run sandboy __seccomp-run");
    // keep the listener alive until after the child exits
    drop(run.keep_open_listener);
    (status.code().unwrap_or(-1), parse_result(result))
}

/// A real AF_INET socket (a `TcpListener`) whose fd has CLOEXEC CLEARED, so a spawned child inherits
/// it — the "planted non-CLOEXEC socket" the scrub must close.
fn planted_inheritable_socket() -> (std::net::TcpListener, i32) {
    let l = std::net::TcpListener::bind("127.0.0.1:0").expect("bind planted socket");
    let fd = l.as_raw_fd();
    // SAFETY: F_SETFD with flags 0 clears FD_CLOEXEC on our own fd; no pointer args.
    let r = unsafe { libc::fcntl(fd, libc::F_SETFD, 0) };
    assert_eq!(r, 0, "clear CLOEXEC: {}", std::io::Error::last_os_error());
    (l, fd)
}

#[test]
#[ignore = "Vertical B: real seccomp-BPF required; network deny (IPv4/IPv6), AF_UNIX preserved"]
fn network_is_denied_for_ipv4_and_ipv6_but_af_unix_is_preserved() {
    if !require_or_skip() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let result = dir.path().join("res");
    let (code, res) = run_seccomp(&result, Run::default());

    assert_eq!(code, 0, "result: {res:?}");
    assert_eq!(field(&res, "seccomp"), "enforced");
    // Baseline: all three families succeed unconfined.
    assert_eq!(field(&res, "inet_pre"), "OK");
    assert_eq!(field(&res, "inet6_pre"), "OK");
    assert_eq!(field(&res, "unix_pre"), "OK");
    // Post-install: IPv4/IPv6 denied with EPERM; AF_UNIX still allowed (family-scoped).
    assert_eq!(
        field(&res, "inet_post"),
        "ERR:1",
        "IPv4 socket must be denied EPERM"
    );
    assert_eq!(
        field(&res, "inet6_post"),
        "ERR:1",
        "IPv6 socket must be denied EPERM"
    );
    assert_eq!(
        field(&res, "unix_post"),
        "OK",
        "AF_UNIX must remain allowed (not a blanket ban)"
    );
}

#[test]
#[ignore = "Vertical B: real seccomp-BPF required; setsid/setpgid denied"]
fn setsid_and_setpgid_are_denied() {
    if !require_or_skip() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let result = dir.path().join("res");
    let (_code, res) = run_seccomp(&result, Run::default());
    // Baseline succeeds in a disposable child (exit 0 == success).
    assert_eq!(
        field(&res, "setsid_pre"),
        "0",
        "setsid must succeed unconfined; result: {res:?}"
    );
    assert_eq!(
        field(&res, "setpgid_pre"),
        "0",
        "setpgid must succeed unconfined; result: {res:?}"
    );
    // Post-install: EPERM.
    assert_eq!(
        field(&res, "setsid_post"),
        "ERR:1",
        "setsid must be denied EPERM"
    );
    assert_eq!(
        field(&res, "setpgid_post"),
        "ERR:1",
        "setpgid must be denied EPERM"
    );
}

#[test]
#[ignore = "Vertical B: real seccomp-BPF required; the x32 namespace cannot bypass the socket deny"]
fn the_x32_syscall_namespace_cannot_bypass_the_socket_deny() {
    if !require_or_skip() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let result = dir.path().join("res");
    let (_code, res) = run_seccomp(&result, Run::default());
    // A socket(AF_INET) issued through the x32 syscall number must NOT create a socket. On this
    // kernel (x32 enabled) it is denied EPERM; even without x32 it would be ENOSYS — never OK.
    let x32 = field(&res, "x32_inet_post");
    assert_ne!(
        x32, "OK",
        "x32 socket(AF_INET) created a socket — deny bypassed! result: {res:?}"
    );
    assert!(
        x32.starts_with("ERR:"),
        "x32 socket(AF_INET) must be an error (EPERM), got {x32:?}"
    );
}

#[test]
#[ignore = "Vertical B: real seccomp-BPF required; inherited-fd scrub closes a planted non-CLOEXEC socket"]
fn the_inherited_fd_scrub_closes_a_planted_non_cloexec_socket() {
    if !require_or_skip() {
        return;
    }
    let (listener, fd) = planted_inheritable_socket();
    let dir = tempfile::tempdir().unwrap();
    let result = dir.path().join("res");
    let (code, res) = run_seccomp(
        &result,
        Run {
            check_fd_closed: Some(fd),
            keep_open_listener: Some(listener),
            ..Run::default()
        },
    );
    assert_eq!(code, 0, "result: {res:?}");
    assert_eq!(
        field(&res, "fd_planted_closed"),
        "1",
        "the planted non-CLOEXEC socket (fd {fd}) must be scrubbed before seccomp; result: {res:?}"
    );
}

#[test]
#[ignore = "Vertical B: real seccomp-BPF required; exact env allowlist + fork inheritance"]
fn the_env_is_reduced_to_the_allowlist_and_the_filter_is_inherited() {
    if !require_or_skip() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let result = dir.path().join("res");
    let (_code, res) = run_seccomp(
        &result,
        Run {
            env_allow: vec!["PATH".to_owned()],
            ..Run::default()
        },
    );
    assert_eq!(
        field(&res, "env_only_allowlisted"),
        "1",
        "a child must see ONLY allowlisted env names; result: {res:?}"
    );
    // The filter is inherited across fork: a forked child sees the IPv4 deny (EPERM == errno 1).
    assert_eq!(
        field(&res, "child_inet_post"),
        "1",
        "the seccomp filter must be inherited across fork; result: {res:?}"
    );
}

#[test]
#[ignore = "Vertical B: real seccomp-BPF required; the compiled BPF policy is frozen by digest"]
fn the_compiled_bpf_policy_matches_the_frozen_digest() {
    if !require_or_skip() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let result = dir.path().join("res");
    let (_code, res) = run_seccomp(&result, Run::default());
    assert_eq!(
        field(&res, "bpf_digest"),
        FROZEN_BPF_DIGEST,
        "the compiled seccomp BPF changed — review the policy, then update the frozen digest"
    );
}

#[test]
#[ignore = "Vertical B: real seccomp-BPF required; a failed install fails closed"]
fn a_failed_seccomp_install_fails_closed() {
    if !require_or_skip() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let result = dir.path().join("res");
    let (code, res) = run_seccomp(
        &result,
        Run {
            fault: Some("apply"),
            ..Run::default()
        },
    );
    assert_eq!(field(&res, "seccomp"), "not_enforced");
    assert_eq!(field(&res, "stage"), "apply");
    assert_eq!(
        code, 96,
        "a forced apply failure returns its distinct code; result: {res:?}"
    );
}
