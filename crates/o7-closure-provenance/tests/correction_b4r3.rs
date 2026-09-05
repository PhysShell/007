//! RED-B4R.3 — three relations a qualified artifact still does not establish.
//!
//! Preregistration only. No production change is in this commit.
//!
//! GREEN-B4R.2 closed the candidate-authority escape and its architecture is not
//! withdrawn. What the external review at `3a9aa49` found is that three separate
//! relations are still not checked, and each of them is the same sentence the
//! whole effort keeps rediscovering: *an artifact was checked for what it IS and
//! never for what it is ABOUT.*
//!
//! ```text
//! F1  the matcher implementation binding is CONSULTED and its
//!     CannotCheck outcome is IGNORED
//! F2  the query snapshot is not bound to the decision's OBSERVATION
//! F3  the record and its authorising assessment may name different
//!     redaction POLICY VERSIONS
//! ```
//!
//! F1 IS THE UNCOMFORTABLE ONE, and it is worth being precise about why.
//! `qualify_query` calls `verify_matched` and reads only `verdict.reproduced`.
//! `verdict.replay.implementation` is never inspected. A `schemaVersion` 1
//! snapshot carries no `matcher.implementationDigest`, so
//! `check_recorded_implementation` returns `ImplementationCheck::CannotCheck`
//! and replay "succeeds" — the exact reading that enum's own `must_use` message
//! forbids: *an unread implementation check is an unchecked axis reported as a
//! passed one*.
//!
//! RED-B4R.1 hardened the FIXTURES to version 2 so they reach `Bound`, and
//! `every_query_fixture_carries_a_bound_implementation` asserts that the
//! FIXTURES are bound. Nothing ever asserted that the PRODUCTION CODE requires
//! it. That guard verified the wrong side of the boundary, and in doing so it
//! removed the only route by which a test could have caught this. F1-A is the
//! witness that should have existed then.
//!
//! WHAT F1 DOES NOT SAY. Version 1 is a REGISTERED schema version and stays
//! structurally valid; §13 defines it and specimen fixtures rely on it. The
//! claim is narrower and is exactly the contract's own:
//!
//! ```text
//! a conforming artifact
//!   but NOT sufficient evidence for a replay-dependent role
//! ```
//!
//! ONE WITNESS PER CONSTRUCTOR, NOT ONE PER CONSUMER. `qualify_query` is the
//! only constructor of `QualifiedQuery`, and `correction_b4r.rs`'s R7 already
//! pins that the absence path and the scan path consume the same qualification.
//! Adding a scan-path twin for F1 would re-express one property per consumer,
//! which is how an architecture property quietly turns back into a procedure.
//!
//! ```text
//! F1-A  V1, COMPLETE, matching observation, replay reproduces   RED
//! F1-B  the same at V2 with a correct implementationDigest      BOUNDARY
//! F2-A  V2 Bound COMPLETE, wrong requiredObservationId          RED
//! F2-B  the same with the matching observation                  BOUNDARY
//! F3-A  reduced record policy A, authorising assessment policy B RED
//! F3-B  the same policy on both                                 BOUNDARY
//! ```
//!
//! Every RED here is paired with the boundary that keeps the capability, for the
//! reason RED-B4R.2's Q3/Q4 existed: a fix that refuses V1 outright, or refuses
//! every query, or refuses every reduced record, would close the finding by
//! removing the feature.

// Justification for the restriction-lint allowance, per AGENTS.md rule 4: the
// `expect` sites below are this file's own handling of JSON literals
// written in it, unreachable unless a specimen a few lines above is malformed.
// Extent (checked by N1): 1 `expect` site.
#![allow(clippy::expect_used)]

use o7_closure_provenance::{
    admissibility, relations_checked, AcquisitionLocator, Admissible, DecisionBasis, DecisionInput,
    DecisionProfile, ExpectedDetector, ExpectedQuery, QueryBinding, RetainedEvidence, Unresolved,
};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

mod common;

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
const OBSERVATION: &str = "review/external";
const ANOTHER_OBSERVATION: &str = "check/an-entirely-different-observation";
const FOREIGN_POLICY: &str = "99-a-policy-nobody-ran";

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
        let d = common::digest_of(o);
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

fn bound_implementation_digest() -> String {
    common::bound_matcher_digest(MATCHER_ID, MATCHER_VERSION)
}

/// A §13 query snapshot. `schema_version` picks whether the matcher block
/// carries `implementationDigest`: §13.1 adds it at version 2, and the closed
/// key sets make a version-1 snapshot carrying it, and a version-2 snapshot
/// missing it, both malformed.
///
/// Everything else is as strong as §13 allows — COMPLETE, an empty candidate set
/// that replay reproduces exactly — so the only thing each witness varies is the
/// relation it names.
fn snapshot(schema_version: i64, observation: &str) -> Value {
    let mut matcher = json!({
        "id": MATCHER_ID,
        "version": MATCHER_VERSION,
        "parameters": {"expectedAuthorLogin": "synthetic-external-reviewer"},
    });
    if schema_version == 2 {
        matcher
            .as_object_mut()
            .expect("the matcher block is an object")
            .insert(
                "implementationDigest".to_owned(),
                json!(bound_implementation_digest()),
            );
    }
    json!({
        "schemaVersion": schema_version,
        "sourceKind": "github-query-snapshot",
        "surface": "pull-request-submitted-reviews",
        "requiredObservationId": observation,
        "binding": {"repository": "PhysShell/007", "pullRequest": "9001"},
        "pagination": {
            "perPage": 100,
            "pagesRequested": ["1"],
            "pagesObtained": ["1"],
            "nextPagePresent": false,
        },
        "enumeration": "COMPLETE",
        "matcher": matcher,
        "allReturnedSnapshotDigests": [],
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
        observation_id: OBSERVATION.to_owned(),
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

/// The decision was refused, AND refused for the relation this witness names.
///
/// WHY THE PREDICATE, AND WHY IT ARRIVED LATE. The first version of this helper
/// asserted only `matches!(outcome, CannotCheck { .. })`. That is the defect two
/// commits of this very round exist to prevent — `ed9b77d` and `6fd62d7` both
/// corrected fixtures that would have gone green for a reason other than the one
/// they name — and it reappeared in the file preregistering the next three.
/// External review caught it. Demonstrated before the repair: deleting the
/// REQUIRED `surface` member from F1-A's snapshot makes it refuse as a MALFORMED
/// ARTIFACT, and F1-A still passed.
///
/// Sibling witnesses in this crate already pinned their variant —
/// `witnesses.rs` asserts `RecordDigestMismatch` and `NoSuchRecord`,
/// `correction_b2.rs` asserts `BindingSubjectMismatch` — so the inconsistency
/// was internal to our own suite rather than a gap nobody had thought about.
///
/// WHAT THE PREDICATE PINS ON. A variant plus an identity, never the whole
/// message. Where the fixture chose an offending VALUE — a foreign observation,
/// a foreign policy version — the predicate requires the refusal to name that
/// value, which is stable against any rewording because the fixture owns it.
/// Where the defect is an ABSENCE there is no such value, and the predicate
/// names the axis instead; that one is prose-coupled on purpose, and a future
/// rewording must update it deliberately rather than silently widen it.
///
/// NEGATIVE CONTROLS. Each was applied to this file, run, and reverted; each
/// must turn the named witness RED, and did.
///
/// ```text
/// NC-1  delete REQUIRED /surface from the snapshot
///       -> F1-A, F2-A refuse as MalformedArtifact         both RED
/// NC-2  delete REQUIRED /representation from the assessment
///       -> F3-A refuses as MalformedArtifact              F3-A RED
/// NC-3  downgrade F2-A's snapshot to schemaVersion 1
///       -> F2-A refuses as QueryDoesNotSupportRole on the
///          IMPLEMENTATION axis                            F2-A RED
/// ```
///
/// NC-3 is the one that matters. NC-1 and NC-2 only show the helper separates a
/// malformed artifact from a relation refusal. NC-3 keeps the VARIANT identical
/// and changes only which relation is open — and the witness still fails. That
/// is the difference between pinning a variant and pinning a variant plus the
/// identity the witness is actually about.
#[track_caller]
fn refuses_because(
    outcome: &Admissible,
    what: &str,
    relation: &str,
    is_the_relation: impl Fn(&Unresolved) -> bool,
) {
    // Two asserts rather than a let-else, because `clippy::panic` is denied
    // tree-wide and this file will not spend an allowance on a test helper.
    let why: &[Unresolved] = match outcome {
        Admissible::CannotCheck { why } => why,
        Admissible::Yes { .. } => &[],
    };
    assert!(
        matches!(outcome, Admissible::CannotCheck { .. }),
        "{what}: admitted. The artifact is conforming and that was never the question"
    );
    assert!(
        why.iter().any(is_the_relation),
        "{what}: refused, but not for {relation}. A witness that accepts ANY refusal reports \
         green over a property it never established — and would keep reporting it while the \
         relation stayed open behind an unrelated check: got {why:?}"
    );
}

#[track_caller]
fn admits(outcome: &Admissible, what: &str) {
    assert!(
        matches!(outcome, Admissible::Yes { .. }),
        "{what}: refused. A fix that closes the escape by removing the capability \
         has closed the wrong thing: got {outcome:?}"
    );
}

// ---- F1. The implementation axis is consulted and its answer is discarded.

/// F1-A — a version-1 snapshot has no `implementationDigest`, replay reaches
/// `ImplementationCheck::CannotCheck`, and qualification never looks at it.
///
/// Everything else about this query is correct: COMPLETE, the observation
/// matches, the candidate set is empty and the replay reproduces the claim. The
/// ONLY thing unestablished is that the code which produced the recorded
/// selection is the code the artifact names — and §13 says replaying V1 leaves
/// that axis `CANNOT_CHECK`, never a pass.
#[test]
fn f1a_an_unbound_matcher_implementation_cannot_qualify_a_replay() {
    let mut store = Store::default();
    let s = store.put(&snapshot(1, OBSERVATION));
    refuses_because(
        &relations(&absence_basis(&s), &store),
        "F1-A",
        "the matcher implementation axis",
        |u| {
            matches!(u, Unresolved::QueryDoesNotSupportRole { why, .. }
                if why.contains("implementation axis"))
        },
    );
}

/// F1-B — BOUNDARY. The same query at version 2 with a correct
/// `implementationDigest` still qualifies.
///
/// Version 1 stays a REGISTERED schema version and a structurally valid
/// artifact; what F1-A denies it is a replay-dependent ROLE, not existence. A
/// fix that refused version 1 at the door would be a schema change nobody
/// authorised.
#[test]
fn f1b_a_bound_matcher_implementation_still_qualifies() {
    let mut store = Store::default();
    let s = store.put(&snapshot(2, OBSERVATION));
    admits(&relations(&absence_basis(&s), &store), "F1-B");
}

// ---- F2. The query is not bound to the decision's observation.

/// F2-A — the snapshot answers a different observation, and nothing compares it.
///
/// `DecisionBasis::observation_id` is carried and never read;
/// `requiredObservationId` appears nowhere in the crate's source. So a COMPLETE,
/// bound, correctly replayed snapshot of ANOTHER observation proves
/// `NotProduced` for this one — the subject-relation escape B3 closed for head
/// reads, at a surface it was never carried to.
#[test]
fn f2a_a_snapshot_for_another_observation_does_not_prove_this_absence() {
    let mut store = Store::default();
    let s = store.put(&snapshot(2, ANOTHER_OBSERVATION));
    refuses_because(
        &relations(&absence_basis(&s), &store),
        "F2-A",
        "the observation the decision is about",
        |u| {
            matches!(u, Unresolved::QueryDoesNotSupportRole { why, .. }
                if why.contains(ANOTHER_OBSERVATION))
        },
    );
}

/// F2-B — BOUNDARY. The same query for the observation the basis names is
/// admissible, and must stay so.
#[test]
fn f2b_a_snapshot_for_this_observation_still_proves_the_absence() {
    let mut store = Store::default();
    let s = store.put(&snapshot(2, OBSERVATION));
    // DECISION-LEVEL, so it asks the decision entry point. This witness's
    // claimed property is that the absence IS established — not that one
    // relation held — and after §17 gained its `absence` row that claim has a
    // profile to be judged under. Its sibling F2-A stays on `relations`: a
    // refusal isolating one relation is a different question.
    admits(
        &admissibility(DecisionProfile::Absence, &absence_basis(&s), &store),
        "F2-B",
    );
}

// ---- F3. The authorising assessment may be from another policy.

fn reduced(policy_version: &str) -> Value {
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
        "redactionPolicyVersion": policy_version,
        "outcome": "BLOCK_SECRET",
        "coverageComplete": true,
        "retainedFields": Value::Object(retained),
        "blockedFields": ["/body"],
    })
}

fn block_body(policy_version: &str) -> Value {
    json!({
        "schemaVersion": 1,
        "sourceKind": "closure-retention-assessment",
        "redactionPolicyVersion": policy_version,
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

fn reads(record: &str, pointer: &str) -> DecisionBasis {
    DecisionBasis {
        expected_redaction_policy: "1".to_owned(),
        expected_detector: ExpectedDetector {
            id: "synthetic-detector".to_owned(),
            version: "1".to_owned(),
            config_digest:
                "sha256:1111111111111111111111111111111111111111111111111111111111111111".to_owned(),
        },
        observation_id: OBSERVATION.to_owned(),
        inputs: vec![DecisionInput {
            source_digest: record.to_owned(),
            pointer: pointer.to_owned(),
            locator: AcquisitionLocator::InPullRequest {
                repository: "PhysShell/007".to_owned(),
                pull_request: "9001".to_owned(),
                stable_id: "9000000901".to_owned(),
            },
        }],
        derived: Vec::new(),
        expected_query: None,
        bindings: Vec::new(),
    }
}

/// F3-A — §9.5: every retained redaction object carrying
/// `redactionPolicyVersion` "carries the same value as the assessment that
/// authorised it". Nothing compares them.
///
/// An assessment made under one policy therefore authorises a record claiming
/// another. Every other relation check passes: the binding is retained and names
/// this record, the outcomes agree, the partition computes. The authorisation
/// trail says a gate ran, and does not say WHICH gate.
///
/// THE RECORD carries FOREIGN_POLICY and the assessment carries the version this
/// evaluation expects, not the other way round. Written the other way this
/// witness was green for the wrong reason for several rounds, and external
/// review found it: `check_authorises` compares the assessment against the
/// CALLER'S expected version before it compares the record against the
/// assessment, so a fixture deviating on both refuses at the first door and
/// never reaches this one. The `why` list held a single element and it was the
/// other rule's. Only the record may deviate here, and the assertion below names
/// this relation's own sentence rather than any refusal mentioning the constant.
#[test]
fn f3a_an_assessment_from_another_policy_does_not_authorise_this_record() {
    let mut store = Store::default();
    let d = store.retain_under(&reduced(FOREIGN_POLICY), &block_body("1"));
    refuses_because(
        &relations(&reads(&d, "/id"), &store),
        "F3-A",
        "the redaction policy version of the authorising assessment",
        |u| {
            matches!(u, Unresolved::AssessmentDoesNotAuthorise { why, .. }
                if why.contains("the record's /redactionPolicyVersion"))
        },
    );
}

/// F3-B — BOUNDARY. The same pair under one policy stays admissible.
#[test]
fn f3b_an_assessment_from_this_policy_still_authorises() {
    let mut store = Store::default();
    let d = store.retain_under(&reduced("1"), &block_body("1"));
    admits(&relations(&reads(&d, "/id"), &store), "F3-B");
}
