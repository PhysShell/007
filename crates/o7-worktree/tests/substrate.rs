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
fn produces_a_real_detached_git_worktree_without_touching_the_source() {
    let repo = TestRepo::init();
    repo.write("a.txt", b"committed\n");
    repo.write("dir/nested.txt", b"nested\n");
    repo.add_all();
    let head = repo.commit("c1");

    let (_root_dir, sr) = state_root();
    let wt = Worktree::create(&repo.hardened(), &sr, run_id("run1"), &head).unwrap();

    // It is a real git worktree: a .git exists, HEAD is DETACHED exactly at the
    // committed revision, and `git status` is clean (working tree matches the tree).
    assert!(
        wt.path().join(".git").exists(),
        "no .git — not a real worktree"
    );
    let wt_git = o7_worktree::HardenedGit::new(wt.path());
    assert_eq!(wt_git.resolve_commit("HEAD").unwrap(), head);

    // Clean working tree (run plain git in the worktree with an isolated HOME).
    let status = std::process::Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(wt.path())
        .env("HOME", repo.home.path())
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap();
    assert!(status.status.success());
    assert!(
        status.stdout.is_empty(),
        "worktree is not clean: {:?}",
        String::from_utf8_lossy(&status.stdout)
    );

    // The source repository is untouched: no linked-worktree admin entry was created in
    // it (this substrate keeps everything self-contained under the state root).
    assert!(
        !repo.path().join(".git/worktrees").exists(),
        "an admin entry was added to the source repo's .git/worktrees"
    );
}

#[test]
fn a_committed_replace_ref_does_not_rewrite_the_materialized_bytes() {
    // A repository can carry a `refs/replace/<oid>` ref that silently swaps one object
    // for another whenever git honors it. The substrate pins GIT_NO_REPLACE_OBJECTS=1, so
    // materialization must read the ORIGINAL committed bytes, never the replacement.
    let repo = TestRepo::init();
    repo.write("a.txt", b"original\n");
    repo.add_all();
    let head = repo.commit("c1");

    // The blob actually committed, and a different replacement blob.
    let orig_blob = repo.git(&["rev-parse", "HEAD:a.txt"]);
    let orig_blob = orig_blob.trim();
    let repl_blob = repo.git_stdin(&["hash-object", "-w", "--stdin"], b"REPLACED-AND-LONGER\n");
    repo.git(&["replace", orig_blob, &repl_blob]);

    // Sanity: with replace honored (plain git), the original oid now reads as the
    // replacement — proving the ref is live and would leak if we honored it.
    let leaked = repo.git(&["cat-file", "blob", orig_blob]);
    assert_eq!(leaked, "REPLACED-AND-LONGER\n");

    let (_root_dir, sr) = state_root();
    let wt = Worktree::create(&repo.hardened(), &sr, run_id("run1"), &head).unwrap();

    // The materialized file is the committed content, not the replacement.
    assert_eq!(read(&wt.path().join("a.txt")), b"original\n");
}

#[test]
fn the_worktree_is_self_contained_after_the_source_objects_vanish() {
    // The worktree copies the full object closure into its own objectdb and drops the
    // alternates tether, so it must keep working as a real git repo even if the source
    // repository's objects are later deleted.
    let repo = TestRepo::init();
    repo.write("a.txt", b"one\n");
    repo.add_all();
    let _c1 = repo.commit("c1");
    repo.write("b.txt", b"two\n");
    repo.add_all();
    let head = repo.commit("c2");
    let a_oid = repo.git(&["rev-parse", "HEAD:a.txt"]).trim().to_owned();

    let (_root_dir, sr) = state_root();
    let wt = Worktree::create(&repo.hardened(), &sr, run_id("run1"), &head).unwrap();
    let wt_path = wt.path().to_owned();

    // No alternates file remains: the worktree borrows nothing.
    assert!(
        !wt_path.join(".git/objects/info/alternates").exists(),
        "alternates tether was not removed"
    );

    // Destroy the source object store entirely.
    std::fs::remove_dir_all(repo.path().join(".git")).unwrap();

    // A plain git in the worktree (isolated HOME, no borrowing) still resolves HEAD,
    // reports a clean tree, reads a blob, walks the full history, and can commit.
    let git = |args: &[&str]| -> (bool, String) {
        let out = std::process::Command::new("git")
            .args(args)
            .current_dir(&wt_path)
            .env("HOME", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .output()
            .unwrap();
        (
            out.status.success(),
            String::from_utf8_lossy(&out.stdout).trim().to_owned(),
        )
    };

    let (ok, rev) = git(&["rev-parse", "HEAD"]);
    assert!(ok, "rev-parse HEAD failed after source objects vanished");
    assert_eq!(rev, head.as_str());

    let (ok, status) = git(&["status", "--porcelain"]);
    assert!(ok && status.is_empty(), "worktree not clean: {status:?}");

    let (ok, blob) = git(&["cat-file", "blob", &a_oid]);
    assert!(ok && blob == "one", "blob read failed: {blob:?}");

    let (ok, log) = git(&["log", "--oneline"]);
    assert!(ok, "log failed after source objects vanished");
    assert_eq!(log.lines().count(), 2, "history incomplete: {log:?}");

    // A brand-new commit succeeds against the self-contained objectdb.
    std::fs::write(wt_path.join("c.txt"), b"three\n").unwrap();
    let (ok, _) = git(&["add", "c.txt"]);
    assert!(ok);
    let (ok, _) = git(&[
        "-c",
        "user.email=t@example.com",
        "-c",
        "user.name=Test",
        "commit",
        "-q",
        "-m",
        "c3",
    ]);
    assert!(ok, "commit failed against the self-contained worktree");
    let (ok, log) = git(&["log", "--oneline"]);
    assert!(
        ok && log.lines().count() == 3,
        "new commit missing: {log:?}"
    );
}

#[test]
fn reserved_git_path_is_rejected_and_nothing_is_written() {
    let repo = TestRepo::init();
    // Seed a normal commit so the repo has a HEAD, then craft the hostile tree.
    repo.write("seed.txt", b"seed\n");
    repo.add_all();
    let _ = repo.commit("seed");
    let evil = repo.commit_with_dotgit_hook(b"#!/bin/sh\ntouch /tmp/o7-pwned\n");

    let (root_dir, sr) = state_root();
    let err = Worktree::create(&repo.hardened(), &sr, run_id("run1"), &evil).unwrap_err();
    assert!(
        matches!(
            err,
            o7_worktree::WorktreeError::Materialize(
                o7_worktree::MaterializeError::ReservedGitPath { .. }
            )
        ),
        "expected ReservedGitPath, got {err:?}"
    );
    // Nothing was written: no worktree directory under the state root, so certainly no
    // hook file.
    let leftovers = absolute_paths_under(sr.path());
    assert!(
        leftovers.is_empty(),
        "a hostile tree left files behind: {leftovers:?}"
    );
    let _ = root_dir;
}

#[test]
fn oversized_and_unsupported_trees_fail_before_writing() {
    use o7_worktree::{MaterializeError, MaterializeLimits, MaterializePlan};
    let repo = TestRepo::init();
    repo.write("small.txt", b"0123456789\n"); // 11 bytes
    repo.add_all();
    let head = repo.commit("c1");
    let git = repo.hardened();

    // A per-blob budget below the file size fails the preflight (read-only, nothing
    // written).
    let tight = MaterializeLimits {
        max_blob_bytes: 4,
        ..MaterializeLimits::default()
    };
    assert!(matches!(
        MaterializePlan::prepare(&git, &head, &tight).unwrap_err(),
        MaterializeError::BlobTooLarge { .. }
    ));

    // A cumulative-byte budget below the total also fails.
    let tiny_total = MaterializeLimits {
        max_total_bytes: 3,
        ..MaterializeLimits::default()
    };
    assert!(matches!(
        MaterializePlan::prepare(&git, &head, &tiny_total).unwrap_err(),
        MaterializeError::TotalTooLarge { .. }
    ));

    // An entry-count budget of zero fails.
    let no_entries = MaterializeLimits {
        max_entries: 0,
        ..MaterializeLimits::default()
    };
    assert!(matches!(
        MaterializePlan::prepare(&git, &head, &no_entries).unwrap_err(),
        MaterializeError::TooManyEntries { .. }
    ));
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
fn create_rejects_a_state_root_inside_the_repo_when_git_is_bound_to_a_subdirectory() {
    // Git is bound to a SUBDIRECTORY of the checkout; a state root elsewhere inside the
    // SAME working tree (not under .git, not under the bound subdir) must still be rejected
    // via the resolved top-level working tree.
    let repo = TestRepo::init();
    repo.write("src/a.txt", b"a\n");
    repo.add_all();
    let head = repo.commit("c1");

    let subdir = repo.path().join("src");
    let git = o7_worktree::HardenedGit::new(&subdir);

    // A state root under the working-tree root, but NOT under `src/` or `.git/`.
    let inside = repo.path().join(".o7-state");
    let sr = o7_worktree::StateRoot::open_or_create(&inside).unwrap();

    let err = Worktree::create(&git, &sr, run_id("run1"), &head).unwrap_err();
    assert!(
        matches!(err, o7_worktree::WorktreeError::StateRoot(_)),
        "expected InsideRepo rejection via the top-level working tree, got {err:?}"
    );
}

#[test]
fn materializes_a_sha256_repository() {
    // A SHA-256 source repo yields 64-hex commit ids; the worktree gitdir must be created
    // with a matching object format or update-ref/read-tree would reject the id.
    let repo = TestRepo::init_with_object_format("sha256");
    repo.write("a.txt", b"sha256\n");
    repo.add_all();
    let head = repo.commit("c1");
    assert_eq!(
        head.as_str().len(),
        64,
        "expected a 64-hex sha256 commit id"
    );

    let (_root_dir, sr) = state_root();
    let wt = Worktree::create(&repo.hardened(), &sr, run_id("run1"), &head).unwrap();

    assert_eq!(read(&wt.path().join("a.txt")), b"sha256\n");
    let wt_git = o7_worktree::HardenedGit::new(wt.path());
    assert_eq!(wt_git.resolve_commit("HEAD").unwrap(), head);
}

#[test]
fn init_detached_worktree_refuses_an_oversized_object_closure() {
    use std::os::unix::fs::DirBuilderExt as _;

    let repo = TestRepo::init();
    repo.write(
        "a.txt",
        b"some bytes that make the closure exceed one byte\n",
    );
    repo.add_all();
    let head = repo.commit("c1");

    let git = repo.hardened();
    let repo_id = git.canonical_repo_id().unwrap();

    // A fresh owner-only worktree directory outside the repo.
    let holder = tempfile::tempdir().unwrap();
    let wt_dir = holder.path().join("wt");
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(&wt_dir)
        .unwrap();

    // A 1-byte closure budget is smaller than any real closure, so the copy is refused
    // BEFORE any pack is written.
    let err = git
        .init_detached_worktree(&wt_dir, &repo_id, &head, 1)
        .unwrap_err();
    assert!(
        matches!(err, o7_worktree::GitError::ClosureTooLarge { .. }),
        "expected ClosureTooLarge, got {err:?}"
    );
    // No pack was written under the worktree's objectdb.
    let pack_dir = wt_dir.join(".git/objects/pack");
    let packed = std::fs::read_dir(&pack_dir)
        .map(|rd| {
            rd.flatten()
                .any(|e| e.file_name().to_string_lossy().ends_with(".pack"))
        })
        .unwrap_or(false);
    assert!(!packed, "a pack was written despite exceeding the budget");

    // A generous budget succeeds.
    let holder2 = tempfile::tempdir().unwrap();
    let wt_dir2 = holder2.path().join("wt");
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(&wt_dir2)
        .unwrap();
    git.init_detached_worktree(&wt_dir2, &repo_id, &head, 8 * 1024 * 1024 * 1024)
        .unwrap();
    assert!(wt_dir2.join(".git").exists());
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
