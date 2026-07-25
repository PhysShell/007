//! A CONTROLLED fake `sandboy` backend for the launch acceptance matrix. It speaks the real
//! protocol — reconstruct the policy from the flags, emit ONE bound report frame on
//! `--report-fd`, wait for the parent's GO byte on `--control-fd`, then `exec` the target — and
//! misbehaves on demand (env `O7_FAKE_MODE`) so the tests can pin every fail-closed path.
//!
//! It is test infrastructure: it is built by the o7-worker package so integration tests can
//! locate it via `CARGO_BIN_EXE_sandboy_fake`, and it is exercised only once the live launch
//! lands GREEN. `unsafe` is still forbidden (fd access goes through `/proc/self/fd/<n>` paths;
//! `exec` uses the safe `CommandExt::exec`).
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

/// A minimal parse of the argv this backend understands.
struct Args {
    report_fd: i32,
    control_fd: i32,
    nonce: String,
    worktree: PathBuf,
    timeout_ms: u128,
    allow_exec: Vec<PathBuf>,
    allow_env: Vec<OsString>,
    target: PathBuf,
    target_args: Vec<OsString>,
}

fn parse_args() -> Option<Args> {
    let mut it = std::env::args_os().skip(1); // skip program name
                                              // First positional must be `run`.
    if it.next()?.to_str()? != "run" {
        return None;
    }
    let mut report_fd = None;
    let mut control_fd = None;
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
    let target_args: Vec<OsString> = it.collect();
    Some(Args {
        report_fd: report_fd?,
        control_fd: control_fd?,
        nonce: nonce?,
        worktree: worktree?,
        timeout_ms: timeout_ms?,
        allow_exec,
        allow_env,
        target,
        target_args,
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
    let mode = std::env::var("O7_FAKE_MODE").unwrap_or_default();

    // The "exit before report" misbehavior: die before delivering anything.
    if mode == "exit_before_report" {
        return 70;
    }

    // Compute the honest bindings, then perturb per mode.
    let policy = reconstructed_policy(&args);
    let mut policy_digest = policy.digest();
    let target_bytes = std::fs::read(&args.target).unwrap_or_default();
    let mut target_digest = Digest256::of_bytes(&target_bytes);
    let mut nonce = args.nonce.clone();
    let mut backend = BackendIdentity::new("sandboy-linux", "0.1.0")
        .unwrap_or_else(|_| BackendIdentity::new("x", "x").expect("static identity"));
    let mut dims = [Enforcement::Enforced; 5];

    match mode.as_str() {
        "wrong_policy" => policy_digest = Digest256::of_bytes(b"a different policy"),
        "wrong_target" => target_digest = Digest256::of_bytes(b"a different target"),
        "wrong_nonce" => nonce = LaunchNonce::from_bytes([0xff; 16]).as_str().to_owned(),
        "wrong_backend" => {
            backend = BackendIdentity::new("trust-me", "9.9").expect("static identity");
        }
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
            policy_digest,
            launch_nonce: nonce,
            target_digest,
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

    // Wait for the parent's GO (a single 'G' byte) before exec'ing the target. Anything else,
    // or EOF, is a NACK — the target must NOT run.
    let control_path = format!("/proc/self/fd/{}", args.control_fd);
    let mut go = [0u8; 1];
    match std::fs::File::open(&control_path).and_then(|mut f| f.read_exact(&mut go)) {
        Ok(()) if go[0] == b'G' => {}
        _ => return 67, // NACK / EOF — fail closed, target never runs.
    }

    // GO: exec the target IN PLACE (the report descriptor is NOT passed on — a real backend
    // marks it CLOEXEC before this point so the target cannot write to the channel).
    let err = Command::new(&args.target).args(&args.target_args).exec();
    eprintln!("sandboy_fake: exec failed: {err}");
    69
}
