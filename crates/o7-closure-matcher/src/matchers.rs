//! The registry. Adding a matcher is a reviewable diff in this file, and an
//! identity pair is meaningless until it appears here.
//!
//! TWO DIGESTS, TWO DIFFERENT JOBS. RED-2 (782cbaf) put a wrong matcher through
//! a fully green suite, because the conformance digest covers results on a
//! finite vector set and §13.1 requires identity over ANY input. So each entry
//! now carries both:
//!
//! ```text
//! implementation_digest   SHA-256 of the exact bytes of the file that defines
//!                         the predicate. Covers the implementation itself, so
//!                         no edit escapes it and no enumeration is involved.
//!                         This is the identity of (id, version).
//!
//! conformance_digest      SHA-256 over the results on the frozen vectors.
//!                         A behavioural regression witness, and nothing more.
//!                         Kept because it says what the rule is SUPPOSED to do,
//!                         which the bytes never state.
//! ```
//!
//! THE BYTES HASHED ARE THE BYTES COMPILED. `include_str!` and `mod` read the
//! same path in the same build, so `implementation_source` cannot drift from the
//! code that runs. That link, not a vector set, is what makes the digest an
//! implementation binding.
//!
//! APPEND-ONLY, ENFORCED BY THE DIGEST ITSELF. A version's file is never edited;
//! a behaviour change adds `..._v2.rs` and a new entry. Editing `..._v1.rs`
//! breaks v1's binding — the enforcement needs no policy, no CI rule and no
//! reviewer remembering.
//!
//! WHY THE VECTORS LIVE HERE AND NOT BESIDE THE PREDICATE. If they shared a file
//! with the implementation, changing the witness set and changing the rule would
//! move the same digest, and the two facts would stop being separable.

use crate::{CandidateRequirement, ConformanceVector, MatcherEntry};

pub(crate) mod actions_check_by_name_v1;
pub(crate) mod review_by_expected_author_login_v1;

pub(crate) const ALL: &[MatcherEntry] =
    &[REVIEW_BY_EXPECTED_AUTHOR_LOGIN_V1, ACTIONS_CHECK_BY_NAME_V1];

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
    implementation_source: include_str!("matchers/review_by_expected_author_login_v1.rs"),
    implementation_digest:
        "sha256:59ea097cb9ea6705ebff04487a35861af0ea2b1b8e3e7c4980485af52f9567e9",
    conformance_digest: "sha256:7ea10c56ced0cc83ac3889750fd2a133584275d39f6f5fe809f744ebf74c5178",
    candidate_requirement: CandidateRequirement {
        source_kind: "github-submitted-review",
        required_strings: &["/user/login"],
    },
    predicate: review_by_expected_author_login_v1::matches,
};

pub(crate) const ACTIONS_CHECK_BY_NAME_V1: MatcherEntry = MatcherEntry {
    id: "actions-check-by-name",
    version: "1",
    parameter_keys: &["checkName"],
    vectors: CHECK_BY_NAME_VECTORS,
    implementation_source: include_str!("matchers/actions_check_by_name_v1.rs"),
    implementation_digest:
        "sha256:0400752fb274d0ab8e1540ce39b95fb9c122749612560519f54882074618c4c4",
    conformance_digest: "sha256:524e0585621242e6e7a995f952473dd15b01d3da66663c6505fc244a7714d01a",
    candidate_requirement: CandidateRequirement {
        source_kind: "github-actions-check",
        required_strings: &["/name"],
    },
    predicate: actions_check_by_name_v1::matches,
};
