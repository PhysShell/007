//! The frozen specimens, executed.
//!
//! `tests/fixtures/closure-provenance/` was authored under PR #152 and its
//! README says, of the rule specimen G contradicts:
//!
//! > **The matcher rule is described, not implemented.** [...] Nothing in this
//! > repository executes it, so G's contradiction is verifiable by reading, not
//! > by running.
//!
//! This file is what changes that sentence's tense. It resolves the identity
//! pair the frozen specimens *already name* — `review-by-expected-author-login`
//! version `1`, which is why this crate's registry uses that name and not a
//! fresh one — and re-runs it over the retained candidates.
//!
//! THE SPECIMENS ARE INPUT, NOT ORACLE. Nothing here reads `satisfiesMatcherRule`
//! or `canonicalDigest` and then checks the matcher against it, except where the
//! point is precisely to compare an independently computed value against the
//! specimen's claim. A test that took the fixture's own annotation as the
//! expected answer would be the artifact certifying the thing it is checked
//! against — the failure mode this whole effort is named after.
//!
//! WHAT THIS DOES NOT DO. The specimens are synthetic and say so. Passing here
//! is evidence that the binding executes the rule the contract froze; it is not
//! evidence that anything in the specimens ever happened.

// Justification for the restriction-lint allowance, per AGENTS.md rule 4 and the
// precedent in `crates/o7-closure-classifier/tests/frozen_fixtures.rs`: every
// panic path below is this test's own assertion failing, or its own reading of a
// frozen specimen checked into this repository. A specimen that will not parse,
// or that lists a digest with no retained candidate, is a corpus defect this test
// exists to report rather than to tolerate.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::PathBuf;

use o7_closure_matcher::{recompute_matched, resolve, verify_matched, Candidate, RecordedMatcher};
use serde_json::Value;

fn fixture(name: &str) -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/closure-provenance")
        .join(name);
    serde_json::from_str(&fs::read_to_string(&path).expect("reading a frozen specimen"))
        .expect("a frozen specimen is JSON")
}

/// The matcher block, the candidates in observation order and the claimed
/// matched subsequence, read out of a specimen's own canonical query snapshot.
struct Query {
    recorded: RecordedMatcher,
    candidates: Vec<Candidate>,
}

fn query_of(name: &str) -> Query {
    let doc = fixture(name);
    let canonical = doc.get("canonical").expect("canonical query snapshot");

    // Candidate snapshots come from the specimen's `candidates` array; the
    // order asserted below is the query snapshot's `allReturnedSnapshotDigests`,
    // which is §13.2's observation order.
    let listed: Vec<String> = canonical
        .get("allReturnedSnapshotDigests")
        .and_then(Value::as_array)
        .expect("allReturnedSnapshotDigests")
        .iter()
        .map(|d| d.as_str().expect("a digest is a string").to_owned())
        .collect();

    let retained = doc
        .get("candidates")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let candidates: Vec<Candidate> = listed
        .iter()
        .map(|digest| {
            let entry = retained
                .iter()
                .find(|c| c.get("canonicalDigest").and_then(Value::as_str) == Some(digest))
                .unwrap_or_else(|| {
                    panic!("{name}: query lists {digest} but no candidate is retained for it")
                });
            Candidate {
                declared_digest: digest.clone(),
                snapshot: entry.get("canonical").expect("candidate canonical").clone(),
            }
        })
        .collect();

    Query {
        recorded: RecordedMatcher::from_query_snapshot(canonical)
            .unwrap_or_else(|e| panic!("{name}: reading the recorded matcher block: {e}")),
        candidates,
    }
}

/// The identity pair the frozen specimens name resolves. If it did not, this
/// crate would have bound a *different* rule under a new name and left the
/// specimens' matcher exactly as unresolvable as §23 recorded it.
#[test]
fn the_matcher_the_specimens_name_now_resolves() {
    for name in [
        "matcher-candidate-set-v1.json",
        "complete-empty-query-v1.json",
        "incomplete-query-v1.json",
        "recorded-implementation-v1.json",
    ] {
        let q = query_of(name);
        resolve(q.recorded.id(), q.recorded.version()).unwrap_or_else(|e| {
            panic!(
                "{name} names {}/{}: {e}",
                q.recorded.id(),
                q.recorded.version()
            )
        });
    }
}

/// Specimen G's whole purpose, executed. The query snapshot claims an empty
/// matched subset over a COMPLETE enumeration that returned two reviews; one of
/// them is by the expected author. Re-running the bound rule over the retained
/// candidates does not reproduce that claim.
#[test]
fn specimen_g_s_empty_matched_subset_does_not_survive_re_execution() {
    let q = query_of("matcher-candidate-set-v1.json");
    assert_eq!(q.candidates.len(), 2, "G retains two candidates");
    assert!(
        q.recorded.matched_snapshot_digests().is_empty(),
        "G claims an empty matched subset"
    );

    let recomputed = recompute_matched(&q.recorded, &q.candidates)
        .expect("re-executing the bound matcher over G's retained candidates");
    assert_eq!(
        recomputed.matched.len(),
        1,
        "exactly one of G's candidates is by the expected author; got {:?}",
        recomputed.matched
    );
    assert!(
        !verify_matched(&q.recorded, &q.candidates)
            .expect("verify")
            .reproduced,
        "G's empty matched subset must not verify"
    );

    // Only now, having computed the answer independently, compare it against
    // what the specimen says about itself. This direction is the point: the
    // specimen's annotation is a claim under test, not the expected value.
    let doc = fixture("matcher-candidate-set-v1.json");
    let annotated: Vec<String> = doc
        .get("candidates")
        .and_then(Value::as_array)
        .expect("candidates")
        .iter()
        .filter(|c| c.get("satisfiesMatcherRule").and_then(Value::as_bool) == Some(true))
        .map(|c| {
            c.get("canonicalDigest")
                .and_then(Value::as_str)
                .expect("digest")
                .to_owned()
        })
        .collect();
    assert_eq!(
        recomputed.matched, annotated,
        "the bound matcher and specimen G's prose annotation disagree about which \
         candidate qualifies — one of them is wrong and the specimen is frozen"
    );
}

/// C and G are the discriminating pair for §13's second half. Under a query
/// snapshot that kept only the matched set they would be one artifact; here they
/// are separated by execution, not by reading.
///
/// C: a COMPLETE enumeration that genuinely returned nothing. Re-running the
/// matcher over its (empty) candidate set reproduces its empty matched subset,
/// so C's absence claim stands. G's does not. Same claimed value, opposite
/// verdicts.
#[test]
fn the_c_g_pair_is_separated_by_re_execution_not_by_the_claimed_value() {
    let c = query_of("complete-empty-query-v1.json");
    let g = query_of("matcher-candidate-set-v1.json");

    assert_eq!(
        c.recorded.matched_snapshot_digests(),
        g.recorded.matched_snapshot_digests(),
        "both claim the same empty matched list"
    );
    assert!(c.candidates.is_empty());
    assert!(!g.candidates.is_empty());

    assert!(
        verify_matched(&c.recorded, &c.candidates)
            .expect("verify")
            .reproduced,
        "C's empty matched subset is reproduced by the bound matcher"
    );
    assert!(
        !verify_matched(&g.recorded, &g.candidates)
            .expect("verify")
            .reproduced,
        "G's is not"
    );
}

/// D is C with an unfinished enumeration. Re-execution reproduces its empty
/// matched subset too — and must, because the matcher's job is selection over
/// what was obtained, not adjudication of whether enough was obtained.
///
/// This is the boundary worth asserting explicitly: a green matcher recomputation
/// says the selection was faithful to the retained candidates. It says nothing
/// about `enumeration`, and a consumer that read it as an absence proof would be
/// treating D as C — the confusion §14 exists to prevent.
#[test]
fn re_execution_is_silent_about_whether_the_enumeration_finished() {
    let d = query_of("incomplete-query-v1.json");
    assert!(
        verify_matched(&d.recorded, &d.candidates)
            .expect("verify")
            .reproduced,
        "D's matched subset is faithful to the candidates it retained"
    );
    assert_eq!(
        fixture("incomplete-query-v1.json")
            .pointer("/canonical/enumeration")
            .and_then(Value::as_str),
        Some("INCOMPLETE"),
        "and D is still INCOMPLETE, which the matcher neither knows nor reports"
    );
}
