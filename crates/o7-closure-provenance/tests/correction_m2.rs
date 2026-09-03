//! RED-DETECTOR-RANGE — M2, reported against b9eaa7d, against frozen §9.4.
//!
//! §9.4 states the property the assessment schema's whole security argument
//! rests on, and states it as a closed disjunction:
//!
//! ```text
//! every field of an assessment is a closed vocabulary value,
//! a structural identifier, a boolean, or a JSON pointer
//!
//! no field of an assessment is free text
//! ```
//!
//! > A closed field cannot carry a secret out because its range does not depend
//! > on the content inspected.
//!
//! All three members of the `detector` block are declared `MemberKind::Text`,
//! which admits every string. `detector` is a field of an assessment. So a
//! producer may write a credential into `id`, `version` or `configDigest`, and
//! the assessment carrying it is canonicalized, digested and retained
//! permanently as the AUTHORITY for a record a decision then reads — the exact
//! channel §9.4 closed `reasonDetail` to prevent, reopened one member over.
//!
//! THIS IS K1 AT A DIFFERENT MEMBER OF THE SAME OBJECT. `redactionPolicyVersion`
//! was the same defect and was repaired by taking the permitted value from the
//! caller, because no contract sentence registers one. Nothing registers a
//! detector id, version or configuration digest either.
//!
//! AND IT FALSIFIES THREE MORE `correction_g3.rs` CLASSIFICATIONS, for the
//! second time in this branch's history. Those rows read `Residual`, citing
//! DET-BIND — "No registry binds a detector identity to an implementation, so a
//! record may name a detector nobody ran". That is true and it answers a
//! DIFFERENT question. Binding an identity to something that ran and bounding
//! what the member may CARRY are two obligations; the DET-BIND residual
//! discharges the first and never claimed the second. §9.4 asks only the second.
//!
//! ```text
//! M2-A  detector.id carries a credential and authorises          RED
//! M2-B  detector.version, same                                   RED
//! M2-C  detector.configDigest, same                              RED
//! M2-D  the caller's own detector still authorises     BOUNDARY  admits
//! M2-E  §9.5's policy rule still fires on its own      BOUNDARY  K1
//! M2-F  the head-read path is closed too                         RED
//! M2-G  §9.4 still says what it says                            FREEZE
//! ```
//!
//! M2-F IS K1-C'S ARGUMENT AND IT IS NOT REDUNDANT. `staleness` takes no
//! `DecisionBasis`, and §5.3 puts `github-pull-request-head` in the gated set,
//! so both head snapshots are authorised through an assessment. A repair
//! reaching only the basis leaves that route open, which is why the expectation
//! travels on `Subject` as well.
//!
//! WHAT A REPAIR HERE MUST NOT CLAIM. Closing the range does not resolve
//! `configDigest`, does not bind any member to anything that ran, and does not
//! discharge DET-BIND. M2-H would be the witness for that if it could be
//! written; it cannot, because "nothing resolves this digest" has no positive
//! observable. It is stated in the module docs of the repair instead, and the
//! residual stays in §23.

// Justification for the restriction-lint allowance, per AGENTS.md rule 4: the
// `expect` sites below are this file's own handling of JSON literals written in
// it, unreachable unless a specimen a few lines above is malformed.
#![allow(clippy::expect_used)]

use o7_closure_canonical::digest;
use o7_closure_provenance::{
    relations_checked, staleness, AcquisitionLocator, DecisionBasis, DecisionInput,
    ExpectedDetector, HeadRead, RetainedEvidence, Staleness, Subject, SubjectRead, Unresolved,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;

const REDACTION: &str = include_str!("../../../docs/architecture/closure-redaction-policy-v1.md");

const SECRET: &str = "ghp_the_entire_credential_rides_out_in_a_detector_field";
const CHECK_ID: &str = "9900000001";
const HEAD_SHA: &str = "1f2e3d4c5b6a798807162534435261708f9e0d1c";
const CONFIG: &str = "sha256:1111111111111111111111111111111111111111111111111111111111111111";

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
            "schemaVersion": 1, "sourceKind": "closure-retention-binding",
            "recordDigest": rd, "assessmentDigest": ad,
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

/// The detector identity the CALLER expects. Every witness below differs from
/// this in exactly one member, or in none.
fn expected() -> ExpectedDetector {
    ExpectedDetector {
        id: "synthetic-detector".to_owned(),
        version: "1".to_owned(),
        config_digest: CONFIG.to_owned(),
    }
}

/// A conforming §9 assessment whose `detector` block is supplied by the witness.
fn assessment(id: &str, version: &str, config_digest: &str) -> Value {
    json!({
        "schemaVersion": 1, "sourceKind": "closure-retention-assessment",
        "redactionPolicyVersion": "1",
        "detector": {"id": id, "version": version, "configDigest": config_digest},
        "representation": "decoded-source-field-values",
        "assessedFields": ["/head_sha", "/id", "/name", "/status"],
        "coverageComplete": true, "outcome": "BLOCK_SECRET",
        "observedAt": "2026-08-05T09:03:00Z",
        "findings": [{"field": "/status", "findingId": "synthetic-finding-1"}],
    })
}

fn reduced() -> Value {
    json!({
        "schemaVersion": 1, "sourceKind": "github-reduced-source-record",
        "locatorKind": "github-actions-check",
        "locator": {"repository": "PhysShell/007", "stableId": CHECK_ID},
        "redactionPolicyVersion": "1",
        "outcome": "BLOCK_SECRET", "coverageComplete": true,
        "retainedFields": {"/head_sha": HEAD_SHA, "/id": CHECK_ID, "/name": "ai/final-review"},
        "blockedFields": ["/status"],
    })
}

fn basis(digest: &str, policy: &str) -> DecisionBasis {
    DecisionBasis {
        expected_redaction_policy: policy.to_owned(),
        expected_detector: expected(),
        observation_id: "check/ai-final-review".to_owned(),
        inputs: vec![DecisionInput {
            source_digest: digest.to_owned(),
            pointer: "/head_sha".to_owned(),
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
fn refuses(outcome: &Result<Vec<Value>, Vec<Unresolved>>, what: &str, member: &str) {
    assert!(
        outcome.is_err(),
        "{what}: an assessment whose detector.{member} carries a credential authorised a record, \
         and the assessment is retained permanently as that record's authority. §9.4: every \
         field of an assessment is a closed vocabulary value, a structural identifier, a \
         boolean, or a JSON pointer — no field of an assessment is free text, because a closed \
         field cannot carry a secret out when its RANGE does not depend on the content \
         inspected: got {outcome:?}"
    );
}

/// M2-A — THE REPORTED MEMBER. `detector.id` is credential bytes.
#[test]
fn m2a_a_detector_id_of_free_text_authorises_a_record() {
    let mut store = Store::default();
    let record = store.retain_under(&reduced(), &assessment(SECRET, "1", CONFIG));
    refuses(
        &relations_checked(&basis(&record, "1"), &store),
        "M2-A",
        "id",
    );
}

/// M2-B — the same member table, the next row. A repair naming only `id` would
/// leave this one open, which is the shape of the defect it is repairing.
#[test]
fn m2b_a_detector_version_of_free_text_authorises_a_record() {
    let mut store = Store::default();
    let record = store.retain_under(
        &reduced(),
        &assessment("synthetic-detector", SECRET, CONFIG),
    );
    refuses(
        &relations_checked(&basis(&record, "1"), &store),
        "M2-B",
        "version",
    );
}

/// M2-C — and the third. `configDigest` is sixty-four hex characters BY
/// CONVENTION and `Text` BY DECLARATION, and nothing here checks the
/// convention; §9.4 does not ask for a grammar, it asks that the range not
/// depend on the content inspected.
#[test]
fn m2c_a_detector_config_digest_of_free_text_authorises_a_record() {
    let mut store = Store::default();
    let record = store.retain_under(&reduced(), &assessment("synthetic-detector", "1", SECRET));
    refuses(
        &relations_checked(&basis(&record, "1"), &store),
        "M2-C",
        "configDigest",
    );
}

/// M2-D — BOUNDARY. The detector the caller expects still authorises.
#[test]
fn m2d_the_callers_own_detector_still_authorises() {
    let mut store = Store::default();
    let record = store.retain_under(&reduced(), &assessment("synthetic-detector", "1", CONFIG));
    let outcome = relations_checked(&basis(&record, "1"), &store);
    assert!(
        outcome.is_ok(),
        "M2-D: refused the detector identity the caller supplied. A range rule that admits \
         nothing has replaced §9.4 with a prohibition: got {outcome:?}"
    );
}

/// M2-E — BOUNDARY. K1's rule still fires on its own, so this round added a
/// rule and swallowed none.
#[test]
fn m2e_the_policy_version_rule_still_fires() {
    let mut store = Store::default();
    let record = store.retain_under(&reduced(), &assessment("synthetic-detector", "1", CONFIG));
    let outcome = relations_checked(&basis(&record, "2"), &store);
    assert!(
        outcome.is_err(),
        "M2-E: an assessment made under redaction policy \"1\" authorised an evaluation made \
         under \"2\". K1's rule must survive M2's: got {outcome:?}"
    );
}

/// M2-F — the head-read route, which takes no `DecisionBasis`.
///
/// K1-C's argument: §5.3 puts `github-pull-request-head` in the gated set, so
/// both head snapshots are authorised through an assessment, and a repair
/// reaching only the basis leaves this open.
#[test]
fn m2f_the_head_read_path_is_closed_too() {
    let mut store = Store::default();
    let head = store.retain_under(
        &json!({
            "schemaVersion": 1, "sourceKind": "github-pull-request-head",
            "repository": "PhysShell/007", "pullRequest": "9001",
            "headSha": HEAD_SHA, "headRef": "claude/closure-classifier-provenance",
            "headRepoFullName": "PhysShell/007",
        }),
        &json!({
            "schemaVersion": 1, "sourceKind": "closure-retention-assessment",
            "redactionPolicyVersion": "1",
            "detector": {"id": SECRET, "version": "1", "configDigest": CONFIG},
            "representation": "decoded-source-field-values",
            "assessedFields": ["/head/ref", "/head/repo/full_name", "/head/sha", "/number"],
            "coverageComplete": true, "outcome": "RETAIN",
            "observedAt": "2026-08-05T09:03:00Z",
        }),
    );
    // BOTH reads resolve, and both agree with the expected SHA, so without the
    // rule under test this pair returns NotStale. An earlier draft of this
    // witness used a FAILED second read — which refuses on its own, so the
    // detector rule was never reached and the witness was green for the wrong
    // reason. That is M4's defect, and catching it here rather than in review
    // is the only reason this comment is shorter than that one.
    let before = store.put(&json!({
        "schemaVersion": 1, "sourceKind": "github-head-read-event",
        "role": "HEAD_BEFORE", "acquisition": "AVAILABLE",
        "snapshotDigest": head, "observedAt": "2026-08-05T09:03:00Z",
    }));
    let after = store.put(&json!({
        "schemaVersion": 1, "sourceKind": "github-head-read-event",
        "role": "HEAD_AFTER", "acquisition": "AVAILABLE",
        "snapshotDigest": head, "observedAt": "2026-08-05T09:03:01Z",
    }));
    let verdict = staleness(
        &Subject {
            repository: "PhysShell/007".to_owned(),
            pull_request: "9001".to_owned(),
            expected_sha: HEAD_SHA.to_owned(),
            expected_redaction_policy: "1".to_owned(),
            expected_detector: expected(),
        },
        &SubjectRead {
            before: HeadRead::Observed {
                event_digest: before,
            },
            after: HeadRead::Observed {
                event_digest: after,
            },
        },
        &store,
    );
    assert!(
        matches!(verdict, Staleness::CannotCheck { .. }),
        "M2-F: a head snapshot authorised by an assessment whose detector.id is a credential \
         was consumed, and both reads resolved and agreed — so without this rule the pair \
         reports NotStale. This entry point takes no DecisionBasis, so the expectation has to \
         travel with the Subject; a repair that reached only the basis would leave the whole \
         subject-read route open: got {verdict:?}"
    );
}

/// M2-G — FREEZE. §9.4 still states the closed disjunction and still says why.
#[test]
fn m2g_the_frozen_rule_is_still_the_frozen_rule() {
    assert!(
        REDACTION.contains("no field of an assessment is free text"),
        "§9.4 no longer says that no field of an assessment is free text. Every witness above \
         goes green if that sentence is softened, with no code change at all"
    );
    assert!(
        REDACTION.contains("A closed field cannot carry a secret")
            && REDACTION.contains("does not depend on the content inspected."),
        "§9.4 still forbids free text but no longer says WHY — and the reason is the rule: the \
         property is about the value SETS, so a member whose range is the caller's expectation \
         satisfies it and a member typed `Text` does not"
    );
}
