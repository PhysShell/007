//! The preregistered escape set for Slice B.
//!
//! FROZEN BEFORE THE IMPLEMENTATION. Slice A took ten review rounds because each
//! fix was written to the instance in front of it and the next round found the
//! next instance. The countermeasure is not more care; it is naming the escapes
//! before the mechanism exists, so the mechanism cannot be shaped to a test set
//! written after it and then declared complete.
//!
//! Each witness is a way a provenance binding can look correct from the inside
//! while certifying nothing:
//!
//! ```text
//! B1  digest substitution        the store chooses what it is checked against
//! B2  bytes substitution         the store returns other bytes under the key
//! B3  missing retained input     an unresolvable input becomes silence
//! B4  blocked irrelevant field   over-refusal: a survivable decision refused
//! B5  blocked required field     under-refusal: a lost input read anyway
//! B6  derived self-certification a fact that is its own evidence
//! B7  wrong derivation provenance a true fact citing sources that don't imply it
//! B8  falsification empty set    a failed scan reported as zero findings
//! B9  subject-read loss          an unavailable head reported as not stale
//! B10 binding substitution       retention authorised by a different assessment
//! ```
//!
//! B4 and B5 are a PAIR and must stay one. Alone, B5 is satisfied by refusing
//! any observation with a blocked field anywhere, which redaction policy §10
//! explicitly rejects — the gate does not determine the state, and R6/R7 are the
//! frozen witness that identical gate outcomes give opposite admissibility
//! depending on what the decision read. B4 is what stops the fix for B5 from
//! being "refuse everything".

// Justification for the restriction-lint allowance, per AGENTS.md rule 4 and the
// precedent in `crates/o7-closure-classifier/tests/frozen_fixtures.rs`: every
// panic path below is this test's own assertion failing, or its own handling of
// a JSON literal written in this file. Nothing here runs against production
// input, and a malformed literal must fail loudly rather than be skipped — a
// skipped witness is the vacuous green this file exists to prevent.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use o7_closure_canonical::digest;
use o7_closure_provenance::{
    admissibility, Admissible, DecisionBasis, DecisionInput, DeclaredBinding, DerivedFact,
    RetainedEvidence, RetentionBinding, Unresolved,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;

// ---- A store, and only the operations a store is allowed to have.

/// An in-memory [`RetainedEvidence`] built from objects keyed by their own
/// digest.
///
/// Note what it cannot do: there is no method that hands out a digest. A test
/// that wanted to model "the store tells the consumer what to check" would have
/// to change the trait, which is the point — the shape of the API is the
/// mechanism, not a convention on top of it.
#[derive(Default)]
struct Store {
    records: BTreeMap<String, Value>,
    bindings: BTreeMap<String, RetentionBinding>,
    /// Digests whose stored bytes are deliberately not the ones the key names.
    substituted: BTreeMap<String, Value>,
}

impl Store {
    fn retain(&mut self, object: &Value) -> String {
        let d = digest(object).expect("digest").as_str().to_owned();
        self.records.insert(d.clone(), object.clone());
        // Everything retained through the gate carries an authorising
        // assessment; a record without one is inadmissible per redaction policy
        // §9.2, and B10's witness supplies its own instead of this default.
        let assessment = format!("sha256:{}", "a".repeat(64));
        self.bindings.insert(
            d.clone(),
            RetentionBinding {
                record_digest: d.clone(),
                assessment_digest: assessment,
            },
        );
        d
    }

    /// Store `bytes` under a key that does NOT name them. Models a resolver
    /// returning the wrong object for a correct request.
    fn substitute(&mut self, under: &str, bytes: &Value) {
        self.substituted.insert(under.to_owned(), bytes.clone());
    }

    fn bind(&mut self, record_digest: &str, assessment_digest: &str) {
        self.bindings.insert(
            record_digest.to_owned(),
            RetentionBinding {
                record_digest: record_digest.to_owned(),
                assessment_digest: assessment_digest.to_owned(),
            },
        );
    }

    fn unbind(&mut self, record_digest: &str) {
        self.bindings.remove(record_digest);
    }
}

impl RetainedEvidence for Store {
    fn resolve(&self, d: &str) -> Option<Value> {
        self.substituted
            .get(d)
            .or_else(|| self.records.get(d))
            .cloned()
    }
    fn binding_for(&self, record_digest: &str) -> Option<RetentionBinding> {
        self.bindings.get(record_digest).cloned()
    }
}

// ---- Fixtures.

fn review(stable_id: &str, commit_id: &str) -> Value {
    json!({
        "schemaVersion": 1,
        "sourceKind": "github-submitted-review",
        "stableId": stable_id,
        "user": {"id": "9000000901", "login": "synthetic-external-reviewer", "type": "User"},
        "authorAssociation": "NONE",
        "state": "CHANGES_REQUESTED",
        "body": "The manifest member is still unreachable.\n",
        "submittedAt": "2026-08-05T09:02:47Z",
        "commitId": commit_id,
    })
}

fn review_comment(stable_id: &str, owning_review: &str) -> Value {
    json!({
        "schemaVersion": 1,
        "sourceKind": "github-review-comment",
        "stableId": stable_id,
        "pullRequestReviewId": owning_review,
        "user": {"id": "9000000901", "login": "synthetic-external-reviewer", "type": "User"},
        "authorAssociation": "NONE",
        "body": "here specifically\n",
        "createdAt": "2026-08-05T09:02:47Z",
        "updatedAt": "2026-08-05T09:02:47Z",
    })
}

/// A `github-reduced-source-record` per redaction policy §7: `/body` blocked,
/// `/commitId` assessed clean and retained.
///
/// This is the R6/R7 shape — the artifact whose whole purpose is that two
/// decisions over the same gate outcome come out differently.
fn reduced_review(stable_id: &str, commit_id: &str) -> Value {
    json!({
        "schemaVersion": 1,
        "sourceKind": "github-reduced-source-record",
        "locatorKind": "github-submitted-review",
        "locator": {
            "repository": "PhysShell/007",
            "pullRequest": "9001",
            "stableId": stable_id,
        },
        "redactionPolicyVersion": "1",
        "outcome": "BLOCK_SECRET",
        "coverageComplete": true,
        "retainedFields": {
            "/commitId": commit_id,
            "/stableId": stable_id,
        },
        "blockedFields": ["/body"],
    })
}

fn reads(source_digest: &str, pointer: &str) -> DecisionInput {
    DecisionInput {
        source_digest: source_digest.to_owned(),
        pointer: pointer.to_owned(),
    }
}

fn basis(observation_id: &str, inputs: Vec<DecisionInput>) -> DecisionBasis {
    DecisionBasis {
        observation_id: observation_id.to_owned(),
        inputs,
        derived: Vec::new(),
        expected_query_digest: None,
        bindings: Vec::new(),
    }
}

#[track_caller]
fn cannot_check(outcome: &Admissible) -> &[Unresolved] {
    match outcome {
        Admissible::CannotCheck { why } => why,
        Admissible::Yes { .. } => panic!(
            "admitted. A decision whose evidence did not resolve was reported as one whose \
             evidence did — which is the absent-signal-as-passed-check error, arriving through \
             provenance instead of through acquisition"
        ),
    }
}

#[track_caller]
fn admitted(outcome: &Admissible) -> &[Value] {
    match outcome {
        Admissible::Yes { values } => values,
        Admissible::CannotCheck { why } => panic!(
            "refused, and should not have been: {why:?}. Redaction policy §10 — the gate does \
             not determine the state; a decision whose every input survived keeps the frozen \
             classifier semantics"
        ),
    }
}

// ---- The control: a decision that should simply work.

#[test]
fn a_fully_resolvable_decision_is_admissible() {
    let mut store = Store::default();
    let d = store.retain(&review(
        "9000000202",
        "1f2e3d4c5b6a798807162534435261708f9e0d1c",
    ));

    let outcome = admissibility(
        &basis("review/external", vec![reads(&d, "/commitId")]),
        &store,
    );
    assert_eq!(
        admitted(&outcome),
        [json!("1f2e3d4c5b6a798807162534435261708f9e0d1c")]
    );
}

// ---- B1. Digest substitution.

/// The store must not be able to choose which digest replay is checked against.
///
/// The decision basis names `D1`. The store holds a perfectly valid, correctly
/// self-consistent query snapshot `Q2` under its own digest `D2`. If the
/// expected digest can be read out of the store, `Q2` verifies against `D2` and
/// the substitution is invisible: every local check passes and the artifact
/// replayed is not the artifact the decision was made from.
#[test]
fn b1_the_store_cannot_supply_the_digest_it_is_checked_against() {
    let mut store = Store::default();
    let d1 = store.retain(&review(
        "9000000202",
        "1111111111111111111111111111111111111111",
    ));
    let d2 = store.retain(&review(
        "9000000999",
        "2222222222222222222222222222222222222222",
    ));
    assert_ne!(d1, d2, "the two snapshots must be genuinely different");

    // The basis names D1. Nothing the store can do may cause D2 to be read.
    let mut b = basis("review/external", vec![reads(&d1, "/commitId")]);
    b.expected_query_digest = Some(d1.clone());

    let outcome = admissibility(&b, &store);
    assert_eq!(
        admitted(&outcome),
        [json!("1111111111111111111111111111111111111111")],
        "the value read must come from the record the BASIS named, not from whatever else \
         the store holds"
    );

    // And the structural half, which is the part that actually holds: there is
    // no way to ask the store for a digest at all. If this assertion ever needs
    // relaxing, the trait has grown the method that makes B1 reachable.
    let trait_source = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/lib.rs"))
        .expect("reading this crate's source");
    let trait_body = trait_source
        .split_once("pub trait RetainedEvidence")
        .expect("the trait is declared")
        .1
        .split_once("\n}")
        .expect("the trait body closes")
        .0;
    assert!(
        !trait_body.contains("-> String") && !trait_body.contains("-> Digest"),
        "RetainedEvidence must not return a digest: a store that can hand out the expectation \
         is a store that certifies itself"
    );
}

// ---- B2. Bytes substitution.

/// The store returns other bytes under the requested key.
///
/// `digest(returned) != requested`, so the resolver's answer is refused rather
/// than used. This is the Slice A binding applied at the store boundary, and it
/// is the reason `resolve` is documented as untrusted.
#[test]
fn b2_bytes_returned_under_a_key_that_does_not_name_them_are_refused() {
    let mut store = Store::default();
    let d1 = store.retain(&review(
        "9000000202",
        "1111111111111111111111111111111111111111",
    ));
    store.substitute(
        &d1,
        &review("9000000999", "2222222222222222222222222222222222222222"),
    );

    let outcome = admissibility(
        &basis("review/external", vec![reads(&d1, "/commitId")]),
        &store,
    );
    let why = cannot_check(&outcome);
    assert!(
        matches!(why, [Unresolved::RecordDigestMismatch { requested, .. }] if requested == &d1),
        "expected a digest mismatch naming the requested key, got {why:?}"
    );
}

// ---- B3. Missing retained input.

/// An input that cannot be resolved is `CANNOT_CHECK` — not silence.
///
/// The first-cut implementation simply skips what it cannot resolve, which turns
/// a decision resting on three sources into one resting on two and reports it as
/// fully evidenced.
#[test]
fn b3_an_unresolvable_input_is_cannot_check_not_a_shorter_input_list() {
    let mut store = Store::default();
    let present = store.retain(&review(
        "9000000202",
        "1111111111111111111111111111111111111111",
    ));
    let absent = format!("sha256:{}", "b".repeat(64));

    let outcome = admissibility(
        &basis(
            "review/external",
            vec![reads(&present, "/commitId"), reads(&absent, "/commitId")],
        ),
        &store,
    );
    let why = cannot_check(&outcome);
    assert!(
        why.iter()
            .any(|u| matches!(u, Unresolved::NoSuchRecord { digest } if digest == &absent)),
        "expected the unresolvable digest to be named, got {why:?}"
    );
}

// ---- B4 / B5. The retention pair. Neither is complete alone.

/// A blocked field the decision never reads does not defeat the decision.
///
/// Redaction policy §10, and R7: a wrong-SHA `OWED` derives from `/commitId` and
/// the subject head. If the body is blocked while `/commitId` was assessed clean
/// and retained, that decision remains fully explainable from retained bytes.
/// Refusing it would be reporting a loss of evidence that did not occur.
#[test]
fn b4_a_blocked_field_this_decision_does_not_read_is_survivable() {
    let mut store = Store::default();
    let d = store.retain(&reduced_review(
        "9000000202",
        "1f2e3d4c5b6a798807162534435261708f9e0d1c",
    ));

    let outcome = admissibility(
        &basis("review/external", vec![reads(&d, "/commitId")]),
        &store,
    );
    assert_eq!(
        admitted(&outcome),
        [json!("1f2e3d4c5b6a798807162534435261708f9e0d1c")],
        "a reduced record resolves the pointers it retained"
    );
}

/// A blocked field the decision DOES read defeats that decision, and says so as
/// a retention loss rather than as an absent field.
#[test]
fn b5_a_blocked_field_this_decision_reads_is_cannot_check() {
    let mut store = Store::default();
    let d = store.retain(&reduced_review(
        "9000000202",
        "1f2e3d4c5b6a798807162534435261708f9e0d1c",
    ));

    let outcome = admissibility(&basis("review/external", vec![reads(&d, "/body")]), &store);
    let why = cannot_check(&outcome);
    assert!(
        matches!(why, [Unresolved::PointerBlocked { pointer, .. }] if pointer == "/body"),
        "a blocked pointer is a retention loss, distinct from a pointer that was never there: \
         got {why:?}"
    );
}

/// The locator is identity, never surviving evidence.
///
/// Redaction policy §7.3: a locator value MUST NOT satisfy a decision-basis
/// pointer. Without this, `/stableId` can sit in `blockedFields` while the same
/// source-derived id remains readable through the locator — the field gate
/// bypassed by an alias.
#[test]
fn b5b_a_locator_value_does_not_satisfy_a_decision_pointer() {
    let mut store = Store::default();
    let mut record = reduced_review("9000000202", "1f2e3d4c5b6a798807162534435261708f9e0d1c");
    // Block the very field the locator also carries.
    record
        .pointer_mut("/retainedFields")
        .and_then(Value::as_object_mut)
        .expect("retainedFields is an object")
        .remove("/stableId");
    record
        .as_object_mut()
        .expect("the record is an object")
        .insert("blockedFields".to_owned(), json!(["/body", "/stableId"]));
    let d = store.retain(&record);

    let outcome = admissibility(
        &basis("review/external", vec![reads(&d, "/stableId")]),
        &store,
    );
    let why = cannot_check(&outcome);
    assert!(
        matches!(why, [Unresolved::PointerBlocked { pointer, .. }] if pointer == "/stableId"),
        "the locator must not be readable as surviving evidence: got {why:?}"
    );
}

// ---- B6 / B7. Derived facts.

/// A supplied derived fact is not its own evidence.
///
/// `carries_finding = true` with sources that do not imply it must be refused.
/// The classifier is documented as never inferring this from body text — which
/// puts the whole weight on the adapter's assertion, and an assertion nobody
/// recomputes is exactly the artifact certifying itself.
#[test]
fn b6_a_derived_fact_that_its_named_sources_do_not_imply_is_refused() {
    let mut store = Store::default();
    let r = store.retain(&review(
        "9000000202",
        "1111111111111111111111111111111111111111",
    ));
    // A comment belonging to a DIFFERENT review: the rule yields false.
    let c = store.retain(&review_comment("9000000301", "9000000999"));

    let mut b = basis("review/external", vec![reads(&r, "/commitId")]);
    b.derived = vec![DerivedFact {
        derivation: "review-carries-finding".to_owned(),
        version: "1".to_owned(),
        value: json!(true),
        derived_from: vec![r.clone(), c.clone()],
    }];

    let outcome = admissibility(&b, &store);
    let why = cannot_check(&outcome);
    assert!(
        why.iter().any(|u| matches!(
            u,
            Unresolved::DerivationDisagrees { claimed, recomputed, .. }
                if claimed == &json!(true) && recomputed == &json!(false)
        )),
        "expected the recomputation to contradict the claim, got {why:?}"
    );
}

/// A fact that happens to be true of the world, citing sources that do not
/// establish it, is refused.
///
/// This is the one that separates *naming inputs* from *being derived from
/// them*. §18 as written requires only the name; a name that is never followed
/// is satisfied equally well by the right answer and the wrong citation.
#[test]
fn b7_a_true_fact_with_the_wrong_named_sources_is_refused() {
    let mut store = Store::default();
    let r = store.retain(&review(
        "9000000202",
        "1111111111111111111111111111111111111111",
    ));
    let owning = store.retain(&review_comment("9000000301", "9000000202"));
    let unrelated = store.retain(&review_comment("9000000302", "9000000999"));
    assert_ne!(owning, unrelated);

    // carries_finding really IS true — `owning` belongs to this review. But the
    // fact cites `unrelated`, which does not establish it.
    let mut b = basis("review/external", vec![reads(&r, "/commitId")]);
    b.derived = vec![DerivedFact {
        derivation: "review-carries-finding".to_owned(),
        version: "1".to_owned(),
        value: json!(true),
        derived_from: vec![r.clone(), unrelated.clone()],
    }];

    let outcome = admissibility(&b, &store);
    let why = cannot_check(&outcome);
    assert!(
        why.iter()
            .any(|u| matches!(u, Unresolved::DerivationDisagrees { .. })),
        "a correct value derived from the wrong sources is not a derived fact, it is a \
         coincidence with a citation: got {why:?}"
    );
}

/// And the positive control, so the fix for B6/B7 cannot be "refuse every
/// derived fact".
#[test]
fn b7b_a_correctly_derived_fact_is_admitted() {
    let mut store = Store::default();
    let r = store.retain(&review(
        "9000000202",
        "1111111111111111111111111111111111111111",
    ));
    let owning = store.retain(&review_comment("9000000301", "9000000202"));

    let mut b = basis("review/external", vec![reads(&r, "/commitId")]);
    b.derived = vec![DerivedFact {
        derivation: "review-carries-finding".to_owned(),
        version: "1".to_owned(),
        value: json!(true),
        derived_from: vec![r.clone(), owning.clone()],
    }];

    admitted(&admissibility(&b, &store));
}

/// A derivation this crate cannot re-execute is refused, not trusted.
///
/// The same law as an unregistered matcher id: an unknown name resolves to
/// nothing, never to a default, and never to "assume the adapter got it right".
#[test]
fn b7c_an_unregistered_derivation_is_refused() {
    let mut store = Store::default();
    let r = store.retain(&review(
        "9000000202",
        "1111111111111111111111111111111111111111",
    ));

    let mut b = basis("review/external", vec![reads(&r, "/commitId")]);
    b.derived = vec![DerivedFact {
        derivation: "vibes-based-finding-detection".to_owned(),
        version: "7".to_owned(),
        value: json!(true),
        derived_from: vec![r.clone()],
    }];

    let outcome = admissibility(&b, &store);
    let why = cannot_check(&outcome);
    assert!(
        why.iter()
            .any(|u| matches!(u, Unresolved::UnknownDerivation { .. })),
        "got {why:?}"
    );
}

// ---- B10. Retention binding substitution.

/// A retained record with no reachable authorising assessment is inadmissible.
///
/// Redaction policy §9.2: bytes somebody kept, not evidence somebody was
/// permitted to keep.
#[test]
fn b10a_a_record_with_no_retention_binding_is_inadmissible() {
    let mut store = Store::default();
    let d = store.retain(&review(
        "9000000202",
        "1111111111111111111111111111111111111111",
    ));
    store.unbind(&d);

    let outcome = admissibility(
        &basis("review/external", vec![reads(&d, "/commitId")]),
        &store,
    );
    let why = cannot_check(&outcome);
    assert!(
        matches!(why, [Unresolved::NoRetentionBinding { record_digest }] if record_digest == &d),
        "got {why:?}"
    );
}

/// The basis cannot re-attribute a retained record to a different assessment
/// than the one that authorised it.
///
/// The retained binding is the authority on its own outcome; a basis that names
/// another assessment is asserting a permission that was never granted, and it
/// would resolve perfectly well if nobody compared the two.
#[test]
fn b10b_a_declared_binding_that_contradicts_the_retained_one_is_refused() {
    let mut store = Store::default();
    let d = store.retain(&review(
        "9000000202",
        "1111111111111111111111111111111111111111",
    ));
    let a1 = format!("sha256:{}", "1".repeat(64));
    let a2 = format!("sha256:{}", "2".repeat(64));
    store.bind(&d, &a1);

    let mut b = basis("review/external", vec![reads(&d, "/commitId")]);
    b.bindings = vec![DeclaredBinding {
        record_digest: d.clone(),
        assessment_digest: a2.clone(),
    }];

    let outcome = admissibility(&b, &store);
    let why = cannot_check(&outcome);
    assert!(
        why.iter().any(|u| matches!(
            u,
            Unresolved::BindingMismatch { declared, retained, .. }
                if declared == &a2 && retained == &a1
        )),
        "got {why:?}"
    );
}
