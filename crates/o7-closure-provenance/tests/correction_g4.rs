//! RED-HEAD-FAIL — G4. The implementation contradicts the frozen contract, under
//! a comment that chose the alternative on purpose.
//!
//! §8.1 states the law and leaves no room in it:
//!
//! ```text
//! HEAD_AFTER failed  ->  CANNOT_CHECK
//!                    ->  never a silent absence of STALE
//! ```
//!
//! `staleness` does this instead:
//!
//! ```text
//! // A resolved disagreement is decisive even if the other end did not resolve.
//! for sha in &observed {
//!     if sha != expected_sha { return Staleness::Stale { .. } }
//! }
//! if unresolved.is_empty() { NotStale } else { CannotCheck { .. } }
//! ```
//!
//! The disagreement scan runs BEFORE the unresolved check, so a `HEAD_AFTER`
//! that failed returns `Stale` whenever `HEAD_BEFORE` resolved to a SHA other
//! than the expected one. The contract says that case is `CANNOT_CHECK`.
//!
//! THIS IS NOT AN OVERSIGHT, AND THAT IS THE PART WORTH RECORDING. The comment
//! above the loop states the alternative rule as a deliberate choice. Somebody
//! read the situation, found the contract's answer unsatisfying, and wrote a
//! better-sounding one into the code without amending the document. That is a
//! worse failure than forgetting, because it is invisible to every process that
//! looks for missing checks: the check is there, it is commented, and it
//! implements a law nobody agreed to.
//!
//! WHY THE CONTRACT IS RIGHT ANYWAY, and this matters because the code's
//! argument is superficially good. `HEAD_BEFORE` disagreeing with
//! `expected_sha` does not establish that the head MOVED DURING the evaluation.
//! It establishes that the caller's expectation and the pre-read disagree, which
//! has at least three causes: the head moved before the bracket opened, the
//! caller's expectation was stale to begin with, or the subject was never at
//! that SHA. `STALE` names exactly one of them — the subject moved under the
//! evaluation — and §8.1's bracket is what distinguishes it. With one end
//! missing there is no bracket, and reporting the one verdict the evidence
//! cannot distinguish is the confident answer that a fail-closed system exists
//! to refuse. `CANNOT_CHECK` is not a weaker `STALE`; it is the true one.
//!
//! THE FIX IS THE CONTRACT'S ORDER, NOT A NEW RULE:
//!
//! ```text
//! any required read unresolved  ->  CannotCheck
//! else any resolved SHA != expected  ->  Stale
//! else  ->  NotStale
//! ```
//!
//! WHY NO EXISTING WITNESS CAUGHT IT. The suite already covers a failed read at
//! either end — `b9c`, `b9d`, `b9e` in `scan_and_subject.rs` — and every one of
//! them pairs the failure with a resolved head that AGREES with the expected
//! SHA. Under agreement both laws return `CANNOT_CHECK`, so the tests pass
//! either way. The single case where the two laws differ is a failed read beside
//! a resolved head that DISAGREES, and nothing exercised it. A suite can cover
//! both ends of a branch and still never separate the two rules.
//!
//! ```text
//! G4-A  head_after failed, head_before resolved and DISAGREES     RED
//! G4-B  head_before failed, head_after resolved and DISAGREES     RED
//! G4-C  head_after unresolvable for a reason other than FAILED,
//!       head_before disagrees — "unresolved" is not only FAILED   RED
//! G4-D  both resolved and differ                        BOUNDARY  Stale
//! G4-E  both resolved and match                         BOUNDARY  NotStale
//! G4-F  one failed, the other resolved and AGREES       BOUNDARY  CannotCheck
//! G4-G  §8.1 still says what it said                       FREEZE
//! ```
//!
//! G4-G is here because of how this finding could be made to disappear. Nothing
//! in the code needs to change for these witnesses to go green — amending §8.1
//! to permit the decisive-disagreement rule would do it, and would leave a
//! green suite behind. So the contract's own sentence is asserted from the
//! markdown. A freeze that only the implementation is held to is not a freeze.

// Justification for the restriction-lint allowance, per AGENTS.md rule 4: the
// `expect` sites below are this file's own handling of JSON literals written in
// it, unreachable unless a specimen a few lines above is malformed.
#![allow(clippy::expect_used)]

use o7_closure_canonical::digest;
use o7_closure_provenance::{
    staleness, ExpectedDetector, FailedRead, HeadRead, RetainedEvidence, Staleness, Subject,
    SubjectRead,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;

const PROVENANCE: &str = include_str!("../../../docs/architecture/closure-source-provenance-v1.md");

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
    /// §5.3 puts `github-pull-request-head` in the gated set, so the snapshot
    /// needs its §9.2 authority like any other gated record.
    fn retain_gated(&mut self, object: &Value) -> String {
        let assessment = self.put(&json!({
            "schemaVersion": 1,
            "sourceKind": "closure-retention-assessment",
            "redactionPolicyVersion": "1",
            "detector": {
                "id": "synthetic-detector",
                "version": "1",
                "configDigest": "sha256:1111111111111111111111111111111111111111111111111111111111111111",
            },
            "representation": "decoded-source-field-values",
            "assessedFields": ["/head/ref", "/head/repo/full_name", "/head/sha", "/number"],
            "coverageComplete": true,
            "outcome": "RETAIN",
            "observedAt": "2026-08-05T09:03:00Z",
        }));
        let d = self.put(object);
        let binding = json!({
            "schemaVersion": 1,
            "sourceKind": "closure-retention-binding",
            "recordDigest": d,
            "assessmentDigest": assessment,
        });
        self.put(&binding);
        self.bindings.insert(d.clone(), binding);
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

fn subject(expected_sha: &str) -> Subject {
    Subject {
        expected_redaction_policy: "1".to_owned(),
        expected_detector: ExpectedDetector {
            id: "synthetic-detector".to_owned(),
            version: "1".to_owned(),
            config_digest:
                "sha256:1111111111111111111111111111111111111111111111111111111111111111".to_owned(),
        },
        repository: "PhysShell/007".to_owned(),
        pull_request: "9001".to_owned(),
        expected_sha: expected_sha.to_owned(),
    }
}

fn evidenced_head(store: &mut Store, role: &str, head_sha: &str) -> HeadRead {
    let snapshot = store.retain_gated(&json!({
        "schemaVersion": 1,
        "sourceKind": "github-pull-request-head",
        "repository": "PhysShell/007",
        "pullRequest": "9001",
        "headSha": head_sha,
        "headRef": "claude/example",
        "headRepoFullName": "PhysShell/007",
    }));
    let event = store.put(&json!({
        "schemaVersion": 1,
        "sourceKind": "github-head-read-event",
        "role": role,
        "acquisition": "AVAILABLE",
        "snapshotDigest": snapshot,
        "observedAt": "2026-08-05T09:00:00Z",
    }));
    HeadRead::Observed {
        event_digest: event,
    }
}

/// An event whose bytes nobody retained. Structurally a successful read, and
/// unresolvable all the same — the second flavour of "unresolved", and the one a
/// fix written only against `HeadRead::Failed` would miss.
fn unretained_head() -> HeadRead {
    HeadRead::Observed {
        event_digest: "sha256:00000000000000000000000000000000000000000000000000000000000000ff"
            .to_owned(),
    }
}

fn failed(reason: &str) -> HeadRead {
    HeadRead::Failed {
        code: FailedRead::RequestFailed,
    }
}

#[track_caller]
fn cannot_check(verdict: &Staleness, what: &str) {
    assert!(
        matches!(verdict, Staleness::CannotCheck { .. }),
        "{what}: §8.1 says a head read that did not happen yields CANNOT_CHECK and never a \
         silent absence of STALE. One end of the bracket is missing, so nothing here \
         distinguishes a head that moved DURING the evaluation from an expectation that was \
         stale before it opened: got {verdict:?}"
    );
}

/// G4-A — the exact case. `HEAD_AFTER` failed; `HEAD_BEFORE` resolved to a SHA
/// the caller did not expect.
#[test]
fn g4a_a_failed_after_read_beside_a_disagreeing_before_is_cannot_check() {
    let mut store = Store::default();
    let read = SubjectRead {
        before: evidenced_head(&mut store, "HEAD_BEFORE", "bbbb"),
        after: failed("HTTP 502 on the second head read"),
    };
    cannot_check(&staleness(&subject("aaaa"), &read, &store), "G4-A");
}

/// G4-B — the symmetric case, so a fix cannot be written for one end. §8.1's
/// sentence names `HEAD_AFTER` because that is the case it was written about;
/// the bracket needs both ends, and a missing `HEAD_BEFORE` leaves the same
/// question open.
#[test]
fn g4b_a_failed_before_read_beside_a_disagreeing_after_is_cannot_check() {
    let mut store = Store::default();
    let read = SubjectRead {
        before: failed("permission denied"),
        after: evidenced_head(&mut store, "HEAD_AFTER", "bbbb"),
    };
    cannot_check(&staleness(&subject("aaaa"), &read, &store), "G4-B");
}

/// G4-C — UNRESOLVED IS NOT ONLY `Failed`. This read declares itself successful
/// and names an event nobody retained, so it resolves to no SHA by a different
/// route. The law is about whether a required read produced one, not about which
/// variant the caller used to say it did not.
#[test]
fn g4c_an_unresolvable_read_beside_a_disagreeing_one_is_cannot_check() {
    let mut store = Store::default();
    let read = SubjectRead {
        before: evidenced_head(&mut store, "HEAD_BEFORE", "bbbb"),
        after: unretained_head(),
    };
    cannot_check(&staleness(&subject("aaaa"), &read, &store), "G4-C");
}

/// G4-D — BOUNDARY. Both ends read, and they disagree with the expectation. The
/// bracket is complete, so `STALE` is exactly what the evidence supports and
/// must stay.
#[test]
fn g4d_both_reads_resolved_and_differing_is_still_stale() {
    let mut store = Store::default();
    let read = SubjectRead {
        before: evidenced_head(&mut store, "HEAD_BEFORE", "aaaa"),
        after: evidenced_head(&mut store, "HEAD_AFTER", "bbbb"),
    };
    let verdict = staleness(&subject("aaaa"), &read, &store);
    assert!(
        matches!(verdict, Staleness::Stale { .. }),
        "G4-D: a complete bracket whose second read disagrees IS the case STALE names. A fix \
         that turned this into CANNOT_CHECK would have deleted the verdict rather than \
         corrected its precondition: got {verdict:?}"
    );
}

/// G4-E — BOUNDARY. Both ends read and agree with the expectation.
#[test]
fn g4e_both_reads_resolved_and_matching_is_not_stale() {
    let mut store = Store::default();
    let read = SubjectRead {
        before: evidenced_head(&mut store, "HEAD_BEFORE", "aaaa"),
        after: evidenced_head(&mut store, "HEAD_AFTER", "aaaa"),
    };
    let verdict = staleness(&subject("aaaa"), &read, &store);
    assert!(
        matches!(verdict, Staleness::NotStale),
        "G4-E: two complete reads that both match the expectation are the only shape that \
         witnesses a subject which did not move: got {verdict:?}"
    );
}

/// G4-F — BOUNDARY, and the one that keeps the fix honest in the other
/// direction. A failed read beside a resolved head that AGREES is already
/// `CANNOT_CHECK` today, and the corrected order must not turn it into anything
/// else. This is the case the existing `b9c`/`b9d` witnesses cover, restated
/// here so the two boundaries sit beside each other rather than in two files.
#[test]
fn g4f_a_failed_read_beside_an_agreeing_one_stays_cannot_check() {
    let mut store = Store::default();
    let read = SubjectRead {
        before: evidenced_head(&mut store, "HEAD_BEFORE", "aaaa"),
        after: failed("HTTP 502"),
    };
    cannot_check(&staleness(&subject("aaaa"), &read, &store), "G4-F");
}

/// G4-G — FREEZE. §8.1 still says what it said.
///
/// Every witness above goes green if §8.1 is amended to permit the
/// decisive-disagreement rule, with no code change at all — and the suite would
/// look repaired. Asserting the contract's own sentence is what makes that route
/// visible: a freeze only the implementation is held to is not a freeze, and the
/// document is the half that can be edited without any test noticing.
#[test]
fn g4g_the_frozen_law_is_still_the_frozen_law() {
    let at = PROVENANCE
        .find("HEAD_AFTER failed  ->  CANNOT_CHECK")
        .expect(
            "§8.1 no longer states that a failed HEAD_AFTER yields CANNOT_CHECK. If that was \
             deliberate, G4 is not a defect and this file should be deleted rather than \
             quietly passing; if it was not, the law just moved and nothing else noticed",
        );
    let rest = PROVENANCE.get(at..).unwrap_or_default();
    assert!(
        rest.contains("never a silent absence of STALE"),
        "§8.1 still names CANNOT_CHECK but no longer forbids a silent absence of STALE. That \
         second line is the half G4 turns on: the first says what a failed read yields, and \
         this one says the verdict may not be substituted with a more confident neighbour"
    );
}
