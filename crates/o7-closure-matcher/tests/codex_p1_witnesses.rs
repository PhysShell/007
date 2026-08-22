//! The two P1s Codex found at `080aa7b`, closed, with the witnesses that keep
//! them closed.
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
    let entry = resolve("review-by-expected-author-login", "1").expect("resolve");
    RecordedMatcher {
        id: entry.id.to_owned(),
        version: entry.version.to_owned(),
        parameters,
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
            source_kind,
            pointer,
            ..
        }) => {
            assert_eq!(source_kind, "github-submitted-review");
            assert_eq!(pointer, "/user/login");
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
            verify_matched(&recorded, &[], &[]),
            Err(MatchError::CandidateSetMismatch { .. })
        ),
        "an empty slice against a snapshot that declared a candidate is refused"
    );

    // Resolved properly, the same empty claim is contradicted rather than
    // refused — the two outcomes are different facts and stay different.
    let verdict = verify_matched(&recorded, &declared, &[]).expect("verify");
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
