//! RED-BLOCKED-EMPTY — K2, adjudicated ACCEPT (P1) against frozen §7.4.
//!
//! §7.4 states it as a MUST, in one sentence, with the reason attached:
//!
//! ```text
//! `blockedFields` MUST be non-empty. A record that blocked nothing is not a
//! reduced record — it is a complete projection, and should be one.
//! ```
//!
//! Nothing implements it. The only `is_empty` in `redaction.rs` is §5.1's
//! `findings: []` rule; `ordered_unique_at` accepts an empty array and every
//! partition rule below it is satisfied vacuously — no pointer is in both
//! halves, no `always` pointer is in neither once `retainedFields` carries them
//! all, no finding names an unblocked field because there are no findings.
//!
//! WHY AN EMPTY PARTITION IS NOT MERELY UNTIDY. §7 defines this kind as the
//! artifact that exists BECAUSE the source did not pass the gate as a whole,
//! and §7.4's second sentence says what a record that blocked nothing actually
//! is. Admitting one lets a producer emit an object that claims the reduced
//! kind's provenance — refused-source semantics, a `locatorKind` naming the
//! kind that was refused, an outcome of `BLOCK_SECRET` or `CANNOT_ASSESS` —
//! while handing over every field it was supposed to have withheld. The gate
//! outcome says content was refused and the partition says none was.
//!
//! The reproducer pairs it with `CANNOT_ASSESS` and `coverageComplete: false`,
//! which is the shape where it bites hardest: the assessment says coverage was
//! incomplete, and the record returns every value anyway. §16's demon —
//! `failure -> empty set -> green` — one level in, where the empty set is the
//! blocked half of a partition.
//!
//! ```text
//! K2-A  CANNOT_ASSESS with blockedFields: [] returns every value        RED
//! K2-B  the same under BLOCK_SECRET, already refused by §7.1
//!                                                            BOUNDARY  refused
//! K2-C  one blocked pointer is enough                        BOUNDARY  passes
//! K2-D  a blocked pointer outside §5.3 is still refused       BOUNDARY  §5.2
//! K2-E  §7.4 still says what it said                                  FREEZE
//! ```
//!
//! ONLY ONE WITNESS IS RED, AND THAT IS A FACT ABOUT THE ESCAPE RATHER THAN
//! THIN COVERAGE. `BLOCK_SECRET` requires a finding (§9.6 computes the outcome
//! from findings), a finding's field must be blocked (§7.1), and an empty
//! blocked half blocks nothing — so that pairing was already refused, by a
//! rule with nothing to do with §7.4. K2-B measures that rather than assuming
//! it. The reachable escape is `CANNOT_ASSESS`, which is exactly the outcome
//! whose assessment says the examination did not complete.

// Justification for the restriction-lint allowance, per AGENTS.md rule 4: the
// `expect` sites below are this file's own handling of JSON literals written in
// it, unreachable unless a specimen a few lines above is malformed.
#![allow(clippy::expect_used)]

use o7_closure_canonical::digest;
use o7_closure_provenance::{
    relations_checked, DecisionBasis, DecisionInput, RetainedEvidence, Unresolved,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;

const REDACTION: &str = include_str!("../../../docs/architecture/closure-redaction-policy-v1.md");

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

/// A §7 reduced record of a check run, with the blocked half supplied by the
/// caller so each witness differs from its neighbour in exactly that member.
fn reduced(outcome: &str, retained: Value, blocked: Value) -> Value {
    json!({
        "schemaVersion": 1,
        "sourceKind": "github-reduced-source-record",
        "locatorKind": "github-actions-check",
        "locator": {"repository": "PhysShell/007", "stableId": CHECK_ID},
        "redactionPolicyVersion": "1",
        "outcome": outcome,
        "coverageComplete": outcome == "BLOCK_SECRET",
        "retainedFields": retained,
        "blockedFields": blocked,
    })
}

/// The authorising assessment. §9.6 computes the outcome from findings and
/// coverage, so the two shapes are not interchangeable and each witness uses
/// the one its record's outcome requires.
fn assessment(outcome: &str) -> Value {
    let mut object = json!({
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
        "coverageComplete": outcome == "BLOCK_SECRET",
        "outcome": outcome,
        "observedAt": "2026-08-05T09:03:00Z",
    });
    let fields = object
        .as_object_mut()
        .expect("the literal above is an object");
    if outcome == "BLOCK_SECRET" {
        fields.insert(
            "findings".to_owned(),
            json!([{"field": "/status", "findingId": "synthetic-finding-1"}]),
        );
    } else {
        fields.insert(
            "coverageFailureCode".to_owned(),
            json!("DETECTOR_UNAVAILABLE"),
        );
    }
    object
}

fn basis(digest: &str, pointer: &str) -> DecisionBasis {
    DecisionBasis {
        expected_redaction_policy: "1".to_owned(),
        observation_id: "check/ai-final-review".to_owned(),
        inputs: vec![DecisionInput {
            source_digest: digest.to_owned(),
            pointer: pointer.to_owned(),
        }],
        derived: Vec::new(),
        expected_query: None,
        bindings: Vec::new(),
    }
}

#[track_caller]
fn refuses(outcome: &Result<Vec<Value>, Vec<Unresolved>>, what: &str) {
    assert!(
        outcome.is_err(),
        "{what}: a record that blocked nothing was accepted as a reduced record, and every \
         value it carries was returned. §7.4: `blockedFields` MUST be non-empty — a record that \
         blocked nothing is not a reduced record, it is a complete projection, and should be \
         one. The gate outcome says content was refused and the partition says none was: got \
         {outcome:?}"
    );
}

/// K2-A — THE REPRODUCER. `CANNOT_ASSESS` with incomplete coverage, every
/// required pointer assessed and retained, nothing blocked.
///
/// This is the pairing where it bites hardest: the assessment says the
/// examination did not complete, and the record hands back every field anyway.
#[test]
fn k2a_a_reduced_record_that_blocked_nothing_is_not_a_reduced_record() {
    let mut store = Store::default();
    let record = store.retain_under(
        &reduced(
            "CANNOT_ASSESS",
            json!({
                "/head_sha": HEAD_SHA,
                "/id": CHECK_ID,
                "/name": "ai/final-review",
                "/status": "completed",
            }),
            json!([]),
        ),
        &assessment("CANNOT_ASSESS"),
    );
    refuses(
        &relations_checked(&basis(&record, "/head_sha"), &store),
        "K2-A",
    );
}

/// K2-B — BOUNDARY, AND THE REASON THE ESCAPE IS OUTCOME-SPECIFIC.
///
/// The same emptiness under `BLOCK_SECRET` was ALREADY refused before this
/// round, and not by anything to do with §7.4. §9.6 computes `BLOCK_SECRET`
/// from the presence of findings, so such a record necessarily has one; §7.1
/// says a field named by a finding is blocked, always; and `blockedFields: []`
/// blocks nothing. Measured, not assumed — the refusal reads:
///
/// ```text
/// AssessmentDoesNotAuthorise { why: "a finding names \"/status\" and the
/// record did not block it. §7.1: a field named by a finding is blocked,
/// always" }
/// ```
///
/// So K2's reachable escape is `CANNOT_ASSESS` alone, which is why K2-A is
/// that outcome and why no `BLOCK_SECRET` witness in this file is red. It is
/// kept as a boundary rather than dropped because the two rules are
/// independent: if §7.1's finding rule ever moves, this case must still be
/// refused, and by then §7.4's floor is the only thing left holding it.
#[test]
fn k2b_the_same_emptiness_under_block_secret_is_refused_either_way() {
    let mut store = Store::default();
    let record = store.retain_under(
        &reduced(
            "BLOCK_SECRET",
            json!({
                "/head_sha": HEAD_SHA,
                "/id": CHECK_ID,
                "/name": "ai/final-review",
                "/status": "completed",
            }),
            json!([]),
        ),
        &assessment("BLOCK_SECRET"),
    );
    refuses(
        &relations_checked(&basis(&record, "/head_sha"), &store),
        "K2-B",
    );
}

/// K2-C — BOUNDARY. One blocked pointer is a reduced record, and must stay
/// admissible for the fields it did retain. §7.4 sets a floor of one, not a
/// proportion.
#[test]
fn k2c_one_blocked_pointer_is_enough() {
    let mut store = Store::default();
    let record = store.retain_under(
        &reduced(
            "BLOCK_SECRET",
            json!({
                "/head_sha": HEAD_SHA,
                "/id": CHECK_ID,
                "/name": "ai/final-review",
            }),
            json!(["/status"]),
        ),
        &assessment("BLOCK_SECRET"),
    );
    let outcome = relations_checked(&basis(&record, "/head_sha"), &store);
    assert!(
        outcome.is_ok(),
        "K2-C: refused a conformant reduced record. §7.4 requires at least one blocked field \
         and this record blocks one; a repair that cannot admit this has replaced the rule with \
         a different one: got {outcome:?}"
    );
}

/// K2-D — BOUNDARY. §5.2's denominator still governs what may be in the
/// partition, so the new floor cannot be satisfied by blocking something the
/// kind does not have. A producer that needed one entry could otherwise invent
/// one.
#[test]
fn k2d_a_blocked_pointer_outside_the_denominator_is_still_refused() {
    let mut store = Store::default();
    let record = store.retain_under(
        &reduced(
            "BLOCK_SECRET",
            json!({
                "/head_sha": HEAD_SHA,
                "/id": CHECK_ID,
                "/name": "ai/final-review",
                "/status": "completed",
            }),
            json!(["/not_a_field_of_this_kind"]),
        ),
        &assessment("BLOCK_SECRET"),
    );
    refuses(
        &relations_checked(&basis(&record, "/head_sha"), &store),
        "K2-D",
    );
}

/// K2-E — FREEZE. §7.4 still says what it said.
///
/// Every witness above goes green if §7.4's sentence is softened, with no code
/// change at all, and the suite would look repaired.
#[test]
fn k2e_the_frozen_rule_is_still_the_frozen_rule() {
    let at = REDACTION.find("`blockedFields` MUST be non-empty").expect(
        "§7.4 no longer requires blockedFields to be non-empty. If that was deliberate, K2 \
             is not a defect and this file should be deleted rather than quietly passing; if it \
             was not, the rule just moved and nothing else noticed",
    );
    let rest = REDACTION.get(at..).unwrap_or_default();
    assert!(
        rest.contains("A record that blocked nothing is not a"),
        "§7.4 still requires a non-empty blockedFields but no longer says what a record that \
         blocked nothing actually is. That second sentence is what makes this a provenance rule \
         rather than a style preference: such an object is a complete projection wearing the \
         reduced kind's identity"
    );
}
