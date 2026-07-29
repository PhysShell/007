//! VB-3 real-seccomp acceptance: network **deny-all** (IPv4/IPv6 socket AND the io_uring
//! socket-creation path, AF_UNIX preserved), `setsid`/`setpgid` deny, the x32-namespace anti-bypass
//! (exact EPERM), a FAIL-CLOSED inherited-fd scrub with multiple planted descriptors + child-side
//! zero-inherited-sockets, a BYTE-EXACT env allowlist, fork inheritance, a frozen compiled-BPF digest,
//! and the full typed failure matrix (exit 95–100).
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

/// The exact compiled-BPF digest of the VB-3 policy (socket + io_uring + setsid/setpgid, native+x32).
/// FROZEN: a `seccompiler`/compiler change or a rule edit changes this, forcing a deliberate review.
const FROZEN_BPF_DIGEST: &str = "27e32aa7c10a13ca2741daf98985a1542cddde02d1b7094832e8ea6cbd020ad0";

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
    extra_env: Vec<(&'static str, &'static str)>,
    /// Inherited (non-CLOEXEC) fds to keep open across the spawn (the scrub must close them).
    keep_open: Vec<KeepOpen>,
}

/// RAII holders that keep a planted (non-CLOEXEC) fd open across the spawn; the payloads exist only
/// for their `Drop` (closing the fd after the child exits), never read directly.
#[allow(dead_code)]
enum KeepOpen {
    Sock(std::net::TcpListener),
    File(std::fs::File),
}

/// Clear FD_CLOEXEC on `fd` so a spawned child inherits it.
fn make_inheritable(fd: i32) {
    // SAFETY: F_SETFD with flags 0 clears FD_CLOEXEC on our own fd; no pointer args.
    let r = unsafe { libc::fcntl(fd, libc::F_SETFD, 0) };
    assert_eq!(r, 0, "clear CLOEXEC: {}", std::io::Error::last_os_error());
}

fn planted_socket() -> (KeepOpen, i32) {
    let l = std::net::TcpListener::bind("127.0.0.1:0").expect("bind planted socket");
    let fd = l.as_raw_fd();
    make_inheritable(fd);
    (KeepOpen::Sock(l), fd)
}

fn planted_file() -> (KeepOpen, i32) {
    let f = std::fs::File::open("/etc/hostname")
        .or_else(|_| std::fs::File::open("/proc/self/status"))
        .expect("open planted file");
    let fd = f.as_raw_fd();
    make_inheritable(fd);
    (KeepOpen::File(f), fd)
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
    for (k, v) in &run.extra_env {
        cmd.env(k, v);
    }
    let status = cmd.status().expect("run sandboy __seccomp-run");
    drop(run.keep_open); // keep planted fds alive until after the child exits
    (status.code().unwrap_or(-1), parse_result(result))
}

#[test]
#[ignore = "Vertical B: real seccomp-BPF required; network deny-all (socket + io_uring), AF_UNIX preserved"]
fn network_is_denied_for_ipv4_and_ipv6_but_af_unix_is_preserved() {
    if !require_or_skip() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let result = dir.path().join("res");
    let (code, res) = run_seccomp(&result, Run::default());

    assert_eq!(code, 0, "result: {res:?}");
    assert_eq!(field(&res, "seccomp"), "enforced");
    assert_eq!(field(&res, "inet_pre"), "OK");
    assert_eq!(field(&res, "inet6_pre"), "OK");
    assert_eq!(field(&res, "unix_pre"), "OK");
    // EXACT EPERM (errno 1), not merely any error.
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
#[ignore = "Vertical B: real seccomp-BPF required; the io_uring socket-creation bypass is closed"]
fn the_io_uring_socket_creation_path_is_denied() {
    if !require_or_skip() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let result = dir.path().join("res");
    let (_code, res) = run_seccomp(&result, Run::default());
    // io_uring_setup must succeed unconfined (the bypass surface is real) and be EXACT EPERM after.
    assert_eq!(
        field(&res, "iouring_pre"),
        "OK",
        "io_uring_setup must be available unconfined; result: {res:?}"
    );
    assert_eq!(
        field(&res, "iouring_post"),
        "ERR:1",
        "io_uring_setup must be denied EPERM (else IORING_OP_SOCKET bypasses network deny); result: {res:?}"
    );
}

#[test]
#[ignore = "Vertical B: real seccomp-BPF required; setsid/setpgid denied (exact EPERM, fresh children)"]
fn setsid_and_setpgid_are_denied_with_exact_eperm() {
    if !require_or_skip() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let result = dir.path().join("res");
    let (_code, res) = run_seccomp(&result, Run::default());
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
    // Post-install probes run in fresh children (same state as baseline) → EXACT EPERM (errno 1).
    assert_eq!(
        field(&res, "setsid_post"),
        "1",
        "setsid must be denied EPERM in a fresh child"
    );
    assert_eq!(
        field(&res, "setpgid_post"),
        "1",
        "setpgid must be denied EPERM in a fresh child"
    );
}

#[test]
#[ignore = "Vertical B: real seccomp-BPF required; x32 returns EXACT EPERM, not merely any error"]
fn the_x32_namespace_returns_exact_eperm() {
    if !require_or_skip() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let result = dir.path().join("res");
    let (_code, res) = run_seccomp(&result, Run::default());
    // On this kernel x32 is available: baseline succeeds, post is EXACTLY EPERM (a real deny rule,
    // not an incidental ENOSYS).
    assert_eq!(
        field(&res, "x32_inet_pre"),
        "OK",
        "x32 socket must be available unconfined; result: {res:?}"
    );
    assert_eq!(
        field(&res, "x32_inet_post"),
        "ERR:1",
        "x32 socket(AF_INET) must be denied by an EXACT EPERM rule; result: {res:?}"
    );
}

#[test]
#[ignore = "Vertical B: real seccomp-BPF required; fail-closed scrub closes multiple planted fds"]
fn the_inherited_fd_scrub_closes_multiple_planted_descriptors() {
    if !require_or_skip() {
        return;
    }
    let (sock, sock_fd) = planted_socket();
    let (file, _file_fd) = planted_file();
    let dir = tempfile::tempdir().unwrap();
    let result = dir.path().join("res");
    let (code, res) = run_seccomp(
        &result,
        Run {
            check_fd_closed: Some(sock_fd),
            keep_open: vec![sock, file],
            ..Run::default()
        },
    );
    // Reaching `enforced` implies the post-scrub re-enumeration found ONLY keep-fds — both the planted
    // socket AND the planted regular file were closed.
    assert_eq!(code, 0, "result: {res:?}");
    assert_eq!(field(&res, "seccomp"), "enforced");
    assert_eq!(
        field(&res, "fd_planted_closed"),
        "1",
        "the planted socket fd must be closed"
    );
    assert_eq!(
        field(&res, "child_inherited_sockets"),
        "0",
        "a child must inherit ZERO socket descriptors; result: {res:?}"
    );
}

#[test]
#[ignore = "Vertical B: real seccomp-BPF required; byte-exact env allowlist"]
fn the_env_is_reduced_to_a_byte_exact_allowlist() {
    if !require_or_skip() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    // Kept sentinel + dropped sentinel; allowlist keeps only O7_KEEP.
    let (_c, res) = run_seccomp(
        &dir.path().join("a"),
        Run {
            env_allow: vec!["O7_KEEP".to_owned()],
            extra_env: vec![("O7_KEEP", "sentinel-value"), ("O7_DROP", "must-disappear")],
            ..Run::default()
        },
    );
    assert_eq!(
        field(&res, "env_exact"),
        "1",
        "child env must be byte-exactly {{O7_KEEP=sentinel-value}} (O7_DROP gone, value intact); result: {res:?}"
    );

    // Empty allowlist → a genuinely empty environment.
    let (_c2, res2) = run_seccomp(
        &dir.path().join("b"),
        Run {
            env_allow: vec![],
            extra_env: vec![("O7_KEEP", "x"), ("O7_DROP", "y")],
            ..Run::default()
        },
    );
    assert_eq!(
        field(&res2, "env_exact"),
        "1",
        "empty allowlist must yield an empty env; result: {res2:?}"
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
#[ignore = "Vertical B: real seccomp-BPF required; every install/effect/scrub failure fails closed"]
fn the_full_failure_matrix_fails_closed() {
    if !require_or_skip() {
        return;
    }
    // (fault, expected stage, expected exit): the typed failure matrix, each proven to go RED.
    let cases = [
        ("no_new_privs", "no_new_privs", 95),
        ("apply", "apply", 96),
        ("fd_enum_fail", "fd_scrub", 97),
        ("fd_close_fail", "fd_scrub_incomplete", 98),
        ("omit_socket_native", "effect_mismatch", 99),
        ("omit_socket_x32", "effect_mismatch", 99),
        ("omit_setsid", "effect_mismatch", 99),
        ("omit_setpgid", "effect_mismatch", 99),
        ("omit_iouring", "effect_mismatch", 99),
        ("baseline_denied", "baseline_denied", 100),
    ];
    let dir = tempfile::tempdir().unwrap();
    for (fault, stage, exit) in cases {
        // fd_close_fail needs a survivor to detect; plant a socket for every case (harmless).
        let (sock, _fd) = planted_socket();
        let result = dir.path().join(fault);
        let (code, res) = run_seccomp(
            &result,
            Run {
                fault: Some(fault),
                keep_open: vec![sock],
                ..Run::default()
            },
        );
        assert_eq!(
            field(&res, "seccomp"),
            "not_enforced",
            "[{fault}] must be not_enforced"
        );
        assert_eq!(field(&res, "stage"), stage, "[{fault}] wrong stage");
        assert_eq!(code, exit, "[{fault}] wrong exit; result: {res:?}");
    }
}
