//! RED-POLICY-DOMAIN — K1, adjudicated ACCEPT (P0) against frozen §9.4.
//!
//! §9.4 is the whole security argument of the assessment schema, and it is not
//! a tidiness rule:
//!
//! ```text
//! every field of an assessment is a closed vocabulary value,
//! a structural identifier, a boolean, or a JSON pointer
//!
//! no field of an assessment is free text
//! ```
//!
//! with the reason stated immediately after: *"A closed field cannot carry a
//! secret out because its RANGE does not depend on the content inspected."*
//!
//! `redactionPolicyVersion` is declared `MemberKind::Text` and its value is
//! bounded by nothing. An unbounded string is free text by §9.4's own
//! definition, and the assessment carrying it is canonicalized, digested and
//! retained permanently as the authority for a record a decision then reads.
//!
//! WHAT WAS CLAIMED, AND WHY THE CLAIM WAS WRONG. This member is not
//! unexamined: `correction_g3.rs`'s denominator walk classifies it, twice, as
//! bounded `Elsewhere` —
//!
//! ```text
//! assessment/redactionPolicyVersion
//!     "§9.5 equality: a record's own /redactionPolicyVersion must equal its
//!      authorising assessment's, checked by check_authorises"
//! reduced/redactionPolicyVersion
//!     "§9.5 equality, as for the assessment's"
//! ```
//!
//! That justification fails in two independent ways, and K1-A and K1-B are the
//! two:
//!
//! 1. **The cited check does not run for a complete projection.** §8's
//!    projections carry no `redactionPolicyVersion`, so there is no second
//!    value to compare, and `check_authorises` says so in its own comment. The
//!    citation names a check that this case never reaches.
//! 2. **Equality is not a bound on range.** Both values come from the same
//!    producer. The same secret on both sides satisfies §9.5 exactly. Equality
//!    constrains DISAGREEMENT; §9.4 requires a constrained RANGE, and those are
//!    different properties.
//!
//! Codex reported only the first. The second is what makes this a hole in the
//! evidence rather than only in the code: the G3 inventory existed precisely to
//! make an unbounded `Text` member impossible to leave unnoticed, and it
//! recorded a bound that never held.
//!
//! WHAT THE CONTRACT DOES AND DOES NOT SAY, because the repair turns on it.
//! §9 declares the member REQUIRED. §9.5 requires the record's to equal the
//! assessment's. §9.4 requires the field not to be free text. **No contract
//! text anywhere registers a permitted value** — not §9, not §17, not a
//! specimen; `"1"` appears in this repository's fixtures and in no normative
//! sentence. So the domain cannot be written down here: an implementation
//! hard-coding a policy identifier would be inventing the norm it claims to
//! enforce, which is the direction G3 refused for `observedAt` and which the
//! adjudication forbids in as many words.
//!
//! What §9.4 does permit is a range that does not depend on the inspected
//! content, and the crate already has one shape for that: the expectation
//! arrives from OUTSIDE the artifacts being judged. `Subject::expected_sha`,
//! `DecisionBasis::expected_query_digest` and the caller-supplied
//! `DecisionProfile` are all the same move, and §17.1's first consequence is
//! the rule behind it. A producer that writes a secret into the field is then
//! refused, because the value it must match was fixed by a different party
//! before the evaluation — and no policy-version vocabulary is invented in
//! implementation.
//!
//! ```text
//! K1-A  complete projection, credential in the assessment's policy
//!       version, consumed by a check decision                          RED
//! K1-B  reduced record, THE SAME CREDENTIAL ON BOTH SIDES — §9.5's
//!       equality satisfied exactly                                     RED
//! K1-C  the head-read path, where no basis is supplied at all          RED
//! K1-D  a conformant policy version still authorises        BOUNDARY  passes
//! K1-E  §9.5's record/assessment disagreement stays refused  BOUNDARY  §9.5
//! K1-F  §9.4 still says what it said                                FREEZE
//! ```
//!
//! K1-C is the one that decides where the repair goes. `staleness` takes no
//! `DecisionBasis` and passes no declared bindings, yet it resolves a gated
//! head snapshot and therefore authorises it through the same assessment path.
//! A repair reaching only the basis would leave this open and would be the
//! "one caller, not the rule" error J3-B was written against.

// Justification for the restriction-lint allowance, per AGENTS.md rule 4: the
// `expect` sites below are this file's own handling of JSON literals written in
// it, unreachable unless a specimen a few lines above is malformed.
// ONE SITE IS NOT THAT, and is named here rather than left to borrow the
// fixture invariant: the read of §9.4's no-free-text rule out of
// `docs/architecture/closure-source-provenance-v1.md`. Its invariant is a
// different one — if the sentence this witness quotes is no longer in the
// contract, the rule moved and nothing else noticed, and the witness must fail
// loudly instead of passing over text that is gone. N1 records why an exception
// is now named: twelve files justified an allowance on the fixture invariant
// while covering sites it never described.
// Extent (checked by N1): 2 `expect` sites.
#![allow(clippy::expect_used)]

use o7_closure_canonical::digest;
use o7_closure_provenance::{
    admissibility, relations_checked, staleness, AcquisitionLocator, Admissible, DecisionBasis,
    DecisionInput, DecisionProfile, ExpectedDetector, HeadRead, RetainedEvidence, Staleness,
    Subject, SubjectRead, Unresolved,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;

const REDACTION: &str = include_str!("../../../docs/architecture/closure-redaction-policy-v1.md");

/// The whole of a credential, in the one member §9.4 says cannot carry one.
const SECRET: &str = "ghp_the_entire_credential_rides_out_in_a_version_field";

const CHECK_ID: &str = "9900000001";
const HEAD_SHA: &str = "1f2e3d4c5b6a798807162534435261708f9e0d1c";

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

/// A conforming §9 assessment reading RETAIN, with the policy version supplied
/// by the caller so each witness differs from its neighbour in that member
/// alone.
fn retain_over(policy: &str, assessed: &[&str]) -> Value {
    json!({
        "schemaVersion": 1,
        "sourceKind": "closure-retention-assessment",
        "redactionPolicyVersion": policy,
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

const CHECK_ASSESSED: [&str; 5] = ["/conclusion", "/head_sha", "/id", "/name", "/status"];
const HEAD_ASSESSED: [&str; 4] = ["/head/ref", "/head/repo/full_name", "/head/sha", "/number"];

fn check_projection() -> Value {
    json!({
        "schemaVersion": 1,
        "sourceKind": "github-actions-check",
        "stableId": CHECK_ID,
        "name": "ai/final-review",
        "headSha": HEAD_SHA,
        "status": "completed",
        "conclusion": "success",
    })
}

fn check_basis(digest: &str) -> DecisionBasis {
    DecisionBasis {
        expected_redaction_policy: "1".to_owned(),
        expected_detector: ExpectedDetector {
            id: "synthetic-detector".to_owned(),
            version: "1".to_owned(),
            config_digest:
                "sha256:1111111111111111111111111111111111111111111111111111111111111111".to_owned(),
        },
        observation_id: "check/ai-final-review".to_owned(),
        inputs: vec![
            DecisionInput {
                source_digest: digest.to_owned(),
                pointer: "/headSha".to_owned(),
                locator: AcquisitionLocator::Check {
                    repository: "PhysShell/007".to_owned(),
                    stable_id: "0".to_owned(),
                },
            },
            DecisionInput {
                source_digest: digest.to_owned(),
                pointer: "/conclusion".to_owned(),
                locator: AcquisitionLocator::Check {
                    repository: "PhysShell/007".to_owned(),
                    stable_id: "0".to_owned(),
                },
            },
        ],
        derived: Vec::new(),
        expected_query: None,
        bindings: Vec::new(),
    }
}

/// What the store permanently holds under the authority of the record a
/// decision read: the assessment's own bytes, credential included.
fn retained_policy_version(store: &Store, record: &str) -> Option<Value> {
    store
        .bindings
        .get(record)
        .and_then(|b| b.pointer("/assessmentDigest").and_then(Value::as_str))
        .and_then(|d| store.resolve(d))
        .and_then(|a| a.pointer("/redactionPolicyVersion").cloned())
}

/// K1-A — THE REPRODUCER. A complete §8.2 projection, authorised by an
/// assessment whose policy version is a credential.
///
/// Nothing about the record is otherwise irregular: every §5.3 field including
/// the present-only `conclusion` is assessed, coverage is complete, the outcome
/// is RETAIN, the binding resolves and names this record. The decision is
/// admitted and the credential is in retained evidence for good.
#[test]
fn k1a_an_unbounded_policy_version_authorises_a_complete_projection() {
    let mut store = Store::default();
    let record = store.retain_under(&check_projection(), &retain_over(SECRET, &CHECK_ASSESSED));
    let outcome = admissibility(DecisionProfile::Check, &check_basis(&record), &store);
    assert!(
        matches!(outcome, Admissible::CannotCheck { .. }),
        "K1-A: admitted. §9.4 says no field of an assessment is free text, and gives the reason \
         — a closed field cannot carry a secret out because its RANGE does not depend on the \
         content inspected. This one's range is every string there is, and the value now sits \
         in a permanently retained, canonicalized, digested assessment: got {outcome:?}, \
         retained policy version {:?}",
        retained_policy_version(&store, &record)
    );
}

/// K1-B — THE HALF THE REVIEW MISSED, and the half that falsifies the G3
/// inventory rather than only the code.
///
/// A reduced record carries its own `redactionPolicyVersion`, so §9.5's
/// equality — the check the inventory cited as this member's bound — actually
/// runs here. It passes. The same credential is on both sides, which is exactly
/// what §9.5 asks for: the record's value equals the assessment's.
///
/// Equality constrains DISAGREEMENT between two producer-supplied values. §9.4
/// requires a constrained RANGE. Citing the first as evidence of the second is
/// the shape this whole branch keeps finding: a guard checks proxy P, reports
/// property Q, and P is not Q.
#[test]
fn k1b_equality_is_not_a_bound_on_range() {
    let mut store = Store::default();
    let record = store.retain_under(
        &json!({
            "schemaVersion": 1,
            "sourceKind": "github-reduced-source-record",
            "locatorKind": "github-actions-check",
            "locator": {"repository": "PhysShell/007", "stableId": CHECK_ID},
            "redactionPolicyVersion": SECRET,
            "outcome": "BLOCK_SECRET",
            "coverageComplete": true,
            "retainedFields": {
                "/head_sha": HEAD_SHA,
                "/id": CHECK_ID,
                "/name": "ai/final-review",
            },
            "blockedFields": ["/status"],
        }),
        &json!({
            "schemaVersion": 1,
            "sourceKind": "closure-retention-assessment",
            "redactionPolicyVersion": SECRET,
            "detector": {
                "id": "synthetic-detector",
                "version": "1",
                "configDigest": "sha256:1111111111111111111111111111111111111111111111111111111111111111",
            },
            "representation": "decoded-source-field-values",
            "assessedFields": ["/head_sha", "/id", "/name", "/status"],
            "coverageComplete": true,
            "outcome": "BLOCK_SECRET",
            "findings": [{"field": "/status", "findingId": "synthetic-finding-1"}],
            "observedAt": "2026-08-05T09:03:00Z",
        }),
    );
    let outcome = relations_checked(
        &DecisionBasis {
            expected_redaction_policy: "1".to_owned(),
            expected_detector: ExpectedDetector {
                id: "synthetic-detector".to_owned(),
                version: "1".to_owned(),
                config_digest:
                    "sha256:1111111111111111111111111111111111111111111111111111111111111111"
                        .to_owned(),
            },
            observation_id: "check/ai-final-review".to_owned(),
            inputs: vec![DecisionInput {
                source_digest: record.clone(),
                pointer: "/head_sha".to_owned(),
                locator: AcquisitionLocator::Check {
                    repository: "PhysShell/007".to_owned(),
                    stable_id: "0".to_owned(),
                },
            }],
            derived: Vec::new(),
            expected_query: None,
            bindings: Vec::new(),
        },
        &store,
    );
    assert!(
        outcome.is_err(),
        "K1-B: accepted, with §9.5's equality satisfied exactly — the same credential on both \
         sides. That equality is what `correction_g3.rs` cites as this member's bound, and it \
         bounds only whether two producer-supplied values agree. A field whose range is every \
         string is free text however many places it is copied to: got {outcome:?}"
    );
}

/// K1-C — the path with no `DecisionBasis` at all, which is what decides where
/// the repair belongs.
///
/// §5.3 puts `github-pull-request-head` in the gated set, so a head snapshot
/// needs §9.2 authority like any other gated record, and `staleness` resolves
/// it. It takes a `Subject` and no basis, and passes no declared bindings — so
/// a repair written into `DecisionBasis` alone would leave this route open and
/// would be the "one caller, not the rule" error J3-B exists to refuse.
#[test]
fn k1c_the_rule_belongs_to_authorisation_and_not_to_one_entry_point() {
    let mut store = Store::default();
    let mut head = |role: &str| {
        let snapshot = store.retain_under(
            &json!({
                "schemaVersion": 1,
                "sourceKind": "github-pull-request-head",
                "repository": "PhysShell/007",
                "pullRequest": "9001",
                "headSha": HEAD_SHA,
                "headRef": "claude/example",
                "headRepoFullName": "PhysShell/007",
            }),
            &retain_over(SECRET, &HEAD_ASSESSED),
        );
        let event = store.put(&json!({
            "schemaVersion": 1,
            "sourceKind": "github-head-read-event",
            "role": role,
            "acquisition": "AVAILABLE",
            "snapshotDigest": snapshot,
            "observedAt": if role == "HEAD_BEFORE" {
                "2026-08-05T09:00:00Z"
            } else {
                "2026-08-05T10:00:00Z"
            },
        }));
        HeadRead::Observed {
            event_digest: event,
        }
    };
    let read = SubjectRead {
        before: head("HEAD_BEFORE"),
        after: head("HEAD_AFTER"),
    };
    let verdict = staleness(
        &Subject {
            expected_redaction_policy: "1".to_owned(),
            expected_detector: ExpectedDetector {
                id: "synthetic-detector".to_owned(),
                version: "1".to_owned(),
                config_digest:
                    "sha256:1111111111111111111111111111111111111111111111111111111111111111"
                        .to_owned(),
            },
            repository: "PhysShell/007".to_owned(),
            pull_request: "9001".to_owned(),
            expected_sha: HEAD_SHA.to_owned(),
        },
        &read,
        &store,
    );
    assert!(
        matches!(verdict, Staleness::CannotCheck { .. }),
        "K1-C: the subject-read path authorised two head snapshots on assessments carrying a \
         credential in their policy version, and answered a verdict. This entry point takes no \
         DecisionBasis, so a repair placed there would not reach it — the rule belongs where \
         authorisation happens: got {verdict:?}"
    );
}

/// K1-D — BOUNDARY. A conformant policy version must still authorise, or the
/// repair has removed the decision rather than evidenced it.
#[test]
fn k1d_a_conformant_policy_version_still_authorises() {
    let mut store = Store::default();
    let record = store.retain_under(&check_projection(), &retain_over("1", &CHECK_ASSESSED));
    let outcome = admissibility(DecisionProfile::Check, &check_basis(&record), &store);
    assert!(
        matches!(outcome, Admissible::Yes { .. }),
        "K1-D: refused the conformant shape: got {outcome:?}"
    );
}

/// K1-E — BOUNDARY. §9.5's own rule must survive the repair. A reduced record
/// whose policy version disagrees with its authorising assessment's is refused
/// for THAT reason, and the refusal must still name it: a version field
/// unrelated to its neighbours is decoration on bytes about to be hashed.
#[test]
fn k1e_the_record_assessment_disagreement_is_still_refused() {
    let mut store = Store::default();
    let record = store.retain_under(
        &json!({
            "schemaVersion": 1,
            "sourceKind": "github-reduced-source-record",
            "locatorKind": "github-actions-check",
            "locator": {"repository": "PhysShell/007", "stableId": CHECK_ID},
            "redactionPolicyVersion": "2",
            "outcome": "BLOCK_SECRET",
            "coverageComplete": true,
            "retainedFields": {
                "/head_sha": HEAD_SHA,
                "/id": CHECK_ID,
                "/name": "ai/final-review",
            },
            "blockedFields": ["/status"],
        }),
        &json!({
            "schemaVersion": 1,
            "sourceKind": "closure-retention-assessment",
            "redactionPolicyVersion": "1",
            "detector": {
                "id": "synthetic-detector",
                "version": "1",
                "configDigest": "sha256:1111111111111111111111111111111111111111111111111111111111111111",
            },
            "representation": "decoded-source-field-values",
            "assessedFields": ["/head_sha", "/id", "/name", "/status"],
            "coverageComplete": true,
            "outcome": "BLOCK_SECRET",
            "findings": [{"field": "/status", "findingId": "synthetic-finding-1"}],
            "observedAt": "2026-08-05T09:03:00Z",
        }),
    );
    let outcome = relations_checked(
        &DecisionBasis {
            expected_redaction_policy: "1".to_owned(),
            expected_detector: ExpectedDetector {
                id: "synthetic-detector".to_owned(),
                version: "1".to_owned(),
                config_digest:
                    "sha256:1111111111111111111111111111111111111111111111111111111111111111"
                        .to_owned(),
            },
            observation_id: "check/ai-final-review".to_owned(),
            inputs: vec![DecisionInput {
                source_digest: record.clone(),
                pointer: "/head_sha".to_owned(),
                locator: AcquisitionLocator::Check {
                    repository: "PhysShell/007".to_owned(),
                    stable_id: "0".to_owned(),
                },
            }],
            derived: Vec::new(),
            expected_query: None,
            bindings: Vec::new(),
        },
        &store,
    );
    let named_9_5 = matches!(&outcome, Err(why) if why.iter().any(|u| matches!(
        u,
        Unresolved::AssessmentDoesNotAuthorise { why, .. } if why.contains("§9.5")
    )));
    assert!(
        named_9_5,
        "K1-E: §9.5's record/assessment agreement is a separate rule from §9.4's range, and the \
         repair must not swallow it. A refusal that no longer names §9.5 has replaced one check \
         with another rather than adding one: got {outcome:?}"
    );
}

/// K1-F — FREEZE. §9.4 still says what it said.
///
/// Every witness above goes green if §9.4's closure argument is softened, with
/// no code change at all, and the suite would look repaired.
#[test]
fn k1f_the_frozen_rule_is_still_the_frozen_rule() {
    let at = REDACTION
        .find("no field of an assessment is free text")
        .expect(
            "§9.4 no longer states that no field of an assessment is free text. If that was \
             deliberate, K1 is not a defect and this file should be deleted rather than quietly \
             passing; if it was not, the rule just moved and nothing else noticed",
        );
    let before = REDACTION.get(..at).unwrap_or_default();
    assert!(
        before.contains("every field of an assessment is a closed vocabulary value"),
        "§9.4 still forbids free text but no longer enumerates what a field MAY be. That list \
         is what makes the rule checkable without guessing what a secret looks like"
    );
    let rest = REDACTION.get(at..).unwrap_or_default();
    assert!(
        rest.contains("range does not depend on the content inspected"),
        "§9.4 no longer states WHY a closed field cannot carry a secret out. That sentence is \
         the whole of K1: it names RANGE as the property, which is what distinguishes it from \
         the equality relation §9.5 imposes on the same member"
    );
}
