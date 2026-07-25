//! A CONTROLLED fake `sandboy` backend for the launch acceptance matrix. It speaks the real
//! protocol AND respects the chosen MONITOR topology over a SINGLE bidirectional control socket on
//! its STDIN (fd 0): read the launch request frame, emit ONE bound report frame then HALF-CLOSE
//! the write side (so the parent can prove the frame is the whole message by reading EOF), read
//! the parent's GO byte, then — on GO — CLOSE the control socket, SCRUB every inherited descriptor
//! except stdout/stderr, and start the target as a CHILD that INHERITS the monitor's process
//! group (its stdin forced to null), remaining alive as the monitor that owns the group and relays
//! the target's exit. It never `exec`s into the target (the rejected exec-in-place architecture).
//! It misbehaves on demand via its own (trusted, control-plane) environment variable
//! `O7_FAKE_MODE` — never the target's env.
//!
//! Test infrastructure: built by the o7-worker package so integration tests can locate it via
//! `CARGO_BIN_EXE_sandboy_fake`. `unsafe` stays forbidden (the control socket comes from a SAFE
//! clone of stdin's descriptor; fd close via nix; no `exec`).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::print_stderr
)]

use std::ffi::OsString;
use std::io::{Read as _, Write as _};
use std::net::Shutdown;
use std::os::fd::AsFd as _;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;

use o7_sandbox_protocol::frame::encode;
use o7_sandbox_protocol::ids::{BackendIdentity, Digest256, LaunchNonce};
use o7_sandbox_protocol::policy::{Enforcement, NetworkPolicy, SandboxPolicy};
use o7_sandbox_protocol::report::SandboxReport;
use o7_sandbox_protocol::SCHEMA_VERSION;

/// A minimal parse of the argv this backend understands. The control transport is NOT on the argv:
/// it is the single socket on stdin. Sensitive target values (env) do NOT ride the argv (visible
/// via `/proc`); only the non-sensitive target path/args/cwd do.
struct Args {
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
    let mut nonce = None;
    let mut worktree = None;
    let mut timeout_ms = None;
    let mut allow_exec = Vec::new();
    let mut allow_env = Vec::new();
    loop {
        let flag = it.next()?;
        let flag = flag.to_str()?;
        match flag {
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
        nonce: nonce?,
        worktree: worktree?,
        timeout_ms: timeout_ms?,
        allow_exec,
        allow_env,
        target,
    })
}

/// The control socket: a SAFE owned clone of stdin's descriptor (fd 0), which the parent mapped to
/// a `UnixStream` end. No `unsafe` `from_raw_fd` — `try_clone_to_owned` dups it for us.
fn control_socket() -> Option<UnixStream> {
    let owned = std::io::stdin().as_fd().try_clone_to_owned().ok()?;
    Some(UnixStream::from(owned))
}

/// Read exactly one length-prefixed frame (4-byte BE length + body) from `sock`, bounded by `max`.
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
    // target's. It is `base;k=v;k=v`: the base behavior plus test-only parameters (a pid file to
    // record this backend's pid, etc.). Encoding params in the fake-mode string keeps the
    // production BackendConfig surface unchanged.
    let raw_mode = std::env::var("O7_FAKE_MODE").unwrap_or_default();
    let mut parts = raw_mode.split(';');
    let mode = parts.next().unwrap_or("").to_owned();
    let mut pid_file = None;
    let mut ready_file = None;
    let mut release_file = None;
    let mut desc_file = None;
    for kv in parts {
        if let Some(path) = kv.strip_prefix("pid=") {
            pid_file = Some(path.to_owned());
        } else if let Some(path) = kv.strip_prefix("ready=") {
            ready_file = Some(path.to_owned());
        } else if let Some(path) = kv.strip_prefix("release=") {
            release_file = Some(path.to_owned());
        } else if let Some(path) = kv.strip_prefix("desc=") {
            desc_file = Some(path.to_owned());
        }
    }
    // Record this backend's pid EARLY so a test can observe reap/teardown even when spawn is
    // cancelled and never returns the process.
    if let Some(path) = &pid_file {
        let _ = std::fs::write(path, std::process::id().to_string());
    }

    if mode == "exit_before_report" {
        return 70; // die before delivering anything
    }

    // The control transport is a SINGLE bidirectional socket on stdin (fd 0).
    let Some(mut sock) = control_socket() else {
        return 62;
    };
    // Read the out-of-band launch request frame (target argv/cwd/env/stdin + target_digest).
    let Some(request_frame) =
        read_frame(&mut sock, o7_sandbox_protocol::request::MAX_REQUEST_BYTES)
    else {
        return 63;
    };
    let request = match o7_sandbox_protocol::request::decode(&request_frame) {
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

    // `fork_before_bad_report`: a BROKEN backend that forks a live descendant (inheriting the
    // leader's owned group) BEFORE it delivers a bad report. The parent must reap the whole GROUP,
    // not just the leader. The BACKEND records the descendant's pid synchronously (from the spawn
    // handle) so the test observes it without racing the cleanup; the report is then malformed.
    if mode == "fork_before_bad_report" {
        let mut d = Command::new("/bin/sleep");
        d.arg("5");
        if let Ok(child) = d.spawn() {
            if let Some(path) = &desc_file {
                let _ = std::fs::write(path, child.id().to_string());
            }
            // Do not wait/kill it here — it is owned by the group and must be reaped by the
            // PARENT's group teardown. (std `Child` drop neither waits nor kills.)
        }
    }

    // Build the report frame (or malformed bytes on demand).
    let frame: Vec<u8> = if mode == "malformed" || mode == "fork_before_bad_report" {
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
    // `stall_mid_report`: write a length prefix + only a PARTIAL body, then stall forever (writer
    // NOT half-closed), so the parent's bounded frame read blocks mid-frame and a cancel must reap.
    if mode == "stall_mid_report" {
        let _ = sock.write_all(&100u32.to_be_bytes()); // claim 100 bytes
        let _ = sock.write_all(b"partial"); // deliver only 7
        let _ = sock.flush();
        loop {
            std::thread::sleep(Duration::from_secs(3600));
        }
    }

    if sock.write_all(&frame).is_err() {
        return 66;
    }
    // `trailing`: emit a SECOND, unexpected byte after the single frame. A correct parent must
    // reject the trailing byte and never authorize the target.
    if mode == "trailing" {
        let _ = sock.write_all(b"X");
    }
    // HALF-CLOSE the write side after exactly one frame, BEFORE reading GO — so the parent can
    // prove the frame is the ENTIRE message by reading EOF. The read side stays open for GO.
    if sock.shutdown(Shutdown::Write).is_err() {
        return 66;
    }

    // Read the parent's GO ('G') from the socket before starting the target. EOF / anything else
    // is a NACK — fail closed, target never runs.
    let mut go = [0u8; 1];
    match sock.read_exact(&mut go) {
        Ok(()) if go[0] == b'G' => {}
        _ => return 67,
    }
    // Close the control socket entirely before the target starts (the dup handle here plus fd 0).
    drop(sock);
    let _ = nix::unistd::close(0);

    // SCRUB the whole inherited descriptor set except stdout/stderr (1/2): enumerate /proc/self/fd
    // and close every other fd, so neither the control channel NOR any other inherited descriptor
    // leaks into the target. A range or a fixed close set is not enough — an arbitrary high fd
    // would survive. Single-threaded here, so enumerate-then-close is race-free (collect all
    // numbers first, then close, so we never mutate the directory we are iterating). fd 0 is
    // already closed above; the target's own stdin is set to null below.
    let mut inherited = Vec::new();
    if let Ok(entries) = std::fs::read_dir("/proc/self/fd") {
        for entry in entries.flatten() {
            if let Some(fd) = entry
                .file_name()
                .to_str()
                .and_then(|n| n.parse::<i32>().ok())
            {
                if fd > 2 {
                    inherited.push(fd);
                }
            }
        }
    }
    for fd in inherited {
        let _ = nix::unistd::close(fd);
    }

    // `hold_before_target`: prove the LIVE hash→exec property. The parent has already sealed the
    // target and bound the launch; here we announce (`ready`) that the sealed target/request are
    // in hand but the target has NOT started, then WAIT (bounded) for the parent to `release`.
    // The parent uses that window to rewrite the SOURCE path — which must NOT change what runs,
    // because the executed object is the sealed memfd, not a re-read of the path.
    if mode == "hold_before_target" {
        if let Some(path) = &ready_file {
            let _ = std::fs::write(path, b"ready");
        }
        if let Some(path) = &release_file {
            // Bounded: ~10s at 20ms polls, then proceed regardless so a test can never wedge us.
            for _ in 0..500 {
                if std::path::Path::new(path).exists() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
        }
    }

    // MONITOR topology: start the target as a CHILD that INHERITS the monitor's process group, so
    // the monitor OWNS the target (and its descendants) for membership and teardown. It must NOT
    // create its own group (`process_group(0)`) — that would orphan the target from the owned set,
    // so `remaining_members`/`force_stop` would miss it. The target's argv/cwd/env come from the
    // out-of-band REQUEST (allowlisted), never from the backend's own argv/env. `env_clear()`
    // first, then set exactly the request's allowlisted env.
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
    // The target's stdin is NULL — never the control socket (which lived on the backend's fd 0).
    .stdin(Stdio::null());
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
    // `monitor_exit_early`: abandon the target — exit immediately WITHOUT waiting it, leaving it
    // running unowned. A correct boundary must treat a monitor that died with a live target as a
    // failure, not a clean exit.
    if mode == "monitor_exit_early" {
        return 0;
    }
    // Remain as the monitor until the target exits, then relay its outcome. (A real backend
    // would also enforce the wall-clock timeout and tear the cgroup down here.)
    match child.wait() {
        Ok(status) => status.code().unwrap_or(1),
        Err(_) => 1,
    }
}
