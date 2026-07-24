//! Adversarial tests for race-safe worktree removal (PR 3 rework: path-based cleanup
//! race).
//!
//! Proven here:
//!   * a nested owned tree is fully removed, and an in-tree symlink is unlinked WITHOUT
//!     following it (its target is untouched);
//!   * a wrong recorded identity is fail-closed (Unproven) and deletes nothing;
//!   * a symlink substituted for the directory is never followed (Unproven), and the
//!     symlink's target is untouched.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::os::unix::fs::DirBuilderExt as _;
use std::path::Path;

use o7_worktree::{remove_verified_dir, FsIdentity, ReapError};

fn owned_dir(path: &Path) {
    std::fs::DirBuilder::new().mode(0o700).create(path).unwrap();
}

#[test]
fn removes_a_nested_owned_tree_without_following_internal_symlinks() {
    let tmp = tempfile::tempdir().unwrap();
    let wt = tmp.path().join("wt");
    owned_dir(&wt);
    owned_dir(&wt.join("a"));
    owned_dir(&wt.join("a/b"));
    std::fs::write(wt.join("a/b/file.txt"), b"deep").unwrap();
    std::fs::write(wt.join("top.txt"), b"top").unwrap();

    // A precious external file, and an in-tree symlink pointing at it. Removal must NOT
    // follow the symlink and delete the target.
    let victim = tmp.path().join("victim.txt");
    std::fs::write(&victim, b"keep me").unwrap();
    std::os::unix::fs::symlink(&victim, wt.join("a/escape")).unwrap();

    let id = FsIdentity::of_dir(&wt).unwrap();
    remove_verified_dir(&wt, id).unwrap();

    assert!(!wt.exists(), "the worktree tree was not fully removed");
    assert!(
        victim.exists(),
        "removal followed an in-tree symlink and deleted its target"
    );
}

#[test]
fn wrong_recorded_identity_deletes_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let wt = tmp.path().join("wt");
    owned_dir(&wt);
    std::fs::write(wt.join("keep.txt"), b"x").unwrap();

    // A bogus identity cannot match the real dir → fail closed, nothing deleted.
    let bogus = FsIdentity {
        dev: 1,
        ino: 999_999_999,
    };
    match remove_verified_dir(&wt, bogus) {
        Err(ReapError::Unproven { .. }) => {}
        other => panic!("expected Unproven, got {other:?}"),
    }
    assert!(
        wt.join("keep.txt").exists(),
        "an unproven dir was partly deleted"
    );
}

#[test]
fn symlink_substituted_for_the_directory_is_not_followed() {
    let tmp = tempfile::tempdir().unwrap();
    // A real directory with a precious file that must survive.
    let real = tmp.path().join("real");
    owned_dir(&real);
    std::fs::write(real.join("precious.txt"), b"keep").unwrap();

    // The "worktree" path is a symlink to that real directory.
    let wt = tmp.path().join("wt");
    std::os::unix::fs::symlink(&real, &wt).unwrap();

    // Any identity: the open must fail closed on the symlink (O_NOFOLLOW), never
    // following it into `real`.
    let bogus = FsIdentity { dev: 1, ino: 2 };
    match remove_verified_dir(&wt, bogus) {
        Err(ReapError::Unproven { .. }) => {}
        other => panic!("expected Unproven, got {other:?}"),
    }
    assert!(
        real.join("precious.txt").exists(),
        "removal followed the substituted symlink"
    );
    assert!(std::fs::symlink_metadata(&wt)
        .unwrap()
        .file_type()
        .is_symlink());
}

#[test]
fn an_already_absent_directory_is_vanished() {
    let tmp = tempfile::tempdir().unwrap();
    let wt = tmp.path().join("nope");
    let id = FsIdentity { dev: 1, ino: 2 };
    assert!(matches!(
        remove_verified_dir(&wt, id),
        Err(ReapError::Vanished(_))
    ));
}
