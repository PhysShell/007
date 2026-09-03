//! NORM/RED-FAILED-READ-CODE — M1, reported against b9eaa7d.
//!
//! §8.1's failed head read carried `reason`, declared and given no domain, so
//! it accepted any string. The event is a RETAINED object and it is UNGATED —
//! a control artifact — so no redaction gate ever examines it. An acquisition
//! layer writes it while holding an HTTP response and an authorization header,
//! which makes it the most natural place in this entire contract for a
//! credential to land:
//!
//! ```text
//! reason: "GET /repos/... failed: Authorization: Bearer ghp_..."
//! ```
//!
//! canonicalized, digested, and retained permanently.
//!
//! THE CONTRACT MOVED FIRST, AND IT HAD TO. Refusing free text where §8.1
//! permitted it would be a consumer inventing a norm — the direction §8.1
//! already refused once for `observedAt`, and the direction `correction_g3.rs`
//! recorded when it classified this member as a residual it would not close:
//! "a closed reason set is a contract change this file does not make on its
//! own". §8.1 now declares `reasonCode` over a closed four-value vocabulary, on
//! redaction §9.4's own argument, and §24.10 records the round.
//!
//! THE SECOND SURFACE, WHICH THE REVIEW DID NOT NAME. `HeadRead::Failed` took a
//! caller-supplied `String` and `staleness` interpolated it verbatim into its
//! verdict:
//!
//! ```text
//! CannotCheck { why: "head_before did not happen (Authorization: Bearer ghp_...)" }
//! ```
//!
//! That is not a retained-artifact question at all — it is this crate's own API
//! handing a credential to whatever logs the verdict. One closed vocabulary
//! closes both, which is why the repair changes the type rather than filtering
//! the string.
//!
//! ```text
//! M1-A  a retained FAILED event carrying free text validates        RED
//! M1-B  the verdict text cannot carry a caller's free string        RED
//! M1-C  each closed code is admissible                    BOUNDARY  admits
//! M1-D  a code outside the vocabulary is refused          BOUNDARY  §8.1
//! M1-E  an AVAILABLE event still needs its snapshotDigest BOUNDARY  §8.1
//! M1-F  §8.1 declares the closed vocabulary                       FREEZE
//! ```
//!
//! M1-B IS A COMPILE-TIME PROPERTY AND IS WITNESSED AS ONE. Once
//! `HeadRead::Failed` carries a closed enum there is no string to smuggle, so
//! the witness asserts over every code that the verdict contains none of the
//! text an acquisition layer might have had. A test that only checked one
//! code would be checking a habit.

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

const SECRET: &str = "ghp_the_entire_credential_rides_out_in_a_diagnostic";
const HEAD_SHA: &str = "1f2e3d4c5b6a798807162534435261708f9e0d1c";
const CONFIG: &str = "sha256:1111111111111111111111111111111111111111111111111111111111111111";

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
    fn retain(&mut self, record: &Value, assessed: &[&str]) -> String {
        let assessment = json!({
            "schemaVersion": 1, "sourceKind": "closure-retention-assessment",
            "redactionPolicyVersion": "1",
            "detector": {"id": "synthetic-detector", "version": "1", "configDigest": CONFIG},
            "representation": "decoded-source-field-values",
            "assessedFields": assessed, "coverageComplete": true,
            "outcome": "RETAIN", "observedAt": "2026-08-05T09:03:00Z",
        });
        let ad = self.put(&assessment);
        let rd = self.put(record);
        let binding = json!({
            "schemaVersion": 1, "sourceKind": "closure-retention-binding",
            "recordDigest": rd, "assessmentDigest": ad,
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

const HEAD_REQ: [&str; 4] = ["/head/ref", "/head/repo/full_name", "/head/sha", "/number"];

fn subject() -> Subject {
    Subject {
        repository: "PhysShell/007".to_owned(),
        pull_request: "9001".to_owned(),
        expected_sha: HEAD_SHA.to_owned(),
        expected_redaction_policy: "1".to_owned(),
        expected_detector: ExpectedDetector {
            id: "synthetic-detector".to_owned(),
            version: "1".to_owned(),
            config_digest: CONFIG.to_owned(),
        },
    }
}

/// A retained AVAILABLE event and the gated head snapshot it names, so the
/// witnesses below differ from a conformant pair in one member only.
fn available_event(store: &mut Store, role: &str, at: &str) -> String {
    let head = store.retain(
        &json!({
            "schemaVersion": 1, "sourceKind": "github-pull-request-head",
            "repository": "PhysShell/007", "pullRequest": "9001",
            "headSha": HEAD_SHA, "headRef": "claude/closure-classifier-provenance",
            "headRepoFullName": "PhysShell/007",
        }),
        &HEAD_REQ,
    );
    store.put(&json!({
        "schemaVersion": 1, "sourceKind": "github-head-read-event",
        "role": role, "acquisition": "AVAILABLE",
        "snapshotDigest": head, "observedAt": at,
    }))
}

/// The refusal text of a `CannotCheck`, or a description of whatever came
/// instead.
///
/// A function rather than a `let ... else { panic! }` at each site: this crate
/// denies `clippy::panic` in tests as well, and taking an allowance for nicer
/// control flow would spend a lint the E1 audit exists to keep honest. Every
/// caller asserts on the returned text, so a verdict that is not `CannotCheck`
/// fails its own assertion with the verdict printed.
fn cannot_check_why(verdict: &Staleness) -> String {
    match verdict {
        Staleness::CannotCheck { why } => why.clone(),
        other => format!("{other:?}"),
    }
}

/// M1-A — a retained FAILED event whose diagnostic carries a credential.
///
/// Consumed through the door as HEAD_BEFORE, so the refusal is the validator's
/// and not a role check's: a `reason` member §8.1 no longer declares is an
/// unknown member of a closed schema.
#[test]
fn m1a_a_failed_event_carrying_free_text_is_not_a_conforming_event() {
    let mut store = Store::default();
    let event = store.put(&json!({
        "schemaVersion": 1, "sourceKind": "github-head-read-event",
        "role": "HEAD_BEFORE", "acquisition": "FAILED",
        "reason": format!("GET /repos failed: Authorization: Bearer {SECRET}"),
        "observedAt": "2026-08-05T09:03:00Z",
    }));
    let after = available_event(&mut store, "HEAD_AFTER", "2026-08-05T09:03:01Z");
    let verdict = staleness(
        &subject(),
        &SubjectRead {
            before: HeadRead::Observed {
                event_digest: event,
            },
            after: HeadRead::Observed {
                event_digest: after,
            },
        },
        &store,
    );
    let why = cannot_check_why(&verdict);
    // THE PROPERTY IS THE DOOR, NOT THE VERDICT TEXT. An earlier draft asserted
    // that the credential did not reach `why`, and passed before the repair —
    // because a FAILED event is refused for its acquisition status on this path
    // and the diagnostic is never quoted. The harm §9.4's argument is about is
    // that the object VALIDATES and is retained permanently, so that is what
    // this measures: a `reason` member §8.1 no longer declares must make the
    // event a malformed artifact.
    assert!(
        why.contains("did not pass validation"),
        "M1-A: a retained head-read event carrying a credential in a free-text `reason` was \
         accepted as a CONFORMING artifact. §8.1 declares `reasonCode` over a closed \
         vocabulary and no longer declares `reason`, so this object is malformed — and \
         redaction §9.4's argument is why the member had to close: a closed field cannot carry \
         a secret out because its range does not depend on the content inspected. The object is \
         canonicalized, digested and retained whatever any verdict says about it: got {why}"
    );
}

/// M1-B — the second surface. A caller's free-text failure string must not be
/// expressible, so it cannot reach the verdict.
///
/// Asserted over EVERY code rather than one: a witness that checked a single
/// variant would be checking a habit, and the property is about the type.
#[test]
fn m1b_a_failed_read_cannot_carry_a_callers_free_text() {
    let mut store = Store::default();
    let before = available_event(&mut store, "HEAD_BEFORE", "2026-08-05T09:03:00Z");
    for code in [
        FailedRead::RateLimited,
        FailedRead::RequestFailed,
        FailedRead::NotFound,
        FailedRead::Unauthorized,
    ] {
        let verdict = staleness(
            &subject(),
            &SubjectRead {
                before: HeadRead::Observed {
                    event_digest: before.clone(),
                },
                after: HeadRead::Failed { code },
            },
            &store,
        );
        let why = cannot_check_why(&verdict);
        assert!(
            matches!(verdict, Staleness::CannotCheck { .. }),
            "M1-B: a failed HEAD_AFTER is CANNOT_CHECK by §8.1: got {verdict:?}"
        );
        assert!(
            !why.contains("Authorization") && !why.contains("ghp_"),
            "M1-B: the verdict text carries content a caller supplied. `HeadRead::Failed` takes \
             a closed code precisely so there is no string to smuggle: got {why}"
        );
    }
}

/// M1-C — BOUNDARY. Every code §8.1 declares is admissible in a retained event.
#[test]
fn m1c_each_closed_code_is_a_conforming_event() {
    for code in [
        "RATE_LIMITED",
        "REQUEST_FAILED",
        "NOT_FOUND",
        "UNAUTHORIZED",
    ] {
        let mut store = Store::default();
        let event = store.put(&json!({
            "schemaVersion": 1, "sourceKind": "github-head-read-event",
            "role": "HEAD_BEFORE", "acquisition": "FAILED",
            "reasonCode": code, "observedAt": "2026-08-05T09:03:00Z",
        }));
        let after = available_event(&mut store, "HEAD_AFTER", "2026-08-05T09:03:01Z");
        let verdict = staleness(
            &subject(),
            &SubjectRead {
                before: HeadRead::Observed {
                    event_digest: event,
                },
                after: HeadRead::Observed {
                    event_digest: after,
                },
            },
            &store,
        );
        let why = cannot_check_why(&verdict);
        assert!(
            matches!(verdict, Staleness::CannotCheck { .. }),
            "M1-C: a FAILED before-read is CANNOT_CHECK: got {verdict:?}"
        );
        assert!(
            !why.contains("did not pass validation"),
            "M1-C: {code:?} is one of §8.1's four codes and the event carrying it was refused as \
             malformed. A closed vocabulary that admits none of its own values has replaced the \
             rule with a prohibition: got {why}"
        );
    }
}

/// M1-D — BOUNDARY. A code outside the vocabulary is refused, which is what
/// makes the set closed rather than decorative.
#[test]
fn m1d_a_code_outside_the_vocabulary_is_refused() {
    let mut store = Store::default();
    let event = store.put(&json!({
        "schemaVersion": 1, "sourceKind": "github-head-read-event",
        "role": "HEAD_BEFORE", "acquisition": "FAILED",
        "reasonCode": "TEAPOT", "observedAt": "2026-08-05T09:03:00Z",
    }));
    let after = available_event(&mut store, "HEAD_AFTER", "2026-08-05T09:03:01Z");
    let verdict = staleness(
        &subject(),
        &SubjectRead {
            before: HeadRead::Observed {
                event_digest: event,
            },
            after: HeadRead::Observed {
                event_digest: after,
            },
        },
        &store,
    );
    let why = cannot_check_why(&verdict);
    assert!(
        why.contains("did not pass validation"),
        "M1-D: a reasonCode outside §8.1's four values was accepted. An open set with four \
         suggested members is not a closed vocabulary, and the range argument needs the set to \
         be closed: got {why}"
    );
}

/// M1-E — BOUNDARY. The AVAILABLE shape is untouched: it still requires the
/// snapshot digest, and a FAILED event still must not carry one.
#[test]
fn m1e_the_available_shape_is_unchanged() {
    let mut store = Store::default();
    let before = available_event(&mut store, "HEAD_BEFORE", "2026-08-05T09:03:00Z");
    let after = available_event(&mut store, "HEAD_AFTER", "2026-08-05T09:03:01Z");
    let verdict = staleness(
        &subject(),
        &SubjectRead {
            before: HeadRead::Observed {
                event_digest: before,
            },
            after: HeadRead::Observed {
                event_digest: after,
            },
        },
        &store,
    );
    assert!(
        matches!(verdict, Staleness::NotStale),
        "M1-E: two conformant AVAILABLE reads of the expected SHA no longer bracket the \
         evaluation. This round changed the FAILED shape and must not have touched this one: \
         got {verdict:?}"
    );
}

/// M1-F — FREEZE. §8.1 declares the member and its vocabulary.
#[test]
fn m1f_the_contract_declares_the_closed_vocabulary() {
    assert!(
        PROVENANCE.contains("  reasonCode      REQUIRED"),
        "§8.1's FAILED event no longer declares `reasonCode`. Every behavioural witness above \
         goes green if the member reverts to an undomained `reason`, with no code change at all"
    );
    assert!(
        PROVENANCE.contains("RATE_LIMITED  REQUEST_FAILED  NOT_FOUND  UNAUTHORIZED"),
        "§8.1 declares `reasonCode` but no longer states its four values. The closed SET is the \
         rule — a member named `reasonCode` whose domain nobody wrote down is the defect this \
         round repaired, wearing a better name"
    );
}
