//! A CONTROLLED fake `sandboy` backend for the launch acceptance matrix. It speaks the real
//! protocol AND respects the chosen MONITOR topology: reconstruct the policy from the flags,
//! emit ONE bound report frame on `--report-fd`, wait for the parent's GO byte on
//! `--control-fd`, then — on GO — CLOSE the inherited control-plane descriptors and start the
//! target as a CHILD in its own process group, remaining alive as the monitor that owns the
//! group and relays the target's exit. It never `exec`s into the target (which would make it
//! disappear, the rejected exec-in-place architecture). It misbehaves on demand via its own
//! (trusted, control-plane) environment variable `O7_FAKE_MODE` — never the target's env.
//!
//! Test infrastructure: built by the o7-worker package so integration tests can locate it via
//! `CARGO_BIN_EXE_sandboy_fake`, and exercised once the live launch lands GREEN. `unsafe`
//! stays forbidden (fd access via `/proc/self/fd/<n>`; fd close via nix; no `exec`).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::print_stderr
)]

use std::ffi::OsString;
use std::io::{Read as _, Write as _};
use std::os::unix::process::CommandExt as _;
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use o7_sandbox_protocol::frame::encode;
use o7_sandbox_protocol::ids::{BackendIdentity, Digest256, LaunchNonce};
use o7_sandbox_protocol::policy::{Enforcement, NetworkPolicy, SandboxPolicy};
use o7_sandbox_protocol::report::SandboxReport;
use o7_sandbox_protocol::SCHEMA_VERSION;

/// A minimal parse of the argv this backend understands. Sensitive target values (env) do NOT
/// ride the argv (visible via `/proc`); only the non-sensitive target path/args/cwd do.
struct Args {
    report_fd: i32,
    control_fd: i32,
    request_fd: i32,
    nonce: String,
    worktree: PathBuf,
    timeout_ms: u128,
    allow_exec: Vec<PathBuf>,
    allow_env: Vec<OsString>,
    target: PathBuf,
}

fn parse_args() -> Option<Args> {
    let mut it = std::env::args_os().skip(1); // skip program name
    if it.next()?.to_str()? != "run" {
        return None;
    }
    let mut report_fd = None;
    let mut control_fd = None;
    let mut request_fd = None;
    let mut nonce = None;
    let mut worktree = None;
    let mut timeout_ms = None;
    let mut allow_exec = Vec::new();
    let mut allow_env = Vec::new();
    loop {
        let flag = it.next()?;
        let flag = flag.to_str()?;
        match flag {
            "--report-fd" => report_fd = Some(it.next()?.to_str()?.parse().ok()?),
            "--control-fd" => control_fd = Some(it.next()?.to_str()?.parse().ok()?),
            "--request-fd" => request_fd = Some(it.next()?.to_str()?.parse().ok()?),
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
    // Only the target EXECUTABLE follows the separator; argv/cwd/env come from the request.
    let target = PathBuf::from(it.next()?);
    Some(Args {
        report_fd: report_fd?,
        control_fd: control_fd?,
        request_fd: request_fd?,
        nonce: nonce?,
        worktree: worktree?,
        timeout_ms: timeout_ms?,
        allow_exec,
        allow_env,
        target,
    })
}

fn reconstructed_policy(args: &Args) -> SandboxPolicy {
    SandboxPolicy {
        worktree: args.worktree.clone(),
        allow_exec: args.allow_exec.clone(),
        network: NetworkPolicy::DenyAll,
        env_allowlist: args.allow_env.clone(),
        timeout: Duration::from_millis(args.timeout_ms.min(u128::from(u64::MAX)) as u64),
    }
}

fn main() {
    std::process::exit(run());
}

fn run() -> i32 {
    let Some(args) = parse_args() else {
        eprintln!("sandboy_fake: could not parse argv");
        return 64;
    };
    // The mode comes from the backend's OWN (trusted, control-plane) environment — never the
    // target's.
    let mode = std::env::var("O7_FAKE_MODE").unwrap_or_default();

    if mode == "exit_before_report" {
        return 70; // die before delivering anything
    }

    // Read the out-of-band launch request (target argv/cwd/env/stdin + target_digest).
    let request_path = format!("/proc/self/fd/{}", args.request_fd);
    let request_bytes = std::fs::read(&request_path).unwrap_or_default();
    let request = match o7_sandbox_protocol::request::decode(&request_bytes) {
        Ok(r) => r,
        Err(_) => return 63,
    };

    let policy = reconstructed_policy(&args);
    let mut policy_digest = policy.digest();
    // The report echoes the request's launch-spec digest (the exact invocation the parent bound)
    // and the digest of THIS backend object (read back from /proc/self/exe — the sealed memfd we
    // were launched from).
    let mut launch_spec_digest = request.spec_digest();
    let mut backend_digest =
        Digest256::of_bytes(&std::fs::read("/proc/self/exe").unwrap_or_default());
    let mut nonce = args.nonce.clone();
    let mut backend = BackendIdentity::new("sandboy-linux", "0.1.0")
        .unwrap_or_else(|_| BackendIdentity::new("x", "x").expect("static identity"));
    let mut dims = [Enforcement::Enforced; 5];

    match mode.as_str() {
        "wrong_policy" => policy_digest = Digest256::of_bytes(b"a different policy"),
        "wrong_target" => launch_spec_digest = Digest256::of_bytes(b"a different launch"),
        "wrong_nonce" => nonce = LaunchNonce::from_bytes([0xff; 16]).as_str().to_owned(),
        "wrong_backend" => {
            backend = BackendIdentity::new("trust-me", "9.9").expect("static identity");
        }
        "wrong_backend_digest" => backend_digest = Digest256::of_bytes(b"a different backend"),
        "partial" => dims[1] = Enforcement::Partial,
        _ => {}
    }

    // Emit the report frame on the report descriptor (or malformed bytes on demand).
    let report_path = format!("/proc/self/fd/{}", args.report_fd);
    let frame: Vec<u8> = if mode == "malformed" {
        b"this is not a valid length-prefixed frame".to_vec()
    } else {
        let Ok(nonce) = LaunchNonce::parse(&nonce) else {
            return 65;
        };
        let report = SandboxReport {
            schema_version: SCHEMA_VERSION,
            backend,
            backend_digest,
            policy_digest,
            launch_nonce: nonce,
            launch_spec_digest,
            filesystem: dims[0],
            network: dims[1],
            env: dims[2],
            process_tree: dims[3],
            timeout: dims[4],
        };
        match encode(&report) {
            Ok(bytes) => bytes,
            Err(_) => return 65,
        }
    };
    match std::fs::OpenOptions::new().write(true).open(&report_path) {
        Ok(mut w) => {
            if w.write_all(&frame).is_err() {
                return 66;
            }
        }
        Err(_) => return 66,
    }

    // Wait for the parent's GO ('G') before starting the target. Anything else / EOF is a NACK.
    let control_path = format!("/proc/self/fd/{}", args.control_fd);
    let mut go = [0u8; 1];
    match std::fs::File::open(&control_path).and_then(|mut f| f.read_exact(&mut go)) {
        Ok(()) if go[0] == b'G' => {}
        _ => return 67, // NACK / EOF — fail closed, target never runs.
    }

    // Close the inherited control-plane descriptors so the TARGET cannot see the report/control
    // channels (a security contract, not a detail). Errors are ignored: a closed fd is fine.
    let _ = nix::unistd::close(args.report_fd);
    let _ = nix::unistd::close(args.control_fd);

    // MONITOR topology: start the target as a CHILD in its own process group and stay alive as
    // the monitor. The target's argv/cwd/env come from the out-of-band REQUEST (allowlisted),
    // never from the backend's own argv/env. `env_clear()` first, then set exactly the
    // request's allowlisted env.
    use std::os::unix::ffi::OsStrExt as _;
    let mut cmd = Command::new(&args.target);
    cmd.args(
        request
            .argv
            .iter()
            .map(|a| OsString::from(std::ffi::OsStr::from_bytes(a))),
    )
    .current_dir(PathBuf::from(std::ffi::OsStr::from_bytes(&request.cwd)))
    .env_clear()
    .process_group(0);
    for entry in &request.env {
        cmd.env(
            std::ffi::OsStr::from_bytes(&entry.name),
            std::ffi::OsStr::from_bytes(&entry.value),
        );
    }
    let mut child = match cmd.spawn() {
        Ok(child) => child,
        Err(e) => {
            eprintln!("sandboy_fake: could not start target: {e}");
            return 68;
        }
    };
    // Remain as the monitor until the target exits, then relay its outcome. (A real backend
    // would also enforce the wall-clock timeout and tear the cgroup down here.)
    match child.wait() {
        Ok(status) => status.code().unwrap_or(1),
        Err(_) => 1,
    }
}
