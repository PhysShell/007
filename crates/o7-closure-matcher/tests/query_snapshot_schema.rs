//! §13: a `github-query-snapshot` is a closed shape, and a matching digest is
//! not proof that an object has it.
//!
//! `RecordedQuerySnapshot::from_canonical` publicly asserts that it built a
//! canonical `github-query-snapshot` before it yields a `RecordedMatcher`. Its
//! digest check establishes that these bytes are the ones the expected digest
//! names — nothing more. A snapshot malformed OUTSIDE the members the matcher
//! parser reads hashes to its own digest perfectly well, so every witness below
//! rebinds its mutation to a freshly recomputed digest. If one of these ever
//! fails with `QuerySnapshotDigestMismatch`, the witness has stopped testing
//! what it is for: the refusal must come from the shape, with the digest
//! agreeing.
//!
//! The boundary this file does NOT cross. Slice A establishes that `enumeration`
//! is present and carries a value §13 defines. Whether a given value is
//! *sufficient input for a NotProduced decision* — the `enumeration = COMPLETE`
//! precondition — is classifier admissibility, and belongs to the layer that
//! makes that decision. `incomplete_enumeration_is_well_formed` is the positive
//! control that keeps this file on its own side of that line.

// Justification for the restriction-lint allowance, per AGENTS.md rule 4 and the
// precedent in `crates/o7-closure-classifier/tests/frozen_fixtures.rs`: every
// panic path below is this test's own assertion failing, or its own handling of
// a JSON literal written in this file. Nothing here runs against production
// input, and a malformed literal must fail loudly rather than be skipped — a
// skipped witness is the vacuous green this file exists to prevent.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use o7_closure_matcher::{MatchError, RecordedQuerySnapshot};
use serde_json::{json, Value};

/// A complete, conforming version-1 `github-query-snapshot`.
///
/// Every member §13 lists as REQUIRED, and no other. Modelled on specimen C
/// (`tests/fixtures/closure-provenance/complete-empty-query-v1.json`) rather than
/// invented, so a drift between this file and the frozen corpus shows up as a
/// disagreement rather than as two independent opinions.
fn conforming_v1() -> Value {
    json!({
        "schemaVersion": 1,
        "sourceKind": "github-query-snapshot",
        "surface": "pull-request-reviews",
        "requiredObservationId": "review/external-auditor",
        "binding": {
            "repository": "PhysShell/007",
            "pullRequest": "9001"
        },
        "pagination": {
            "perPage": 100,
            "pagesRequested": ["1"],
            "pagesObtained": ["1"],
            "nextPagePresent": false
        },
        "enumeration": "COMPLETE",
        "matcher": {
            "id": "review-by-expected-author-login",
            "version": "1",
            "parameters": {"expectedAuthorLogin": "synthetic-external-reviewer"}
        },
        "allReturnedSnapshotDigests": [],
        "matchedSnapshotDigests": []
    })
}

/// Bind a snapshot to the digest of its own bytes and try to construct.
///
/// Self-consistent by construction, deliberately: these witnesses are about the
/// shape check, so the digest must always agree. The binding's own discriminating
/// witnesses, where the expected digest is read out of a fixture computed outside
/// this workspace, live in `recorded_implementation.rs`.
fn construct(snapshot: &Value) -> Result<RecordedQuerySnapshot, MatchError> {
    let bound = o7_closure_canonical::digest(snapshot).expect("digest");
    RecordedQuerySnapshot::from_canonical(snapshot, bound.as_str())
}

/// The refusal came from the shape, and the digest agreed.
fn refused_for_shape(snapshot: &Value, expected_in_why: &str) {
    match construct(snapshot) {
        Err(MatchError::MalformedQuerySnapshot { why }) => assert!(
            why.contains(expected_in_why),
            "refused, but not for the reason under test: wanted {expected_in_why:?} in {why:?}"
        ),
        Err(MatchError::QuerySnapshotDigestMismatch { .. }) => panic!(
            "refused for a digest mismatch. The witness rebinds every mutation to its own \
             recomputed digest precisely so the digest cannot be what refuses it; this \
             failure means the witness no longer tests the schema"
        ),
        Err(other) => panic!("refused, but not as a malformed query snapshot: {other}"),
        Ok(_) => panic!(
            "constructed. A snapshot malformed outside the members the matcher parser reads \
             hashes to its own digest, so nothing else in the chain will catch this"
        ),
    }
}

/// The control on the control: the object the witnesses mutate is itself
/// admissible. Without this, every refusal below could be the base object being
/// malformed for some unrelated reason.
#[test]
fn the_conforming_snapshot_constructs() {
    construct(&conforming_v1()).expect("a complete §13 query snapshot is admissible");
}

/// §7 requires every canonical object to carry `sourceKind`, and §13 names it
/// REQUIRED. Without the check, any canonical object carrying a matcher block
/// parses as a query snapshot.
#[test]
fn a_snapshot_without_source_kind_is_refused() {
    let mut snapshot = conforming_v1();
    snapshot.as_object_mut().unwrap().remove("sourceKind");
    refused_for_shape(&snapshot, "sourceKind");
}

/// The value matters, not merely the type. A canonical object of some other kind
/// that happens to carry a matcher block is not a query snapshot.
#[test]
fn a_snapshot_declaring_another_source_kind_is_refused() {
    let mut snapshot = conforming_v1();
    snapshot.as_object_mut().unwrap()["sourceKind"] = json!("github-submitted-review");
    refused_for_shape(&snapshot, "sourceKind");
}

/// §13 lists `enumeration` REQUIRED. Slice A establishes that it is present and
/// admissible; §13's `NotProduced` precondition is Slice B's to apply.
#[test]
fn a_snapshot_without_enumeration_is_refused() {
    let mut snapshot = conforming_v1();
    snapshot.as_object_mut().unwrap().remove("enumeration");
    refused_for_shape(&snapshot, "enumeration");
}

/// A value outside the set §13 defines is not an enumeration state. Admitting it
/// would leave a later reader to guess whether an unrecognised string is
/// complete.
#[test]
fn a_snapshot_with_an_undefined_enumeration_state_is_refused() {
    let mut snapshot = conforming_v1();
    snapshot.as_object_mut().unwrap()["enumeration"] = json!("PROBABLY_FINE");
    refused_for_shape(&snapshot, "enumeration");
}

/// Nested shapes are closed too. `binding` identifies what was queried; a
/// snapshot that does not say which pull request it enumerated cannot be
/// evidence about one.
#[test]
fn a_snapshot_without_binding_pull_request_is_refused() {
    let mut snapshot = conforming_v1();
    snapshot["binding"]
        .as_object_mut()
        .unwrap()
        .remove("pullRequest");
    refused_for_shape(&snapshot, "pullRequest");
}

/// The superset direction of the same closed-shape obligation, inside the
/// matcher block.
///
/// This was carried as a standalone P2 into Slice B on the reading that an extra
/// key is cosmetic. It is not standalone: subset and superset are two sides of
/// one obligation, and the subset side turned out to be a semantic escape. A
/// validator that closes only one side closes neither.
#[test]
fn a_snapshot_with_an_unknown_matcher_member_is_refused() {
    let mut snapshot = conforming_v1();
    snapshot["matcher"].as_object_mut().unwrap().insert(
        "implementationSource".to_owned(),
        json!("fn predicate() -> bool { true }"),
    );
    refused_for_shape(&snapshot, "implementationSource");
}

/// The same, at the top level.
#[test]
fn a_snapshot_with_an_unknown_top_level_member_is_refused() {
    let mut snapshot = conforming_v1();
    snapshot
        .as_object_mut()
        .unwrap()
        .insert("verdict".to_owned(), json!("PASS"));
    refused_for_shape(&snapshot, "verdict");
}

/// §13: the two shapes are closed and neither may borrow from the other. A
/// version-1 snapshot carrying `implementationDigest` is malformed, not a
/// version-2 snapshot that mislabelled itself.
#[test]
fn a_version_1_snapshot_carrying_an_implementation_digest_is_refused() {
    let mut snapshot = conforming_v1();
    snapshot["matcher"].as_object_mut().unwrap().insert(
        "implementationDigest".to_owned(),
        json!("sha256:0000000000000000000000000000000000000000000000000000000000000000"),
    );
    assert!(
        construct(&snapshot).is_err(),
        "version 1 does not define implementationDigest"
    );
}

/// And the other direction.
#[test]
fn a_version_2_snapshot_without_an_implementation_digest_is_refused() {
    let mut snapshot = conforming_v1();
    snapshot.as_object_mut().unwrap()["schemaVersion"] = json!(2);
    assert!(
        construct(&snapshot).is_err(),
        "version 2 requires implementationDigest"
    );
}

/// §13 gives a changed shape a new version, so an unregistered version is an
/// object whose shape is unknown — the same law `check_candidate_shape` applies
/// to candidates, applied to the snapshot that declares them.
#[test]
fn a_snapshot_declaring_an_unregistered_schema_version_is_refused() {
    let mut snapshot = conforming_v1();
    snapshot.as_object_mut().unwrap()["schemaVersion"] = json!(3);
    refused_for_shape(&snapshot, "schemaVersion");
}

/// §8's null rule holds here too: an absent field is absent, and `null` is a
/// value claiming the field was observed as empty.
#[test]
fn a_null_required_member_is_refused() {
    let mut snapshot = conforming_v1();
    snapshot.as_object_mut().unwrap()["requiredObservationId"] = Value::Null;
    refused_for_shape(&snapshot, "requiredObservationId");
}

/// Types are checked, not merely presence. `nextPagePresent` is the member §14
/// turns on, and a string `"false"` is not a pagination state.
#[test]
fn a_mistyped_pagination_member_is_refused() {
    let mut snapshot = conforming_v1();
    snapshot["pagination"].as_object_mut().unwrap()["nextPagePresent"] = json!("false");
    refused_for_shape(&snapshot, "nextPagePresent");
}

/// The digest arrays are arrays of strings, not of anything.
#[test]
fn a_non_string_candidate_digest_is_refused() {
    let mut snapshot = conforming_v1();
    snapshot.as_object_mut().unwrap()["allReturnedSnapshotDigests"] = json!([1]);
    refused_for_shape(&snapshot, "allReturnedSnapshotDigests");
}

/// THE BOUNDARY WITNESS, and the reason this file does not simply require
/// `COMPLETE`.
///
/// §13 defines `INCOMPLETE` as a state a query snapshot may legitimately record —
/// specimen D is the frozen instance, and it exists precisely so that a
/// non-authoritative empty result stays recordable and distinguishable from an
/// authoritative one. Refusing it at construction would destroy the artifact that
/// witnesses the distinction.
///
/// So Slice A admits it, and says nothing about it. Constructing a
/// `RecordedQuerySnapshot` is not a `NotProduced` claim and Slice A has no way to
/// make one: there is no verdict, no `OWED`, no `PASS` anywhere in this crate's
/// API. The `enumeration = COMPLETE` precondition is applied by whoever decides,
/// against the value this type now guarantees is present and defined.
#[test]
fn incomplete_enumeration_is_well_formed_and_makes_no_claim() {
    let mut snapshot = conforming_v1();
    snapshot.as_object_mut().unwrap()["enumeration"] = json!("INCOMPLETE");
    snapshot.as_object_mut().unwrap().insert(
        "incompleteReason".to_owned(),
        json!("page 2 fetch returned HTTP 502; not retried"),
    );

    let recorded = construct(&snapshot).expect(
        "§13 defines INCOMPLETE as a recordable enumeration state, and specimen D is the \
         frozen instance of one; refusing it here would make the non-authoritative case \
         unrepresentable",
    );

    // What was established is shape, and only shape.
    assert_eq!(
        recorded.recorded_matcher().id(),
        "review-by-expected-author-login"
    );
    assert!(recorded
        .recorded_matcher()
        .matched_snapshot_digests()
        .is_empty());
}

/// `incompleteReason` is OPTIONAL-IF-PRESENT, so its absence is not a defect —
/// but `null` still is.
#[test]
fn an_optional_member_may_be_absent_but_not_null() {
    let mut snapshot = conforming_v1();
    snapshot
        .as_object_mut()
        .unwrap()
        .insert("incompleteReason".to_owned(), Value::Null);
    refused_for_shape(&snapshot, "incompleteReason");
}

/// `binding.sha` is the other OPTIONAL-IF-PRESENT member, and it must be
/// admissible when present.
#[test]
fn an_optional_binding_member_is_admitted_when_present() {
    let mut snapshot = conforming_v1();
    snapshot["binding"].as_object_mut().unwrap().insert(
        "sha".to_owned(),
        json!("1f2e3d4c5b6a798807162534435261708f9e0d1c"),
    );
    construct(&snapshot).expect("§13 lists binding.sha OPTIONAL-IF-PRESENT");
}
