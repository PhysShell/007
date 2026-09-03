//! NORM/RED-ABSENCE-BASIS — §17's fifth row.
//!
//! THE GAP, AND IT BECAME VISIBLE ONLY BECAUSE G2 WAS FIXED. Before
//! GREEN-BASIS-COMPLETENESS nothing checked minimum basis at all, so an absent
//! requirement and a satisfied one looked identical. With profiles in place the
//! hole has a shape:
//!
//! ```text
//! DecisionProfile::Check   ->  produced check
//! DecisionProfile::Review  ->  produced review
//! absence                  ->  ???
//! ```
//!
//! `Review` IS NOT A SUBSTITUTE, IT IS THE OPPOSITE. An authoritative absence is
//! the one decision whose subject is that no object was found. Evaluating it
//! under the review profile would require `observed commit_id` and a derived
//! `carries_finding` — evidence OF THE VERY OBJECT the decision says is not
//! there. A basis that could satisfy it would be a basis that refutes it.
//!
//! And `relations_checked` is not an answer either: it returns relation-level
//! obligations, never a decision verdict, which is exactly the distinction
//! GREEN-BASIS-COMPLETENESS drew into the type.
//!
//! THE ROW IS NORMATIVE FIRST. §17 now reads:
//!
//! ```text
//! absence       expected query snapshot digest
//! ```
//!
//! added to the contract in this same commit, because a library inventing its
//! own fifth row would be implementation → contract, the direction G3 already
//! refused for `observedAt`.
//!
//! WHAT THE ROW DELIBERATELY DOES NOT SAY. It does not restate that the snapshot
//! must be COMPLETE, that its matcher must be bound to its implementation, that
//! replay must reproduce the recorded selection, that `requiredObservationId`
//! must equal the basis's observation, or that the matched set must be empty.
//! §13 and §14 impose all five and this crate already enforces all five. A
//! minimum-basis row repeating them would be a second copy of a frozen rule.
//!
//! The division is exact, and A3/A4 exist to hold it:
//!
//! ```text
//! §13/§14   what the query snapshot must BE
//! §17 row   what the BASIS must PROVIDE before those questions have a subject
//! ```
//!
//! A basis naming no snapshot does not fail the §13 checks. It never reaches
//! them — which is how a decision with no evidence reads as a decision with no
//! problems.
//!
//! §17 IS A MINIMUM, NOT AN EXACT SCHEMA. A2 carries an extra input the profile
//! never asked for and must still be admissible. A profile that forbade what it
//! did not require would have quietly become a whitelist.
//!
//! ```text
//! A1  Absence, no expected_query_digest                              RED
//! A2  Absence, COMPLETE + bound + replayed + empty + matching
//!     observation, plus an unrequired extra input          BOUNDARY  Yes
//! A3  Absence, valid query whose matched set is NOT empty  BOUNDARY  §14
//! A4  Absence, valid query for another observation         BOUNDARY  §13
//! ```

// Justification for the restriction-lint allowance, per AGENTS.md rule 4: the
// `expect` sites below are this file's own handling of JSON literals written in
// it, unreachable unless a specimen a few lines above is malformed.
#![allow(clippy::expect_used)]

use o7_closure_canonical::digest;
use o7_closure_matcher::{resolve as resolve_matcher, verify_implementation};
use o7_closure_provenance::{
    admissibility, AcquisitionLocator, Admissible, DecisionBasis, DecisionInput, DecisionProfile,
    ExpectedDetector, ExpectedQuery, QueryBinding, RetainedEvidence, Unresolved,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;

const MATCHER_ID: &str = "review-by-expected-author-login";
const MATCHER_VERSION: &str = "1";
const OBSERVATION: &str = "review/external";
const ANOTHER_OBSERVATION: &str = "check/an-entirely-different-observation";
const REVIEW_ID: &str = "9000000901";

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

fn bound_implementation_digest() -> String {
    let entry = resolve_matcher(MATCHER_ID, MATCHER_VERSION).expect("the matcher is registered");
    verify_implementation(entry)
        .expect("the registry is bound to its own implementation")
        .as_str()
        .to_owned()
}

/// A §13 query snapshot at schemaVersion 2 — COMPLETE, matcher bound to its
/// implementation, and a candidate set replay reproduces exactly.
fn snapshot(observation: &str, all: &[String], matched: &[String]) -> Value {
    json!({
        "schemaVersion": 2,
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
        "matcher": {
            "id": MATCHER_ID,
            "version": MATCHER_VERSION,
            "parameters": {"expectedAuthorLogin": "synthetic-external-reviewer"},
            "implementationDigest": bound_implementation_digest(),
        },
        "allReturnedSnapshotDigests": all,
        "matchedSnapshotDigests": matched,
    })
}

fn retain_all(assessed: &[&str]) -> Value {
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
        "assessedFields": assessed,
        "coverageComplete": true,
        "outcome": "RETAIN",
        "observedAt": "2026-08-05T09:03:00Z",
    })
}

/// A §8.2 submitted review by the login the matcher selects on, so replay scores
/// it MATCHED and the absence claim is contradicted by its own evidence.
fn matching_review() -> Value {
    json!({
        "schemaVersion": 1, "sourceKind": "github-submitted-review",
        "stableId": REVIEW_ID,
        "user": {"id": REVIEW_ID, "login": "synthetic-external-reviewer", "type": "User"},
        "authorAssociation": "NONE", "state": "CHANGES_REQUESTED",
        "body": "there is in fact a review here\n",
        "submittedAt": "2026-08-05T09:02:47Z",
        "commitId": "1f2e3d4c5b6a798807162534435261708f9e0d1c",
    })
}

fn absence_basis(expected: Option<&str>, inputs: Vec<DecisionInput>) -> DecisionBasis {
    DecisionBasis {
        expected_redaction_policy: "1".to_owned(),
        expected_detector: ExpectedDetector {
            id: "synthetic-detector".to_owned(),
            version: "1".to_owned(),
            config_digest:
                "sha256:1111111111111111111111111111111111111111111111111111111111111111".to_owned(),
        },
        observation_id: OBSERVATION.to_owned(),
        inputs,
        derived: Vec::new(),
        expected_query: expected.map(|digest| ExpectedQuery {
            digest: digest.to_owned(),
            subject: QueryBinding {
                repository: "PhysShell/007".to_owned(),
                pull_request: "9001".to_owned(),
            },
        }),
        bindings: Vec::new(),
    }
}

#[track_caller]
fn refuses_as_incomplete(outcome: &Admissible, what: &str, missing: &str) {
    let why: &[Unresolved] = match outcome {
        Admissible::CannotCheck { why } => why,
        Admissible::Yes { .. } => &[],
    };
    assert!(
        matches!(outcome, Admissible::CannotCheck { .. }),
        "{what}: admitted. §17's minimum basis for an absence decision requires {missing}, and \
         the basis names none. Nothing failed a §13 check here because nothing was presented \
         to one — the claim never reached the machinery that would have judged it"
    );
    assert!(
        why.iter().any(|u| matches!(
            u,
            Unresolved::BasisIncompleteForProfile { profile: "absence", missing: m } if *m == missing
        )),
        "{what}: refused, but not for an absence basis missing {missing}: got {why:?}"
    );
}

/// A1 — an absence decision that names no query snapshot at all.
///
/// The claim is that no matching review exists. The evidence is nothing. Today
/// this is `Admissible::Yes`, which is the oldest failure in this project
/// wearing the newest costume: the §13/§14 checks that would have refused it are
/// all present, correct, and never reached.
#[test]
fn a1_an_absence_basis_naming_no_query_snapshot_is_incomplete() {
    let store = Store::default();
    refuses_as_incomplete(
        &admissibility(
            DecisionProfile::Absence,
            &absence_basis(None, Vec::new()),
            &store,
        ),
        "A1",
        "expected query snapshot digest",
    );
}

/// A2 — BOUNDARY. Everything §17 asks an absence decision for, and one thing it
/// does not.
///
/// The extra input is deliberate: §17 states a MINIMUM basis, not an exact
/// schema, so a profile that refused an unrequired input would have turned a
/// floor into a whitelist. The review it reads is one the matcher does NOT
/// select, so the empty matched set stays honest.
#[test]
fn a2_a_complete_absence_basis_with_an_unrequired_extra_input_still_admits() {
    let mut store = Store::default();
    let other = store.retain_under(
        &json!({
            "schemaVersion": 1, "sourceKind": "github-submitted-review",
            "stableId": "9000000777",
            "user": {"id": "9000000777", "login": "somebody-else", "type": "User"},
            "authorAssociation": "NONE", "state": "COMMENTED",
            "body": "unrelated\n", "submittedAt": "2026-08-05T09:02:47Z",
            "commitId": "1f2e3d4c5b6a798807162534435261708f9e0d1c",
        }),
        &retain_all(&REVIEW_REQ),
    );
    let s = store.put(&snapshot(OBSERVATION, std::slice::from_ref(&other), &[]));
    let outcome = admissibility(
        DecisionProfile::Absence,
        &absence_basis(
            Some(&s),
            vec![DecisionInput {
                source_digest: other,
                pointer: "/commitId".to_owned(),
                locator: AcquisitionLocator::Check {
                    repository: "PhysShell/007".to_owned(),
                    stable_id: "0".to_owned(),
                },
            }],
        ),
        &store,
    );
    assert!(
        matches!(outcome, Admissible::Yes { .. }),
        "A2: refused. A COMPLETE snapshot with a bound matcher, an exactly reproduced empty \
         selection and the right observation is what §13 and §14 ask an absence claim for; a \
         minimum-basis row that also refused an input it never required would have stopped \
         being a minimum: got {outcome:?}"
    );
}

/// A3 — BOUNDARY. §14's question, answered by §14. The snapshot is complete,
/// bound and replayed, and its matched set is NOT empty: the evidence the claim
/// rests on contradicts the claim.
///
/// Here so the new row cannot be credited with this refusal, and so a later
/// round cannot "simplify" the profile by folding §14 into it. The refusal must
/// come from query qualification, which already owns it.
#[test]
fn a3_a_non_empty_matched_set_is_still_refused_by_query_qualification() {
    let mut store = Store::default();
    let review = store.retain_under(&matching_review(), &retain_all(&REVIEW_REQ));
    let s = store.put(&snapshot(
        OBSERVATION,
        std::slice::from_ref(&review),
        std::slice::from_ref(&review),
    ));
    let outcome = admissibility(
        DecisionProfile::Absence,
        &absence_basis(Some(&s), Vec::new()),
        &store,
    );
    let Admissible::CannotCheck { why } = &outcome else {
        unreachable!()
    };
    assert!(
        why.iter()
            .any(|u| matches!(u, Unresolved::QueryDoesNotSupportRole { .. })),
        "A3: refused, but not by query qualification. A complete enumeration whose matched set \
         is non-empty contradicts the absence it is offered for, and §14 owns that refusal — \
         the §17 row says only what the basis must PROVIDE: got {why:?}"
    );
    assert!(
        !why.iter()
            .any(|u| matches!(u, Unresolved::BasisIncompleteForProfile { .. })),
        "A3: also reported as an incomplete basis. The basis DID name a snapshot; the snapshot \
         is the thing that does not support the claim: got {why:?}"
    );
}

/// A4 — BOUNDARY. §13's question, answered by §13. A complete enumeration of
/// ANOTHER observation establishes nothing about this one, and F2 already
/// refuses it.
#[test]
fn a4_a_snapshot_for_another_observation_is_still_refused_by_the_relation() {
    let mut store = Store::default();
    let s = store.put(&snapshot(ANOTHER_OBSERVATION, &[], &[]));
    let outcome = admissibility(
        DecisionProfile::Absence,
        &absence_basis(Some(&s), Vec::new()),
        &store,
    );
    let Admissible::CannotCheck { why } = &outcome else {
        unreachable!()
    };
    assert!(
        why.iter().any(|u| matches!(
            u,
            Unresolved::QueryDoesNotSupportRole { why, .. } if why.contains(ANOTHER_OBSERVATION)
        )),
        "A4: refused, but not for the observation the decision is about: got {why:?}"
    );
    assert!(
        !why.iter()
            .any(|u| matches!(u, Unresolved::BasisIncompleteForProfile { .. })),
        "A4: also reported as an incomplete basis. A snapshot WAS named — it is about something \
         else, which is a relation defect and not a missing one: got {why:?}"
    );
}
