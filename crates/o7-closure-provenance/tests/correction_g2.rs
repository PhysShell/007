//! RED-BASIS-COMPLETENESS — G2. Nothing checks that the basis contains what the
//! decision required.
//!
//! THE CLAIM:
//!
//! > A decision basis that does not carry §17's minimum for the decision being
//! > made has not been checked, and reporting it as admissible is an absent
//! > signal read as a negative result.
//!
//! WHAT `admissibility` DOES TODAY. It walks whatever the basis happens to
//! contain, refuses what fails, and ends:
//!
//! ```text
//! if why.is_empty() { Admissible::Yes { values } } else { CannotCheck { why } }
//! ```
//!
//! An EMPTY basis produces an empty `why`, so it is admissible. Every input it
//! did not name resolved vacuously; every derived fact it did not claim
//! recomputed vacuously. `Admissible::Yes { values: [] }` is returned for a
//! decision nobody evidenced at all.
//!
//! AND THE EMPTY CASE IS THE EASY HALF. A basis carrying SOME of what §17
//! requires is the one that will actually occur, because it is what a partly
//! wired adapter emits: a check decision that observed `head_sha` and never
//! looked at `conclusion` reads as fully evidenced, and the missing half is the
//! half that says whether the check passed. G2-C through G2-F are those cases,
//! and they are the reason this file does not stop at "an empty basis must be
//! refused".
//!
//! §17'S TABLE, which is the authority here and not this crate's judgement:
//!
//! ```text
//! check         observed head_sha, observed conclusion
//! review        observed commit_id, derived carries_finding
//! subject       head_before, head_after
//! falsification subject_sha (if any), verification status
//! ```
//!
//! ONLY TWO OF THE FOUR ROWS ARE PROFILES OF A `DecisionBasis`. `subject`
//! describes the arguments of `staleness` and `falsification` those of
//! `scan_verdict`; neither takes a basis. G2-K holds that split so the two
//! absent rows read as a routing fact rather than as two requirements someone
//! forgot. It is a statement about scope, and it is here because leaving it to
//! prose is how a gap becomes a claim.
//!
//! THE PROFILE MUST COME FROM THE CALLER. `DecisionBasis` already carries
//! `observation_id`, and selecting the requirements from it would let the object
//! under examination nominate the standard it is examined against — the adapter
//! saying "I am a check, so this is what I owe". That is self-certification with
//! a table in front of it, and it is the same defect §17.1 already refuses for
//! the subject: the expectation arrives from outside the artifacts being
//! checked. So the profile is an argument.
//!
//! `observation_id` keeps its existing job — it is compared against a query
//! snapshot's `requiredObservationId`. That is a relation between two artifacts,
//! not a selection of which rule applies, and the difference is the whole point.
//!
//! ```text
//! G2-A  empty basis, check profile                          RED
//! G2-B  empty basis, review profile                         RED
//! G2-C  check: head_sha observed, conclusion never          RED
//! G2-D  check: conclusion observed, head_sha never          RED
//! G2-E  review: commit_id observed, carries_finding never   RED
//! G2-F  review: carries_finding derived, commit_id never    RED
//! G2-G  check: both pointers, one read off the wrong
//!       surface — a head projection's headSha is not the
//!       check's                                             RED
//! G2-H  a complete check basis still admits                 BOUNDARY
//! G2-I  a complete review basis still admits                BOUNDARY
//! G2-J  a complete check basis over REDUCED records still
//!       admits — the requirement is the field, not the
//!       representation                                      BOUNDARY
//! G2-K  §17 has four rows; two are basis profiles           SCOPE
//! ```

// Justification for the restriction-lint allowance, per AGENTS.md rule 4: the
// `expect` sites below are this file's own handling of JSON literals written in
// it, unreachable unless a specimen a few lines above is malformed.
#![allow(clippy::expect_used)]

use o7_closure_canonical::digest;
use o7_closure_provenance::{
    admissibility, AcquisitionLocator, Admissible, CitedSource, DecisionBasis, DecisionInput,
    DecisionProfile, DerivedFact, ExpectedDetector, RetainedEvidence, Unresolved,
};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

const REVIEW_ID: &str = "9000000901";
const COMMENT_ID: &str = "9000000202";
const CHECK_ID: &str = "9000000303";
const HEAD_SHA: &str = "1f2e3d4c5b6a798807162534435261708f9e0d1c";

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
const HEAD_REQ: [&str; 4] = ["/head/ref", "/head/repo/full_name", "/head/sha", "/number"];

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
    /// Retain a complete §8 projection with everything §5.3 requires assessed
    /// and nothing blocked.
    fn retain(&mut self, record: &Value, assessed: &[&str]) -> String {
        let a = retain_all(assessed);
        self.retain_under(record, &a)
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

fn blocking(assessed: &[&str], blocked: &str) -> Value {
    let mut a = retain_all(assessed);
    let o = a.as_object_mut().expect("the assessment is an object");
    o.insert("outcome".to_owned(), json!("BLOCK_SECRET"));
    o.insert(
        "findings".to_owned(),
        json!([{"field": blocked, "findingId": "rule-aws-key"}]),
    );
    a
}

/// §8.3. `conclusion` is optional in the projection and `present_only` in §5.3 —
/// a check that has not finished does not have one. §17 requires it for a check
/// DECISION, which is the same statement from the other side: a decision about
/// whether a check passed cannot be made from a check that has not concluded.
fn actions_check() -> Value {
    json!({
        "schemaVersion": 1, "sourceKind": "github-actions-check",
        "stableId": CHECK_ID, "name": "some-ci-job",
        "headSha": HEAD_SHA, "status": "completed", "conclusion": "success",
    })
}

/// §8.1. Carries `headSha` too, and is not the check's.
fn pull_request_head() -> Value {
    json!({
        "schemaVersion": 1, "sourceKind": "github-pull-request-head",
        "repository": "PhysShell/007", "pullRequest": "9001",
        "headSha": HEAD_SHA, "headRef": "claude/closure-classifier-provenance",
        "headRepoFullName": "PhysShell/007",
    })
}

fn complete_review() -> Value {
    json!({
        "schemaVersion": 1, "sourceKind": "github-submitted-review",
        "stableId": REVIEW_ID,
        "user": {"id": REVIEW_ID, "login": "synthetic-external-reviewer", "type": "User"},
        "authorAssociation": "NONE", "state": "CHANGES_REQUESTED",
        "body": "still unreachable\n", "submittedAt": "2026-08-05T09:02:47Z",
        "commitId": HEAD_SHA,
    })
}

fn complete_comment() -> Value {
    json!({
        "schemaVersion": 1, "sourceKind": "github-review-comment",
        "stableId": COMMENT_ID, "pullRequestReviewId": REVIEW_ID,
        "user": {"id": REVIEW_ID, "login": "synthetic-external-reviewer", "type": "User"},
        "authorAssociation": "NONE", "body": "here specifically\n",
        "commitId": HEAD_SHA, "originalCommitId": HEAD_SHA,
        "path": "crates/o7-closure-provenance/src/lib.rs",
        "createdAt": "2026-08-05T09:02:47Z", "updatedAt": "2026-08-05T09:02:47Z",
    })
}

/// A §7 reduced record: `blocked` withheld, every other §5.3 required field
/// retained under its decoded name.
///
/// The locator follows §7.3's per-kind shape — a `github-actions-check` is
/// identified by repository and stableId and has no `pullRequest`, and a locator
/// carrying one is refused as malformed before any of this file's questions are
/// reached.
fn reduced(locator_kind: &str, required: &[&str], stable_id: &str, blocked: &str) -> Value {
    let mut retained = Map::new();
    for pointer in required {
        if *pointer == blocked {
            continue;
        }
        let value = match *pointer {
            "/id" => json!(stable_id),
            "/head_sha" | "/commit_id" => json!(HEAD_SHA),
            "/conclusion" => json!("success"),
            "/status" => json!("completed"),
            "/pull_request_review_id" => json!(REVIEW_ID),
            _ => json!("value-as-the-projection-carries-it"),
        };
        retained.insert((*pointer).to_owned(), value);
    }
    json!({
        "schemaVersion": 1,
        "sourceKind": "github-reduced-source-record",
        "locatorKind": locator_kind,
        "locator": if locator_kind == "github-actions-check" {
            json!({"repository": "PhysShell/007", "stableId": stable_id})
        } else {
            json!({
                "repository": "PhysShell/007",
                "pullRequest": "9001",
                "stableId": stable_id,
            })
        },
        "redactionPolicyVersion": "1",
        "outcome": "BLOCK_SECRET",
        "coverageComplete": true,
        "retainedFields": Value::Object(retained),
        "blockedFields": [blocked],
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
            stable_id: "0".to_owned(),
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
                    stable_id: "0".to_owned(),
                },
            },
            CitedSource {
                digest: comment.to_owned(),
                locator: AcquisitionLocator::InPullRequest {
                    repository: "PhysShell/007".to_owned(),
                    pull_request: "9001".to_owned(),
                    stable_id: "0".to_owned(),
                },
            },
        ],
    }
}

/// THE ENTRY POINT UNDER TEST, behind one adapter line.
///
/// At RED nothing consumed a profile, so this dropped it; GREEN forwards it.
/// Not one witness below changed between the two commits — which is the point:
/// evidence rewritten by the change it was supposed to judge has judged nothing.
fn decide(profile: DecisionProfile, basis: &DecisionBasis, store: &Store) -> Admissible {
    admissibility(profile, basis, store)
}

/// Refused, AND refused because the named §17 requirement was not in the basis.
///
/// Pinned on the profile and §17's own wording for the requirement, per CR-N1.
/// Both are values this test supplies or the contract fixes, so neither moves
/// when a message is reworded.
#[track_caller]
fn refuses_as_incomplete(outcome: &Admissible, what: &str, profile: &str, missing: &str) {
    let why: &[Unresolved] = match outcome {
        Admissible::CannotCheck { why } => why,
        Admissible::Yes { .. } => &[],
    };
    assert!(
        matches!(outcome, Admissible::CannotCheck { .. }),
        "{what}: admitted. §17's minimum basis for a {profile} decision requires {missing}, and \
         the basis does not carry it. Nothing failed a check here because nothing was presented \
         to one — an absent signal reported as a passed check"
    );
    assert!(
        why.iter().any(|u| matches!(
            u,
            Unresolved::BasisIncompleteForProfile { profile: p, missing: m }
                if *p == profile && *m == missing
        )),
        "{what}: refused, but not for a {profile} basis missing {missing}. A refusal for some \
         other reason leaves the completeness question unasked, and would keep reporting green \
         over it the moment the unrelated defect was fixed: got {why:?}"
    );
}

#[track_caller]
fn admits(outcome: &Admissible, what: &str) {
    assert!(
        matches!(outcome, Admissible::Yes { .. }),
        "{what}: refused. A completeness rule that refuses a basis carrying everything §17 asks \
         for has stopped being a completeness rule: got {outcome:?}"
    );
}

// ---- The empty basis.

/// G2-A — nothing was evidenced, and the decision is admissible.
#[test]
fn g2a_an_empty_basis_is_not_a_check_decision() {
    let store = Store::default();
    refuses_as_incomplete(
        &decide(DecisionProfile::Check, &basis(vec![], vec![]), &store),
        "G2-A",
        "check",
        "observed head_sha",
    );
}

/// G2-B — the same for a review, carried separately because the two profiles
/// require different things and a fix that hard-coded one would pass the other.
#[test]
fn g2b_an_empty_basis_is_not_a_review_decision() {
    let store = Store::default();
    refuses_as_incomplete(
        &decide(DecisionProfile::Review, &basis(vec![], vec![]), &store),
        "G2-B",
        "review",
        "observed commit_id",
    );
}

// ---- The partial basis, which is the case that will actually occur.

/// G2-C — the adapter observed the head_sha and never read the conclusion. The
/// decision is about whether the check passed; the missing half is the half that
/// says so.
#[test]
fn g2c_a_check_basis_without_the_conclusion_is_incomplete() {
    let mut store = Store::default();
    let c = store.retain(&actions_check(), &CHECK_REQ);
    refuses_as_incomplete(
        &decide(
            DecisionProfile::Check,
            &basis(vec![reads(&c, "/headSha")], vec![]),
            &store,
        ),
        "G2-C",
        "check",
        "observed conclusion",
    );
}

/// G2-D — the mirror. A conclusion nobody tied to a commit is a verdict about an
/// unnamed subject.
#[test]
fn g2d_a_check_basis_without_the_head_sha_is_incomplete() {
    let mut store = Store::default();
    let c = store.retain(&actions_check(), &CHECK_REQ);
    refuses_as_incomplete(
        &decide(
            DecisionProfile::Check,
            &basis(vec![reads(&c, "/conclusion")], vec![]),
            &store,
        ),
        "G2-D",
        "check",
        "observed head_sha",
    );
}

/// G2-E — a review basis that read the commit and never derived the fact §18
/// exists for. `carries_finding` is not a GitHub field; a basis omitting it has
/// not asserted it, and nothing notices.
#[test]
fn g2e_a_review_basis_without_the_derived_fact_is_incomplete() {
    let mut store = Store::default();
    let r = store.retain(&complete_review(), &REVIEW_REQ);
    refuses_as_incomplete(
        &decide(
            DecisionProfile::Review,
            &basis(vec![reads(&r, "/commitId")], vec![]),
            &store,
        ),
        "G2-E",
        "review",
        "derived carries_finding",
    );
}

/// G2-F — the mirror: the fact is derived and recomputed correctly, and the
/// commit it is about was never observed.
#[test]
fn g2f_a_review_basis_without_the_commit_id_is_incomplete() {
    let mut store = Store::default();
    let r = store.retain(&complete_review(), &REVIEW_REQ);
    let c = store.retain(&complete_comment(), &COMMENT_REQ);
    refuses_as_incomplete(
        &decide(
            DecisionProfile::Review,
            &basis(vec![], vec![carries_finding(&r, &c)]),
            &store,
        ),
        "G2-F",
        "review",
        "observed commit_id",
    );
}

/// G2-G — both requirements present by NAME, one read off the wrong surface.
///
/// §8.1's `github-pull-request-head` carries a `headSha` too, and it is the pull
/// request's, not this check's. A completeness rule that counted pointers would
/// pass this: two inputs, both spelled right. The requirement is a field OF A
/// SURFACE, and G1 established that the crate can now tell which surface an
/// artifact is.
#[test]
fn g2g_a_check_basis_reading_head_sha_off_a_head_projection_is_incomplete() {
    let mut store = Store::default();
    let c = store.retain(&actions_check(), &CHECK_REQ);
    let h = store.retain(&pull_request_head(), &HEAD_REQ);
    refuses_as_incomplete(
        &decide(
            DecisionProfile::Check,
            &basis(
                vec![reads(&h, "/headSha"), reads(&c, "/conclusion")],
                vec![],
            ),
            &store,
        ),
        "G2-G",
        "check",
        "observed head_sha",
    );
}

// ---- The boundaries.

/// G2-H — BOUNDARY. Everything §17 asks a check decision for.
#[test]
fn g2h_a_complete_check_basis_still_admits() {
    let mut store = Store::default();
    let c = store.retain(&actions_check(), &CHECK_REQ);
    admits(
        &decide(
            DecisionProfile::Check,
            &basis(
                vec![reads(&c, "/headSha"), reads(&c, "/conclusion")],
                vec![],
            ),
            &store,
        ),
        "G2-H",
    );
}

/// G2-I — BOUNDARY. Everything §17 asks a review decision for.
#[test]
fn g2i_a_complete_review_basis_still_admits() {
    let mut store = Store::default();
    let r = store.retain(&complete_review(), &REVIEW_REQ);
    let c = store.retain(&complete_comment(), &COMMENT_REQ);
    admits(
        &decide(
            DecisionProfile::Review,
            &basis(vec![reads(&r, "/commitId")], vec![carries_finding(&r, &c)]),
            &store,
        ),
        "G2-I",
    );
}

/// G2-J — BOUNDARY, and the one a representation-blind fix fails. The same
/// complete check basis over a REDUCED record, whose fields are keyed in §5.3's
/// decoded space: `/head_sha`, not `/headSha`. §17 requires the observation, and
/// redaction §8 requires a decision whose inputs survived the gate to remain
/// makeable. A requirement written against the canonical spelling alone would
/// refuse every reduced record and call it completeness.
#[test]
fn g2j_a_complete_check_basis_over_reduced_records_still_admits() {
    let mut store = Store::default();
    let record = reduced("github-actions-check", &CHECK_REQ, CHECK_ID, "/name");
    let a = blocking(&CHECK_REQ, "/name");
    let c = store.retain_under(&record, &a);
    admits(
        &decide(
            DecisionProfile::Check,
            &basis(
                vec![reads(&c, "/head_sha"), reads(&c, "/conclusion")],
                vec![],
            ),
            &store,
        ),
        "G2-J",
    );
}

/// G2-M — an input the store cannot produce is a RETENTION failure, and must not
/// also be reported as an incomplete basis.
///
/// The basis names both of a check decision's requirements. One of them resolves
/// to nothing, so its surface is unknown and there is no way to say which
/// requirement it was meant to satisfy. Reporting incompleteness here would
/// misdescribe lost bytes as a wrongly built adapter — the same misdiagnosis
/// class G1-F refuses — and would send the reader to fix code when the remedy is
/// to retain the record.
///
/// The decision is still refused. That is the safety argument for the skip: the
/// completeness pass only ever ADDS refusals and is skipped only when one is
/// already recorded, so it can change which reasons are listed and can never
/// admit a decision.
#[test]
fn g2m_an_unresolved_input_is_not_reported_as_an_incomplete_basis() {
    let mut store = Store::default();
    let c = store.retain(&actions_check(), &CHECK_REQ);
    let outcome = decide(
        DecisionProfile::Check,
        &basis(
            vec![
                reads(&c, "/headSha"),
                reads(
                    "sha256:00000000000000000000000000000000000000000000000000000000000000ff",
                    "/conclusion",
                ),
            ],
            vec![],
        ),
        &store,
    );
    // Two asserts rather than a let-else: `clippy::panic` is denied tree-wide
    // and a witness does not spend a restriction-lint allowance to save a line.
    let why: &[Unresolved] = match &outcome {
        Admissible::CannotCheck { why } => why,
        Admissible::Yes { .. } => &[],
    };
    assert!(
        matches!(outcome, Admissible::CannotCheck { .. }),
        "G2-M: admitted. An input nobody retained is not evidence"
    );
    assert!(
        why.iter()
            .any(|u| matches!(u, Unresolved::NoSuchRecord { .. })),
        "G2-M: refused, but not for the record nobody retained: got {why:?}"
    );
    assert!(
        !why.iter()
            .any(|u| matches!(u, Unresolved::BasisIncompleteForProfile { .. })),
        "G2-M: also reported as an incomplete basis. The basis NAMED the conclusion input; the \
         store could not produce it. Calling that incompleteness blames the adapter for a \
         retention failure and sends the reader to the wrong repair: got {why:?}"
    );
}

const PROVENANCE: &str = include_str!("../../../docs/architecture/closure-source-provenance-v1.md");

/// §17's minimum-basis table, parsed out of the contract.
///
/// Returns `(row name, [requirement, ..])` for all four rows.
fn contract_minimum_basis() -> Vec<(String, Vec<String>)> {
    let at = PROVENANCE
        .find("Minimum decision basis per observation:")
        .expect("§17 no longer states the minimum decision basis");
    let rest = &PROVENANCE[at..];
    let open = rest
        .find("```text")
        .expect("no fenced block follows §17's table");
    let body = &rest[at + open - at + "```text".len()..];
    let close = body.find("```").expect("unterminated fenced block in §17");
    body[..close]
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(|line| {
            let (row, requirements) = line
                .split_once(char::is_whitespace)
                .expect("every §17 row is a name followed by its requirements");
            (
                row.to_owned(),
                requirements
                    .split(',')
                    .map(|r| r.trim().to_owned())
                    .collect(),
            )
        })
        .collect()
}

/// G2-L — the table in `lib.rs` is §17's, or the build fails.
///
/// Two transcriptions of one rule is one rule too many, and this one decides
/// what a decision must carry to be made at all. The expectation is the
/// markdown, parsed: correcting §17 without the table fails here, and "fixing"
/// the table to match drifted code fails here too. Neither is reachable by
/// editing one file.
///
/// The check is over §17's own WORDING because that wording is what a refusal
/// quotes back — a reader who sees `missing: "observed conclusion"` must be able
/// to find that phrase in the contract, not a paraphrase of it.
#[test]
fn g2l_the_requirement_table_is_the_contracts() {
    let contract = contract_minimum_basis();
    let rows: Vec<&str> = contract.iter().map(|(r, _)| r.as_str()).collect();
    assert_eq!(
        rows,
        vec!["check", "review", "absence", "subject", "falsification"],
        "§17's table changed shape. Every row below is read positionally, so a \
         reordering or a new row must be looked at rather than absorbed"
    );

    for (profile, row) in [
        (DecisionProfile::Check, "check"),
        (DecisionProfile::Review, "review"),
        (DecisionProfile::Absence, "absence"),
    ] {
        let (_, expected) = contract
            .iter()
            .find(|(r, _)| r == row)
            .expect("the row exists; the assertion above holds that");
        let declared: Vec<&str> = profile.requires().iter().map(|r| r.name).collect();
        assert_eq!(
            &declared,
            &expected.iter().map(String::as_str).collect::<Vec<_>>(),
            "the {row} profile requires {declared:?}, and §17 says {expected:?}. The contract \
             is the authority: a requirement this crate adds is one it invented, and one it \
             drops is a decision that can be made without evidence §17 says it needs"
        );
    }
}

/// G2-K — SCOPE, not behaviour. §17 tabulates four minimum bases and only two of
/// them describe a `DecisionBasis`: `subject` names the arguments of
/// `staleness`, `falsification` those of `scan_verdict`, and neither function
/// takes a basis at all.
///
/// Held as an executable count so the two absent rows are a routing fact with a
/// test behind it rather than two requirements that quietly went missing. If a
/// third profile is ever added to `DecisionProfile`, this fails and whoever adds
/// it must say which row it is.
#[test]
fn g2k_two_of_four_contract_rows_are_basis_profiles() {
    let profiles = [
        DecisionProfile::Check,
        DecisionProfile::Review,
        DecisionProfile::Absence,
    ];
    let names: Vec<&str> = profiles.iter().map(|p| p.name()).collect();
    assert_eq!(
        names,
        vec!["check", "review", "absence"],
        "§17's five rows are check, review, absence, subject and falsification. The first three describe a \
         decision basis and are profiles; subject is the argument shape of `staleness` and \
         falsification that of `scan_verdict`, and neither takes a basis. A profile added here \
         without a §17 row to point at is this crate inventing a requirement; a §17 row that \
         gains a basis shape belongs here and this assertion is where that is noticed"
    );
}
