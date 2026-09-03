//! RED-B4R — relation and authority qualification.
//!
//! B4 established a real property and this preregistration does not withdraw it:
//!
//! > For the artifact kinds included in B4's denominator, a raw or structurally
//! > malformed artifact cannot reach downstream semantics without first becoming
//! > a `ValidatedArtifact`.
//!
//! That remains true. What does not survive is the wider claim GREEN-B4's commit
//! message made — *the class of defect stops being expressible* — because there
//! are two classes and B4 turned only the first into a construction:
//!
//! ```text
//! artifact validity          relation validity
//!   bytes                      subject
//!   digest                     role
//!   kind                       state
//!   closed structure           authority
//!        |                     replay
//!        v                     claim/evidence agreement
//!  ValidatedArtifact                |
//!                                   v
//!                        evidence fit for THIS decision
//! ```
//!
//! The contract said so first. §17.1 separates artifact validity from relation
//! validity and lists, among the latter, whether the artifact's *state* supports
//! this claim. B4 made clause 1 a door; clause 2 is still a set of procedures
//! somebody has to remember to call.
//!
//! CX-2 IS THE DIAGNOSTIC ONE. A query snapshot recording `INCOMPLETE` is
//! *supposed* to be structurally valid — §13 says so outright, and specimen D
//! exists to keep it representable. Only the decision consuming it as evidence
//! of `NotProduced` must reject it. And `085fd01` demonstrates the split
//! perfectly: `admissibility` resolves `expected_query_digest` through the
//! structural door and stops, while the separate scan path remembers to inspect
//! `/enumeration`. Same artifact, two consumers, one remembered clause 2 and one
//! did not.
//!
//! THE PROPERTY THIS PREREGISTERS:
//!
//! > A structurally validated artifact cannot support a decision merely because
//! > it is valid. Every consumption role must produce a role-qualified witness
//! > whose state and relations support that exact decision.
//!
//! ```text
//! ValidatedArtifact
//!         |
//!         v  qualify_for_role(independent context, retained evidence)
//!         v    required state · subject relation · matcher replay
//!         v    recorded claim agrees · authority chain
//!         |
//!  role-qualified evidence
//!         |
//!         v
//!  Admissible::Yes | ZeroClaimsMeaningful | NotStale
//! ```
//!
//! The Rust type does not matter. The property does: a plain `ValidatedArtifact`
//! must cease to be sufficient input to anything able to produce one of those
//! three answers when the answer depends on a relation structural validation
//! cannot establish.
//!
//! WHAT MUST NOT BE THE FIX. `if enumeration != "COMPLETE"` inside
//! `admissibility` would close CX-2 and preserve the design that produced it.
//! The absence path and the scan path must consume the SAME qualification
//! primitive, and `o7-closure-matcher` already has the expensive pieces —
//! `RecordedQuerySnapshot`, `recompute_matched`, `verify_matched`, the last of
//! which compares recomputation against the claim stored in the artifact rather
//! than one taken loosely from the caller.
//!
//! ```text
//! D1  every kind the CONTRACT declares is one this crate can admit
//! D2  every kind this crate names is one the contract declares
//!
//! R1  assessedFields outside the §5.3 universe is malformed authorization
//! R2  a present-only field present-and-unassessed under complete coverage
//! R3  a present-only field absent upstream stays representable   BOUNDARY
//!
//! R4  an INCOMPLETE snapshot does not evidence an absence claim
//! R5  an absence claim whose matcher replay does not reproduce is refused
//! R6  an absence claim whose candidates do not resolve is refused
//! R7  the scan path and the absence path refuse the same artifact
//!
//! R8  zero claims over a non-empty replayed matched set is refused
//! R9  zero claims over an empty replayed matched set stays meaningful  BOUNDARY
//!
//! R10 a binding with no retained bytes authorises nothing
//! R11 a binding whose bytes are not a conforming §9.5 form is refused
//! R12 a binding of an unregistered schemaVersion or sourceKind is refused
//! ```
//!
//! WHAT THIS PREREGISTRATION DOES NOT CLAIM. Slice B can establish that a
//! conforming retained binding about this record exists and leads to a
//! conforming assessment that authorises it. It cannot establish WHEN or BY WHOM
//! those retained bytes were produced. Authentication remains the later
//! attestation layer, and no witness here should be read as reaching it.
//!
//! DET-BIND stays out. Whether `findingId` belongs to the detector configuration
//! remains OWED. Whether `/zz/<secret>` is a field this gate may claim it
//! assessed is decidable today, from §5.2 and §5.3 alone.

// Justification for the restriction-lint allowance, per AGENTS.md rule 4: most
// `expect` and `panic` sites below are this file's own handling of JSON literals
// written in it, each unreachable unless a specimen a few lines above is
// malformed. A broken specimen must fail loudly rather than weaken a witness.
// TWO SITES ARE NOT THAT and are named here rather than left to borrow that
// invariant: the two reads of the artifact module's source from disk. A source
// file this check cannot read is a check reporting green over text it never
// opened. N1 records why the exception is named rather than assumed.
// Extent (checked by N1): 4 `expect` sites, 3 `panic` sites.
#![allow(clippy::expect_used, clippy::panic)]

use o7_closure_matcher::{
    check_recorded_implementation, resolve as resolve_matcher, ImplementationCheck,
    RecordedQuerySnapshot,
};
use o7_closure_provenance::{
    relations_checked, scan_verdict, AcquisitionLocator, Admissible, DecisionBasis, DecisionInput,
    ExpectedDetector, ExpectedQuery, FalsificationSurfaceScan, QueryBinding, RetainedEvidence,
    ScanCompleteness, ScanVerdict,
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

const PROVENANCE: &str = include_str!("../../../docs/architecture/closure-source-provenance-v1.md");
const REDACTION: &str = include_str!("../../../docs/architecture/closure-redaction-policy-v1.md");

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
const CHECK_ALWAYS: [&str; 4] = ["/head_sha", "/id", "/name", "/status"];

// ---- Store.

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
    /// Retain record + assessment, and RETAIN the binding object too.
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
    /// Bind with binding bytes the caller chose, retained or not.
    fn bind_raw(&mut self, record_digest: &str, binding: Value) {
        self.bindings.insert(record_digest.to_owned(), binding);
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

// ---- Fixtures.

fn assessment(assessed: &[&str], outcome: &str, findings: Option<Value>) -> Value {
    let mut a = json!({
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
        "outcome": outcome,
        "observedAt": "2026-08-05T09:03:00Z",
    });
    if let Some(f) = findings {
        a.as_object_mut()
            .expect("assessment is an object")
            .insert("findings".to_owned(), f);
    }
    a
}

fn review(login: &str) -> Value {
    json!({
        "schemaVersion": 1,
        "sourceKind": "github-submitted-review",
        "stableId": "9000000901",
        "user": {"id": "9000000901", "login": login, "type": "User"},
        "authorAssociation": "NONE",
        "state": "CHANGES_REQUESTED",
        "body": "still unreachable\n",
        "submittedAt": "2026-08-05T09:02:47Z",
        "commitId": "1f2e3d4c5b6a798807162534435261708f9e0d1c",
    })
}

const MATCHER_ID: &str = "review-by-expected-author-login";
const MATCHER_VERSION: &str = "1";

/// The digest §13.1 binds `review-by-expected-author-login/1` to, taken from the
/// registry that binds it.
///
/// NOT a literal copied into this file. A hand-copied digest that has gone stale
/// makes every snapshot below malformed, and a preregistered witness that fails
/// because its fixture is malformed is a witness that proves nothing about the
/// property it names.
fn bound_implementation_digest() -> String {
    common::bound_matcher_digest(MATCHER_ID, MATCHER_VERSION)
}

/// A §13 query snapshot at `schemaVersion` 2, carrying the implementation digest
/// §13.1 requires at that version.
///
/// WHY VERSION 2 AND NOT 1, and this is a fixture correction rather than a
/// change of denominator. Every witness below is about what a consumer does with
/// a snapshot whose *relations* do not support the decision — an INCOMPLETE
/// enumeration, a candidate set that contradicts the claim, a matched set the
/// caller reported as zero. A version-1 snapshot carries no
/// `matcher.implementationDigest`, so replay reaches
/// `ImplementationCheck::CannotCheck` and the consumer has an earlier, unrelated
/// reason to refuse. The witnesses would then be red for a reason that is not
/// the property, and a GREEN round could close them by leaving the property
/// untouched.
///
/// So the snapshots here are made as strong as §13 allows: everything an
/// implementation binding can establish IS established, and the only thing left
/// wrong is the relation each witness is about.
fn snapshot(enumeration: &str, all: &[&str], matched: &[&str]) -> Value {
    let mut s = json!({
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
        "enumeration": enumeration,
        "matcher": {
            "id": MATCHER_ID,
            "version": MATCHER_VERSION,
            "implementationDigest": bound_implementation_digest(),
            "parameters": {"expectedAuthorLogin": "synthetic-external-reviewer"},
        },
        "allReturnedSnapshotDigests": all,
        "matchedSnapshotDigests": matched,
    });
    if enumeration == "INCOMPLETE" {
        s.as_object_mut()
            .expect("snapshot is an object")
            .insert("incompleteReason".to_owned(), json!("page 2 fetch 502"));
    }
    s
}

/// Every query snapshot this file builds reaches `ImplementationCheck::Bound`.
///
/// The guard on the paragraph above, and it is executed rather than asserted in
/// prose: `CannotCheck` is a *silent* pass-through — replay returns it happily —
/// so a fixture that quietly lost its `implementationDigest` would still run,
/// still refuse, and still look like a preregistered witness turning red.
///
/// It refuses the other direction too: a digest present and WRONG fails here
/// with a drift error, which is the same defect wearing the opposite sign.
///
/// The snapshot goes through `from_canonical` first, so this checks the artifact
/// these witnesses actually build rather than a matcher block assembled beside
/// it. Candidates are not involved — the implementation binding is a property of
/// the matcher block, and requiring the full candidate set would make the guard
/// depend on the very lists each witness deliberately varies.
#[track_caller]
fn assert_implementation_is_bound(snapshot: &Value, what: &str) {
    let d = common::digest_of(snapshot);
    let recorded = RecordedQuerySnapshot::from_canonical(snapshot, &d)
        .unwrap_or_else(|e| panic!("{what}: the fixture is not a conforming §13 snapshot: {e}"));
    let matcher = recorded.recorded_matcher();
    let entry = resolve_matcher(matcher.id(), matcher.version())
        .unwrap_or_else(|e| panic!("{what}: the fixture names an unregistered matcher: {e}"));
    let status = check_recorded_implementation(entry, matcher.implementation())
        .unwrap_or_else(|e| panic!("{what}: the fixture's implementation binding is wrong: {e}"));
    assert!(
        matches!(status, ImplementationCheck::Bound { .. }),
        "{what}: the fixture reaches {status:?}, so a consumer could refuse it for a reason \
         that is not the relation this witness is about"
    );
}

/// Every shape the witnesses below ask the helper for, checked once.
///
/// A per-witness assertion would cover only the snapshots some witness happened
/// to build; this covers the helper, which is where the defect was.
#[test]
fn every_query_fixture_carries_a_bound_implementation() {
    let candidate = format!("sha256:{}", "a".repeat(64));
    let fixtures = [
        (
            "INCOMPLETE, no candidates",
            snapshot("INCOMPLETE", &[], &[]),
        ),
        ("COMPLETE, no candidates", snapshot("COMPLETE", &[], &[])),
        (
            "COMPLETE, a candidate and an empty matched set",
            snapshot("COMPLETE", &[&candidate], &[]),
        ),
        (
            "COMPLETE, a matched candidate",
            snapshot("COMPLETE", &[&candidate], &[&candidate]),
        ),
    ];
    for (what, fixture) in &fixtures {
        assert_implementation_is_bound(fixture, what);
    }
}

fn absence_basis(expected: &str) -> DecisionBasis {
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
        derived: Vec::new(),
        expected_query: Some(ExpectedQuery {
            digest: expected.to_owned(),
            subject: QueryBinding {
                repository: "PhysShell/007".to_owned(),
                pull_request: "9001".to_owned(),
            },
        }),
        bindings: Vec::new(),
    }
}

fn basis(record: &str, pointer: &str) -> DecisionBasis {
    DecisionBasis {
        expected_redaction_policy: "1".to_owned(),
        expected_detector: ExpectedDetector {
            id: "synthetic-detector".to_owned(),
            version: "1".to_owned(),
            config_digest:
                "sha256:1111111111111111111111111111111111111111111111111111111111111111".to_owned(),
        },
        observation_id: "review/external".to_owned(),
        inputs: vec![DecisionInput {
            source_digest: record.to_owned(),
            pointer: pointer.to_owned(),
            locator: AcquisitionLocator::Check {
                repository: "PhysShell/007".to_owned(),
                stable_id: "9100000201".to_owned(),
            },
        }],
        derived: Vec::new(),
        expected_query: None,
        bindings: Vec::new(),
    }
}

/// A candidate belonging to a relation-specific witness carries §9.2 authority.
///
/// THE SECOND FIXTURE GUARD IN THIS ROUND, and it exists because the first one's
/// lesson generalised. R5 and R8 originally built their candidates with `put`
/// rather than `retain_under`, so neither had a `RetentionBinding` — and
/// `qualify_query` did not check candidate authority, so nobody noticed. The
/// correct fix for that (RED-B4R.2's Q1) would have turned both witnesses green
/// for candidate authority rather than for the replay disagreement and the
/// zero-versus-non-empty contradiction they actually name.
///
/// The oracle is deliberately NOT `qualify_query`: it asks the one consumer
/// whose §9.2 chain GREEN-B4R already closed — reading a pointer out of the
/// record as a gated source — so it means the same thing before and after
/// candidate authority is implemented.
#[track_caller]
fn assert_candidate_is_authorised(store: &Store, candidate: &str, what: &str) {
    let outcome = relations(&basis(candidate, "/body"), store);
    assert!(
        matches!(outcome, Admissible::Yes { .. }),
        "{what}: the candidate does not carry §9.2 authority, so this witness would go green \
         on candidate authority rather than on the relation it names: got {outcome:?}"
    );
}

#[track_caller]
fn refuses(outcome: &Admissible, what: &str) {
    assert!(
        matches!(outcome, Admissible::CannotCheck { .. }),
        "{what}: admitted, and structural validity was never the question here"
    );
}

// ---- D. The denominator, taken from the contract.

/// Every `sourceKind` the two contracts declare, as a set.
///
/// A declaration is a `github-*` or `closure-*` token on a line that mentions
/// `sourceKind`. That excludes `schemaVersion  sourceKind  stableId`, which is a
/// member list rather than a value.
fn contract_declared_kinds() -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for doc in [PROVENANCE, REDACTION] {
        // A markdown table row is prose ABOUT the contract, not the contract.
        // Without this the acceptance-criteria row that mentions
        // `github-head-read-event` alongside the word `sourceKind` would count as
        // a declaration, and the parser would confirm whatever the last DOC pass
        // happened to write down.
        for line in doc
            .lines()
            .filter(|l| l.contains("sourceKind") && !l.trim_start().starts_with('|'))
        {
            for token in line.split(|c: char| !(c.is_ascii_alphanumeric() || c == '-')) {
                let token = token.trim_matches('-');
                if (token.starts_with("github-") || token.starts_with("closure-"))
                    && token.len() > 8
                    && !out.iter().any(|k| k == token)
                {
                    out.push(token.to_owned());
                }
            }
        }
    }
    out.sort();
    out
}

/// D1 — the denominator must come from the CONTRACT, not from the enum.
///
/// GREEN-B4's "every artifact kind" guard ranged over `ArtifactKind`, so its
/// strongest property was that the implementation had not forgotten anything it
/// had remembered to enumerate. `closure-retention-binding` is declared by
/// redaction §9.2 with a complete §9.5 shape and was simply not in the enum, so
/// no amount of mutation testing over that enum could ever have found it.
///
/// This witness must fail if `closure-retention-binding` is ever dropped from
/// the contract-to-implementation mapping again.
#[test]
fn d1_every_contract_declared_kind_can_be_admitted() {
    let declared = contract_declared_kinds();
    assert!(
        declared.len() >= 9,
        "parsed only {} kinds from the contracts, fewer than they have ever declared: \
         {declared:?}",
        declared.len()
    );
    assert!(
        declared.iter().any(|k| k == "closure-retention-binding"),
        "the parser stopped seeing §9.2's binding; fix the parser, not the expectation"
    );

    let source = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/artifact.rs"))
        .expect("reading the artifact module");
    let unhandled: Vec<&String> = declared
        .iter()
        .filter(|kind| !source.contains(kind.as_str()))
        .collect();
    assert!(
        unhandled.is_empty(),
        "the contract declares {} kind(s) the artifact door does not name: {unhandled:?}. A \
         denominator drawn from the implementation can only ever confirm that the \
         implementation remembers itself",
        unhandled.len()
    );
}

/// D2 — and the mapping is bidirectional.
///
/// `github-head-read-event` is named by this crate and by nothing in either
/// contract: §8.1 describes the event's two shapes without ever declaring a
/// `sourceKind` for it, so the NAME is this implementation's invention. That is
/// the same defect as D1 pointing the other way — a kind that exists because
/// somebody wrote it down here rather than because the contract defines it.
///
/// Resolution is the owner's: either §8.1 gains the declaration, or the
/// exception is recorded deliberately. It is not resolved by widening this test.
#[test]
fn d2_every_kind_this_crate_names_is_contract_declared() {
    let source = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/artifact.rs"))
        .expect("reading the artifact module");
    let declared = contract_declared_kinds();

    let mut named: Vec<String> = Vec::new();
    for token in source.split(|c: char| !(c.is_ascii_alphanumeric() || c == '-')) {
        if (token.starts_with("github-") || token.starts_with("closure-"))
            && token.len() > 8
            && !named.iter().any(|k| k == token)
        {
            named.push(token.to_owned());
        }
    }
    let undeclared: Vec<&String> = named
        .iter()
        .filter(|k| !declared.iter().any(|d| d == *k))
        .collect();
    assert!(
        undeclared.is_empty(),
        "this crate names {} kind(s) neither contract declares: {undeclared:?}",
        undeclared.len()
    );
}

// ---- R1-R3. CX-1: the assessment's field universe.

/// R1 — §5.2 makes §5.3 the normative denominator, and §9.4 requires every
/// assessment field to have a value range independent of inspected content.
///
/// `/zz/ghp_…` is syntactically a JSON pointer and is not a permissible
/// structural identifier for this assessment, because its value was chosen from
/// the content being inspected. It sorts after `/user/type`, so §5.5's ordering
/// rule is satisfied and the string rides into a permanently retained
/// closed-schema artifact.
///
/// The minimum invariant: `assessedFields ⊆ always ∪ present_only` for the
/// record kind. Anything outside that maximum normative universe is malformed
/// authorization.
#[test]
fn r1_assessed_fields_outside_the_required_universe_are_refused() {
    let mut store = Store::default();
    let mut assessed = REVIEW_REQ.to_vec();
    assessed.push("/zz/ghp_the_credential_riding_in_a_closed_schema");
    assessed.sort_unstable();
    let d = store.retain_under(
        &review("synthetic-external-reviewer"),
        &assessment(&assessed, "RETAIN", None),
    );
    refuses(&relations(&basis(&d, "/body"), &store), "R1");
}

/// R2 — BOUNDARY, and it already holds. A present-only field that IS present and
/// was NOT assessed, under `coverageComplete: true`, is refused today — not by a
/// coverage rule but by §7.1's "a field the detector never successfully assessed
/// is blocked, always", since the record retains it. Preregistered anyway
/// because the owner named this pair explicitly and because a GREEN that
/// reworks the coverage rules could lose it without noticing.
///
/// Scoped to a reduced record on purpose. §7.1 builds the partition from
/// `always ∪ (present-only actually present)`, so a reduced record's own
/// `retainedFields ∪ blockedFields` establishes which present-only fields were
/// there. For a COMPLETE projection it does not: §5.3's present-only set is in
/// decoded space, the projection carries canonical members, and §7.5 forbids
/// inventing the correspondence. That limitation is recorded in R3 rather than
/// papered over with a check that cannot see what it claims to.
#[test]
fn r2_a_present_field_left_unassessed_under_complete_coverage_is_refused() {
    let mut store = Store::default();
    // /conclusion is present-only for github-actions-check and this record
    // accounts for it — so it was present upstream — yet it is not assessed.
    let mut retained = Map::new();
    for p in CHECK_ALWAYS.iter().filter(|p| **p != "/name") {
        retained.insert((*p).to_owned(), json!("v"));
    }
    retained.insert("/conclusion".to_owned(), json!("success"));
    let record = json!({
        "schemaVersion": 1,
        "sourceKind": "github-reduced-source-record",
        "locatorKind": "github-actions-check",
        "locator": {"repository": "PhysShell/007", "stableId": "9100000201"},
        "redactionPolicyVersion": "1",
        "outcome": "BLOCK_SECRET",
        "coverageComplete": true,
        "retainedFields": Value::Object(retained),
        "blockedFields": ["/name"],
    });
    let a = assessment(
        &CHECK_ALWAYS,
        "BLOCK_SECRET",
        Some(json!([{"field": "/name", "findingId": "rule-aws-key"}])),
    );
    let d = store.retain_under(&record, &a);
    refuses(&relations(&basis(&d, "/head_sha"), &store), "R2");
}

/// R3 — BOUNDARY. A present-only field absent upstream, and therefore
/// unassessed, must stay representable. §5.3: absent means nothing to assess.
///
/// This passes today and must keep passing. Refusing it would make every
/// conformant check-run record with no `conclusion` yet inadmissible, which is
/// how a coverage rule turns into an outage.
#[test]
fn r3_a_present_only_field_absent_upstream_stays_representable() {
    let mut store = Store::default();
    let mut retained = Map::new();
    for p in CHECK_ALWAYS.iter().filter(|p| **p != "/name") {
        retained.insert((*p).to_owned(), json!("v"));
    }
    let record = json!({
        "schemaVersion": 1,
        "sourceKind": "github-reduced-source-record",
        "locatorKind": "github-actions-check",
        "locator": {"repository": "PhysShell/007", "stableId": "9100000201"},
        "redactionPolicyVersion": "1",
        "outcome": "BLOCK_SECRET",
        "coverageComplete": true,
        "retainedFields": Value::Object(retained),
        "blockedFields": ["/name"],
    });
    let a = assessment(
        &CHECK_ALWAYS,
        "BLOCK_SECRET",
        Some(json!([{"field": "/name", "findingId": "rule-aws-key"}])),
    );
    let d = store.retain_under(&record, &a);
    assert!(
        matches!(
            relations(&basis(&d, "/head_sha"), &store),
            Admissible::Yes { .. }
        ),
        "R3: a present-only field that was never there is not a coverage hole"
    );
}

// ---- R4-R7. CX-2: qualification for the absence-evidence role.

/// R4 — an INCOMPLETE snapshot is a WELL-FORMED record (§13 says so, and
/// specimen D exists to keep it representable). It simply cannot evidence
/// `NotProduced`. Structural validity is the wrong question, and `085fd01`
/// answers only that one.
#[test]
fn r4_an_incomplete_snapshot_does_not_evidence_an_absence_claim() {
    let mut store = Store::default();
    let s = store.put(&snapshot("INCOMPLETE", &[], &[]));
    refuses(&relations(&absence_basis(&s), &store), "R4");
}

/// R5 — §13's rule: `NotProduced` is legal only when the named matcher, applied
/// to the RETAINED candidate set, yields an empty matched subset. Here a
/// retained candidate matches the recorded matcher and `matchedSnapshotDigests`
/// is empty anyway, so the absence claim is contradicted by the evidence it
/// cites.
#[test]
fn r5_an_absence_claim_its_own_candidates_contradict_is_refused() {
    let mut store = Store::default();
    // HARDENED in RED-B4R.2. `put` left this candidate with no RetentionBinding,
    // so closing the candidate-authority escape would have turned R5 green for
    // that and not for the contradiction it names.
    let candidate = store.retain_under(
        &review("synthetic-external-reviewer"),
        &assessment(&REVIEW_REQ, "RETAIN", None),
    );
    assert_candidate_is_authorised(&store, &candidate, "R5");
    let s = store.put(&snapshot("COMPLETE", &[&candidate], &[]));
    refuses(&relations(&absence_basis(&s), &store), "R5");
}

/// R6 — §13: `allReturnedSnapshotDigests` is the complete candidate set, each
/// retained under §11. A candidate nobody kept cannot be replayed against, so an
/// absence claim resting on it is unreplayable rather than merely unverified.
#[test]
fn r6_an_absence_claim_whose_candidates_do_not_resolve_is_refused() {
    let mut store = Store::default();
    let never_retained = format!("sha256:{}", "c".repeat(64));
    let s = store.put(&snapshot("COMPLETE", &[&never_retained], &[]));
    refuses(&relations(&absence_basis(&s), &store), "R6");
}

/// R7 — the two consumers must not disagree about the same artifact.
///
/// This is the shape of the defect rather than another instance of it. On
/// `085fd01` the scan path inspects `/enumeration` and the absence path does
/// not, so one INCOMPLETE snapshot is refused as scan evidence and admitted as
/// absence evidence. Whatever qualification primitive replaces this, both paths
/// must consume it.
#[test]
fn r7_the_scan_path_and_the_absence_path_agree_about_one_artifact() {
    let mut store = Store::default();
    let incomplete = snapshot("INCOMPLETE", &[], &[]);
    let s = store.put(&incomplete);

    let as_scan = scan_verdict(
        &FalsificationSurfaceScan {
            expected_redaction_policy: "1".to_owned(),
            expected_detector: ExpectedDetector {
                id: "synthetic-detector".to_owned(),
                version: "1".to_owned(),
                config_digest:
                    "sha256:1111111111111111111111111111111111111111111111111111111111111111"
                        .to_owned(),
            },
            surface: "pull-request-submitted-reviews".to_owned(),
            binding: QueryBinding {
                repository: "PhysShell/007".to_owned(),
                pull_request: "9001".to_owned(),
            },
            completeness: ScanCompleteness::Complete,
            snapshot_digest: s.clone(),
        },
        0,
        &store,
    );
    let as_absence = relations(&absence_basis(&s), &store);

    let scan_refused = matches!(as_scan, ScanVerdict::CannotCheck { .. });
    let absence_refused = matches!(as_absence, Admissible::CannotCheck { .. });
    assert_eq!(
        scan_refused, absence_refused,
        "one artifact, two consumers, two answers: scan refused = {scan_refused}, absence \
         refused = {absence_refused}. A clause that one path remembers and the other does \
         not is a procedure, not a property"
    );
}

// ---- R8-R9. CX-4: the claim must agree with the evidence.

/// R8 — the caller supplies the very answer whose evidentiary support is being
/// checked. A `usize` may not independently turn retained, non-empty evidence
/// into zero.
///
/// Deliberately NOT preregistered as `claims == matchedSnapshotDigests.len()`:
/// the contract does not say one matched source yields exactly one falsification
/// fact, so the invariant asserted here is only the non-negotiable half.
#[test]
fn r8_zero_claims_over_a_non_empty_matched_set_is_refused() {
    let mut store = Store::default();
    // HARDENED in RED-B4R.2, for the same reason as R5.
    let candidate = store.retain_under(
        &review("synthetic-external-reviewer"),
        &assessment(&REVIEW_REQ, "RETAIN", None),
    );
    assert_candidate_is_authorised(&store, &candidate, "R8");
    let s = store.put(&snapshot("COMPLETE", &[&candidate], &[&candidate]));
    let verdict = scan_verdict(
        &FalsificationSurfaceScan {
            expected_redaction_policy: "1".to_owned(),
            expected_detector: ExpectedDetector {
                id: "synthetic-detector".to_owned(),
                version: "1".to_owned(),
                config_digest:
                    "sha256:1111111111111111111111111111111111111111111111111111111111111111"
                        .to_owned(),
            },
            surface: "pull-request-submitted-reviews".to_owned(),
            binding: QueryBinding {
                repository: "PhysShell/007".to_owned(),
                pull_request: "9001".to_owned(),
            },
            completeness: ScanCompleteness::Complete,
            snapshot_digest: s,
        },
        0,
        &store,
    );
    assert!(
        matches!(verdict, ScanVerdict::CannotCheck { .. }),
        "R8: the evidence records a match and the caller said zero: got {verdict:?}"
    );
}

/// R9 — BOUNDARY. A genuinely empty matched set with zero claims is still the
/// one case where zero means something about the surface. §16 exists to keep
/// this expressible, and a fix that refused it would have closed R8 by making
/// the verdict useless.
#[test]
fn r9_zero_claims_over_an_empty_matched_set_stays_meaningful() {
    let mut store = Store::default();
    let s = store.put(&snapshot("COMPLETE", &[], &[]));
    let verdict = scan_verdict(
        &FalsificationSurfaceScan {
            expected_redaction_policy: "1".to_owned(),
            expected_detector: ExpectedDetector {
                id: "synthetic-detector".to_owned(),
                version: "1".to_owned(),
                config_digest:
                    "sha256:1111111111111111111111111111111111111111111111111111111111111111"
                        .to_owned(),
            },
            surface: "pull-request-submitted-reviews".to_owned(),
            binding: QueryBinding {
                repository: "PhysShell/007".to_owned(),
                pull_request: "9001".to_owned(),
            },
            completeness: ScanCompleteness::Complete,
            snapshot_digest: s,
        },
        0,
        &store,
    );
    assert_eq!(
        verdict,
        ScanVerdict::ZeroClaimsMeaningful,
        "R9: a complete scan over an empty matched set is the one meaningful zero"
    );
}

// ---- R10-R12. CX-5 / CR-1: the binding is an artifact.

fn conforming_pair(store: &mut Store) -> (String, String) {
    let a = assessment(&REVIEW_REQ, "RETAIN", None);
    let ad = store.put(&a);
    let rd = store.put(&review("synthetic-external-reviewer"));
    (rd, ad)
}

/// R10 — a binding with no retained bytes behind it authorises nothing.
///
/// §9.2 requires the binding to be a separately retained object; §9.5 gives it
/// an exact four-member shape. On `085fd01` a store could synthesize authority
/// from two strings it made up at call time, and `closure-retention-binding`
/// appeared exactly once in the crate — in a doc comment.
#[test]
fn r10_a_binding_with_no_retained_bytes_authorises_nothing() {
    let mut store = Store::default();
    let (rd, ad) = conforming_pair(&mut store);
    // Handed over, never retained.
    store.bind_raw(
        &rd,
        json!({
            "schemaVersion": 1,
            "sourceKind": "closure-retention-binding",
            "recordDigest": rd,
            "assessmentDigest": ad,
        }),
    );
    assert!(
        !store
            .records
            .values()
            .any(|v| v.pointer("/sourceKind").and_then(Value::as_str)
                == Some("closure-retention-binding")),
        "fixture: no binding object is retained in this store"
    );
    refuses(&relations(&basis(&rd, "/body"), &store), "R10");
}

/// R11 — §9.5 makes the binding an exact key set, for the reason §9.4 gives one
/// level up: an open schema is where the refused content goes.
#[test]
fn r11_a_binding_carrying_an_unknown_member_is_refused() {
    let mut store = Store::default();
    let (rd, ad) = conforming_pair(&mut store);
    let binding = json!({
        "schemaVersion": 1,
        "sourceKind": "closure-retention-binding",
        "recordDigest": rd,
        "assessmentDigest": ad,
        "debug": "ghp_the_content_the_gate_refused",
    });
    store.put(&binding);
    store.bind_raw(&rd, binding);
    refuses(&relations(&basis(&rd, "/body"), &store), "R11");
}

/// R12 — and its `schemaVersion` and `sourceKind` are values admissibility turns
/// on, not merely typed fields. §9.5: the three independently retained redaction
/// objects each carry `schemaVersion: 1`.
#[test]
fn r12_a_binding_of_an_unregistered_kind_or_version_is_refused() {
    for (label, binding_of) in [
        ("sourceKind", "github-submitted-review"),
        ("schemaVersion", "closure-retention-binding"),
    ] {
        let mut store = Store::default();
        let (rd, ad) = conforming_pair(&mut store);
        let binding = json!({
            "schemaVersion": if label == "schemaVersion" { 99 } else { 1 },
            "sourceKind": binding_of,
            "recordDigest": rd,
            "assessmentDigest": ad,
        });
        store.put(&binding);
        store.bind_raw(&rd, binding);
        refuses(&relations(&basis(&rd, "/body"), &store), label);
    }
}
