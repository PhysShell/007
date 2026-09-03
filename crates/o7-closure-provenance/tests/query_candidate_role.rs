//! What a §13 query candidate may be, and what refuses it.
//!
//! GREEN-B4R.2 routes every declared candidate through the door under
//! `ConsumedAs::QueryCandidate` rather than `ConsumedAs::GatedSource`, and the
//! difference is one kind:
//!
//! ```text
//! GatedSource     CompleteProjection(_)  |  ReducedSourceRecord
//! QueryCandidate  CompleteProjection(_)
//! ```
//!
//! Both are gated, so §9.2 applies to both and the gate classification cannot
//! tell them apart — being gated is what §9.2 turns on, not what §13 does. §13's
//! matcher is defined over canonical §8 source snapshots; a reduced source
//! record is a legitimate thing to read a decision pointer out of and not a
//! thing a matcher can score.
//!
//! WHY THIS FILE EXISTS. Mutation testing during GREEN-B4R.2: widening
//! `QueryCandidate` to accept `ReducedSourceRecord` changed no verdict anywhere
//! in the crate. The role's accepting half was witnessed by `correction_b4r2`'s
//! Q3 and Q4; its REFUSING half was witnessed by nothing, and a check with no
//! reachable failure is not a check.
//!
//! WHAT THE ASSERTIONS BELOW CAN AND CANNOT SEE. Qualification failures are
//! reported as text — `QueryDoesNotSupportRole { why: String }`, inherited from
//! `ScanVerdict::CannotCheck { why }` — so these witnesses assert on the
//! refusal's WORDING, which is weaker than matching a variant. It is what
//! distinguishes the claim being made here from a coincidence: a widened role
//! still refuses a reduced record, one step later and for a different reason,
//! so "was it refused" cannot tell the two apart and "which refusal" can.

// Justification for the restriction-lint allowance, per AGENTS.md rule 4: the
// `expect` and `panic` sites below are this file's own handling of JSON literals
// written in it, unreachable unless a specimen a few lines above is malformed.
#![allow(clippy::expect_used, clippy::panic)]

use o7_closure_canonical::digest;
use o7_closure_matcher::{resolve as resolve_matcher, verify_implementation};
use o7_closure_provenance::{
    relations_checked, AcquisitionLocator, Admissible, DecisionBasis, ExpectedDetector,
    ExpectedQuery, QueryBinding, RetainedEvidence, Unresolved,
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

const MATCHER_ID: &str = "review-by-expected-author-login";
const MATCHER_VERSION: &str = "1";

/// The role name `ConsumedAs::QueryCandidate` reports itself under. A refusal
/// carrying this is a refusal about the ROLE; the later ones are not.
const CANDIDATE_ROLE: &str = "a query snapshot's replay candidate";

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

/// A §7 reduced source record with `/body` blocked and everything else retained.
fn reduced_review() -> Value {
    let mut retained = Map::new();
    for pointer in REVIEW_REQ {
        if pointer == "/body" {
            continue;
        }
        let value = match pointer {
            "/id" => json!("9000000901"),
            _ => json!("value-as-the-projection-carries-it"),
        };
        retained.insert(pointer.to_owned(), value);
    }
    json!({
        "schemaVersion": 1,
        "sourceKind": "github-reduced-source-record",
        "locatorKind": "github-submitted-review",
        "locator": {
            "repository": "PhysShell/007",
            "pullRequest": "9001",
            "stableId": "9000000901",
        },
        "redactionPolicyVersion": "1",
        "outcome": "BLOCK_SECRET",
        "coverageComplete": true,
        "retainedFields": Value::Object(retained),
        "blockedFields": ["/body"],
    })
}

/// The §9 assessment that authorises exactly that partition.
fn block_body() -> Value {
    json!({
        "schemaVersion": 1,
        "sourceKind": "closure-retention-assessment",
        "redactionPolicyVersion": "1",
        "detector": {
            "id": "synthetic-detector",
            "version": "1",
            "configDigest": "sha256:1111111111111111111111111111111111111111111111111111111111111111",
        },
        "representation": "decoded-source-field-values",
        "assessedFields": REVIEW_REQ,
        "coverageComplete": true,
        "outcome": "BLOCK_SECRET",
        "findings": [{"field": "/body", "findingId": "rule-aws-key"}],
        "observedAt": "2026-08-05T09:03:00Z",
    })
}

fn bound_implementation_digest() -> String {
    let entry = resolve_matcher(MATCHER_ID, MATCHER_VERSION).expect("the matcher is registered");
    verify_implementation(entry)
        .expect("the registry is bound to its own implementation")
        .as_str()
        .to_owned()
}

fn snapshot(all: &[&str]) -> Value {
    json!({
        "schemaVersion": 2,
        "sourceKind": "github-query-snapshot",
        "surface": "pull-request-submitted-reviews",
        "requiredObservationId": "review/external",
        "binding": {"repository": "PhysShell/007", "pullRequest": "9001"},
        "pagination": {
            "perPage": 100,
            "pagesRequested": ["1"],
            "pagesObtained": ["1"],
            "nextPagePresent": false,
        },
        "enumeration": "COMPLETE",
        "matcher": {
            "id": MATCHER_ID,
            "version": MATCHER_VERSION,
            "implementationDigest": bound_implementation_digest(),
            "parameters": {"expectedAuthorLogin": "synthetic-external-reviewer"},
        },
        "allReturnedSnapshotDigests": all,
        "matchedSnapshotDigests": [],
    })
}

fn absence_basis(expected: &str) -> DecisionBasis {
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
        derived: Vec::new(),
        expected_query: Some(ExpectedQuery {
            digest: expected.to_owned(),
            subject: QueryBinding {
                repository: "PhysShell/007".to_owned(),
                pull_request: "9001".to_owned(),
            },
        }),
        bindings: Vec::new(),
    }
}

#[track_caller]
fn refusal_naming_the_role(outcome: &Admissible, what: &str) {
    let Admissible::CannotCheck { why } = outcome else {
        panic!("{what}: admitted, and §13's matcher is not defined over this kind");
    };
    let named = why.iter().any(|u| match u {
        Unresolved::QueryDoesNotSupportRole { why, .. } => why.contains(CANDIDATE_ROLE),
        _ => false,
    });
    assert!(
        named,
        "{what}: refused, but not as the wrong kind for the candidate role. A role that \
         admits this kind and is stopped one step later by the matcher refuses just as \
         visibly and means something else: {why:?}"
    );
}

/// A reduced source record is a gated source and is NOT a query candidate.
///
/// Fully authorised on purpose — its own §9.2 chain is complete, so the only
/// thing wrong with it is the role. Without that the witness would go green on
/// candidate authority, which is the fixture defect this correction round has
/// now caught twice.
#[test]
fn a_reduced_source_record_is_not_a_query_candidate() {
    let mut store = Store::default();
    let reduced = store.retain_under(&reduced_review(), &block_body());
    let s = store.put(&snapshot(&[&reduced]));

    // The candidate really does carry authority: §13, not §9.2, is what refuses it.
    assert!(
        matches!(
            relations(
                &DecisionBasis {
                    expected_redaction_policy: "1".to_owned(),
        expected_detector: ExpectedDetector {
            id: "synthetic-detector".to_owned(),
            version: "1".to_owned(),
            config_digest:
                "sha256:1111111111111111111111111111111111111111111111111111111111111111"
                    .to_owned(),
        },
                    observation_id: "review/external".to_owned(),
                    inputs: vec![o7_closure_provenance::DecisionInput {
                        source_digest: reduced.clone(),
                        pointer: "/id".to_owned(),
                        locator: AcquisitionLocator::InPullRequest {
                repository: "PhysShell/007".to_owned(),
                pull_request: "9001".to_owned(),
                stable_id: "9000000901".to_owned(),
            },
                    }],
                    derived: Vec::new(),
                    expected_query: None,
                    bindings: Vec::new(),
                },
                &store
            ),
            Admissible::Yes { .. }
        ),
        "fixture: the reduced record must be admissible as a gated source, or this witness \
         is about authority rather than about the role"
    );

    refusal_naming_the_role(
        &relations(&absence_basis(&s), &store),
        "a reduced source record cited as a candidate",
    );
}

/// And a query snapshot cited as another query snapshot's candidate.
///
/// The self-referential shape, one step removed: an enumeration artifact is not
/// one of the things it enumerates. Ungated, so it reaches the role check with
/// nothing else to stop it.
#[test]
fn a_query_snapshot_is_not_a_query_candidate() {
    let mut store = Store::default();
    let inner = store.put(&snapshot(&[]));
    let outer = store.put(&snapshot(&[&inner]));

    refusal_naming_the_role(
        &relations(&absence_basis(&outer), &store),
        "a query snapshot cited as another's candidate",
    );
}
