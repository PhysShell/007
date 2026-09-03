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
#![allow(clippy::expect_used, clippy::panic)]

use o7_closure_canonical::digest;
use o7_closure_provenance::{
    relations_checked, AcquisitionLocator, Admissible, CitedSource, DecisionBasis, DecisionInput,
    DeclaredBinding, DerivedFact, ExpectedDetector, ExpectedQuery, QueryBinding, RetainedEvidence,
    Unresolved,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;

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

/// The canonical bytes of a conforming §9.5 `closure-retention-binding`.
///
/// RED-B4R reshaped `binding_for` to hand over BYTES rather than a decoded
/// struct, so a store can now express a binding that is absent, substituted or
/// malformed — none of which the old two-string return could represent.
fn binding_bytes(record_digest: &str, assessment_digest: &str) -> Value {
    json!({
        "schemaVersion": 1,
        "sourceKind": "closure-retention-binding",
        "recordDigest": record_digest,
        "assessmentDigest": assessment_digest,
    })
}

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
    bindings: BTreeMap<String, Value>,
    /// Digests whose stored bytes are deliberately not the ones the key names.
    substituted: BTreeMap<String, Value>,
}

impl Store {
    fn retain(&mut self, object: &Value) -> String {
        // The authorising assessment is RETAINED, not merely named.
        //
        // An earlier version of this helper bound every record to a fixed
        // `sha256:aaaa…` that was never stored. Every "admitted" test in this
        // file therefore passed with an unreachable assessment, and external
        // review found the defect by reading this helper rather than the
        // implementation — the fixture was the witness. §9.2 requires the
        // assessment's canonical bytes retained and reachable, and a test store
        // that cannot satisfy its own contract is modelling a store production
        // must refuse.
        let assessment = conforming_assessment(object);
        let assessment_digest = digest(&assessment).expect("digest").as_str().to_owned();
        self.records
            .insert(assessment_digest.clone(), assessment.clone());

        let d = digest(object).expect("digest").as_str().to_owned();
        self.records.insert(d.clone(), object.clone());
        let binding = binding_bytes(&d, &assessment_digest);
        // RETAINED, not merely handed over: §9.2 makes the binding a separately
        // retained object, and GREEN-B4R resolves it under its own digest.
        self.put(&binding);
        self.bindings.insert(d.clone(), binding);
        d
    }

    /// Retain an object with NO retention binding. §5.3 places
    /// `github-query-snapshot` outside the gate, so no assessment about one can
    /// exist and none is demanded.
    fn put(&mut self, object: &Value) -> String {
        let d = digest(object).expect("digest").as_str().to_owned();
        self.records.insert(d.clone(), object.clone());
        d
    }

    /// Store `bytes` under a key that does NOT name them. Models a resolver
    /// returning the wrong object for a correct request.
    fn substitute(&mut self, under: &str, bytes: &Value) {
        self.substituted.insert(under.to_owned(), bytes.clone());
    }

    /// Re-bind a record to a DIFFERENT retained assessment, for the cases about
    /// which assessment authorised what.
    fn bind_to_another_assessment(&mut self, record_digest: &str) -> String {
        let mut other = conforming_assessment(
            &self
                .records
                .get(record_digest)
                .cloned()
                .expect("the record being re-bound is retained"),
        );
        other
            .as_object_mut()
            .expect("the assessment is an object")
            .insert("observedAt".to_owned(), json!("2026-08-05T11:11:11Z"));
        let other_digest = digest(&other).expect("digest").as_str().to_owned();
        self.records.insert(other_digest.clone(), other);
        let binding = binding_bytes(record_digest, &other_digest);
        // RETAINED, not merely handed over: §9.2 makes the binding a separately
        // retained object, and GREEN-B4R resolves it under its own digest.
        self.put(&binding);
        self.bindings.insert(record_digest.to_owned(), binding);
        other_digest
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
    fn binding_for(&self, record_digest: &str) -> Option<Value> {
        self.bindings.get(record_digest).cloned()
    }
}

/// The §5.3 always-set, in §5.5 order, for the kinds these fixtures use.
///
/// Pointers into the DECODED source object — §5.3 says in as many words that
/// these are GitHub's field names rather than the canonical projection's, and
/// §9 fixes the assessment's `representation` as `decoded-source-field-values`
/// to match. An earlier revision of this file listed `/commitId` and `/stableId`
/// here, which is the §8 projection's vocabulary and a different set of names
/// for a different space.
fn required_fields(kind: &str) -> &'static [&'static str] {
    match kind {
        "github-submitted-review" => &[
            "/author_association",
            "/body",
            "/commit_id",
            "/id",
            "/state",
            "/submitted_at",
            "/user/id",
            "/user/login",
            "/user/type",
        ],
        "github-review-comment" => &[
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
        ],
        other => panic!("no §5.3 required set is transcribed for {other}"),
    }
}

/// A conforming §9 `RetentionAssessment` that authorises `record`.
///
/// Derived from the record rather than fixed, because an assessment only means
/// anything relative to one: §5.2 fixes the denominator by source kind, and
/// §7.1 computes the partition from this assessment's own findings and coverage.
/// A single constant assessment could not be the authority for two records of
/// different kinds, and pretending otherwise is what let a `RETAIN` assessment
/// stand behind a reduced record that blocked a field.
fn conforming_assessment(record: &Value) -> Value {
    let is_reduced = record.pointer("/sourceKind").and_then(Value::as_str)
        == Some("github-reduced-source-record");
    let kind = record
        .pointer(if is_reduced {
            "/locatorKind"
        } else {
            "/sourceKind"
        })
        .and_then(Value::as_str)
        .expect("the fixture names its kind");
    let assessed = required_fields(kind);

    let mut assessment = json!({
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
    });

    if is_reduced {
        // §7.1 runs the other way: the record's blocked set is what the findings
        // produced, so the fixture's assessment has to name exactly those fields
        // or the two artifacts describe different gate runs.
        let blocked = record
            .pointer("/blockedFields")
            .and_then(Value::as_array)
            .expect("a reduced record carries blockedFields");
        let findings: Vec<Value> = blocked
            .iter()
            .map(|field| json!({"field": field, "findingId": "rule-aws-key"}))
            .collect();
        let object = assessment
            .as_object_mut()
            .expect("the assessment is an object");
        object.insert("outcome".to_owned(), json!("BLOCK_SECRET"));
        object.insert("findings".to_owned(), Value::Array(findings));
    }
    assessment
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
        // §8.4 makes commitId, originalCommitId and path REQUIRED. This fixture
        // carried none of them and was accepted for as long as nothing validated
        // the closed form — the same defect at the fixture level that RED-B4
        // measured at the implementation level.
        "commitId": "1f2e3d4c5b6a798807162534435261708f9e0d1c",
        "originalCommitId": "1f2e3d4c5b6a798807162534435261708f9e0d1c",
        "path": "crates/o7-closure-provenance/src/lib.rs",
        "createdAt": "2026-08-05T09:02:47Z",
        "updatedAt": "2026-08-05T09:02:47Z",
    })
}

/// A `github-reduced-source-record` per redaction policy §7: `/body` blocked,
/// every other §5.3 field assessed clean and retained.
///
/// This is the R6/R7 shape — the artifact whose whole purpose is that two
/// decisions over the same gate outcome come out differently.
///
/// The partition is the §7.1 COMPUTATION and not a selection: `blockedFields` is
/// what the findings flagged, `retainedFields` is the rest of the §5.3 set, and
/// together they account for every required field exactly once. An earlier
/// revision of this fixture retained two pointers out of nine and named them in
/// the §8 projection's vocabulary; it satisfied every check that existed at the
/// time and was not a conformant record.
fn reduced_review(stable_id: &str, commit_id: &str) -> Value {
    reduced_review_blocking(stable_id, commit_id, &["/body"])
}

/// The same, with the blocked set chosen by the caller.
fn reduced_review_blocking(stable_id: &str, commit_id: &str, blocked: &[&str]) -> Value {
    let mut retained = serde_json::Map::new();
    for pointer in required_fields("github-submitted-review") {
        if blocked.contains(pointer) {
            continue;
        }
        // §7.2: each retained pointer holds exactly the value the COMPLETE §8
        // projection would have carried for it.
        let value = match *pointer {
            "/commit_id" => json!(commit_id),
            "/id" => json!(stable_id),
            "/user/id" => json!("9000000901"),
            "/user/login" => json!("synthetic-external-reviewer"),
            "/user/type" => json!("User"),
            "/author_association" => json!("NONE"),
            "/state" => json!("CHANGES_REQUESTED"),
            "/submitted_at" => json!("2026-08-05T09:02:47Z"),
            "/body" => json!("The manifest member is still unreachable.\n"),
            other => panic!("no §7.2 value is defined for {other}"),
        };
        retained.insert((*pointer).to_owned(), value);
    }
    let mut sorted = blocked.to_vec();
    sorted.sort_unstable();
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
        "retainedFields": Value::Object(retained),
        "blockedFields": sorted,
    })
}

/// A §13 `github-query-snapshot`. §5.3 places this kind OUTSIDE the gate — it is
/// constructed rather than fetched, and retains only enumeration facts and
/// digests of objects that passed the gate on their own — so it carries no
/// retention binding and none is demanded of it.
fn query_snapshot() -> Value {
    json!({
        // schemaVersion 2 since GREEN-B4R.3: §13.1 adds
        // `matcher.implementationDigest` at that version, and a replay-dependent
        // role now REQUIRES a bound implementation. A version-1 snapshot is
        // still a conforming artifact — `correction_b4r3.rs`'s F1-A is the
        // witness that it passes the door and is refused the ROLE — but it can
        // no longer stand as a positive control for qualification.
        "schemaVersion": 2,
        "sourceKind": "github-query-snapshot",
        "surface": "pull-request-submitted-reviews",
        "requiredObservationId": "review/external",
        "binding": {"repository": "PhysShell/007", "pullRequest": "9001"},
        "pagination": {
            "perPage": 100,
            "pagesRequested": ["1"],
            "pagesObtained": ["1"],
            "nextPagePresent": false,
        },
        "enumeration": "COMPLETE",
        "matcher": {
            "id": "review-by-expected-author-login",
            "version": "1",
            "implementationDigest": bound_implementation_digest(),
            "parameters": {"expectedAuthorLogin": "synthetic-external-reviewer"},
        },
        "allReturnedSnapshotDigests": [],
        "matchedSnapshotDigests": [],
    })
}

fn reads(source_digest: &str, pointer: &str) -> DecisionInput {
    DecisionInput {
        source_digest: source_digest.to_owned(),
        pointer: pointer.to_owned(),
        locator: AcquisitionLocator::Check {
            repository: "PhysShell/007".to_owned(),
            stable_id: "0".to_owned(),
        },
    }
}

fn basis(observation_id: &str, inputs: Vec<DecisionInput>) -> DecisionBasis {
    DecisionBasis {
        expected_redaction_policy: "1".to_owned(),
        expected_detector: ExpectedDetector {
            id: "synthetic-detector".to_owned(),
            version: "1".to_owned(),
            config_digest:
                "sha256:1111111111111111111111111111111111111111111111111111111111111111".to_owned(),
        },
        observation_id: observation_id.to_owned(),
        inputs,
        derived: Vec::new(),
        expected_query: None,
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

    let outcome = relations(
        &basis("review/external", vec![reads(&d, "/commitId")]),
        &store,
    );
    assert_eq!(
        admitted(&outcome),
        [json!("1f2e3d4c5b6a798807162534435261708f9e0d1c")]
    );
}

// ---- B1. Digest substitution.

/// The decision basis names the digest; the store resolves it.
///
/// WITHDRAWN IN PART. This witness previously also asserted, against the trait's
/// own source, that "RetainedEvidence has no method returning a digest". That
/// claim was false when it was written — `binding_for` returns a
/// `RetentionBinding` carrying two digest strings a caller reads — and the
/// substring test that was meant to enforce it could not have seen them. A green
/// witness for an absent property is worse than no witness, so it is removed
/// rather than strengthened.
///
/// What survives here is the behavioural half, which was always the real content:
/// the value read comes from the record the BASIS named. The API-surface guard
/// now lives in `correction_b2.rs::e2_…`, which claims only that the trust
/// surface cannot change unnoticed, and the law itself is carried by the
/// semantic witnesses in that file.
#[test]
fn b1_the_value_read_comes_from_the_record_the_basis_named() {
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

    let mut b = basis("review/external", vec![reads(&d1, "/commitId")]);
    // The expected query digest is a QUERY SNAPSHOT. An earlier revision pointed
    // it at `d1` — the submitted review this decision reads — which resolved and
    // re-digested perfectly while standing in for an artifact of a different
    // kind, in a role only a query snapshot can fill.
    b.expected_query = Some(ExpectedQuery {
        digest: store.put(&query_snapshot()),
        subject: QueryBinding {
            repository: "PhysShell/007".to_owned(),
            pull_request: "9001".to_owned(),
        },
    });

    let outcome = relations(&b, &store);
    assert_eq!(
        admitted(&outcome),
        [json!("1111111111111111111111111111111111111111")],
        "the value read must come from the record the BASIS named, not from whatever else \
         the store holds"
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

    let outcome = relations(
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

    let outcome = relations(
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

    // A reduced record's partition is keyed in §5.3's DECODED-source space, so
    // the pointer that reads it is `/commit_id` where the same decision over a
    // complete §8 projection reads `/commitId`. The basis pointer's space follows
    // the record kind, which `read_pointer` already dispatches on.
    let outcome = relations(
        &basis("review/external", vec![reads(&d, "/commit_id")]),
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

    let outcome = relations(&basis("review/external", vec![reads(&d, "/body")]), &store);
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
    // Block the very field the locator also carries. §5.3 calls it `/id` in the
    // decoded source, and `locator.stableId` is the same source-derived value
    // under the §8 projection's name — which is exactly the aliasing §7.3 exists
    // to close.
    let record = reduced_review_blocking(
        "9000000202",
        "1f2e3d4c5b6a798807162534435261708f9e0d1c",
        &["/body", "/id"],
    );
    assert_eq!(
        record.pointer("/locator/stableId"),
        Some(&json!("9000000202")),
        "fixture: the locator must still carry the id this witness blocks"
    );
    let d = store.retain(&record);

    let outcome = relations(&basis("review/external", vec![reads(&d, "/id")]), &store);
    let why = cannot_check(&outcome);
    assert!(
        matches!(why, [Unresolved::PointerBlocked { pointer, .. }] if pointer == "/id"),
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
        derived_from: vec![
            CitedSource {
                digest: r.clone(),
                locator: AcquisitionLocator::InPullRequest {
                    repository: "PhysShell/007".to_owned(),
                    pull_request: "9001".to_owned(),
                    stable_id: "0".to_owned(),
                },
            },
            CitedSource {
                digest: c.clone(),
                locator: AcquisitionLocator::InPullRequest {
                    repository: "PhysShell/007".to_owned(),
                    pull_request: "9001".to_owned(),
                    stable_id: "0".to_owned(),
                },
            },
        ],
    }];

    let outcome = relations(&b, &store);
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
        derived_from: vec![
            CitedSource {
                digest: r.clone(),
                locator: AcquisitionLocator::InPullRequest {
                    repository: "PhysShell/007".to_owned(),
                    pull_request: "9001".to_owned(),
                    stable_id: "0".to_owned(),
                },
            },
            CitedSource {
                digest: unrelated.clone(),
                locator: AcquisitionLocator::InPullRequest {
                    repository: "PhysShell/007".to_owned(),
                    pull_request: "9001".to_owned(),
                    stable_id: "0".to_owned(),
                },
            },
        ],
    }];

    let outcome = relations(&b, &store);
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
        derived_from: vec![
            CitedSource {
                digest: r.clone(),
                locator: AcquisitionLocator::InPullRequest {
                    repository: "PhysShell/007".to_owned(),
                    pull_request: "9001".to_owned(),
                    stable_id: "0".to_owned(),
                },
            },
            CitedSource {
                digest: owning.clone(),
                locator: AcquisitionLocator::InPullRequest {
                    repository: "PhysShell/007".to_owned(),
                    pull_request: "9001".to_owned(),
                    stable_id: "0".to_owned(),
                },
            },
        ],
    }];

    admitted(&relations(&b, &store));
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
        derived_from: vec![CitedSource {
            digest: r.clone(),
            locator: AcquisitionLocator::InPullRequest {
                repository: "PhysShell/007".to_owned(),
                pull_request: "9001".to_owned(),
                stable_id: "0".to_owned(),
            },
        }],
    }];

    let outcome = relations(&b, &store);
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

    let outcome = relations(
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
    // Both assessments are real and retained — the question is which one
    // authorised THIS record, and the basis does not get to choose.
    let retained = store.bind_to_another_assessment(&d);
    let asserted = format!("sha256:{}", "2".repeat(64));
    assert_ne!(retained, asserted);

    let mut b = basis("review/external", vec![reads(&d, "/commitId")]);
    b.bindings = vec![DeclaredBinding {
        record_digest: d.clone(),
        assessment_digest: asserted.clone(),
    }];

    let outcome = relations(&b, &store);
    let why = cannot_check(&outcome);
    assert!(
        why.iter().any(|u| matches!(
            u,
            Unresolved::BindingMismatch { declared, retained: r, .. }
                if declared == &asserted && r == &retained
        )),
        "got {why:?}"
    );
}

/// The digest §13.1 binds `review-by-expected-author-login/1` to, taken from the
/// registry that binds it rather than copied as a literal that can go stale.
fn bound_implementation_digest() -> String {
    let entry = o7_closure_matcher::resolve("review-by-expected-author-login", "1")
        .expect("the matcher is registered");
    o7_closure_matcher::verify_implementation(entry)
        .expect("the registry is bound to its own implementation")
        .as_str()
        .to_owned()
}
