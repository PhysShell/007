//! The three frozen Step 0B fixtures, driven through the classifier.
//!
//! The fixtures are read from `tests/fixtures/github/` at the workspace root and
//! are NEVER written to. Each test asserts the fixture's hostile precondition
//! FIRST, straight from the JSON, before asserting anything about classifier
//! behaviour — a test that passes because the adversarial condition was never
//! constructed is the failure mode this corpus exists to prevent.
//!
//! THE ENVELOPE IS NOT THE PRODUCTION INPUT SCHEMA. The fixture files carry
//! several API surfaces in one document because a file must. The loader below
//! unwraps those surfaces and builds typed [`ClassifierInput`]s; it is test
//! infrastructure, and the production input model deliberately looks nothing
//! like the envelope.
//!
//! THE LOADER MAY NOT INJECT OUTCOMES. Nowhere below does a fixture name map to
//! an expected state. The loader supplies observable facts — which commit a
//! review is bound to, whether a review comment references it, which checks
//! exist at which SHA — and the classifier decides the states.

// Justification for the restriction-lint allowance, per the precedent in
// `tests/a0_candidate_state_e2e.rs`: every panic path below is this test's own
// assertion or its own reading of a file it ships with.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use o7_closure_classifier::{
    classify, Acquisition, CheckConclusion, CheckEvidence, ClassifierInput, FalsificationFact,
    ObservationInput, Policy, Predicate, ReviewEvidence, SourceKind, State, Subject, Verification,
};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::PathBuf;

/// HARNESS MAPPING, not a fixture fact: #147's policy enumerates observation ids
/// but does not state which vendor check name produces each. Resolving that is
/// the acquisition layer's job; the table is spelled out here so the choice is
/// visible rather than buried in a match arm.
const CHECK_NAME_TO_ID: &[(&str, &str)] = &[
    ("worker gate", "ci/worker-gate"),
    ("dependency policy", "ci/dependency-policy"),
    ("dependency advisories", "ci/dependency-advisories"),
    ("worktree+verifier tests", "ci/worktree-verifier"),
];

/// HARNESS MAPPING, same status as above: which policy observation a given bot's
/// submitted review satisfies.
fn reviewer_id_for(login: &str) -> Option<&'static str> {
    if login.starts_with("chatgpt-codex-connector") {
        Some("review/codex")
    } else if login.starts_with("coderabbitai") {
        Some("review/coderabbit")
    } else {
        None
    }
}

fn policy() -> Policy {
    Policy {
        id: "007/closure/v1".to_owned(),
        required_observations: vec![
            "ci/worker-gate".to_owned(),
            "ci/dependency-policy".to_owned(),
            "ci/dependency-advisories".to_owned(),
            "ci/worktree-verifier".to_owned(),
            "review/codex".to_owned(),
            "review/coderabbit".to_owned(),
        ],
        completeness_claimed: false,
    }
}

fn fixture(name: &str) -> Value {
    let path: PathBuf = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/github")
        .join(name);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("reading frozen fixture {}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("parsing frozen fixture {name}: {e}"))
}

fn ptr<'a>(doc: &'a Value, p: &str) -> &'a Value {
    doc.pointer(p)
        .unwrap_or_else(|| panic!("frozen fixture pointer {p} does not resolve"))
}

fn text<'a>(doc: &'a Value, p: &str) -> &'a str {
    ptr(doc, p)
        .as_str()
        .unwrap_or_else(|| panic!("frozen fixture pointer {p} is not a string"))
}

/// Unwrap the envelope into typed observations. Purely structural: a review
/// carries a finding iff some review comment references it by
/// `pull_request_review_id`. No body text is read.
fn observations_from(doc: &Value) -> BTreeMap<String, ObservationInput> {
    let mut out = BTreeMap::new();

    if let Some(runs) = doc
        .pointer("/check_runs/check_runs")
        .and_then(Value::as_array)
    {
        for run in runs {
            let name = run.get("name").and_then(Value::as_str).unwrap_or_default();
            let Some((_, id)) = CHECK_NAME_TO_ID.iter().find(|(n, _)| *n == name) else {
                continue;
            };
            let head_sha = run
                .get("head_sha")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let success = run.get("conclusion").and_then(Value::as_str) == Some("success");
            out.insert(
                (*id).to_owned(),
                ObservationInput::Check(Acquisition::Available(CheckEvidence {
                    stable_id: run
                        .get("id")
                        .map(std::string::ToString::to_string)
                        .unwrap_or_default(),
                    head_sha: head_sha.to_owned(),
                    conclusion: if success {
                        CheckConclusion::Success
                    } else {
                        CheckConclusion::Failure
                    },
                })),
            );
        }
    }

    let empty = Vec::new();
    let review_comments = doc
        .pointer("/review_comments")
        .and_then(Value::as_array)
        .unwrap_or(&empty);

    if let Some(reviews) = doc.pointer("/reviews").and_then(Value::as_array) {
        for review in reviews {
            let login = review
                .pointer("/user/login")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let Some(id) = reviewer_id_for(login) else {
                continue;
            };
            let review_id = review.get("id").cloned().unwrap_or(Value::Null);
            let carries_finding = review_comments
                .iter()
                .any(|c| c.get("pull_request_review_id") == Some(&review_id));
            out.insert(
                id.to_owned(),
                ObservationInput::Review(Acquisition::Available(ReviewEvidence {
                    stable_id: review_id.to_string(),
                    commit_id: review
                        .get("commit_id")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_owned(),
                    author: login.to_owned(),
                    carries_finding,
                })),
            );
        }
    }

    // Any required reviewer with no submitted review object produced no verdict.
    // Stated explicitly rather than left to be inferred from a missing key.
    for id in policy().required_observations {
        if id.starts_with("review/") {
            out.entry(id)
                .or_insert(ObservationInput::Review(Acquisition::NotProduced));
        }
    }

    out
}

fn classify_fixture(doc: &Value, falsifications: Vec<FalsificationFact>) -> Predicate {
    let head = text(doc, "/pull_request/head/sha").to_owned();
    classify(&ClassifierInput {
        subject: Subject {
            repository: text(doc, "/pull_request/head/repo/full_name").to_owned(),
            pull_request: ptr(doc, "/pull_request/number")
                .as_u64()
                .unwrap_or_default(),
            expected_sha: head.clone(),
            head_before: head.clone(),
            head_after: head,
        },
        policy: policy(),
        observations: observations_from(doc),
        falsifications,
        known_debts: Vec::new(),
    })
}

fn state_of(p: &Predicate, id: &str) -> State {
    p.observations
        .iter()
        .find(|o| o.id == id)
        .map(|o| o.state)
        .unwrap_or_else(|| panic!("observation {id} missing"))
}

// --------------------------------------------------- stale-review-wrong-sha --

#[test]
fn stale_review_wrong_sha() {
    let doc = fixture("stale-review-wrong-sha.json");

    // HOSTILE PRECONDITION, asserted before any classifier expectation.
    let head = text(&doc, "/pull_request/head/sha");
    let review_sha = text(&doc, "/reviews/0/commit_id");
    assert_ne!(
        review_sha, head,
        "fixture must present a review bound to a commit other than the head"
    );
    assert_eq!(
        text(&doc, "/check_runs/check_runs/0/head_sha"),
        head,
        "fixture must present checks bound to the head"
    );

    let p = classify_fixture(&doc, Vec::new());

    // The review does not bind to the subject, so it yields no verdict for it —
    // neither a pass nor a finding.
    assert_eq!(state_of(&p, "review/codex"), State::Owed);
    // The checks that DO bind to the head must not rescue the wrong-SHA review.
    for id in [
        "ci/worker-gate",
        "ci/dependency-policy",
        "ci/dependency-advisories",
        "ci/worktree-verifier",
    ] {
        assert_eq!(state_of(&p, id), State::Pass);
    }
    assert_eq!(state_of(&p, "review/coderabbit"), State::Owed);
    assert_eq!(p.headline, State::Owed);
    assert_ne!(p.headline, State::Pass);
    // The SUBJECT is not stale here — the head never moved. It is the REVIEW
    // artifact that is bound to a superseded commit, which is a different thing
    // and must not be conflated with a stale snapshot.
    assert!(!p.subject_stale);
}

// ------------------------------------------------- falsification-in-comment --

#[test]
fn falsification_in_comment() {
    let doc = fixture("falsification-in-comment.json");

    // HOSTILE PRECONDITION first.
    let head = text(&doc, "/pull_request/head/sha");
    assert_eq!(text(&doc, "/reviews/0/commit_id"), head);
    assert!(
        ptr(&doc, "/review_comments")
            .as_array()
            .is_some_and(Vec::is_empty),
        "fixture must present an empty review-comment surface"
    );
    let comment_author = text(&doc, "/issue_comments/0/user/login");
    assert!(
        !text(&doc, "/issue_comments/0/body").is_empty(),
        "fixture must present a defect claim on the issue-comment surface"
    );

    // The frozen README documents `/issue_comments/0` as carrying a concrete,
    // reproducible defect claim. The harness supplies that as an already
    // established fact WITH ITS PROVENANCE — it does not infer a prose grammar,
    // and nothing here reads the body to decide whether it is "real".
    let falsification = FalsificationFact {
        source_kind: SourceKind::IssueComment,
        stable_id: ptr(&doc, "/issue_comments/0/id").to_string(),
        subject_sha: None,
        author: comment_author.to_owned(),
        verification: Verification::Reproduced,
    };

    let p = classify_fixture(&doc, vec![falsification]);

    // The review surface at the head records no finding, so it passes as a
    // verdict...
    assert_eq!(state_of(&p, "review/codex"), State::Pass);
    // ...the other required reviewer produced no verdict object at all...
    assert_eq!(state_of(&p, "review/coderabbit"), State::Owed);
    // ...and the claim on the verdict-less surface still falsifies.
    assert_eq!(p.falsifications.len(), 1);
    assert_eq!(p.headline, State::Finding);
}

// ---------------------------------------------- conflicting-review-surfaces --

#[test]
fn conflicting_review_surfaces() {
    let doc = fixture("conflicting-review-surfaces.json");

    // HOSTILE PRECONDITION first: one vendor, one head, two surfaces.
    let head = text(&doc, "/pull_request/head/sha");
    assert_eq!(text(&doc, "/reviews/0/commit_id"), head);
    assert_eq!(text(&doc, "/review_comments/0/commit_id"), head);
    assert_eq!(
        text(&doc, "/reviews/0/user/login"),
        text(&doc, "/issue_comments/0/user/login"),
        "fixture must present the same author on both surfaces"
    );
    assert_eq!(
        ptr(&doc, "/review_comments/0/pull_request_review_id"),
        ptr(&doc, "/reviews/0/id"),
        "the review comment must belong to that submitted review"
    );

    let p = classify_fixture(&doc, Vec::new());

    // The submitted, commit-bound review controls verdict accounting, and it
    // carries a finding. The issue comment claiming the same commit is clean has
    // NO channel into positive evidence, so it cannot overturn this — and it is
    // not deleted either: it remains in the fixture, unread by the classifier.
    assert_eq!(state_of(&p, "review/codex"), State::Finding);
    assert_eq!(state_of(&p, "review/coderabbit"), State::Owed);
    assert_eq!(p.headline, State::Finding);
    assert_ne!(p.headline, State::Pass);
}

#[test]
fn frozen_fixtures_are_never_written_to() {
    // Reading is the only access this file makes; recorded as an assertion so a
    // future edit that starts writing has to delete a line that says not to.
    for name in [
        "stale-review-wrong-sha.json",
        "falsification-in-comment.json",
        "conflicting-review-surfaces.json",
    ] {
        let before = fixture(name);
        let _ = classify_fixture(&before, Vec::new());
        assert_eq!(before, fixture(name));
    }
}
