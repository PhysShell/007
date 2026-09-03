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
#![allow(clippy::expect_used, clippy::panic)]

use o7_closure_canonical::digest;
use o7_closure_provenance::{
    relations_checked, scan_verdict, staleness, Admissible, DecisionBasis, DecisionInput,
    ExpectedDetector, FalsificationSurfaceScan, HeadRead, QueryBinding, RetainedEvidence,
    ScanCompleteness, ScanVerdict, Staleness, Subject, SubjectRead, Unresolved,
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

// ---- A store that can misbehave in each of the ways the law forbids.

#[derive(Default)]
struct Store {
    records: BTreeMap<String, Value>,
    bindings: BTreeMap<String, Value>,
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

    /// Retain a gated head projection together with the assessment that
    /// authorises it. §5.3 gates `github-pull-request-head`; §9.2 then applies.
    fn retain_under_head(&mut self, object: &Value) -> String {
        let assessment = self.put(&head_assessment());
        let d = self.put(object);
        self.bind(&d, &d, &assessment);
        d
    }

    /// Bind `subject` (what the store will answer for) to a binding naming
    /// `names` — normally the same, deliberately different in S1.
    fn bind(&mut self, subject: &str, names: &str, assessment_digest: &str) {
        let binding = binding_bytes(names, assessment_digest);
        // RETAINED, not merely handed over: §9.2 makes the binding a separately
        // retained object, and GREEN-B4R resolves it under its own digest.
        self.put(&binding);
        self.bindings.insert(subject.to_owned(), binding);
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
        // §5.3's pointers are into the DECODED source object — GitHub's field
        // names, not the §8 projection's. §9 fixes the assessment's
        // `representation` as `decoded-source-field-values` to match. An earlier
        // revision of this fixture listed `/commitId` and `/stableId`, which are
        // the same fields named in the wrong space.
        "assessedFields": [
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

/// A conforming §9 assessment for the §5.3 always-set of a head projection.
///
/// §5.3 gates `github-pull-request-head`, so §9.2 requires its retained
/// projection to have a reachable authorising assessment exactly as a review
/// does. These fixtures retained head snapshots with none, which modelled a
/// world the contract forbids — CX-6, at the fixture level.
fn head_assessment() -> Value {
    let mut a = conforming_assessment();
    a.as_object_mut()
        .expect("the assessment is an object")
        .insert(
            "assessedFields".to_owned(),
            json!(["/head/ref", "/head/repo/full_name", "/head/sha", "/number"]),
        );
    a
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
            "implementationDigest": bound_implementation_digest(),
            "parameters": {"expectedAuthorLogin": "synthetic-external-reviewer"},
        },
        "allReturnedSnapshotDigests": [],
        "matchedSnapshotDigests": [],
    })
}

/// The subject these reads are supposed to be about, stated INDEPENDENTLY of
/// the evidence checked against it.
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

fn basis(digest_of: &str, pointer: &str) -> DecisionBasis {
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
            source_digest: digest_of.to_owned(),
            pointer: pointer.to_owned(),
        }],
        derived: Vec::new(),
        expected_query: None,
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
        admitted(&relations(&basis(&d, "/commitId"), &store)),
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

    let outcome = relations(&basis(&a, "/commitId"), &store);
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

    let outcome = relations(&basis(&d, "/commitId"), &store);
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

    let outcome = relations(&basis(&d, "/commitId"), &store);
    let why = refused(&outcome);
    assert!(
        why.iter()
            .any(|u| matches!(u, Unresolved::RecordDigestMismatch { .. })),
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

    let outcome = relations(&basis(&d, "/commitId"), &store);
    let why = refused(&outcome);
    // WrongKindForRole and not MalformedArtifact, and the distinction is one the
    // single door made available: this decoy is a perfectly well-formed §8.3
    // projection. Nothing about it is malformed — it simply cannot fill the role
    // of authorising a retention. Before the door, one assessment-shaped refusal
    // stood for both facts.
    assert!(
        why.iter()
            .any(|u| matches!(u, Unresolved::WrongKindForRole { .. })),
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
    let verdict = staleness(&subject("deadbeef"), &read, &store);
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
        staleness(&subject("deadbeef"), &read, &store),
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
    let snapshot = store.retain_under_head(&head_snapshot("deadbeef"));
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
    let verdict = staleness(&subject("deadbeef"), &read, &store);
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
    let snapshot = store.retain_under_head(&head_snapshot("deadbeef"));
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
    assert_eq!(
        staleness(&subject("deadbeef"), &read, &store),
        Staleness::NotStale
    );
}

/// And a genuinely moved head is still `Stale`, read out of retained bytes.
#[test]
fn s5d_an_evidenced_moved_head_is_stale() {
    let mut store = Store::default();
    let first = store.retain_under_head(&head_snapshot("deadbeef"));
    let second = store.retain_under_head(&head_snapshot("feedface"));
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
        staleness(&subject("deadbeef"), &read, &store),
        Staleness::Stale { .. }
    ));
}

// ---- S6 / S7. A scan must be evidenced, and by evidence about ITS query.

fn scan(binding: QueryBinding, snapshot_digest: &str) -> FalsificationSurfaceScan {
    FalsificationSurfaceScan {
        expected_redaction_policy: "1".to_owned(),
        expected_detector: ExpectedDetector {
            id: "synthetic-detector".to_owned(),
            version: "1".to_owned(),
            config_digest:
                "sha256:1111111111111111111111111111111111111111111111111111111111111111".to_owned(),
        },
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
/// something — checked PER LINT, over EVERY file, against REAL call sites.
///
/// Stated as the RULE rather than as the one file that violated it. AGENTS.md
/// rule 4 requires a new allowance to state the invariant that makes it sound;
/// an allowance over a file with no matching site states an invariant about code
/// that does not exist. A false comment beside a provenance boundary is worse
/// than no comment: the next reader believes a check was considered.
///
/// THIS TEST WAS ITSELF THE DEFECT IT NAMES, and external review found it. Three
/// separate ways, each of which let it report green over the property it claims:
///
/// ```text
/// denominator   it skipped any file not containing the literal text
///               "allow(clippy::unwrap_used", so 7 of 12 files carrying a
///               restriction-lint allowance were never examined at all
///
/// per lint      the site test was a disjunction — .unwrap() OR .expect( OR
///               panic! — so an `expect` site justified an `unwrap_used`
///               allowance
///
/// self-feeding  it searched for the TEXT ".unwrap()" while filtering only
///               comment lines, so the string literals inside its own filter
///               expression counted as this file's unwrap site. The guard
///               manufactured the proxy it then measured
/// ```
///
/// All three are one shape, and it is the shape of the round that found it:
/// a guard checks proxy P, reports property Q, and P is not Q.
///
/// WHY AN AST AND NOT ANOTHER SUBSTRING SEARCH. The two questions here are
/// exactly the two a parser answers and a text search cannot: which lints an
/// `#[allow]` names, and whether a call site is real code rather than a comment
/// or a string literal. `syn` is a dev-dependency for this test alone.
#[test]
fn e1_every_restriction_lint_allowance_suppresses_something() {
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut files_with_allowances = 0;

    for path in walk(&dir.join("src"))
        .into_iter()
        .chain(walk(&dir.join("tests")))
    {
        let source = std::fs::read_to_string(&path).expect("reading a source file");
        let parsed = syn::parse_file(&source)
            .unwrap_or_else(|e| panic!("{path:?} is not parseable Rust, which E1 requires: {e}"));

        let mut audit = Audit::default();
        syn::visit::Visit::visit_file(&mut audit, &parsed);

        if audit.allowed.is_empty() {
            continue;
        }
        files_with_allowances += 1;

        for lint in &audit.allowed {
            assert!(
                lint.has_site(&audit),
                "{path:?} allows {} and contains no {} site. The allowance suppresses \
                 nothing, and its justification comment describes paths that do not exist",
                lint.name(),
                lint.site()
            );
        }
    }

    assert!(
        files_with_allowances >= 12,
        "E1 examined only {files_with_allowances} files carrying a restriction-lint \
         allowance. Its denominator once came from one lint's spelling and silently \
         excluded most of the crate; a shrinking denominator is how this check reported \
         green over files it never opened"
    );
}

/// The three restriction lints this crate ever allows, each paired with the call
/// site that would justify allowing it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Restriction {
    UnwrapUsed,
    ExpectUsed,
    Panic,
}

impl Restriction {
    fn parse(path: &syn::Path) -> Option<Self> {
        let segments: Vec<String> = path.segments.iter().map(|s| s.ident.to_string()).collect();
        let (tool, lint) = match segments.as_slice() {
            [tool, lint] => (tool.as_str(), lint.as_str()),
            _ => return None,
        };
        if tool != "clippy" {
            return None;
        }
        match lint {
            "unwrap_used" => Some(Self::UnwrapUsed),
            "expect_used" => Some(Self::ExpectUsed),
            "panic" => Some(Self::Panic),
            _ => None,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::UnwrapUsed => "clippy::unwrap_used",
            Self::ExpectUsed => "clippy::expect_used",
            Self::Panic => "clippy::panic",
        }
    }

    fn site(self) -> &'static str {
        match self {
            Self::UnwrapUsed => "`.unwrap()`",
            Self::ExpectUsed => "`.expect(..)`",
            Self::Panic => "`panic!`",
        }
    }

    fn has_site(self, audit: &Audit) -> bool {
        match self {
            Self::UnwrapUsed => audit.unwrap_sites > 0,
            Self::ExpectUsed => audit.expect_sites > 0,
            Self::Panic => audit.panic_sites > 0,
        }
    }
}

/// What one file allows, and what it actually contains.
///
/// Both halves come from the parsed syntax tree. A `.unwrap()` inside a string
/// literal or a comment is not an expression and never reaches `visit_expr`,
/// which is the whole reason this is not a text search.
#[derive(Default)]
struct Audit {
    allowed: Vec<Restriction>,
    unwrap_sites: usize,
    expect_sites: usize,
    panic_sites: usize,
}

impl<'ast> syn::visit::Visit<'ast> for Audit {
    fn visit_attribute(&mut self, attr: &'ast syn::Attribute) {
        if attr.path().is_ident("allow") {
            let _ = attr.parse_nested_meta(|meta| {
                if let Some(lint) = Restriction::parse(&meta.path) {
                    if !self.allowed.contains(&lint) {
                        self.allowed.push(lint);
                    }
                }
                Ok(())
            });
        }
        syn::visit::visit_attribute(self, attr);
    }

    fn visit_expr_method_call(&mut self, call: &'ast syn::ExprMethodCall) {
        match call.method.to_string().as_str() {
            "unwrap" => self.unwrap_sites += 1,
            "expect" => self.expect_sites += 1,
            _ => {}
        }
        syn::visit::visit_expr_method_call(self, call);
    }

    fn visit_macro(&mut self, mac: &'ast syn::Macro) {
        if mac.path.is_ident("panic") {
            self.panic_sites += 1;
        }
        syn::visit::visit_macro(self, mac);
    }
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
            "fn binding_for(&self, record_digest: &str) -> Option<Value>".to_owned(),
        ],
        "the RetainedEvidence surface changed. That is not forbidden — it is required to be \
         deliberate. Update this list and state, in the trait's own documentation, why the \
         new method cannot become an authority merely by returning a value.\n\n\
         CHANGED IN RED-B4R, and the old signature is no longer sacred: evidence proved it \
         modelled the contract incorrectly. `binding_for` returned a decoded \
         `RetentionBinding` of two strings, which let a store synthesize authority with no \
         retained bytes behind it (CX-5/CR-1). §9.2 and §9.5 define the binding as a \
         separately canonicalized, retained object, so the store now hands over BYTES that \
         go through the same artifact door as everything else. Deliberately NOT a bare \
         digest: a store answering with the digest to check itself against is the store \
         choosing its own expectation, and hexadecimal does not make a claim independent."
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
