//! VB-0 wire-compatibility: drive the REAL compiled `sandboy` backend (built from `crates/sandboy`)
//! through the PRODUCTION `SandboyBoundary` — no fake, no `test-harness` knob. The backend honestly
//! bootstraps (installs no kernel enforcement yet), so it emits a correctly-BOUND but DOWNGRADED
//! report; the boundary must reject it specifically as `NotFullyEnforced` (proving every binding —
//! policy/nonce/launch-spec/backend identity+digest — matched, i.e. real wire compatibility) and
//! the target must never run. Also proves the backend-object identity/digest binding fails closed.
//!
//! VB-0 installs no confinement, so this needs no kernel capabilities and runs in the PORTABLE gate.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use o7_worker::boundary::BoundarySpawnSpec;
use o7_worker::sandbox_protocol::ids::{BackendIdentity, Digest256};
use o7_worker::sandbox_protocol::policy::{NetworkPolicy, SandboxPolicy};
use o7_worker::{
    BackendImage, BoundaryError, ProcessBoundary, SandboyBoundary, SandboyLaunchError, StdinMode,
};

/// Locate the real `sandboy` binary. It is a workspace member, so `cargo test --workspace` (the
/// gate) builds it as the sibling of this test binary in the target profile dir; under a bare
/// `cargo test -p o7-worker` it may be absent, so build it on demand. No new dependency.
fn real_backend_path() -> PathBuf {
    let mut dir = std::env::current_exe().expect("current_exe");
    dir.pop(); // the test binary itself
    if dir.ends_with("deps") {
        dir.pop(); // -> <target>/<profile>
    }
    let bin = dir.join("sandboy");
    if !bin.exists() {
        let status = Command::new(env!("CARGO"))
            .args(["build", "-p", "sandboy"])
            .status()
            .expect("run cargo build -p sandboy");
        assert!(status.success(), "building the sandboy backend failed");
    }
    assert!(
        bin.exists(),
        "sandboy backend binary not found at {}",
        bin.display()
    );
    bin
}

fn backend_identity() -> BackendIdentity {
    BackendIdentity::new("sandboy-linux", "0.1.0").unwrap()
}

/// Acquire the REAL backend as a sealed, digest-bound object (the production trust path).
fn real_backend() -> BackendImage {
    let path = real_backend_path();
    let bytes = std::fs::read(&path).expect("read the sandboy backend binary");
    BackendImage::acquire(&path, Digest256::of_bytes(&bytes), backend_identity())
        .expect("acquire the real backend")
}

fn policy_for(dir: &std::path::Path) -> SandboxPolicy {
    SandboxPolicy {
        worktree: dir.to_path_buf(),
        allow_exec: vec![dir.to_path_buf()],
        network: NetworkPolicy::DenyAll,
        env_allowlist: vec![OsString::from("PATH")],
        timeout: Duration::from_secs(30),
    }
}

/// A self-contained target: a REGULAR executable file under `dir` that would `touch(marker)` if it
/// ever ran (it must not, at VB-0). A regular file passes the `O_NOFOLLOW` acquisition on any host
/// — unlike `/bin/sh`, which is a symlink on many distros and would fail closed at acquisition.
fn touch_target(dir: &std::path::Path, marker: &std::path::Path) -> BoundarySpawnSpec {
    let target = dir.join("target");
    std::fs::write(
        &target,
        format!("#!/bin/sh\ntouch {}\nexit 0\n", marker.display()),
    )
    .expect("write the would-be target");
    BoundarySpawnSpec {
        executable: target,
        arguments: vec![],
        working_directory: dir.to_path_buf(),
        environment: BTreeMap::new(),
        stdin: StdinMode::Null,
    }
}

#[tokio::test]
async fn the_real_backend_is_wire_compatible_and_rejected_as_not_fully_enforced() {
    let dir = tempfile::tempdir().unwrap();
    let marker = dir.path().join("target-ran");
    let target = touch_target(dir.path(), &marker);

    let boundary =
        SandboyBoundary::new(real_backend(), policy_for(dir.path())).expect("valid boundary");
    let err = boundary
        .spawn(target)
        .await
        .expect_err("an honestly-downgraded report must be rejected, target withheld");

    // The rejection is an EVIDENCE failure — not a cleanup/Signal failure — which also proves the
    // spawned backend group was reaped and drained (no surviving monitor/descendant).
    assert!(
        matches!(err, BoundaryError::Evidence(_)),
        "expected an evidence rejection, got {err:?}"
    );
    // And specifically NotFullyEnforced: every binding matched (real wire compatibility); only the
    // enforcement was honestly absent. Pin the path UNAMBIGUOUSLY to the typed variant rather than
    // hunting for two lucky words — `spawn` stringifies `SandboyLaunchError` into
    // `BoundaryError::Evidence`, so rebuild the exact expected message from the same types. A
    // binding mismatch (a different `SandboyLaunchError`) would not equal this string.
    let expected =
        BoundaryError::Evidence(SandboyLaunchError::NotFullyEnforced.to_string()).to_string();
    assert_eq!(
        err.to_string(),
        expected,
        "the rejection must be exactly the NotFullyEnforced path (every binding matched)"
    );
    // The honest bootstrap ran nothing.
    assert!(
        !marker.exists(),
        "the target must never run on a downgraded report"
    );
}

#[test]
fn acquiring_the_backend_with_a_wrong_digest_fails_closed() {
    // The backend object is trust-bound by digest: a mismatched expected digest (a substitution /
    // TOCTOU) fails closed at acquisition, before any spawn.
    let path = real_backend_path();
    let err = BackendImage::acquire(
        &path,
        Digest256::of_bytes(b"not the real backend bytes"),
        backend_identity(),
    )
    .expect_err("a wrong backend digest must fail closed");
    assert!(
        matches!(err, SandboyLaunchError::BackendObjectMismatch),
        "expected BackendObjectMismatch, got {err:?}"
    );
}
