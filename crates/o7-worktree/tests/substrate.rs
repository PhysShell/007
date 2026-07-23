//! Adversarial tests for the worktree substrate (PR 3, slice 1).
//!
//! Proven here:
//!   * a dirty source checkout is never read — only committed bytes are materialized,
//!     and the operator's working copy is left untouched;
//!   * no repository-controlled helper runs (hook / smudge filter / fsmonitor);
//!   * symlink substitution and rename/inode replacement of the worktree fail closed,
//!     so deletion is refused and the files are preserved for investigation;
//!   * the state root and every worktree are owner-only and live outside the repo.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod support;

use o7_worktree::{CleanupOutcome, Worktree};
use support::*;

#[test]
fn dirty_source_checkout_unchanged_and_uncommitted_bytes_absent() {
    let repo = TestRepo::init();
    repo.write("a.txt", b"committed\n");
    repo.add_all();
    let head = repo.commit("c1");

    // Make the operator checkout DIRTY: modify the tracked file and add an untracked
    // one. Neither is committed, so neither may appear in the agent's worktree.
    repo.write("a.txt", b"DIRTY-uncommitted\n");
    repo.write("untracked.txt", b"never-committed\n");

    let (_root_dir, sr) = state_root();
    let wt = Worktree::create(&repo.hardened(), &sr, run_id("run1"), &head).unwrap();

    // The agent's copy is the COMMITTED bytes, not the dirty ones.
    assert_eq!(read(&wt.path().join("a.txt")), b"committed\n");
    // The uncommitted, untracked file is absent.
    assert!(!wt.path().join("untracked.txt").exists());

    // The operator's checkout is untouched by materialization.
    assert_eq!(read(&repo.path().join("a.txt")), b"DIRTY-uncommitted\n");
    assert!(repo.path().join("untracked.txt").exists());
}

#[test]
fn hooks_filters_and_fsmonitor_do_not_run() {
    let repo = TestRepo::init();
    let sentinels = tempfile::tempdir().unwrap();
    let hook_sentinel = sentinels.path().join("post_checkout_ran");
    let smudge_sentinel = sentinels.path().join("smudge_ran");
    let fsmon_sentinel = sentinels.path().join("fsmonitor_ran");

    // A post-checkout hook that would fire on a real checkout.
    let hook = repo.path().join(".git/hooks/post-checkout");
    std::fs::write(
        &hook,
        format!("#!/bin/sh\ntouch {}\n", hook_sentinel.display()),
    )
    .unwrap();
    std::fs::set_permissions(&hook, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();

    // A smudge filter that would fire on checkout of `secret`.
    repo.write(".gitattributes", b"secret filter=evil\n");
    repo.write("secret", b"payload\n");
    repo.git(&[
        "config",
        "filter.evil.smudge",
        &format!("sh -c 'touch {}'", smudge_sentinel.display()),
    ]);
    repo.add_all();
    let head = repo.commit("c1");

    // The attribute really is wired (so absence of the sentinel is meaningful).
    let attr = repo.git(&["check-attr", "filter", "--", "secret"]);
    assert!(attr.contains("filter: evil"), "attr not wired: {attr}");

    // An fsmonitor that would fire on any index-refreshing command.
    repo.git(&[
        "config",
        "core.fsmonitor",
        &format!("touch {}", fsmon_sentinel.display()),
    ]);

    let (_root_dir, sr) = state_root();
    let wt = Worktree::create(&repo.hardened(), &sr, run_id("run1"), &head).unwrap();

    // The committed bytes are present verbatim (no smudge transformation, no filter).
    assert_eq!(read(&wt.path().join("secret")), b"payload\n");
    // NONE of the repository-controlled helpers ran.
    assert!(!hook_sentinel.exists(), "post-checkout hook ran");
    assert!(!smudge_sentinel.exists(), "smudge filter ran");
    assert!(!fsmon_sentinel.exists(), "fsmonitor ran");
}

#[test]
fn materializes_symlinks_and_exec_bits_faithfully() {
    let repo = TestRepo::init();
    repo.write("plain.txt", b"plain\n");
    repo.write_exec("bin/run.sh", b"#!/bin/sh\necho hi\n");
    repo.symlink("link", "plain.txt");
    repo.add_all();
    let head = repo.commit("c1");

    let (_root_dir, sr) = state_root();
    let wt = Worktree::create(&repo.hardened(), &sr, run_id("run1"), &head).unwrap();

    assert_eq!(mode_of(&wt.path().join("bin/run.sh")), 0o755);
    assert_eq!(mode_of(&wt.path().join("plain.txt")), 0o644);
    let link = wt.path().join("link");
    let meta = std::fs::symlink_metadata(&link).unwrap();
    assert!(meta.file_type().is_symlink());
    assert_eq!(
        std::fs::read_link(&link).unwrap(),
        std::path::PathBuf::from("plain.txt")
    );

    assert_eq!(wt.summary().files, 2);
    assert_eq!(wt.summary().symlinks, 1);
}

#[test]
fn gitlink_submodule_pointer_is_skipped_not_materialized() {
    let repo = TestRepo::init();
    repo.write("a.txt", b"a\n");
    repo.add_all();
    let head = repo.commit("c1");

    // Craft a gitlink (submodule pointer) in the index without any real submodule, then
    // commit it. Its bytes live in another repository, so it must NOT be materialized.
    repo.git(&[
        "update-index",
        "--add",
        "--cacheinfo",
        &format!("160000,{},sub", head.as_str()),
    ]);
    let head2 = repo.commit("add gitlink");

    let (_root_dir, sr) = state_root();
    let wt = Worktree::create(&repo.hardened(), &sr, run_id("run1"), &head2).unwrap();
    assert!(!wt.path().join("sub").exists(), "gitlink was materialized");
    assert_eq!(wt.summary().skipped_gitlinks, 1);
}

#[test]
fn symlink_substitution_fails_closed() {
    let repo = TestRepo::init();
    repo.write("a.txt", b"a\n");
    repo.add_all();
    let head = repo.commit("c1");

    let (_root_dir, sr) = state_root();
    let wt = Worktree::create(&repo.hardened(), &sr, run_id("run1"), &head).unwrap();
    let path = wt.path().to_path_buf();

    // An attacker replaces the worktree directory with a symlink pointing elsewhere.
    let elsewhere = tempfile::tempdir().unwrap();
    std::fs::remove_dir_all(&path).unwrap();
    std::os::unix::fs::symlink(elsewhere.path(), &path).unwrap();

    // Attestation fails, and cleanup refuses to delete — the symlink (and its target)
    // are preserved for investigation.
    assert!(wt.attest().is_err());
    match wt.cleanup().unwrap() {
        CleanupOutcome::PreservedForInvestigation(_) => {}
        other => panic!("expected preservation, got {other:?}"),
    }
    assert!(std::fs::symlink_metadata(&path)
        .unwrap()
        .file_type()
        .is_symlink());
    assert!(
        elsewhere.path().exists(),
        "cleanup followed the symlink and deleted the target"
    );
}

#[test]
fn rename_inode_replacement_fails_closed() {
    let repo = TestRepo::init();
    repo.write("a.txt", b"a\n");
    repo.add_all();
    let head = repo.commit("c1");

    let (_root_dir, sr) = state_root();
    let wt = Worktree::create(&repo.hardened(), &sr, run_id("run1"), &head).unwrap();
    let path = wt.path().to_path_buf();

    // Move our directory aside and drop a DIFFERENT directory (new inode) at the path.
    use std::os::unix::fs::DirBuilderExt as _;
    let aside = path.with_extension("aside");
    std::fs::rename(&path, &aside).unwrap();
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(&path)
        .unwrap();

    // The recorded (dev,ino) no longer match — attestation fails closed.
    assert!(wt.attest().is_err());
    match wt.cleanup().unwrap() {
        CleanupOutcome::PreservedForInvestigation(_) => {}
        other => panic!("expected preservation, got {other:?}"),
    }
    // The impostor directory is still there (not deleted).
    assert!(path.exists());
}

#[test]
fn state_root_and_worktree_are_owner_only() {
    let repo = TestRepo::init();
    repo.write("a.txt", b"a\n");
    repo.add_all();
    let head = repo.commit("c1");

    let (_root_dir, sr) = state_root();
    assert_eq!(mode_of(sr.path()), 0o700);
    assert_eq!(owner_of(sr.path()), our_uid());

    let wt = Worktree::create(&repo.hardened(), &sr, run_id("run1"), &head).unwrap();
    assert_eq!(mode_of(wt.path()), 0o700);
    assert_eq!(owner_of(wt.path()), our_uid());
}

#[test]
fn create_rejects_a_state_root_inside_the_repo() {
    let repo = TestRepo::init();
    repo.write("a.txt", b"a\n");
    repo.add_all();
    let head = repo.commit("c1");

    // A state root nested inside the repo working tree must be rejected.
    let inside = repo.path().join("nested-state");
    let sr = o7_worktree::StateRoot::open_or_create(&inside).unwrap();
    let err = Worktree::create(&repo.hardened(), &sr, run_id("run1"), &head).unwrap_err();
    assert!(
        matches!(err, o7_worktree::WorktreeError::StateRoot(_)),
        "expected InsideRepo rejection, got {err:?}"
    );
}

#[test]
fn cleanup_removes_when_identity_is_proven() {
    let repo = TestRepo::init();
    repo.write("a.txt", b"a\n");
    repo.add_all();
    let head = repo.commit("c1");

    let (_root_dir, sr) = state_root();
    let wt = Worktree::create(&repo.hardened(), &sr, run_id("run1"), &head).unwrap();
    let path = wt.path().to_path_buf();
    assert_eq!(wt.cleanup().unwrap(), CleanupOutcome::Removed);
    assert!(!path.exists());
}

#[test]
fn duplicate_create_for_the_same_identity_refuses() {
    let repo = TestRepo::init();
    repo.write("a.txt", b"a\n");
    repo.add_all();
    let head = repo.commit("c1");

    let (_root_dir, sr) = state_root();
    let _wt = Worktree::create(&repo.hardened(), &sr, run_id("run1"), &head).unwrap();
    // The path is derived from the identity digest, so a second create for the SAME
    // (run, repo, revision) refuses rather than clobbering.
    let err = Worktree::create(&repo.hardened(), &sr, run_id("run1"), &head).unwrap_err();
    assert!(matches!(err, o7_worktree::WorktreeError::AlreadyExists(_)));
}
