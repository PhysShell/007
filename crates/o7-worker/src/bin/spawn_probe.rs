//! A test helper that drives ONE `SandboyBoundary::spawn` against a caller-chosen target and
//! reports the outcome via its EXIT CODE — so an integration test can put `spawn` under a
//! wall-clock bound in a SEPARATE process. A target whose acquisition BLOCKS (a FIFO with no
//! writer) wedges this helper instead of the test's own runtime; the parent test bounds the
//! helper's lifetime and kills it, turning "acquisition blocks forever" into a clean RED that a
//! non-blocking hardened acquisition converts to a fast fail-closed exit.
//!
//! argv: `<backend-binary> <allow-exec-dir> <target-path>`. Per AGENTS.md §2 the exit code is a
//! PASS/FAIL/ERROR verdict, with `0` reserved for `PASS` alone. `0` is PASS: the target was refused
//! exactly as intended — a typed [`BoundaryError::TargetAcquisition`] (the hardened
//! `O_NOFOLLOW`/regular-file open rejecting a symlink/FIFO/non-regular/over-size target); ONLY this
//! outcome earns the pass. `1` is FAIL: a target that should have been refused unexpectedly
//! **LAUNCHED**. `4` is ERROR: any other [`BoundaryError`] (backend spawn, RNG, control socket,
//! evidence/protocol, unsupported platform, or a target STAGING/sealing failure) — an infrastructure
//! fault, never a refusal, with the category and message on stderr; a test that accepted it as a
//! refusal would false-green. `2` is a usage ERROR. `unsafe` stays forbidden.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::print_stderr
)]

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use o7_worker::boundary::{BoundaryError, BoundarySpawnSpec};
use o7_worker::sandbox_protocol::ids::{BackendIdentity, Digest256};
use o7_worker::sandbox_protocol::policy::{NetworkPolicy, SandboxPolicy};
use o7_worker::{BackendConfig, BackendImage, ProcessBoundary, SandboyBoundary, StdinMode};

fn main() {
    let code = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("runtime")
        .block_on(run());
    std::process::exit(code);
}

async fn run() -> i32 {
    let mut argv = std::env::args_os().skip(1);
    let (Some(backend), Some(allow), Some(target)) = (argv.next(), argv.next(), argv.next()) else {
        eprintln!("usage: spawn_probe <backend-binary> <allow-exec-dir> <target-path>");
        return 2;
    };
    let backend = PathBuf::from(backend);
    let allow = PathBuf::from(allow);
    let target = PathBuf::from(target);

    let bytes = std::fs::read(&backend).expect("read backend binary");
    let image = BackendImage::acquire(
        &backend,
        Digest256::of_bytes(&bytes),
        BackendIdentity::new("sandboy-linux", "0.1.0").expect("identity"),
    )
    .expect("acquire backend");

    let boundary = SandboyBoundary::new(
        image,
        SandboxPolicy {
            worktree: std::env::temp_dir(),
            allow_exec: vec![allow],
            network: NetworkPolicy::DenyAll,
            env_allowlist: vec![std::ffi::OsString::from("PATH")],
            timeout: Duration::from_secs(30),
        },
    )
    .expect("valid boundary")
    .with_backend_config(BackendConfig {
        fake_mode: Some("ok".to_owned()),
    });

    let spec = BoundarySpawnSpec {
        executable: target,
        arguments: vec![],
        working_directory: std::env::temp_dir(),
        environment: BTreeMap::new(),
        stdin: StdinMode::Null,
    };

    match boundary.spawn(spec).await {
        // PASS (exit 0, per AGENTS.md §2): the hardened acquisition refused the target exactly as
        // intended — the ONLY outcome that earns the pass.
        Err(BoundaryError::TargetAcquisition(msg)) => {
            eprintln!("PASS: target acquisition failed closed (expected): {msg}");
            0
        }
        // FAIL (exit 1): the target should have been refused, but it launched; do not leak it.
        Ok(mut launch) => {
            let _ = launch.process.force_stop().await;
            eprintln!("FAIL: target unexpectedly launched (should have failed closed)");
            1
        }
        // ERROR (exit 4): any other error is infrastructure, NOT a target refusal — print the
        // category so a consumer can distinguish it and never mistake it for the PASS above.
        Err(other) => {
            eprintln!(
                "ERROR: unexpected boundary error [{}]: {other}",
                category(&other)
            );
            4
        }
    }
}

/// A stable, human-readable category tag for an unexpected [`BoundaryError`], printed to stderr so a
/// failing run names the infrastructure fault instead of collapsing it into a bare exit code.
fn category(err: &BoundaryError) -> &'static str {
    match err {
        BoundaryError::Spawn(_) => "spawn",
        BoundaryError::TargetAcquisition(_) => "target-acquisition",
        BoundaryError::Signal(_) => "signal",
        BoundaryError::Wait(_) => "wait",
        BoundaryError::Membership(_) => "membership",
        BoundaryError::Evidence(_) => "evidence",
        BoundaryError::UnsupportedPlatform => "unsupported-platform",
    }
}
