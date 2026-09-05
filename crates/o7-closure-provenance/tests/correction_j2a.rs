//! RED-BRACKET-ORDER — J2a, adjudicated ACCEPT (P1) against frozen §8.1.
//!
//! §8.1's purpose, in the contract's own words:
//!
//! ```text
//! §8.1's whole purpose is to bracket an evaluation between two reads, and
//! `observedAt` is what says which read came first.
//! ```
//!
//! `staleness` never compares them. `observedAt` appears nowhere in `lib.rs`;
//! `resolve_head_sha` returns `Result<String, String>` — the SHA and nothing
//! else — so a `HEAD_BEFORE` at 10:00Z beside a `HEAD_AFTER` at 09:00Z resolves
//! cleanly, agrees with the expectation, and returns `NotStale`. Two reads in
//! that order bracket no interval; the evaluation they claim to enclose
//! happened outside them.
//!
//! THIS ONE IS SELF-INFLICTED, which is why it is recorded at length. G3 froze
//! `observedAt`'s value domain and argued for the freeze in exactly these
//! terms — "a value nothing constrains cannot order two events, so an
//! implementation that accepts any string has an event pair whose bracket is
//! decorative" — and then nothing ordered them. G4 corrected the order of the
//! CHECKS inside `staleness` and left the order of the READS unexamined. A
//! constrained domain with no comparison is the same decorative bracket the
//! freeze was written against, one round later and better documented.
//!
//! WHERE THE CHECK BELONGS, and it follows G4's established order rather than
//! competing with it:
//!
//! ```text
//! any required read unresolved   ->  CannotCheck   (G4)
//! else the pair does not bracket ->  CannotCheck   (here)
//! else any resolved SHA != expected -> Stale
//! else                              -> NotStale
//! ```
//!
//! G4's argument transfers without modification. A disagreement between the
//! expectation and a read does not establish that the head moved DURING the
//! evaluation; §8.1's bracket is what distinguishes that from an expectation
//! that was stale before the evaluation opened. A reversed pair is a missing
//! bracket by a second route, so `STALE` is as unsupported here as it is beside
//! a failed read. J2A-B is the witness that the two checks are in that order
//! and not the other one.
//!
//! WHY LEXICOGRAPHIC COMPARISON IS CHRONOLOGICAL COMPARISON HERE, and why
//! J2A-G guards it. §8.1 froze the value domain to exactly
//! `YYYY-MM-DDThh:mm:ssZ`: fixed width, every field zero-padded, always UTC, no
//! offset and no fractional part. Over that domain byte order and time order
//! are the same order. They stop being the same the moment the domain admits an
//! offset or a fractional second, and nothing in a string comparison would say
//! so — it would simply start being wrong.
//!
//! THE EQUAL-TIMESTAMP CASE IS NOT REPAIRED HERE, BY RULING, and J2A-E is what
//! holds that line. CodeRabbit prescribed strict `BEFORE < AFTER` and called
//! equal timestamps the same defect. The owner's adjudication split it:
//!
//! > §8.1 froze one-second timestamp precision. At that precision, two genuine
//! > reads bracketing a fast evaluation may serialize to the same timestamp.
//! > Conversely, equal serialized timestamps cannot themselves establish which
//! > read occurred first. This is therefore an evidence/contract precision
//! > issue, not a repair prescription that may be invented in implementation.
//!
//! Both halves of that are true at once, which is exactly why it is not an
//! implementation question. A reversed pair is contradicted by its own
//! evidence. An equal pair is UNDETERMINED at the precision the contract
//! froze — the witness is not adequate to settle it either way, and choosing a
//! verdict in code would be choosing which of the two true statements to
//! implement. It stays recorded as an open §8.1 witness-adequacy question and
//! `NotStale` is left exactly as it was.
//!
//! ```text
//! J2A-A  before 10:00Z, after 09:00Z, both SHAs as expected           RED
//! J2A-B  the same reversal with a DISAGREEING SHA — CannotCheck, not
//!        Stale, so the two checks are in G4's order                   RED
//! J2A-C  before 09:00Z, after 10:00Z                       BOUNDARY  NotStale
//! J2A-D  proper order, SHAs differ                         BOUNDARY  Stale
//! J2A-E  EQUAL timestamps — deliberately unchanged         BOUNDARY  NotStale
//! J2A-F  a failed read still wins over the ordering check  BOUNDARY  CannotCheck
//! J2A-G  §8.1's bracket sentence AND its frozen value domain     FREEZE
//! ```

use o7_closure_provenance::{
    staleness, ExpectedDetector, FailedRead, HeadRead, RetainedEvidence, Staleness, Subject,
    SubjectRead,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;

mod common;

const PROVENANCE: &str = include_str!("../../../docs/architecture/closure-source-provenance-v1.md");

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

/// One retained head read. The SHA and the instant are both the caller's, so
/// every witness below differs from its neighbour in exactly one of them.
fn head(store: &mut Store, role: &str, head_sha: &str, observed_at: &str) -> HeadRead {
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
        "observedAt": observed_at,
    }));
    HeadRead::Observed {
        event_digest: event,
    }
}

/// J2A-A — THE REPRODUCER. Two clean reads of the right subject, both agreeing
/// with the expectation, in an order that encloses nothing.
#[test]
fn j2aa_a_reversed_pair_does_not_bracket_the_evaluation() {
    let mut store = Store::default();
    let read = SubjectRead {
        before: head(&mut store, "HEAD_BEFORE", "aaaa", "2026-08-05T10:00:00Z"),
        after: head(&mut store, "HEAD_AFTER", "aaaa", "2026-08-05T09:00:00Z"),
    };
    let verdict = staleness(&subject("aaaa"), &read, &store);
    assert!(
        matches!(verdict, Staleness::CannotCheck { .. }),
        "J2A-A: reported that the subject did not move, over a pair whose `after` read happened \
         an hour BEFORE its `before` read. §8.1's whole purpose is to bracket an evaluation \
         between two reads and `observedAt` is what says which came first; these two enclose no \
         interval, so the head is unwitnessed for the whole of the one that matters: got \
         {verdict:?}"
    );
}

/// J2A-B — the same reversal with a SHA that disagrees, which is what pins the
/// new check ahead of the disagreement scan rather than behind it.
///
/// G4 established why: a disagreement between the expectation and a read does
/// not establish that the head moved DURING the evaluation, and §8.1's bracket
/// is the thing that distinguishes that from an expectation which was already
/// stale. A reversed pair is a missing bracket by a second route. Answering
/// `Stale` here would be G4's defect returning through a door G4 did not check.
#[test]
fn j2ab_a_reversed_pair_with_a_disagreeing_read_is_not_stale_either() {
    let mut store = Store::default();
    let read = SubjectRead {
        before: head(&mut store, "HEAD_BEFORE", "bbbb", "2026-08-05T10:00:00Z"),
        after: head(&mut store, "HEAD_AFTER", "bbbb", "2026-08-05T09:00:00Z"),
    };
    let verdict = staleness(&subject("aaaa"), &read, &store);
    assert!(
        matches!(verdict, Staleness::CannotCheck { .. }),
        "J2A-B: answered a verdict about movement DURING an evaluation from a pair that does \
         not enclose one. STALE is not the safe answer here — it is the confident one, and it \
         is the exact substitution G4 removed at the other end of the same function: got \
         {verdict:?}"
    );
}

/// J2A-C — BOUNDARY. The ordinary shape, and it must stay `NotStale`.
#[test]
fn j2ac_a_properly_ordered_pair_still_witnesses_a_subject_that_did_not_move() {
    let mut store = Store::default();
    let read = SubjectRead {
        before: head(&mut store, "HEAD_BEFORE", "aaaa", "2026-08-05T09:00:00Z"),
        after: head(&mut store, "HEAD_AFTER", "aaaa", "2026-08-05T10:00:00Z"),
    };
    let verdict = staleness(&subject("aaaa"), &read, &store);
    assert!(
        matches!(verdict, Staleness::NotStale),
        "J2A-C: refused the only shape that witnesses a subject which did not move. A repair \
         that cannot admit this has deleted the verdict rather than evidenced it: got {verdict:?}"
    );
}

/// J2A-D — BOUNDARY. A complete, correctly ordered bracket whose second read
/// disagrees IS the case `STALE` names, and G4-D must not be undone here.
#[test]
fn j2ad_a_properly_ordered_pair_that_disagrees_is_still_stale() {
    let mut store = Store::default();
    let read = SubjectRead {
        before: head(&mut store, "HEAD_BEFORE", "aaaa", "2026-08-05T09:00:00Z"),
        after: head(&mut store, "HEAD_AFTER", "bbbb", "2026-08-05T10:00:00Z"),
    };
    let verdict = staleness(&subject("aaaa"), &read, &store);
    assert!(
        matches!(verdict, Staleness::Stale { .. }),
        "J2A-D: an ordered bracket whose second read disagrees is exactly what STALE is for: \
         got {verdict:?}"
    );
}

/// J2A-E — BOUNDARY, AND IT IS A DELIBERATE NON-DECISION.
///
/// Two reads that serialize to the same second. CodeRabbit prescribed strict
/// `BEFORE < AFTER` and called this the same defect as the reversal; the
/// adjudication did not, and this witness is where that ruling is executable.
///
/// The reason is the frozen precision. §8.1 froze `observedAt` to whole
/// seconds, so two genuine reads bracketing a fast evaluation legitimately
/// share one — and equally, two timestamps that are equal cannot themselves say
/// which read came first. Both statements are true at once, which is what makes
/// this an evidence-adequacy question about §8.1 rather than a rule an
/// implementation may pick. A reversed pair is contradicted by its own
/// evidence; an equal pair is undetermined by it.
///
/// RECORDED AS OPEN, NOT AS SETTLED. If §8.1 later gains sub-second precision
/// or an explicit ordering obligation, this witness is the one that must be
/// revisited, and it is written to fail loudly rather than drift if the
/// behaviour changes without the contract changing.
#[test]
fn j2ae_equal_timestamps_are_left_exactly_as_they_were() {
    let mut store = Store::default();
    let read = SubjectRead {
        before: head(&mut store, "HEAD_BEFORE", "aaaa", "2026-08-05T09:00:00Z"),
        after: head(&mut store, "HEAD_AFTER", "aaaa", "2026-08-05T09:00:00Z"),
    };
    let verdict = staleness(&subject("aaaa"), &read, &store);
    assert!(
        matches!(verdict, Staleness::NotStale),
        "J2A-E: the equal-timestamp case changed behaviour. It was adjudicated NOT to be the \
         same defect as the reversal and NOT to be repaired in this round: §8.1 froze \
         one-second precision, so two genuine reads around a fast evaluation share a second, \
         and a strict `<` would refuse conformant evidence. If this needs to become a refusal, \
         it needs §8.1 to say so first — an implementation choosing between two true readings \
         of an inadequate witness is the direction G3 refused: got {verdict:?}"
    );
}

/// J2A-F — BOUNDARY. An unresolved read still wins, so the ordering check
/// cannot become a route around G4's rule. There is no second timestamp to
/// compare when one end never produced an event, and inventing an order for
/// that pair would answer a question no evidence was offered for.
#[test]
fn j2af_an_unresolved_read_is_still_refused_before_any_ordering_question() {
    let mut store = Store::default();
    let read = SubjectRead {
        before: head(&mut store, "HEAD_BEFORE", "bbbb", "2026-08-05T10:00:00Z"),
        after: HeadRead::Failed {
            code: FailedRead::RequestFailed,
        },
    };
    let verdict = staleness(&subject("aaaa"), &read, &store);
    assert!(
        matches!(verdict, Staleness::CannotCheck { .. }),
        "J2A-F: G4's rule must survive this round: a read that did not happen yields \
         CANNOT_CHECK and never a silent absence of STALE: got {verdict:?}"
    );
}

/// J2A-G — FREEZE, and it guards two sentences rather than one.
///
/// The first is §8.1's purpose, which is what makes a reversed pair a defect at
/// all. The second is the frozen value domain, which is what makes comparing
/// two of these strings the same thing as comparing two instants. If the domain
/// ever admits an offset or a fractional second, a lexicographic comparison
/// does not start failing — it starts being quietly wrong, which is the failure
/// mode this whole crate exists to refuse.
#[test]
fn j2ag_the_frozen_purpose_and_the_frozen_domain_are_both_still_frozen() {
    assert!(
        PROVENANCE.contains("bracket an\nevaluation between two reads, and `observedAt` is what")
            || PROVENANCE.contains("bracket an evaluation between two reads"),
        "§8.1 no longer says the two reads bracket an evaluation, which is the sentence J2a \
         turns on. If that was deliberate, J2a is not a defect and this file should be deleted \
         rather than quietly passing"
    );
    assert!(
        PROVENANCE.contains("A value nothing constrains cannot order two events"),
        "§8.1 no longer states that an unconstrained value cannot order two events. That is \
         the argument G3's freeze was made on and the argument this repair completes"
    );
    assert!(
        PROVENANCE.contains("A conforming value matches exactly `YYYY-MM-DDThh:mm:ssZ`"),
        "§8.1 no longer froze `observedAt` to exactly `YYYY-MM-DDThh:mm:ssZ`. Comparing two of \
         these strings is comparing two instants ONLY over that domain: admit an offset or a \
         fractional second and byte order stops being time order, silently and without any \
         comparison failing"
    );
}
