//! RED-B4 — an ARCHITECTURE CORRECTION, not another escape patch.
//!
//! Three review rounds found the same law and the next place it had not been
//! carried to. That is no longer a sequence of oversights; it is a property of
//! the design. The law is stated correctly in both contracts and in this crate's
//! own documentation, and it is implemented as **separate checks in separate
//! paths**, so every new consumer has to remember the whole of it independently.
//!
//! ```text
//! ROUND-B    22 witnesses      four laws
//! ROUND-B2   the store is untrusted for the BYTES it returns
//! ROUND-B3   the store is untrusted for what those bytes are ABOUT
//! ROUND-B4   ... and the next six functions were each trusted to remember
//! ```
//!
//! So this preregistration does not enumerate the seven findings from
//! `dc91e70`. It enumerates ONE property, over every artifact the crate admits:
//!
//! ```text
//! No artifact may take part in relation semantics, or contribute to
//! Admissible::Yes, until it has passed full validation of its closed form.
//! ```
//!
//! WHAT THAT MEANS FOR THE SHAPE. `resolve_record` today does
//!
//! ```text
//! Value -> digest check -> a few local semantic checks -> Value
//! ```
//!
//! and hands a raw `serde_json::Value` onward to `read_pointer`, to scan replay
//! and to head resolution, each of which then reasons about members of an object
//! nobody established was allowed to exist. The correction is a single door:
//!
//! ```text
//! raw retained Value
//!         |
//!         v
//!   validate_artifact()
//!         +-- digest identity
//!         +-- known artifact kind
//!         +-- exact closed schema: required, optional-if-present,
//!         |     no unknown members, nested closure too
//!         +-- gate classification
//!         |     gated   -> §9.2 authority required
//!         |     ungated -> no fake assessment invented
//!         v
//!   ValidatedArtifact
//!         +--> pointer resolution
//!         +--> scan semantics
//!         +--> staleness
//!         +--> absence replay
//!         +--> relation checks
//! ```
//!
//! ORDERING IS PART OF THE CLAIM, NOT A DETAIL. A malformed reduced record must
//! answer `MalformedArtifact`, never `PointerBlocked`: `PointerBlocked` is a
//! statement about the retention semantics of an object the contract forbids to
//! exist, and computing it means the partition was consulted before the artifact
//! earned the right to have one. The two witnesses at the bottom of this file
//! pin exactly that, because both were live escapes on `dc91e70`.
//!
//! THE DENOMINATOR, FIXED IN ADVANCE. Every kind this crate can admit, not the
//! kinds a defect was found in:
//!
//! ```text
//! complete §8 projections (gated)
//!     github-pull-request-head   github-actions-check
//!     github-submitted-review    github-review-comment
//!     github-issue-comment
//! constructed and supporting
//!     github-reduced-source-record   gated
//!     github-query-snapshot          ungated, §5.3 places it outside the gate
//!     github-head-read-event         §8.1
//!     closure-retention-assessment   §9
//! ```
//!
//! and the SAME adversarial family for each, generated rather than hand-picked:
//!
//! ```text
//! + unknown top-level member
//! + unknown nested member
//! - each required member, one at a time
//!   wrong member type
//!   wrong schemaVersion
//!   wrong sourceKind, and wrong role where the kind has one
//! ```
//!
//! WHY GENERATED. Hand-written cases are a list of what somebody thought of,
//! which is the failure this round exists to stop repeating. A table of kinds
//! crossed with a table of mutations has no such ceiling: adding a kind to the
//! denominator adds its whole family automatically, and a kind that is never
//! added is visibly absent from one list rather than invisibly absent from
//! sixty.
//!
//! WHAT IS DELIBERATELY NOT HERE.
//!
//! - **The query snapshot's retention binding is NOT restored.** GREEN-B3 was
//!   right that §9.2 scopes that requirement to records produced through the
//!   gate and that §5.3 places query snapshots outside it. Putting the binding
//!   back would be green and conceptually false. The correct replacement for the
//!   removed check was always full §13 validity, and that is what
//!   `s_query_snapshot_*` below demands.
//! - **No caller-supplied allowlist for `findingId`.** That would move "who is
//!   the authority" up one level and leave the provenance of the allowlist to be
//!   investigated in a later round. `detector.configDigest` must resolve to
//!   retained, versioned detector semantics, and redaction §11 records that as
//!   OWED. It stays OWED, and Slice B correspondingly may NOT claim that an
//!   assessment's findings were produced by the configuration they name.

// Justification for the restriction-lint allowance, per AGENTS.md rule 4: the
// `expect` and `panic` sites below are this file's own handling of JSON literals
// written in it — `digest(..).expect` in `put`, and the `expect`s in the mutation
// helpers, each unreachable unless a specimen a few lines above is malformed. A
// broken specimen must fail loudly rather than silently weaken a whole family.
// Extent (checked by N1): 16 `expect` sites, 3 `panic` sites.
#![allow(clippy::expect_used, clippy::panic)]

use o7_closure_canonical::digest;
use o7_closure_provenance::{
    relations_checked, scan_verdict, staleness, AcquisitionLocator, Admissible, DecisionBasis,
    DecisionInput, ExpectedDetector, ExpectedQuery, FalsificationSurfaceScan, HeadRead,
    QueryBinding, RetainedEvidence, ScanCompleteness, ScanVerdict, Staleness, Subject, SubjectRead,
    Unresolved,
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

// ---- Store.

#[derive(Default)]
struct Store {
    records: BTreeMap<String, Value>,
    bindings: BTreeMap<String, Value>,
}

impl Store {
    fn put(&mut self, object: &Value) -> String {
        let d = common::digest_of(object);
        self.records.insert(d.clone(), object.clone());
        d
    }

    fn retain_under(&mut self, record: &Value, assessment: &Value) -> String {
        let assessment_digest = self.put(assessment);
        let record_digest = self.put(record);
        let binding = binding_bytes(&record_digest, &assessment_digest);
        // RETAINED, not merely handed over. §9.2 makes the binding a separately
        // retained object, and GREEN-B4R resolves it under its own digest before
        // reading a member out of it — so a store that only produces the bytes
        // at call time is producing a claim nobody kept.
        self.put(&binding);
        self.bindings.insert(record_digest.clone(), binding);
        record_digest
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

// ---- The §5.3 always-sets, in §5.5 order, for the gated kinds.

fn assessed_fields(locator_kind: &str) -> &'static [&'static str] {
    match locator_kind {
        "github-issue-comment" => &[
            "/author_association",
            "/body",
            "/created_at",
            "/id",
            "/updated_at",
            "/user/id",
            "/user/login",
            "/user/type",
        ],
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
        "github-pull-request-head" => {
            &["/head/ref", "/head/repo/full_name", "/head/sha", "/number"]
        }
        "github-actions-check" => &["/head_sha", "/id", "/name", "/status"],
        other => panic!("no §5.3 always-set transcribed for {other}"),
    }
}

/// A conforming §9 `RETAIN` assessment for a complete projection of `kind`.
fn retain_assessment(kind: &str) -> Value {
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
        "assessedFields": assessed_fields(kind),
        "coverageComplete": true,
        "outcome": "RETAIN",
        "observedAt": "2026-08-05T09:03:00Z",
    })
}

// ---- Conforming specimens, one per kind in the denominator.

fn pull_request_head() -> Value {
    json!({
        "schemaVersion": 1,
        "sourceKind": "github-pull-request-head",
        "repository": "PhysShell/007",
        "pullRequest": "9001",
        "headSha": "1f2e3d4c5b6a798807162534435261708f9e0d1c",
        "headRef": "claude/example",
        "headRepoFullName": "PhysShell/007",
    })
}

fn actions_check() -> Value {
    json!({
        "schemaVersion": 1,
        "sourceKind": "github-actions-check",
        "stableId": "9100000201",
        "name": "worker gate",
        "headSha": "1f2e3d4c5b6a798807162534435261708f9e0d1c",
        "status": "completed",
    })
}

fn submitted_review() -> Value {
    json!({
        "schemaVersion": 1,
        "sourceKind": "github-submitted-review",
        "stableId": "9000000901",
        "user": {"id": "9000000901", "login": "synthetic-external-reviewer", "type": "User"},
        "authorAssociation": "NONE",
        "state": "CHANGES_REQUESTED",
        "body": "still unreachable\n",
        "submittedAt": "2026-08-05T09:02:47Z",
        "commitId": "1f2e3d4c5b6a798807162534435261708f9e0d1c",
    })
}

fn review_comment() -> Value {
    json!({
        "schemaVersion": 1,
        "sourceKind": "github-review-comment",
        "stableId": "9000000202",
        "pullRequestReviewId": "9000000901",
        "user": {"id": "9000000901", "login": "synthetic-external-reviewer", "type": "User"},
        "authorAssociation": "NONE",
        "body": "here specifically\n",
        "commitId": "1f2e3d4c5b6a798807162534435261708f9e0d1c",
        "originalCommitId": "1f2e3d4c5b6a798807162534435261708f9e0d1c",
        "path": "crates/o7-closure-provenance/src/lib.rs",
        "createdAt": "2026-08-05T09:02:47Z",
        "updatedAt": "2026-08-05T09:02:47Z",
    })
}

fn issue_comment() -> Value {
    json!({
        "schemaVersion": 1,
        "sourceKind": "github-issue-comment",
        "stableId": "9000000303",
        "user": {"id": "9000000901", "login": "synthetic-external-reviewer", "type": "User"},
        "authorAssociation": "NONE",
        "body": "a comment\n",
        "createdAt": "2026-08-05T09:02:47Z",
        "updatedAt": "2026-08-05T09:02:47Z",
    })
}

fn reduced_record() -> Value {
    let mut retained = Map::new();
    for pointer in assessed_fields("github-submitted-review") {
        if *pointer == "/body" {
            continue;
        }
        retained.insert(
            (*pointer).to_owned(),
            json!("value-as-the-projection-carries-it"),
        );
    }
    json!({
        "schemaVersion": 1,
        "sourceKind": "github-reduced-source-record",
        "locatorKind": "github-submitted-review",
        "locator": {
            "repository": "PhysShell/007",
            "pullRequest": "9001",
            "stableId": "9000000901",
        },
        "redactionPolicyVersion": "1",
        "outcome": "BLOCK_SECRET",
        "coverageComplete": true,
        "retainedFields": Value::Object(retained),
        "blockedFields": ["/body"],
    })
}

/// The assessment the reduced record above is the §7.1 computation of.
fn block_secret_assessment() -> Value {
    let mut a = retain_assessment("github-submitted-review");
    let o = a.as_object_mut().expect("assessment is an object");
    o.insert("outcome".to_owned(), json!("BLOCK_SECRET"));
    o.insert(
        "findings".to_owned(),
        json!([{"field": "/body", "findingId": "rule-aws-key"}]),
    );
    a
}

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
        "surface": "pull-request-review-comments",
        // "review/external" since GREEN-B4R.3, and this fixture was never
        // coherent: it declared a query FOR "falsification/scan" while the basis
        // consuming it as absence evidence is about "review/external". Nothing
        // compared the two, so nothing noticed. §13's requiredObservationId is
        // the observation a snapshot answers, and an enumeration of another
        // question establishes nothing about this one.
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

fn head_read_event(snapshot_digest: &str) -> Value {
    json!({
        "schemaVersion": 1,
        "sourceKind": "github-head-read-event",
        "role": "HEAD_BEFORE",
        "acquisition": "AVAILABLE",
        "snapshotDigest": snapshot_digest,
        "observedAt": "2026-08-05T09:00:00Z",
    })
}

// ---- The uniform adversarial family.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Family {
    UnknownTopLevelMember,
    UnknownNestedMember,
    MissingRequiredMember,
    WrongMemberType,
    WrongSchemaVersion,
    WrongSourceKind,
    WrongRole,
}

const EXFILTRATED: &str = "ghp_the_content_the_gate_never_assessed";

/// Every mutation of `specimen` in this family. One family can yield many
/// mutants — `MissingRequiredMember` yields one per required member, which is
/// the point: a validator that happens to probe three of nine members passes a
/// hand-picked case and fails here.
fn mutants(specimen: &Value, family: Family, nested: Option<&str>) -> Vec<(String, Value)> {
    let object = specimen.as_object().expect("a specimen is an object");
    let mut out = Vec::new();
    match family {
        Family::UnknownTopLevelMember => {
            let mut m = specimen.clone();
            m.as_object_mut()
                .expect("object")
                .insert("unexpectedMember".to_owned(), json!(EXFILTRATED));
            out.push(("+/unexpectedMember".to_owned(), m));
        }
        Family::UnknownNestedMember => {
            if let Some(path) = nested {
                let mut m = specimen.clone();
                m.pointer_mut(path)
                    .and_then(Value::as_object_mut)
                    .expect("the nested path names an object")
                    .insert("unexpectedMember".to_owned(), json!(EXFILTRATED));
                out.push((format!("+{path}/unexpectedMember"), m));
            }
        }
        Family::MissingRequiredMember => {
            for key in object.keys() {
                let mut m = specimen.clone();
                m.as_object_mut().expect("object").remove(key);
                out.push((format!("-/{key}"), m));
            }
        }
        Family::WrongMemberType => {
            for (key, value) in object {
                // `sourceKind` has a family of its own; mutating it here would
                // produce an artifact of a different KIND rather than one of this
                // kind with a wrong member.
                if key == "sourceKind" {
                    continue;
                }
                // THE MUTATION MUST ACTUALLY CHANGE THE TYPE, and this used to
                // read `|| !value.is_string()`. That skipped every object, array
                // and bool member — so no mutant ever carried
                // `"retainedFields": 1234`, `"pagination": 1234`,
                // `"coverageComplete": 1234` or `"user": 1234`, and a family
                // named WrongMemberType meant
                // "wrong type for string members except sourceKind".
                //
                // The exclusion had a real reason and the wrong shape: replacing
                // a NUMBER with `1234` is a no-op, so the fix is to pick a
                // replacement of a different type per member rather than to skip
                // the members a single constant cannot mutate. Found by external
                // review, in the file that exists to assert this surface is
                // closed.
                let (label, wrong) = if value.is_number() {
                    ("a string", json!("not-a-number"))
                } else {
                    ("a number", json!(1234))
                };
                let mut m = specimen.clone();
                m.as_object_mut()
                    .expect("object")
                    .insert(key.clone(), wrong);
                out.push((format!("/{key} is {label}"), m));
            }
        }
        Family::WrongSchemaVersion => {
            let mut m = specimen.clone();
            m.as_object_mut()
                .expect("object")
                .insert("schemaVersion".to_owned(), json!(99));
            out.push(("/schemaVersion is 99".to_owned(), m));
        }
        Family::WrongSourceKind => {
            let mut m = specimen.clone();
            m.as_object_mut()
                .expect("object")
                .insert("sourceKind".to_owned(), json!("github-not-a-thing"));
            out.push(("/sourceKind is unregistered".to_owned(), m));
        }
        Family::WrongRole => {
            if object.contains_key("role") {
                let mut m = specimen.clone();
                m.as_object_mut()
                    .expect("object")
                    .insert("role".to_owned(), json!("HEAD_SIDEWAYS"));
                out.push(("/role is unregistered".to_owned(), m));
            }
        }
    }
    out
}

const ALL_FAMILIES: [Family; 7] = [
    Family::UnknownTopLevelMember,
    Family::UnknownNestedMember,
    Family::MissingRequiredMember,
    Family::WrongMemberType,
    Family::WrongSchemaVersion,
    Family::WrongSourceKind,
    Family::WrongRole,
];

// ---- How each artifact reaches decision semantics.

#[derive(Debug, Clone, Copy)]
enum Probe {
    /// A gated source record read through a decision pointer.
    GatedSource {
        assessment_kind: &'static str,
        pointer: &'static str,
    },
    /// The reduced record, which carries its own outcome and partition.
    ReducedSource { pointer: &'static str },
    /// The assessment bound to an otherwise conforming record.
    Assessment,
    /// The query snapshot an absence claim is replayed against.
    ExpectedQuery,
    /// The query snapshot evidencing a falsification scan.
    ScanEvidence,
    /// The §8.1 event in the HEAD_BEFORE slot.
    HeadEvent,
    /// The head projection the HEAD_BEFORE event names.
    HeadSnapshot,
}

fn basis(record_digest: &str, pointer: &str) -> DecisionBasis {
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
            source_digest: record_digest.to_owned(),
            pointer: pointer.to_owned(),
            locator: AcquisitionLocator::InPullRequest {
                repository: "PhysShell/007".to_owned(),
                pull_request: "9001".to_owned(),
                stable_id: "9000000901".to_owned(),
            },
        }],
        derived: Vec::new(),
        expected_query: None,
        bindings: Vec::new(),
    }
}

fn subject() -> Subject {
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
        expected_sha: "1f2e3d4c5b6a798807162534435261708f9e0d1c".to_owned(),
    }
}

/// Run `artifact` through its probe and report whether the crate REFUSED it.
///
/// Refusal is the strong form in every case. For staleness that means
/// `CannotCheck` specifically and not `Stale`: `Stale` is also a semantic
/// conclusion drawn from an artifact the contract forbids to exist.
fn refused(probe: Probe, artifact: &Value) -> bool {
    let mut store = Store::default();
    match probe {
        Probe::GatedSource {
            assessment_kind,
            pointer,
        } => {
            let d = store.retain_under(artifact, &retain_assessment(assessment_kind));
            matches!(
                relations(&basis(&d, pointer), &store),
                Admissible::CannotCheck { .. }
            )
        }
        Probe::ReducedSource { pointer } => {
            let d = store.retain_under(artifact, &block_secret_assessment());
            matches!(
                relations(&basis(&d, pointer), &store),
                Admissible::CannotCheck { .. }
            )
        }
        Probe::Assessment => {
            let d = store.retain_under(&submitted_review(), artifact);
            matches!(
                relations(&basis(&d, "/body"), &store),
                Admissible::CannotCheck { .. }
            )
        }
        Probe::ExpectedQuery => {
            let expected = store.put(artifact);
            let mut b = DecisionBasis {
                expected_redaction_policy: "1".to_owned(),
                expected_detector: ExpectedDetector {
                    id: "synthetic-detector".to_owned(),
                    version: "1".to_owned(),
                    config_digest:
                        "sha256:1111111111111111111111111111111111111111111111111111111111111111"
                            .to_owned(),
                },
                observation_id: "review/external".to_owned(),
                inputs: Vec::new(),
                derived: Vec::new(),
                expected_query: None,
                bindings: Vec::new(),
            };
            b.expected_query = Some(ExpectedQuery {
                digest: expected,
                subject: QueryBinding {
                    repository: "PhysShell/007".to_owned(),
                    pull_request: "9001".to_owned(),
                },
            });
            matches!(relations(&b, &store), Admissible::CannotCheck { .. })
        }
        Probe::ScanEvidence => {
            let evidence = store.put(artifact);
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
                    surface: "pull-request-review-comments".to_owned(),
                    binding: QueryBinding {
                        repository: "PhysShell/007".to_owned(),
                        pull_request: "9001".to_owned(),
                    },
                    completeness: ScanCompleteness::Complete,
                    snapshot_digest: evidence,
                },
                0,
                &store,
            );
            matches!(verdict, ScanVerdict::CannotCheck { .. })
        }
        Probe::HeadEvent => {
            // §5.3 gates github-pull-request-head, so §9.2 requires the snapshot
            // to carry retention authority. RED-B4's fixture retained it with
            // none, which modelled a world the contract forbids — that is CX-6,
            // and the door now refuses it.
            let snapshot = store.retain_under(
                &pull_request_head(),
                &retain_assessment("github-pull-request-head"),
            );
            // The specimen already names this snapshot's digest, because a10
            // builds it from `digest(&pull_request_head())` and the store retains
            // that exact object. An earlier revision re-pointed `snapshotDigest`
            // here to be safe, which silently OVERWROTE the WrongMemberType
            // mutation and made that whole mutant untested — a harness that
            // repairs the specimen it is supposed to be breaking.
            if let Some(named) = artifact.pointer("/snapshotDigest").and_then(Value::as_str) {
                assert_eq!(
                    named, snapshot,
                    "fixture: an unmutated /snapshotDigest must already name this store's \
                     snapshot, so that nothing here needs to repair the specimen"
                );
            }
            let before = store.put(artifact);
            let mut after_event = head_read_event(&snapshot);
            after_event
                .as_object_mut()
                .expect("object")
                .insert("role".to_owned(), json!("HEAD_AFTER"));
            let after = store.put(&after_event);
            let read = SubjectRead {
                before: HeadRead::Observed {
                    event_digest: before,
                },
                after: HeadRead::Observed {
                    event_digest: after,
                },
            };
            matches!(
                staleness(&subject(), &read, &store),
                Staleness::CannotCheck { .. }
            )
        }
        Probe::HeadSnapshot => {
            // Both snapshots carry authority, so the ONLY thing that can refuse a
            // mutant is its own malformation.
            let assessment = retain_assessment("github-pull-request-head");
            let mutated = store.retain_under(artifact, &assessment);
            let conforming = store.retain_under(&pull_request_head(), &assessment);
            let before = store.put(&head_read_event(&mutated));
            let mut after_event = head_read_event(&conforming);
            after_event
                .as_object_mut()
                .expect("object")
                .insert("role".to_owned(), json!("HEAD_AFTER"));
            let after = store.put(&after_event);
            let read = SubjectRead {
                before: HeadRead::Observed {
                    event_digest: before,
                },
                after: HeadRead::Observed {
                    event_digest: after,
                },
            };
            matches!(
                staleness(&subject(), &read, &store),
                Staleness::CannotCheck { .. }
            )
        }
    }
}

/// Run every family against one kind and report the mutants that were ADMITTED.
#[track_caller]
fn admitted_mutants(specimen: &Value, probe: Probe, nested: Option<&str>) -> Vec<String> {
    let mut escaped = Vec::new();
    for family in ALL_FAMILIES {
        for (label, mutant) in mutants(specimen, family, nested) {
            if !refused(probe, &mutant) {
                escaped.push(format!("{family:?}: {label}"));
            }
        }
    }
    escaped
}

#[track_caller]
fn no_mutant_survives(kind: &str, specimen: &Value, probe: Probe, nested: Option<&str>) {
    // The conforming specimen must be ADMITTED, or the family below proves
    // nothing: a validator that refuses everything refuses every mutant too.
    assert!(
        !refused(probe, specimen),
        "{kind}: the conforming specimen is refused, so this family cannot \
         distinguish a malformed artifact from a well-formed one"
    );
    let escaped = admitted_mutants(specimen, probe, nested);
    assert!(
        escaped.is_empty(),
        "{kind}: {} malformed mutant(s) reached decision semantics:\n  {}",
        escaped.len(),
        escaped.join("\n  ")
    );
}

// ---- One test per kind in the denominator.

#[test]
fn a1_github_pull_request_head_as_a_gated_source() {
    no_mutant_survives(
        "github-pull-request-head",
        &pull_request_head(),
        Probe::GatedSource {
            assessment_kind: "github-pull-request-head",
            pointer: "/headSha",
        },
        None,
    );
}

#[test]
fn a2_github_actions_check() {
    no_mutant_survives(
        "github-actions-check",
        &actions_check(),
        Probe::GatedSource {
            assessment_kind: "github-actions-check",
            pointer: "/name",
        },
        None,
    );
}

#[test]
fn a3_github_submitted_review() {
    no_mutant_survives(
        "github-submitted-review",
        &submitted_review(),
        Probe::GatedSource {
            assessment_kind: "github-submitted-review",
            pointer: "/body",
        },
        Some("/user"),
    );
}

#[test]
fn a4_github_review_comment() {
    no_mutant_survives(
        "github-review-comment",
        &review_comment(),
        Probe::GatedSource {
            assessment_kind: "github-review-comment",
            pointer: "/body",
        },
        Some("/user"),
    );
}

#[test]
fn a5_github_issue_comment() {
    no_mutant_survives(
        "github-issue-comment",
        &issue_comment(),
        Probe::GatedSource {
            assessment_kind: "github-issue-comment",
            pointer: "/body",
        },
        Some("/user"),
    );
}

#[test]
fn a6_github_reduced_source_record() {
    no_mutant_survives(
        "github-reduced-source-record",
        &reduced_record(),
        Probe::ReducedSource {
            pointer: "/commit_id",
        },
        Some("/locator"),
    );
}

#[test]
fn a7_closure_retention_assessment() {
    no_mutant_survives(
        "closure-retention-assessment",
        &retain_assessment("github-submitted-review"),
        Probe::Assessment,
        Some("/detector"),
    );
}

#[test]
fn a8_github_query_snapshot_as_absence_evidence() {
    no_mutant_survives(
        "github-query-snapshot (expected-query role)",
        &query_snapshot(),
        Probe::ExpectedQuery,
        Some("/pagination"),
    );
}

#[test]
fn a9_github_query_snapshot_as_scan_evidence() {
    no_mutant_survives(
        "github-query-snapshot (scan-evidence role)",
        &query_snapshot(),
        Probe::ScanEvidence,
        Some("/matcher"),
    );
}

#[test]
fn a10_github_head_read_event() {
    let snapshot = digest(&pull_request_head())
        .expect("canonicalizable")
        .as_str()
        .to_owned();
    no_mutant_survives(
        "github-head-read-event",
        &head_read_event(&snapshot),
        Probe::HeadEvent,
        None,
    );
}

#[test]
fn a11_github_pull_request_head_as_a_subject_read() {
    no_mutant_survives(
        "github-pull-request-head (subject-read role)",
        &pull_request_head(),
        Probe::HeadSnapshot,
        None,
    );
}

// ---- The two escapes from dc91e70 that name the ORDER, not just the outcome.

/// A complete projection carrying an unassessed member must be refused as a
/// MALFORMED ARTIFACT — and the member must never be readable as a value.
///
/// On `dc91e70` reading `/debug` returned the secret inside `Admissible::Yes`.
#[test]
fn b1_an_unassessed_member_is_never_an_admitted_value() {
    let mut store = Store::default();
    let mut record = submitted_review();
    record
        .as_object_mut()
        .expect("object")
        .insert("debug".to_owned(), json!(EXFILTRATED));
    let d = store.retain_under(&record, &retain_assessment("github-submitted-review"));

    let outcome = relations(&basis(&d, "/debug"), &store);
    let Admissible::CannotCheck { why } = &outcome else {
        panic!("the gate was defeated at the consumer: {outcome:?}");
    };
    assert!(
        why.iter()
            .any(|u| matches!(u, Unresolved::MalformedArtifact { .. })),
        "an object §8 forbids to exist must be refused AS AN ARTIFACT, before any \
         question about one of its members is answered: got {why:?}"
    );
}

/// A malformed reduced record must answer `MalformedArtifact`, never
/// `PointerBlocked`.
///
/// `PointerBlocked` is a statement about the retention semantics of this record.
/// Producing it means the partition was consulted — that the object was treated
/// as a reduced record — before anything established that it is one. On
/// `dc91e70` this returned `PointerBlocked`, which reads as a clean, principled
/// refusal and is the wrong refusal for the wrong reason.
#[test]
fn b2_retention_semantics_are_not_computed_for_an_artifact_that_may_not_exist() {
    let mut store = Store::default();
    let mut record = reduced_record();
    record
        .as_object_mut()
        .expect("object")
        .insert("debug".to_owned(), json!(EXFILTRATED));
    let d = store.retain_under(&record, &block_secret_assessment());

    let outcome = relations(&basis(&d, "/debug"), &store);
    let Admissible::CannotCheck { why } = &outcome else {
        panic!("admitted: {outcome:?}");
    };
    assert!(
        why.iter()
            .any(|u| matches!(u, Unresolved::MalformedArtifact { .. })),
        "expected MalformedArtifact; PointerBlocked would mean the partition of a \
         forbidden object was consulted: got {why:?}"
    );
}

/// ADDED IN GREEN-B4, and it guards the denominator rather than the law.
///
/// Every test above asserts that no mutant escaped. All of them would also pass
/// if the families generated nothing at all — an empty adversarial surface has
/// no escapes either, and that is the `failure -> empty set -> green` demon
/// wearing the costume of a passing suite. So the size of the surface is itself
/// asserted: shrinking a specimen, dropping a family, or removing a kind from
/// the denominator fails here even when every remaining mutant is refused.
///
/// The floor is deliberately below the current count. It is a guard against
/// collapse, not a transcription of a number that legitimately grows whenever a
/// contract gains a member.
#[test]
fn the_adversarial_surface_has_not_silently_shrunk() {
    let cases = adversarial_cases();

    let mut total = 0usize;
    for (kind, specimen, nested) in &cases {
        let mut per_kind = 0usize;
        for family in ALL_FAMILIES {
            per_kind += mutants(specimen, family, *nested).len();
        }
        assert!(
            per_kind >= 6,
            "{kind}: the family generates only {per_kind} mutants, which is fewer than one \
             per adversarial class"
        );
        total += per_kind;
    }
    assert!(
        total >= 100,
        "the whole adversarial surface generates {total} mutants; RED-B4 measured 122 across \
         eleven probes and a collapse to below a hundred means a specimen, a family or a \
         kind was dropped"
    );
}

/// The kinds and specimens the adversarial families range over.
///
/// ONE list, read by both the surface guard and the per-member denominator
/// below. A second copy is a second denominator, and a denominator that can
/// disagree with itself is how a family comes to generate nothing for a whole
/// CLASS of member while every count stays comfortable.
fn adversarial_cases() -> Vec<(&'static str, Value, Option<&'static str>)> {
    vec![
        ("github-pull-request-head", pull_request_head(), None),
        ("github-actions-check", actions_check(), None),
        ("github-submitted-review", submitted_review(), Some("/user")),
        ("github-review-comment", review_comment(), Some("/user")),
        ("github-issue-comment", issue_comment(), Some("/user")),
        (
            "github-reduced-source-record",
            reduced_record(),
            Some("/locator"),
        ),
        (
            "closure-retention-assessment",
            retain_assessment("github-submitted-review"),
            Some("/detector"),
        ),
        (
            "github-query-snapshot",
            query_snapshot(),
            Some("/pagination"),
        ),
        (
            "github-head-read-event",
            head_read_event(
                "sha256:0000000000000000000000000000000000000000000000000000000000000000",
            ),
            None,
        ),
    ]
}

/// EVERY eligible member of every specimen gets exactly one mutant, and that
/// mutant genuinely changes the member's JSON type.
///
/// THE COUNT WAS A PROXY AND THIS IS THE PROPERTY.
/// `the_adversarial_surface_has_not_silently_shrunk` asserts totals — at least
/// six mutants per kind, at least a hundred overall — and a total cannot see a
/// family that skips a CLASS of member. `WrongMemberType` excluded every
/// non-string value, so no mutant ever carried `"retainedFields": 1234` or
/// `"pagination": 1234`, and both thresholds stayed satisfied while a large part
/// of the surface did not exist. Found by external review, in the file whose
/// purpose is to assert that this surface is closed.
///
/// Per member rather than in aggregate is the difference between "the generator
/// produced a lot" and "the generator produced one for each thing it covers".
#[test]
fn every_eligible_member_gets_exactly_one_wrong_type_mutant() {
    for (kind, specimen, _) in adversarial_cases() {
        let object = specimen.as_object().expect("a specimen is an object");
        let eligible: Vec<&String> = object
            .keys()
            .filter(|k| k.as_str() != "sourceKind")
            .collect();

        let generated = mutants(&specimen, Family::WrongMemberType, None);
        assert_eq!(
            generated.len(),
            eligible.len(),
            "{kind}: WrongMemberType generated {} mutant(s) for {} eligible member(s). A \
             family that names a class and covers part of it is a family whose green result \
             is about the part",
            generated.len(),
            eligible.len()
        );

        for member in eligible {
            let mut alone = generated.iter().filter(|(_, mutant)| {
                let mutated = mutant.as_object().expect("a mutant is an object");
                mutated.iter().all(|(k, v)| {
                    if k == member {
                        json_type(v) != json_type(&object[k])
                    } else {
                        v == &object[k]
                    }
                })
            });
            assert!(
                alone.next().is_some(),
                "{kind}: no mutant changes the TYPE of /{member} while leaving every other \
                 member alone. Replacing a number with another number is not a type \
                 mutation, and skipping the member is not one either"
            );
            assert!(
                alone.next().is_none(),
                "{kind}: more than one mutant changes /{member} alone; a duplicate inflates \
                 the surface count without widening the surface"
            );
        }
    }
}

/// The JSON type of a value, named by its variant.
fn json_type(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// The digest §13.1 binds `review-by-expected-author-login/1` to, taken from the
/// registry that binds it rather than copied as a literal that can go stale.
fn bound_implementation_digest() -> String {
    common::bound_matcher_digest("review-by-expected-author-login", "1")
}
