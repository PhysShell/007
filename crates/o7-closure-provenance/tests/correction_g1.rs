//! RED-DERIV-SLOT — G1. A derivation's source slot has a KIND, and nothing
//! checks it.
//!
//! THE CLAIM, in one sentence:
//!
//! > `review-carries-finding/1` reads a SUBMITTED REVIEW in slot 0 and a REVIEW
//! > COMMENT in slot 1, and a fact citing anything else in those slots is a
//! > recomputation of a different rule reported under this one's name.
//!
//! WHAT THE REGISTRY DECLARES TODAY, and it is only half the slot:
//!
//! ```text
//! sources: &[
//!     &[DerivationInput { canonical: "/stableId",            decoded: "/id" }],
//!     &[DerivationInput { canonical: "/pullRequestReviewId", decoded: "/pull_request_review_id" }],
//! ]
//! ```
//!
//! Field names, and no kind. `check_derived` resolves each cited source through
//! `ConsumedAs::GatedSource` — the same role for every slot — and then reads
//! those pointers out of whatever came back.
//!
//! THE ANTI-PATTERN, named: **a guard checks proxy P, reports property Q, and P
//! is not Q.** Here P is "the slot's field is readable in this artifact" and Q
//! is "this artifact is the surface the slot takes". The two come apart in both
//! directions, and this file holds one witness for each direction.
//!
//! WHERE P AND Q COME APART — measured against the current five §8 schemas
//! rather than argued:
//!
//! ```text
//! slot 0, /stableId  (§8) and /id (§5.3)
//!     carried by github-submitted-review   <- the kind the slot takes
//!                github-issue-comment      <- G1-A  ADMITTED TODAY
//!                github-actions-check      <- G1-B  ADMITTED TODAY
//!                github-review-comment
//!     reduced records under those locatorKinds likewise  <- G1-C  ADMITTED TODAY
//!
//! slot 1, /pullRequestReviewId (§8) and /pull_request_review_id (§5.3)
//!     carried by github-review-comment ONLY               <- G1-F
//! ```
//!
//! SLOT 1 IS NOT SAFE, IT IS LUCKY, and G1-F is in this file to say so before
//! somebody reads the table above as an argument that only slot 0 needs fixing.
//! Two accidents currently bound it: no other §8 projection happens to declare
//! `pullRequestReviewId`, and §5.2's normative denominator happens to keep a
//! reduced record of another `locatorKind` from carrying
//! `/pull_request_review_id` in its partition. Neither is a statement about
//! slots. Both are properties of a schema table that a sixth surface changes.
//!
//! And the refusal slot 1 produces today is worse than absent — it is wrong.
//! A well-formed reduced ISSUE COMMENT in slot 1 is refused as:
//!
//! ```text
//! PointerBlocked { pointer: "/pull_request_review_id" }
//! DerivationInputUnavailable { .. }
//! ```
//!
//! which says the REDACTION GATE removed a field this decision needed. It did
//! not. Nothing was blocked, nothing was lost, and the operator who follows that
//! refusal goes to audit a redaction policy that is working correctly. The
//! citation is malformed; that is a different fact with a different remedy.
//!
//! WHY A COMPLETE PROJECTION AND A REDUCED RECORD ARE CHECKED THROUGH DIFFERENT
//! MEMBERS. A complete projection declares its surface in `/sourceKind`. Every
//! reduced record declares `sourceKind: "github-reduced-source-record"` — the
//! surface it was reduced FROM is in `/locatorKind` (§7.3). One `kind` field in
//! the registry can therefore serve both only if the two vocabularies are the
//! same set of names. They are today, and G1-G holds them to it, because a
//! single field standing for two vocabularies that have drifted apart is this
//! file's own anti-pattern reappearing inside the fix for it.
//!
//! WHAT THIS FILE DOES NOT CLAIM. Not that reduced records should be refused in
//! derivation slots — redaction §8 requires the opposite, and G1-E is the
//! boundary that keeps a fix from buying G1-C by forbidding the representation.
//! Not that the derivation's own arity or implementation binding is wrong; both
//! are checked already and are not this finding.
//!
//! ```text
//! G1-A  complete github-issue-comment in slot 0            RED
//! G1-B  complete github-actions-check in slot 0            RED
//! G1-C  reduced record, locatorKind issue-comment, slot 0  RED
//! G1-D  the correct complete pair still admits             BOUNDARY
//! G1-E  the correct REDUCED pair still admits              BOUNDARY
//! G1-F  reduced issue comment in slot 1: refused for the
//!       SLOT, not for a redaction the gate never made      RED
//! G1-G  §8 sourceKind and §7.3 locatorKind are one
//!       vocabulary — the premise of a single `kind` field  INVARIANT
//! ```

// Justification for the restriction-lint allowance, per AGENTS.md rule 4: the
// `expect` sites below are this file's own handling of JSON literals written in
// it, unreachable unless a specimen a few lines above is malformed.
#![allow(clippy::expect_used)]

use o7_closure_canonical::digest;
use o7_closure_matcher::SOURCE_SCHEMAS;
use o7_closure_provenance::artifact::LOCATOR_SHAPES;
use o7_closure_provenance::{
    relations_checked, Admissible, DecisionBasis, DerivedFact, RetainedEvidence, Unresolved,
};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use std::collections::BTreeSet;

/// This file's witnesses ask about ONE relation each, over a basis built to
/// isolate it. That is not a §17 decision basis and was never meant to be, so
/// they ask `relation_refusals` — "did anything this basis NAMED fail?" —
/// rather than `admissibility`, which additionally requires §17's minimum for
/// the decision being made. Shaped back into `Admissible` so the assertions
/// below read as they always have.
///
/// The distinction is not cosmetic. Before `admissibility` took a profile,
/// these witnesses' `admits` assertions claimed that a basis carrying one
/// pointer was an admissible DECISION. That was only ever true because nothing
/// checked completeness. `correction_g2.rs` carries the decision-level claim.
fn relations<E: RetainedEvidence>(basis: &DecisionBasis, store: &E) -> Admissible {
    match relations_checked(basis, store) {
        Ok(values) => Admissible::Yes { values },
        Err(why) => Admissible::CannotCheck { why },
    }
}

const REVIEW_ID: &str = "9000000901";
const COMMENT_ID: &str = "9000000202";

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
const ISSUE_REQ: [&str; 8] = [
    "/author_association",
    "/body",
    "/created_at",
    "/id",
    "/updated_at",
    "/user/id",
    "/user/login",
    "/user/type",
];
const CHECK_REQ: [&str; 4] = ["/head_sha", "/id", "/name", "/status"];

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

/// The assessment a reduced record needs: §7 gives a reduced record only
/// `BLOCK_SECRET` and `CANNOT_ASSESS`, so a record with nothing blocked is not a
/// shape the contract defines. `/body` is the blocked field throughout.
fn block_body(assessed: &[&str]) -> Value {
    let mut a = retain_all(assessed);
    let o = a.as_object_mut().expect("the assessment is an object");
    o.insert("outcome".to_owned(), json!("BLOCK_SECRET"));
    o.insert(
        "findings".to_owned(),
        json!([{"field": "/body", "findingId": "rule-aws-key"}]),
    );
    a
}

// ---- The four complete projections these witnesses put in slot 0.

fn complete_review() -> Value {
    json!({
        "schemaVersion": 1, "sourceKind": "github-submitted-review",
        "stableId": REVIEW_ID,
        "user": {"id": REVIEW_ID, "login": "synthetic-external-reviewer", "type": "User"},
        "authorAssociation": "NONE", "state": "CHANGES_REQUESTED",
        "body": "still unreachable\n", "submittedAt": "2026-08-05T09:02:47Z",
        "commitId": "1f2e3d4c5b6a798807162534435261708f9e0d1c",
    })
}

fn complete_comment() -> Value {
    json!({
        "schemaVersion": 1, "sourceKind": "github-review-comment",
        "stableId": COMMENT_ID, "pullRequestReviewId": REVIEW_ID,
        "user": {"id": REVIEW_ID, "login": "synthetic-external-reviewer", "type": "User"},
        "authorAssociation": "NONE", "body": "here specifically\n",
        "commitId": "1f2e3d4c5b6a798807162534435261708f9e0d1c",
        "originalCommitId": "1f2e3d4c5b6a798807162534435261708f9e0d1c",
        "path": "crates/o7-closure-provenance/src/lib.rs",
        "createdAt": "2026-08-05T09:02:47Z", "updatedAt": "2026-08-05T09:02:47Z",
    })
}

/// An §8.5 issue comment. Not a review: it was never submitted, carries no
/// `state`, and §5.3 gives it a different required set. Its `stableId` is the
/// review's id, which is all slot 0 currently looks at.
fn issue_comment_wearing_the_review_id() -> Value {
    json!({
        "schemaVersion": 1, "sourceKind": "github-issue-comment",
        "stableId": REVIEW_ID,
        "user": {"id": REVIEW_ID, "login": "anybody-at-all", "type": "User"},
        "authorAssociation": "NONE", "body": "a drive-by comment\n",
        "createdAt": "2026-08-05T09:02:47Z", "updatedAt": "2026-08-05T09:02:47Z",
    })
}

/// An §8.3 CI check. The distance from "a human submitted a review" is the
/// point: nothing about a check run is a review, and slot 0 takes it today.
fn actions_check_wearing_the_review_id() -> Value {
    json!({
        "schemaVersion": 1, "sourceKind": "github-actions-check",
        "stableId": REVIEW_ID, "name": "some-ci-job",
        "headSha": "1f2e3d4c5b6a798807162534435261708f9e0d1c",
        "status": "completed",
    })
}

/// A §7 reduced record under any `locatorKind`, `/body` blocked and every other
/// §5.3 required field retained.
fn reduced(locator_kind: &str, required: &[&str], stable_id: &str) -> Value {
    let mut retained = Map::new();
    for pointer in required {
        if *pointer == "/body" {
            continue;
        }
        let value = match *pointer {
            "/id" => json!(stable_id),
            "/pull_request_review_id" => json!(REVIEW_ID),
            _ => json!("value-as-the-projection-carries-it"),
        };
        retained.insert((*pointer).to_owned(), value);
    }
    json!({
        "schemaVersion": 1,
        "sourceKind": "github-reduced-source-record",
        "locatorKind": locator_kind,
        "locator": {
            "repository": "PhysShell/007",
            "pullRequest": "9001",
            "stableId": stable_id,
        },
        "redactionPolicyVersion": "1",
        "outcome": "BLOCK_SECRET",
        "coverageComplete": true,
        "retainedFields": Value::Object(retained),
        "blockedFields": ["/body"],
    })
}

fn carries_finding(review: &str, comment: &str) -> DecisionBasis {
    DecisionBasis {
        expected_redaction_policy: "1".to_owned(),
        observation_id: "review/external".to_owned(),
        inputs: Vec::new(),
        derived: vec![DerivedFact {
            derivation: "review-carries-finding".to_owned(),
            version: "1".to_owned(),
            value: json!(true),
            derived_from: vec![review.to_owned(), comment.to_owned()],
        }],
        expected_query: None,
        bindings: Vec::new(),
    }
}

/// The decision was refused, AND refused because the named slot was filled with
/// the named kind.
///
/// Pinned on a variant plus the slot index and both kind names, per the CR-N1
/// repair in `correction_b4r3.rs`: a witness that accepted any refusal would go
/// green on G1-F today, where the refusal is real and describes a redaction that
/// never happened.
#[track_caller]
fn refuses_for_slot(outcome: &Admissible, what: &str, slot: usize, expected: &str, found: &str) {
    let why: &[Unresolved] = match outcome {
        Admissible::CannotCheck { why } => why,
        Admissible::Yes { .. } => &[],
    };
    assert!(
        matches!(outcome, Admissible::CannotCheck { .. }),
        "{what}: admitted. Slot {slot} takes a {expected}; it was handed a {found}, and the \
         derived fact was recomputed from it anyway"
    );
    assert!(
        why.iter().any(|u| matches!(
            u,
            Unresolved::DerivationSlotKindMismatch { slot: s, expected: e, found: f, .. }
                if *s == slot && *e == expected && f == found
        )),
        "{what}: refused, but not for slot {slot} being handed a {found} where a {expected} \
         belongs. A refusal for another reason leaves the slot unchecked and misdescribes what \
         is wrong with the citation: got {why:?}"
    );
}

#[track_caller]
fn admits(outcome: &Admissible, what: &str) {
    assert!(
        matches!(outcome, Admissible::Yes { .. }),
        "{what}: refused. A fix that closes the slot by refusing the representation has closed \
         the wrong thing — redaction §8 requires a fact whose inputs survived to remain \
         usable: got {outcome:?}"
    );
}

/// G1-A — an §8.5 ISSUE COMMENT fills the submitted-review slot, and
/// `carries_finding` reports that a review carries the finding. There is no
/// review anywhere in the evidence.
#[test]
fn g1a_an_issue_comment_is_not_a_submitted_review() {
    let mut store = Store::default();
    let r = store.retain_under(
        &issue_comment_wearing_the_review_id(),
        &retain_all(&ISSUE_REQ),
    );
    let c = store.retain_under(&complete_comment(), &retain_all(&COMMENT_REQ));
    refuses_for_slot(
        &relations(&carries_finding(&r, &c), &store),
        "G1-A",
        0,
        "github-submitted-review",
        "github-issue-comment",
    );
}

/// G1-B — the same hole, entered by an §8.3 CHECK RUN. Carried separately from
/// G1-A because it is not the same near-miss: an issue comment is at least a
/// human utterance on the pull request, and a check run is a machine status
/// whose `stableId` merely lives in the same namespace.
#[test]
fn g1b_a_check_run_is_not_a_submitted_review() {
    let mut store = Store::default();
    let r = store.retain_under(
        &actions_check_wearing_the_review_id(),
        &retain_all(&CHECK_REQ),
    );
    let c = store.retain_under(&complete_comment(), &retain_all(&COMMENT_REQ));
    refuses_for_slot(
        &relations(&carries_finding(&r, &c), &store),
        "G1-B",
        0,
        "github-submitted-review",
        "github-actions-check",
    );
}

/// G1-C — the REDUCED half of G1-A, and the reason the check cannot read
/// `/sourceKind` alone. Every reduced record's `sourceKind` is
/// `github-reduced-source-record`; the surface it was reduced from is in
/// `/locatorKind`. A fix that checked only `sourceKind` would refuse every
/// reduced record in every slot and call that a repair.
#[test]
fn g1c_a_reduced_issue_comment_is_not_a_submitted_review() {
    let mut store = Store::default();
    let r = store.retain_under(
        &reduced("github-issue-comment", &ISSUE_REQ, REVIEW_ID),
        &block_body(&ISSUE_REQ),
    );
    let c = store.retain_under(&complete_comment(), &retain_all(&COMMENT_REQ));
    refuses_for_slot(
        &relations(&carries_finding(&r, &c), &store),
        "G1-C",
        0,
        "github-submitted-review",
        "github-issue-comment",
    );
}

/// G1-D — BOUNDARY. The pair the derivation is actually about, as complete §8
/// projections, admits today and must keep admitting.
#[test]
fn g1d_the_right_complete_pair_still_admits() {
    let mut store = Store::default();
    let r = store.retain_under(&complete_review(), &retain_all(&REVIEW_REQ));
    let c = store.retain_under(&complete_comment(), &retain_all(&COMMENT_REQ));
    admits(&relations(&carries_finding(&r, &c), &store), "G1-D");
}

/// G1-E — BOUNDARY, and the one that costs a wrong fix its escape. The same
/// pair REDUCED, `/body` blocked in both, every field the rule reads retained.
/// Redaction §8: a fact derived solely from fields that survived §7.1 is
/// unaffected. GREEN-B4D bought this case; a slot-kind check that reads
/// `sourceKind` for every artifact would sell it back.
#[test]
fn g1e_the_right_reduced_pair_still_admits() {
    let mut store = Store::default();
    let r = store.retain_under(
        &reduced("github-submitted-review", &REVIEW_REQ, REVIEW_ID),
        &block_body(&REVIEW_REQ),
    );
    let c = store.retain_under(
        &reduced("github-review-comment", &COMMENT_REQ, COMMENT_ID),
        &block_body(&COMMENT_REQ),
    );
    admits(&relations(&carries_finding(&r, &c), &store), "G1-E");
}

/// G1-F — SLOT 1, which is refused today for a reason that is not true.
///
/// A well-formed reduced ISSUE COMMENT in the review-comment slot currently
/// yields `PointerBlocked { "/pull_request_review_id" }` and
/// `DerivationInputUnavailable`: the redaction gate removed a field this
/// decision needed. It did not — the field was never in this surface's §5.3
/// required set, nothing was assessed for it, and nothing was blocked. The
/// operator who acts on that refusal audits a policy that is working.
///
/// The distinction is not cosmetic and not only about the message. What bounds
/// slot 1 today is that no other §8 projection declares `pullRequestReviewId`
/// and that §5.2's denominator keeps another `locatorKind` from carrying
/// `/pull_request_review_id`. Both are facts about a five-row schema table, and
/// a sixth surface that carries a review id — a thread, a discussion, a
/// suggestion batch — restores the G1-A hole in a slot everyone had recorded as
/// safe. The refusal must come from the slot.
#[test]
fn g1f_slot_one_is_refused_for_the_slot_and_not_for_a_redaction() {
    let mut store = Store::default();
    let r = store.retain_under(&complete_review(), &retain_all(&REVIEW_REQ));
    let c = store.retain_under(
        &reduced("github-issue-comment", &ISSUE_REQ, COMMENT_ID),
        &block_body(&ISSUE_REQ),
    );
    let outcome = relations(&carries_finding(&r, &c), &store);
    refuses_for_slot(
        &outcome,
        "G1-F",
        1,
        "github-review-comment",
        "github-issue-comment",
    );
    let Admissible::CannotCheck { why } = &outcome else {
        return;
    };
    assert!(
        !why.iter()
            .any(|u| matches!(u, Unresolved::PointerBlocked { .. })),
        "G1-F: still reports a blocked pointer. Nothing was blocked — \
         /pull_request_review_id is not in the §5.3 required set for an issue comment, so it \
         was never assessed and never gated. Reporting evidence loss for a malformed citation \
         sends the reader to audit a redaction policy that is behaving correctly: got {why:?}"
    );
}

/// G1-G — INVARIANT, and the premise the fix rests on rather than a witness
/// about behaviour.
///
/// A registry slot names ONE kind, checked against `/sourceKind` for a complete
/// projection and `/locatorKind` for a reduced record. That is sound only while
/// the two are the same vocabulary. If §8 gains a surface §7.3 has no locator
/// shape for — or the reverse — one name would silently stop meaning the same
/// thing on the two paths, and the single field would become a proxy for two
/// properties: this file's own anti-pattern, reappearing inside the repair.
///
/// Held here rather than in prose, because prose does not fail.
#[test]
fn g1g_source_kinds_and_locator_kinds_are_one_vocabulary() {
    let source_kinds: BTreeSet<&str> = SOURCE_SCHEMAS.iter().map(|s| s.source_kind).collect();
    let locator_kinds: BTreeSet<&str> = LOCATOR_SHAPES.iter().map(|l| l.locator_kind).collect();
    assert_eq!(
        source_kinds, locator_kinds,
        "§8 sourceKind and §7.3 locatorKind have drifted apart. A derivation slot names one \
         kind and checks it against whichever of the two members the artifact carries; that is \
         one check only while the two name the same surfaces. Add the missing row, or give the \
         slot two fields and stop pretending it is one"
    );
}
