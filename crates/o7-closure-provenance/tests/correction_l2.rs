//! RED-ABSENCE-SUBJECT — L2, adjudicated ACCEPT (P1) against frozen §13 / §17.1.
//!
//! An authoritative absence is the one decision whose answer is that nothing was
//! found, so the question of what was searched is the whole of it.
//! `supports_absence` compares the snapshot's `requiredObservationId` against the
//! basis's `observation_id` and stops there. An observation id is a role name —
//! `review/external`, `check/ai-final-review` — and role names are reused across
//! every pull request in the repository, and across repositories.
//!
//! So a COMPLETE, matcher-bound, correctly replayed, empty enumeration of
//! ANOTHER repository's pull request proves absence here. Every §13 and §14
//! obligation is satisfied by that artifact, honestly, about a different change.
//!
//! §17.1 states the rule and names this exact shape as its first consequence:
//!
//! > The subject must arrive from outside the artifacts being checked. Two head
//! > reads of another pull request are real, correctly digested, correctly
//! > roled, and agree with each other perfectly.
//!
//! §13 supplies the other half: the snapshot's `binding` is part of what the
//! artifact IS, so the comparison has something to compare against.
//!
//! THE ASYMMETRY IS THE EVIDENCE THAT THIS IS AN OMISSION RATHER THAN A
//! POSITION. `scan_verdict` already takes a caller-supplied
//! `QueryBinding { repository, pull_request }` and refuses a scan whose evidence
//! enumerates another query — b2's S7. The absence path consumes the same
//! artifact for a role whose whole content is a negative, and took no subject at
//! all. One qualification, two consumers, and only one of them asked.
//!
//! ```text
//! L2-A  an empty enumeration of another repository proves absence   RED
//! L2-B  the same for another pull request in this repository        RED
//! L2-C  the decision's own subject still admits           BOUNDARY  admits
//! L2-D  a mismatching observation is still refused        BOUNDARY  §13
//! L2-E  a non-empty matched set is still refused          BOUNDARY  §14
//! L2-F  the scan path still refuses another query's snapshot BOUNDARY §16
//! L2-G  §17.1 still requires the subject from outside              FREEZE
//! ```
//!
//! L2-B IS NOT A WEAKER L2-A. A cross-REPOSITORY snapshot could be refused by
//! any number of coarse rules — a repository allow-list, a hostname check — that
//! would leave the far likelier case wide open: two pull requests in this
//! repository, the same reviewer role, one enumerated and the other decided
//! about. The two witnesses differ in exactly which half of the binding moved.
//!
//! L2-D AND L2-E ARE HERE BECAUSE A NEW SUBJECT CHECK COULD SWALLOW THE OLD
//! ONES. The observation comparison and §14's empty-matched rule answer
//! different questions, and a repair that made them unreachable would look like
//! three rules and be one.

// Justification for the restriction-lint allowance, per AGENTS.md rule 4: the
// `expect` sites below are this file's own handling of JSON literals written in
// it, unreachable unless a specimen a few lines above is malformed.
#![allow(clippy::expect_used)]

use o7_closure_canonical::digest;
use o7_closure_matcher::{resolve as resolve_matcher, verify_implementation};
use o7_closure_provenance::{
    admissibility, scan_verdict, Admissible, DecisionBasis, DecisionProfile, ExpectedDetector,
    ExpectedQuery, FalsificationSurfaceScan, QueryBinding, RetainedEvidence, ScanCompleteness,
    ScanVerdict, Unresolved,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;

const PROVENANCE: &str = include_str!("../../../docs/architecture/closure-source-provenance-v1.md");

const MATCHER_ID: &str = "review-by-expected-author-login";
const MATCHER_VERSION: &str = "1";
const OBSERVATION: &str = "review/external";
const OURS_REPO: &str = "PhysShell/007";
const OURS_PR: &str = "9001";
const SURFACE: &str = "pull-request-submitted-reviews";

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
    /// Retain a gated record WITH the §9.2 binding that authorises it.
    ///
    /// This exists because M4 found L2-E passing without it. The absence path's
    /// snapshot is ungated, which is why this file's first store had no binding
    /// at all — but §13's CANDIDATE SET is gated, and a candidate with no
    /// binding never reaches the matched-set rule the witness is named after.
    fn retain(&mut self, record: &Value, assessed: &[&str]) -> String {
        let assessment = json!({
            "schemaVersion": 1,
            "sourceKind": "closure-retention-assessment",
            "redactionPolicyVersion": "1",
            "detector": {
                "id": "synthetic-detector",
                "version": "1",
                "configDigest":
                    "sha256:1111111111111111111111111111111111111111111111111111111111111111",
            },
            "representation": "decoded-source-field-values",
            "assessedFields": assessed,
            "coverageComplete": true,
            "outcome": "RETAIN",
            "observedAt": "2026-08-05T09:03:00Z",
        });
        let ad = self.put(&assessment);
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

/// §5.3's required set for a submitted review, so the candidate this file
/// retains is assessed over its whole denominator rather than partially.
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

fn bound_implementation_digest() -> String {
    let entry = resolve_matcher(MATCHER_ID, MATCHER_VERSION).expect("the matcher is registered");
    verify_implementation(entry)
        .expect("the registry is bound to its own implementation")
        .as_str()
        .to_owned()
}

/// A §13 query snapshot: COMPLETE, matcher bound to its implementation, and a
/// candidate set replay reproduces exactly. Every witness here differs from its
/// neighbour in the binding or the matched set and in nothing else.
fn snapshot(repository: &str, pull_request: &str, observation: &str, matched: &[String]) -> Value {
    json!({
        "schemaVersion": 2,
        "sourceKind": "github-query-snapshot",
        "surface": SURFACE,
        "requiredObservationId": observation,
        "binding": {"repository": repository, "pullRequest": pull_request},
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
        "allReturnedSnapshotDigests": matched,
        "matchedSnapshotDigests": matched,
    })
}

/// A submitted review the registered matcher DOES select, for the witness that
/// needs a non-empty matched set.
fn selected_review() -> Value {
    json!({
        "schemaVersion": 1, "sourceKind": "github-submitted-review",
        "stableId": "9000000901",
        "user": {"id": "9000000901", "login": "synthetic-external-reviewer", "type": "User"},
        "authorAssociation": "NONE", "state": "CHANGES_REQUESTED",
        "body": "there is in fact a review here\n",
        "submittedAt": "2026-08-05T09:02:47Z",
        "commitId": "1f2e3d4c5b6a798807162534435261708f9e0d1c",
    })
}

/// The decision: an absence claim about THIS pull request, evidenced by the
/// named snapshot.
fn absence_basis(digest: &str) -> DecisionBasis {
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
            digest: digest.to_owned(),
            subject: QueryBinding {
                repository: OURS_REPO.to_owned(),
                pull_request: OURS_PR.to_owned(),
            },
        }),
        bindings: Vec::new(),
    }
}

/// Refused, AND refused because the enumeration is of another change.
///
/// Pinned on the refusal naming both subjects — values this test supplies, so
/// the assertion does not move when the message is reworded, and does not pass
/// for a refusal about the observation, the enumeration state or the matched
/// set.
#[track_caller]
fn refuses_as_another_subject(outcome: &Admissible, what: &str, enumerated: &str) {
    let why: &[Unresolved] = match outcome {
        Admissible::CannotCheck { why } => why,
        Admissible::Yes { .. } => &[],
    };
    let named = why.iter().any(|u| match u {
        Unresolved::QueryDoesNotSupportRole { why, .. } => {
            why.contains(enumerated) && why.contains(OURS_PR) && why.contains(OURS_REPO)
        }
        _ => false,
    });
    assert!(
        named,
        "{what}: an absence claim about {OURS_REPO}#{OURS_PR} was evidenced by a complete, \
         bound, empty enumeration of {enumerated}, and the refusal does not say so. §17.1: the \
         subject must arrive from outside the artifacts being checked, and an observation id is \
         a role name reused across changes. A real enumeration of another change is not \
         evidence about this one: got {outcome:?}"
    );
}

/// L2-A — THE REPRODUCER, preserved as adjudicated. Another repository
/// entirely, and the decision is admissible.
#[test]
fn l2a_an_empty_enumeration_of_another_repository_proves_absence_here() {
    let mut store = Store::default();
    let elsewhere = store.put(&snapshot(
        "SomeoneElse/unrelated-repo",
        "4242",
        OBSERVATION,
        &[],
    ));
    refuses_as_another_subject(
        &admissibility(DecisionProfile::Absence, &absence_basis(&elsewhere), &store),
        "L2-A",
        "SomeoneElse/unrelated-repo",
    );
}

/// L2-B — the likelier half, and the one a coarse repository rule would miss:
/// the same repository, the same reviewer role, another pull request.
#[test]
fn l2b_an_empty_enumeration_of_another_pull_request_proves_absence_here() {
    let mut store = Store::default();
    let other_pr = store.put(&snapshot(OURS_REPO, "9002", OBSERVATION, &[]));
    refuses_as_another_subject(
        &admissibility(DecisionProfile::Absence, &absence_basis(&other_pr), &store),
        "L2-B",
        "9002",
    );
}

/// L2-C — BOUNDARY. The decision's own subject, enumerated completely and
/// empty, still evidences the absence.
#[test]
fn l2c_the_decisions_own_subject_still_admits() {
    let mut store = Store::default();
    let ours = store.put(&snapshot(OURS_REPO, OURS_PR, OBSERVATION, &[]));
    let outcome = admissibility(DecisionProfile::Absence, &absence_basis(&ours), &store);
    assert!(
        matches!(outcome, Admissible::Yes { .. }),
        "L2-C: refused a complete, bound, empty enumeration of the very change the decision is \
         about. A subject rule that cannot admit its own subject has stopped being a subject \
         rule: got {outcome:?}"
    );
}

/// L2-D — BOUNDARY. §13's observation comparison still fires on its own.
///
/// Right subject, wrong observation: the snapshot enumerates this pull request
/// and answers a different question about it.
#[test]
fn l2d_a_mismatching_observation_is_still_refused() {
    let mut store = Store::default();
    let other_question = store.put(&snapshot(
        OURS_REPO,
        OURS_PR,
        "check/an-entirely-different-observation",
        &[],
    ));
    let outcome = admissibility(
        DecisionProfile::Absence,
        &absence_basis(&other_question),
        &store,
    );
    let why: &[Unresolved] = match &outcome {
        Admissible::CannotCheck { why } => why,
        Admissible::Yes { .. } => &[],
    };
    assert!(
        why.iter().any(|u| matches!(
            u,
            Unresolved::QueryDoesNotSupportRole { why, .. }
                if why.contains("check/an-entirely-different-observation")
        )),
        "L2-D: the observation comparison stopped firing. Subject and observation are different \
         relations — the same change is enumerated for many observations — and a subject rule \
         that made the older one unreachable would look like two rules and be one: got \
         {outcome:?}"
    );
}

/// L2-E — BOUNDARY. §13's empty-matched rule still fires on its own: right
/// subject, right observation, and the enumeration found something.
///
/// THE CANDIDATE IS AUTHORISED, AND THAT IS THE WHOLE POINT OF THIS FIXTURE.
/// As first written this witness retained the selected review with `put`, over
/// a store whose `binding_for` returned `None`, so the candidate was refused at
/// the door — measured:
///
/// ```text
/// QueryDoesNotSupportRole { why: "the query snapshot's candidate sha256:91e78b…
/// did not pass the door: [NoRetentionBinding { … }]" }
/// ```
///
/// It refused, so it passed, and it would have gone on passing with the
/// matched-set rule deleted. `correction_absence.rs`'s A3 — the same question,
/// asked one round earlier — already retained its candidate with authority; the
/// regression was this file's, not the suite's.
///
/// The refusal is now pinned to the rule this witness is named after, by the
/// digest of the matched candidate: a value the test supplies, so it does not
/// move when a message is reworded and cannot be satisfied by a refusal about
/// authority, the observation, or the subject.
#[test]
fn l2e_a_non_empty_matched_set_is_still_refused() {
    let mut store = Store::default();
    let review = store.retain(&selected_review(), &REVIEW_REQ);
    let found = store.put(&snapshot(
        OURS_REPO,
        OURS_PR,
        OBSERVATION,
        std::slice::from_ref(&review),
    ));
    let outcome = admissibility(DecisionProfile::Absence, &absence_basis(&found), &store);
    let why: &[Unresolved] = match &outcome {
        Admissible::CannotCheck { why } => why,
        Admissible::Yes { .. } => &[],
    };
    assert!(
        why.iter().any(|u| matches!(
            u,
            Unresolved::QueryDoesNotSupportRole { why, .. } if why.contains(&review)
        )),
        "L2-E: an absence claim whose own evidence matched a candidate must be refused BY THE \
         MATCHED SET, and the refusal must name the candidate that matched. §13 permits \
         NotProduced only over an empty matched subsequence, and evidence that contradicts a \
         claim is not support for it. A refusal for any other reason leaves that rule \
         unwitnessed — which is exactly what this witness did before M4: got {outcome:?}"
    );
}

/// L2-F — BOUNDARY. The sibling path still refuses another query's snapshot.
///
/// The two consumers share one qualification, so a repair written for the
/// absence path must not cost the scan path the check it already had.
#[test]
fn l2f_the_scan_path_still_refuses_another_querys_snapshot() {
    let mut store = Store::default();
    let elsewhere = store.put(&snapshot(OURS_REPO, "9002", OBSERVATION, &[]));
    let verdict = scan_verdict(
        &FalsificationSurfaceScan {
            surface: SURFACE.to_owned(),
            binding: QueryBinding {
                repository: OURS_REPO.to_owned(),
                pull_request: OURS_PR.to_owned(),
            },
            completeness: ScanCompleteness::Complete,
            expected_redaction_policy: "1".to_owned(),
            expected_detector: ExpectedDetector {
                id: "synthetic-detector".to_owned(),
                version: "1".to_owned(),
                config_digest:
                    "sha256:1111111111111111111111111111111111111111111111111111111111111111"
                        .to_owned(),
            },
            snapshot_digest: elsewhere,
        },
        0,
        &store,
    );
    assert!(
        matches!(verdict, ScanVerdict::CannotCheck { .. }),
        "L2-F: a scan of this pull request evidenced by an enumeration of another one was \
         accepted. §16's scan already had this check; a repair that moved it must not have \
         dropped it: got {verdict:?}"
    );
}

/// L2-G — FREEZE. §17.1 still requires the subject to arrive from outside the
/// artifacts being checked.
#[test]
fn l2g_the_frozen_rule_is_still_the_frozen_rule() {
    assert!(
        PROVENANCE.contains("The subject must arrive from outside the artifacts being checked."),
        "§17.1 no longer requires the subject to arrive from outside the artifacts being \
         checked. That sentence is the whole of L2: without it, a snapshot supplying the only \
         identity it is compared against is a defensible reading"
    );
}
