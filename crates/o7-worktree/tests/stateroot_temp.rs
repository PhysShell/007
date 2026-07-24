//! Dedicated single-test binary for the atomic-write temp-file guarantee (PR 3, item 2).
//!
//! The temp filename is derived from the process id and a process-global counter that
//! starts at zero. Because this file contains exactly ONE atomic write, the counter is
//! deterministically `0`, so the temp name is predictable — which lets us plant a symlink
//! at exactly that name and prove the `O_EXCL | O_NOFOLLOW` open refuses it (fails closed)
//! rather than following it to a victim.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use o7_worktree::{StateRoot, StateRootError};

#[test]
fn a_symlink_planted_at_the_temp_name_fails_the_atomic_write_closed() {
    let base = tempfile::tempdir().unwrap();
    let root = base.path().join("root");
    let sr = StateRoot::open_or_create(&root).unwrap();

    // First and only atomic write in this binary ⇒ temp counter is 0.
    let tmp_name = format!(".worktrees.json.tmp.{}.0", std::process::id());
    let victim = base.path().join("victim");
    std::fs::write(&victim, b"victim-untouched").unwrap();
    std::os::unix::fs::symlink(&victim, root.join(&tmp_name)).unwrap();

    let err = sr
        .write_state_file_atomic("worktrees.json", b"payload")
        .unwrap_err();
    assert!(
        matches!(err, StateRootError::Io { .. }),
        "expected the O_EXCL temp open to fail closed, got {err:?}"
    );
    assert_eq!(
        std::fs::read(&victim).unwrap(),
        b"victim-untouched",
        "the atomic write followed the temp symlink to the victim"
    );
    assert!(
        sr.read_state_file("worktrees.json").unwrap().is_none(),
        "a state file was written despite the temp collision"
    );
}
