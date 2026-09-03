//! RED-PARTITION-EQUALITY — L3, adjudicated ACCEPT (P1) against frozen §7.1.
//!
//! §7.1 does not state an inclusion. It states an EQUALITY, and then says the
//! missing direction out loud:
//!
//! ```text
//! blockedFields   = flagged  ∪  (required \ assessed)
//! retainedFields  = required \ blockedFields
//! ```
//!
//! > Retention is not discretionary in the other direction either. A field that
//! > survives the computation is retained, so the record cannot be thinned by
//! > judgement after the fact.
//!
//! `check_authorises` enforces two inclusions and calls it the equality:
//!
//! ```text
//! flagged   ⊆ blocked      "a field named by a finding is blocked, always"
//! retained  ⊆ assessed     "an unassessed field is blocked, always"
//! ```
//!
//! Nothing enforces `blocked ⊆ flagged ∪ (required \ assessed)`. A field that
//! WAS assessed and was NOT flagged may be blocked anyway, and every remaining
//! rule holds while it happens: the halves are disjoint, nothing required is in
//! neither, the finding's own field is blocked, and every retained field was
//! assessed.
//!
//! WHY SUPPRESSING CLEAN EVIDENCE IS A PROVENANCE DEFECT AND NOT UNTIDINESS.
//! The gate's output is the input to a decision. A producer that may withhold
//! fields the computation retained can choose which evidenced decisions remain
//! makeable — and it does not have to lie to do it, because the record it emits
//! passes every check. §7.1 answers this by making the partition COMPUTED: both
//! halves are determined by the assessment, so "what was withheld" is not a
//! degree of freedom the producer holds. One unchecked direction hands it back.
//!
//! The consequence is measured rather than asserted. Reading a field §7.1
//! computes as RETAINED returns `PointerBlocked` — a retention LOSS — so an
//! evidenced decision reads as `CannotCheck` and the reason names the redaction
//! gate for something the gate never decided.
//!
//! ```text
//! L3-A  an assessed, unflagged field is blocked                    RED
//! L3-B  the same for a PRESENT present-only field                  RED
//! L3-C  the computed partition still authorises          BOUNDARY  admits
//! L3-D  an UNASSESSED field is blocked with no finding    BOUNDARY  admits
//! L3-E  a retained unassessed field is still refused      BOUNDARY  §7.1
//! L3-F  §7.1 still states the equality and the direction          FREEZE
//! ```
//!
//! L3-B IS NOT A DUPLICATE OF L3-A. §5.3 splits the required set into `always`
//! and present-only, and J1 established that a present-only field JOINS the
//! required set exactly when the record declares it present. A repair written
//! over `always` alone would pass L3-A and leave the present half exposed —
//! which is the shape of the defect J1 was, one rule later. The two witnesses
//! differ in exactly which half of §5.3 the suppressed field comes from.

// Justification for the restriction-lint allowance, per AGENTS.md rule 4: the
// `expect` sites below are this file's own handling of JSON literals written in
// it, unreachable unless a specimen a few lines above is malformed.
#![allow(clippy::expect_used)]

use o7_closure_canonical::digest;
use o7_closure_provenance::{
    relations_checked, AcquisitionLocator, DecisionBasis, DecisionInput, ExpectedDetector,
    RetainedEvidence, Unresolved,
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

/// The value a §7 reduced record carries for each §5.3 decoded pointer.
fn value_of(pointer: &str) -> Value {
    match pointer {
        "/head_sha" => json!(HEAD_SHA),
        "/id" => json!(CHECK_ID),
        "/name" => json!("ai/final-review"),
        "/status" => json!("completed"),
        "/conclusion" => json!("success"),
        _ => json!("value-as-the-projection-carries-it"),
    }
}

/// A §7 reduced record of a check run. `blocked` is supplied by the caller and
/// `retained` is everything else the witness names, so neighbouring witnesses
/// differ in exactly which pointer moved across the partition.
fn reduced(outcome: &str, coverage_complete: bool, retained: &[&str], blocked: &[&str]) -> Value {
    let mut kept = serde_json::Map::new();
    for pointer in retained {
        kept.insert((*pointer).to_owned(), value_of(pointer));
    }
    json!({
        "schemaVersion": 1,
        "sourceKind": "github-reduced-source-record",
        "locatorKind": "github-actions-check",
        "locator": {"repository": "PhysShell/007", "stableId": CHECK_ID},
        "redactionPolicyVersion": "1",
        "outcome": outcome,
        "coverageComplete": coverage_complete,
        "retainedFields": Value::Object(kept),
        "blockedFields": blocked,
    })
}

/// The authorising assessment. §9.6 computes the outcome from findings, so a
/// witness cannot choose the outcome and the findings independently.
fn assessment(
    outcome: &str,
    coverage_complete: bool,
    assessed: &[&str],
    flagged: &[&str],
) -> Value {
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
        "assessedFields": assessed,
        "coverageComplete": coverage_complete,
        "outcome": outcome,
        "observedAt": "2026-08-05T09:03:00Z",
    });
    let fields = object
        .as_object_mut()
        .expect("the literal above is an object");
    if outcome == "BLOCK_SECRET" {
        fields.insert(
            "findings".to_owned(),
            Value::Array(
                flagged
                    .iter()
                    .enumerate()
                    .map(
                        |(i, f)| json!({"field": f, "findingId": format!("synthetic-finding-{i}")}),
                    )
                    .collect(),
            ),
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
        expected_detector: ExpectedDetector {
            id: "synthetic-detector".to_owned(),
            version: "1".to_owned(),
            config_digest:
                "sha256:1111111111111111111111111111111111111111111111111111111111111111".to_owned(),
        },
        observation_id: "check/ai-final-review".to_owned(),
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
fn refuses(outcome: &Result<Vec<Value>, Vec<Unresolved>>, what: &str, suppressed: &str) {
    assert!(
        outcome.is_err(),
        "{what}: a record that withheld {suppressed} was authorised. That field was assessed \
         and no finding names it, so §7.1's computation retains it: `blockedFields = flagged ∪ \
         (required \\ assessed)`, and retention is not discretionary in the other direction \
         either. A producer that may thin the record after the fact chooses which evidenced \
         decisions stay makeable: got {outcome:?}"
    );
}

/// L3-A — THE REPRODUCER, preserved as adjudicated. Every `always` field
/// assessed, one finding on `/status`, and `/name` blocked alongside it.
///
/// §7.1 computes `blockedFields = {"/status"}` for this assessment. The record
/// says `{"/name", "/status"}` and is admitted.
#[test]
fn l3a_an_assessed_unflagged_field_may_be_blocked_anyway() {
    let mut store = Store::default();
    let record = store.retain_under(
        &reduced(
            "BLOCK_SECRET",
            true,
            &["/head_sha", "/id"],
            &["/name", "/status"],
        ),
        &assessment(
            "BLOCK_SECRET",
            true,
            &["/head_sha", "/id", "/name", "/status"],
            &["/status"],
        ),
    );
    refuses(
        &relations_checked(&basis(&record, "/head_sha"), &store),
        "L3-A",
        "\"/name\"",
    );
}

/// L3-B — the same defect where the suppressed field is a PRESENT present-only
/// field, which is the half a repair written over `always` alone would miss.
///
/// `/conclusion` is present-only in §5.3. The record declares it present by
/// naming it in the partition at all — J1's rule — so it joins the required set
/// and is assessed here. Nothing flags it, so §7.1 retains it.
#[test]
fn l3b_a_present_present_only_field_may_be_blocked_anyway() {
    let mut store = Store::default();
    let record = store.retain_under(
        &reduced(
            "BLOCK_SECRET",
            true,
            &["/head_sha", "/id", "/name"],
            &["/conclusion", "/status"],
        ),
        &assessment(
            "BLOCK_SECRET",
            true,
            &["/conclusion", "/head_sha", "/id", "/name", "/status"],
            &["/status"],
        ),
    );
    refuses(
        &relations_checked(&basis(&record, "/head_sha"), &store),
        "L3-B",
        "the present present-only field \"/conclusion\"",
    );
}

/// L3-C — BOUNDARY. The partition §7.1 actually computes still authorises.
///
/// A rule that refused this would have replaced the equality with a demand that
/// nothing be blocked.
#[test]
fn l3c_the_computed_partition_still_authorises() {
    let mut store = Store::default();
    let record = store.retain_under(
        &reduced(
            "BLOCK_SECRET",
            true,
            &["/head_sha", "/id", "/name"],
            &["/status"],
        ),
        &assessment(
            "BLOCK_SECRET",
            true,
            &["/head_sha", "/id", "/name", "/status"],
            &["/status"],
        ),
    );
    let outcome = relations_checked(&basis(&record, "/head_sha"), &store);
    assert!(
        outcome.is_ok(),
        "L3-C: refused the exact partition §7.1 computes for this assessment. A repair that \
         cannot admit `flagged ∪ (required \\ assessed)` has stopped implementing the equality \
         and started forbidding redaction: got {outcome:?}"
    );
}

/// L3-D — BOUNDARY, and it is the half of the union that has no finding behind
/// it. `/name` is blocked because it was never assessed, which §7.1 requires
/// and which a repair phrased as "every blocked field is flagged" would refuse.
#[test]
fn l3d_an_unassessed_field_is_blocked_without_any_finding() {
    let mut store = Store::default();
    let record = store.retain_under(
        &reduced(
            "CANNOT_ASSESS",
            false,
            &["/head_sha", "/id", "/status"],
            &["/name"],
        ),
        &assessment(
            "CANNOT_ASSESS",
            false,
            &["/head_sha", "/id", "/status"],
            &[],
        ),
    );
    let outcome = relations_checked(&basis(&record, "/head_sha"), &store);
    assert!(
        outcome.is_ok(),
        "L3-D: refused a field blocked for being unassessed. `required \\ assessed` is half of \
         §7.1's union, and a rule that only admits flagged fields has implemented a different \
         equality: got {outcome:?}"
    );
}

/// L3-E — BOUNDARY. The direction that was already enforced still is: a record
/// retaining a field the detector never assessed stays refused.
///
/// Here so a repair cannot satisfy the new direction by loosening the old one —
/// the two together are the equality, and either alone is an inclusion.
#[test]
fn l3e_a_retained_unassessed_field_is_still_refused() {
    let mut store = Store::default();
    let record = store.retain_under(
        &reduced(
            "CANNOT_ASSESS",
            false,
            &["/head_sha", "/id", "/name", "/status"],
            &["/conclusion"],
        ),
        &assessment(
            "CANNOT_ASSESS",
            false,
            &["/head_sha", "/id", "/status"],
            &[],
        ),
    );
    let outcome = relations_checked(&basis(&record, "/head_sha"), &store);
    assert!(
        outcome.is_err(),
        "L3-E: admitted a record retaining \"/name\", which is not in /assessedFields. §7.1: a \
         field the detector never successfully assessed is blocked, always: got {outcome:?}"
    );
}

/// L3-F — FREEZE. §7.1 still states an equality, and still states the direction
/// this round implements.
///
/// Every behavioural witness above goes green if §7.1 is softened to an
/// inclusion, with no code change at all, and the suite would look repaired.
#[test]
fn l3f_the_frozen_rule_is_still_the_frozen_rule() {
    assert!(
        REDACTION.contains("blockedFields   = flagged  ∪  (required \\ assessed)"),
        "§7.1 no longer computes blockedFields as `flagged ∪ (required \\ assessed)`. If that \
         was deliberate, L3 is not a defect and this file should be deleted rather than \
         quietly passing; if it was not, the rule just moved and nothing else noticed"
    );
    assert!(
        REDACTION.contains("Retention is not discretionary in the other direction either."),
        "§7.1 still computes the partition but no longer says that retention is not \
         discretionary in the other direction. That sentence is the one this round implements: \
         without it the equality reads as two inclusions a consumer may pick between, which is \
         exactly what the implementation did"
    );
}
