//! RED-PAGINATION — J3, adjudicated ACCEPT (P1) against frozen §14.
//!
//! §14 states the rule and names the verdict:
//!
//! ```text
//! first page != complete query
//!
//! NotProduced is legal ONLY after COMPLETE enumeration
//! ```
//!
//! and then, in as many words:
//!
//! > If a next page exists, pagination terminated early, a page fetch failed,
//! > or the pagination state is unknown, the acquisition layer **may not**
//! > claim authoritative absence. That is: `CANNOT_CHECK`.
//!
//! `qualify_query` reads `/enumeration` and stops there. `nextPagePresent`
//! appears nowhere in this crate — only in the matcher's schema table, where it
//! is checked for being a boolean and never read. So a snapshot whose own two
//! fields contradict each other,
//!
//! ```text
//! enumeration              COMPLETE
//! pagination.nextPagePresent  true
//! ```
//!
//! qualifies, and an absence claim resting on it comes back admissible.
//!
//! WHY THIS IS NOT MERELY A MISSING CHECK. `enumeration` is a summary the
//! producer wrote; `nextPagePresent` is the observation it is supposed to
//! summarise. §13 requires both to be retained precisely so the summary can be
//! checked against the observation, and reading only the summary is
//! self-certification with the contradicting evidence sitting in the same
//! object. It is the same shape as R4 and the scan/absence agreement, one level
//! in: there, two artifacts disagreed; here, one artifact disagrees with
//! itself.
//!
//! WHAT IS DELIBERATELY NOT ADDED. §14 names four conditions and this repair
//! implements the two the adjudication named — a next page exists, and the
//! pagination state is unknown. "Pagination terminated early" is not
//! `pagesObtained < pagesRequested`: obtaining fewer pages than were asked for
//! is the ordinary shape of a query whose data ran out, and it is reported by
//! `nextPagePresent: false`. Inventing a rule there would be implementation →
//! contract, which is the direction G3 already refused. "A page fetch failed"
//! is an INCOMPLETE enumeration with an `incompleteReason`, and the branch
//! above already refuses it.
//!
//! ```text
//! J3-A  absence over COMPLETE + nextPagePresent: true                  RED
//! J3-B  a falsification scan over the same snapshot — one rule, not
//!       one caller's                                                   RED
//! J3-C  COMPLETE + nextPagePresent: false                    BOUNDARY  Yes
//! J3-D  INCOMPLETE + nextPagePresent: false stays refused     BOUNDARY  §13
//! J3-E  nextPagePresent spelled as the string "false"         BOUNDARY  refused
//! J3-F  no pagination block at all                            BOUNDARY  refused
//! J3-G  §14 still says what it said                                 FREEZE
//! ```
//!
//! J3-E and J3-F are the "pagination state is unknown" half. Both are refused
//! at the artifact door — §13 makes `pagination.nextPagePresent` REQUIRED and
//! the matcher's table types it as a boolean — so neither is expected to start
//! red. They are here because the adjudication requires that a missing
//! completeness witness cannot manufacture a positive result, and a property
//! that holds only because of where a check happens to live is one refactor
//! away from not holding. Recorded as boundaries rather than counted as
//! findings.

// Justification for the restriction-lint allowance, per AGENTS.md rule 4: the
// `expect` sites below are this file's own handling of JSON literals written in
// it, unreachable unless a specimen a few lines above is malformed.
// ONE SITE IS NOT THAT, and is named here rather than left to borrow the
// fixture invariant: the read of §14 out of
// `docs/architecture/closure-source-provenance-v1.md`. Its invariant is a
// different one — if the sentence this witness quotes is no longer in the
// contract, the rule moved and nothing else noticed, and the witness must fail
// loudly instead of passing over text that is gone. N1 records why an exception
// is now named: twelve files justified an allowance on the fixture invariant
// while covering sites it never described.
// Extent (checked by N1): 2 `expect` sites.
#![allow(clippy::expect_used)]

use o7_closure_provenance::{
    admissibility, scan_verdict, Admissible, DecisionBasis, DecisionProfile, ExpectedDetector,
    ExpectedQuery, FalsificationSurfaceScan, QueryBinding, RetainedEvidence, ScanCompleteness,
    ScanVerdict,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;

mod common;

const PROVENANCE: &str = include_str!("../../../docs/architecture/closure-source-provenance-v1.md");

const MATCHER_ID: &str = "review-by-expected-author-login";
const MATCHER_VERSION: &str = "1";
const OBSERVATION: &str = "review/external";

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
}
impl RetainedEvidence for Store {
    fn resolve(&self, d: &str) -> Option<Value> {
        self.records.get(d).cloned()
    }
    fn binding_for(&self, record_digest: &str) -> Option<Value> {
        self.bindings.get(record_digest).cloned()
    }
}

fn bound_implementation_digest() -> String {
    common::bound_matcher_digest(MATCHER_ID, MATCHER_VERSION)
}

/// A §13 query snapshot that is conformant in every respect the other rounds
/// established — COMPLETE, matcher bound to its implementation, empty candidate
/// set so replay reproduces the empty selection exactly — with the pagination
/// block supplied by the caller.
///
/// Everything except `pagination` is held fixed on purpose: whatever these
/// witnesses show, they show about pagination.
fn snapshot(enumeration: &str, pagination: Value) -> Value {
    let mut object = json!({
        "schemaVersion": 2,
        "sourceKind": "github-query-snapshot",
        "surface": "pull-request-submitted-reviews",
        "requiredObservationId": OBSERVATION,
        "binding": {"repository": "PhysShell/007", "pullRequest": "9001"},
        "enumeration": enumeration,
        "matcher": {
            "id": MATCHER_ID,
            "version": MATCHER_VERSION,
            "parameters": {"expectedAuthorLogin": "synthetic-external-reviewer"},
            "implementationDigest": bound_implementation_digest(),
        },
        "allReturnedSnapshotDigests": [],
        "matchedSnapshotDigests": [],
    });
    let fields = object
        .as_object_mut()
        .expect("the literal above is an object");
    if !pagination.is_null() {
        fields.insert("pagination".to_owned(), pagination);
    }
    if enumeration == "INCOMPLETE" {
        fields.insert(
            "incompleteReason".to_owned(),
            json!("the second page was never requested"),
        );
    }
    object
}

/// §13's pagination block with `nextPagePresent` supplied by the caller.
fn pagination(next_page_present: Value) -> Value {
    json!({
        "perPage": 100,
        "pagesRequested": ["1", "2"],
        "pagesObtained": ["1"],
        "nextPagePresent": next_page_present,
    })
}

fn absence_over(digest: &str) -> DecisionBasis {
    DecisionBasis {
        expected_redaction_policy: "1".to_owned(),
        expected_detector: ExpectedDetector {
            id: "synthetic-detector".to_owned(),
            version: "1".to_owned(),
            config_digest:
                "sha256:1111111111111111111111111111111111111111111111111111111111111111".to_owned(),
        },
        observation_id: OBSERVATION.to_owned(),
        inputs: Vec::new(),
        derived: Vec::new(),
        expected_query: Some(ExpectedQuery {
            digest: digest.to_owned(),
            subject: QueryBinding {
                repository: "PhysShell/007".to_owned(),
                pull_request: "9001".to_owned(),
            },
        }),
        bindings: Vec::new(),
    }
}

/// J3-A — THE REPRODUCER. The snapshot says the enumeration was complete and
/// says, one member away, that another page exists. §14 gives that pair one
/// verdict and it is not the confident one.
#[test]
fn j3a_a_complete_enumeration_beside_a_next_page_cannot_support_absence() {
    let mut store = Store::default();
    let digest = store.put(&snapshot("COMPLETE", pagination(json!(true))));
    let outcome = admissibility(DecisionProfile::Absence, &absence_over(&digest), &store);
    assert!(
        matches!(outcome, Admissible::CannotCheck { .. }),
        "J3-A: admitted an authoritative absence over an enumeration that says another page \
         exists. §14: if a next page exists the acquisition layer MAY NOT claim authoritative \
         absence — that is CANNOT_CHECK, not OWED, not PASS, not NotProduced. `enumeration` is \
         the producer's summary and `nextPagePresent` is the observation it summarises; \
         reading only the summary is self-certification with the contradiction retained in the \
         same object: got {outcome:?}"
    );
}

/// J3-B — the same snapshot under the other consumer. §14's rule is the
/// snapshot's, so a repair written into one caller would leave the other
/// admitting the identical evidence.
#[test]
fn j3b_the_rule_belongs_to_the_snapshot_and_not_to_one_caller() {
    let mut store = Store::default();
    let digest = store.put(&snapshot("COMPLETE", pagination(json!(true))));
    let outcome = scan_verdict(
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
            snapshot_digest: digest,
        },
        0,
        &store,
    );
    assert!(
        matches!(outcome, ScanVerdict::CannotCheck { .. }),
        "J3-B: a falsification scan reported a complete surface over the same contradictory \
         snapshot. §16's whole subject is `failure -> empty set -> green`, and a scan that \
         examined one page of two is exactly the failure it names: got {outcome:?}"
    );
}

/// J3-C — BOUNDARY. The conformant shape: complete, and no next page. This must
/// stay admissible, or the repair has removed the verdict rather than
/// evidencing it.
#[test]
fn j3c_a_complete_enumeration_with_no_next_page_still_supports_absence() {
    let mut store = Store::default();
    let digest = store.put(&snapshot("COMPLETE", pagination(json!(false))));
    let outcome = admissibility(DecisionProfile::Absence, &absence_over(&digest), &store);
    assert!(
        matches!(outcome, Admissible::Yes { .. }),
        "J3-C: refused the one shape §14 permits an absence claim over. `pagesObtained` shorter \
         than `pagesRequested` is not evidence of early termination — a query whose data ran \
         out asks for more pages than it gets, and says so with nextPagePresent: false: got \
         {outcome:?}"
    );
}

/// J3-D — BOUNDARY. An INCOMPLETE enumeration stays refused whatever pagination
/// says, so the new read cannot become an alternative route to admission.
#[test]
fn j3d_an_incomplete_enumeration_is_still_refused() {
    let mut store = Store::default();
    let digest = store.put(&snapshot("INCOMPLETE", pagination(json!(false))));
    let outcome = admissibility(DecisionProfile::Absence, &absence_over(&digest), &store);
    assert!(
        matches!(outcome, Admissible::CannotCheck { .. }),
        "J3-D: a partial enumeration cannot establish what was not there, and a pagination \
         block agreeing that no further page exists does not repair it: got {outcome:?}"
    );
}

/// J3-E — BOUNDARY, the unknown-state half. A string `"false"` is truthy in
/// every language that would read this artifact loosely, and §13 types the
/// member as a boolean for that reason.
#[test]
fn j3e_a_pagination_flag_that_is_not_a_boolean_is_refused() {
    let mut store = Store::default();
    let digest = store.put(&snapshot("COMPLETE", pagination(json!("false"))));
    let outcome = admissibility(DecisionProfile::Absence, &absence_over(&digest), &store);
    assert!(
        matches!(outcome, Admissible::CannotCheck { .. }),
        "J3-E: accepted a pagination flag whose type leaves the state unknown. §14 puts an \
         unknown pagination state in the same bucket as a next page that exists: got {outcome:?}"
    );
}

/// J3-F — BOUNDARY, the same half one level up: no pagination block at all.
#[test]
fn j3f_a_snapshot_with_no_pagination_block_is_refused() {
    let mut store = Store::default();
    let digest = store.put(&snapshot("COMPLETE", Value::Null));
    let outcome = admissibility(DecisionProfile::Absence, &absence_over(&digest), &store);
    assert!(
        matches!(outcome, Admissible::CannotCheck { .. }),
        "J3-F: an absence claim was admitted over a snapshot that records nothing about \
         pagination. The absence of the witness must not manufacture the positive result the \
         witness exists to license: got {outcome:?}"
    );
}

/// J3-G — FREEZE. §14 still says what it said.
#[test]
fn j3g_the_frozen_rule_is_still_the_frozen_rule() {
    let at = PROVENANCE
        .find("## 14. Pagination")
        .expect("§14 is no longer in the provenance contract");
    let rest = PROVENANCE.get(at..).unwrap_or_default();
    assert!(
        rest.contains("If a next page exists"),
        "§14 no longer opens its rule with the next-page condition. That clause is the half J3 \
         turns on: if it was removed deliberately, J3 is not a defect and this file should be \
         deleted rather than quietly passing"
    );
    assert!(
        rest.contains("may not** claim") && rest.contains("CANNOT_CHECK"),
        "§14 still names the next-page condition but no longer forbids an authoritative \
         absence under it, or no longer names CANNOT_CHECK as the verdict"
    );
    assert!(
        rest.contains("NotProduced is legal ONLY after COMPLETE enumeration"),
        "§14 no longer confines NotProduced to a complete enumeration"
    );
}
