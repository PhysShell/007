//! VB-3 — real **seccomp** confinement (network deny-all + `setsid`/`setpgid` deny), a fail-closed
//! inherited-fd scrub, and an exact env allowlist, all PROVEN by DIFFERENTIAL effect-based
//! self-checks with typed fail-closed verdicts. **No Landlock/cgroup change; no `confinement_backend()`
//! switch; no RED-matrix flip; no VB-4 integration.**
//!
//! Network deny-all is REAL, not a single-door lock: the filter denies `socket(AF_INET|AF_INET6)` AND
//! the io_uring path to socket creation (`io_uring_setup`/`enter`/`register`), for BOTH the native and
//! the x32 syscall numbers, and the fd scrub drops any inherited ring/socket. AF_UNIX stays allowed
//! (family-scoped). `setsid`/`setpgid` are denied unconditionally. Default action Allow; deny action
//! `Errno(EPERM)`.
//!
//! Enforcement is PROVEN, never asserted: every intended-denied op is proven PERMITTED unconfined
//! (baseline), then required to return EXACTLY `EPERM` after install — and the baseline results
//! PARTICIPATE in the verdict, so a pre-existing denial cannot masquerade as this filter's. `setsid`/
//! `setpgid` are probed in fresh children both before and after, identical process state. A digest of
//! the compiled BPF is frozen in a test. Compiled ONLY under `test-harness`; a production build
//! compiles neither this module nor `seccompiler`/`libc` nor any `unsafe`.

mod sys;

// The process/fd raw primitives the VB-4 launch state machine drives from safe code (their `unsafe`
// lives in `sys`). Re-exported at `crate::seccomp::*` so `launch` needs no new unsafe location.
pub(crate) use sys::{
    borrow_fd_as_file, close_fd, dup2_fd, exit_now, fd_is_socket, fork_raw, list_fds,
    make_pipe_cloexec, read_fd, try_wait, write_fd, ForkResult,
};

use std::collections::BTreeMap;
// These are used only by the `test-harness`-gated standalone `__seccomp-run` driver.
#[cfg(feature = "test-harness")]
use std::ffi::OsString;
#[cfg(feature = "test-harness")]
use std::fs::File;
#[cfg(feature = "test-harness")]
use std::io::Write as _;
#[cfg(feature = "test-harness")]
use std::os::fd::AsRawFd as _;
use std::os::fd::RawFd;
#[cfg(feature = "test-harness")]
use std::path::PathBuf;

#[cfg(feature = "test-harness")]
use o7_sandbox_protocol::ids::Digest256;
use seccompiler::{
    BpfProgram, SeccompAction, SeccompCmpArgLen, SeccompCmpOp, SeccompCondition, SeccompFilter,
    SeccompRule, TargetArch,
};

const EPERM: i32 = libc::EPERM;

/// Why the seccomp policy is not a proven, fully-enforced boundary. Typed, distinct stage + exit code.
/// Some variants/accessors are constructed only by the test-harness/fault paths; inert in pure prod.
#[derive(Debug)]
#[allow(dead_code)]
pub(crate) enum InstallError {
    /// Not x86_64 — the x32 guarantee is expressed for x86_64 only; fail the arch gate, never Allow.
    #[allow(dead_code)]
    UnsupportedArch,
    FilterBuild(String),
    NoNewPrivs(i32),
    Apply(String),
    /// The inherited-fd enumeration itself failed (cannot prove the scrub).
    FdScrub(i32),
    /// After the scrub, an fd outside the keep-set survived (proven by independent re-enumeration).
    FdScrubIncomplete(RawFd),
    /// An intended-denied op was ALREADY denied at baseline — a later denial cannot be attributed to
    /// this filter.
    BaselineDenied(&'static str),
    /// A post-install effect did not match the policy (the filter did not really take).
    EffectMismatch(&'static str),
}

impl InstallError {
    #[cfg(feature = "test-harness")]
    pub(crate) fn stage(&self) -> &'static str {
        match self {
            InstallError::UnsupportedArch => "unsupported_arch",
            InstallError::FilterBuild(_) => "filter_build",
            InstallError::NoNewPrivs(_) => "no_new_privs",
            InstallError::Apply(_) => "apply",
            InstallError::FdScrub(_) => "fd_scrub",
            InstallError::FdScrubIncomplete(_) => "fd_scrub_incomplete",
            InstallError::BaselineDenied(_) => "baseline_denied",
            InstallError::EffectMismatch(_) => "effect_mismatch",
        }
    }
    #[cfg(feature = "test-harness")]
    pub(crate) fn exit_code(&self) -> i32 {
        match self {
            InstallError::UnsupportedArch => 93,
            InstallError::FilterBuild(_) => 94,
            InstallError::NoNewPrivs(_) => 95,
            InstallError::Apply(_) => 96,
            InstallError::FdScrub(_) => 97,
            InstallError::FdScrubIncomplete(_) => 98,
            InstallError::EffectMismatch(_) => 99,
            InstallError::BaselineDenied(_) => 100,
        }
    }
}

impl std::fmt::Display for InstallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InstallError::UnsupportedArch => {
                write!(f, "seccomp x32-safe policy is x86_64-only here")
            }
            InstallError::FilterBuild(e) => write!(f, "seccomp filter build: {e}"),
            InstallError::NoNewPrivs(e) => write!(f, "prctl(NO_NEW_PRIVS): errno {e}"),
            InstallError::Apply(e) => write!(f, "seccomp apply_filter: {e}"),
            InstallError::FdScrub(e) => write!(f, "fd enumeration failed: errno {e}"),
            InstallError::FdScrubIncomplete(fd) => write!(f, "fd scrub left fd {fd} open"),
            InstallError::BaselineDenied(w) => write!(f, "baseline already denied: {w}"),
            InstallError::EffectMismatch(w) => write!(f, "post-install effect mismatch: {w}"),
        }
    }
}

/// TEST-ONLY forced-fault knob (`O7_SC_FAULT`): forces one install/effect/scrub failure so every
/// fail-closed verdict is provable.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum Fault {
    #[default]
    None,
    NoNewPrivs,
    Apply,
    FdEnumFail,
    FdCloseFail,
    BaselineDenied,
    OmitSocketNative,
    OmitSocketX32,
    OmitSetsid,
    OmitSetpgid,
    OmitIoUring,
}

impl Fault {
    #[cfg(feature = "test-harness")]
    fn from_env() -> Fault {
        match std::env::var("O7_SC_FAULT").ok().as_deref() {
            Some("no_new_privs") => Fault::NoNewPrivs,
            Some("apply") => Fault::Apply,
            Some("fd_enum_fail") => Fault::FdEnumFail,
            Some("fd_close_fail") => Fault::FdCloseFail,
            Some("baseline_denied") => Fault::BaselineDenied,
            Some("omit_socket_native") => Fault::OmitSocketNative,
            Some("omit_socket_x32") => Fault::OmitSocketX32,
            Some("omit_setsid") => Fault::OmitSetsid,
            Some("omit_setpgid") => Fault::OmitSetpgid,
            Some("omit_iouring") => Fault::OmitIoUring,
            _ => Fault::None,
        }
    }
}

fn domain_is(value: u64) -> Result<SeccompCondition, InstallError> {
    SeccompCondition::new(0, SeccompCmpArgLen::Dword, SeccompCmpOp::Eq, value)
        .map_err(|e| InstallError::FilterBuild(e.to_string()))
}

fn socket_inet_rules() -> Result<Vec<SeccompRule>, InstallError> {
    Ok(vec![
        SeccompRule::new(vec![domain_is(libc::AF_INET as u64)?])
            .map_err(|e| InstallError::FilterBuild(e.to_string()))?,
        SeccompRule::new(vec![domain_is(libc::AF_INET6 as u64)?])
            .map_err(|e| InstallError::FilterBuild(e.to_string()))?,
    ])
}

/// Build and compile the seccomp BPF. Default Allow; deny (EPERM) socket(AF_INET|AF_INET6), the
/// io_uring syscalls, setsid, setpgid — each for BOTH the native and the x32 syscall number. x86_64
/// only (fail-closed elsewhere). `fault` omits a specific rule so a test can prove the effect-check
/// catches the gap.
fn build_bpf(fault: Fault) -> Result<BpfProgram, InstallError> {
    #[cfg(not(target_arch = "x86_64"))]
    {
        let _ = fault;
        return Err(InstallError::UnsupportedArch);
    }
    #[cfg(target_arch = "x86_64")]
    {
        let x32 = sys::X32_SYSCALL_BIT;
        let mut rules: BTreeMap<i64, Vec<SeccompRule>> = BTreeMap::new();
        if fault != Fault::OmitSocketNative {
            rules.insert(libc::SYS_socket, socket_inet_rules()?);
        }
        if fault != Fault::OmitSocketX32 {
            rules.insert(libc::SYS_socket | x32, socket_inet_rules()?);
        }
        if fault != Fault::OmitSetsid {
            rules.insert(libc::SYS_setsid, vec![]);
            rules.insert(libc::SYS_setsid | x32, vec![]);
        }
        if fault != Fault::OmitSetpgid {
            rules.insert(libc::SYS_setpgid, vec![]);
            rules.insert(libc::SYS_setpgid | x32, vec![]);
        }
        if fault != Fault::OmitIoUring {
            for nr in [
                libc::SYS_io_uring_setup,
                libc::SYS_io_uring_enter,
                libc::SYS_io_uring_register,
            ] {
                rules.insert(nr, vec![]);
                rules.insert(nr | x32, vec![]);
            }
        }
        let filter = SeccompFilter::new(
            rules,
            SeccompAction::Allow,
            SeccompAction::Errno(EPERM as u32),
            TargetArch::x86_64,
        )
        .map_err(|e| InstallError::FilterBuild(e.to_string()))?;
        let bpf: BpfProgram = filter
            .try_into()
            .map_err(|e: seccompiler::BackendError| InstallError::FilterBuild(e.to_string()))?;
        Ok(bpf)
    }
}

#[cfg(feature = "test-harness")]
fn bpf_to_bytes(bpf: &BpfProgram) -> Vec<u8> {
    let mut out = Vec::with_capacity(bpf.len() * 8);
    for insn in bpf {
        out.extend_from_slice(&insn.code.to_le_bytes());
        out.push(insn.jt);
        out.push(insn.jf);
        out.extend_from_slice(&insn.k.to_le_bytes());
    }
    out
}

/// The digest of the canonical (fault-free) compiled BPF — frozen by a test.
#[cfg(feature = "test-harness")]
pub(crate) fn compiled_bpf_digest() -> Result<Digest256, InstallError> {
    let bpf = build_bpf(Fault::None)?;
    Ok(Digest256::of_bytes(&bpf_to_bytes(&bpf)))
}

/// Post-install effect self-check for the network dimension: IPv4/IPv6 socket creation is denied
/// (EXACTLY EPERM) while AF_UNIX still succeeds. Used by the VB-4 launch child's self-check.
pub(crate) fn network_is_denied() -> bool {
    matches!(sys::probe_socket(libc::AF_INET, false), Err(e) if e == EPERM)
        && matches!(sys::probe_socket(libc::AF_INET6, false), Err(e) if e == EPERM)
        && sys::probe_socket(libc::AF_UNIX, false).is_ok()
}

/// Install the filter on THIS thread (inherited across fork/exec): `no_new_privs` then `apply_filter`.
pub(crate) fn install_seccomp(fault: Fault) -> Result<(), InstallError> {
    if fault == Fault::NoNewPrivs {
        return Err(InstallError::NoNewPrivs(EPERM));
    }
    sys::set_no_new_privs().map_err(InstallError::NoNewPrivs)?;
    let bpf = build_bpf(fault)?;
    if fault == Fault::Apply {
        return Err(InstallError::Apply("forced apply fault (test)".to_owned()));
    }
    seccompiler::apply_filter(&bpf).map_err(|e| InstallError::Apply(e.to_string()))
}

/// Close every inherited fd not in `keep`, then PROVE it by independent re-enumeration: only keep-fds
/// may survive. Fail-closed — enumeration/parse failures and any survivor are typed errors.
#[cfg(feature = "test-harness")]
fn scrub_fds(keep: &[RawFd], fault: Fault) -> Result<(), InstallError> {
    if fault == Fault::FdEnumFail {
        return Err(InstallError::FdScrub(libc::EIO));
    }
    let fds = sys::list_fds().map_err(InstallError::FdScrub)?;
    for fd in fds {
        if fd >= 0 && !keep.contains(&fd) {
            if fault == Fault::FdCloseFail {
                continue; // leave a survivor; the re-enumeration below must catch it (exit 98)
            }
            // A close that fails with anything but EBADF (already-closed) is left for the re-check.
            let _ = sys::close_fd(fd);
        }
    }
    // Independent post-scrub re-enumeration: only keep-fds may remain.
    let remaining = sys::list_fds().map_err(InstallError::FdScrub)?;
    for fd in remaining {
        if fd >= 0 && !keep.contains(&fd) {
            return Err(InstallError::FdScrubIncomplete(fd));
        }
    }
    Ok(())
}

/// Remove every env var whose name is not in `allow`.
#[cfg(feature = "test-harness")]
fn scrub_env(allow: &[OsString]) {
    let names: Vec<OsString> = std::env::vars_os().map(|(k, _)| k).collect();
    for name in names {
        if !allow.iter().any(|a| a == &name) {
            std::env::remove_var(&name);
        }
    }
}

#[cfg(feature = "test-harness")]
fn token(key: &str, res: Result<(), i32>) -> String {
    match res {
        Ok(()) => format!("{key}=OK\n"),
        Err(e) => format!("{key}=ERR:{e}\n"),
    }
}

#[cfg(feature = "test-harness")]
fn is_eperm(res: &Result<(), i32>) -> bool {
    matches!(res, Err(e) if *e == EPERM)
}

// --- harness entry ---

#[cfg(feature = "test-harness")]
struct Args {
    result: PathBuf,
    check_fd_closed: Option<RawFd>,
    env_allow: Vec<OsString>,
}

#[cfg(feature = "test-harness")]
fn parse_args() -> Option<Args> {
    let mut it = std::env::args_os().skip(2);
    let mut result = None;
    let mut check_fd_closed = None;
    let mut env_allow = Vec::new();
    while let Some(flag) = it.next() {
        match flag.to_str()? {
            "--result" => result = Some(PathBuf::from(it.next()?)),
            "--check-fd-closed" => check_fd_closed = Some(it.next()?.to_str()?.parse().ok()?),
            "--env-allow" => env_allow.push(it.next()?),
            _ => return None,
        }
    }
    Some(Args {
        result: result?,
        check_fd_closed,
        env_allow,
    })
}

/// TEST-HARNESS ENTRY (`sandboy __seccomp-run …`): fail-closed fd scrub → env snapshot/construction →
/// baseline probes (participating in the verdict) → install seccomp → differential exact-EPERM
/// self-check (incl. io_uring, x32, fork inheritance, child-side zero-inherited-sockets, byte-exact
/// env). `seccomp=enforced` is written ONLY if every effect matches; else `not_enforced` + the stage.
#[cfg(feature = "test-harness")]
pub(crate) fn harness_main() -> i32 {
    let Some(args) = parse_args() else {
        eprintln!("sandboy __seccomp-run: bad arguments");
        return 64;
    };
    let out = match File::create(&args.result) {
        Ok(f) => f,
        Err(e) => {
            eprintln!(
                "sandboy __seccomp-run: cannot open result {:?}: {e}",
                args.result
            );
            return 65;
        }
    };
    let fault = Fault::from_env();
    let mut rec = String::new();

    // 1. Fail-closed inherited-fd scrub (BEFORE seccomp and before any probe).
    let keep = [0, 1, 2, out.as_raw_fd()];
    if let Err(e) = scrub_fds(&keep, fault) {
        return finish_err(out, &mut rec, &e);
    }
    if let Some(fd) = args.check_fd_closed {
        let closed = !sys::fd_is_open(fd);
        rec.push_str(&format!("fd_planted_closed={}\n", u8::from(closed)));
        if !closed {
            return finish_err(out, &mut rec, &InstallError::FdScrubIncomplete(fd));
        }
    }

    // 2. Snapshot the EXACT expected env (inherited, filtered by allowlist) BEFORE scrubbing.
    let expected_env: BTreeMap<OsString, OsString> = std::env::vars_os()
        .filter(|(k, _)| args.env_allow.iter().any(|a| a == k))
        .collect();

    // 3. Baseline probes (BEFORE install) — captured as VALUES that participate in the verdict.
    let inet_pre = sys::probe_socket(libc::AF_INET, false);
    let inet6_pre = sys::probe_socket(libc::AF_INET6, false);
    let unix_pre = sys::probe_socket(libc::AF_UNIX, false);
    let x32_pre = sys::probe_socket(libc::AF_INET, true);
    let iouring_pre = sys::probe_io_uring_setup(false);
    let setsid_pre = sys::run_in_child(|| sys::probe_setsid().err().unwrap_or(0));
    let setpgid_pre = sys::run_in_child(|| sys::probe_setpgid().err().unwrap_or(0));
    rec.push_str(&token("inet_pre", inet_pre));
    rec.push_str(&token("inet6_pre", inet6_pre));
    rec.push_str(&token("unix_pre", unix_pre));
    rec.push_str(&token("x32_inet_pre", x32_pre));
    rec.push_str(&token("iouring_pre", iouring_pre));
    rec.push_str(&format!("setsid_pre={setsid_pre}\n"));
    rec.push_str(&format!("setpgid_pre={setpgid_pre}\n"));

    // Baseline must PERMIT what we intend to deny, or a later denial is unattributable.
    let x32_avail = x32_pre.is_ok();
    let iouring_avail = iouring_pre.is_ok();
    if fault == Fault::BaselineDenied
        || inet_pre.is_err()
        || inet6_pre.is_err()
        || unix_pre.is_err()
        || setsid_pre != 0
        || setpgid_pre != 0
    {
        return finish_err(out, &mut rec, &InstallError::BaselineDenied("a target op"));
    }

    // 4. Env construction (BEFORE install).
    scrub_env(&args.env_allow);

    // 5. Install seccomp on the launch thread.
    if let Err(e) = install_seccomp(fault) {
        return finish_err(out, &mut rec, &e);
    }

    // 6. Post-install effect self-check. setsid/setpgid in fresh children (identical to baseline).
    let inet_post = sys::probe_socket(libc::AF_INET, false);
    let inet6_post = sys::probe_socket(libc::AF_INET6, false);
    let unix_post = sys::probe_socket(libc::AF_UNIX, false);
    let x32_post = sys::probe_socket(libc::AF_INET, true);
    let iouring_post = sys::probe_io_uring_setup(false);
    let setsid_post = sys::run_in_child(|| sys::probe_setsid().err().unwrap_or(0));
    let setpgid_post = sys::run_in_child(|| sys::probe_setpgid().err().unwrap_or(0));
    let child_inet =
        sys::run_in_child(|| sys::probe_socket(libc::AF_INET, false).err().unwrap_or(0));
    let child_sockets = sys::run_in_child(|| match sys::list_fds() {
        Ok(fds) => i32::from(fds.iter().any(|&fd| sys::fd_is_socket(fd))),
        Err(_) => 2,
    });
    let env_mismatch = sys::run_in_child(move || {
        let current: BTreeMap<OsString, OsString> = std::env::vars_os().collect();
        i32::from(current != expected_env)
    });

    rec.push_str(&token("inet_post", inet_post));
    rec.push_str(&token("inet6_post", inet6_post));
    rec.push_str(&token("unix_post", unix_post));
    rec.push_str(&token("x32_inet_post", x32_post));
    rec.push_str(&token("iouring_post", iouring_post));
    rec.push_str(&format!("setsid_post={setsid_post}\n"));
    rec.push_str(&format!("setpgid_post={setpgid_post}\n"));
    rec.push_str(&format!("child_inet_post={child_inet}\n"));
    rec.push_str(&format!("child_inherited_sockets={child_sockets}\n"));
    rec.push_str(&format!("env_exact={}\n", u8::from(env_mismatch == 0)));

    // 7. EXACT differential verdict — every intended deny is EXACTLY EPERM; conditionals honor
    //    baseline availability; AF_UNIX still allowed; no inherited sockets; env byte-exact.
    let effects_ok = is_eperm(&inet_post)
        && is_eperm(&inet6_post)
        && unix_post.is_ok()
        && (if x32_avail {
            is_eperm(&x32_post)
        } else {
            x32_post.is_err()
        })
        && (if iouring_avail {
            is_eperm(&iouring_post)
        } else {
            iouring_post.is_err()
        })
        && setsid_post == EPERM
        && setpgid_post == EPERM
        && child_inet == EPERM
        && child_sockets == 0
        && env_mismatch == 0;
    if !effects_ok {
        return finish_err(
            out,
            &mut rec,
            &InstallError::EffectMismatch("post-install probe"),
        );
    }

    if let Ok(d) = compiled_bpf_digest() {
        rec.push_str(&format!("bpf_digest={d}\n"));
    }
    rec.insert_str(0, "seccomp=enforced\n");
    let mut out = out;
    let _ = out.write_all(rec.as_bytes());
    let _ = out.flush();
    0
}

#[cfg(feature = "test-harness")]
fn finish_err(mut out: File, rec: &mut String, e: &InstallError) -> i32 {
    rec.insert_str(0, &format!("seccomp=not_enforced\nstage={}\n", e.stage()));
    let _ = out.write_all(rec.as_bytes());
    let _ = out.flush();
    eprintln!("sandboy __seccomp-run: not enforced at {}: {e}", e.stage());
    e.exit_code()
}
