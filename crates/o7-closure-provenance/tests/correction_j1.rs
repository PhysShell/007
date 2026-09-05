//! RED-PRESENT-ONLY — J1, adjudicated ACCEPT (P0) against frozen §5.3.
//!
//! §5.3 states the rule and states it without a hedge:
//!
//! ```text
//! A present-only field joins the required set exactly when it is present in
//! the decoded source. Absent means nothing to assess; present means it is
//! retained and must therefore be assessed like any other.
//! ```
//!
//! `redaction::check_authorises` implements the coverage half over the `always`
//! set only:
//!
//! ```text
//! if assessment.pointer("/coverageComplete") == Some(&Value::Bool(true)) {
//!     if let Some(missing) = always.iter().find(|p| !is_assessed(p)) { ... }
//! }
//! ```
//!
//! So a complete §8.2 projection that CARRIES `conclusion` is authorised by an
//! assessment naming only `/head_sha /id /name /status` with
//! `coverageComplete: true`, and `admissibility` then hands the unscanned
//! `conclusion` value to the decision. That is a durable unscanned-content
//! channel: not a one-off escape, but a field the gate is structurally never
//! asked about, present on two of the five gated kinds.
//!
//! THE REASON THE HOLE WAS ARGUED FOR, because it was, in a doc comment above
//! `check_authorises`:
//!
//! ```text
//! A consumer holds the record and not the decoded source, so it cannot
//! distinguish a present-only field that was absent upstream from one that was
//! dropped here.
//! ```
//!
//! That is true of a field the record does not carry, and it is the whole
//! argument for not demanding the absent half. It is NOT true of a field the
//! record does carry. A complete §8 projection lists `conclusion` under
//! OPTIONAL-IF-PRESENT and provenance §8 makes `null` and absent the same
//! input, so a projection carrying a non-null `conclusion` IS the record
//! saying the field was present in the decoded source. Presence is
//! determinable exactly where it matters, and the argument for the ceiling was
//! quietly reused as an argument against the floor.
//!
//! THE TWO VOCABULARIES, because the rule has to hold in both (§7.5). §5.3's
//! pointers are the decoded source's; a complete projection is keyed
//! canonically and a §7 reduced record in §5.3's decoded space. The record
//! declares presence in whichever space it is written in:
//!
//! ```text
//! complete projection   the canonical OPTIONAL-IF-PRESENT member is present
//! reduced record        the decoded pointer appears in the partition
//! ```
//!
//! A reduced record naming `/conclusion` in `blockedFields` has said the field
//! existed to be partitioned; §7.1 puts every field of the required set in
//! exactly one half, so a field in neither half was never in the set. The
//! retained half of that partition is already closed — §7.1's "a field the
//! detector never successfully assessed is blocked, always" forces every
//! RETAINED pointer into `assessedFields` — which is why J1's reproducer is a
//! complete projection and why J1-E exists to cover the half that is not.
//!
//! WHAT THIS ROUND DOES NOT DO. It does not demand a present-only field the
//! record does not carry. That half is genuinely undeterminable from the
//! record alone, refusing on it would refuse conformant records, and the
//! ceiling rule (`assessedFields ⊆ always ∪ present_only`) stays exactly as it
//! was. J1-D is the boundary that holds the line.
//!
//! ```text
//! J1-A  complete check carrying `conclusion`, assessed over `always` only,
//!       consumed by a check decision                                     RED
//! J1-B  complete review comment carrying `line`, assessed over `always`
//!       only — a second kind, so the fix cannot be one kind's branch     RED
//! J1-C  reduced record blocking a present-only field it declares present,
//!       `coverageComplete: true`, that field unassessed                  RED
//! J1-D  complete head projection with its present-only member ABSENT,
//!       assessed over `always` only                          BOUNDARY  passes
//! J1-E  complete check carrying `conclusion` WITH `/conclusion` assessed
//!                                                            BOUNDARY  passes
//! J1-F  `null` present-only member is absent, per provenance §8
//!                                                            BOUNDARY  passes
//! J1-G  §5.3 still says what it said                                  FREEZE
//! ```
//!
//! J1-G is here for the same reason G4-G was. Every witness above goes green if
//! §5.3's sentence is softened, with no code change at all, and the suite would
//! look repaired.

// Justification for the restriction-lint allowance, per AGENTS.md rule 4: the
// `expect` sites below are this file's own handling of JSON literals written in
// it, unreachable unless a specimen a few lines above is malformed.
// ONE SITE IS NOT THAT, and is named here rather than left to borrow the
// fixture invariant: the read of §5.3's present-only field rule out of
// `docs/architecture/closure-source-provenance-v1.md`. Its invariant is a
// different one — if the sentence this witness quotes is no longer in the
// contract, the rule moved and nothing else noticed, and the witness must fail
// loudly instead of passing over text that is gone. N1 records why an exception
// is now named: twelve files justified an allowance on the fixture invariant
// while covering sites it never described.
// Extent (checked by N1): 2 `expect` sites.
#![allow(clippy::expect_used)]

use o7_closure_provenance::{
    admissibility, relations_checked, AcquisitionLocator, Admissible, DecisionBasis, DecisionInput,
    DecisionProfile, ExpectedDetector, RetainedEvidence,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;

mod common;

const REDACTION: &str = include_str!("../../../docs/architecture/closure-redaction-policy-v1.md");

const OBSERVATION: &str = "check/ai-final-review";
const CHECK_ID: &str = "9900000001";

/// §5.3's `always` set for `github-actions-check`, in §5.5 order.
const CHECK_ALWAYS: [&str; 4] = ["/head_sha", "/id", "/name", "/status"];

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

/// A conforming §9 assessment reading RETAIN over exactly `assessed`.
///
/// §9 forbids `RETAIN` with `coverageComplete` other than true, so every
/// specimen here claims complete coverage — which is precisely the claim §5.2
/// says a consumer must check against §5.3 rather than against the producer.
fn retain_over(assessed: &[&str]) -> Value {
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

/// A §8.2 check-run projection. `conclusion` is supplied by the caller so the
/// present and absent cases differ in exactly one member.
fn check_projection(conclusion: Option<Value>) -> Value {
    let mut object = json!({
        "schemaVersion": 1,
        "sourceKind": "github-actions-check",
        "stableId": CHECK_ID,
        "name": "ai/final-review",
        "headSha": "1f2e3d4c5b6a798807162534435261708f9e0d1c",
        "status": "completed",
    });
    if let Some(value) = conclusion {
        object
            .as_object_mut()
            .expect("the literal above is an object")
            .insert("conclusion".to_owned(), value);
    }
    object
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
        observation_id: OBSERVATION.to_owned(),
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

fn one_input(digest: &str, pointer: &str) -> DecisionBasis {
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
            source_digest: digest.to_owned(),
            pointer: pointer.to_owned(),
            locator: AcquisitionLocator::Check {
                repository: "PhysShell/007".to_owned(),
                stable_id: CHECK_ID.to_owned(),
            },
        }],
        derived: Vec::new(),
        expected_query: None,
        bindings: Vec::new(),
    }
}

#[track_caller]
fn refuses(outcome: &Result<Vec<Value>, Vec<o7_closure_provenance::Unresolved>>, what: &str) {
    assert!(
        outcome.is_err(),
        "{what}: the pair was accepted. §5.3 puts a PRESENT present-only field in the required \
         set, and §5.2 makes that set the normative denominator for the record's own \
         `coverageComplete` claim. The record itself declares the field present, so nothing \
         undeterminable is being demanded here — the assessment simply never looked at content \
         that was kept: got {outcome:?}"
    );
}

/// J1-A — THE REPRODUCER. A complete check-run projection carrying
/// `conclusion`, authorised by an assessment that never assessed it, feeding a
/// §17 `check` decision that reads exactly that field.
///
/// The value is deliberately not a plausible conclusion. Whatever a decision
/// does with it, no detector was ever asked whether it was safe to keep.
#[test]
fn j1a_an_unassessed_present_only_field_must_not_authorise_the_record() {
    let mut store = Store::default();
    let record = store.retain_under(
        &check_projection(Some(json!("arbitrary text nobody scanned"))),
        &retain_over(&CHECK_ALWAYS),
    );
    let outcome = admissibility(DecisionProfile::Check, &check_basis(&record), &store);
    assert!(
        matches!(outcome, Admissible::CannotCheck { .. }),
        "J1-A: admitted, and the values it admitted include content the gate was never asked \
         about. §5.3: present means it is retained and must therefore be assessed like any \
         other: got {outcome:?}"
    );
}

/// J1-B — a second kind, so the repair cannot be a branch on
/// `github-actions-check`. §8.4's review comment carries five
/// OPTIONAL-IF-PRESENT members; this one carries `line`.
#[test]
fn j1b_the_rule_is_not_one_kinds_special_case() {
    let mut store = Store::default();
    let record = store.retain_under(
        &json!({
            "schemaVersion": 1,
            "sourceKind": "github-review-comment",
            "stableId": "9900000002",
            "pullRequestReviewId": "9900000003",
            "user": {"id": "42", "login": "synthetic-reviewer", "type": "User"},
            "authorAssociation": "NONE",
            "body": "a remark\n",
            "commitId": "1f2e3d4c5b6a798807162534435261708f9e0d1c",
            "originalCommitId": "1f2e3d4c5b6a798807162534435261708f9e0d1c",
            "path": "crates/o7-closure-provenance/src/lib.rs",
            "createdAt": "2026-08-05T09:00:00Z",
            "updatedAt": "2026-08-05T09:00:00Z",
            "line": 1869,
        }),
        &retain_over(&[
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
        ]),
    );
    refuses(
        &relations_checked(&one_input(&record, "/line"), &store),
        "J1-B",
    );
}

/// J1-C — the same rule in the other vocabulary. A reduced record names
/// `/conclusion` in `blockedFields`, which is the record declaring the field
/// existed to be partitioned, and the assessment claims complete coverage
/// without ever assessing it.
///
/// The retained half of this partition is already closed by §7.1's
/// "a field the detector never successfully assessed is blocked, always". The
/// blocked half is not, and `coverageComplete: true` is a claim about the whole
/// required set rather than about the half that survived.
#[test]
fn j1c_a_blocked_present_only_field_is_still_in_the_denominator() {
    let mut store = Store::default();
    let record = store.retain_under(
        &json!({
            "schemaVersion": 1,
            "sourceKind": "github-reduced-source-record",
            "locatorKind": "github-actions-check",
            "locator": {"repository": "PhysShell/007", "stableId": CHECK_ID},
            "redactionPolicyVersion": "1",
            "outcome": "BLOCK_SECRET",
            "coverageComplete": true,
            "retainedFields": {
                "/head_sha": "1f2e3d4c5b6a798807162534435261708f9e0d1c",
                "/id": CHECK_ID,
                "/name": "ai/final-review",
            },
            "blockedFields": ["/conclusion", "/status"],
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
            "assessedFields": CHECK_ALWAYS,
            "coverageComplete": true,
            "outcome": "BLOCK_SECRET",
            "findings": [{"field": "/status", "findingId": "synthetic-finding-1"}],
            "observedAt": "2026-08-05T09:03:00Z",
        }),
    );
    refuses(
        &relations_checked(&one_input(&record, "/head_sha"), &store),
        "J1-C",
    );
}

/// J1-D — BOUNDARY, and the one that keeps the repair from overshooting. §8.1's
/// head projection has one OPTIONAL-IF-PRESENT member, `updatedAt`, and this
/// specimen does not carry it. Nothing about the record says whether the
/// decoded source had one, so §5.3 puts it outside the required set and a
/// conformant assessment over `always` alone is complete.
///
/// A repair that demanded the whole `present_only` set unconditionally would
/// turn this green witness red, and would be refusing conformant records while
/// reporting completeness.
#[test]
fn j1d_an_absent_present_only_field_is_not_required() {
    let mut store = Store::default();
    let record = store.retain_under(
        &json!({
            "schemaVersion": 1,
            "sourceKind": "github-pull-request-head",
            "repository": "PhysShell/007",
            "pullRequest": "9001",
            "headSha": "1f2e3d4c5b6a798807162534435261708f9e0d1c",
            "headRef": "claude/example",
            "headRepoFullName": "PhysShell/007",
        }),
        &retain_over(&["/head/ref", "/head/repo/full_name", "/head/sha", "/number"]),
    );
    let outcome = relations_checked(&one_input(&record, "/headSha"), &store);
    assert!(
        outcome.is_ok(),
        "J1-D: refused a record whose present-only member is absent. §5.3 says absent means \
         nothing to assess, and demanding the undeterminable half refuses conformant records \
         while calling it completeness: got {outcome:?}"
    );
}

/// J1-E — BOUNDARY. The same specimen as J1-A with `/conclusion` in
/// `assessedFields`. This is what a conformant producer emits, and it must stay
/// admissible: the repair is a floor, not a ban on the field.
#[test]
fn j1e_an_assessed_present_only_field_still_authorises() {
    let mut store = Store::default();
    let record = store.retain_under(
        &check_projection(Some(json!("success"))),
        &retain_over(&["/conclusion", "/head_sha", "/id", "/name", "/status"]),
    );
    let outcome = admissibility(DecisionProfile::Check, &check_basis(&record), &store);
    assert!(
        matches!(outcome, Admissible::Yes { .. }),
        "J1-E: refused the conformant shape. Every §5.3 field that is present was assessed and \
         the coverage claim is honest; if this cannot be admitted the repair has removed the \
         decision rather than evidenced it: got {outcome:?}"
    );
}

/// J1-F — BOUNDARY. Provenance §8: `null` and absent are the same input, and
/// §5.3 restates it for this exact set ("a `null` present-only field is absent
/// here too"). A repair keyed on member presence alone would read `null` as
/// present and refuse a record §5.3 does not require anything about.
///
/// The projection carrying an explicit `null` is separately non-conformant to
/// §8's closed shape, and that is a different refusal in a different place.
/// What this witness pins is the redaction rule: `null` does not join the
/// required set.
#[test]
fn j1f_a_null_present_only_member_is_absent() {
    let mut store = Store::default();
    let record = store.retain_under(
        &json!({
            "schemaVersion": 1,
            "sourceKind": "github-reduced-source-record",
            "locatorKind": "github-actions-check",
            "locator": {"repository": "PhysShell/007", "stableId": CHECK_ID},
            "redactionPolicyVersion": "1",
            "outcome": "BLOCK_SECRET",
            "coverageComplete": true,
            "retainedFields": {
                "/head_sha": "1f2e3d4c5b6a798807162534435261708f9e0d1c",
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
            "assessedFields": CHECK_ALWAYS,
            "coverageComplete": true,
            "outcome": "BLOCK_SECRET",
            "findings": [{"field": "/status", "findingId": "synthetic-finding-1"}],
            "observedAt": "2026-08-05T09:03:00Z",
        }),
    );
    let outcome = relations_checked(&one_input(&record, "/head_sha"), &store);
    assert!(
        outcome.is_ok(),
        "J1-F: refused a partition that names no present-only field at all. The three \
         present-only pointers of this kind are in neither half, so §7.1 says they were never \
         in the required set: got {outcome:?}"
    );
}

/// J1-G — FREEZE. §5.3 still says what it said.
///
/// Softening this sentence would turn every witness above green with no code
/// change, and the suite would look repaired. A freeze only the implementation
/// is held to is not a freeze, and the document is the half that can be edited
/// without any test noticing.
#[test]
fn j1g_the_frozen_rule_is_still_the_frozen_rule() {
    let at = REDACTION
        .find("A **present-only** field joins the required set exactly when it is present in")
        .expect(
            "§5.3 no longer states when a present-only field joins the required set. If that \
             was deliberate, J1 is not a defect and this file should be deleted rather than \
             quietly passing; if it was not, the rule just moved and nothing else noticed",
        );
    let rest = REDACTION.get(at..).unwrap_or_default();
    assert!(
        rest.contains("must therefore be assessed like any other"),
        "§5.3 still names the required set but no longer says a present field must be assessed \
         like any other. That second clause is the half J1 turns on: the first says WHEN the \
         field joins the set, and this one says what joining it obliges"
    );
    assert!(
        rest.contains("`null` present-only field is absent here too"),
        "§5.3 no longer equates a null present-only field with an absent one, which is the \
         rule J1-F holds the repair to"
    );
}
