//! VB-0 backend behavior tests: drive the REAL `sandboy` binary directly over the fd-0 control
//! socket (the same transport `SandboyBoundary` uses) and prove the honest fail-closed bootstrap —
//! a correctly-bound but HONESTLY-DOWNGRADED report, end-of-message half-close, and fail-closed
//! refusal on a malformed/truncated/oversized request, a NACK, or even a (buffered or explicit) GO.
//!
//! The "the target never runs" claim is proven with a REAL, RUNNABLE target: a marker-target script
//! that UNCONDITIONALLY creates its marker when executed (independent of the request argv), pointed
//! at as the sealed target after `--`. So `!marker.exists()` is a live assertion — if the backend
//! ever erroneously spawned the target it WOULD create the marker — not a vacuous check of a
//! non-existent file. Each case asserts BOTH the exact exit code AND that live effect.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::ffi::OsString;
use std::io::{Read as _, Write as _};
use std::os::fd::OwnedFd;
use std::os::unix::fs::PermissionsExt as _;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use o7_sandbox_protocol::ids::{Digest256, LaunchNonce};
use o7_sandbox_protocol::policy::{NetworkPolicy, SandboxPolicy};
use o7_sandbox_protocol::request::{EnvEntry, LaunchRequest, StdinKind};
use o7_sandbox_protocol::SCHEMA_VERSION;

const NONCE_HEX: &str = "0123456789abcdef0123456789abcdef";
const TIMEOUT_MS: u128 = 30_000;

fn backend_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_sandboy"))
}

/// The digest of the on-disk backend binary — what `/proc/self/exe` resolves to for a plainly
/// exec'd `sandboy`, so the report's `backend_digest` must equal this.
fn backend_digest() -> Digest256 {
    Digest256::of_bytes(&std::fs::read(backend_bin()).expect("read backend binary"))
}

/// A REAL, executable target that — IF ever run — creates `marker`, with NO dependency on the
/// request argv (it hardcodes the marker path). Returns `(target_path, marker_path)`. The backend
/// must never run it at VB-0, so `!marker.exists()` afterwards is a meaningful, non-vacuous effect.
fn make_marker_target(dir: &Path) -> (PathBuf, PathBuf) {
    let marker = dir.join("target-ran");
    let target = dir.join("real-target.sh");
    std::fs::write(
        &target,
        format!("#!/bin/sh\ntouch '{}'\n", marker.display()),
    )
    .expect("write marker-target");
    let mut perms = std::fs::metadata(&target).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&target, perms).expect("chmod +x marker-target");
    (target, marker)
}

/// A worktree/allow-exec/allow-env set used consistently by the argv AND the reconstructed policy,
/// so the report's `policy_digest` is checkable.
fn policy() -> SandboxPolicy {
    SandboxPolicy {
        worktree: std::env::temp_dir(),
        allow_exec: vec![PathBuf::from("/bin")],
        network: NetworkPolicy::DenyAll,
        env_allowlist: vec![OsString::from("PATH")],
        timeout: Duration::from_millis(TIMEOUT_MS as u64),
    }
}

/// The exact confined-backend argv `SandboyBoundary::backend_argv` + `policy_flags` build. `target`
/// is the sealed target path after `--`.
fn argv(target: &Path) -> Vec<OsString> {
    let p = policy();
    let mut a: Vec<OsString> = vec![
        "run".into(),
        "--launch-nonce".into(),
        NONCE_HEX.into(),
        "--deny-net".into(),
        "--worktree".into(),
        p.worktree.clone().into_os_string(),
        "--timeout-ms".into(),
        OsString::from(TIMEOUT_MS.to_string()),
    ];
    for e in &p.allow_exec {
        a.push("--allow-exec".into());
        a.push(e.clone().into_os_string());
    }
    for n in &p.env_allowlist {
        a.push("--allow-env".into());
        a.push(n.clone());
    }
    a.push("--".into());
    a.push(target.as_os_str().to_os_string());
    a
}

/// A valid launch request. The target's marker is created by the target SCRIPT itself, so the
/// request argv is deliberately empty — the effect must not depend on it. The request nonce equals
/// the argv nonce (the parent sets both from one mint), so `launch_spec_digest` is well-defined.
fn valid_request() -> LaunchRequest {
    LaunchRequest {
        schema_version: SCHEMA_VERSION,
        target_digest: Digest256::of_bytes(b"vb0-test-target"),
        argv: vec![],
        cwd: std::env::temp_dir()
            .to_string_lossy()
            .into_owned()
            .into_bytes(),
        env: vec![EnvEntry {
            name: b"PATH".to_vec(),
            value: b"/usr/bin:/bin".to_vec(),
        }],
        stdin: StdinKind::Null,
        launch_nonce: LaunchNonce::parse(NONCE_HEX).unwrap(),
    }
}

/// Spawn the backend with `args`, its stdin mapped to `child` (a `UnixStream` end); return the
/// child process and the parent's socket end.
fn spawn(
    args: &[OsString],
    parent: UnixStream,
    child: UnixStream,
) -> (std::process::Child, UnixStream) {
    let backend = Command::new(backend_bin())
        .args(args)
        .stdin(Stdio::from(OwnedFd::from(child)))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn sandboy");
    (backend, parent)
}

/// Read one length-prefixed report frame from `sock`, then prove end-of-message: one more read must
/// be EOF (the backend half-closed after the single frame). Returns the frame bytes.
fn read_report_and_prove_eof(sock: &mut UnixStream) -> Vec<u8> {
    let mut len_buf = [0u8; 4];
    sock.read_exact(&mut len_buf).expect("read report length");
    let len = u32::from_be_bytes(len_buf) as usize;
    let mut body = vec![0u8; len];
    sock.read_exact(&mut body).expect("read report body");
    let mut extra = [0u8; 1];
    assert_eq!(
        sock.read(&mut extra).expect("read past the frame"),
        0,
        "the backend must HALF-CLOSE after exactly one frame (EOF proves end-of-message)"
    );
    let mut frame = len_buf.to_vec();
    frame.extend_from_slice(&body);
    frame
}

fn exit_code(mut child: std::process::Child) -> i32 {
    child
        .wait()
        .expect("wait backend")
        .code()
        .expect("exit code")
}

#[test]
fn honest_downgraded_report_then_nack_and_the_runnable_target_never_runs() {
    let dir = tempfile::tempdir().unwrap();
    let (target, marker) = make_marker_target(dir.path());
    let req = valid_request();

    let (parent, child) = UnixStream::pair().unwrap();
    let (backend, mut parent) = spawn(&argv(&target), parent, child);

    parent
        .write_all(&o7_sandbox_protocol::request::encode(&req).unwrap())
        .unwrap();
    let frame = read_report_and_prove_eof(&mut parent);
    let report = o7_sandbox_protocol::frame::decode(&frame).expect("valid report frame");

    // The report is correctly BOUND to this policy/launch/backend object. (The per-dimension
    // enforcement is now VB-4-real and environment-dependent — this test drops the socket before GO,
    // so what matters here is the binding + that a NACK never runs the target.)
    assert_eq!(report.schema_version, SCHEMA_VERSION);
    assert_eq!(report.backend.name(), "sandboy-linux");
    assert_eq!(report.backend_digest, backend_digest());
    assert_eq!(report.policy_digest, policy().digest());
    assert_eq!(report.launch_nonce, LaunchNonce::parse(NONCE_HEX).unwrap());
    assert_eq!(report.launch_spec_digest, req.spec_digest());

    // NACK: drop the socket without GO. The backend reads EOF, tears down, and exits; the RUNNABLE
    // target is never released, so its marker never appears.
    drop(parent);
    assert_eq!(exit_code(backend), 67, "NACK/EOF before GO");
    assert!(
        !marker.exists(),
        "a NACK'd launch must never run the target"
    );
}

#[test]
fn an_explicit_go_on_a_downgraded_report_still_refuses_a_runnable_target() {
    // Defense in depth: a (broken) parent sends GO on a downgraded report, and the target is a REAL
    // runnable script — yet the backend refuses to launch it, so the marker never appears.
    let dir = tempfile::tempdir().unwrap();
    let (target, marker) = make_marker_target(dir.path());

    let (parent, child) = UnixStream::pair().unwrap();
    let (backend, mut parent) = spawn(&argv(&target), parent, child);
    parent
        .write_all(&o7_sandbox_protocol::request::encode(&valid_request()).unwrap())
        .unwrap();
    let _ = read_report_and_prove_eof(&mut parent);
    parent.write_all(b"G").unwrap(); // explicit GO on an unenforced self-check

    assert_eq!(exit_code(backend), 71, "GO on unenforced → refuse");
    assert!(
        !marker.exists(),
        "even a runnable target must not run on a spurious GO"
    );
}

#[test]
fn a_buffered_early_go_before_the_report_still_refuses_a_runnable_target() {
    // A parent that PRE-SENDS the GO (request frame + 'G' in ONE write, before the backend has
    // emitted its report) must not bypass the self-check: the buffered 'G' is read only after the
    // downgraded report + half-close, and the backend still refuses the runnable target.
    let dir = tempfile::tempdir().unwrap();
    let (target, marker) = make_marker_target(dir.path());

    let (parent, child) = UnixStream::pair().unwrap();
    let (backend, mut parent) = spawn(&argv(&target), parent, child);
    let mut buf = o7_sandbox_protocol::request::encode(&valid_request()).unwrap();
    buf.push(b'G'); // early GO, buffered behind the request
    parent.write_all(&buf).unwrap();
    let _ = read_report_and_prove_eof(&mut parent);

    assert_eq!(
        exit_code(backend),
        71,
        "buffered early GO must not bypass the self-check"
    );
    assert!(
        !marker.exists(),
        "a buffered early GO must not run the runnable target"
    );
}

#[test]
fn a_malformed_request_body_fails_closed_and_the_runnable_target_never_runs() {
    let dir = tempfile::tempdir().unwrap();
    let (target, marker) = make_marker_target(dir.path());
    let (parent, child) = UnixStream::pair().unwrap();
    let (backend, mut parent) = spawn(&argv(&target), parent, child);
    // A well-framed but non-JSON body → decode fails.
    let body = b"this is not a launch request";
    parent
        .write_all(&(body.len() as u32).to_be_bytes())
        .unwrap();
    parent.write_all(body).unwrap();
    drop(parent);
    assert_eq!(exit_code(backend), 63, "malformed body → BAD_REQUEST");
    assert!(!marker.exists(), "no target on a malformed request");
}

#[test]
fn a_truncated_request_fails_closed_and_the_runnable_target_never_runs() {
    let dir = tempfile::tempdir().unwrap();
    let (target, marker) = make_marker_target(dir.path());
    let (parent, child) = UnixStream::pair().unwrap();
    let (backend, mut parent) = spawn(&argv(&target), parent, child);
    // A length prefix promising 100 bytes, but only 4 delivered, then close.
    parent.write_all(&100u32.to_be_bytes()).unwrap();
    parent.write_all(b"frag").unwrap();
    drop(parent);
    assert_eq!(exit_code(backend), 63, "truncated request → BAD_REQUEST");
    assert!(!marker.exists(), "no target on a truncated request");
}

#[test]
fn an_oversized_request_prefix_fails_closed_and_the_runnable_target_never_runs() {
    let dir = tempfile::tempdir().unwrap();
    let (target, marker) = make_marker_target(dir.path());
    let (parent, child) = UnixStream::pair().unwrap();
    let (backend, mut parent) = spawn(&argv(&target), parent, child);
    let oversized = (o7_sandbox_protocol::request::MAX_REQUEST_BYTES as u32) + 1;
    parent.write_all(&oversized.to_be_bytes()).unwrap();
    drop(parent);
    assert_eq!(exit_code(backend), 63, "oversized request → BAD_REQUEST");
    assert!(!marker.exists(), "no target on an oversized request");
}

#[test]
fn bad_argv_fails_closed_and_the_runnable_target_never_runs() {
    // A valid `run` with a runnable target after `--`, but an UNKNOWN flag before it → argv parse
    // fails before the target is ever used. The runnable target must still not run.
    let dir = tempfile::tempdir().unwrap();
    let (target, marker) = make_marker_target(dir.path());
    let (parent, child) = UnixStream::pair().unwrap();
    let args = vec![
        OsString::from("run"),
        OsString::from("--not-a-real-flag"),
        OsString::from("--"),
        target.clone().into_os_string(),
    ];
    let (backend, _parent) = spawn(&args, parent, child);
    assert_eq!(exit_code(backend), 64, "bad argv → BAD_ARGV");
    assert!(!marker.exists(), "no target on unparsable argv");
}
