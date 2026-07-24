//! Adversarial tests for the boundary-integrated verifier runner (PR 3, slice 4).
//!
//! Proven here:
//!   * production requires a `FullyEnforced` boundary and NEVER falls back — an
//!     unconfined boundary is refused before spawn;
//!   * an untrusted command is not run;
//!   * spawn failure, timeout, and output loss are each non-completions (never a pass),
//!     and a timeout force-kills the whole owned process set;
//!   * a clean, trusted, fully-enforced run is a pass candidate that o7d accepts — but
//!     only o7d's `adjudicate` accepts it;
//!   * end to end against the REAL host boundary, a timeout kills the real process.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod common;

use std::time::Duration;

use common::*;
use o7_verifier::{
    adjudicate, AttestedEnforcement, RequiredBoundary, TrustStore, Verifier, VerifierOutcome,
};
use o7_worker::{BoundaryRequirement, EnforcementLevel, UnconfinedHostBoundary};

fn worktree_root() -> tempfile::TempDir {
    tempfile::tempdir().unwrap()
}

#[tokio::test]
async fn production_accepts_a_clean_run_under_a_fully_enforced_boundary() {
    let exe_dir = tempfile::tempdir().unwrap();
    let root = worktree_root();
    let repo = repo_id();
    let cmd = command_for(
        make_exe(exe_dir.path(), "v"),
        Duration::from_secs(5),
        1 << 20,
    );
    let trust = trust_for(&repo, &cmd);
    let boundary = FakeBoundary::fully_enforced().exit_code(0);

    let ev = Verifier::production()
        .verify(boundary.boxed(), &repo, root.path(), &cmd, &trust)
        .await;

    assert_eq!(ev.outcome, VerifierOutcome::Completed { exit_code: 0 });
    assert_eq!(
        ev.boundary_enforcement,
        Some(AttestedEnforcement::FullyEnforced)
    );
    assert!(ev.trusted);
    // Only o7d accepts — and it does, given trust + full enforcement + clean exit.
    assert!(adjudicate(&ev, &trust).is_accepted());
}

#[tokio::test]
async fn production_refuses_an_unconfined_boundary_with_no_fallback() {
    let exe_dir = tempfile::tempdir().unwrap();
    let root = worktree_root();
    let repo = repo_id();
    let cmd = command_for(
        make_exe(exe_dir.path(), "v"),
        Duration::from_secs(5),
        1 << 20,
    );
    let trust = trust_for(&repo, &cmd);
    // Honestly attests less than FullyEnforced.
    let boundary = FakeBoundary::with_enforcement(EnforcementLevel::None);
    let state = boundary.state();

    let ev = Verifier::production()
        .verify(boundary.boxed(), &repo, root.path(), &cmd, &trust)
        .await;

    assert!(matches!(
        ev.outcome,
        VerifierOutcome::BoundaryUnavailable { .. }
    ));
    // Nothing was spawned — fail closed BEFORE spawn, no substitute boundary.
    assert!(
        !state.spawn_entered(),
        "a process was spawned under an unconfined boundary"
    );
    assert!(!adjudicate(&ev, &trust).is_accepted());
}

#[tokio::test]
async fn an_untrusted_command_is_not_run() {
    let exe_dir = tempfile::tempdir().unwrap();
    let root = worktree_root();
    let repo = repo_id();
    let cmd = command_for(
        make_exe(exe_dir.path(), "v"),
        Duration::from_secs(5),
        1 << 20,
    );
    let empty_trust = o7_verifier::TrustStore::new();
    let boundary = FakeBoundary::fully_enforced().exit_code(0);
    let state = boundary.state();

    let ev = Verifier::production()
        .verify(boundary.boxed(), &repo, root.path(), &cmd, &empty_trust)
        .await;

    assert!(matches!(ev.outcome, VerifierOutcome::NotRun { .. }));
    assert!(!ev.trusted);
    assert!(!state.spawn_entered(), "an untrusted command was spawned");
}

#[tokio::test]
async fn spawn_failure_is_not_a_pass() {
    let exe_dir = tempfile::tempdir().unwrap();
    let root = worktree_root();
    let repo = repo_id();
    let cmd = command_for(
        make_exe(exe_dir.path(), "v"),
        Duration::from_secs(5),
        1 << 20,
    );
    let trust = trust_for(&repo, &cmd);
    let boundary = FakeBoundary::fully_enforced().spawn_failure("no exec for you");

    let ev = Verifier::production()
        .verify(boundary.boxed(), &repo, root.path(), &cmd, &trust)
        .await;

    assert!(matches!(ev.outcome, VerifierOutcome::SpawnFailed { .. }));
    assert!(!adjudicate(&ev, &trust).is_accepted());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn timeout_force_kills_the_whole_process_set_and_is_not_a_pass() {
    let exe_dir = tempfile::tempdir().unwrap();
    let root = worktree_root();
    let repo = repo_id();
    // A short timeout; the leader never exits on its own and has 3 same-group members.
    let cmd = command_for(
        make_exe(exe_dir.path(), "v"),
        Duration::from_millis(300),
        1 << 20,
    );
    let trust = trust_for(&repo, &cmd);
    let boundary = FakeBoundary::fully_enforced().live_with_members(3);
    let state = boundary.state();

    let ev = Verifier::production()
        .verify(boundary.boxed(), &repo, root.path(), &cmd, &trust)
        .await;

    assert_eq!(ev.outcome, VerifierOutcome::TimedOut);
    // The whole owned set was force-killed (members only drain on FORCE).
    assert!(
        state.force_stops() >= 1,
        "the process set was not force-killed"
    );
    assert!(!adjudicate(&ev, &trust).is_accepted());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn output_loss_is_not_a_pass() {
    let exe_dir = tempfile::tempdir().unwrap();
    let root = worktree_root();
    let repo = repo_id();
    // A tiny output budget against an endlessly-writing process.
    let cmd = command_for(make_exe(exe_dir.path(), "v"), Duration::from_secs(5), 1024);
    let trust = trust_for(&repo, &cmd);
    let boundary = FakeBoundary::fully_enforced().infinite_stdout();

    let ev = Verifier::production()
        .verify(boundary.boxed(), &repo, root.path(), &cmd, &trust)
        .await;

    assert!(matches!(ev.outcome, VerifierOutcome::OutputLost { .. }));
    assert!(!adjudicate(&ev, &trust).is_accepted());
}

#[tokio::test]
async fn a_nonzero_exit_completes_but_o7d_rejects() {
    let exe_dir = tempfile::tempdir().unwrap();
    let root = worktree_root();
    let repo = repo_id();
    let cmd = command_for(
        make_exe(exe_dir.path(), "v"),
        Duration::from_secs(5),
        1 << 20,
    );
    let trust = trust_for(&repo, &cmd);
    let boundary = FakeBoundary::fully_enforced().exit_code(1);

    let ev = Verifier::production()
        .verify(boundary.boxed(), &repo, root.path(), &cmd, &trust)
        .await;

    assert_eq!(ev.outcome, VerifierOutcome::Completed { exit_code: 1 });
    assert!(!ev.is_pass_candidate());
    assert!(!adjudicate(&ev, &trust).is_accepted());
}

// ---- hardened executable acquisition (item 7): only a regular file is ever read ----

/// A symlinked executable path is refused (O_NOFOLLOW): the exact bytes must be a real
/// regular file, not a symlink the operator path could redirect.
#[tokio::test]
async fn a_symlinked_executable_is_refused() {
    let exe_dir = tempfile::tempdir().unwrap();
    let real = make_exe(exe_dir.path(), "real");
    let link = exe_dir.path().join("link");
    std::os::unix::fs::symlink(&real, &link).unwrap();

    let repo = repo_id();
    let cmd = command_for(link, Duration::from_secs(5), 1 << 20);
    // TRUST the command (its identity resolved through the link's target), so the ONLY
    // remaining rejection cause is acquisition — a NotRun here proves acquisition rejected
    // the symlink, not that the command was untrusted.
    let trust = trust_for(&repo, &cmd);
    let boundary = FakeBoundary::fully_enforced().exit_code(0);
    let state = boundary.state();

    let ev = Verifier::production()
        .verify(
            boundary.boxed(),
            &repo,
            worktree_root().path(),
            &cmd,
            &trust,
        )
        .await;

    assert!(matches!(ev.outcome, VerifierOutcome::NotRun { .. }));
    assert!(!state.spawn_entered(), "a symlinked executable was spawned");
}

/// A device node (here `/dev/null`, a character device) is refused — never a regular file.
#[tokio::test]
async fn a_device_node_executable_is_refused() {
    let repo = repo_id();
    let cmd = command_for(
        std::path::PathBuf::from("/dev/null"),
        Duration::from_secs(5),
        1 << 20,
    );
    // Trust the command so acquisition is the sole rejection cause.
    let trust = trust_for(&repo, &cmd);
    let boundary = FakeBoundary::fully_enforced().exit_code(0);
    let state = boundary.state();

    let ev = Verifier::production()
        .verify(
            boundary.boxed(),
            &repo,
            worktree_root().path(),
            &cmd,
            &trust,
        )
        .await;

    assert!(matches!(ev.outcome, VerifierOutcome::NotRun { .. }));
    assert!(!state.spawn_entered(), "a device node was spawned");
}

/// A procfs pseudo-file (`/proc/self/status`, a regular file reporting size 0) is refused:
/// a real executable has a positive, bounded size that matches the bytes it yields.
#[tokio::test]
async fn a_procfs_pseudo_file_executable_is_refused() {
    if !std::path::Path::new("/proc/self/status").exists() {
        return; // no procfs on this host; nothing to prove.
    }
    let repo = repo_id();
    let cmd = command_for(
        std::path::PathBuf::from("/proc/self/status"),
        Duration::from_secs(5),
        1 << 20,
    );
    // Trust the command so acquisition is the sole rejection cause.
    let trust = trust_for(&repo, &cmd);
    let boundary = FakeBoundary::fully_enforced().exit_code(0);
    let state = boundary.state();

    let ev = Verifier::production()
        .verify(
            boundary.boxed(),
            &repo,
            worktree_root().path(),
            &cmd,
            &trust,
        )
        .await;

    assert!(matches!(ev.outcome, VerifierOutcome::NotRun { .. }));
    assert!(!state.spawn_entered(), "a procfs pseudo-file was spawned");
}

// ---- end to end against the REAL host boundary (lifecycle only, no isolation) ----

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn real_host_boundary_clean_run() {
    let root = worktree_root();
    let repo = repo_id();
    // /bin/true always exists and exits 0.
    let mut cmd = command_for_boundary(
        std::path::PathBuf::from("/bin/true"),
        Duration::from_secs(10),
        1 << 20,
        RequiredBoundary::AllowUnconfined,
    );
    cmd.arguments.clear();
    let trust = trust_for(&repo, &cmd);

    // Tooling path: an unconfined boundary is allowed ONLY under AllowUnconfined.
    let ev = Verifier::with_requirement(BoundaryRequirement::AllowUnconfined)
        .verify(
            Box::new(UnconfinedHostBoundary),
            &repo,
            root.path(),
            &cmd,
            &trust,
        )
        .await;

    assert_eq!(ev.outcome, VerifierOutcome::Completed { exit_code: 0 });
    assert!(adjudicate(&ev, &trust).is_accepted());
    // The digest is what gates: against an empty store (as after a revocation) the very
    // same evidence is rejected — o7d's store is the sole authority.
    assert!(!adjudicate(&ev, &TrustStore::new()).is_accepted());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn real_host_boundary_timeout_kills_the_real_process() {
    let root = worktree_root();
    let repo = repo_id();
    // /bin/sleep runs far longer than the timeout; the supervisor must kill it.
    let mut cmd = command_for_boundary(
        std::path::PathBuf::from("/bin/sleep"),
        Duration::from_millis(400),
        1 << 20,
        RequiredBoundary::AllowUnconfined,
    );
    cmd.arguments = vec![std::ffi::OsString::from("30")];
    let trust = trust_for(&repo, &cmd);

    let ev = Verifier::with_requirement(BoundaryRequirement::AllowUnconfined)
        .verify(
            Box::new(UnconfinedHostBoundary),
            &repo,
            root.path(),
            &cmd,
            &trust,
        )
        .await;

    assert_eq!(ev.outcome, VerifierOutcome::TimedOut);
    assert!(!adjudicate(&ev, &trust).is_accepted());
}

// The runner executes the staged bytes FD-EXACTLY, through a /proc/<pid>/fd/<n> path
// backed by a held-open descriptor to the staged inode — so the bytes hashed for trust
// are the bytes that run, immune to a directory-entry swap (no hash-to-spawn TOCTOU). A
// shell fixture that prints its own argv[0] proves it ran through the /proc fd path, not
// the operator path.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_executed_binary_is_the_fd_exact_staged_copy() {
    use std::os::unix::fs::PermissionsExt as _;
    let exe_dir = tempfile::tempdir().unwrap();
    let exe = exe_dir.path().join("verify.sh");
    std::fs::write(&exe, b"#!/bin/sh\nprintf '%s' \"$0\"\n").unwrap();
    std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).unwrap();

    let root = worktree_root();
    let repo = repo_id();
    let mut cmd = command_for_boundary(
        exe.clone(),
        Duration::from_secs(10),
        1 << 20,
        RequiredBoundary::AllowUnconfined,
    );
    cmd.arguments.clear();
    let trust = trust_for(&repo, &cmd);

    let ev = Verifier::with_requirement(BoundaryRequirement::AllowUnconfined)
        .verify(
            Box::new(UnconfinedHostBoundary),
            &repo,
            root.path(),
            &cmd,
            &trust,
        )
        .await;

    assert_eq!(ev.outcome, VerifierOutcome::Completed { exit_code: 0 });
    let printed = String::from_utf8_lossy(&ev.stdout);
    // argv[0] is the /proc/<pid>/fd/<n> path — proof of fd-exact execution through the
    // held descriptor, not a re-resolvable operator path.
    assert!(
        printed.starts_with("/proc/") && printed.contains("/fd/"),
        "expected an fd-exact /proc/<pid>/fd/<n> exec path, got {printed:?}"
    );
    assert!(
        !printed.contains(exe.to_str().unwrap()),
        "ran the operator path instead of the fd-exact staged copy: {printed:?}"
    );
}
