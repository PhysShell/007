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

// This crate is the designated external confinement boundary. VB-4 makes the Landlock/seccomp/cgroup
// confinement the PRODUCTION launch path, so `forbid(unsafe_code)` is lifted here — but the `unsafe`
// surface stays CONTAINED to the two audited syscall modules `landlock/sys.rs` + `seccomp/sys.rs`
// (the gate greps every other file to prove it). Every OTHER 007 crate remains forbid-unsafe.
#![deny(unsafe_op_in_unsafe_fn)]

use std::ffi::OsString;
use std::io::Read as _;
use std::os::fd::AsFd as _;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::Duration;

use o7_sandbox_protocol::ids::{BackendIdentity, Digest256, LaunchNonce};
use o7_sandbox_protocol::policy::{NetworkPolicy, SandboxPolicy};

// The three confinement mechanisms — now PRODUCTION code composed by the VB-4 launch path (`launch`).
// Their standalone `__*-run` harness entries (VB-1/2/3 acceptance drivers) are `test-harness`-only and
// call the SAME production install functions. The only `unsafe` lives in `landlock/sys.rs` +
// `seccomp/sys.rs`.
mod cgroup;
mod landlock;
mod launch;
mod runtime;
mod seccomp;
mod staging;

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
    /// The report could not be self-bound (e.g. `/proc/self/exe` unreadable).
    pub(crate) const REPORT_BUILD: i32 = 65;
    // The report/GO/launch exit codes now live in `launch::exit` (the state machine owns them).
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

    // TEST-HARNESS ONLY: the VB-3 seccomp entry, exercised by the `#[ignore]`d confinement tests.
    #[cfg(feature = "test-harness")]
    if std::env::args_os().nth(1).as_deref() == Some(std::ffi::OsStr::new("__seccomp-run")) {
        return seccomp::harness_main();
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
    // The production identity. A `fault-injection` build is a DISTINCT artifact (the compiled fault
    // seam) and MUST carry a distinct identity so it can never be bound as the production backend: the
    // GREEN oracles bind the production digest+identity, so a fault artifact (different digest AND
    // version) is rejected on both. The frozen setup-failure oracles bind THIS fault identity.
    #[cfg(not(feature = "fault-injection"))]
    const BACKEND_VERSION: &str = env!("CARGO_PKG_VERSION");
    #[cfg(feature = "fault-injection")]
    const BACKEND_VERSION: &str = concat!(env!("CARGO_PKG_VERSION"), "+faultinject");
    let identity = match BackendIdentity::new("sandboy-linux", BACKEND_VERSION) {
        Ok(id) => id,
        Err(_) => {
            eprintln!("sandboy: static backend identity is invalid");
            return exit::REPORT_BUILD;
        }
    };

    // VB-4: hand off to the confined-launch state machine. It composes the cgroup leaf + Landlock +
    // fd-scrub + env + seccomp in ONE launch child, self-checks all five dimensions, publishes the
    // report built ONLY from the verified plan, runs the GO/release/supervise sequence, and returns
    // the exit code. The report/GO/half-close all happen inside, so a dimension can never be flipped
    // to `Enforced` by an independent field assignment, and the target never runs unconfined.
    let bindings = launch::Bindings {
        identity,
        backend_digest,
        policy_digest,
        launch_nonce: nonce,
        launch_spec_digest,
        target_digest: request.target_digest.clone(),
    };
    let faults = launch::monitor_faults();
    launch::run_confined_launch(
        &mut sock,
        &policy,
        &request,
        &args.target,
        bindings,
        &faults,
    )
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
    /// The sealed target descriptor (`/proc/<worker>/fd/<n>`) the launch child execveat's.
    target: PathBuf,
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
        target,
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
