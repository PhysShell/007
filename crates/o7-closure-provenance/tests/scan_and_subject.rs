//! B8 and B9 — the two escapes that live outside the per-decision basis.
//!
//! Both are the same demon in different clothes, and it is the oldest one in
//! this project: *failure -> empty set -> green*. A scan that broke and a head
//! read that did not happen are both losses of evidence, and both have an
//! obvious wrong answer that looks like an answer.

// Justification for the restriction-lint allowance, per AGENTS.md rule 4 and the
// precedent in `crates/o7-closure-classifier/tests/frozen_fixtures.rs`: every
// panic path below is this test's own assertion failing on a literal written in
// this file. Nothing here runs against production input.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use o7_closure_provenance::{
    scan_verdict, staleness, FalsificationSurfaceScan, HeadRead, ScanCompleteness, ScanVerdict,
    Staleness, SubjectRead,
};

fn scan(completeness: ScanCompleteness) -> FalsificationSurfaceScan {
    FalsificationSurfaceScan {
        surface: "pull-request-review-comments".to_owned(),
        query_binding: "PhysShell/007#9001".to_owned(),
        completeness,
        snapshot_digest: format!("sha256:{}", "c".repeat(64)),
    }
}

// ---- B8. The falsification empty-set escape.

/// A COMPLETE scan finding nothing is the only case where zero claims is a fact
/// about the surface.
#[test]
fn b8a_zero_claims_after_a_complete_scan_is_meaningful() {
    assert_eq!(
        scan_verdict(&scan(ScanCompleteness::Complete), 0),
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
    let verdict = scan_verdict(
        &scan(ScanCompleteness::Incomplete {
            reason: "page 2 fetch returned HTTP 502; not retried".to_owned(),
        }),
        0,
    );
    assert!(
        matches!(verdict, ScanVerdict::CannotCheck { .. }),
        "an incomplete scan with zero claims must not read as zero falsifications: got {verdict:?}"
    );
}

/// A FAILED scan finding nothing is `CANNOT_CHECK`.
#[test]
fn b8c_zero_claims_after_a_failed_scan_is_cannot_check() {
    let verdict = scan_verdict(
        &scan(ScanCompleteness::Failed {
            reason: "surface unavailable".to_owned(),
        }),
        0,
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
    let verdict = scan_verdict(
        &scan(ScanCompleteness::Incomplete {
            reason: "pagination terminated early".to_owned(),
        }),
        2,
    );
    assert!(
        matches!(verdict, ScanVerdict::CannotCheck { .. }),
        "a partial scan's claim count is a lower bound, not a total: got {verdict:?}"
    );
}

// ---- B9. Subject-read loss.

#[test]
fn b9a_two_matching_head_reads_are_not_stale() {
    let read = SubjectRead {
        before: HeadRead::Observed("aaaa".to_owned()),
        after: HeadRead::Observed("aaaa".to_owned()),
    };
    assert_eq!(staleness(&read, "aaaa"), Staleness::NotStale);
}

#[test]
fn b9b_a_moved_head_is_stale() {
    let read = SubjectRead {
        before: HeadRead::Observed("aaaa".to_owned()),
        after: HeadRead::Observed("bbbb".to_owned()),
    };
    assert!(matches!(staleness(&read, "aaaa"), Staleness::Stale { .. }));
}

/// An unavailable `head_after` is `CANNOT_CHECK`, never "not stale".
///
/// Provenance V1 §8.1 says this outright, and the first-cut implementation
/// violates it in the most natural way possible: it looks for a head that
/// disagrees, finds none because one was never read, and concludes agreement.
/// Absence of a contradiction is not evidence of consistency.
#[test]
fn b9c_an_unavailable_head_after_is_cannot_check_not_not_stale() {
    let read = SubjectRead {
        before: HeadRead::Observed("aaaa".to_owned()),
        after: HeadRead::Failed {
            reason: "HTTP 502 on the second head read".to_owned(),
        },
    };
    let verdict = staleness(&read, "aaaa");
    assert!(
        matches!(verdict, Staleness::CannotCheck { .. }),
        "a head read that did not happen cannot witness that the head did not move: got {verdict:?}"
    );
}

/// The same for `head_before`, so the fix is not written for one end.
#[test]
fn b9d_an_unavailable_head_before_is_cannot_check() {
    let read = SubjectRead {
        before: HeadRead::Failed {
            reason: "permission denied".to_owned(),
        },
        after: HeadRead::Observed("aaaa".to_owned()),
    };
    assert!(matches!(
        staleness(&read, "aaaa"),
        Staleness::CannotCheck { .. }
    ));
}

/// A read that failed at BOTH ends is still one verdict, and still not stale-ness.
#[test]
fn b9e_both_reads_unavailable_is_cannot_check() {
    let read = SubjectRead {
        before: HeadRead::Failed {
            reason: "a".to_owned(),
        },
        after: HeadRead::Failed {
            reason: "b".to_owned(),
        },
    };
    assert!(matches!(
        staleness(&read, "aaaa"),
        Staleness::CannotCheck { .. }
    ));
}
