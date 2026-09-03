//! RED-LOCATOR-IDENTITY — M3, reported against b9eaa7d, against frozen §7.3.
//!
//! §7.3 is titled "The locator is identity, not surviving evidence" and states
//! three normative rules. The implementation enforced the first two and read the
//! third as a shape check:
//!
//! ```text
//! a locator value is NOT surviving source evidence
//! and MUST NOT satisfy a decision-basis pointer
//!
//! locatorKind MUST equal the source kind that was gated
//!
//! locator MUST equal the acquisition locator of that source
//! ```
//!
//! §7.3 then says exactly what the gap is, in a sentence that reads as though it
//! were written for this finding:
//!
//! > The second and third exist because shape alone is not identity. A record
//! > whose locator has the right keys and the wrong values, or the right values
//! > under the wrong `locatorKind`, is a well-formed pointer at the wrong object
//! > — which is worse than a missing one, because it resolves.
//!
//! `check_reduced_locator` validated the locator against §7.3's SHAPE for the
//! declared kind and returned the contract-side kind name. Nothing compared a
//! single VALUE. So a reduced record naming another repository, another pull
//! request or another object was admitted, and a decision read a field out of
//! it: evidence attributed to a source it is not about.
//!
//! THIS IS L2'S SENTENCE AT THE OTHER GATED ARTIFACT. L2 established that an
//! absence claim's query snapshot must be an enumeration of the change the
//! decision is about, from a subject supplied outside the artifact. A reduced
//! record is the same question one layer down, and §7.3 had already answered it
//! — the answer was in the contract before either round.
//!
//! ```text
//! M3-A  another repository, right shape                            RED
//! M3-B  another object in this repository                          RED
//! M3-C  a derived fact's cited source, same defect                 RED
//! M3-D  the object the caller asked for still admits     BOUNDARY  admits
//! M3-E  a locator of the wrong SHAPE is still refused     BOUNDARY  §7.3
//! M3-F  a complete projection is unaffected              BOUNDARY  admits
//! M3-G  §7.3 still states the rule and the reason                FREEZE
//! ```
//!
//! M3-C IS NOT A DUPLICATE OF M3-A. A derived fact cites its own sources, and
//! redaction §8 keeps a fact usable when every field it reads survived the gate
//! — so a reduced record reaches the rule engine by a second route. A repair
//! written only where the basis resolves its inputs leaves that one open, which
//! is the shape J3-B and L2-F were written against.
//!
//! M3-F IS THE SCOPE STATEMENT. §7.3 governs the reduced record's locator. A
//! complete §8 projection has no `locator` member — its identity is its own
//! fields — so this round must not start demanding one, and the witness holds
//! that line rather than leaving it to a reader.

// Justification for the restriction-lint allowance, per AGENTS.md rule 4: the
// `expect` sites below are this file's own handling of JSON literals written in
// it, unreachable unless a specimen a few lines above is malformed.
// Extent (checked by N1): 2 `expect` sites.
#![allow(clippy::expect_used)]

use o7_closure_canonical::digest;
use o7_closure_provenance::{
    relations_checked, AcquisitionLocator, CitedSource, DecisionBasis, DecisionInput, DerivedFact,
    ExpectedDetector, RetainedEvidence, Unresolved,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;

const REDACTION: &str = include_str!("../../../docs/architecture/closure-redaction-policy-v1.md");

const OURS: &str = "PhysShell/007";
const CHECK_ID: &str = "9900000001";
const REVIEW_ID: &str = "9000000901";
const COMMENT_ID: &str = "9000000202";
const HEAD_SHA: &str = "1f2e3d4c5b6a798807162534435261708f9e0d1c";
const CONFIG: &str = "sha256:1111111111111111111111111111111111111111111111111111111111111111";

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
    fn retain(&mut self, record: &Value, assessed: &[&str], outcome: &str) -> String {
        let blocked = if assessed.contains(&"/status") {
            "/status"
        } else {
            "/body"
        };
        let mut assessment = json!({
            "schemaVersion": 1, "sourceKind": "closure-retention-assessment",
            "redactionPolicyVersion": "1",
            "detector": {"id": "synthetic-detector", "version": "1", "configDigest": CONFIG},
            "representation": "decoded-source-field-values",
            "assessedFields": assessed, "coverageComplete": true,
            "outcome": outcome, "observedAt": "2026-08-05T09:03:00Z",
        });
        if outcome == "BLOCK_SECRET" {
            assessment
                .as_object_mut()
                .expect("the literal above is an object")
                .insert(
                    "findings".to_owned(),
                    json!([{"field": blocked, "findingId": "rule-aws-key"}]),
                );
        }
        let ad = self.put(&assessment);
        let rd = self.put(record);
        let binding = json!({
            "schemaVersion": 1, "sourceKind": "closure-retention-binding",
            "recordDigest": rd, "assessmentDigest": ad,
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

/// A §7 reduced check record whose locator is supplied by the witness.
fn reduced_check(repository: &str, stable_id: &str) -> Value {
    json!({
        "schemaVersion": 1, "sourceKind": "github-reduced-source-record",
        "locatorKind": "github-actions-check",
        "locator": {"repository": repository, "stableId": stable_id},
        "redactionPolicyVersion": "1",
        "outcome": "BLOCK_SECRET", "coverageComplete": true,
        "retainedFields": {"/head_sha": HEAD_SHA, "/id": stable_id, "/name": "ai/final-review"},
        "blockedFields": ["/status"],
    })
}

const CHECK_ASSESSED: [&str; 4] = ["/head_sha", "/id", "/name", "/status"];

/// The check the decision is about.
fn ours() -> AcquisitionLocator {
    AcquisitionLocator::Check {
        repository: OURS.to_owned(),
        stable_id: CHECK_ID.to_owned(),
    }
}

fn basis(inputs: Vec<DecisionInput>, derived: Vec<DerivedFact>) -> DecisionBasis {
    DecisionBasis {
        expected_redaction_policy: "1".to_owned(),
        expected_detector: ExpectedDetector {
            id: "synthetic-detector".to_owned(),
            version: "1".to_owned(),
            config_digest: CONFIG.to_owned(),
        },
        observation_id: "check/ai-final-review".to_owned(),
        inputs,
        derived,
        expected_query: None,
        bindings: Vec::new(),
    }
}

fn reads(digest: &str, pointer: &str, locator: AcquisitionLocator) -> DecisionInput {
    DecisionInput {
        source_digest: digest.to_owned(),
        pointer: pointer.to_owned(),
        locator,
    }
}

/// Refused, AND refused because the locator names another object.
///
/// Pinned on the variant, per CR-N1 and per M4: `is_err()` alone would pass for
/// a malformed fixture, which is how three witnesses in this round were briefly
/// red for the wrong reason.
#[track_caller]
fn refuses(outcome: &Result<Vec<Value>, Vec<Unresolved>>, what: &str, named: &str) {
    let why: &[Unresolved] = match outcome {
        Ok(_) => &[],
        Err(why) => why,
    };
    assert!(
        why.iter()
            .any(|u| matches!(u, Unresolved::LocatorSubjectMismatch { .. })),
        "{what}: a reduced record whose locator names {named} was admitted, and the decision \
         read a field out of it. §7.3: `locator` MUST equal the acquisition locator of that \
         source — shape alone is not identity, and a record whose locator has the right keys \
         and the wrong values is a well-formed pointer at the wrong object, which is worse than \
         a missing one because it resolves: got {outcome:?}"
    );
}

/// M3-A — THE REPORTED CASE. Right keys, another repository entirely.
#[test]
fn m3a_a_locator_naming_another_repository_is_admitted() {
    let mut store = Store::default();
    let record = store.retain(
        &reduced_check("SomeoneElse/unrelated-repo", CHECK_ID),
        &CHECK_ASSESSED,
        "BLOCK_SECRET",
    );
    refuses(
        &relations_checked(
            &basis(vec![reads(&record, "/head_sha", ours())], Vec::new()),
            &store,
        ),
        "M3-A",
        "another repository",
    );
}

/// M3-B — the likelier half: this repository, another object.
///
/// A rule that compared only the repository would pass M3-A and leave this
/// open, which is one inclusion of §7.3's equality offered as the equality.
#[test]
fn m3b_a_locator_naming_another_object_here_is_admitted() {
    let mut store = Store::default();
    let record = store.retain(
        &reduced_check(OURS, "4242"),
        &CHECK_ASSESSED,
        "BLOCK_SECRET",
    );
    refuses(
        &relations_checked(
            &basis(vec![reads(&record, "/head_sha", ours())], Vec::new()),
            &store,
        ),
        "M3-B",
        "another check run in this repository",
    );
}

/// M3-C — the second route in. A derived fact cites its own sources, and
/// redaction §8 keeps a fact usable over reduced records, so a repair written
/// only where the basis resolves its inputs leaves this one open.
#[test]
fn m3c_a_derived_facts_cited_source_is_not_checked_either() {
    let mut store = Store::default();
    let review = store.retain(
        &json!({
            "schemaVersion": 1, "sourceKind": "github-reduced-source-record",
            "locatorKind": "github-submitted-review",
            "locator": {
                "repository": "SomeoneElse/unrelated-repo",
                "pullRequest": "4242",
                "stableId": REVIEW_ID,
            },
            "redactionPolicyVersion": "1",
            "outcome": "BLOCK_SECRET", "coverageComplete": true,
            "retainedFields": {
                "/author_association": "NONE", "/commit_id": HEAD_SHA, "/id": REVIEW_ID,
                "/state": "CHANGES_REQUESTED", "/submitted_at": "2026-08-05T09:02:47Z",
                "/user/id": REVIEW_ID, "/user/login": "synthetic-external-reviewer",
                "/user/type": "User",
            },
            "blockedFields": ["/body"],
        }),
        &REVIEW_REQ,
        "BLOCK_SECRET",
    );
    let comment = store.retain(
        &json!({
            "schemaVersion": 1, "sourceKind": "github-reduced-source-record",
            "locatorKind": "github-review-comment",
            "locator": {"repository": OURS, "pullRequest": "9001", "stableId": COMMENT_ID},
            "redactionPolicyVersion": "1",
            "outcome": "BLOCK_SECRET", "coverageComplete": true,
            "retainedFields": {
                "/author_association": "NONE", "/commit_id": HEAD_SHA,
                "/created_at": "2026-08-05T09:02:47Z", "/id": COMMENT_ID,
                "/original_commit_id": HEAD_SHA,
                "/path": "crates/o7-closure-provenance/src/lib.rs",
                "/pull_request_review_id": REVIEW_ID,
                "/updated_at": "2026-08-05T09:02:47Z", "/user/id": REVIEW_ID,
                "/user/login": "synthetic-external-reviewer", "/user/type": "User",
            },
            "blockedFields": ["/body"],
        }),
        &COMMENT_REQ,
        "BLOCK_SECRET",
    );
    let outcome = relations_checked(
        &basis(
            Vec::new(),
            vec![DerivedFact {
                derivation: "review-carries-finding".to_owned(),
                version: "1".to_owned(),
                value: json!(true),
                derived_from: vec![
                    CitedSource {
                        digest: review,
                        locator: AcquisitionLocator::InPullRequest {
                            repository: OURS.to_owned(),
                            pull_request: "9001".to_owned(),
                            stable_id: REVIEW_ID.to_owned(),
                        },
                    },
                    CitedSource {
                        digest: comment,
                        locator: AcquisitionLocator::InPullRequest {
                            repository: OURS.to_owned(),
                            pull_request: "9001".to_owned(),
                            stable_id: COMMENT_ID.to_owned(),
                        },
                    },
                ],
            }],
        ),
        &store,
    );
    refuses(
        &outcome,
        "M3-C",
        "another pull request, cited by a derived fact",
    );
}

/// M3-D — BOUNDARY. The object the caller asked for still admits.
#[test]
fn m3d_the_object_the_caller_asked_for_still_admits() {
    let mut store = Store::default();
    let record = store.retain(
        &reduced_check(OURS, CHECK_ID),
        &CHECK_ASSESSED,
        "BLOCK_SECRET",
    );
    let outcome = relations_checked(
        &basis(vec![reads(&record, "/head_sha", ours())], Vec::new()),
        &store,
    );
    assert!(
        outcome.is_ok(),
        "M3-D: refused the record whose locator names exactly the object the caller asked for. \
         An identity rule that admits nothing has replaced §7.3 with a prohibition: got \
         {outcome:?}"
    );
}

/// M3-E — BOUNDARY. §7.3's SHAPE rule still fires on its own: a check locator
/// carrying a `pullRequest` is malformed, and must stay malformed rather than
/// becoming a value mismatch.
#[test]
fn m3e_a_locator_of_the_wrong_shape_is_still_malformed() {
    let mut store = Store::default();
    let record = store.retain(
        &json!({
            "schemaVersion": 1, "sourceKind": "github-reduced-source-record",
            "locatorKind": "github-actions-check",
            "locator": {"repository": OURS, "pullRequest": "9001", "stableId": CHECK_ID},
            "redactionPolicyVersion": "1",
            "outcome": "BLOCK_SECRET", "coverageComplete": true,
            "retainedFields": {"/head_sha": HEAD_SHA, "/id": CHECK_ID, "/name": "ai/final-review"},
            "blockedFields": ["/status"],
        }),
        &CHECK_ASSESSED,
        "BLOCK_SECRET",
    );
    let outcome = relations_checked(
        &basis(vec![reads(&record, "/head_sha", ours())], Vec::new()),
        &store,
    );
    let why: &[Unresolved] = match &outcome {
        Ok(_) => &[],
        Err(why) => why,
    };
    assert!(
        why.iter()
            .any(|u| matches!(u, Unresolved::MalformedArtifact { .. })),
        "M3-E: a check locator carrying a pullRequest §7.3 does not give that kind must stay a \
         MALFORMED artifact. §17.2 puts closed-form validation before every relation check, and \
         a new identity rule that swallowed the shape rule would report a wrong-object error \
         for an object that is not well formed: got {outcome:?}"
    );
}

/// M3-F — BOUNDARY, and the scope statement. §7.3 governs the reduced record's
/// locator; a complete §8 projection has no `locator` member at all, and its
/// identity is its own fields. This round must not start demanding one.
#[test]
fn m3f_a_complete_projection_is_unaffected() {
    let mut store = Store::default();
    let projection = store.retain(
        &json!({
            "schemaVersion": 1, "sourceKind": "github-actions-check",
            "stableId": CHECK_ID, "name": "ai/final-review",
            "headSha": HEAD_SHA, "status": "completed", "conclusion": "success",
        }),
        &["/conclusion", "/head_sha", "/id", "/name", "/status"],
        "RETAIN",
    );
    let outcome = relations_checked(
        &basis(vec![reads(&projection, "/headSha", ours())], Vec::new()),
        &store,
    );
    assert!(
        outcome.is_ok(),
        "M3-F: refused a complete §8 projection. §7.3 shapes the REDUCED record's locator and a \
         projection carries none — its identity is its own fields, and demanding a locator of \
         it would invent a member §8 does not declare: got {outcome:?}"
    );
}

/// M3-G — FREEZE. §7.3 still states the rule and still states why.
#[test]
fn m3g_the_frozen_rule_is_still_the_frozen_rule() {
    assert!(
        REDACTION.contains("locator MUST equal the acquisition locator of that source"),
        "§7.3 no longer requires the locator to equal the acquisition locator. Every witness \
         above goes green if that rule is dropped, with no code change at all"
    );
    assert!(
        REDACTION.contains("shape alone is not identity"),
        "§7.3 still states the rule but no longer says why. That sentence is the finding: a \
         record whose locator has the right keys and the wrong values resolves, which is what \
         makes it worse than a missing one"
    );
}
