//! RED acceptance: the confined LIVE LAUNCH of the Sandboy backend.
//!
//! These pin the behavior the GREEN follow-up must deliver and are RED today —
//! [`SandboyBoundary::spawn`] fails closed with "launch not yet implemented" rather than
//! pretend to confine.
//!
//! Scope note (why the negative matrix is not here yet): a test that asserts `spawn`
//! *fails* would pass VACUOUSLY against the current fail-closed stub and prove nothing — a
//! false RED. So the report-content negatives (partial / malformed / oversized / truncated /
//! mis-binding) are proven now, PURELY and GREEN, against
//! [`SandboyBoundary::verify_report`] in `sandbox_contract.rs`; and the launch-level
//! negatives that genuinely need a live process (backend exits before exec, spawn cancelled
//! mid-report leaves no backend/target, report fd write-end closed) land WITH the live
//! launch and its controlled fake backend in the GREEN commit. What remains here is what can
//! only be RED with the launch absent: the rich POSITIVE contract, and the sealed-memfd
//! probe — assertions an unconfined spawn could never satisfy.
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
use o7_worker::sandbox_protocol::policy::{NetworkPolicy, SandboxPolicy};
use o7_worker::{
    BoundaryExit, BoundaryKind, EnforcementLevel, ProcessBoundary, SandboyBoundary, StdinMode,
};

/// A backend that exists and is absolute so `new()` succeeds. The GREEN commit swaps this for
/// a controlled fake `sandboy` that emits a bound full report and execs the target; until
/// then `spawn` fails closed and every test below is RED.
const BACKEND: &str = "/bin/sh";

fn launch_policy(allow_exec: Vec<PathBuf>) -> SandboxPolicy {
    SandboxPolicy {
        worktree: std::env::temp_dir(),
        allow_exec,
        network: NetworkPolicy::DenyAll,
        env_allowlist: vec![OsString::from("PATH")],
        timeout: Duration::from_secs(30),
    }
}

#[tokio::test]
async fn a_confined_full_report_launch_runs_the_target_and_proves_full_enforcement() {
    // RED. When GREEN: the backend confines, emits a report bound to policy/launch/target,
    // execs the target, and spawn returns a live process plus evidence attesting
    // Sandboy/FullyEnforced; the target waits to a clean exit. None of this can be satisfied
    // by an unconfined spawn (which establishes NO evidence), so the test is not false-green.
    let boundary = SandboyBoundary::new(
        PathBuf::from(BACKEND),
        launch_policy(vec![PathBuf::from("/bin")]),
    )
    .expect("valid boundary");
    let target = BoundarySpawnSpec {
        executable: PathBuf::from("/bin/sh"),
        arguments: vec![OsString::from("-c"), OsString::from("exit 0")],
        working_directory: std::env::temp_dir(),
        environment: Default::default(),
        stdin: StdinMode::Null,
    };

    let mut launch = boundary
        .spawn(target)
        .await
        .expect("a permitted target launches under the confined backend");
    assert_eq!(
        launch.evidence.attestation.implementation,
        BoundaryKind::Sandboy
    );
    assert_eq!(
        launch.evidence.attestation.enforcement,
        EnforcementLevel::FullyEnforced,
        "the launch must PROVE full enforcement, not merely claim it"
    );
    assert!(
        launch.evidence.report.is_some(),
        "the raw report frame must be carried for content-addressed persistence"
    );
    let exit = launch.process.wait().await.expect("the target is waited");
    assert_eq!(
        exit,
        BoundaryExit::Code(0),
        "the confined target ran to exit 0"
    );
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn the_exact_sealed_memfd_execs_under_the_final_landlock_policy() {
    // The load-bearing PR-3 invariant, proven LIVE rather than lexically. A verified sealed
    // memfd is exec'd via /proc/<pid>/fd/<n> AFTER the backend installs its final Landlock
    // policy; the target's identity/output must prove the exact memfd ran. The kernel caveat
    // (objects reachable only through /proc/<pid>/fd/* may not be restrictable by ordinary
    // Landlock path rules) is exactly what this must settle; if it cannot hold on the
    // supported ABI, GREEN must evolve BoundarySpawnSpec to carry the descriptor — never a
    // path-copy fallback.
    //
    // RED today: spawn is unimplemented. The exact fd path is granted (not a broad "/proc").
    use std::os::unix::io::AsRawFd as _;

    // A stand-in for the sealed object: a real /proc/self/fd/<n> path. GREEN replaces this
    // with the verifier's genuinely sealed memfd.
    let file = std::fs::File::open("/bin/sh").expect("open a real executable");
    let fd = file.as_raw_fd();
    let sealed_path = PathBuf::from(format!("/proc/self/fd/{fd}"));

    let boundary = SandboyBoundary::new(
        PathBuf::from(BACKEND),
        // Grant the EXACT fd path — not the whole /proc — so the test pins the precise
        // requirement rather than a permissive declaration.
        launch_policy(vec![sealed_path.clone(), PathBuf::from("/bin")]),
    )
    .expect("valid boundary");
    let target = BoundarySpawnSpec {
        executable: sealed_path,
        arguments: vec![OsString::from("-c"), OsString::from("exit 0")],
        working_directory: std::env::temp_dir(),
        environment: Default::default(),
        stdin: StdinMode::Null,
    };

    let launch = boundary
        .spawn(target)
        .await
        .expect("the exact sealed memfd must exec under the final Landlock policy");
    assert_eq!(
        launch.evidence.attestation.enforcement,
        EnforcementLevel::FullyEnforced
    );
}
