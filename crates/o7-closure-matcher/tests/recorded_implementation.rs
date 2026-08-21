//! GREEN-3. The implementation binding's authority moves out of this source tree.
//!
//! THE PROBLEM RED-3 LANDED. 48548ef bound `(id, version)` to a SHA-256 of the
//! predicate file's bytes. That fixed *what* the certificate covers. It left the
//! certificate where it was — in `src/matchers.rs`, two lines from the
//! `include_str!` that supplies the bytes it judges:
//!
//! ```text
//! implementation_source: include_str!("matchers/review_..._v1.rs"),
//! implementation_digest: "sha256:59ea...",
//! ```
//!
//! One commit edits both, and `verify_implementation()` is satisfied. So it
//! establishes that two current fields agree, not that `/1` still means what
//! `/1` meant — an artifact certifying the thing it is checked against, third
//! time, one level up each time.
//!
//! WHAT CHANGED. `github-query-snapshot` gains `matcher.implementationDigest` at
//! `schemaVersion` 2, so a durable artifact records which implementation
//! produced its matched subsequence. The expected value is then written by a
//! different act, at a different time, and covered by the artifact's own digest.
//! Specimen I is that record, and the tests below read their expectation out of
//! it — never out of `REGISTRY`.
//!
//! ```text
//! before:  hash(bytes in tree) == constant in tree
//! after:   hash(bytes in tree) == digest in an artifact this tree did not write
//! ```
//!
//! WHAT THIS DOES AND DOES NOT ESTABLISH. It is not tamper-proofing, and calling
//! it that would repeat the overclaim in 48548ef that RED-3 had to demolish. A
//! commit that edits the predicate, the registry constant, specimen I's
//! `implementationDigest` and specimen I's `canonicalDigest` still passes, and no
//! arrangement of files inside one repository can prevent that — the tree is
//! writable by whoever writes the tree. Three things are true instead, and they
//! are the whole claim:
//!
//! - The record and the implementation are no longer edited by the same act.
//!   Drift is now a four-file diff in a fixture reviewers read, not two adjacent
//!   lines.
//! - Specimen I's digest was computed by rfc8785 0.1.4 outside this workspace,
//!   per the corpus rule, so re-blessing it means going back to the external
//!   tool rather than running the code under test.
//! - The part that actually leaves reach: an emitted closure artifact — an
//!   attestation, a snapshot already handed to someone — carries the digest of
//!   the code that produced it and is not in this repository at all. Specimen I
//!   is that situation's stand-in; the real binding starts when artifacts are
//!   emitted. Within the tree the mechanism catches drift, which is the failure
//!   that actually happens; it does not catch an author who means it.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use o7_closure_matcher::{
    check_recorded_implementation, recompute_matched, resolve, Candidate, ImplementationCheck,
    MatchError, RecordedImplementation, RecordedMatcher,
};
use serde_json::Value;
use std::fs;
use std::path::PathBuf;

fn fixture(name: &str) -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/closure-provenance")
        .join(name);
    serde_json::from_str(&fs::read_to_string(&path).expect("reading a frozen specimen"))
        .expect("a frozen specimen is JSON")
}

const SPECIMEN_I: &str = "recorded-implementation-v1.json";

fn specimen_i_snapshot() -> Value {
    fixture(SPECIMEN_I)
        .get("canonical")
        .expect("canonical query snapshot")
        .clone()
}

fn specimen_i_candidates() -> Vec<Candidate> {
    let doc = fixture(SPECIMEN_I);
    let listed: Vec<String> = doc
        .pointer("/canonical/allReturnedSnapshotDigests")
        .and_then(Value::as_array)
        .expect("allReturnedSnapshotDigests")
        .iter()
        .map(|d| d.as_str().expect("digest").to_owned())
        .collect();
    let retained = doc
        .get("candidates")
        .and_then(Value::as_array)
        .expect("candidates")
        .clone();
    listed
        .into_iter()
        .map(|digest| {
            let found = retained
                .iter()
                .find(|c| c.get("canonicalDigest").and_then(Value::as_str) == Some(&digest))
                .unwrap_or_else(|| panic!("no candidate retained for {digest}"));
            Candidate {
                declared_digest: digest,
                snapshot: found.get("canonical").expect("canonical").clone(),
            }
        })
        .collect()
}

/// The expected digest is a literal in a JSON file that this crate does not
/// write. If it ever starts coming from `REGISTRY`, every test below reverts to
/// checking that the tree agrees with itself.
#[test]
fn the_expectation_is_a_literal_in_the_frozen_corpus() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/closure-provenance")
        .join(SPECIMEN_I);
    let raw = fs::read_to_string(&path).expect("read");
    let recorded = fixture(SPECIMEN_I)
        .pointer("/canonical/matcher/implementationDigest")
        .and_then(Value::as_str)
        .expect("specimen I records an implementation digest")
        .to_owned();
    assert!(
        raw.contains(&recorded),
        "the recorded digest must be present as text in the fixture"
    );
    assert!(
        recorded.starts_with("sha256:") && recorded.len() == 71,
        "recorded digest has the frozen syntax: {recorded}"
    );
}

/// Replaying specimen I: the selection reproduces AND the implementation that
/// reproduced it is the one the artifact named.
#[test]
fn replaying_the_recorded_snapshot_binds_the_implementation_that_ran() {
    let snapshot = specimen_i_snapshot();
    let recorded = RecordedMatcher::from_query_snapshot(&snapshot).expect("recorded matcher");
    let claimed: Vec<String> = snapshot
        .get("matchedSnapshotDigests")
        .and_then(Value::as_array)
        .expect("matchedSnapshotDigests")
        .iter()
        .map(|d| d.as_str().expect("digest").to_owned())
        .collect();

    let replay = recompute_matched(&recorded, &specimen_i_candidates()).expect("replay");
    assert_eq!(
        replay.matched, claimed,
        "specimen I's claim reproduces, so a failure here can only be the binding"
    );
    let RecordedImplementation::Bound(expected) = &recorded.implementation else {
        panic!("specimen I is a version-2 snapshot and must record an implementation");
    };
    assert_eq!(
        replay.implementation,
        ImplementationCheck::Bound {
            digest: expected.clone()
        }
    );
}

/// The discriminating case, and the one RED-3 walks through today: the artifact
/// names an implementation this tree does not resolve to.
///
/// A mutant moves the tree while the record stays put; this test moves the
/// record while the tree stays put. Those are the same comparison approached
/// from opposite sides, and only the first is what actually happens — which is
/// why the RED-3 commit exists as history rather than as an assertion here.
#[test]
fn a_recorded_digest_this_tree_does_not_resolve_to_is_refused() {
    let recorded = RecordedMatcher::from_query_snapshot(&specimen_i_snapshot()).expect("recorded");
    let entry = resolve(&recorded.id, &recorded.version).expect("resolve");

    let drifted = RecordedImplementation::Bound(
        "sha256:0000000000000000000000000000000000000000000000000000000000000000".to_owned(),
    );
    match check_recorded_implementation(entry, &drifted) {
        Err(MatchError::RecordedImplementationDrift {
            recorded, resolved, ..
        }) => {
            assert_ne!(recorded, resolved);
        }
        other => panic!("drift must be refused, got {other:?}"),
    }
}

/// And drift is refused through the replay path too, not only when called
/// directly — a caller cannot recompute a matched subsequence while ignoring
/// which code recomputed it, because there is no entry point that omits it.
#[test]
fn replay_refuses_drift_rather_than_reporting_it_alongside_a_result() {
    let mut recorded =
        RecordedMatcher::from_query_snapshot(&specimen_i_snapshot()).expect("recorded");
    recorded.implementation = RecordedImplementation::Bound(
        "sha256:1111111111111111111111111111111111111111111111111111111111111111".to_owned(),
    );
    assert!(matches!(
        recompute_matched(&recorded, &specimen_i_candidates()),
        Err(MatchError::RecordedImplementationDrift { .. })
    ));
}

/// A version-1 snapshot predates the field. That is CANNOT_CHECK — an axis with
/// no evidence — and must never read as a passed one. C, D and G stay at version
/// 1 precisely so this case keeps a witness.
#[test]
fn a_version_one_snapshot_cannot_check_the_implementation() {
    for name in [
        "matcher-candidate-set-v1.json",
        "complete-empty-query-v1.json",
        "incomplete-query-v1.json",
    ] {
        let snapshot = fixture(name).get("canonical").expect("canonical").clone();
        let recorded = RecordedMatcher::from_query_snapshot(&snapshot)
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(
            recorded.implementation,
            RecordedImplementation::Unrecorded,
            "{name} is a version-1 snapshot"
        );
        let entry = resolve(&recorded.id, &recorded.version).expect("resolve");
        assert_eq!(
            check_recorded_implementation(entry, &recorded.implementation).expect("check"),
            ImplementationCheck::CannotCheck,
            "{name} carries no implementation evidence, so nothing about the \
             implementation is established — including that it did not drift"
        );
    }
}

/// Corpus-wide rather than specimen-wide: every version-2 query snapshot the
/// corpus holds must bind to an implementation this tree resolves. A future v2
/// specimen extends this check by existing, and deleting the one specimen that
/// currently drives it is visible as coverage going to zero rather than as a
/// test quietly passing over an empty set.
#[test]
fn every_recorded_implementation_in_the_corpus_binds() {
    let dir =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/closure-provenance");
    let mut bound = 0;
    for entry in fs::read_dir(&dir).expect("corpus") {
        let path = entry.expect("dir entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let doc: Value =
            serde_json::from_str(&fs::read_to_string(&path).expect("read")).expect("parse");
        let Some(snapshot) = doc.get("canonical") else {
            continue;
        };
        if snapshot.get("sourceKind").and_then(Value::as_str) != Some("github-query-snapshot") {
            continue;
        }
        let recorded = RecordedMatcher::from_query_snapshot(snapshot)
            .unwrap_or_else(|e| panic!("{path:?}: {e}"));
        let RecordedImplementation::Bound(_) = recorded.implementation else {
            continue;
        };
        let resolved =
            resolve(&recorded.id, &recorded.version).unwrap_or_else(|e| panic!("{path:?}: {e}"));
        let check = check_recorded_implementation(resolved, &recorded.implementation)
            .unwrap_or_else(|e| panic!("{path:?}: {e}"));
        assert!(
            matches!(check, ImplementationCheck::Bound { .. }),
            "{path:?} records an implementation, so the check must bind it"
        );
        bound += 1;
    }
    assert!(
        bound >= 1,
        "no version-2 query snapshot in the corpus records an implementation, so \
         nothing outside this tree has an opinion about what /1 is"
    );
}

/// §8's schemas are closed key sets. A shape that can borrow a field from its
/// neighbour is not closed, so both directions are refused.
#[test]
fn the_two_snapshot_shapes_do_not_borrow_fields_from_each_other() {
    let mut v1_with_digest = fixture("matcher-candidate-set-v1.json")
        .get("canonical")
        .expect("canonical")
        .clone();
    v1_with_digest
        .pointer_mut("/matcher")
        .and_then(Value::as_object_mut)
        .expect("matcher object")
        .insert(
            "implementationDigest".to_owned(),
            Value::String("sha256:00".to_owned()),
        );
    assert!(matches!(
        RecordedMatcher::from_query_snapshot(&v1_with_digest),
        Err(MatchError::MalformedRecordedMatcher { .. })
    ));

    let mut v2_without_digest = specimen_i_snapshot();
    v2_without_digest
        .pointer_mut("/matcher")
        .and_then(Value::as_object_mut)
        .expect("matcher object")
        .remove("implementationDigest");
    assert!(matches!(
        RecordedMatcher::from_query_snapshot(&v2_without_digest),
        Err(MatchError::MalformedRecordedMatcher { .. })
    ));
}
