//! Adversarial tests for durable ownership and recovery (PR 3, slice 2).
//!
//! Proven here:
//!   * a restart re-attests: reopen accepts only an EXACT identity (repo drift and a
//!     tampered record are rejected);
//!   * a forged or stale record/summary confers no delete authority — deletion always
//!     requires live attestation;
//!   * concurrent create/reopen/cleanup never produce two owners (exclusive lock);
//!   * interrupted ownership is never silently lost — an orphaned directory is
//!     reported, never deleted, and an interrupted cleanup reconciles cleanly;
//!   * the versioned state fails closed on schema drift and corruption.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod support;

use std::sync::Arc;

use o7_worktree::{CleanupOutcome, StateRoot, StoreError, WorktreeStore};
use support::*;

fn store(sr: StateRoot) -> WorktreeStore {
    WorktreeStore::new(sr)
}

fn state_json(sr: &StateRoot) -> serde_json::Value {
    let bytes = std::fs::read(sr.path().join("worktrees.json")).unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

fn write_state_json(sr: &StateRoot, value: &serde_json::Value) {
    std::fs::write(
        sr.path().join("worktrees.json"),
        serde_json::to_vec_pretty(value).unwrap(),
    )
    .unwrap();
}

fn records_len(sr: &StateRoot) -> usize {
    state_json(sr)["records"].as_array().unwrap().len()
}

#[test]
fn reopen_after_restart_accepts_exact_identity() {
    let repo = TestRepo::init();
    repo.write("a.txt", b"a\n");
    repo.add_all();
    let head = repo.commit("c1");
    let (dir, sr) = state_root();

    let created_path = {
        let st = store(sr.clone());
        let wt = st.create(&repo.hardened(), run_id("run1"), &head).unwrap();
        wt.path().to_path_buf()
    };

    // "Restart": a fresh store over the same, still-verified state root.
    let sr2 = StateRoot::open_or_create(sr.path()).unwrap();
    let st2 = store(sr2);
    let recovered = st2.reopen(&repo.hardened(), &run_id("run1")).unwrap();
    assert_eq!(recovered.worktree.path(), created_path);
    // The reopened handle re-proves on demand.
    assert!(recovered.worktree.attest().is_ok());
    let _keep = dir;
}

#[test]
fn reopen_rejects_repo_identity_drift() {
    let repo = TestRepo::init();
    repo.write("a.txt", b"a\n");
    repo.add_all();
    let head = repo.commit("c1");
    let (_dir, sr) = state_root();
    let st = store(sr);
    st.create(&repo.hardened(), run_id("run1"), &head).unwrap();

    // A DIFFERENT repository (different git-common-dir dev/ino) must not reopen the run.
    let other = TestRepo::init();
    other.write("x", b"x\n");
    other.add_all();
    let _ = other.commit("c1");
    let err = st.reopen(&other.hardened(), &run_id("run1")).unwrap_err();
    assert!(
        matches!(err, StoreError::RepoIdentityDrift { .. }),
        "expected RepoIdentityDrift, got {err:?}"
    );
}

#[test]
fn reopen_rejects_a_tampered_record() {
    let repo = TestRepo::init();
    repo.write("a.txt", b"a\n");
    repo.add_all();
    let head = repo.commit("c1");
    let (_dir, sr) = state_root();
    let st = store(sr.clone());
    st.create(&repo.hardened(), run_id("run1"), &head).unwrap();

    // Tamper the recorded revision without recomputing the digest: the record no longer
    // matches its own identity, so it is refused (never trusted).
    let mut json = state_json(&sr);
    json["records"][0]["revision"] = serde_json::Value::String("b".repeat(40));
    write_state_json(&sr, &json);

    let err = st.reopen(&repo.hardened(), &run_id("run1")).unwrap_err();
    assert!(
        matches!(err, StoreError::RecordTampered { .. }),
        "expected RecordTampered, got {err:?}"
    );
}

#[test]
fn stale_fs_identity_confers_no_delete_authority() {
    let repo = TestRepo::init();
    repo.write("a.txt", b"a\n");
    repo.add_all();
    let head = repo.commit("c1");
    let (_dir, sr) = state_root();
    let st = store(sr.clone());
    let wt = st.create(&repo.hardened(), run_id("run1"), &head).unwrap();
    let path = wt.path().to_path_buf();

    // Forge the record's fs identity (wrong inode) and give it a flattering summary.
    // Neither buys any authority: cleanup re-attests live and refuses to delete.
    let mut json = state_json(&sr);
    json["records"][0]["fs_identity"]["ino"] =
        serde_json::Value::Number(serde_json::Number::from(999_999_999_u64));
    json["records"][0]["summary"]["files"] =
        serde_json::Value::Number(serde_json::Number::from(4242_u64));
    write_state_json(&sr, &json);

    match st.cleanup(&run_id("run1")).unwrap() {
        CleanupOutcome::PreservedForInvestigation(_) => {}
        other => panic!("a forged/stale record must not authorize delete, got {other:?}"),
    }
    // The worktree directory is untouched and the record is retained.
    assert!(path.exists());
    assert_eq!(records_len(&sr), 1);
}

#[test]
fn forged_record_path_is_refused_not_actioned() {
    let repo = TestRepo::init();
    repo.write("a.txt", b"a\n");
    repo.add_all();
    let head = repo.commit("c1");
    let (_dir, sr) = state_root();
    let st = store(sr.clone());
    st.create(&repo.hardened(), run_id("run1"), &head).unwrap();

    // A victim directory an attacker would love o7d to delete.
    let victim = tempfile::tempdir().unwrap();
    std::fs::write(victim.path().join("precious"), b"keep me").unwrap();

    // Repoint the record at the victim. The path is bound to the identity digest, so
    // this fails the integrity check — never a delete.
    let mut json = state_json(&sr);
    json["records"][0]["worktree_path"] =
        serde_json::Value::String(victim.path().to_string_lossy().into_owned());
    write_state_json(&sr, &json);

    let err = st.cleanup(&run_id("run1")).unwrap_err();
    assert!(
        matches!(err, StoreError::RecordTampered { .. }),
        "expected RecordTampered, got {err:?}"
    );
    assert!(
        victim.path().join("precious").exists(),
        "victim was deleted"
    );
}

#[test]
fn concurrent_create_for_same_run_does_not_double_own() {
    let repo = TestRepo::init();
    repo.write("a.txt", b"a\n");
    repo.add_all();
    let head = repo.commit("c1");
    let (_dir, sr) = state_root();
    let st = Arc::new(store(sr.clone()));

    let mut handles = Vec::new();
    for _ in 0..8 {
        let st = Arc::clone(&st);
        let git = repo.hardened();
        let head = head.clone();
        handles.push(std::thread::spawn(move || {
            st.create(&git, run_id("same"), &head).is_ok()
        }));
    }
    let successes = handles
        .into_iter()
        .map(|h| h.join().unwrap())
        .filter(|ok| *ok)
        .count();
    // Exactly one create wins; the rest are rejected. One record, one owner.
    assert_eq!(successes, 1, "more than one create claimed ownership");
    assert_eq!(records_len(&sr), 1);
}

#[test]
fn interrupted_create_leaves_an_orphan_that_is_reported_not_deleted() {
    let repo = TestRepo::init();
    repo.write("a.txt", b"a\n");
    repo.add_all();
    let head = repo.commit("c1");
    let (_dir, sr) = state_root();

    // Simulate a create that materialized a directory but crashed before persisting the
    // record: a legit worktree dir with NO record. Build one, then wipe the state file.
    let st = store(sr.clone());
    let wt = st.create(&repo.hardened(), run_id("run1"), &head).unwrap();
    let orphan = wt.path().to_path_buf();
    std::fs::remove_file(sr.path().join("worktrees.json")).unwrap();

    // prune must REPORT the orphan (authority not silently lost) and NOT delete it.
    let outcome = st.prune().unwrap();
    assert!(
        outcome.unreferenced.iter().any(|p| p == &orphan),
        "orphan not reported: {:?}",
        outcome.unreferenced
    );
    assert!(orphan.exists(), "prune deleted an unreferenced directory");
}

#[test]
fn interrupted_cleanup_with_missing_dir_reconciles_without_losing_authority() {
    let repo = TestRepo::init();
    repo.write("a.txt", b"a\n");
    repo.add_all();
    let head = repo.commit("c1");
    let (_dir, sr) = state_root();
    let st = store(sr.clone());
    let wt = st.create(&repo.hardened(), run_id("run1"), &head).unwrap();
    let path = wt.path().to_path_buf();

    // Simulate an interrupted cleanup: the directory was removed but the record remains.
    std::fs::remove_dir_all(&path).unwrap();

    // prune sees a provably-gone worktree and drops the stale record — no error, no
    // spurious preservation.
    let outcome = st.prune().unwrap();
    assert_eq!(outcome.dropped_missing, vec![run_id("run1")]);
    assert!(outcome.preserved.is_empty());
    assert_eq!(records_len(&sr), 0);
}

#[test]
fn prune_keeps_proven_and_preserves_unprovable() {
    let repo = TestRepo::init();
    repo.write("a.txt", b"a\n");
    repo.add_all();
    let head = repo.commit("c1");
    let (_dir, sr) = state_root();
    let st = store(sr.clone());
    let good = st.create(&repo.hardened(), run_id("good"), &head).unwrap();
    assert!(good.attest().is_ok());

    // A second run whose on-disk dir gets substituted by a symlink (unprovable).
    let bad = st.create(&repo.hardened(), run_id("bad"), &head).unwrap();
    let bad_path = bad.path().to_path_buf();
    let elsewhere = tempfile::tempdir().unwrap();
    std::fs::remove_dir_all(&bad_path).unwrap();
    std::os::unix::fs::symlink(elsewhere.path(), &bad_path).unwrap();

    let outcome = st.prune().unwrap();
    assert!(outcome.kept.contains(&run_id("good")));
    assert!(outcome.preserved.iter().any(|(r, _)| r == &run_id("bad")));
    // Neither the proven worktree nor the unprovable one was deleted.
    assert!(good.path().exists());
    assert!(std::fs::symlink_metadata(&bad_path)
        .unwrap()
        .file_type()
        .is_symlink());
    assert_eq!(records_len(&sr), 2);
}

#[test]
fn schema_drift_and_corruption_fail_closed() {
    let repo = TestRepo::init();
    repo.write("a.txt", b"a\n");
    repo.add_all();
    let head = repo.commit("c1");
    let (_dir, sr) = state_root();
    let st = store(sr.clone());
    st.create(&repo.hardened(), run_id("run1"), &head).unwrap();

    // A future schema is not silently accepted.
    let mut json = state_json(&sr);
    json["schema"] = serde_json::Value::Number(serde_json::Number::from(999_u64));
    write_state_json(&sr, &json);
    assert!(matches!(
        st.reopen(&repo.hardened(), &run_id("run1")).unwrap_err(),
        StoreError::SchemaMismatch { .. }
    ));

    // Corruption is an error, never "treat as empty" (which would lose authority).
    std::fs::write(sr.path().join("worktrees.json"), b"{ not json").unwrap();
    assert!(matches!(
        st.prune().unwrap_err(),
        StoreError::Corrupt { .. }
    ));
}

#[test]
fn cleanup_via_store_removes_and_updates_state() {
    let repo = TestRepo::init();
    repo.write("a.txt", b"a\n");
    repo.add_all();
    let head = repo.commit("c1");
    let (_dir, sr) = state_root();
    let st = store(sr.clone());
    let wt = st.create(&repo.hardened(), run_id("run1"), &head).unwrap();
    let path = wt.path().to_path_buf();

    assert_eq!(
        st.cleanup(&run_id("run1")).unwrap(),
        CleanupOutcome::Removed
    );
    assert!(!path.exists());
    assert_eq!(records_len(&sr), 0);
}
