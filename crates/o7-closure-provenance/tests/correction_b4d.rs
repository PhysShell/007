//! RED-B4D — a derived fact must survive a representation change.
//!
//! Deliberately a separate group from RED-B4R, and deliberately small. This is
//! not a new theory about admission; it is a completeness bug, and shoving it
//! under "relation validity" to make the round look tidy would misdescribe it.
//!
//! ```text
//! RED-B4R   relation / authority qualification   CX-1 CX-2 CX-4 CX-5
//! RED-B4D   derived-fact reduced-record completeness   CX-3   <- this file
//! ```
//!
//! THE CLAIM, and it is one sentence:
//!
//! > The same derivation reproduces the same fact whenever every input it reads
//! > survived, whether the sources are complete §8 projections or reduced
//! > records.
//!
//! WHY IT FAILS TODAY. `check_derived` resolves every cited source through the
//! B4 door — correctly — and then does this:
//!
//! ```text
//! ValidatedArtifact  ->  as_value()  ->  derivation over raw JSON
//! ```
//!
//! which throws away the one abstraction that distinguishes a complete
//! projection from a reduced record. The predicate then reads canonical
//! `/stableId` and `/pullRequestReviewId` from a record keyed in §5.3's decoded
//! space, both lookups miss, and the fact comes back `CannotCheck`.
//!
//! The evidence survived, the authority survived, validation succeeded — and the
//! final accessor pretended none of it had happened.
//!
//! WHAT THIS COSTS, AND WHY IT IS NOT MERELY COSMETIC. Redaction §8 exists so
//! that a derived fact whose required inputs survived redaction REMAINS usable;
//! §7.5 says a reduced record resolves its fields through `retainedFields` in
//! the decoded space. Together they mean the R6/R7 pair — same gate outcome,
//! opposite admissibility — has a derived-fact half that this crate currently
//! cannot express. The failure direction matters too: this LOSES evidence rather
//! than admitting bad evidence. A false `CannotCheck` is the safe direction and
//! is still wrong, because a system that refuses what it was built to preserve
//! stops being used.
//!
//! THE REPORTED REFUSAL IS ALSO MISLEADING. `DerivationDisagrees` with
//! `recomputed: Null` reads as "the sources do not imply the claim". Nothing
//! disagreed. The derivation could not read its inputs, which is a different
//! fact and the one a reader needs.
//!
//! ```text
//! D1  complete sources, inputs present   -> the derivation reproduces  BOUNDARY
//! D2  reduced sources, same inputs retained -> the SAME fact reproduces     RED
//! D3  reduced source, one required input blocked -> CannotCheck       BOUNDARY
//! D4  a locator value never substitutes for retained evidence         BOUNDARY
//! ```
//!
//! D4 is carried here rather than assumed: §7.3 forbids a locator value from
//! satisfying a decision-basis pointer, and a fix that taught the derivation
//! layer to look "wherever the value happens to be" would satisfy D2 by
//! violating D4. The two must be closed together or not at all.

// Justification for the restriction-lint allowance, per AGENTS.md rule 4: the
// `expect` sites below are this file's own handling of JSON literals written in
// it, unreachable unless a specimen a few lines above is malformed.
#![allow(clippy::expect_used)]

use o7_closure_canonical::digest;
use o7_closure_provenance::{
    relations_checked, Admissible, DecisionBasis, DerivedFact, ExpectedDetector, RetainedEvidence,
};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

/// This file's witnesses ask about ONE relation each, over a basis built to
/// isolate it. That is not a §17 decision basis and was never meant to be, so
/// they ask `relation_refusals` — "did anything this basis NAMED fail?" —
/// rather than `admissibility`, which additionally requires §17's minimum for
/// the decision being made. Shaped back into `Admissible` so the assertions
/// below read as they always have.
///
/// The distinction is not cosmetic. Before `admissibility` took a profile,
/// these witnesses' `admits` assertions claimed that a basis carrying one
/// pointer was an admissible DECISION. That was only ever true because nothing
/// checked completeness. `correction_g2.rs` carries the decision-level claim.
fn relations<E: RetainedEvidence>(basis: &DecisionBasis, store: &E) -> Admissible {
    match relations_checked(basis, store) {
        Ok(values) => Admissible::Yes { values },
        Err(why) => Admissible::CannotCheck { why },
    }
}

const REVIEW_REQ: [&str; 9] = [
    "/author_association",
    "/body",
    "/commit_id",
    "/id",
    "/state",
    "/submitted_at",
    "/user/id",
    "/user/login",
    "/user/type",
];
const COMMENT_REQ: [&str; 12] = [
    "/author_association",
    "/body",
    "/commit_id",
    "/created_at",
    "/id",
    "/original_commit_id",
    "/path",
    "/pull_request_review_id",
    "/updated_at",
    "/user/id",
    "/user/login",
    "/user/type",
];

const REVIEW_ID: &str = "9000000901";
const COMMENT_ID: &str = "9000000202";

#[derive(Default)]
struct Store {
    records: BTreeMap<String, Value>,
    bindings: BTreeMap<String, Value>,
}
impl Store {
    fn put(&mut self, o: &Value) -> String {
        let d = digest(o).expect("digest").as_str().to_owned();
        self.records.insert(d.clone(), o.clone());
        d
    }
    fn retain_under(&mut self, record: &Value, assessment: &Value) -> String {
        let ad = self.put(assessment);
        let rd = self.put(record);
        let binding = json!({
            "schemaVersion": 1,
            "sourceKind": "closure-retention-binding",
            "recordDigest": rd,
            "assessmentDigest": ad,
        });
        self.put(&binding);
        self.bindings.insert(rd.clone(), binding);
        rd
    }
}
impl RetainedEvidence for Store {
    fn resolve(&self, d: &str) -> Option<Value> {
        self.records.get(d).cloned()
    }
    fn binding_for(&self, record_digest: &str) -> Option<Value> {
        self.bindings.get(record_digest).cloned()
    }
}

fn assessment(assessed: &[&str], outcome: &str, findings: Option<Value>) -> Value {
    let mut a = json!({
        "schemaVersion": 1,
        "sourceKind": "closure-retention-assessment",
        "redactionPolicyVersion": "1",
        "detector": {
            "id": "synthetic-detector",
            "version": "1",
            "configDigest": "sha256:1111111111111111111111111111111111111111111111111111111111111111",
        },
        "representation": "decoded-source-field-values",
        "assessedFields": assessed,
        "coverageComplete": true,
        "outcome": outcome,
        "observedAt": "2026-08-05T09:03:00Z",
    });
    if let Some(f) = findings {
        a.as_object_mut()
            .expect("assessment is an object")
            .insert("findings".to_owned(), f);
    }
    a
}

/// The §8.3 projection and the §8.4 projection §18's example derivation reads.
fn complete_review() -> Value {
    json!({
        "schemaVersion": 1, "sourceKind": "github-submitted-review",
        "stableId": REVIEW_ID,
        "user": {"id": REVIEW_ID, "login": "synthetic-external-reviewer", "type": "User"},
        "authorAssociation": "NONE", "state": "CHANGES_REQUESTED",
        "body": "still unreachable\n", "submittedAt": "2026-08-05T09:02:47Z",
        "commitId": "1f2e3d4c5b6a798807162534435261708f9e0d1c",
    })
}
fn complete_comment() -> Value {
    json!({
        "schemaVersion": 1, "sourceKind": "github-review-comment",
        "stableId": COMMENT_ID, "pullRequestReviewId": REVIEW_ID,
        "user": {"id": REVIEW_ID, "login": "synthetic-external-reviewer", "type": "User"},
        "authorAssociation": "NONE", "body": "here specifically\n",
        "commitId": "1f2e3d4c5b6a798807162534435261708f9e0d1c",
        "originalCommitId": "1f2e3d4c5b6a798807162534435261708f9e0d1c",
        "path": "crates/o7-closure-provenance/src/lib.rs",
        "createdAt": "2026-08-05T09:02:47Z", "updatedAt": "2026-08-05T09:02:47Z",
    })
}

/// The SAME two objects reduced: `/body` blocked, every other §5.3 field
/// retained — including the two the derivation actually reads.
fn reduced(locator_kind: &str, required: &[&str], stable_id: &str, blocked: &[&str]) -> Value {
    let mut retained = Map::new();
    for pointer in required {
        if blocked.contains(pointer) {
            continue;
        }
        let value = match *pointer {
            "/id" => json!(stable_id),
            "/pull_request_review_id" => json!(REVIEW_ID),
            _ => json!("value-as-the-projection-carries-it"),
        };
        retained.insert((*pointer).to_owned(), value);
    }
    let mut sorted = blocked.to_vec();
    sorted.sort_unstable();
    json!({
        "schemaVersion": 1,
        "sourceKind": "github-reduced-source-record",
        "locatorKind": locator_kind,
        "locator": {
            "repository": "PhysShell/007",
            "pullRequest": "9001",
            "stableId": stable_id,
        },
        "redactionPolicyVersion": "1",
        "outcome": "BLOCK_SECRET",
        "coverageComplete": true,
        "retainedFields": Value::Object(retained),
        "blockedFields": sorted,
    })
}

fn block_secret(required: &[&str], blocked: &[&str]) -> Value {
    let findings: Vec<Value> = blocked
        .iter()
        .map(|f| json!({"field": f, "findingId": "rule-aws-key"}))
        .collect();
    assessment(required, "BLOCK_SECRET", Some(Value::Array(findings)))
}

fn carries_finding(review: &str, comment: &str) -> DecisionBasis {
    DecisionBasis {
        expected_redaction_policy: "1".to_owned(),
        expected_detector: ExpectedDetector {
            id: "synthetic-detector".to_owned(),
            version: "1".to_owned(),
            config_digest:
                "sha256:1111111111111111111111111111111111111111111111111111111111111111".to_owned(),
        },
        observation_id: "review/external".to_owned(),
        inputs: Vec::new(),
        derived: vec![DerivedFact {
            derivation: "review-carries-finding".to_owned(),
            version: "1".to_owned(),
            value: json!(true),
            derived_from: vec![review.to_owned(), comment.to_owned()],
        }],
        expected_query: None,
        bindings: Vec::new(),
    }
}

/// D1 — BOUNDARY. Over complete projections the derivation reproduces today, and
/// must keep doing so. A fix that taught the derivation layer about reduced
/// records and broke this would have traded one representation for the other.
#[test]
fn d1_a_derivation_over_complete_projections_reproduces() {
    let mut store = Store::default();
    let r = store.retain_under(&complete_review(), &assessment(&REVIEW_REQ, "RETAIN", None));
    let c = store.retain_under(
        &complete_comment(),
        &assessment(&COMMENT_REQ, "RETAIN", None),
    );
    let outcome = relations(&carries_finding(&r, &c), &store);
    assert!(
        matches!(outcome, Admissible::Yes { .. }),
        "D1: got {outcome:?}"
    );
}

/// D2 — THE ESCAPE. The same logical inputs, retained through the gate, in
/// reduced records. `/id` and `/pull_request_review_id` both survived; §8 says
/// the derived fact remains usable and §7.5 says a reduced record resolves them
/// through `retainedFields`. Today both lookups read the canonical top level,
/// miss, and the fact is lost.
#[test]
fn d2_the_same_derivation_reproduces_over_reduced_records() {
    let mut store = Store::default();
    let r = store.retain_under(
        &reduced(
            "github-submitted-review",
            &REVIEW_REQ,
            REVIEW_ID,
            &["/body"],
        ),
        &block_secret(&REVIEW_REQ, &["/body"]),
    );
    let c = store.retain_under(
        &reduced(
            "github-review-comment",
            &COMMENT_REQ,
            COMMENT_ID,
            &["/body"],
        ),
        &block_secret(&COMMENT_REQ, &["/body"]),
    );
    let outcome = relations(&carries_finding(&r, &c), &store);
    assert!(
        matches!(outcome, Admissible::Yes { .. }),
        "D2: every input this derivation reads survived redaction, so §8 keeps the fact \
         usable. The evidence survived, the authority survived, validation succeeded, and \
         the accessor threw the distinction away: got {outcome:?}"
    );
}

/// D3 — BOUNDARY. One required input blocked, and the answer is `CannotCheck`.
///
/// This must stay refused after D2 is closed, or "read reduced records properly"
/// will have become "read them optimistically". A blocked input is a retention
/// loss, and a derivation resting on one has established nothing.
#[test]
fn d3_a_derivation_over_a_blocked_input_is_cannot_check() {
    let mut store = Store::default();
    let r = store.retain_under(
        &reduced(
            "github-submitted-review",
            &REVIEW_REQ,
            REVIEW_ID,
            &["/body", "/id"],
        ),
        &block_secret(&REVIEW_REQ, &["/body", "/id"]),
    );
    let c = store.retain_under(
        &reduced(
            "github-review-comment",
            &COMMENT_REQ,
            COMMENT_ID,
            &["/body"],
        ),
        &block_secret(&COMMENT_REQ, &["/body"]),
    );
    let outcome = relations(&carries_finding(&r, &c), &store);
    assert!(
        matches!(outcome, Admissible::CannotCheck { .. }),
        "D3: /id was blocked, so nothing establishes the relation: got {outcome:?}"
    );
}

/// D4 — BOUNDARY, and it is the trap D2's fix must not fall into.
///
/// §7.3: a locator value is NOT surviving source evidence and MUST NOT satisfy a
/// decision-basis pointer. The locator here carries `stableId` equal to the very
/// `/id` that was blocked, so a derivation layer taught to look "wherever the
/// value happens to be" would reproduce the fact from an alias and satisfy D2
/// while defeating the field gate. D2 and D4 close together or not at all.
#[test]
fn d4_a_locator_value_does_not_revive_a_blocked_derivation_input() {
    let mut store = Store::default();
    let record = reduced(
        "github-submitted-review",
        &REVIEW_REQ,
        REVIEW_ID,
        &["/body", "/id"],
    );
    assert_eq!(
        record.pointer("/locator/stableId"),
        Some(&json!(REVIEW_ID)),
        "fixture: the locator must still carry the id this witness blocks"
    );
    let r = store.retain_under(&record, &block_secret(&REVIEW_REQ, &["/body", "/id"]));
    let c = store.retain_under(
        &reduced(
            "github-review-comment",
            &COMMENT_REQ,
            COMMENT_ID,
            &["/body"],
        ),
        &block_secret(&COMMENT_REQ, &["/body"]),
    );
    let outcome = relations(&carries_finding(&r, &c), &store);
    assert!(
        matches!(outcome, Admissible::CannotCheck { .. }),
        "D4: the field gate must not be bypassed by an alias: got {outcome:?}"
    );
}
