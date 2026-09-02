//! B8 and B9 — the two escapes that live outside the per-decision basis.
//!
//! Both are the same demon in different clothes, and it is the oldest one in
//! this project: *failure -> empty set -> green*. A scan that broke and a head
//! read that did not happen are both losses of evidence, and both have an
//! obvious wrong answer that looks like an answer.

// Justification for the restriction-lint allowance, per AGENTS.md rule 4: exactly
// one site, `digest(object).expect("digest")` in the `Store::put` helper below,
// on a JSON literal written in this file. A literal this canonicalizer cannot
// hash is a defect in the fixture and must fail loudly rather than be skipped.
#![allow(clippy::expect_used)]

use o7_closure_canonical::digest;
use o7_closure_provenance::{
    scan_verdict, staleness, FalsificationSurfaceScan, HeadRead, QueryBinding, RetainedEvidence,
    ScanCompleteness, ScanVerdict, Staleness, Subject, SubjectRead,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;

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

/// The minimum store these verdicts now need. B8 and B9 are about completeness
/// ordering and missing reads; the evidence-binding escapes they do NOT cover are
/// in `correction_b2.rs`, which is the point of keeping both files.
#[derive(Default)]
struct Store {
    records: BTreeMap<String, Value>,
    bindings: BTreeMap<String, Value>,
}
impl Store {
    fn put(&mut self, object: &Value) -> String {
        let d = digest(object).expect("digest").as_str().to_owned();
        self.records.insert(d.clone(), object.clone());
        d
    }

    /// Retain a GATED projection with the §9.2 authority it requires.
    ///
    /// This store used to answer `None` from `binding_for` unconditionally, and
    /// staleness consumed head snapshots through it happily — which is CX-6
    /// exactly, and external review found it by reading this helper rather than
    /// the implementation. §5.3's gated set includes `github-pull-request-head`,
    /// so a head projection needs an authorising assessment like any other
    /// gated source, and the fixture that could not express one was modelling a
    /// world the contract forbids.
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
            "assessedFields": [
                "/head/ref", "/head/repo/full_name", "/head/sha", "/number"
            ],
            "coverageComplete": true,
            "outcome": "RETAIN",
            "observedAt": "2026-08-05T09:03:00Z",
        }));
        let d = self.put(object);
        let binding = binding_bytes(&d, &assessment);
        // RETAINED, not merely handed over: §9.2 makes the binding a separately
        // retained object, and GREEN-B4R resolves it under its own digest.
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

/// A retained head read: the §8.1 event plus the snapshot it points at.
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

/// A retained query snapshot that evidences a scan of this surface and binding.
fn evidence_for(store: &mut Store, surface: &str) -> String {
    store.put(&json!({
        // schemaVersion 2 since GREEN-B4R.3: §13.1 adds
        // `matcher.implementationDigest` at that version, and a replay-dependent
        // role now REQUIRES a bound implementation. A version-1 snapshot is
        // still a conforming artifact — `correction_b4r3.rs`'s F1-A is the
        // witness that it passes the door and is refused the ROLE — but it can
        // no longer stand as a positive control for qualification.
        "schemaVersion": 2,
        "sourceKind": "github-query-snapshot",
        "surface": surface,
        "requiredObservationId": "falsification/scan",
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
    }))
}

/// The subject these reads are supposed to be about, stated INDEPENDENTLY of
/// the evidence checked against it.
fn subject(expected_sha: &str) -> Subject {
    Subject {
        expected_redaction_policy: "1".to_owned(),
        repository: "PhysShell/007".to_owned(),
        pull_request: "9001".to_owned(),
        expected_sha: expected_sha.to_owned(),
    }
}

fn scan(completeness: ScanCompleteness, snapshot_digest: &str) -> FalsificationSurfaceScan {
    FalsificationSurfaceScan {
        expected_redaction_policy: "1".to_owned(),
        surface: "pull-request-review-comments".to_owned(),
        binding: QueryBinding {
            repository: "PhysShell/007".to_owned(),
            pull_request: "9001".to_owned(),
        },
        completeness,
        snapshot_digest: snapshot_digest.to_owned(),
    }
}

// ---- B8. The falsification empty-set escape.

/// A COMPLETE scan finding nothing is the only case where zero claims is a fact
/// about the surface.
#[test]
fn b8a_zero_claims_after_a_complete_scan_is_meaningful() {
    let mut store = Store::default();
    let evidence = evidence_for(&mut store, "pull-request-review-comments");
    assert_eq!(
        scan_verdict(&scan(ScanCompleteness::Complete, &evidence), 0, &store),
        ScanVerdict::ZeroClaimsMeaningful
    );
}

/// An INCOMPLETE scan finding nothing is `CANNOT_CHECK`.
///
/// Provenance V1 §16: an empty vector equally permits never fetched, fetch
/// broke, only page 1 read, parser died, surface unavailable. The first-cut
/// implementation reads the claim count and never looks at the scan at all,
/// which is precisely how "we didn't look" becomes "there is nothing there".
#[test]
fn b8b_zero_claims_after_an_incomplete_scan_is_cannot_check() {
    let mut store = Store::default();
    let evidence = evidence_for(&mut store, "pull-request-review-comments");
    let verdict = scan_verdict(
        &scan(
            ScanCompleteness::Incomplete {
                reason: "page 2 fetch returned HTTP 502; not retried".to_owned(),
            },
            &evidence,
        ),
        0,
        &store,
    );
    assert!(
        matches!(verdict, ScanVerdict::CannotCheck { .. }),
        "an incomplete scan with zero claims must not read as zero falsifications: got {verdict:?}"
    );
}

/// A FAILED scan finding nothing is `CANNOT_CHECK`.
#[test]
fn b8c_zero_claims_after_a_failed_scan_is_cannot_check() {
    let mut store = Store::default();
    let evidence = evidence_for(&mut store, "pull-request-review-comments");
    let verdict = scan_verdict(
        &scan(
            ScanCompleteness::Failed {
                reason: "surface unavailable".to_owned(),
            },
            &evidence,
        ),
        0,
        &store,
    );
    assert!(
        matches!(verdict, ScanVerdict::CannotCheck { .. }),
        "got {verdict:?}"
    );
}

/// And an incomplete scan that DID find claims is still not a clean count.
///
/// The claims it found are real; the number is not a total. Reporting `Claims(n)`
/// for a partial scan invites the count to be treated as exhaustive later.
#[test]
fn b8d_an_incomplete_scan_with_claims_is_still_not_a_total() {
    let mut store = Store::default();
    let evidence = evidence_for(&mut store, "pull-request-review-comments");
    let verdict = scan_verdict(
        &scan(
            ScanCompleteness::Incomplete {
                reason: "pagination terminated early".to_owned(),
            },
            &evidence,
        ),
        2,
        &store,
    );
    assert!(
        matches!(verdict, ScanVerdict::CannotCheck { .. }),
        "a partial scan's claim count is a lower bound, not a total: got {verdict:?}"
    );
}

// ---- B9. Subject-read loss.

#[test]
fn b9a_two_matching_head_reads_are_not_stale() {
    let mut store = Store::default();
    let read = SubjectRead {
        before: evidenced_head(&mut store, "HEAD_BEFORE", "aaaa"),
        after: evidenced_head(&mut store, "HEAD_AFTER", "aaaa"),
    };
    assert_eq!(
        staleness(&subject("aaaa"), &read, &store),
        Staleness::NotStale
    );
}

#[test]
fn b9b_a_moved_head_is_stale() {
    let mut store = Store::default();
    let read = SubjectRead {
        before: evidenced_head(&mut store, "HEAD_BEFORE", "aaaa"),
        after: evidenced_head(&mut store, "HEAD_AFTER", "bbbb"),
    };
    assert!(matches!(
        staleness(&subject("aaaa"), &read, &store),
        Staleness::Stale { .. }
    ));
}

/// An unavailable `head_after` is `CANNOT_CHECK`, never "not stale".
///
/// Provenance V1 §8.1 says this outright, and the first-cut implementation
/// violates it in the most natural way possible: it looks for a head that
/// disagrees, finds none because one was never read, and concludes agreement.
/// Absence of a contradiction is not evidence of consistency.
#[test]
fn b9c_an_unavailable_head_after_is_cannot_check_not_not_stale() {
    let mut store = Store::default();
    let read = SubjectRead {
        before: evidenced_head(&mut store, "HEAD_BEFORE", "aaaa"),
        after: HeadRead::Failed {
            reason: "HTTP 502 on the second head read".to_owned(),
        },
    };
    let verdict = staleness(&subject("aaaa"), &read, &store);
    assert!(
        matches!(verdict, Staleness::CannotCheck { .. }),
        "a head read that did not happen cannot witness that the head did not move: got {verdict:?}"
    );
}

/// The same for `head_before`, so the fix is not written for one end.
#[test]
fn b9d_an_unavailable_head_before_is_cannot_check() {
    let mut store = Store::default();
    let read = SubjectRead {
        before: HeadRead::Failed {
            reason: "permission denied".to_owned(),
        },
        after: evidenced_head(&mut store, "HEAD_AFTER", "aaaa"),
    };
    assert!(matches!(
        staleness(&subject("aaaa"), &read, &store),
        Staleness::CannotCheck { .. }
    ));
}

/// A read that failed at BOTH ends is still one verdict, and still not stale-ness.
#[test]
fn b9e_both_reads_unavailable_is_cannot_check() {
    let store = Store::default();
    let read = SubjectRead {
        before: HeadRead::Failed {
            reason: "a".to_owned(),
        },
        after: HeadRead::Failed {
            reason: "b".to_owned(),
        },
    };
    assert!(matches!(
        staleness(&subject("aaaa"), &read, &store),
        Staleness::CannotCheck { .. }
    ));
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
