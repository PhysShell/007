//! RED-B4R.2 — a query candidate is a consumption role too.
//!
//! GREEN-B4R's architecture is not withdrawn and this preregistration does not
//! ask for it to be. The ordering it established holds:
//!
//! ```text
//! ARTIFACT  ->  AUTHORITY  ->  ROLE QUALIFICATION
//! ```
//!
//! What it missed is that `qualify_query` reads OTHER artifacts, and those
//! references are a fourth arrow inside it:
//!
//! ```text
//! validated query snapshot
//!         |
//!         v  allReturnedSnapshotDigests
//!  candidate references
//!         |
//!         +-- VALIDATED candidates      established
//!         +-- AUTHORISED candidates     ABSENT      <- this file
//!         |
//!         v
//!  matcher replay
//! ```
//!
//! The implementation goes `store.resolve -> artifact::validate -> Candidate`
//! for every declared candidate, and that is clause 1 of §17.1 by itself.
//! `resolve_artifact` is where role, gate classification and §9.2 authority
//! live, and the candidate loop does not call it. The comment beside that loop
//! calls what it does "the door", which is the precise word for what it is not.
//!
//! WHY THE CONTRACT REQUIRES MORE. §5.3 places `github-query-snapshot` outside
//! the gate because it holds only enumeration facts and the digests of objects
//! that **passed the gate on their own**. §9.2 then requires a reachable
//! `RetentionBinding` for every retained record produced through that gate —
//! without one it is bytes somebody kept, not evidence somebody was permitted to
//! keep. §13 requires replay over the RETAINED candidate set. So a candidate is
//! a gated source consumed in a role, and the same law the other paths now obey
//! applies to it unchanged.
//!
//! THE ESCAPE, and it is simpler than the four before it:
//!
//! ```text
//! candidate       a conforming §8 submitted review, correct digest,
//!                 login != the matcher's expected author,
//!                 NO RetentionBinding, NO retained assessment
//!
//! query snapshot  schemaVersion 2, COMPLETE, implementationDigest bound,
//!                 allReturnedSnapshotDigests = [candidate]
//!                 matchedSnapshotDigests     = []
//!
//! replay          candidate -> false,  [] == []      reproduces
//!
//! today           qualify_query -> QualifiedQuery { matched: [] }
//!                 supports_absence -> OK
//!                 admissibility -> Yes
//! ```
//!
//! An unauthorised retained artifact takes part in proving an absence. And the
//! bytes need not be innocuous: the same slot accepts a complete projection
//! carrying a `body` the gate never permitted anyone to keep. A provenance
//! system with four closed doors and one service entrance is a provenance system
//! with a service entrance.
//!
//! THE PROPERTY THIS PREREGISTERS:
//!
//! > Every artifact a qualification reads is itself resolved through the door.
//! > A reference inside a qualified artifact is not a lesser artifact.
//!
//! ```text
//! Q1  conforming candidate, no binding, query otherwise reproducible   RED
//! Q2  candidate whose binding is unretained or malformed               RED
//! Q3  fully authorised NON-matching candidate, replayed empty set  BOUNDARY
//! Q4  fully authorised matching candidate, claim agrees            BOUNDARY
//! ```
//!
//! Q3 and Q4 are carried because the obvious fix — refuse a candidate that does
//! not resolve through the gated path — must not become "refuse candidates". An
//! absence claim over a properly retained non-match is exactly what §13's
//! machinery exists to support, and a round that closed Q1 by making Q3 refuse
//! would have removed the capability rather than the escape.
//!
//! AND THE FIXTURE GUARD, for the second time in one correction round. R5 and R8
//! in `correction_b4r.rs` build their candidates with `put` rather than
//! `retain_under`, so neither has a binding. The correct fix for Q1 would turn
//! both of them green — for candidate authority, not for the replay
//! disagreement and the zero-versus-non-empty contradiction they name. They are
//! hardened in the same commit as this file, and
//! `every_candidate_fixture_has_the_authority_status_its_witness_needs` here
//! makes the property executable rather than remembered: a candidate belonging
//! to a relation-specific witness must PROVABLY carry authority, and a candidate
//! belonging to an authority witness must provably not.
//!
//! The oracle for that guard is deliberately NOT `qualify_query`. It asks the
//! one path that already enforces §9.2 — consumption as a gated source through
//! `admissibility` — so it means the same thing before and after the fix.
//!
//! R6 IS NOT TOUCHED. Its candidate is absent from the store on purpose, and
//! being unresolvable is its own checkable reason.

// Justification for the restriction-lint allowance, per AGENTS.md rule 4: the
// `expect` sites below are this file's own handling of JSON literals
// written in it, unreachable unless a specimen a few lines above is malformed.
#![allow(clippy::expect_used)]

use o7_closure_canonical::digest;
use o7_closure_matcher::{resolve as resolve_matcher, verify_implementation};
use o7_closure_provenance::{
    admissibility, scan_verdict, Admissible, DecisionBasis, DecisionInput,
    FalsificationSurfaceScan, QueryBinding, RetainedEvidence, ScanCompleteness, ScanVerdict,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;

const MATCHER_ID: &str = "review-by-expected-author-login";
const MATCHER_VERSION: &str = "1";
const EXPECTED_AUTHOR: &str = "synthetic-external-reviewer";
const SOMEBODY_ELSE: &str = "an-entirely-different-person";

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

// ---- Store.

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
    /// Retain a record, its assessment, and a conforming §9.5 binding — the
    /// binding RETAINED under its own digest, as GREEN-B4R requires.
    fn retain_under(&mut self, record: &Value, assessment: &Value) -> String {
        let ad = self.put(assessment);
        let rd = self.put(record);
        let binding = binding_bytes(&rd, &ad);
        self.put(&binding);
        self.bindings.insert(rd.clone(), binding);
        rd
    }
    /// Hand over binding bytes the caller chose, retained or not.
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

fn binding_bytes(record_digest: &str, assessment_digest: &str) -> Value {
    json!({
        "schemaVersion": 1,
        "sourceKind": "closure-retention-binding",
        "recordDigest": record_digest,
        "assessmentDigest": assessment_digest,
    })
}

fn retain_assessment() -> Value {
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
        "assessedFields": REVIEW_REQ,
        "coverageComplete": true,
        "outcome": "RETAIN",
        "observedAt": "2026-08-05T09:03:00Z",
    })
}

/// A conforming §8.3 submitted review. `login` decides whether the registered
/// matcher scores it, and nothing else about the object changes with it.
fn review(login: &str, stable_id: &str) -> Value {
    json!({
        "schemaVersion": 1,
        "sourceKind": "github-submitted-review",
        "stableId": stable_id,
        "user": {"id": stable_id, "login": login, "type": "User"},
        "authorAssociation": "NONE",
        "state": "CHANGES_REQUESTED",
        "body": "ghp_the_body_the_gate_was_never_asked_about\n",
        "submittedAt": "2026-08-05T09:02:47Z",
        "commitId": "1f2e3d4c5b6a798807162534435261708f9e0d1c",
    })
}

fn bound_implementation_digest() -> String {
    let entry = resolve_matcher(MATCHER_ID, MATCHER_VERSION).expect("the matcher is registered");
    verify_implementation(entry)
        .expect("the registry is bound to its own implementation")
        .as_str()
        .to_owned()
}

/// A §13 query snapshot with everything §13 can establish about ITSELF already
/// established: schemaVersion 2, COMPLETE, a bound implementation digest. The
/// only thing any witness below varies is the candidate's authority.
fn snapshot(all: &[&str], matched: &[&str]) -> Value {
    json!({
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
            "id": MATCHER_ID,
            "version": MATCHER_VERSION,
            "implementationDigest": bound_implementation_digest(),
            "parameters": {"expectedAuthorLogin": EXPECTED_AUTHOR},
        },
        "allReturnedSnapshotDigests": all,
        "matchedSnapshotDigests": matched,
    })
}

fn absence_basis(expected: &str) -> DecisionBasis {
    DecisionBasis {
        observation_id: "review/external".to_owned(),
        inputs: Vec::new(),
        derived: Vec::new(),
        expected_query_digest: Some(expected.to_owned()),
        bindings: Vec::new(),
    }
}

fn reads(record: &str, pointer: &str) -> DecisionBasis {
    DecisionBasis {
        observation_id: "review/external".to_owned(),
        inputs: vec![DecisionInput {
            source_digest: record.to_owned(),
            pointer: pointer.to_owned(),
        }],
        derived: Vec::new(),
        expected_query_digest: None,
        bindings: Vec::new(),
    }
}

fn scan_of(snapshot_digest: &str) -> FalsificationSurfaceScan {
    FalsificationSurfaceScan {
        surface: "pull-request-submitted-reviews".to_owned(),
        binding: QueryBinding {
            repository: "PhysShell/007".to_owned(),
            pull_request: "9001".to_owned(),
        },
        completeness: ScanCompleteness::Complete,
        snapshot_digest: snapshot_digest.to_owned(),
    }
}

/// Whether a retained record carries §9.2 authority, asked through the ONE path
/// that already enforces it.
///
/// DELIBERATELY NOT `qualify_query`. The guard must mean the same thing before
/// and after the fix, so it asks a consumer whose authority chain GREEN-B4R
/// already closed: reading a pointer out of the record as a gated source. If
/// THAT is admissible, the record is authorised; if it is not, no amount of
/// candidate-shaped optimism should make it so.
fn is_authorised(store: &Store, record_digest: &str) -> bool {
    matches!(
        admissibility(&reads(record_digest, "/body"), store),
        Admissible::Yes { .. }
    )
}

#[track_caller]
fn refuses(outcome: &Admissible, what: &str) {
    assert!(
        matches!(outcome, Admissible::CannotCheck { .. }),
        "{what}: admitted. The candidate was validated and never authorised, and structural \
         validity was never the question here"
    );
}

// ---- Q1. The escape.

/// Q1 — a conforming candidate nobody was permitted to keep still evidences an
/// absence.
///
/// Everything else about this query is as strong as §13 allows: COMPLETE, a
/// bound implementation digest, and a `matchedSnapshotDigests` that replay
/// reproduces exactly. The candidate is a NON-match, so the matched set is
/// legitimately empty and the absence claim is legitimately supported — by an
/// artifact with no `RetentionBinding` and no retained assessment behind it.
#[test]
fn q1_a_candidate_with_no_retention_binding_cannot_evidence_an_absence() {
    let mut store = Store::default();
    // `put`, not `retain_under`: retained bytes, no authority.
    let candidate = store.put(&review(SOMEBODY_ELSE, "9000000801"));
    let s = store.put(&snapshot(&[&candidate], &[]));

    assert!(
        !is_authorised(&store, &candidate),
        "fixture: this candidate must have no §9.2 authority, or Q1 is about nothing"
    );
    refuses(&admissibility(&absence_basis(&s), &store), "Q1");
}

// ---- Q2. And the authority chain is checked, not merely present.

/// Q2 — a candidate whose binding is unretained, or retained and malformed.
///
/// Two cases and not one, for the reason R10 and R11 are two: producing bytes
/// nobody kept and producing an object §9.5 forbids are different facts about
/// the store, and a fix that reached only the first would leave the second as a
/// place to put anything.
#[test]
fn q2_a_candidate_whose_binding_does_not_hold_up_is_refused() {
    for (label, retain_the_binding, extra) in [
        ("an unretained binding", false, None),
        (
            "a binding carrying an unknown member",
            true,
            Some(json!("ghp_the_content_the_gate_refused")),
        ),
    ] {
        let mut store = Store::default();
        let assessment_digest = store.put(&retain_assessment());
        let candidate = store.put(&review(SOMEBODY_ELSE, "9000000801"));

        let mut binding = binding_bytes(&candidate, &assessment_digest);
        if let Some(value) = extra {
            binding
                .as_object_mut()
                .expect("the binding is an object")
                .insert("debug".to_owned(), value);
        }
        if retain_the_binding {
            store.put(&binding);
        }
        store.bind_raw(&candidate, binding);

        let s = store.put(&snapshot(&[&candidate], &[]));
        assert!(
            !is_authorised(&store, &candidate),
            "fixture ({label}): this candidate must fail §9.2, or the witness is about nothing"
        );
        refuses(&admissibility(&absence_basis(&s), &store), label);
    }
}

// ---- Q3 and Q4. What must survive the fix.

/// Q3 — BOUNDARY. A fully authorised non-matching candidate, a COMPLETE
/// enumeration, and an empty matched set the replay reproduces: the absence
/// claim stands.
///
/// This is the capability §13's machinery exists to provide, and the reason Q1's
/// fix cannot be "refuse candidates". A round that closed the escape by making
/// this refuse would have removed the feature instead of the hole.
#[test]
fn q3_an_authorised_non_matching_candidate_still_supports_an_absence() {
    let mut store = Store::default();
    let candidate = store.retain_under(&review(SOMEBODY_ELSE, "9000000801"), &retain_assessment());
    let s = store.put(&snapshot(&[&candidate], &[]));

    assert!(
        is_authorised(&store, &candidate),
        "fixture: this candidate must carry §9.2 authority, or Q3 proves nothing"
    );
    let outcome = admissibility(&absence_basis(&s), &store);
    assert!(
        matches!(outcome, Admissible::Yes { .. }),
        "Q3: a properly retained non-match is exactly what an absence claim rests on: \
         got {outcome:?}"
    );
}

/// Q4 — BOUNDARY. A fully authorised candidate that DOES match, with a
/// `matchedSnapshotDigests` that agrees, qualifies the query.
///
/// The positive control on the other side of the matched set. Q3 keeps the
/// empty case expressible and this keeps the non-empty one, so a fix cannot
/// satisfy either by refusing everything that reaches replay.
#[test]
fn q4_an_authorised_matching_candidate_qualifies_the_query() {
    let mut store = Store::default();
    let candidate =
        store.retain_under(&review(EXPECTED_AUTHOR, "9000000901"), &retain_assessment());
    let s = store.put(&snapshot(&[&candidate], &[&candidate]));

    assert!(
        is_authorised(&store, &candidate),
        "fixture: this candidate must carry §9.2 authority, or Q4 proves nothing"
    );
    let verdict = scan_verdict(&scan_of(&s), 1, &store);
    assert_eq!(
        verdict,
        ScanVerdict::Claims(1),
        "Q4: a matched, authorised candidate with an agreeing claim count qualifies"
    );
}

// ---- The fixture guard.

/// Every candidate fixture in this file has the authority status its witness
/// needs, and the two groups are checked against each other.
///
/// The second fixture guard in one correction round, and it is here because the
/// first one's lesson generalised: a witness whose fixture trips an earlier
/// check proves nothing about the property it names. Q1 and Q2 are about
/// authority, so their candidates must provably lack it — a helper that quietly
/// gained a `retain_under` would make both of them green while the escape stayed
/// open. Q3 and Q4 are about what survives, so theirs must provably have it — a
/// helper that quietly lost its binding would make both of them RED after the
/// fix, and the fix would look like an over-correction it is not.
#[test]
fn every_candidate_fixture_has_the_authority_status_its_witness_needs() {
    let mut store = Store::default();

    let unauthorised = store.put(&review(SOMEBODY_ELSE, "9000000801"));
    let authorised =
        store.retain_under(&review(EXPECTED_AUTHOR, "9000000901"), &retain_assessment());

    assert!(
        !is_authorised(&store, &unauthorised),
        "Q1/Q2's candidate shape must fail §9.2 authority"
    );
    assert!(
        is_authorised(&store, &authorised),
        "Q3/Q4's candidate shape must pass §9.2 authority"
    );
}
