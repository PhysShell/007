//! What counts as admissible input to a replay: three P1s from two reviewers,
//! closed, with the witnesses that keep them closed.
//!
//! Both are the same sentence from AGENTS.md, at two different layers:
//!
//! ```text
//! missing evidence is not a passed check   an absent signal is not a negative result
//! a first page is not the whole result     partial success is not success
//! ```
//!
//! P1 #1 — a `github-submitted-review` whose §8.2-required `user.login` did not
//! survive projection reached the login comparison and came out `false`. So a
//! snapshot that could not be read was reported as a review that did not match,
//! and an empty `matchedSnapshotDigests` could be assembled out of broken
//! candidates while every digest still recomputed. That is an absence claim built
//! from unreadable evidence, which is the exact thing §13 exists to refuse.
//!
//! P1 #2 — replay ran over whatever candidates the caller passed. §13 obliges the
//! matched subsequence to be recomputable from *the complete candidate set*, and
//! `RecordedMatcher` did not carry it, so a caller that resolved none of the
//! declared candidates got `reproduced: true` against an empty claim. Specimen
//! G's whole failure mode, re-entering through the API rather than through a
//! fixture.
//!
//! P1 #3 — the first fix for #1 was incomplete, and the contract said so. It
//! checked only the fields the predicate reads, and §13.1 as written says a
//! candidate that "omits a field that kind requires" is inadmissible. So a review
//! carrying `user.login` but missing `state`, `commitId` or `submittedAt` passed
//! the check, scored `false` on the login comparison, and could contribute to an
//! empty matched set — the same defect as #1, one field over. The document and
//! the mechanism disagreed, and both were written in the same push.
//!
//! The check is now the whole closed §8 shape for the kind: every required member
//! present with the right type, no member outside the declared set, and no `null`
//! standing in for an absent one. `commitId` is validated even though no matcher
//! reads it, because "is this a canonical source snapshot" is not the same
//! question as "does this matcher need it".
//!
//! WHERE THE FIXES LIVE, AND WHY NOT WHERE THEY WERE SUGGESTED. The review
//! proposed validating inside the predicate. The check sits in
//! [`recompute_matched`] instead, driven by a declared `candidate_requirement`,
//! for two reasons. §13.1 defines `f` over *canonical source snapshots*, so an
//! object that is not one is outside `f`'s domain rather than an input it should
//! answer `false` for. And the predicate's bytes are what `implementation_digest`
//! freezes: editing them is a behaviour change that §13.1 says takes a new
//! version, which would mean inventing `/2` to fix code that has never been
//! emitted. Neither predicate file changed; specimen I's recorded binding for
//! `/1` still holds.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

// Justification for the restriction-lint allowance, per AGENTS.md rule 4 and the
// precedent in `crates/o7-closure-classifier/tests/frozen_fixtures.rs`: every
// panic path below is this test's own assertion failing, or its own construction
// of a candidate from a JSON literal written in the same function. Nothing here
// runs against production input.

use o7_closure_matcher::{
    recompute_matched, resolve, verify_matched, Candidate, MatchError, RecordedImplementation,
    RecordedMatcher,
};
use serde_json::{json, Value};

fn candidate(snapshot: Value) -> Candidate {
    Candidate {
        declared_digest: o7_closure_canonical::digest(&snapshot)
            .expect("digest")
            .as_str()
            .to_owned(),
        snapshot,
    }
}

fn review(login: Option<&str>) -> Value {
    let user = match login {
        Some(login) => json!({ "id": "1", "login": login, "type": "User" }),
        // §8.2 requires user.login. This projection lost it.
        None => json!({ "id": "1", "type": "User" }),
    };
    json!({
        "schemaVersion": 1,
        "sourceKind": "github-submitted-review",
        "stableId": "1",
        "user": user,
        "authorAssociation": "NONE",
        "state": "APPROVED",
        "body": "",
        "submittedAt": "2026-01-01T00:00:00Z",
        "commitId": "3333333333333333333333333333333333333333"
    })
}

fn over(candidates: &[Candidate], parameters: Value) -> RecordedMatcher {
    claiming(candidates, parameters, Vec::new())
}

fn claiming(candidates: &[Candidate], parameters: Value, claimed: Vec<String>) -> RecordedMatcher {
    let entry = resolve("review-by-expected-author-login", "1").expect("resolve");
    RecordedMatcher {
        id: entry.id.to_owned(),
        version: entry.version.to_owned(),
        parameters,
        matched_snapshot_digests: claimed,
        all_returned_snapshot_digests: candidates
            .iter()
            .map(|c| c.declared_digest.clone())
            .collect(),
        implementation: RecordedImplementation::Bound(entry.implementation_digest.to_owned()),
    }
}

/// P1 #1. A candidate of the matcher's own kind that is missing a field the
/// matcher reads is refused, not scored.
#[test]
fn an_unreadable_review_is_refused_rather_than_counted_as_a_non_match() {
    let broken = [candidate(review(None))];
    let outcome = recompute_matched(
        &over(&broken, json!({"expectedAuthorLogin": "wanted"})),
        &broken,
    );
    match outcome {
        Err(MatchError::IncompleteCandidate {
            source_kind, why, ..
        }) => {
            assert_eq!(source_kind, "github-submitted-review");
            assert!(
                why.contains("login"),
                "the diagnostic names the field: {why}"
            );
        }
        other => panic!("an unreadable review must be refused, got {other:?}"),
    }
}

/// The digest check does not catch it, which is why the shape check has to
/// exist: a truncated projection hashes to its own digest perfectly well.
#[test]
fn the_digest_check_alone_does_not_notice_a_truncated_projection() {
    let broken = candidate(review(None));
    assert_eq!(
        o7_closure_canonical::digest(&broken.snapshot)
            .expect("digest")
            .as_str(),
        broken.declared_digest,
        "the candidate binding holds — so nothing about digests would have \
         rejected this candidate, and only the §8 shape check does"
    );
}

/// The delivery-surface law is untouched: a candidate of a DIFFERENT kind is a
/// real non-match, not a malformed one, and must keep returning false rather
/// than becoming an error.
#[test]
fn a_candidate_of_another_kind_is_still_an_ordinary_non_match() {
    let comment = [candidate(json!({
        "schemaVersion": 1, "sourceKind": "github-issue-comment", "stableId": "9",
        "user": {"id": "1", "type": "Bot"},
        "authorAssociation": "NONE", "body": "",
        "createdAt": "2026-01-01T00:00:00Z", "updatedAt": "2026-01-01T00:00:00Z"
    }))];
    let replay = recompute_matched(
        &over(&comment, json!({"expectedAuthorLogin": "wanted"})),
        &comment,
    )
    .expect("a non-review candidate is not this matcher's to validate");
    assert!(replay.matched.is_empty());
}

/// P1 #2. A caller that resolved none of the declared candidates cannot
/// reproduce an empty claim.
#[test]
fn an_unresolvable_candidate_set_cannot_reproduce_an_empty_claim() {
    let declared = [candidate(review(Some("wanted")))];
    let recorded = over(&declared, json!({"expectedAuthorLogin": "wanted"}));

    assert!(
        matches!(
            verify_matched(&recorded, &[]),
            Err(MatchError::CandidateSetMismatch { .. })
        ),
        "an empty slice against a snapshot that declared a candidate is refused"
    );

    // Resolved properly, the same empty claim is contradicted rather than
    // refused — the two outcomes are different facts and stay different.
    let verdict = verify_matched(&recorded, &declared).expect("verify");
    assert!(!verdict.reproduced);
}

/// Partial resolution is refused for the same reason, and so is reordering:
/// §13.2 puts observation order inside the query digest.
#[test]
fn a_partial_or_reordered_candidate_set_is_refused() {
    let a = candidate(review(Some("wanted")));
    let b = candidate(json!({
        "schemaVersion": 1, "sourceKind": "github-submitted-review", "stableId": "2",
        "user": {"id": "2", "login": "other", "type": "User"},
        "authorAssociation": "NONE", "state": "APPROVED", "body": "",
        "submittedAt": "2026-01-02T00:00:00Z",
        "commitId": "5555555555555555555555555555555555555555"
    }));
    let declared = [a.clone(), b.clone()];
    let recorded = over(&declared, json!({"expectedAuthorLogin": "wanted"}));

    for (what, supplied) in [
        ("a prefix", vec![a.clone()]),
        ("a suffix", vec![b.clone()]),
        ("a reordering", vec![b, a]),
    ] {
        assert!(
            matches!(
                recompute_matched(&recorded, &supplied),
                Err(MatchError::CandidateSetMismatch { .. })
            ),
            "{what} of the declared sequence must be refused"
        );
    }
}

/// An empty declared set replayed over an empty candidate set is still fine —
/// the refusal is about disagreement, not about emptiness. Specimen C's case.
#[test]
fn an_empty_declaration_replayed_over_nothing_is_not_a_mismatch() {
    let recorded = over(&[], json!({"expectedAuthorLogin": "wanted"}));
    let replay = recompute_matched(&recorded, &[]).expect("replay");
    assert!(replay.matched.is_empty());
}

// ---- P1 #3: the shape check is the whole §8 shape, not the fields read.

fn review_missing(field: &str) -> Value {
    let mut v = review(Some("other"));
    v.as_object_mut().expect("object").remove(field);
    v
}

/// The exact case the review named: a login that is present and simply does not
/// match, on an object that is missing fields no matcher reads. Before the fix
/// this scored `false` and fed an empty matched set.
#[test]
fn a_review_missing_a_field_no_matcher_reads_is_still_inadmissible() {
    for field in [
        "state",
        "commitId",
        "submittedAt",
        "body",
        "authorAssociation",
        "stableId",
    ] {
        let broken = [candidate(review_missing(field))];
        let outcome = recompute_matched(
            &over(&broken, json!({"expectedAuthorLogin": "wanted"})),
            &broken,
        );
        match outcome {
            Err(MatchError::IncompleteCandidate { why, .. }) => {
                assert!(why.contains(field), "the diagnostic names {field}: {why}");
            }
            other => panic!("a review missing {field} must be refused, got {other:?}"),
        }
    }
}

/// §8 shapes are closed in both directions, so an unknown member is refused too.
/// A subset check would let a field ride along into replay unexamined.
#[test]
fn a_member_outside_the_closed_shape_is_refused() {
    let mut extra = review(Some("wanted"));
    extra
        .as_object_mut()
        .expect("object")
        .insert("htmlUrl".to_owned(), json!("https://example.invalid/x"));
    let broken = [candidate(extra)];
    assert!(
        matches!(
            recompute_matched(
                &over(&broken, json!({"expectedAuthorLogin": "wanted"})),
                &broken
            ),
            Err(MatchError::IncompleteCandidate { .. })
        ),
        "a locator smuggled into a canonical snapshot is not a canonical snapshot"
    );
}

/// §8: an absent field is absent. `null` is a value claiming the field was
/// observed and empty, which is a different assertion.
#[test]
fn null_is_not_how_an_absent_field_is_expressed() {
    let mut nulled = review(Some("wanted"));
    nulled.as_object_mut().expect("object")["commitId"] = Value::Null;
    let broken = [candidate(nulled)];
    match recompute_matched(
        &over(&broken, json!({"expectedAuthorLogin": "wanted"})),
        &broken,
    ) {
        Err(MatchError::IncompleteCandidate { why, .. }) => {
            assert!(why.contains("null"), "{why}");
        }
        other => panic!("a null required field must be refused, got {other:?}"),
    }
}

/// The nested shape is closed as well — `user` is not exempt from being checked
/// because it is an object rather than a scalar.
#[test]
fn the_nested_user_object_is_checked_too() {
    let mut no_type = review(Some("wanted"));
    no_type.as_object_mut().expect("object")["user"]
        .as_object_mut()
        .expect("user object")
        .remove("type");
    let broken = [candidate(no_type)];
    match recompute_matched(
        &over(&broken, json!({"expectedAuthorLogin": "wanted"})),
        &broken,
    ) {
        Err(MatchError::IncompleteCandidate { why, .. }) => {
            assert!(
                why.contains("user/type"),
                "user.type is REQUIRED and distinguishes Bot from User without \
                 string-matching a login: {why}"
            );
        }
        other => panic!("a user object missing type must be refused, got {other:?}"),
    }
}

// ---- P1 #4: an object that declares no kind is not "a candidate of another kind".

/// §7 requires every canonical object to carry `sourceKind`. Before the fix, an
/// object without one took the delivery-surface path — treated as a legitimate
/// candidate of some other surface — and scored `false`. A truncated snapshot
/// that still held the expected login therefore fed an empty absence claim.
#[test]
fn a_candidate_that_declares_no_kind_is_refused_not_waved_through() {
    for mangled in [
        // absent entirely
        {
            let mut v = review(Some("wanted"));
            v.as_object_mut().expect("object").remove("sourceKind");
            v
        },
        // present but not a string
        {
            let mut v = review(Some("wanted"));
            v.as_object_mut().expect("object")["sourceKind"] = json!(7);
            v
        },
    ] {
        let broken = [candidate(mangled)];
        match recompute_matched(
            &over(&broken, json!({"expectedAuthorLogin": "wanted"})),
            &broken,
        ) {
            Err(MatchError::IncompleteCandidate { why, .. }) => {
                assert!(why.contains("sourceKind"), "{why}");
            }
            other => panic!("an object declaring no kind must be refused, got {other:?}"),
        }
    }
}

/// The same for §7's other universal member. A candidate with no
/// `schemaVersion` is not a canonical object either, whatever kind it names.
#[test]
fn a_candidate_that_declares_no_schema_version_is_refused() {
    let mut v = json!({
        "sourceKind": "github-issue-comment", "stableId": "9",
        "user": {"id": "1", "login": "wanted", "type": "Bot"},
        "authorAssociation": "NONE", "body": "",
        "createdAt": "2026-01-01T00:00:00Z", "updatedAt": "2026-01-01T00:00:00Z"
    });
    v.as_object_mut().expect("object").remove("schemaVersion");
    let broken = [candidate(v)];
    match recompute_matched(
        &over(&broken, json!({"expectedAuthorLogin": "wanted"})),
        &broken,
    ) {
        Err(MatchError::IncompleteCandidate { why, .. }) => {
            assert!(why.contains("schemaVersion"), "{why}");
        }
        other => panic!("§7 applies to every candidate, not only matching ones: {other:?}"),
    }
}

// ---- P1 #5: the claim is the artifact's, not the caller's.

/// `verify_matched` takes no `claimed` argument. The claim is read from the
/// snapshot, so a caller cannot hand the verifier the recomputed value and
/// receive `reproduced: true` for an artifact that contradicts itself.
#[test]
fn the_claim_compared_against_is_the_snapshots_own() {
    let a = candidate(review(Some("wanted")));
    let declared = [a.clone()];

    // The artifact claims nothing matched, while its own candidate does match.
    let contradicting = claiming(
        &declared,
        json!({"expectedAuthorLogin": "wanted"}),
        Vec::new(),
    );
    let verdict = verify_matched(&contradicting, &declared).expect("verify");
    assert!(
        !verdict.reproduced,
        "a snapshot that contradicts its own candidate set must not reproduce"
    );
    assert_eq!(verdict.replay.matched, vec![a.declared_digest.clone()]);

    // An artifact whose claim is honest reproduces.
    let honest = claiming(
        &declared,
        json!({"expectedAuthorLogin": "wanted"}),
        vec![a.declared_digest.clone()],
    );
    assert!(
        verify_matched(&honest, &declared)
            .expect("verify")
            .reproduced
    );
}

/// And the snapshot parser reads the claim rather than leaving it to a caller —
/// specimen G is the case that matters, since its claim is the false one.
#[test]
fn the_parser_carries_the_recorded_claim() {
    let doc: Value = serde_json::from_str(
        &std::fs::read_to_string(
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures/closure-provenance/matcher-candidate-set-v1.json"),
        )
        .expect("specimen G"),
    )
    .expect("json");
    let snapshot = doc.get("canonical").expect("canonical");
    let recorded = RecordedMatcher::from_query_snapshot(snapshot).expect("recorded");
    assert!(
        recorded.matched_snapshot_digests.is_empty(),
        "G claims an empty matched subset, and that claim now travels with the \
         recorded matcher instead of being supplied alongside it"
    );
    assert_eq!(recorded.all_returned_snapshot_digests.len(), 2);
}
