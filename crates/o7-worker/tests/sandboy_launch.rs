//! RED acceptance: the confined LIVE LAUNCH of the Sandboy backend.
//!
//! This pins the behavior the GREEN follow-up must deliver: spawning a permitted target
//! through the external backend yields a live [`BoundaryProcess`], and the run-time report
//! is re-proven before the boundary is trusted. It is RED today —
//! [`SandboyBoundary::spawn`] fails closed with "launch not yet implemented" rather than
//! pretend to confine — and turns GREEN when the backend launch + report channel land.
//!
//! The launch keeps the child in the SAME PID namespace so PR 3's sealed-memfd exec via
//! `/proc/<verifier_pid>/fd/<n>` still resolves; that fd path is granted here through the
//! policy's exec allowance.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::ffi::OsString;
use std::path::PathBuf;
use std::time::Duration;

use o7_worker::boundary::BoundarySpawnSpec;
use o7_worker::{NetworkPolicy, ProcessBoundary, SandboxPolicy, SandboyBoundary, StdinMode};

/// A policy that grants `/proc` (so a sealed-memfd `/proc/<pid>/fd/<n>` target may exec)
/// and the standard binary dir, deny-all network, and a real timeout.
fn launch_policy() -> SandboxPolicy {
    SandboxPolicy {
        worktree: std::env::temp_dir(),
        allow_exec: vec![PathBuf::from("/proc"), PathBuf::from("/bin")],
        network: NetworkPolicy::DenyAll,
        env_allowlist: vec![OsString::from("PATH")],
        timeout: Duration::from_secs(30),
    }
}

fn permitted_target() -> BoundarySpawnSpec {
    BoundarySpawnSpec {
        // A permitted, real target under an allowed exec dir.
        executable: PathBuf::from("/bin/sh"),
        arguments: vec![OsString::from("-c"), OsString::from("exit 0")],
        working_directory: std::env::temp_dir(),
        environment: Default::default(),
        stdin: StdinMode::Null,
    }
}

#[tokio::test]
async fn a_confined_launch_of_a_permitted_target_yields_a_live_process() {
    // RED: the backend launch is not implemented yet, so spawn fails closed. When GREEN,
    // the external backend confines, emits a full report, execs the target, and this
    // returns a live BoundaryProcess that waits to a clean exit.
    let boundary = SandboyBoundary::new(PathBuf::from("/bin/sh"), launch_policy())
        .expect("a full-confinement policy is accepted");
    let result = boundary.spawn(permitted_target()).await;
    assert!(
        result.is_ok(),
        "a permitted target must launch under the confined backend (RED until the launch lands): {:?}",
        result.err()
    );
}
