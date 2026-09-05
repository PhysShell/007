//! The derivation source view — what a rule is allowed to see, and what it is
//! told when it cannot see it.
//!
//! `correction_b4d.rs` establishes the property: the same derivation reproduces
//! the same fact over a complete §8 projection and over a reduced source record
//! whose retained fields include everything the rule reads. This file carries
//! the parts of the mechanism that property does not reach on its own.
//!
//! ```text
//! 1  the declaration is a fact about the contracts, not a local convenience
//! 2  a cited source count the rule does not take is a CLAIM defect
//! 3  inputs that resolved but cannot be used is not the negative answer
//! ```
//!
//! WHY 1 IS HERE. `DerivationInput` names each field it reads twice — once in
//! §8's canonical vocabulary, once in §5.3's decoded one — because redaction
//! §7.5 records that the two are different name sets and refuses a global
//! correspondence table between them. Per-derivation declaration is how that
//! refusal is honoured, and it is only sound while each declared name really is
//! a name one of the contracts gives. A typo in the canonical half is caught by
//! execution (D1 and D2 both go red); a typo in the decoded half is caught by
//! execution (D2 goes red); a name that is plausible in both spaces and wrong in
//! the contract is caught here and nowhere else.

// Justification for the restriction-lint allowance, per AGENTS.md rule 4 and the
// precedent in `tests/derivation_binding.rs`: every panic path below is this
// test's own assertion failing, or its own parse of a JSON literal written in
// this file. Nothing here runs against production input.
// Extent (checked by N1): 2 `expect` sites, 1 `panic` site.
#![allow(clippy::expect_used, clippy::panic)]

use o7_closure_matcher::{Member, ValueKind, SOURCE_SCHEMAS};
use o7_closure_provenance::derivations::REGISTRY;
use o7_closure_provenance::redaction::REQUIRED_FIELDS;
use o7_closure_provenance::{
    relations_checked, AcquisitionLocator, Admissible, CitedSource, DecisionBasis, DerivedFact,
    ExpectedDetector, RetainedEvidence, Unresolved,
};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

mod common;

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

// ---- 1. The declaration names fields the contracts actually define.

/// Walk a §8 projection's member tree by RFC 6901 pointer.
fn member_at<'a>(members: &'a [Member], pointer: &str) -> Option<&'a Member> {
    let segments: Vec<String> = pointer
        .strip_prefix('/')?
        .split('/')
        .map(|s| s.replace("~1", "/").replace("~0", "~"))
        .collect();
    let (head, rest) = segments.split_first()?;
    let found = members.iter().find(|m| m.name == *head)?;
    if rest.is_empty() {
        return Some(found);
    }
    match found.kind {
        ValueKind::Object(nested) => member_at(nested, &format!("/{}", rest.join("/"))),
        _ => None,
    }
}

/// A slot's declared kind is a surface both contracts define.
///
/// The `kind` field is the expected side of the G1 slot check, and an expected
/// value nothing audits is a typo waiting to admit everything: a slot declaring
/// `"github-submited-review"` matches no artifact and refuses every citation,
/// which reads as a strict rule rather than as a broken one.
///
/// Checked against BOTH tables because the field is matched against
/// `/sourceKind` on one path and `/locatorKind` on the other.
#[test]
fn every_declared_slot_kind_is_a_surface_both_contracts_define() {
    for entry in REGISTRY {
        for (position, source) in entry.sources.iter().enumerate() {
            assert!(
                SOURCE_SCHEMAS.iter().any(|s| s.source_kind == source.kind),
                "{}/{} slot {position}: {:?} is not an §8 sourceKind. A slot expecting a \
                 surface that does not exist refuses every citation and looks strict doing it",
                entry.id,
                entry.version,
                source.kind,
            );
            assert!(
                REQUIRED_FIELDS
                    .iter()
                    .any(|r| r.locator_kind == source.kind),
                "{}/{} slot {position}: {:?} is not a §5.3 locatorKind. The slot check compares \
                 one name against /locatorKind for a reduced record, so a kind absent from that \
                 table refuses every reduced record in this slot — buying G1 back by forbidding \
                 the representation redaction §8 requires to stay usable",
                entry.id,
                entry.version,
                source.kind,
            );
        }
    }
}

/// Every canonical input is a member of THE SLOT'S OWN projection.
///
/// Was "a member of SOME projection", which is the weakest form of the same
/// question and a proxy of exactly the kind G1 is about. It admits a slot
/// declaring `kind: "github-submitted-review"` alongside
/// `canonical: "/pullRequestReviewId"` — a field no submitted review carries —
/// because some OTHER projection defines that name. The slot now names its
/// surface, so the audit can ask the question it always meant.
#[test]
fn every_declared_canonical_input_is_a_member_of_its_slots_projection() {
    for entry in REGISTRY {
        for (position, source) in entry.sources.iter().enumerate() {
            for input in source.inputs {
                assert!(
                    input.canonical.starts_with('/'),
                    "{}/{} source {position}: {:?} is not a pointer, so no view can be built \
                     from it",
                    entry.id,
                    entry.version,
                    input.canonical
                );
                let schema = SOURCE_SCHEMAS
                    .iter()
                    .find(|s| s.source_kind == source.kind)
                    .expect("the slot kind is an §8 sourceKind; the audit above holds that");
                assert!(
                    member_at(schema.members, input.canonical).is_some(),
                    "{}/{} slot {position} takes a {}, and {:?} is not a member of that \
                     projection. The rule would read this field out of the surface the slot \
                     admits and find nothing there — every citation refused as evidence loss, \
                     for a field the surface was never going to carry",
                    entry.id,
                    entry.version,
                    source.kind,
                    input.canonical
                );
            }
        }
    }
}

/// And every decoded input is a §5.3 required field of THE SLOT'S OWN locator
/// kind — the reduced-representation half of the same tightening.
#[test]
fn every_declared_decoded_input_is_a_required_field_of_its_slots_locator_kind() {
    for entry in REGISTRY {
        for (position, source) in entry.sources.iter().enumerate() {
            for input in source.inputs {
                let required = REQUIRED_FIELDS
                    .iter()
                    .find(|r| r.locator_kind == source.kind)
                    .expect("the slot kind is a §5.3 locatorKind; the audit above holds that");
                assert!(
                    required.always.contains(&input.decoded)
                        || required
                            .present_only
                            .iter()
                            .any(|f| f.decoded == input.decoded),
                    "{}/{} slot {position} takes a {}, and {:?} is not in that kind's §5.3 \
                     required set. §7.1 builds `retainedFields` out of that set and nothing \
                     else, so a reduced record of the surface this slot admits can never carry \
                     the name — and the reduced half of the declaration is dead on the only \
                     records it applies to",
                    entry.id,
                    entry.version,
                    source.kind,
                    input.decoded
                );
            }
        }
    }
}

/// The two halves are not interchangeable, and a declaration that used one
/// vocabulary for both would pass the two checks above while breaking one
/// representation.
///
/// Asserted for the registry as it stands rather than in general: §7.5's
/// asymmetry is real but not total — a field GitHub and the projection happen to
/// spell alike would legitimately declare the same name twice. This refuses the
/// specific way that assertion goes wrong, which is a declaration where the
/// decoded half was filled in by copying the canonical one.
#[test]
fn no_declared_input_uses_the_canonical_name_in_the_decoded_half() {
    for entry in REGISTRY {
        for source in entry.sources {
            for input in source.inputs {
                if input.canonical == input.decoded {
                    continue;
                }
                assert!(
                    !SOURCE_SCHEMAS
                        .iter()
                        .any(|schema| member_at(schema.members, input.decoded).is_some()),
                    "{}/{}: {:?} is a §8 projection member standing in the §5.3 slot",
                    entry.id,
                    entry.version,
                    input.decoded
                );
            }
        }
    }
}

// ---- 2 and 3. The refusals, over a store.

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

fn block_secret(assessed: &[&str], blocked: &[&str]) -> Value {
    let findings: Vec<Value> = blocked
        .iter()
        .map(|f| json!({"field": f, "findingId": "rule-aws-key"}))
        .collect();
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
        "outcome": "BLOCK_SECRET",
        "findings": findings,
        "observedAt": "2026-08-05T09:03:00Z",
    })
}

/// A reduced record whose `/id` is retained as whatever `id_value` says.
fn reduced(locator_kind: &str, required: &[&str], stable_id: &str, id_value: Value) -> Value {
    let mut retained = Map::new();
    for pointer in required {
        if *pointer == "/body" {
            continue;
        }
        let value = match *pointer {
            "/id" => id_value.clone(),
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

fn basis(sources: Vec<String>) -> DecisionBasis {
    DecisionBasis {
        expected_redaction_policy: "1".to_owned(),
        expected_detector: ExpectedDetector {
            id: "synthetic-detector".to_owned(),
            version: "1".to_owned(),
            config_digest:
                "sha256:1111111111111111111111111111111111111111111111111111111111111111".to_owned(),
        },
        observation_id: "review/external".to_owned(),
        inputs: Vec::new(),
        derived: vec![DerivedFact {
            derivation: "review-carries-finding".to_owned(),
            version: "1".to_owned(),
            value: json!(true),
            derived_from: sources
                .into_iter()
                .enumerate()
                .map(|(slot, digest)| CitedSource {
                    digest,
                    locator: AcquisitionLocator::InPullRequest {
                        repository: "PhysShell/007".to_owned(),
                        pull_request: "9001".to_owned(),
                        // §18's slot order: the submitted review, then the
                        // review comment. The citation names which object each
                        // slot asked for, and getting that from the record
                        // would be the record naming itself.
                        stable_id: if slot == 0 {
                            REVIEW_ID.to_owned()
                        } else {
                            COMMENT_ID.to_owned()
                        },
                    },
                })
                .collect(),
        }],
        expected_query: None,
        bindings: Vec::new(),
    }
}

fn why(outcome: &Admissible) -> Vec<Unresolved> {
    match outcome {
        Admissible::CannotCheck { why } => why.clone(),
        Admissible::Yes { .. } => panic!("expected a refusal, got {outcome:?}"),
    }
}

/// A fact citing a source count the rule does not take is refused as a claim
/// defect, not as a disagreement.
///
/// The arity check has existed since the derivation registry did, and reported
/// `DerivationDisagrees { recomputed: Null }` — the spelling this round removed
/// precisely because it made a claim defect, an evidence loss and a contradicted
/// value all read alike. A rule with no witness is a rule nobody has seen fail,
/// so it gets one here rather than a new name and continued silence.
#[test]
fn a_fact_citing_the_wrong_number_of_sources_is_an_arity_mismatch() {
    let mut store = Store::default();
    let r = store.retain_under(
        &reduced(
            "github-submitted-review",
            &REVIEW_REQ,
            REVIEW_ID,
            json!(REVIEW_ID),
        ),
        &block_secret(&REVIEW_REQ, &["/body"]),
    );

    let outcome = relations(&basis(vec![r]), &store);
    assert!(
        why(&outcome).iter().any(|u| matches!(
            u,
            Unresolved::DerivationArityMismatch { expected, cited, .. }
                if *expected == 2 && *cited == 1
        )),
        "got {outcome:?}"
    );
}

/// A blocked input is reported as evidence loss, alongside the pointer refusal
/// that says which field and why.
///
/// `correction_b4d`'s D3 establishes that the outcome is `CannotCheck`. This
/// establishes what the reader is told, which is the half that was wrong before:
/// the same situation used to arrive as `DerivationDisagrees` with a null
/// recomputation, i.e. as though the sources had been read and had contradicted
/// the claim.
#[test]
fn a_blocked_input_is_reported_as_evidence_loss_and_names_the_field() {
    let mut store = Store::default();
    let record = json!({
        "schemaVersion": 1,
        "sourceKind": "github-reduced-source-record",
        "locatorKind": "github-submitted-review",
        "locator": {
            "repository": "PhysShell/007", "pullRequest": "9001", "stableId": REVIEW_ID,
        },
        "redactionPolicyVersion": "1",
        "outcome": "BLOCK_SECRET",
        "coverageComplete": true,
        "retainedFields": {
            "/author_association": "NONE", "/commit_id": "c", "/state": "CHANGES_REQUESTED",
            "/submitted_at": "2026-08-05T09:02:47Z", "/user/id": "1", "/user/login": "l",
            "/user/type": "User",
        },
        "blockedFields": ["/body", "/id"],
    });
    let r = store.retain_under(&record, &block_secret(&REVIEW_REQ, &["/body", "/id"]));
    let c = store.retain_under(
        &reduced(
            "github-review-comment",
            &COMMENT_REQ,
            COMMENT_ID,
            json!(COMMENT_ID),
        ),
        &block_secret(&COMMENT_REQ, &["/body"]),
    );

    let refusals = why(&relations(&basis(vec![r.clone(), c]), &store));
    assert!(
        refusals.iter().any(|u| matches!(
            u,
            Unresolved::PointerBlocked { pointer, .. } if pointer == "/id"
        )),
        "the refusal must name the field that was lost: {refusals:?}"
    );
    assert!(
        refusals.iter().any(|u| matches!(
            u,
            Unresolved::DerivationInputUnavailable { source_digest, .. } if *source_digest == r
        )),
        "and must say that this is what cost the derivation its answer: {refusals:?}"
    );
    assert!(
        !refusals
            .iter()
            .any(|u| matches!(u, Unresolved::DerivationDisagrees { .. })),
        "nothing disagreed — no source was read: {refusals:?}"
    );
}

/// Every declared input survived, and the rule still cannot use them.
///
/// `retainedFields` is an open object: §7.1 fixes its KEYS from §5.3 and says
/// nothing about the values, so a record can retain `/id` as a number. Every
/// input the rule reads is then present and none of them is what it reads. That
/// is neither a retention loss nor a disagreement, and reporting it as either
/// would send a reader to the wrong half of the system.
#[test]
fn inputs_that_survived_but_cannot_be_used_are_not_a_negative_result() {
    let mut store = Store::default();
    let r = store.retain_under(
        &reduced(
            "github-submitted-review",
            &REVIEW_REQ,
            REVIEW_ID,
            // Retained, accounted for, and not a string.
            json!(9_000_000_901_i64),
        ),
        &block_secret(&REVIEW_REQ, &["/body"]),
    );
    let c = store.retain_under(
        &reduced(
            "github-review-comment",
            &COMMENT_REQ,
            COMMENT_ID,
            json!(COMMENT_ID),
        ),
        &block_secret(&COMMENT_REQ, &["/body"]),
    );

    let refusals = why(&relations(&basis(vec![r, c]), &store));
    assert!(
        refusals
            .iter()
            .any(|u| matches!(u, Unresolved::DerivationCannotRecompute { .. })),
        "got {refusals:?}"
    );
    assert!(
        !refusals.iter().any(|u| matches!(
            u,
            Unresolved::DerivationDisagrees { .. } | Unresolved::DerivationInputUnavailable { .. }
        )),
        "nothing was lost and nothing disagreed: {refusals:?}"
    );
}
