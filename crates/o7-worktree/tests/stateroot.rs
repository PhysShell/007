//! Adversarial tests for the descriptor-bound state root (PR 3, item 2).
//!
//! Proven here: once the state root is bound to a descriptor, none of these substitutions
//! can redirect an operation at a victim —
//!   * an ANCESTOR directory swapped for a symlink after binding;
//!   * a `.lock` control file replaced by a symlink;
//!   * the state file replaced by a symlink (the atomic write never follows it);
//!   * the state root directory RENAMED out from under the descriptor.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use o7_worktree::{StateRoot, StateRootError};

#[test]
fn an_ancestor_swapped_for_a_symlink_after_binding_cannot_redirect_writes() {
    let base = tempfile::tempdir().unwrap();
    let mid = base.path().join("mid");
    std::fs::create_dir(&mid).unwrap();
    let root = mid.join("root");
    let sr = StateRoot::open_or_create(&root).unwrap();

    // After binding, replace the ANCESTOR `mid` with a symlink to a victim directory.
    let victim = base.path().join("victim");
    std::fs::create_dir(&victim).unwrap();
    let mid_real = base.path().join("mid_real");
    std::fs::rename(&mid, &mid_real).unwrap();
    std::os::unix::fs::symlink(&victim, &mid).unwrap();

    // Operations still act on the REAL root inode via the bound fd, not the victim.
    sr.write_state_file_atomic("worktrees.json", b"payload")
        .unwrap();
    assert_eq!(
        sr.read_state_file("worktrees.json").unwrap().unwrap(),
        b"payload"
    );

    assert!(
        mid_real.join("root/worktrees.json").exists(),
        "the write did not land in the real root"
    );
    assert!(
        std::fs::read_dir(&victim).unwrap().next().is_none(),
        "the victim directory was written through the ancestor symlink"
    );
}

#[test]
fn a_lock_symlink_fails_closed_and_leaves_the_victim_untouched() {
    let base = tempfile::tempdir().unwrap();
    let root = base.path().join("root");
    let sr = StateRoot::open_or_create(&root).unwrap();

    let victim = base.path().join("victim");
    std::fs::write(&victim, b"victim-untouched").unwrap();
    std::os::unix::fs::symlink(&victim, root.join(".lock")).unwrap();

    let err = sr.open_lock_file().unwrap_err();
    assert!(
        matches!(err, StateRootError::Symlink { .. }),
        "expected Symlink, got {err:?}"
    );
    assert_eq!(std::fs::read(&victim).unwrap(), b"victim-untouched");
}

#[test]
fn a_state_file_symlink_is_replaced_not_followed() {
    let base = tempfile::tempdir().unwrap();
    let root = base.path().join("root");
    let sr = StateRoot::open_or_create(&root).unwrap();

    // Plant a symlink at the final state-file name, pointing at a victim.
    let victim = base.path().join("victim");
    std::fs::write(&victim, b"victim-untouched").unwrap();
    std::os::unix::fs::symlink(&victim, root.join("worktrees.json")).unwrap();

    // The atomic write goes through a fresh O_EXCL temp + renameat, so it REPLACES the
    // symlink with a real file and never writes through it to the victim.
    sr.write_state_file_atomic("worktrees.json", b"payload")
        .unwrap();
    assert_eq!(std::fs::read(&victim).unwrap(), b"victim-untouched");
    let meta = std::fs::symlink_metadata(root.join("worktrees.json")).unwrap();
    assert!(
        meta.file_type().is_file(),
        "the state file is still a symlink after the write"
    );
    assert_eq!(
        sr.read_state_file("worktrees.json").unwrap().unwrap(),
        b"payload"
    );
}

#[test]
fn a_renamed_state_root_keeps_working_through_the_bound_descriptor() {
    let base = tempfile::tempdir().unwrap();
    let root = base.path().join("root");
    let sr = StateRoot::open_or_create(&root).unwrap();
    sr.write_state_file_atomic("worktrees.json", b"before")
        .unwrap();

    // Rename the whole state root away; the bound fd still addresses the same inode.
    let moved = base.path().join("root_moved");
    std::fs::rename(&root, &moved).unwrap();

    sr.write_state_file_atomic("worktrees.json", b"after")
        .unwrap();
    assert_eq!(
        sr.read_state_file("worktrees.json").unwrap().unwrap(),
        b"after"
    );
    assert_eq!(
        std::fs::read(moved.join("worktrees.json")).unwrap(),
        b"after"
    );
}
