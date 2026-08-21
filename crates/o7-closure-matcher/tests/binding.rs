//! The binding itself: resolution, conformance and recomputation.

// Justification for the restriction-lint allowance, per AGENTS.md rule 4 and the
// precedent in `crates/o7-closure-classifier/tests/frozen_fixtures.rs`: every
// panic path below is this test's own assertion failing, or its own parsing of a
// JSON literal written a few lines above the parse. Nothing here runs against
// production input.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use o7_closure_canonical::digest;
use o7_closure_matcher::{
    recompute_matched, resolve, verify_binding, verify_matched, Candidate, MatchError,
    RecordedImplementation, RecordedMatcher, REGISTRY,
};
use serde_json::{json, Value};

/// Every registered matcher behaves as the version bound to it, on every frozen
/// vector, and hashes to the digest that version carries.
#[test]
fn every_registered_matcher_matches_its_binding() {
    assert!(!REGISTRY.is_empty(), "an empty registry binds nothing");
    for entry in REGISTRY {
        verify_binding(entry).unwrap_or_else(|e| panic!("{}/{}: {e}", entry.id, entry.version));
        assert!(
            !entry.vectors.is_empty(),
            "{}/{}: a binding over zero vectors detects no behaviour change",
            entry.id,
            entry.version
        );
        assert!(
            entry.vectors.iter().any(|v| v.expected) && entry.vectors.iter().any(|v| !v.expected),
            "{}/{}: vectors must witness both outcomes, or a constant predicate passes",
            entry.id,
            entry.version
        );
    }
}

#[test]
fn identity_pairs_are_unique() {
    for (i, a) in REGISTRY.iter().enumerate() {
        for b in REGISTRY.iter().skip(i + 1) {
            assert!(
                !(a.id == b.id && a.version == b.version),
                "{}/{} is bound twice — an identity pair must resolve to one predicate",
                a.id,
                a.version
            );
        }
    }
}

#[test]
fn resolution_fails_closed() {
    assert!(resolve("review-by-expected-author-login", "1").is_ok());

    assert!(matches!(
        resolve("no-such-matcher", "1"),
        Err(MatchError::UnknownMatcherId { .. })
    ));
    assert!(matches!(
        resolve("review-by-expected-author-login", "2"),
        Err(MatchError::UnknownMatcherVersion { .. })
    ));
    // An unknown version is a different fact from an unknown id, and the two must
    // not collapse: one says the rule was never bound, the other that this
    // repository does not have the revision a record was produced by.
    assert!(!matches!(
        resolve("review-by-expected-author-login", "2"),
        Err(MatchError::UnknownMatcherId { .. })
    ));
}

/// §13.1: parameters are exactly what the matcher reads.
#[test]
fn parameters_must_be_exactly_what_the_matcher_reads() {
    let candidates: Vec<Candidate> = Vec::new();
    let ok = json!({"expectedAuthorLogin": "a"});
    assert!(recompute_matched(
        &named("review-by-expected-author-login", "1", &ok),
        &candidates
    )
    .is_ok());

    for bad in [
        json!({}),
        json!({"expectedAuthorLogin": "a", "extra": 1}),
        json!({"wrongKey": "a"}),
    ] {
        assert!(
            matches!(
                recompute_matched(
                    &named("review-by-expected-author-login", "1", &bad),
                    &candidates
                ),
                Err(MatchError::ParameterMismatch { .. })
            ),
            "{bad} should be refused"
        );
    }
    assert!(matches!(
        recompute_matched(
            &named("review-by-expected-author-login", "1", &json!("nope")),
            &candidates
        ),
        Err(MatchError::MalformedCandidate { .. })
    ));
}

/// A recorded matcher block that agrees with this tree by construction.
///
/// These tests are about selection — order, duplicates, parameters, surface —
/// not about implementation drift, so the recorded digest is taken from the
/// resolved entry. That makes it useless as a drift check, which is exactly why
/// the drift checks live in `recorded_implementation.rs` and read their expected
/// digest out of a frozen fixture instead of out of the registry.
fn named(id: &str, version: &str, parameters: &Value) -> RecordedMatcher {
    RecordedMatcher {
        id: id.to_owned(),
        version: version.to_owned(),
        parameters: parameters.clone(),
        implementation: match resolve(id, version) {
            Ok(entry) => RecordedImplementation::Bound(entry.implementation_digest.to_owned()),
            Err(_) => RecordedImplementation::Unrecorded,
        },
    }
}

fn review(stable_id: &str, login: &str) -> Value {
    json!({
        "schemaVersion": 1, "sourceKind": "github-submitted-review", "stableId": stable_id,
        "user": {"id": "1", "login": login, "type": "User"},
        "authorAssociation": "NONE", "state": "APPROVED", "body": "",
        "submittedAt": "2026-01-01T00:00:00Z",
        "commitId": "1111111111111111111111111111111111111111",
    })
}

fn candidate(v: Value) -> Candidate {
    Candidate {
        declared_digest: digest(&v).expect("digest").as_str().to_owned(),
        snapshot: v,
    }
}

/// §13.2: the matched list is a subsequence — same members, same relative order,
/// duplicates preserved.
#[test]
fn the_matched_list_is_a_subsequence_in_observation_order() {
    let a = candidate(review("1", "wanted"));
    let b = candidate(review("2", "other"));
    let c = candidate(review("3", "wanted"));
    let params = json!({"expectedAuthorLogin": "wanted"});

    let matched = recompute_matched(
        &named("review-by-expected-author-login", "1", &params),
        &[a.clone(), b.clone(), c.clone()],
    )
    .expect("recompute");
    assert_eq!(
        matched.matched,
        vec![a.declared_digest.clone(), c.declared_digest.clone()]
    );

    // Reversing the candidates reverses the matched order: the selection carries
    // observation order rather than imposing one of its own.
    let reversed = recompute_matched(
        &named("review-by-expected-author-login", "1", &params),
        &[c.clone(), b, a.clone()],
    )
    .expect("recompute");
    assert_eq!(
        reversed.matched,
        vec![c.declared_digest.clone(), a.declared_digest.clone()]
    );

    // A duplicate in the candidate sequence appears twice in the matched one.
    let twice = recompute_matched(
        &named("review-by-expected-author-login", "1", &params),
        &[a.clone(), a.clone()],
    )
    .expect("recompute");
    assert_eq!(
        twice.matched.len(),
        2,
        "§13.2 retains duplicates rather than collapsing them"
    );
}

/// An empty candidate sequence yields an empty matched sequence — and that is a
/// fact about the enumeration, produced by running the predicate over nothing,
/// not by an early return.
#[test]
fn an_empty_candidate_sequence_yields_an_empty_match() {
    let matched = recompute_matched(
        &named(
            "review-by-expected-author-login",
            "1",
            &json!({"expectedAuthorLogin": "wanted"}),
        ),
        &[],
    )
    .expect("recompute");
    assert!(matched.matched.is_empty());
}

/// The candidate binding is derived. A snapshot that does not hash to the digest
/// declared beside it is refused, so recomputation cannot be run over a producer's
/// account of what the candidates were.
#[test]
fn a_candidate_whose_digest_does_not_hold_is_refused() {
    let mut tampered = candidate(review("1", "wanted"));
    tampered.snapshot = review("1", "somebody-else");
    assert!(matches!(
        recompute_matched(
            &named(
                "review-by-expected-author-login",
                "1",
                &json!({"expectedAuthorLogin": "wanted"}),
            ),
            &[tampered],
        ),
        Err(MatchError::CandidateDigestMismatch { .. })
    ));
}

/// The delivery-surface law from issue #147, exercised through the public API
/// rather than only as a frozen vector: same author, wrong API surface.
#[test]
fn an_issue_comment_by_the_expected_author_is_not_review_evidence() {
    let comment = candidate(json!({
        "schemaVersion": 1, "sourceKind": "github-issue-comment", "stableId": "9",
        "user": {"id": "1", "login": "wanted", "type": "Bot"},
        "authorAssociation": "NONE",
        "body": "No actionable defects found in 1111111111111111111111111111111111111111",
        "createdAt": "2026-01-01T00:00:00Z", "updatedAt": "2026-01-01T00:00:00Z",
    }));
    let matched = recompute_matched(
        &named(
            "review-by-expected-author-login",
            "1",
            &json!({"expectedAuthorLogin": "wanted"}),
        ),
        &[comment],
    )
    .expect("recompute");
    assert!(
        matched.matched.is_empty(),
        "a positive verdict in a comment is not weak review evidence, it is not \
         review evidence — classification follows the API object shape"
    );
}

#[test]
fn verify_matched_compares_against_the_claim() {
    let a = candidate(review("1", "wanted"));
    let b = candidate(review("2", "other"));
    let params = json!({"expectedAuthorLogin": "wanted"});
    let cands = [a.clone(), b.clone()];

    assert!(
        verify_matched(
            &named("review-by-expected-author-login", "1", &params),
            &cands,
            std::slice::from_ref(&a.declared_digest)
        )
        .expect("verify")
        .reproduced
    );
    // A claim that the non-matching candidate qualified.
    assert!(
        !verify_matched(
            &named("review-by-expected-author-login", "1", &params),
            &cands,
            std::slice::from_ref(&b.declared_digest)
        )
        .expect("verify")
        .reproduced
    );
    // A claim of an empty match where one candidate qualifies — the exact shape
    // §13 exists to make detectable.
    assert!(
        !verify_matched(
            &named("review-by-expected-author-login", "1", &params),
            &cands,
            &[]
        )
        .expect("verify")
        .reproduced
    );
}
