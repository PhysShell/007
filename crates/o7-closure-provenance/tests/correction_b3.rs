//! The relation-binding round's escape set, frozen before the fix.
//!
//! Five findings from the paired external round on `b125d83`, and they are one
//! law rather than five defects:
//!
//! ```text
//! A retained artifact may influence a decision only after both are
//! established:
//!
//!   1. artifact validity   bytes, digest, type, closed schema
//!   2. relation validity   the artifact's own fields establish the exact
//!                          subject, role, state, partition and relation
//!                          under which this decision consumes it.
//! ```
//!
//! The previous round closed clause 1 for the *bytes* — resolve, re-digest,
//! refuse a substitution — and then treated a correctly-resolved artifact as
//! though that settled what it was ABOUT. It does not. Resolving the right bytes
//! proves *this artifact exists and these are its bytes*, and nothing more. It
//! does not prove the artifact concerns this subject, has the role this decision
//! assigns it, is in a semantic state that supports this claim, or authorises
//! this other artifact.
//!
//! Stated as the single sentence the whole file is about: **an artifact was
//! being checked for what it IS and never for what it is ABOUT.**
//!
//! ```text
//! ARTIFACT VALIDITY — the assessment was checked member-by-member, not closed
//!   S8   an extra top-level member (§9.4's exfiltration channel)
//!   S9   an extra member inside `detector` (§9.5 — closed at EVERY level)
//!   S10  `findings` present while outcome is RETAIN (§9's iff)
//!   S11  `coverageComplete: false` with no `coverageFailureCode` (§5.4)
//!   S12  a finding carrying a member beyond field/findingId (§9.3)
//!
//! RELATION VALIDITY — does this assessment authorise THIS record
//!   S13  a BLOCK_SECRET assessment authorising a complete §8 projection (§6.2)
//!   S14  a reduced record whose own outcome disagrees with it (§9.6)
//!   S15  a reduced record retaining a field it never assessed (§7.1)
//!   S16  a reduced record retaining a field one of its findings names (§7.1)
//!   S17  a partition that omits a §5.3 always-field (§7.1 exhaustive)
//!   S18  a partition carrying a pointer outside §5.3 (nominated, not computed)
//!   S19  a reduced record whose locatorKind is not a gated kind
//!   S20  a finding naming a field outside the §5.3 set (§9's worked example)
//!   S21  a finding naming a field that is not in assessedFields (§9)
//!   S22  a RETAIN assessment that did not assess the whole §5.3 set (§5.2)
//!
//! RELATION VALIDITY — does this snapshot evidence THIS scan's claim
//!   S23  a COMPLETE scan evidenced by an INCOMPLETE enumeration (§13)
//!
//! RELATION VALIDITY — is this event the read this slot claims, about US
//!   S24  the before slot holding an event whose role is HEAD_AFTER (§8.1)
//!   S25  one event answering for both slots — two pointers, one read (§8.1)
//!   S26  head reads of a DIFFERENT pull request reporting NotStale
//!   S27  head reads of a different repository
//!
//! RELATION VALIDITY — is this artifact the KIND its role requires
//!   S28  expected_query_digest resolving to something that is not a query
//!        snapshot
//!
//! BOUNDARY — the rule must not refuse what the contract permits
//!   S29  a correctly computed BLOCK_SECRET partition is admitted
//!   S30  an absent present-only field is not a hole in the partition
//! ```
//!
//! WHY S26 FORCED AN API CHANGE. There is no way to check that a head read is
//! about this pull request while the only identity available comes out of the
//! events being checked. Two reads of some other subject agree with each other
//! perfectly. So `staleness` now takes a [`Subject`] the caller states, and the
//! retained artifacts are checked against it — rather than the target being
//! inferred from the same retained events that are under examination.

// Justification for the restriction-lint allowance, per AGENTS.md rule 4: the
// `expect` and `panic` sites below are this test's own assertions failing, or
// its own handling of JSON literals written in this file — `digest(..).expect`
// in `put`, the `panic!` in `refused`, and the `expect`s in the mutation
// helpers, each unreachable unless a literal a few lines above it is malformed.
// A malformed fixture must fail loudly rather than silently weaken a witness.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use o7_closure_canonical::digest;
use o7_closure_provenance::{
    admissibility, scan_verdict, staleness, Admissible, DecisionBasis, DecisionInput,
    FalsificationSurfaceScan, HeadRead, QueryBinding, RetainedEvidence, RetentionBinding,
    ScanCompleteness, ScanVerdict, Staleness, Subject, SubjectRead, Unresolved,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;

// ---- Store.

#[derive(Default)]
struct Store {
    records: BTreeMap<String, Value>,
    bindings: BTreeMap<String, RetentionBinding>,
}

impl Store {
    fn put(&mut self, object: &Value) -> String {
        let d = digest(object).expect("digest").as_str().to_owned();
        self.records.insert(d.clone(), object.clone());
        d
    }

    /// Retain `record` under an assessment the caller chose, and bind the two.
    ///
    /// Every witness below differs only in what that pair is, which is the point:
    /// the bytes always resolve and the digests always match, so nothing the
    /// previous round established can distinguish them.
    fn retain_under(&mut self, record: &Value, assessment: &Value) -> String {
        let assessment_digest = self.put(assessment);
        let record_digest = self.put(record);
        self.bindings.insert(
            record_digest.clone(),
            RetentionBinding {
                record_digest: record_digest.clone(),
                assessment_digest,
            },
        );
        record_digest
    }
}

impl RetainedEvidence for Store {
    fn resolve(&self, d: &str) -> Option<Value> {
        self.records.get(d).cloned()
    }
    fn binding_for(&self, record_digest: &str) -> Option<RetentionBinding> {
        self.bindings.get(record_digest).cloned()
    }
}

// ---- Fixtures, all of them conforming until a witness breaks exactly one thing.

/// The §5.3 always-set for `github-submitted-review`, in §5.5 order.
const REVIEW_REQUIRED: [&str; 9] = [
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

/// A complete §8.3 projection — the artifact a `RETAIN` outcome produces.
fn review() -> Value {
    json!({
        "schemaVersion": 1,
        "sourceKind": "github-submitted-review",
        "stableId": "9000000901",
        "user": {"id": "9000000901", "login": "synthetic-external-reviewer", "type": "User"},
        "authorAssociation": "NONE",
        "state": "CHANGES_REQUESTED",
        "body": "still unreachable\n",
        "submittedAt": "2026-08-05T09:02:47Z",
        "commitId": "1f2e3d4c5b6a798807162534435261708f9e0d1c",
    })
}

/// A conforming §9 assessment. `assessedFields` are pointers into the DECODED
/// source, because §9 fixes `representation: decoded-source-field-values` and
/// §5.3 says in as many words that its pointers are GitHub's field names rather
/// than the canonical projection's.
fn assessment(outcome: &str, assessed: &[&str], findings: &[(&str, &str)]) -> Value {
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
        "coverageComplete": assessed.len() == REVIEW_REQUIRED.len(),
        "outcome": outcome,
        "observedAt": "2026-08-05T09:03:00Z",
    });
    if !findings.is_empty() {
        let list: Vec<Value> = findings
            .iter()
            .map(|(field, id)| json!({"field": field, "findingId": id}))
            .collect();
        object
            .as_object_mut()
            .expect("assessment is an object")
            .insert("findings".to_owned(), Value::Array(list));
    }
    object
}

/// A `RETAIN` assessment that assessed the whole §5.3 set and flagged nothing.
fn clean_assessment() -> Value {
    assessment("RETAIN", &REVIEW_REQUIRED, &[])
}

/// A reduced record whose partition is the §7.1 computation over `assessment`,
/// rather than a set somebody nominated.
fn reduced(outcome: &str, retained: &[&str], blocked: &[&str]) -> Value {
    let mut fields = serde_json::Map::new();
    for pointer in retained {
        fields.insert((*pointer).to_owned(), json!("value-as-the-projection-carries-it"));
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
        "redactionPolicyVersion": "1",
        "outcome": outcome,
        "coverageComplete": true,
        "retainedFields": Value::Object(fields),
        "blockedFields": blocked,
    })
}

/// The §7.1 computation for "`/body` carries a finding, everything assessed".
fn body_blocked_partition() -> (Vec<&'static str>, Vec<&'static str>) {
    let blocked = vec!["/body"];
    let retained = REVIEW_REQUIRED
        .iter()
        .copied()
        .filter(|p| !blocked.contains(p))
        .collect();
    (retained, blocked)
}

fn basis(record_digest: &str, pointer: &str) -> DecisionBasis {
    DecisionBasis {
        observation_id: "review/external".to_owned(),
        inputs: vec![DecisionInput {
            source_digest: record_digest.to_owned(),
            pointer: pointer.to_owned(),
        }],
        derived: Vec::new(),
        expected_query_digest: None,
        bindings: Vec::new(),
    }
}

#[track_caller]
fn refused(outcome: &Admissible) -> &[Unresolved] {
    match outcome {
        Admissible::CannotCheck { why } => why,
        Admissible::Yes { .. } => panic!(
            "admitted. The artifact resolved and re-digested correctly, and was then consumed \
             without establishing what it is ABOUT — which is the law this round exists to state"
        ),
    }
}

// ---- Artifact validity: the assessment was checked member-by-member.

/// §9.4 is explicit that the closed schema is the security argument rather than
/// a tidiness rule. A checker that verifies the members it thought of leaves a
/// producer free to add one holding the very content the gate refused.
#[test]
fn s8_an_assessment_with_an_extra_top_level_member_is_not_conforming() {
    let mut store = Store::default();
    let mut a = clean_assessment();
    a.as_object_mut()
        .expect("assessment is an object")
        .insert("debug".to_owned(), json!("ghp_the_secret_the_gate_refused"));
    let d = store.retain_under(&review(), &a);
    let why = refused(&admissibility(&basis(&d, "/body"), &store)).to_vec();
    assert!(
        why.iter()
            .any(|u| matches!(u, Unresolved::MalformedAssessment { .. })),
        "an assessment carrying a member outside §9's closed key set is non-conformant, and \
         §9.4 names that member as the exfiltration channel: got {why:?}"
    );
}

/// §9.5: closed means closed at every level. A checker that closes the top and
/// trusts the nested objects has moved the hole one step down.
#[test]
fn s9_an_assessment_with_an_extra_member_inside_detector_is_not_conforming() {
    let mut store = Store::default();
    let mut a = clean_assessment();
    a.pointer_mut("/detector")
        .and_then(Value::as_object_mut)
        .expect("detector is an object")
        .insert("note".to_owned(), json!("ghp_the_secret_the_gate_refused"));
    let d = store.retain_under(&review(), &a);
    let why = refused(&admissibility(&basis(&d, "/body"), &store)).to_vec();
    assert!(
        why.iter()
            .any(|u| matches!(u, Unresolved::MalformedAssessment { .. })),
        "got {why:?}"
    );
}

/// The conditionals are **iff**, not "at least". `findings` on a RETAIN outcome
/// is not a harmless extra field: it is a record whose own two halves disagree.
#[test]
fn s10_findings_present_on_a_retain_outcome_is_not_conforming() {
    let mut store = Store::default();
    let a = assessment("RETAIN", &REVIEW_REQUIRED, &[("/body", "rule-aws-key")]);
    let d = store.retain_under(&review(), &a);
    let why = refused(&admissibility(&basis(&d, "/body"), &store)).to_vec();
    assert!(
        why.iter()
            .any(|u| matches!(u, Unresolved::MalformedAssessment { .. })),
        "findings is present IFF outcome is BLOCK_SECRET; a RETAIN record carrying findings \
         claims both that something was found and that nothing was: got {why:?}"
    );
}

/// §5.4: whenever coverage is incomplete the record says why, independently of
/// the outcome. Incomplete coverage with no reason is the partial assessment
/// that cannot be told apart from a complete one after the fact.
#[test]
fn s11_incomplete_coverage_without_a_failure_code_is_not_conforming() {
    let mut store = Store::default();
    let a = assessment("CANNOT_ASSESS", &REVIEW_REQUIRED[..8], &[]);
    assert_eq!(
        a.pointer("/coverageComplete"),
        Some(&Value::Bool(false)),
        "fixture: this witness needs incomplete coverage"
    );
    assert!(
        a.pointer("/coverageFailureCode").is_none(),
        "fixture: this witness needs the code to be the ONLY thing missing"
    );
    let (retained, blocked) = (&REVIEW_REQUIRED[..8], &REVIEW_REQUIRED[8..]);
    let d = store.retain_under(&reduced("CANNOT_ASSESS", retained, blocked), &a);
    let why = refused(&admissibility(&basis(&d, "/body"), &store)).to_vec();
    assert!(
        why.iter()
            .any(|u| matches!(u, Unresolved::MalformedAssessment { .. })),
        "got {why:?}"
    );
}

/// §9.3 forbids a finding from carrying the matched substring, an excerpt, a
/// prefix or suffix, a length, a character count, or a digest of the matched
/// bytes. A closed finding shape is how that prohibition is enforced instead of
/// trusted — enumerating the forbidden members would be a blocklist, and the
/// next one is never on it.
#[test]
fn s12_a_finding_carrying_more_than_field_and_finding_id_is_not_conforming() {
    let mut store = Store::default();
    let (retained, blocked) = body_blocked_partition();
    let mut a = assessment("BLOCK_SECRET", &REVIEW_REQUIRED, &[("/body", "rule-aws-key")]);
    a.pointer_mut("/findings/0")
        .and_then(Value::as_object_mut)
        .expect("the first finding is an object")
        .insert("matchedLength".to_owned(), json!(40));
    let d = store.retain_under(&reduced("BLOCK_SECRET", &retained, &blocked), &a);
    let why = refused(&admissibility(&basis(&d, "/commit_id"), &store)).to_vec();
    assert!(
        why.iter()
            .any(|u| matches!(u, Unresolved::MalformedAssessment { .. })),
        "got {why:?}"
    );
}

// ---- Relation validity: does this assessment authorise THIS record.

/// §6.2: under `BLOCK_SECRET`, a normal source snapshot of any §8 sourceKind is
/// FORBIDDEN — it would require the bytes, which is the thing being refused.
///
/// This is the escape the previous round left widest open. The assessment is
/// retained, correctly digested and perfectly conformant. It simply says the
/// opposite of what keeping this record requires, and nothing compared the two.
#[test]
fn s13_a_block_secret_assessment_does_not_authorise_a_complete_projection() {
    let mut store = Store::default();
    let a = assessment("BLOCK_SECRET", &REVIEW_REQUIRED, &[("/body", "rule-aws-key")]);
    let d = store.retain_under(&review(), &a);
    let why = refused(&admissibility(&basis(&d, "/body"), &store)).to_vec();
    assert!(
        !why.is_empty(),
        "a conforming assessment refusing this content is not a permission to keep it: got {why:?}"
    );
}

/// §9.6: the retained assessment is the authority on its own outcome, and
/// anything else is an expectation checked against it, never a substitute.
///
/// Without this the self-certification returns one layer out — a record reading
/// `CANNOT_ASSESS` bound to an assessment reading `BLOCK_SECRET`, with every
/// structural check still passing and the two artifacts disagreeing about what
/// was done.
#[test]
fn s14_a_reduced_record_whose_outcome_contradicts_its_assessment_is_refused() {
    let mut store = Store::default();
    let (retained, blocked) = body_blocked_partition();
    let a = assessment("BLOCK_SECRET", &REVIEW_REQUIRED, &[("/body", "rule-aws-key")]);
    let d = store.retain_under(&reduced("CANNOT_ASSESS", &retained, &blocked), &a);
    let why = refused(&admissibility(&basis(&d, "/commit_id"), &store)).to_vec();
    assert!(!why.is_empty(), "got {why:?}");
}

/// §7.1, verbatim: *a field the detector never successfully assessed is blocked.
/// Always* — an unassessed field is not "probably a timestamp", it is unexamined.
#[test]
fn s15_a_reduced_record_retaining_an_unassessed_field_is_refused() {
    let mut store = Store::default();
    // The detector never reached /commit_id, so §7.1 puts it in blockedFields.
    // This record retains it anyway.
    let assessed: Vec<&str> = REVIEW_REQUIRED
        .iter()
        .copied()
        .filter(|p| *p != "/commit_id")
        .collect();
    let a = assessment("BLOCK_SECRET", &assessed, &[("/body", "rule-aws-key")]);
    let (retained, blocked) = body_blocked_partition();
    let d = store.retain_under(&reduced("BLOCK_SECRET", &retained, &blocked), &a);
    let why = refused(&admissibility(&basis(&d, "/commit_id"), &store)).to_vec();
    assert!(
        !why.is_empty(),
        "an unexamined field retained as though it had passed: got {why:?}"
    );
}

/// §7.1, verbatim: *a field named by a finding is blocked. Always.*
#[test]
fn s16_a_reduced_record_retaining_a_flagged_field_is_refused() {
    let mut store = Store::default();
    let a = assessment("BLOCK_SECRET", &REVIEW_REQUIRED, &[("/body", "rule-aws-key")]);
    // The partition keeps the very field the finding names.
    let retained: Vec<&str> = REVIEW_REQUIRED.to_vec();
    let d = store.retain_under(&reduced("BLOCK_SECRET", &retained, &[]), &a);
    let why = refused(&admissibility(&basis(&d, "/body"), &store)).to_vec();
    assert!(
        !why.is_empty(),
        "the detector found a secret in /body and the record kept /body: got {why:?}"
    );
}

/// §7.1: `retainedFields` and `blockedFields` **exhaustively partition** the
/// required set — every field appears in exactly one, and nothing in neither.
///
/// A record that simply omits a field accounts for it nowhere, and an incomplete
/// partition reads as evidence that nothing was blocked.
#[test]
fn s17_a_partition_that_omits_a_required_field_is_refused() {
    let mut store = Store::default();
    let a = assessment("BLOCK_SECRET", &REVIEW_REQUIRED, &[("/body", "rule-aws-key")]);
    let retained: Vec<&str> = REVIEW_REQUIRED
        .iter()
        .copied()
        .filter(|p| *p != "/body" && *p != "/user/type")
        .collect();
    let d = store.retain_under(&reduced("BLOCK_SECRET", &retained, &["/body"]), &a);
    let why = refused(&admissibility(&basis(&d, "/commit_id"), &store)).to_vec();
    assert!(
        !why.is_empty(),
        "/user/type appears in neither list, so the record does not account for it: got {why:?}"
    );
}

/// §5.2: the denominator is normative, not declared. A consumer MUST compute the
/// required set from §5.3 rather than accept the one the record believed in.
#[test]
fn s18_a_partition_carrying_a_pointer_outside_the_required_set_is_refused() {
    let mut store = Store::default();
    let a = assessment("BLOCK_SECRET", &REVIEW_REQUIRED, &[("/body", "rule-aws-key")]);
    let (mut retained, blocked) = body_blocked_partition();
    retained.push("/made_up_field");
    let d = store.retain_under(&reduced("BLOCK_SECRET", &retained, &blocked), &a);
    let why = refused(&admissibility(&basis(&d, "/commit_id"), &store)).to_vec();
    assert!(!why.is_empty(), "got {why:?}");
}

/// §7: `locatorKind` is *the provenance V1 kind that was refused*. A kind with
/// no §5.3 entry has no required set, so its partition is vacuously exhaustive
/// and every rule above is satisfied by a record that assessed nothing.
#[test]
fn s19_a_reduced_record_with_an_ungated_locator_kind_is_refused() {
    let mut store = Store::default();
    let a = assessment("BLOCK_SECRET", &REVIEW_REQUIRED, &[("/body", "rule-aws-key")]);
    let (retained, blocked) = body_blocked_partition();
    let mut record = reduced("BLOCK_SECRET", &retained, &blocked);
    record
        .as_object_mut()
        .expect("record is an object")
        .insert("locatorKind".to_owned(), json!("github-query-snapshot"));
    let d = store.retain_under(&record, &a);
    let why = refused(&admissibility(&basis(&d, "/commit_id"), &store)).to_vec();
    assert!(
        !why.is_empty(),
        "§5.3 places github-query-snapshot outside the gate, so it cannot be the kind a gate \
         outcome refused: got {why:?}"
    );
}

/// §9's worked exfiltration example, reproduced exactly:
///
/// ```text
/// finding.field = /made-up-field    not in the required set, so blocked nothing
/// /body          assessed, unflagged, therefore RETAINED
/// outcome        BLOCK_SECRET, because findings is non-empty
/// ```
///
/// Every other rule holds in that record. The partition is exhaustive, the
/// locator matches, coverage is honest, the rule id is configured. The secret is
/// retained anyway.
#[test]
fn s20_a_finding_naming_a_field_outside_the_required_set_is_refused() {
    let mut store = Store::default();
    let a = assessment(
        "BLOCK_SECRET",
        &REVIEW_REQUIRED,
        &[("/made-up-field", "rule-aws-key")],
    );
    let retained: Vec<&str> = REVIEW_REQUIRED.to_vec();
    let d = store.retain_under(&reduced("BLOCK_SECRET", &retained, &[]), &a);
    let why = refused(&admissibility(&basis(&d, "/body"), &store)).to_vec();
    assert!(
        !why.is_empty(),
        "a finding that could not have been produced blocks nothing while reporting \
         BLOCK_SECRET: got {why:?}"
    );
}

/// §9: every field a finding names MUST be in `assessedFields`. A detector can
/// only find something in a field it actually looked at.
#[test]
fn s21_a_finding_naming_an_unassessed_field_is_refused() {
    let mut store = Store::default();
    let assessed: Vec<&str> = REVIEW_REQUIRED
        .iter()
        .copied()
        .filter(|p| *p != "/body")
        .collect();
    let a = assessment("BLOCK_SECRET", &assessed, &[("/body", "rule-aws-key")]);
    let (retained, blocked) = body_blocked_partition();
    let d = store.retain_under(&reduced("BLOCK_SECRET", &retained, &blocked), &a);
    let why = refused(&admissibility(&basis(&d, "/commit_id"), &store)).to_vec();
    assert!(!why.is_empty(), "got {why:?}");
}

/// §5.2, the self-certification it names in as many words:
///
/// ```text
/// record:   "I assessed everything"
/// checker:  "well, if you say so"
/// ```
///
/// `coverageComplete: true` is the record's own claim. The check is whether the
/// listed fields actually cover the §5.3 set.
#[test]
fn s22_a_retain_assessment_that_did_not_assess_the_whole_required_set_is_refused() {
    let mut store = Store::default();
    let mut a = assessment("RETAIN", &REVIEW_REQUIRED[..8], &[]);
    a.as_object_mut()
        .expect("assessment is an object")
        .insert("coverageComplete".to_owned(), json!(true));
    let d = store.retain_under(&review(), &a);
    let why = refused(&admissibility(&basis(&d, "/body"), &store)).to_vec();
    assert!(
        why.iter()
            .any(|u| matches!(u, Unresolved::MalformedAssessment { .. })),
        "/user/type was never assessed and the record says coverage is complete: got {why:?}"
    );
}

// ---- Relation validity: does this snapshot evidence THIS scan's claim.

fn query_snapshot(enumeration: &str) -> Value {
    let mut object = json!({
        "schemaVersion": 1,
        "sourceKind": "github-query-snapshot",
        "surface": "pull-request-review-comments",
        "requiredObservationId": "falsification/scan",
        "binding": {"repository": "PhysShell/007", "pullRequest": "9001"},
        "pagination": {
            "perPage": 100,
            "pagesRequested": ["1", "2"],
            "pagesObtained": ["1"],
            "nextPagePresent": true,
        },
        "enumeration": enumeration,
        "matcher": {
            "id": "review-by-expected-author-login",
            "version": "1",
            "parameters": {"expectedAuthorLogin": "synthetic-external-reviewer"},
        },
        "allReturnedSnapshotDigests": [],
        "matchedSnapshotDigests": [],
    });
    if enumeration == "INCOMPLETE" {
        object
            .as_object_mut()
            .expect("snapshot is an object")
            .insert(
                "incompleteReason".to_owned(),
                json!("page 2 fetch returned HTTP 502; not retried"),
            );
    }
    object
}

/// The scan's `Complete` is the CALLER's claim about how far it got. §13 puts
/// the enumeration state in the snapshot and says the rule turns on that value.
///
/// So a scan can declare itself complete, name a real snapshot of exactly the
/// right surface and the right pull request, resolve it, re-digest it — and the
/// snapshot itself says the enumeration was cut short. Every check the previous
/// round added passes. Zero claims comes back as a fact about the surface.
#[test]
fn s23_a_complete_scan_evidenced_by_an_incomplete_enumeration_is_cannot_check() {
    let mut store = Store::default();
    let evidence = store.put(&query_snapshot("INCOMPLETE"));
    let verdict = scan_verdict(
        &FalsificationSurfaceScan {
            surface: "pull-request-review-comments".to_owned(),
            binding: QueryBinding {
                repository: "PhysShell/007".to_owned(),
                pull_request: "9001".to_owned(),
            },
            completeness: ScanCompleteness::Complete,
            snapshot_digest: evidence,
        },
        0,
        &store,
    );
    assert!(
        matches!(verdict, ScanVerdict::CannotCheck { .. }),
        "the scan says COMPLETE and its own evidence says INCOMPLETE; the artifact is the \
         authority on its enumeration state, not the caller: got {verdict:?}"
    );
}

// ---- Relation validity: is this event the read this slot claims, about US.

fn head_snapshot(repository: &str, pull_request: &str, head_sha: &str) -> Value {
    json!({
        "schemaVersion": 1,
        "sourceKind": "github-pull-request-head",
        "repository": repository,
        "pullRequest": pull_request,
        "headSha": head_sha,
        "headRef": "claude/example",
        "headRepoFullName": repository,
    })
}

fn head_read(store: &mut Store, role: &str, snapshot: &Value) -> HeadRead {
    let snapshot_digest = store.put(snapshot);
    let event = store.put(&json!({
        "schemaVersion": 1,
        "sourceKind": "github-head-read-event",
        "role": role,
        "acquisition": "AVAILABLE",
        "snapshotDigest": snapshot_digest,
        "observedAt": "2026-08-05T09:00:00Z",
    }));
    HeadRead::Observed {
        event_digest: event,
    }
}

fn subject(repository: &str, pull_request: &str, expected_sha: &str) -> Subject {
    Subject {
        repository: repository.to_owned(),
        pull_request: pull_request.to_owned(),
        expected_sha: expected_sha.to_owned(),
    }
}

/// §8.1 gives every event a `role` of `HEAD_BEFORE` or `HEAD_AFTER`. A slot that
/// never reads it will accept the after-read as the before-read, and the pair
/// then witnesses nothing about an interval.
#[test]
fn s24_an_event_in_the_wrong_slot_is_not_that_slot_s_read() {
    let mut store = Store::default();
    let snapshot = head_snapshot("PhysShell/007", "9001", "aaaa");
    let read = SubjectRead {
        before: head_read(&mut store, "HEAD_AFTER", &snapshot),
        after: head_read(&mut store, "HEAD_AFTER", &snapshot),
    };
    let verdict = staleness(&subject("PhysShell/007", "9001", "aaaa"), &read, &store);
    assert!(
        matches!(verdict, Staleness::CannotCheck { .. }),
        "an event tagged HEAD_AFTER is not the before-read, and a pair with no before-read \
         brackets no interval: got {verdict:?}"
    );
}

/// §8.1, verbatim: *two head reads are two acquisition events, not two pointers.
/// Retaining one snapshot and referring to it twice does not record that two
/// reads were performed.*
///
/// One event answering for both slots is that sentence one level up: not one
/// snapshot referenced twice, but one EVENT referenced twice. It resolves, it
/// re-digests, its role can even be right for one of the two slots, and the two
/// SHAs agree because they are the same SHA.
#[test]
fn s25_one_event_cannot_be_both_reads() {
    let mut store = Store::default();
    let snapshot = head_snapshot("PhysShell/007", "9001", "aaaa");
    let once = head_read(&mut store, "HEAD_BEFORE", &snapshot);
    let read = SubjectRead {
        before: once.clone(),
        after: once,
    };
    let verdict = staleness(&subject("PhysShell/007", "9001", "aaaa"), &read, &store);
    assert!(
        matches!(verdict, Staleness::CannotCheck { .. }),
        "one read reported twice is not two reads: got {verdict:?}"
    );
}

/// The escape that forced the API change. Both events are real, both resolve,
/// both re-digest, both roles are right, and both are reads of a DIFFERENT pull
/// request. They agree with each other perfectly.
#[test]
fn s26_head_reads_of_another_pull_request_do_not_witness_this_one() {
    let mut store = Store::default();
    let elsewhere = head_snapshot("PhysShell/007", "424242", "aaaa");
    let read = SubjectRead {
        before: head_read(&mut store, "HEAD_BEFORE", &elsewhere),
        after: head_read(&mut store, "HEAD_AFTER", &elsewhere),
    };
    let verdict = staleness(&subject("PhysShell/007", "9001", "aaaa"), &read, &store);
    assert!(
        matches!(verdict, Staleness::CannotCheck { .. }),
        "two consistent reads of another subject say nothing about this one: got {verdict:?}"
    );
}

/// The same escape through the other half of the identity.
#[test]
fn s27_head_reads_of_another_repository_do_not_witness_this_one() {
    let mut store = Store::default();
    let elsewhere = head_snapshot("PhysShell/somewhere-else", "9001", "aaaa");
    let read = SubjectRead {
        before: head_read(&mut store, "HEAD_BEFORE", &elsewhere),
        after: head_read(&mut store, "HEAD_AFTER", &elsewhere),
    };
    let verdict = staleness(&subject("PhysShell/007", "9001", "aaaa"), &read, &store);
    assert!(
        matches!(verdict, Staleness::CannotCheck { .. }),
        "got {verdict:?}"
    );
}

// ---- Relation validity: is this artifact the KIND its role requires.

/// `expected_query_digest` is the digest replay is checked against for an
/// absence claim, and §13 is explicit that only a query snapshot carries the
/// enumeration and matcher an absence rests on.
///
/// The basis names it, the store resolves it, the digest matches — and it is a
/// review. The record is consumed in a role its own `sourceKind` does not
/// support, which is the relation clause with `role` in place of `subject`.
#[test]
fn s28_an_expected_query_digest_that_is_not_a_query_snapshot_is_refused() {
    let mut store = Store::default();
    let d = store.retain_under(&review(), &clean_assessment());
    let mut b = basis(&d, "/body");
    b.expected_query_digest = Some(d.clone());
    let why = refused(&admissibility(&b, &store)).to_vec();
    assert!(
        !why.is_empty(),
        "a submitted review standing in for the query snapshot an absence claim rests on: \
         got {why:?}"
    );
}

// ---- Boundary. The rule must not refuse what the contract permits.

/// The whole point of §7: the fields that individually passed MAY still be
/// retained. A rule that refused this would have made the reduced record
/// unusable and quietly reintroduced the all-or-nothing gate §7 exists to
/// replace.
#[test]
fn s29_a_correctly_computed_partition_is_admitted() {
    let mut store = Store::default();
    let (retained, blocked) = body_blocked_partition();
    let a = assessment("BLOCK_SECRET", &REVIEW_REQUIRED, &[("/body", "rule-aws-key")]);
    let d = store.retain_under(&reduced("BLOCK_SECRET", &retained, &blocked), &a);
    let outcome = admissibility(&basis(&d, "/commit_id"), &store);
    assert!(
        matches!(outcome, Admissible::Yes { .. }),
        "the §7.1 computation over this assessment is exactly this partition: got {outcome:?}"
    );
}

/// §5.3: a present-only field joins the required set exactly when it is present
/// in the decoded source. Absent means nothing to assess.
///
/// A consumer holds the record and not the decoded source, so it cannot tell an
/// absent present-only field from a dropped one — and must therefore not treat
/// its absence as a hole. Refusing here would refuse every conformant check-run
/// record that happened to have no `conclusion` yet.
#[test]
fn s30_an_absent_present_only_field_is_not_a_hole_in_the_partition() {
    let mut store = Store::default();
    let always = ["/head_sha", "/id", "/name", "/status"];
    let a = json!({
        "schemaVersion": 1,
        "sourceKind": "closure-retention-assessment",
        "redactionPolicyVersion": "1",
        "detector": {
            "id": "synthetic-detector",
            "version": "1",
            "configDigest": "sha256:1111111111111111111111111111111111111111111111111111111111111111",
        },
        "representation": "decoded-source-field-values",
        "assessedFields": always,
        "coverageComplete": true,
        "outcome": "BLOCK_SECRET",
        "findings": [{"field": "/name", "findingId": "rule-aws-key"}],
        "observedAt": "2026-08-05T09:03:00Z",
    });
    let record = json!({
        "schemaVersion": 1,
        "sourceKind": "github-reduced-source-record",
        "locatorKind": "github-actions-check",
        "locator": {
            "repository": "PhysShell/007",
            "pullRequest": "9001",
            "stableId": "9100000201",
        },
        "redactionPolicyVersion": "1",
        "outcome": "BLOCK_SECRET",
        "coverageComplete": true,
        "retainedFields": {
            "/head_sha": "1f2e3d4c5b6a798807162534435261708f9e0d1c",
            "/id": "9100000201",
            "/status": "completed",
        },
        "blockedFields": ["/name"],
    });
    let d = store.retain_under(&record, &a);
    let outcome = admissibility(&basis(&d, "/head_sha"), &store);
    assert!(
        matches!(outcome, Admissible::Yes { .. }),
        "/conclusion, /started_at and /completed_at are present-only and this check has none \
         of them: got {outcome:?}"
    );
}
