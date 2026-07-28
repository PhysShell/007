//! A test helper that drives ONE `SandboyBoundary::spawn` against a caller-chosen target and
//! reports the outcome via its EXIT CODE — so an integration test can put `spawn` under a
//! wall-clock bound in a SEPARATE process. A target whose acquisition BLOCKS (a FIFO with no
//! writer) wedges this helper instead of the test's own runtime; the parent test bounds the
//! helper's lifetime and kills it, turning "acquisition blocks forever" into a clean RED that a
//! non-blocking hardened acquisition converts to a fast fail-closed exit.
//!
//! argv: `<backend-binary> <allow-exec-dir> <target-path>`. Exit codes classify the OUTCOME so the
//! parent test can demand exactly the one it pins, never "some error happened". `3` is the expected
//! **target-acquisition refusal** ([`BoundaryError::TargetAcquisition`]) — the hardened open refused
//! a symlink/FIFO/non-regular target; ONLY this outcome earns it. `4` is an
//! **unexpected/infrastructure ERROR** — any other [`BoundaryError`] (backend spawn, RNG, control
//! socket, evidence/protocol, unsupported platform), with the category and message on stderr; it is
//! NOT proof the target was refused, and a test that accepted it as such would false-green on an
//! infrastructure failure. `1` is an unexpectedly **SUCCEEDED** spawn (the target ran) — a FAIL for a
//! target that should have been refused; it is non-zero because AGENTS.md §2 reserves exit `0` for
//! `PASS` alone (a launched-but-should-be-refused target is a FAIL, never a pass). `2` is usage.
//! `unsafe` stays forbidden.
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
        Ok(mut launch) => {
            // FAIL (non-zero per AGENTS.md §2 — exit 0 is PASS only): the target should have been
            // refused, but it launched; do not leak the launch.
            let _ = launch.process.force_stop().await;
            eprintln!("FAIL: target unexpectedly launched (should have failed closed)");
            1
        }
        // The one outcome that earns exit 3: the hardened acquisition refused the target.
        Err(BoundaryError::TargetAcquisition(msg)) => {
            eprintln!("target acquisition failed closed (expected): {msg}");
            3
        }
        // Any other error is infrastructure, NOT a target refusal — exit 4 with the category so a
        // consumer can distinguish it and never mistake it for the fail-closed behavior above.
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
