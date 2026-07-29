//! VB-3 — real **seccomp** confinement (network deny + `setsid`/`setpgid` deny), an inherited-fd
//! scrub, and an exact env allowlist, all PROVEN by effect-based self-checks with typed fail-closed
//! verdicts. Scope, strictly: build a typed seccomp policy with `seccompiler`, compile it to BPF,
//! scrub inherited fds, construct the env, install the filter on the launch thread, and observe the
//! effect. **No Landlock/cgroup change; no `confinement_backend()` switch; no RED-matrix flip; no
//! VB-4 integration.**
//!
//! The filter: default **Allow**; deny (`Errno(EPERM)`) `socket` ONLY when arg0 ∈ {AF_INET, AF_INET6}
//! (so AF_UNIX stays allowed — family-scoped, not a blanket socket ban), and `setsid`/`setpgid`
//! unconditionally. `seccompiler`'s arch gate already KILLs on `AUDIT_ARCH` mismatch, but x32 reports
//! the same audit arch and uses `nr | X32_SYSCALL_BIT`; since rules match exact numbers, the deny
//! rules are ALSO installed for the x32 syscall numbers, and an adversarial oracle proves the x32
//! namespace cannot bypass them. A digest of the compiled BPF is frozen in a test so a dependency or
//! compiler change cannot alter the policy unnoticed.
//!
//! Compiled ONLY under `test-harness` (like VB-1/VB-2): production `run` compiles NEITHER this module
//! NOR `seccompiler`/`libc` NOR any `unsafe`. A TEST-ONLY `O7_SC_FAULT` knob forces install failure.

mod sys;

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs::File;
use std::io::{self, Write as _};
use std::os::fd::{AsRawFd as _, RawFd};
use std::path::PathBuf;

use o7_sandbox_protocol::ids::Digest256;
use seccompiler::{
    BpfProgram, SeccompAction, SeccompCmpArgLen, SeccompCmpOp, SeccompCondition, SeccompFilter,
    SeccompRule, TargetArch,
};

/// Why the seccomp policy is not a proven, fully-enforced boundary. Typed, distinct stage + exit code.
#[derive(Debug)]
pub(crate) enum InstallError {
    /// Not x86_64 — the x32 guarantee is expressed for x86_64 only; fail the arch gate, never Allow.
    /// Constructed only on non-x86_64 targets (the x86_64 build never reaches it).
    #[allow(dead_code)]
    UnsupportedArch,
    /// `seccompiler` rejected the typed policy or its compilation to BPF.
    FilterBuild(String),
    NoNewPrivs(i32),
    /// `seccompiler::apply_filter` failed.
    Apply(String),
    FdScrub(io::Error),
    /// The inherited-fd scrub did not close a fd it was required to close.
    FdScrubIncomplete(RawFd),
    /// A post-install effect did not match the policy (the filter did not really take).
    EffectMismatch(&'static str),
}

impl InstallError {
    pub(crate) fn stage(&self) -> &'static str {
        match self {
            InstallError::UnsupportedArch => "unsupported_arch",
            InstallError::FilterBuild(_) => "filter_build",
            InstallError::NoNewPrivs(_) => "no_new_privs",
            InstallError::Apply(_) => "apply",
            InstallError::FdScrub(_) => "fd_scrub",
            InstallError::FdScrubIncomplete(_) => "fd_scrub_incomplete",
            InstallError::EffectMismatch(_) => "effect_mismatch",
        }
    }
    pub(crate) fn exit_code(&self) -> i32 {
        match self {
            InstallError::UnsupportedArch => 93,
            InstallError::FilterBuild(_) => 94,
            InstallError::NoNewPrivs(_) => 95,
            InstallError::Apply(_) => 96,
            InstallError::FdScrub(_) => 97,
            InstallError::FdScrubIncomplete(_) => 98,
            InstallError::EffectMismatch(_) => 99,
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
            InstallError::FdScrub(e) => write!(f, "fd scrub: {e}"),
            InstallError::FdScrubIncomplete(fd) => write!(f, "fd scrub left fd {fd} open"),
            InstallError::EffectMismatch(w) => write!(f, "post-install effect mismatch: {w}"),
        }
    }
}

/// One `arg0 == value` condition (the socket domain is a 32-bit int).
fn domain_is(value: u64) -> Result<SeccompCondition, InstallError> {
    SeccompCondition::new(0, SeccompCmpArgLen::Dword, SeccompCmpOp::Eq, value)
        .map_err(|e| InstallError::FilterBuild(e.to_string()))
}

fn rule(conditions: Vec<SeccompCondition>) -> Result<SeccompRule, InstallError> {
    SeccompRule::new(conditions).map_err(|e| InstallError::FilterBuild(e.to_string()))
}

/// The socket-domain deny chain: AF_INET OR AF_INET6 (two rules; AF_UNIX matches neither → Allow).
fn socket_inet_rules() -> Result<Vec<SeccompRule>, InstallError> {
    Ok(vec![
        rule(vec![domain_is(libc::AF_INET as u64)?])?,
        rule(vec![domain_is(libc::AF_INET6 as u64)?])?,
    ])
}

/// Build and compile the seccomp BPF: default Allow; deny socket(AF_INET|AF_INET6), setsid, setpgid —
/// for BOTH the native and the x32 syscall numbers. x86_64 only (fail-closed elsewhere).
fn build_bpf() -> Result<BpfProgram, InstallError> {
    #[cfg(not(target_arch = "x86_64"))]
    {
        return Err(InstallError::UnsupportedArch);
    }
    #[cfg(target_arch = "x86_64")]
    {
        let x32 = sys::X32_SYSCALL_BIT;
        let mut rules: BTreeMap<i64, Vec<SeccompRule>> = BTreeMap::new();
        // socket(AF_INET|AF_INET6) — native + x32. Each needs its own freshly-built rule vector.
        rules.insert(libc::SYS_socket, socket_inet_rules()?);
        rules.insert(libc::SYS_socket | x32, socket_inet_rules()?);
        // setsid / setpgid — unconditional (empty rule vector), native + x32.
        rules.insert(libc::SYS_setsid, vec![]);
        rules.insert(libc::SYS_setsid | x32, vec![]);
        rules.insert(libc::SYS_setpgid, vec![]);
        rules.insert(libc::SYS_setpgid | x32, vec![]);

        let filter = SeccompFilter::new(
            rules,
            SeccompAction::Allow, // default: everything not denied
            SeccompAction::Errno(libc::EPERM as u32), // matched deny action
            TargetArch::x86_64,
        )
        .map_err(|e| InstallError::FilterBuild(e.to_string()))?;

        let bpf: BpfProgram = filter
            .try_into()
            .map_err(|e: seccompiler::BackendError| InstallError::FilterBuild(e.to_string()))?;
        Ok(bpf)
    }
}

/// Serialize the compiled BPF to bytes (`sock_filter` is `{code:u16, jt:u8, jf:u8, k:u32}`), for a
/// stable digest that freezes the exact policy instruction sequence.
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

/// A digest of the compiled BPF instruction sequence — frozen by a test so a dependency/compiler
/// change that alters the policy is caught. Also usable as evidence in the run result.
pub(crate) fn compiled_bpf_digest() -> Result<Digest256, InstallError> {
    let bpf = build_bpf()?;
    Ok(Digest256::of_bytes(&bpf_to_bytes(&bpf)))
}

/// TEST-ONLY forced-fault knob.
#[derive(Debug, Clone, Copy, Default)]
struct Faults {
    apply: bool,
}

impl Faults {
    fn from_env() -> Faults {
        Faults {
            apply: std::env::var("O7_SC_FAULT").ok().as_deref() == Some("apply"),
        }
    }
}

/// Install the seccomp filter on THIS thread (inherited across fork/exec): `no_new_privs` then
/// `apply_filter`. Fail-closed and typed.
fn install_seccomp(faults: Faults) -> Result<(), InstallError> {
    sys::set_no_new_privs().map_err(InstallError::NoNewPrivs)?;
    let bpf = build_bpf()?;
    if faults.apply {
        return Err(InstallError::Apply("forced apply fault (test)".to_owned()));
    }
    seccompiler::apply_filter(&bpf).map_err(|e| InstallError::Apply(e.to_string()))
}

/// Close every inherited fd not in `keep` (a leaked non-CLOEXEC fd is closed here, BEFORE seccomp and
/// before any target runs). Reads the fd list first, then closes, so the directory fd is not disturbed
/// mid-iteration.
fn scrub_fds(keep: &[RawFd]) -> Result<(), InstallError> {
    let fds: Vec<RawFd> = std::fs::read_dir("/proc/self/fd")
        .map_err(InstallError::FdScrub)?
        .filter_map(|e| e.ok())
        .filter_map(|e| e.file_name().to_str().and_then(|s| s.parse::<RawFd>().ok()))
        .collect();
    for fd in fds {
        if fd >= 0 && !keep.contains(&fd) {
            sys::close_fd(fd);
        }
    }
    Ok(())
}

/// Construct the child environment: remove every variable whose name is not in `allow`.
fn scrub_env(allow: &[OsString]) {
    let names: Vec<OsString> = std::env::vars_os().map(|(k, _)| k).collect();
    for name in names {
        if !allow.iter().any(|a| a == &name) {
            std::env::remove_var(&name);
        }
    }
}

fn errno_token(key: &str, res: Result<(), i32>) -> String {
    match res {
        Ok(()) => format!("{key}=OK\n"),
        Err(e) => format!("{key}=ERR:{e}\n"),
    }
}

// --- harness entry ---

struct Args {
    result: PathBuf,
    check_fd_closed: Option<RawFd>,
    env_allow: Vec<OsString>,
}

fn parse_args() -> Option<Args> {
    let mut it = std::env::args_os().skip(2); // program + "__seccomp-run"
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

/// TEST-HARNESS ENTRY (`sandboy __seccomp-run …`): scrub fds → construct env → baseline probes →
/// install seccomp → post-install effect self-check (incl. x32 + fork inheritance) → record. On any
/// install/effect failure, `seccomp=not_enforced` + the stage; the confinement is never falsely
/// claimed.
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
    let faults = Faults::from_env();
    let mut rec = String::new();

    // 1. Inherited-fd scrub — BEFORE seccomp and before any probe. Keep stdio + the result fd.
    let out_fd = out.as_raw_fd();
    let keep = [0, 1, 2, out_fd];
    if let Err(e) = scrub_fds(&keep) {
        return finish_err(out, &mut rec, &e);
    }
    // The planted non-CLOEXEC fd (if any) MUST now be closed.
    if let Some(fd) = args.check_fd_closed {
        let closed = !sys::fd_is_open(fd);
        rec.push_str(&format!("fd_planted_closed={}\n", u8::from(closed)));
        if !closed {
            return finish_err(out, &mut rec, &InstallError::FdScrubIncomplete(fd));
        }
    }

    // 2. Baseline probes (BEFORE install): the exact ops must be permitted unconfined. setsid/setpgid
    //    are run in disposable children (real session/pgroup side effects).
    rec.push_str(&errno_token(
        "inet_pre",
        sys::probe_socket(libc::AF_INET, false),
    ));
    rec.push_str(&errno_token(
        "inet6_pre",
        sys::probe_socket(libc::AF_INET6, false),
    ));
    rec.push_str(&errno_token(
        "unix_pre",
        sys::probe_socket(libc::AF_UNIX, false),
    ));
    rec.push_str(&format!(
        "setsid_pre={}\n",
        sys::run_in_child(|| sys::probe_setsid().err().unwrap_or(0))
    ));
    rec.push_str(&format!(
        "setpgid_pre={}\n",
        sys::run_in_child(|| sys::probe_setpgid().err().unwrap_or(0))
    ));

    // 3. Env construction — BEFORE install.
    scrub_env(&args.env_allow);

    // 4. Install seccomp on this (launch) thread.
    if let Err(e) = install_seccomp(faults) {
        return finish_err(out, &mut rec, &e);
    }

    // 5. Post-install effect self-check. socket probes are side-effect-free (direct); setsid/setpgid
    //    are denied by seccomp BEFORE the kernel's group-leader check, so direct calls are unambiguous.
    let inet_post = sys::probe_socket(libc::AF_INET, false);
    let inet6_post = sys::probe_socket(libc::AF_INET6, false);
    let unix_post = sys::probe_socket(libc::AF_UNIX, false);
    let setsid_post = sys::probe_setsid();
    let setpgid_post = sys::probe_setpgid();
    // x32 namespace must NOT bypass the socket deny.
    let x32_inet_post = sys::probe_socket(libc::AF_INET, true);
    // Filter is inherited across fork: a child must see the same deny.
    let child_inet =
        sys::run_in_child(|| sys::probe_socket(libc::AF_INET, false).err().unwrap_or(0));

    rec.push_str(&errno_token("inet_post", inet_post));
    rec.push_str(&errno_token("inet6_post", inet6_post));
    rec.push_str(&errno_token("unix_post", unix_post));
    rec.push_str(&errno_token("setsid_post", setsid_post));
    rec.push_str(&errno_token("setpgid_post", setpgid_post));
    rec.push_str(&errno_token("x32_inet_post", x32_inet_post));
    rec.push_str(&format!("child_inet_post={child_inet}\n"));

    // 6. Env effect: a child sees ONLY the allowlisted names.
    let allow_snapshot = args.env_allow.clone();
    let leaked = sys::run_in_child(move || {
        let ok = std::env::vars_os().all(|(k, _)| allow_snapshot.iter().any(|a| a == &k));
        i32::from(!ok) // 0 = only allowlisted, 1 = a non-allowlisted var leaked
    });
    rec.push_str(&format!("env_only_allowlisted={}\n", u8::from(leaked == 0)));

    if let Ok(d) = compiled_bpf_digest() {
        rec.push_str(&format!("bpf_digest={d}\n"));
    }

    // 7. Typed self-verdict: enforced only if every effect matches the policy.
    let denied = |r: &Result<(), i32>| matches!(r, Err(libc::EPERM) | Err(libc::EACCES));
    let effects_ok = denied(&inet_post)
        && denied(&inet6_post)
        && unix_post.is_ok()
        && denied(&setsid_post)
        && denied(&setpgid_post)
        && (x32_inet_post.is_err()) // x32 must NOT create a socket (EPERM, or ENOSYS if x32 absent)
        && child_inet == libc::EPERM
        && leaked == 0;
    if !effects_ok {
        return finish_err(
            out,
            &mut rec,
            &InstallError::EffectMismatch("post-install probe"),
        );
    }
    rec.insert_str(0, "seccomp=enforced\n");
    let mut out = out;
    let _ = out.write_all(rec.as_bytes());
    let _ = out.flush();
    0
}

/// Record `seccomp=not_enforced` + the stage and return the typed exit code.
fn finish_err(mut out: File, rec: &mut String, e: &InstallError) -> i32 {
    rec.insert_str(0, &format!("seccomp=not_enforced\nstage={}\n", e.stage()));
    let _ = out.write_all(rec.as_bytes());
    let _ = out.flush();
    eprintln!("sandboy __seccomp-run: not enforced at {}: {e}", e.stage());
    e.exit_code()
}
