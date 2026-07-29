//! `sandboy` — the REAL external confinement backend for 007 (Vertical B).
//!
//! This is the separate, `unsafe`-permitted trust boundary described in
//! `docs/architecture/sandboy-boundary.md` (Decision 1) and
//! `docs/architecture/sandboy-backend-vb0.md`. Every OTHER 007 crate forbids `unsafe`; the real
//! Landlock/seccomp/cgroup enforcement lands HERE, in later Vertical-B slices.
//!
//! **VB-0 — honest fail-closed bootstrap.** It speaks the frozen protocol verbatim over the single
//! bidirectional control socket on fd 0, but installs NO kernel enforcement (none, not half). So
//! its self-check is never satisfied, it emits an HONEST report with every dimension
//! `not_enforced`, and it NEVER runs the target — a correct parent rejects the downgraded report as
//! `NotFullyEnforced` and never sends GO. The point of VB-0 is a real, wire-compatible production
//! AUTHORITY that the VB-1..VB-4 slices fill with enforcement; it does not run anything yet.
//!
//! `unsafe` inventory at VB-0: NONE. The control socket is a SAFE owned clone of stdin's
//! descriptor; framing/decoding/digests come from `o7-sandbox-protocol`; the backend digest is a
//! plain `std::fs` read of `/proc/self/exe`.

// This crate is the designated external confinement boundary. A PRODUCTION build (feature off) still
// installs NO kernel enforcement and contains NO `unsafe` — so it keeps `forbid(unsafe_code)`, and
// the VB-0 empty-unsafe-inventory guarantee is byte-for-byte intact. The `test-harness` feature
// unlocks `unsafe` for the VB-2 Landlock syscalls, which live SOLELY in `landlock::sys` (the gate
// greps to prove containment) — and nowhere in the forbid-unsafe 007 crates. VB-4 wires the confined
// `run` path in and lifts this entirely.
#![cfg_attr(not(feature = "test-harness"), forbid(unsafe_code))]

use std::ffi::OsString;
use std::io::{Read as _, Write as _};
use std::net::Shutdown;
use std::os::fd::AsFd as _;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::Duration;

use o7_sandbox_protocol::frame::encode;
use o7_sandbox_protocol::ids::{BackendIdentity, Digest256, LaunchNonce};
use o7_sandbox_protocol::policy::{Enforcement, NetworkPolicy, SandboxPolicy};
use o7_sandbox_protocol::report::SandboxReport;
use o7_sandbox_protocol::SCHEMA_VERSION;

// VB-1 cgroup monitor + its `__cgroup-run` harness entry. Compiled ONLY under `test-harness`; a
// production build has neither the module nor the subcommand (see Cargo.toml). VB-4 promotes it.
#[cfg(feature = "test-harness")]
mod cgroup;

// VB-2 Landlock filesystem confinement + its `__landlock-run` harness entry. Also `test-harness`-only
// (the single `unsafe` surface, `landlock::sys`, is compiled ONLY here). VB-4 promotes it into `run`.
#[cfg(feature = "test-harness")]
mod landlock;

/// Exit codes. Distinct per fail-closed reason so a test (and an operator) can tell WHICH gate
/// refused. All are non-zero except a clean bootstrap has no success exit — the backend only ever
/// exits 0 by relaying a target it ran, which cannot happen at VB-0.
mod exit {
    /// Could not parse the confined-backend argv.
    pub(crate) const BAD_ARGV: i32 = 64;
    /// The fd-0 control socket could not be obtained.
    pub(crate) const NO_SOCKET: i32 = 62;
    /// The launch-request frame was truncated, oversized, malformed, or a bad version.
    pub(crate) const BAD_REQUEST: i32 = 63;
    /// The report could not be self-bound (e.g. `/proc/self/exe` unreadable) or encoded.
    pub(crate) const REPORT_BUILD: i32 = 65;
    /// Writing the report frame or half-closing the socket failed.
    pub(crate) const REPORT_WRITE: i32 = 66;
    /// The parent did NOT authorize (EOF / NACK before GO): the target never runs. Expected at VB-0.
    pub(crate) const NACK: i32 = 67;
    /// GO arrived but the self-check is not satisfied (no enforcement installed): the target is
    /// STILL refused. A correct parent never reaches here at VB-0; it is a defense-in-depth refusal.
    pub(crate) const REFUSED_UNENFORCED: i32 = 71;
}

fn main() {
    std::process::exit(run());
}

fn run() -> i32 {
    // TEST-HARNESS ONLY: the VB-1 cgroup-monitor entry, exercised by the `#[ignore]`d confinement
    // tests. Never present in a production build.
    #[cfg(feature = "test-harness")]
    if std::env::args_os().nth(1).as_deref() == Some(std::ffi::OsStr::new("__cgroup-run")) {
        return cgroup::harness_main();
    }

    // TEST-HARNESS ONLY: the VB-2 Landlock filesystem entry, exercised by the `#[ignore]`d
    // confinement tests. Never present in a production build.
    #[cfg(feature = "test-harness")]
    if std::env::args_os().nth(1).as_deref() == Some(std::ffi::OsStr::new("__landlock-run")) {
        return landlock::harness_main();
    }

    let Some(args) = parse_args() else {
        eprintln!("sandboy: could not parse the confined-backend argv");
        return exit::BAD_ARGV;
    };

    // The single bidirectional control transport is the socket on fd 0 (the parent mapped a
    // `UnixStream` end onto our stdin). A SAFE owned clone — no `from_raw_fd`.
    let Some(mut sock) = control_socket() else {
        eprintln!("sandboy: could not acquire the fd-0 control socket");
        return exit::NO_SOCKET;
    };

    // Read exactly ONE length-prefixed launch-request frame, bounded, and decode it. A truncated,
    // oversized, malformed, duplicate-env, or wrong-version request FAILS CLOSED: no report, no
    // target.
    let Some(request_frame) =
        read_frame(&mut sock, o7_sandbox_protocol::request::MAX_REQUEST_BYTES)
    else {
        eprintln!("sandboy: could not read a bounded launch-request frame");
        return exit::BAD_REQUEST;
    };
    let request = match o7_sandbox_protocol::request::decode(&request_frame) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("sandboy: rejecting malformed launch request: {e}");
            return exit::BAD_REQUEST;
        }
    };

    // Reconstruct the exact policy the parent digested from the argv, and bind the report to this
    // policy / launch / backend object.
    let policy = reconstructed_policy(&args);
    let policy_digest = policy.digest();
    let launch_spec_digest = request.spec_digest();
    let Ok(nonce) = LaunchNonce::parse(&args.nonce) else {
        eprintln!("sandboy: launch nonce is not a valid 128-bit hex nonce");
        return exit::REPORT_BUILD;
    };
    // The backend digest is the digest of the SEALED object we were launched from
    // (`/proc/self/exe` resolves to that sealed memfd). If we cannot read our own image we cannot
    // honestly self-bind the report — fail closed rather than emit a mis-bound report.
    let backend_digest = match std::fs::read("/proc/self/exe") {
        Ok(bytes) => Digest256::of_bytes(&bytes),
        Err(e) => {
            eprintln!("sandboy: cannot read /proc/self/exe to self-bind the report: {e}");
            return exit::REPORT_BUILD;
        }
    };
    let identity = match BackendIdentity::new("sandboy-linux", env!("CARGO_PKG_VERSION")) {
        Ok(id) => id,
        Err(_) => {
            eprintln!("sandboy: static backend identity is invalid");
            return exit::REPORT_BUILD;
        }
    };

    // Install confinement and SELF-CHECK before anything could run. VB-0: the installer is an
    // honest no-op — it installs nothing, so the self-check reports every dimension `not_enforced`.
    // Later slices fill `install_confinement` and flip dimensions to `enforced` only after a real
    // self-check. Because this happens BEFORE any target could start, a downgrade can never mean
    // "the target already ran unconfined".
    let outcome = install_confinement(&policy, &request);

    let report = SandboxReport {
        schema_version: SCHEMA_VERSION,
        backend: identity,
        backend_digest,
        policy_digest,
        launch_nonce: nonce,
        launch_spec_digest,
        filesystem: outcome.filesystem,
        network: outcome.network,
        env: outcome.env,
        process_tree: outcome.process_tree,
        timeout: outcome.timeout,
    };
    let frame = match encode(&report) {
        Ok(bytes) => bytes,
        Err(e) => {
            eprintln!("sandboy: could not encode the report frame: {e}");
            return exit::REPORT_BUILD;
        }
    };

    // Emit exactly ONE report frame, then HALF-CLOSE the write side so the parent can prove the
    // frame is the entire message by reading EOF. The read side stays open for GO.
    if sock.write_all(&frame).is_err() || sock.flush().is_err() {
        eprintln!("sandboy: could not write the report frame");
        return exit::REPORT_WRITE;
    }
    if sock.shutdown(Shutdown::Write).is_err() {
        eprintln!("sandboy: could not half-close the control socket");
        return exit::REPORT_WRITE;
    }

    // Parent authorization barrier: read the GO byte ('G'). EOF or anything else is a NACK.
    let mut go = [0u8; 1];
    let authorized = matches!(sock.read_exact(&mut go), Ok(())) && go[0] == b'G';
    if !authorized {
        // The common, correct VB-0 path: the parent verified the honest report, saw a downgraded
        // dimension, and dropped its end (NACK / EOF). The target never runs.
        return exit::NACK;
    }

    // GO arrived. VB-0 self-check is NEVER satisfied (no enforcement installed), so we REFUSE to
    // launch the target rather than run it unconfined — a downgraded self-check must never become a
    // live unconfined run. A correct parent never sends GO on a downgraded report; this is
    // defense in depth. The on-GO monitor→target spawn is added by the slice that first achieves
    // full enforcement (VB-4).
    if !outcome.fully_enforced() {
        eprintln!(
            "sandboy: refusing to launch — confinement is not established (VB-0 installs none)"
        );
        return exit::REFUSED_UNENFORCED;
    }
    // Unreachable at VB-0 (`fully_enforced()` is always false); the real launch lands in VB-4.
    exit::REFUSED_UNENFORCED
}

/// The self-check outcome per dimension. VB-0 produces all-`not_enforced`; later slices flip a
/// dimension to `enforced` only after installing AND self-checking it.
struct EnforcementOutcome {
    filesystem: Enforcement,
    network: Enforcement,
    env: Enforcement,
    process_tree: Enforcement,
    timeout: Enforcement,
}

impl EnforcementOutcome {
    /// Every dimension `enforced`. VB-0 is never fully enforced.
    fn fully_enforced(&self) -> bool {
        matches!(self.filesystem, Enforcement::Enforced)
            && matches!(self.network, Enforcement::Enforced)
            && matches!(self.env, Enforcement::Enforced)
            && matches!(self.process_tree, Enforcement::Enforced)
            && matches!(self.timeout, Enforcement::Enforced)
    }
}

/// Install confinement and self-check it. **VB-0: an HONEST no-op.** It installs nothing and
/// therefore self-checks every dimension as `not_enforced`. VB-1 (cgroup+timeout), VB-2 (Landlock),
/// and VB-3 (seccomp+env) fill this in, flipping a dimension to `enforced` ONLY after a real
/// kernel-side self-check. Its inputs are named now so the signature does not churn later.
fn install_confinement(
    _policy: &SandboxPolicy,
    _request: &o7_sandbox_protocol::LaunchRequest,
) -> EnforcementOutcome {
    EnforcementOutcome {
        filesystem: Enforcement::NotEnforced,
        network: Enforcement::NotEnforced,
        env: Enforcement::NotEnforced,
        process_tree: Enforcement::NotEnforced,
        timeout: Enforcement::NotEnforced,
    }
}

/// The confined-backend argv this backend understands. The control transport is NOT on the argv
/// (it is the fd-0 socket); the target's argv/cwd/env travel out-of-band in the request. Only the
/// non-sensitive sealed target path follows `--`.
struct Args {
    nonce: String,
    worktree: PathBuf,
    timeout_ms: u128,
    allow_exec: Vec<PathBuf>,
    allow_env: Vec<OsString>,
    /// Retained for the VB-4 launch (the sealed target descriptor to exec); unused at VB-0.
    _target: PathBuf,
}

fn parse_args() -> Option<Args> {
    let mut it = std::env::args_os().skip(1); // skip program name
    if it.next()?.to_str()? != "run" {
        return None;
    }
    let mut nonce = None;
    let mut worktree = None;
    let mut timeout_ms = None;
    let mut allow_exec = Vec::new();
    let mut allow_env = Vec::new();
    loop {
        let flag = it.next()?;
        match flag.to_str()? {
            "--launch-nonce" => nonce = Some(it.next()?.to_str()?.to_owned()),
            "--deny-net" => {}
            "--worktree" => worktree = Some(PathBuf::from(it.next()?)),
            "--timeout-ms" => timeout_ms = Some(it.next()?.to_str()?.parse().ok()?),
            "--allow-exec" => allow_exec.push(PathBuf::from(it.next()?)),
            "--allow-env" => allow_env.push(it.next()?),
            "--" => break,
            _ => return None,
        }
    }
    let target = PathBuf::from(it.next()?);
    Some(Args {
        nonce: nonce?,
        worktree: worktree?,
        timeout_ms: timeout_ms?,
        allow_exec,
        allow_env,
        _target: target,
    })
}

/// The fd-0 control socket: a SAFE owned clone of stdin's descriptor (the parent mapped a
/// `UnixStream` end onto our stdin). No `from_raw_fd` — `try_clone_to_owned` dups it for us.
fn control_socket() -> Option<UnixStream> {
    let owned = std::io::stdin().as_fd().try_clone_to_owned().ok()?;
    Some(UnixStream::from(owned))
}

/// Read exactly one length-prefixed frame (4-byte big-endian length + body) from `sock`, bounded by
/// `max`. Returns the full frame (prefix + body) for the protocol decoder, or `None` on any I/O
/// error or an over-`max` declared length.
fn read_frame(sock: &mut UnixStream, max: usize) -> Option<Vec<u8>> {
    let mut len_buf = [0u8; 4];
    sock.read_exact(&mut len_buf).ok()?;
    let len = u32::from_be_bytes(len_buf) as usize;
    if len > max {
        return None;
    }
    let mut body = vec![0u8; len];
    sock.read_exact(&mut body).ok()?;
    let mut frame = len_buf.to_vec();
    frame.extend_from_slice(&body);
    Some(frame)
}

/// Reconstruct the exact `SandboxPolicy` the parent digested, from the argv flags. The parent
/// serializes whole-millisecond timeouts (`--timeout-ms`), so a clamp to `u64` millis reproduces
/// the same `Duration` the parent hashed (its `validate()` rejects sub-millisecond precision).
fn reconstructed_policy(args: &Args) -> SandboxPolicy {
    SandboxPolicy {
        worktree: args.worktree.clone(),
        allow_exec: args.allow_exec.clone(),
        network: NetworkPolicy::DenyAll,
        env_allowlist: args.allow_env.clone(),
        timeout: Duration::from_millis(args.timeout_ms.min(u128::from(u64::MAX)) as u64),
    }
}
