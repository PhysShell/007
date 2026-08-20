//! The bound matchers. Adding one is a reviewable diff in this file, and its
//! identity pair is meaningless until it appears here.
//!
//! Every predicate below is a `fn`, so none of them can capture anything, and
//! each reads its two arguments only.
//!
//! WHY THESE MATCHERS DO NOT LOOK AT A SHA. Selecting by subject SHA is
//! deliberately not a matcher's job. A wrong-SHA review that the matcher filtered
//! out would leave an empty matched subsequence, and an empty matched subsequence
//! after a complete enumeration is exactly what provenance V1 §13 permits to mean
//! `NotProduced` — so a stale review would become "nobody reviewed", collapsing
//! the distinction the frozen `stale-review-wrong-sha` fixture exists to preserve.
//! The matcher selects the candidates a required observation is about; the
//! classifier decides admissibility, and a wrong SHA has to be *visible* to be
//! judged.
//!
//! WHY THEY LOOK AT `sourceKind` AND NOT ONLY AT AN AUTHOR. Recorded on issue
//! #147 as the reviewer verdict delivery-surface witness: a positive verdict
//! delivered in an issue comment is not weak review evidence, it is not review
//! evidence at all, because an issue comment has no field that can carry a
//! subject binding. Classification follows the API object shape, never the
//! author's identity — so `review-by-expected-author-login` requires the candidate to
//! *be* a submitted review, and an issue comment by the same account is false.

use serde_json::Value;

use crate::{ConformanceVector, MatchError, MatcherEntry};

pub(crate) const ALL: &[MatcherEntry] =
    &[REVIEW_BY_EXPECTED_AUTHOR_LOGIN_V1, ACTIONS_CHECK_BY_NAME_V1];

fn string_at<'a>(candidate: &'a Value, pointer: &str) -> Option<&'a str> {
    candidate.pointer(pointer).and_then(Value::as_str)
}

/// True when the candidate is a submitted review whose author login is the one
/// the parameters name.
fn review_by_expected_author_login(
    candidate: &Value,
    parameters: &Value,
) -> Result<bool, MatchError> {
    let expected = parameters
        .get("expectedAuthorLogin")
        .and_then(Value::as_str)
        .ok_or(MatchError::MalformedCandidate {
            why: "expectedAuthorLogin is not a string",
        })?;
    if string_at(candidate, "/sourceKind") != Some("github-submitted-review") {
        return Ok(false);
    }
    Ok(string_at(candidate, "/user/login") == Some(expected))
}

/// True when the candidate is a check run with the named check.
fn actions_check_by_name(candidate: &Value, parameters: &Value) -> Result<bool, MatchError> {
    let expected = parameters.get("checkName").and_then(Value::as_str).ok_or(
        MatchError::MalformedCandidate {
            why: "checkName is not a string",
        },
    )?;
    if string_at(candidate, "/sourceKind") != Some("github-actions-check") {
        return Ok(false);
    }
    Ok(string_at(candidate, "/name") == Some(expected))
}

const REVIEW_BY_AUTHOR_VECTORS: &[ConformanceVector] = &[
    ConformanceVector {
        name: "submitted review by the expected author",
        parameters: r#"{"expectedAuthorLogin":"expected-reviewer"}"#,
        candidate: r#"{"schemaVersion":1,"sourceKind":"github-submitted-review","stableId":"1","user":{"id":"1","login":"expected-reviewer","type":"User"},"authorAssociation":"NONE","state":"APPROVED","body":"","submittedAt":"2026-01-01T00:00:00Z","commitId":"1111111111111111111111111111111111111111"}"#,
        expected: true,
    },
    ConformanceVector {
        name: "submitted review by somebody else",
        parameters: r#"{"expectedAuthorLogin":"expected-reviewer"}"#,
        candidate: r#"{"schemaVersion":1,"sourceKind":"github-submitted-review","stableId":"2","user":{"id":"2","login":"other-reviewer","type":"User"},"authorAssociation":"NONE","state":"APPROVED","body":"","submittedAt":"2026-01-01T00:00:00Z","commitId":"1111111111111111111111111111111111111111"}"#,
        expected: false,
    },
    ConformanceVector {
        // The #147 delivery-surface law, as a vector: same author, wrong surface.
        name: "issue comment by the expected author is not a review",
        parameters: r#"{"expectedAuthorLogin":"expected-reviewer"}"#,
        candidate: r#"{"schemaVersion":1,"sourceKind":"github-issue-comment","stableId":"3","user":{"id":"1","login":"expected-reviewer","type":"User"},"authorAssociation":"NONE","body":"looks clean on 1111111111111111111111111111111111111111","createdAt":"2026-01-01T00:00:00Z","updatedAt":"2026-01-01T00:00:00Z"}"#,
        expected: false,
    },
    ConformanceVector {
        name: "review comment by the expected author is not a submitted review",
        parameters: r#"{"expectedAuthorLogin":"expected-reviewer"}"#,
        candidate: r#"{"schemaVersion":1,"sourceKind":"github-review-comment","stableId":"4","pullRequestReviewId":"9","user":{"id":"1","login":"expected-reviewer","type":"User"},"authorAssociation":"NONE","body":"","commitId":"1111111111111111111111111111111111111111","originalCommitId":"1111111111111111111111111111111111111111","path":"a","createdAt":"2026-01-01T00:00:00Z","updatedAt":"2026-01-01T00:00:00Z"}"#,
        expected: false,
    },
    ConformanceVector {
        // A wrong-SHA review still MATCHES. The matcher makes it visible; the
        // classifier decides it is inadmissible. See this module's header.
        name: "submitted review by the expected author on another commit still matches",
        parameters: r#"{"expectedAuthorLogin":"expected-reviewer"}"#,
        candidate: r#"{"schemaVersion":1,"sourceKind":"github-submitted-review","stableId":"5","user":{"id":"1","login":"expected-reviewer","type":"User"},"authorAssociation":"NONE","state":"APPROVED","body":"","submittedAt":"2026-01-01T00:00:00Z","commitId":"2222222222222222222222222222222222222222"}"#,
        expected: true,
    },
    ConformanceVector {
        name: "login differing only in case does not match",
        parameters: r#"{"expectedAuthorLogin":"expected-reviewer"}"#,
        candidate: r#"{"schemaVersion":1,"sourceKind":"github-submitted-review","stableId":"6","user":{"id":"6","login":"Expected-Reviewer","type":"User"},"authorAssociation":"NONE","state":"APPROVED","body":"","submittedAt":"2026-01-01T00:00:00Z","commitId":"1111111111111111111111111111111111111111"}"#,
        expected: false,
    },
];

const CHECK_BY_NAME_VECTORS: &[ConformanceVector] = &[
    ConformanceVector {
        name: "check run with the named check",
        parameters: r#"{"checkName":"worker gate"}"#,
        candidate: r#"{"schemaVersion":1,"sourceKind":"github-actions-check","stableId":"1","name":"worker gate","headSha":"1111111111111111111111111111111111111111","status":"completed","conclusion":"success"}"#,
        expected: true,
    },
    ConformanceVector {
        name: "check run with another name",
        parameters: r#"{"checkName":"worker gate"}"#,
        candidate: r#"{"schemaVersion":1,"sourceKind":"github-actions-check","stableId":"2","name":"dependency policy","headSha":"1111111111111111111111111111111111111111","status":"completed","conclusion":"success"}"#,
        expected: false,
    },
    ConformanceVector {
        // Selection is by name, not by outcome: a failed run of the required
        // check must be selected so the classifier can see that it failed.
        name: "a failed run of the named check still matches",
        parameters: r#"{"checkName":"worker gate"}"#,
        candidate: r#"{"schemaVersion":1,"sourceKind":"github-actions-check","stableId":"3","name":"worker gate","headSha":"1111111111111111111111111111111111111111","status":"completed","conclusion":"failure"}"#,
        expected: true,
    },
    ConformanceVector {
        name: "a submitted review is not a check run",
        parameters: r#"{"checkName":"worker gate"}"#,
        candidate: r#"{"schemaVersion":1,"sourceKind":"github-submitted-review","stableId":"4","user":{"id":"1","login":"worker gate","type":"Bot"},"authorAssociation":"NONE","state":"APPROVED","body":"","submittedAt":"2026-01-01T00:00:00Z","commitId":"1111111111111111111111111111111111111111"}"#,
        expected: false,
    },
];

pub(crate) const REVIEW_BY_EXPECTED_AUTHOR_LOGIN_V1: MatcherEntry = MatcherEntry {
    id: "review-by-expected-author-login",
    version: "1",
    parameter_keys: &["expectedAuthorLogin"],
    vectors: REVIEW_BY_AUTHOR_VECTORS,
    conformance_digest: "sha256:7ea10c56ced0cc83ac3889750fd2a133584275d39f6f5fe809f744ebf74c5178",
    predicate: review_by_expected_author_login,
};

pub(crate) const ACTIONS_CHECK_BY_NAME_V1: MatcherEntry = MatcherEntry {
    id: "actions-check-by-name",
    version: "1",
    parameter_keys: &["checkName"],
    vectors: CHECK_BY_NAME_VECTORS,
    conformance_digest: "sha256:524e0585621242e6e7a995f952473dd15b01d3da66663c6505fc244a7714d01a",
    predicate: actions_check_by_name,
};
