//! A test helper that drives ONE `SandboyBoundary::spawn` against a caller-chosen target and
//! reports the outcome via its EXIT CODE — so an integration test can put `spawn` under a
//! wall-clock bound in a SEPARATE process. A target whose acquisition BLOCKS (a FIFO with no
//! writer) wedges this helper instead of the test's own runtime; the parent test bounds the
//! helper's lifetime and kills it, turning "acquisition blocks forever" into a clean RED that a
//! non-blocking hardened acquisition converts to a fast fail-closed exit.
//!
//! argv: `<backend-binary> <allow-exec-dir> <target-path>`. Exit codes: 3 = spawn failed closed
//! (the desired hardened behavior); 0 = spawn unexpectedly SUCCEEDED (the target ran); 2 = usage;
//! anything else = internal error. Under the current happy-path engine a blocking target never
//! lets this return at all (the parent observes the hang). `unsafe` stays forbidden.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::print_stderr
)]

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use o7_worker::boundary::BoundarySpawnSpec;
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
            // Should not happen for a non-regular / blocking target; do not leak the launch.
            let _ = launch.process.force_stop().await;
            0
        }
        Err(_) => 3,
    }
}
