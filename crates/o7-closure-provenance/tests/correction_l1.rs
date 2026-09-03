//! RED-BASIS-SUBJECT — L1, adjudicated ACCEPT (P1) against frozen §17 / §17.1.
//!
//! §17 does not tabulate requirements. It tabulates a **minimum decision basis
//! per observation**:
//!
//! ```text
//! check         observed head_sha, observed conclusion
//! review        observed commit_id, derived carries_finding
//! ```
//!
//! `admissibility` reads each row as an independent existential scan — is there
//! SOME input that observed this field on this surface, is there SOME derived
//! fact with this derivation id — and treats the conjunction as though one
//! artifact had witnessed the row. Nothing relates the two.
//!
//! So a review decision is fully evidenced by two different reviews: review A
//! supplies the observed `commitId`, review B and B's own comment supply
//! `carries_finding`. Every artifact resolves, validates, and answers the
//! question put to it; the derivation recomputes correctly over the sources it
//! cites; and the verdict is `Admissible::Yes` carrying A's commit while the
//! finding evidence is about B, at a different commit entirely.
//!
//! §17.1 is what that violates. Relation validity requires "the artifact's own
//! fields establish the exact subject, role, state, partition and relation under
//! which this decision consumes it", and its first consequence is about exactly
//! this shape: two head reads of another pull request are "real, correctly
//! digested, correctly roled, and agree with each other perfectly". Here they do
//! not even need to be of another subject to be wrong — they need only be two.
//!
//! THE SAME SENTENCE IN THE CHECK PROFILE, WHICH IS WHY L1-B EXISTS. Both check
//! rows are observations of `github-actions-check`, and nothing said they were
//! observations of ONE check run. A decision reporting check X's `head_sha`
//! beside check Y's `conclusion` says a check passed at a commit where that
//! check never ran. The reviewer reported the review profile; the defect is the
//! table's, not the row's, and a repair that closed only the reported half
//! would leave the other admitting identical evidence — J3-B's argument, one
//! entry point over.
//!
//! ```text
//! L1-A  two reviews jointly evidence one review decision           RED
//! L1-B  two check runs jointly evidence one check decision         RED
//! L1-C  one review satisfying both rows                  BOUNDARY  admits
//! L1-D  one check run satisfying both rows               BOUNDARY  admits
//! L1-E  a row that is genuinely absent is still absent   BOUNDARY  §17
//! L1-F  a second review beside a correct pairing         BOUNDARY  admits
//! L1-G  §17 is still per observation, §17.1 still exact           FREEZE
//! ```
//!
//! L1-F IS THE OVER-TIGHTENING GUARD. §17 states a MINIMUM, not an exact
//! schema, and A2 already holds that line for unrequired inputs of other
//! surfaces. The rule this round adds is "there EXISTS one artifact satisfying
//! the rows", and a rule written as "exactly one artifact of this surface
//! appears" would pass L1-A and L1-B and turn a superset basis into
//! `CannotCheck`. The two are different rules and only one of them is §17's.
//!
//! WHAT THE RULE COMPARES, STATED SO IT IS NOT MISTAKEN FOR MORE. It compares
//! DIGESTS: the rows must be witnessed by one retained artifact. It does not
//! reconstruct a subject identity across kinds, so a basis reading one row off a
//! complete §8 projection and the other off a §7 reduced record OF THE SAME
//! object would be refused. That case is not reachable from one gate decision
//! per source per observation — the gate produces one representation — and if it
//! ever becomes reachable the rule needs a subject extractor rather than a
//! loosening. Recorded here rather than discovered later.

use o7_closure_provenance::{
    admissibility, AcquisitionLocator, Admissible, CitedSource, DecisionBasis, DecisionInput,
    DecisionProfile, DerivedFact, ExpectedDetector, RetainedEvidence, Unresolved,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;

mod common;

const PROVENANCE: &str = include_str!("../../../docs/architecture/closure-source-provenance-v1.md");

const REVIEW_A: &str = "9000000901";
const REVIEW_B: &str = "9000000902";
const COMMENT_B: &str = "9000000202";
const CHECK_X: &str = "9000000303";
const CHECK_Y: &str = "9000000304";
const SHA_DECIDED: &str = "1f2e3d4c5b6a798807162534435261708f9e0d1c";
const SHA_OTHER: &str = "aaaabbbbccccddddeeeeffff00001111222233ff";

const CHECK_REQ: [&str; 5] = ["/conclusion", "/head_sha", "/id", "/name", "/status"];
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
        let d = common::digest_of(o);
        self.records.insert(d.clone(), o.clone());
        d
    }
    fn retain(&mut self, record: &Value, assessed: &[&str]) -> String {
        let assessment = retain_all(assessed);
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

fn review(stable_id: &str, commit: &str) -> Value {
    json!({
        "schemaVersion": 1, "sourceKind": "github-submitted-review",
        "stableId": stable_id,
        "user": {"id": stable_id, "login": "synthetic-external-reviewer", "type": "User"},
        "authorAssociation": "NONE", "state": "CHANGES_REQUESTED",
        "body": "still unreachable\n", "submittedAt": "2026-08-05T09:02:47Z",
        "commitId": commit,
    })
}

fn comment_of(review_id: &str, commit: &str) -> Value {
    json!({
        "schemaVersion": 1, "sourceKind": "github-review-comment",
        "stableId": COMMENT_B, "pullRequestReviewId": review_id,
        "user": {"id": review_id, "login": "synthetic-external-reviewer", "type": "User"},
        "authorAssociation": "NONE", "body": "here specifically\n",
        "commitId": commit, "originalCommitId": commit,
        "path": "crates/o7-closure-provenance/src/lib.rs",
        "createdAt": "2026-08-05T09:02:47Z", "updatedAt": "2026-08-05T09:02:47Z",
    })
}

fn actions_check(stable_id: &str, head_sha: &str, conclusion: &str) -> Value {
    json!({
        "schemaVersion": 1, "sourceKind": "github-actions-check",
        "stableId": stable_id, "name": "ai/final-review",
        "headSha": head_sha, "status": "completed", "conclusion": conclusion,
    })
}

fn basis(inputs: Vec<DecisionInput>, derived: Vec<DerivedFact>) -> DecisionBasis {
    DecisionBasis {
        expected_redaction_policy: "1".to_owned(),
        expected_detector: ExpectedDetector {
            id: "synthetic-detector".to_owned(),
            version: "1".to_owned(),
            config_digest:
                "sha256:1111111111111111111111111111111111111111111111111111111111111111".to_owned(),
        },
        observation_id: "review/external".to_owned(),
        inputs,
        derived,
        expected_query: None,
        bindings: Vec::new(),
    }
}

fn reads(digest: &str, pointer: &str) -> DecisionInput {
    DecisionInput {
        source_digest: digest.to_owned(),
        pointer: pointer.to_owned(),
        locator: AcquisitionLocator::Check {
            repository: "PhysShell/007".to_owned(),
            stable_id: REVIEW_A.to_owned(),
        },
    }
}

fn carries_finding(review: &str, comment: &str) -> DerivedFact {
    DerivedFact {
        derivation: "review-carries-finding".to_owned(),
        version: "1".to_owned(),
        value: json!(true),
        derived_from: vec![
            CitedSource {
                digest: review.to_owned(),
                locator: AcquisitionLocator::InPullRequest {
                    repository: "PhysShell/007".to_owned(),
                    pull_request: "9001".to_owned(),
                    stable_id: REVIEW_A.to_owned(),
                },
            },
            CitedSource {
                digest: comment.to_owned(),
                locator: AcquisitionLocator::InPullRequest {
                    repository: "PhysShell/007".to_owned(),
                    pull_request: "9001".to_owned(),
                    stable_id: REVIEW_A.to_owned(),
                },
            },
        ],
    }
}

/// Refused, AND refused because §17's rows for one observation were witnessed
/// by different artifacts of that surface.
///
/// Pinned on the variant plus the surface, per CR-N1: a refusal for some other
/// reason would leave this question unasked and would start reporting green the
/// moment the unrelated defect was fixed.
#[track_caller]
fn refuses_as_split(outcome: &Admissible, what: &str, profile: &str, surface: &str) {
    let why: &[Unresolved] = match outcome {
        Admissible::CannotCheck { why } => why,
        Admissible::Yes { .. } => &[],
    };
    assert!(
        why.iter().any(|u| matches!(
            u,
            Unresolved::BasisSubjectNotShared { profile: p, surface: s }
                if *p == profile && *s == surface
        )),
        "{what}: §17's minimum basis for a {profile} decision is per OBSERVATION, and two \
         different {surface} artifacts each satisfied one of its rows. Every artifact resolved \
         and answered the question put to it; what nothing established is that they are one \
         observation. §17.1: relation validity requires the artifact's own fields to establish \
         the EXACT subject: got {outcome:?}"
    );
}

#[track_caller]
fn admits(outcome: &Admissible, what: &str) {
    assert!(
        matches!(outcome, Admissible::Yes { .. }),
        "{what}: refused a basis that carries §17's minimum witnessed by one artifact. §17 is a \
         minimum, not an exact schema, and a rule that cannot admit this has stopped being \
         about the subject relation: got {outcome:?}"
    );
}

/// L1-A — THE REPRODUCER, preserved as adjudicated. Review A supplies the
/// observed commit; review B and B's comment supply the derived fact.
#[test]
fn l1a_two_different_reviews_evidence_one_review_decision() {
    let mut store = Store::default();
    let a = store.retain(&review(REVIEW_A, SHA_DECIDED), &REVIEW_REQ);
    let b = store.retain(&review(REVIEW_B, SHA_OTHER), &REVIEW_REQ);
    let cb = store.retain(&comment_of(REVIEW_B, SHA_OTHER), &COMMENT_REQ);
    refuses_as_split(
        &admissibility(
            DecisionProfile::Review,
            &basis(vec![reads(&a, "/commitId")], vec![carries_finding(&b, &cb)]),
            &store,
        ),
        "L1-A",
        "review",
        "github-submitted-review",
    );
}

/// L1-B — the same sentence in the other profile. Check X's `headSha` beside
/// check Y's `conclusion`: a decision that says a check passed at a commit where
/// that check never ran.
#[test]
fn l1b_two_different_check_runs_evidence_one_check_decision() {
    let mut store = Store::default();
    let x = store.retain(&actions_check(CHECK_X, SHA_DECIDED, "failure"), &CHECK_REQ);
    let y = store.retain(&actions_check(CHECK_Y, SHA_OTHER, "success"), &CHECK_REQ);
    refuses_as_split(
        &admissibility(
            DecisionProfile::Check,
            &basis(
                vec![reads(&x, "/headSha"), reads(&y, "/conclusion")],
                Vec::new(),
            ),
            &store,
        ),
        "L1-B",
        "check",
        "github-actions-check",
    );
}

/// L1-C — BOUNDARY. One review witnessing both rows still admits.
#[test]
fn l1c_one_review_satisfying_both_rows_still_admits() {
    let mut store = Store::default();
    let a = store.retain(&review(REVIEW_A, SHA_DECIDED), &REVIEW_REQ);
    let ca = store.retain(&comment_of(REVIEW_A, SHA_DECIDED), &COMMENT_REQ);
    admits(
        &admissibility(
            DecisionProfile::Review,
            &basis(vec![reads(&a, "/commitId")], vec![carries_finding(&a, &ca)]),
            &store,
        ),
        "L1-C",
    );
}

/// L1-D — BOUNDARY. One check run witnessing both rows still admits.
#[test]
fn l1d_one_check_run_satisfying_both_rows_still_admits() {
    let mut store = Store::default();
    let x = store.retain(&actions_check(CHECK_X, SHA_DECIDED, "failure"), &CHECK_REQ);
    admits(
        &admissibility(
            DecisionProfile::Check,
            &basis(
                vec![reads(&x, "/headSha"), reads(&x, "/conclusion")],
                Vec::new(),
            ),
            &store,
        ),
        "L1-D",
    );
}

/// L1-E — BOUNDARY. A row that is genuinely absent is still reported absent,
/// and as incompleteness rather than as a split subject.
///
/// The two refusals answer different questions — "nothing was presented" versus
/// "two things were, and they are not one observation" — and a repair that
/// reported the second for the first would have made §17's completeness rule
/// unreachable while looking stricter.
#[test]
fn l1e_a_row_that_is_absent_is_still_absent() {
    let mut store = Store::default();
    let a = store.retain(&review(REVIEW_A, SHA_DECIDED), &REVIEW_REQ);
    let outcome = admissibility(
        DecisionProfile::Review,
        &basis(vec![reads(&a, "/commitId")], Vec::new()),
        &store,
    );
    let why: &[Unresolved] = match &outcome {
        Admissible::CannotCheck { why } => why,
        Admissible::Yes { .. } => &[],
    };
    assert!(
        why.iter().any(|u| matches!(
            u,
            Unresolved::BasisIncompleteForProfile {
                profile: "review",
                missing: "derived carries_finding",
            }
        )),
        "L1-E: a basis carrying no derived fact at all must still be refused as INCOMPLETE. \
         Reporting a split subject for an absent row would answer a question nobody could have \
         asked and would hide §17's completeness rule behind the new one: got {outcome:?}"
    );
}

/// L1-F — BOUNDARY, and the over-tightening guard. A second review that also
/// carries `commitId`, beside a correct pairing, still admits.
///
/// §17 states a MINIMUM. The rule is "there EXISTS one artifact witnessing the
/// rows"; a rule reading "exactly one artifact of this surface appears in the
/// basis" would pass L1-A and turn a superset basis into `CannotCheck`.
#[test]
fn l1f_an_extra_review_beside_a_correct_pairing_still_admits() {
    let mut store = Store::default();
    let a = store.retain(&review(REVIEW_A, SHA_DECIDED), &REVIEW_REQ);
    let ca = store.retain(&comment_of(REVIEW_A, SHA_DECIDED), &COMMENT_REQ);
    let b = store.retain(&review(REVIEW_B, SHA_OTHER), &REVIEW_REQ);
    admits(
        &admissibility(
            DecisionProfile::Review,
            &basis(
                vec![reads(&a, "/commitId"), reads(&b, "/commitId")],
                vec![carries_finding(&a, &ca)],
            ),
            &store,
        ),
        "L1-F",
    );
}

/// L1-G — FREEZE. §17 is still a minimum basis PER OBSERVATION, and §17.1 still
/// requires the exact subject.
///
/// Every witness above goes green if §17 is reworded to tabulate independent
/// requirements, with no code change at all.
#[test]
fn l1g_the_frozen_rules_are_still_the_frozen_rules() {
    assert!(
        PROVENANCE.contains("Minimum decision basis per observation:"),
        "§17 no longer states its table as a minimum basis PER OBSERVATION. That phrase is what \
         makes two rows about one review one requirement rather than two: without it the \
         implementation's independent scans are a defensible reading"
    );
    assert!(
        PROVENANCE.contains("the artifact's own fields establish the exact subject,"),
        "§17.1 no longer requires an artifact's own fields to establish the EXACT subject under \
         which a decision consumes it. That sentence is the one this round implements at the \
         basis level"
    );
}
