//! The correction round's escape set, frozen before the fix.
//!
//! Six findings from the paired external round on `6a27273`. They fall into two
//! classes that must not be conflated, because only one of them is about the
//! mechanism at all.
//!
//! ```text
//! SEMANTIC — one law, four surfaces
//!   S1  a binding answering for one record while naming another
//!   S2  an authorising assessment that is not retained
//!   S3  an assessment whose bytes are not the ones its digest names
//!   S4  an assessment that is not a conforming §9 RetentionAssessment
//!   S5  a head read asserted with no retained event behind it
//!   S6  a scan asserted with no retained snapshot behind it
//!   S7  a scan evidenced by a real snapshot answering a DIFFERENT query
//!
//! EVIDENCE ABOUT THE MECHANISM — in the witnesses, not the code
//!   E1  a restriction-lint allowance that suppresses nothing, carrying a
//!       justification describing panic paths that do not exist
//!   E2  a structural witness asserting a property it does not enforce, for a
//!       claim that was already false when it was written
//! ```
//!
//! THE WITHDRAWN CLAIM. The previous head asserted, in a commit message and in a
//! test's own documentation:
//!
//! > `RetainedEvidence` has no method returning a digest.
//!
//! That is **false**, and was false when written: `binding_for` returns a
//! `RetentionBinding` carrying two digest strings the caller reads. The old B1
//! witness tested for the substrings `-> String` and `-> Digest` in the trait
//! body, which `Option<RetentionBinding>` does not contain — so the witness was
//! green while the property it named was absent. It is withdrawn rather than
//! strengthened, because strengthening it would preserve an architecturally wrong
//! rule and merely prove it better.
//!
//! THE RULE THAT REPLACES IT:
//!
//! ```text
//! A retained store is never an authority merely because it returned a value.
//! Every value the store returns is an untrusted claim.
//!
//! A digest or reference returned INSIDE such a value may be consumed only when
//!   1. its subject relation is checked against the independently requested subject
//!   2. every referenced artifact required for admissibility is resolved
//!   3. the resolved bytes are re-digested against the reference
//!   4. the required type, schema and relationship checks succeed
//!
//! The store MAY resolve an independently supplied digest.
//! It MAY return artifacts containing further digest references.
//! It MAY NEVER make those references authoritative merely by returning them.
//! ```
//!
//! The division of labour that follows, and it is the point of splitting the two
//! classes: the structural witness guards the API SURFACE — a change to the trust
//! surface cannot pass unnoticed — and the semantic witnesses guard the LAW. The
//! structural one is not evidence of the law and must never be documented as if
//! it were. That conflation is the defect E2 records.

// Justification for the restriction-lint allowance, per AGENTS.md rule 4: the
// `expect` and `panic` sites below are this test's own assertions failing, or its
// own handling of JSON literals written in this file. There are 20 such sites and
// each is unreachable unless a literal a few lines above it is malformed, which
// must fail loudly rather than be skipped.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use o7_closure_canonical::digest;
use o7_closure_provenance::{
    admissibility, scan_verdict, staleness, Admissible, DecisionBasis, DecisionInput,
    FalsificationSurfaceScan, HeadRead, QueryBinding, RetainedEvidence, RetentionBinding,
    ScanCompleteness, ScanVerdict, Staleness, SubjectRead, Unresolved,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;

// ---- A store that can misbehave in each of the ways the law forbids.

#[derive(Default)]
struct Store {
    records: BTreeMap<String, Value>,
    bindings: BTreeMap<String, RetentionBinding>,
}

impl Store {
    /// Retain an object under its own digest, with no binding.
    fn put(&mut self, object: &Value) -> String {
        let d = digest(object).expect("digest").as_str().to_owned();
        self.records.insert(d.clone(), object.clone());
        d
    }

    /// Retain an object AND a conforming assessment authorising it.
    fn retain(&mut self, object: &Value) -> String {
        let assessment = self.put(&conforming_assessment());
        let d = self.put(object);
        self.bind(&d, &d, &assessment);
        d
    }

    /// Bind `subject` (what the store will answer for) to a binding naming
    /// `names` — normally the same, deliberately different in S1.
    fn bind(&mut self, subject: &str, names: &str, assessment_digest: &str) {
        self.bindings.insert(
            subject.to_owned(),
            RetentionBinding {
                record_digest: names.to_owned(),
                assessment_digest: assessment_digest.to_owned(),
            },
        );
    }
}

impl RetainedEvidence for Store {
    fn resolve(&self, d: &str) -> Option<Value> {
        self.records.get(d).cloned()
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
        "body": "still unreachable\n",
        "submittedAt": "2026-08-05T09:02:47Z",
        "commitId": commit_id,
    })
}

/// A conforming §9 `RetentionAssessment` with `outcome: RETAIN`.
fn conforming_assessment() -> Value {
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
        "assessedFields": ["/body", "/commitId", "/stableId"],
        "coverageComplete": true,
        "outcome": "RETAIN",
        "observedAt": "2026-08-05T09:03:00Z",
    })
}

/// A §8.1 `HeadReadEvent` with `acquisition = AVAILABLE`.
fn head_event(role: &str, snapshot_digest: &str) -> Value {
    json!({
        "schemaVersion": 1,
        "sourceKind": "github-head-read-event",
        "role": role,
        "acquisition": "AVAILABLE",
        "snapshotDigest": snapshot_digest,
        "observedAt": "2026-08-05T09:00:00Z",
    })
}

/// A §8.1 `github-pull-request-head` projection.
fn head_snapshot(head_sha: &str) -> Value {
    json!({
        "schemaVersion": 1,
        "sourceKind": "github-pull-request-head",
        "repository": "PhysShell/007",
        "pullRequest": "9001",
        "headSha": head_sha,
        "headRef": "claude/example",
        "headRepoFullName": "PhysShell/007",
    })
}

/// A §13 `github-query-snapshot` bound to one repository and pull request.
fn query_snapshot(repository: &str, pull_request: &str, surface: &str) -> Value {
    json!({
        "schemaVersion": 1,
        "sourceKind": "github-query-snapshot",
        "surface": surface,
        "requiredObservationId": "falsification/scan",
        "binding": {"repository": repository, "pullRequest": pull_request},
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
            "parameters": {"expectedAuthorLogin": "synthetic-external-reviewer"},
        },
        "allReturnedSnapshotDigests": [],
        "matchedSnapshotDigests": [],
    })
}

fn basis(digest_of: &str, pointer: &str) -> DecisionBasis {
    DecisionBasis {
        observation_id: "review/external".to_owned(),
        inputs: vec![DecisionInput {
            source_digest: digest_of.to_owned(),
            pointer: pointer.to_owned(),
        }],
        derived: Vec::new(),
        expected_query_digest: None,
        bindings: Vec::new(),
    }
}

#[track_caller]
fn refused(outcome: &Admissible) -> &[Unresolved] {
    match outcome {
        Admissible::CannotCheck { why } => why,
        Admissible::Yes { .. } => panic!(
            "admitted. A value the store returned was treated as authoritative because the \
             store returned it — which is the whole law this round exists to state"
        ),
    }
}

#[track_caller]
fn admitted(outcome: &Admissible) -> &[Value] {
    match outcome {
        Admissible::Yes { values } => values,
        Admissible::CannotCheck { why } => {
            panic!("refused, and should not have been: {why:?}")
        }
    }
}

// ---- The control. Without it every refusal below could be the fixture failing.

#[test]
fn a_fully_evidenced_decision_is_admissible() {
    let mut store = Store::default();
    let d = store.retain(&review("R1", "1111111111111111111111111111111111111111"));
    assert_eq!(
        admitted(&admissibility(&basis(&d, "/commitId"), &store)),
        [json!("1111111111111111111111111111111111111111")]
    );
}

// ---- S1. The binding answers for one record and names another.

#[test]
fn s1_a_binding_naming_another_record_is_refused() {
    let mut store = Store::default();
    let assessment = store.put(&conforming_assessment());
    let a = store.put(&review("A", "aaaa"));
    let b = store.put(&review("B", "bbbb"));
    // The store answers a request about A with a binding that names B.
    store.bind(&a, &b, &assessment);

    let outcome = admissibility(&basis(&a, "/commitId"), &store);
    let why = refused(&outcome);
    assert!(
        why.iter().any(|u| matches!(
            u,
            Unresolved::BindingSubjectMismatch { requested, binding_names }
                if requested == &a && binding_names == &b
        )),
        "got {why:?}"
    );
}

// ---- S2 / S3 / S4. The assessment must exist, be itself, and conform.

#[test]
fn s2_an_assessment_that_is_not_retained_authorises_nothing() {
    let mut store = Store::default();
    let d = store.put(&review("R1", "aaaa"));
    let phantom = format!("sha256:{}", "f".repeat(64));
    store.bind(&d, &d, &phantom);

    let outcome = admissibility(&basis(&d, "/commitId"), &store);
    let why = refused(&outcome);
    assert!(
        why.iter().any(|u| matches!(
            u,
            Unresolved::NoSuchAssessment { assessment_digest, .. } if assessment_digest == &phantom
        )),
        "redaction policy §9.2 requires the assessment bytes retained and reachable: got {why:?}"
    );
}

#[test]
fn s3_assessment_bytes_that_are_not_the_ones_the_digest_names_are_refused() {
    let mut store = Store::default();
    let d = store.put(&review("R1", "aaaa"));
    // A key that does not name the bytes stored under it.
    let wrong_key = format!("sha256:{}", "e".repeat(64));
    store
        .records
        .insert(wrong_key.clone(), conforming_assessment());
    store.bind(&d, &d, &wrong_key);

    let outcome = admissibility(&basis(&d, "/commitId"), &store);
    let why = refused(&outcome);
    assert!(
        why.iter()
            .any(|u| matches!(u, Unresolved::AssessmentDigestMismatch { .. })),
        "got {why:?}"
    );
}

#[test]
fn s4_an_object_that_is_not_a_retention_assessment_is_refused() {
    let mut store = Store::default();
    let d = store.put(&review("R1", "aaaa"));
    // A perfectly valid object of the wrong kind, retained under its own digest.
    let not_an_assessment = store.put(&review("DECOY", "cccc"));
    store.bind(&d, &d, &not_an_assessment);

    let outcome = admissibility(&basis(&d, "/commitId"), &store);
    let why = refused(&outcome);
    assert!(
        why.iter()
            .any(|u| matches!(u, Unresolved::MalformedAssessment { .. })),
        "an authorising assessment must be a §9 RetentionAssessment, not merely some \
         retained object: got {why:?}"
    );
}

// ---- S5. A head read asserted with nothing retained behind it.

#[test]
fn s5_a_fabricated_head_read_is_not_evidence_of_a_still_subject() {
    let store = Store::default();
    let fabricated = format!("sha256:{}", "9".repeat(64));
    let read = SubjectRead {
        before: HeadRead::Observed {
            event_digest: fabricated.clone(),
        },
        after: HeadRead::Observed {
            event_digest: fabricated.clone(),
        },
    };
    let verdict = staleness(&read, "deadbeef", &store);
    assert!(
        matches!(verdict, Staleness::CannotCheck { .. }),
        "two references to an event nobody retained cannot witness that the head did not \
         move: got {verdict:?}"
    );
}

#[test]
fn s5b_a_head_read_whose_event_is_malformed_is_refused() {
    let mut store = Store::default();
    // An event claiming AVAILABLE with no snapshotDigest — §8.1 requires it.
    let bad = store.put(&json!({
        "schemaVersion": 1,
        "sourceKind": "github-head-read-event",
        "role": "HEAD_BEFORE",
        "acquisition": "AVAILABLE",
        "observedAt": "2026-08-05T09:00:00Z",
    }));
    let read = SubjectRead {
        before: HeadRead::Observed {
            event_digest: bad.clone(),
        },
        after: HeadRead::Observed { event_digest: bad },
    };
    assert!(matches!(
        staleness(&read, "deadbeef", &store),
        Staleness::CannotCheck { .. }
    ));
}

/// An event that records a FAILED read cannot be consumed as a successful one.
///
/// §8.1 tags each event by its acquisition status and forbids `snapshotDigest`
/// on a FAILED one, precisely so a failed read cannot look like a successful read
/// of unchanged bytes. This witness carries the contradiction — FAILED *with* a
/// digest — because that is the shape a producer would have to construct, and
/// without it nothing tests that the status is read at all.
///
/// Added after mutation testing: deleting the `acquisition == AVAILABLE` check
/// failed no test, because the only malformed-event case covered was a missing
/// `snapshotDigest`, which the next check catches anyway. A relation with no
/// witness is a relation nobody is holding.
#[test]
fn s5e_an_event_recording_a_failed_read_is_not_a_successful_one() {
    let mut store = Store::default();
    let snapshot = store.put(&head_snapshot("deadbeef"));
    let contradictory = store.put(&json!({
        "schemaVersion": 1,
        "sourceKind": "github-head-read-event",
        "role": "HEAD_AFTER",
        "acquisition": "FAILED",
        "reason": "HTTP 502",
        "snapshotDigest": snapshot,
        "observedAt": "2026-08-05T09:00:00Z",
    }));
    let read = SubjectRead {
        before: HeadRead::Observed {
            event_digest: contradictory.clone(),
        },
        after: HeadRead::Observed {
            event_digest: contradictory,
        },
    };
    let verdict = staleness(&read, "deadbeef", &store);
    assert!(
        matches!(verdict, Staleness::CannotCheck { .. }),
        "an event that says the read failed must not be consumed as one that says it \
         succeeded, whatever digest somebody attached to it: got {verdict:?}"
    );
}

/// The positive control, so the fix for S5 is not "refuse every head read".
#[test]
fn s5c_two_properly_evidenced_matching_head_reads_are_not_stale() {
    let mut store = Store::default();
    let snapshot = store.put(&head_snapshot("deadbeef"));
    let before = store.put(&head_event("HEAD_BEFORE", &snapshot));
    let after = store.put(&head_event("HEAD_AFTER", &snapshot));

    let read = SubjectRead {
        before: HeadRead::Observed {
            event_digest: before,
        },
        after: HeadRead::Observed {
            event_digest: after,
        },
    };
    assert_eq!(staleness(&read, "deadbeef", &store), Staleness::NotStale);
}

/// And a genuinely moved head is still `Stale`, read out of retained bytes.
#[test]
fn s5d_an_evidenced_moved_head_is_stale() {
    let mut store = Store::default();
    let first = store.put(&head_snapshot("deadbeef"));
    let second = store.put(&head_snapshot("feedface"));
    let before = store.put(&head_event("HEAD_BEFORE", &first));
    let after = store.put(&head_event("HEAD_AFTER", &second));

    let read = SubjectRead {
        before: HeadRead::Observed {
            event_digest: before,
        },
        after: HeadRead::Observed {
            event_digest: after,
        },
    };
    assert!(matches!(
        staleness(&read, "deadbeef", &store),
        Staleness::Stale { .. }
    ));
}

// ---- S6 / S7. A scan must be evidenced, and by evidence about ITS query.

fn scan(binding: QueryBinding, snapshot_digest: &str) -> FalsificationSurfaceScan {
    FalsificationSurfaceScan {
        surface: "pull-request-review-comments".to_owned(),
        binding,
        completeness: ScanCompleteness::Complete,
        snapshot_digest: snapshot_digest.to_owned(),
    }
}

fn ours() -> QueryBinding {
    QueryBinding {
        repository: "PhysShell/007".to_owned(),
        pull_request: "9001".to_owned(),
    }
}

#[test]
fn s6_a_scan_evidenced_by_nothing_establishes_nothing() {
    let store = Store::default();
    let phantom = format!("sha256:{}", "0".repeat(64));
    let verdict = scan_verdict(&scan(ours(), &phantom), 0, &store);
    assert!(
        matches!(verdict, ScanVerdict::CannotCheck { .. }),
        "§16 requires the scan to be evidenced by a retained snapshot: got {verdict:?}"
    );
}

/// The one the merge-risk note named, and the reason resolving the digest alone
/// is not enough: the snapshot is real, complete and correctly digested — and it
/// answers a different question.
#[test]
fn s7_a_scan_evidenced_by_another_querys_snapshot_is_refused() {
    let mut store = Store::default();
    let elsewhere = store.put(&query_snapshot(
        "PhysShell/007",
        "9999",
        "pull-request-review-comments",
    ));
    let verdict = scan_verdict(&scan(ours(), &elsewhere), 0, &store);
    assert!(
        matches!(verdict, ScanVerdict::CannotCheck { .. }),
        "a real scan of another query is not evidence about this one: got {verdict:?}"
    );
}

/// The same for the surface: right pull request, wrong surface.
#[test]
fn s7b_a_scan_evidenced_by_another_surfaces_snapshot_is_refused() {
    let mut store = Store::default();
    let other_surface = store.put(&query_snapshot(
        "PhysShell/007",
        "9001",
        "pull-request-reviews",
    ));
    let verdict = scan_verdict(&scan(ours(), &other_surface), 0, &store);
    assert!(
        matches!(verdict, ScanVerdict::CannotCheck { .. }),
        "got {verdict:?}"
    );
}

/// The positive control, so the fix is not "refuse every scan".
#[test]
fn s7c_a_properly_evidenced_complete_scan_still_means_what_it_meant() {
    let mut store = Store::default();
    let evidence = store.put(&query_snapshot(
        "PhysShell/007",
        "9001",
        "pull-request-review-comments",
    ));
    assert_eq!(
        scan_verdict(&scan(ours(), &evidence), 0, &store),
        ScanVerdict::ZeroClaimsMeaningful
    );
    assert_eq!(
        scan_verdict(&scan(ours(), &evidence), 3, &store),
        ScanVerdict::Claims(3)
    );
}

// ---- E1. An allowance that suppresses nothing, justified by a false invariant.

/// Every restriction-lint allowance in this crate must actually suppress
/// something.
///
/// Stated as the RULE rather than as the one file that violated it. AGENTS.md
/// rule 4 requires a new allowance to state the invariant that makes it sound;
/// an allowance over a file with no `unwrap`, `expect` or `panic` site states an
/// invariant about code that does not exist. A false comment beside a provenance
/// boundary is worse than no comment: the next reader believes a check was
/// considered.
#[test]
fn e1_every_restriction_lint_allowance_suppresses_something() {
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut checked = 0;
    for entry in walk(&dir.join("src"))
        .into_iter()
        .chain(walk(&dir.join("tests")))
    {
        let source = std::fs::read_to_string(&entry).expect("reading a source file");
        if !source.contains("allow(clippy::unwrap_used") {
            continue;
        }
        checked += 1;
        let sites = source
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .filter(|l| l.contains(".unwrap()") || l.contains(".expect(") || l.contains("panic!"))
            .count();
        assert!(
            sites > 0,
            "{entry:?} allows unwrap_used/expect_used/panic and contains no such site. The \
             allowance suppresses nothing, and its justification comment describes paths that \
             do not exist"
        );
    }
    assert!(
        checked >= 2,
        "expected several allowance sites, saw {checked}"
    );
}

fn walk(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(walk(&path));
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            out.push(path);
        }
    }
    out
}

// ---- E2. The API surface is exact and reviewed. It is NOT the law.

/// A change to the trust surface cannot pass unnoticed.
///
/// This asserts the EXACT set of method signatures on `RetainedEvidence`, so any
/// added method fails this test and forces whoever adds it to say why it is
/// sound. That is all it does.
///
/// It deliberately does NOT claim that no digest leaves the store. That claim was
/// made by the witness this one replaces, it was false when written —
/// `binding_for` returns a `RetentionBinding` carrying two digest strings — and
/// the substring test that was supposed to enforce it could not have seen them.
/// The property that actually matters is enforced by S1 through S7 above, which
/// exercise behaviour rather than text.
#[test]
fn e2_retained_evidence_api_surface_is_exact_and_reviewed() {
    let source = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/lib.rs"))
        .expect("reading this crate's source");

    let after = source
        .split_once("pub trait RetainedEvidence {")
        .expect("the trait is declared")
        .1;
    // Scan to the first line that is exactly a closing brace at column 0, which
    // is the trait's own close. Slicing at the first "\n}" anywhere would stop at
    // any nested item that happened to close there.
    let body: String = after
        .lines()
        .take_while(|l| *l != "}")
        .collect::<Vec<_>>()
        .join("\n");

    let signatures: Vec<String> = body
        .lines()
        .map(str::trim)
        .filter(|l| l.starts_with("fn "))
        .map(|l| l.trim_end_matches(';').to_owned())
        .collect();

    assert_eq!(
        signatures,
        vec![
            "fn resolve(&self, digest: &str) -> Option<Value>".to_owned(),
            "fn binding_for(&self, record_digest: &str) -> Option<RetentionBinding>".to_owned(),
        ],
        "the RetainedEvidence surface changed. That is not forbidden — it is required to be \
         deliberate. Update this list and state, in the trait's own documentation, why the \
         new method cannot become an authority merely by returning a value"
    );
}
