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
    RecordedMatcher, RecordedQuerySnapshot, REGISTRY,
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
        &named_over("review-by-expected-author-login", "1", &ok, &candidates),
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
                    &named_over("review-by-expected-author-login", "1", &bad, &candidates),
                    &candidates
                ),
                Err(MatchError::ParameterMismatch { .. })
            ),
            "{bad} should be refused"
        );
    }
    // A non-object `parameters` is refused one layer earlier than the three
    // above: §13 fixes the matcher block's shape, so replay never receives this
    // snapshot at all. The three cases above are the ones §13 deliberately does
    // NOT fix — parameters is an open object whose admissible keys are whichever
    // the *named* matcher reads, which is knowable only once the matcher is
    // resolved. That split is why both checks exist and neither subsumes the
    // other: the shape is the contract's, the key set is the matcher's.
    let implementation = resolve("review-by-expected-author-login", "1")
        .ok()
        .map(|e| e.implementation_digest);
    assert!(matches!(
        try_construct(&snapshot_document(
            "review-by-expected-author-login",
            "1",
            &json!("nope"),
            &[],
            &[],
            implementation,
        )),
        Err(MatchError::MalformedQuerySnapshot { .. })
    ));
}

/// A recorded matcher that agrees with this tree by construction.
///
/// These tests are about selection — order, duplicates, parameters, surface —
/// not about implementation drift, so the recorded digest is the resolved one.
/// That makes them useless as a drift check, which is why the drift checks live
/// in `recorded_implementation.rs` and read their expectation out of a fixture.
fn named(id: &str, version: &str, parameters: &Value) -> RecordedMatcher {
    named_over(id, version, parameters, &[])
}

fn named_over(
    id: &str,
    version: &str,
    parameters: &Value,
    candidates: &[Candidate],
) -> RecordedMatcher {
    named_claiming(id, version, parameters, candidates, Vec::new())
}

fn named_claiming(
    id: &str,
    version: &str,
    parameters: &Value,
    candidates: &[Candidate],
    claimed: Vec<String>,
) -> RecordedMatcher {
    let declared: Vec<String> = candidates
        .iter()
        .map(|c| c.declared_digest.clone())
        .collect();
    let implementation = resolve(id, version).ok().map(|e| e.implementation_digest);
    snapshot(id, version, parameters, &declared, &claimed, implementation)
}

/// Build a `github-query-snapshot` and parse it, because that is the only way to
/// obtain a `RecordedMatcher`.
///
/// Deliberately not a shortcut past the parser: a test that could assemble one
/// field-by-field would be testing a value production code can never see.
fn snapshot(
    id: &str,
    version: &str,
    parameters: &Value,
    all_returned: &[String],
    claimed: &[String],
    implementation: Option<&str>,
) -> RecordedMatcher {
    let document = snapshot_document(
        id,
        version,
        parameters,
        all_returned,
        claimed,
        implementation,
    );
    try_construct(&document).expect("a well-formed query snapshot parses")
}

/// The document `snapshot` builds, for the cases that need to watch it be
/// refused rather than parsed.
fn snapshot_document(
    id: &str,
    version: &str,
    parameters: &Value,
    all_returned: &[String],
    claimed: &[String],
    implementation: Option<&str>,
) -> Value {
    let mut matcher = json!({
        "id": id,
        "version": version,
        "parameters": parameters.clone(),
    });
    let schema_version = match implementation {
        Some(digest) => {
            matcher
                .as_object_mut()
                .expect("the matcher block is an object")
                .insert("implementationDigest".to_owned(), json!(digest));
            2
        }
        None => 1,
    };
    // The digest is computed from the snapshot this helper just built, so it is
    // self-consistent by construction and proves nothing about authority. That is
    // deliberate: these tests are about selection semantics, and the binding's own
    // discriminating witnesses live in `recorded_implementation.rs`, where the
    // expected digest is read out of a frozen fixture computed outside this
    // workspace.
    // Complete per §13, not merely the members these tests read. A partial
    // document is refused at construction now, which is the point of that check:
    // a test helper able to build a snapshot production could never accept would
    // be exercising a shape no artifact can have. The members below are fixed
    // because nothing here varies them; `query_snapshot_schema.rs` is where each
    // one is varied on purpose.
    json!({
        "schemaVersion": schema_version,
        "sourceKind": "github-query-snapshot",
        "surface": "pull-request-reviews",
        "requiredObservationId": "review/external-auditor",
        "binding": {"repository": "PhysShell/007", "pullRequest": "9001"},
        "pagination": {
            "perPage": 100,
            "pagesRequested": ["1"],
            "pagesObtained": ["1"],
            "nextPagePresent": false,
        },
        "enumeration": "COMPLETE",
        "matcher": matcher,
        "allReturnedSnapshotDigests": all_returned,
        "matchedSnapshotDigests": claimed,
    })
}

/// The same construction without the `expect`, for the cases that are about the
/// snapshot being refused rather than about what replay does with it.
fn try_construct(document: &Value) -> Result<RecordedMatcher, MatchError> {
    let bound = o7_closure_canonical::digest(document).expect("digest");
    Ok(
        RecordedQuerySnapshot::from_canonical(document, bound.as_str())?
            .recorded_matcher()
            .clone(),
    )
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

    let abc = [a.clone(), b.clone(), c.clone()];
    let matched = recompute_matched(
        &named_over("review-by-expected-author-login", "1", &params, &abc),
        &abc,
    )
    .expect("recompute");
    assert_eq!(
        matched.matched,
        vec![a.declared_digest.clone(), c.declared_digest.clone()]
    );

    // Reversing the candidates reverses the matched order: the selection carries
    // observation order rather than imposing one of its own.
    let cba = [c.clone(), b, a.clone()];
    let reversed = recompute_matched(
        &named_over("review-by-expected-author-login", "1", &params, &cba),
        &cba,
    )
    .expect("recompute");
    assert_eq!(
        reversed.matched,
        vec![c.declared_digest.clone(), a.declared_digest.clone()]
    );

    // A duplicate in the candidate sequence appears twice in the matched one.
    let aa = [a.clone(), a.clone()];
    let twice = recompute_matched(
        &named_over("review-by-expected-author-login", "1", &params, &aa),
        &aa,
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
            &named_over(
                "review-by-expected-author-login",
                "1",
                &json!({"expectedAuthorLogin": "wanted"}),
                std::slice::from_ref(&tampered),
            ),
            std::slice::from_ref(&tampered),
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
    let only = [comment];
    let matched = recompute_matched(
        &named_over(
            "review-by-expected-author-login",
            "1",
            &json!({"expectedAuthorLogin": "wanted"}),
            &only,
        ),
        &only,
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
            &named_claiming(
                "review-by-expected-author-login",
                "1",
                &params,
                &cands,
                vec![a.declared_digest.clone()]
            ),
            &cands
        )
        .expect("verify")
        .reproduced
    );
    // A claim that the non-matching candidate qualified.
    assert!(
        !verify_matched(
            &named_claiming(
                "review-by-expected-author-login",
                "1",
                &params,
                &cands,
                vec![b.declared_digest.clone()]
            ),
            &cands
        )
        .expect("verify")
        .reproduced
    );
    // A claim of an empty match where one candidate qualifies — the exact shape
    // §13 exists to make detectable.
    assert!(
        !verify_matched(
            &named_claiming(
                "review-by-expected-author-login",
                "1",
                &params,
                &cands,
                Vec::new()
            ),
            &cands
        )
        .expect("verify")
        .reproduced
    );
}
