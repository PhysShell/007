//! GREEN-3. The implementation binding's authority moves out of this source tree.
//!
//! THE PROBLEM RED-3 LANDED. 48548ef bound `(id, version)` to a SHA-256 of the
//! predicate file's bytes. That fixed *what* the certificate covers. It left the
//! certificate where it was — in `src/matchers.rs`, two lines from the
//! `include_str!` that supplies the bytes it judges:
//!
//! ```text
//! implementation_source: include_str!("matchers/review_..._v1.rs"),
//! implementation_digest: "sha256:59ea...",
//! ```
//!
//! One commit edits both, and `verify_implementation()` is satisfied. So it
//! establishes that two current fields agree, not that `/1` still means what
//! `/1` meant — an artifact certifying the thing it is checked against, third
//! time, one level up each time.
//!
//! WHAT CHANGED. `github-query-snapshot` gains `matcher.implementationDigest` at
//! `schemaVersion` 2, so a durable artifact records which implementation
//! produced its matched subsequence. The expected value is then written by a
//! different act, at a different time, and covered by the artifact's own digest.
//! Specimen I is that record, and the tests below read their expectation out of
//! it — never out of `REGISTRY`.
//!
//! ```text
//! before:  hash(bytes in tree) == constant in tree
//! after:   hash(bytes in tree) == digest in an artifact this tree did not write
//! ```
//!
//! WHAT THIS DOES AND DOES NOT ESTABLISH. It is not tamper-proofing, and calling
//! it that would repeat the overclaim in 48548ef that RED-3 had to demolish. A
//! commit that edits the predicate, the registry constant, specimen I's
//! `implementationDigest` and specimen I's `canonicalDigest` still passes, and no
//! arrangement of files inside one repository can prevent that — the tree is
//! writable by whoever writes the tree. Three things are true instead, and they
//! are the whole claim:
//!
//! - The record and the implementation are no longer edited by the same act.
//!   Drift is now a four-file diff in a fixture reviewers read, not two adjacent
//!   lines.
//! - Specimen I's digest was computed by rfc8785 0.1.4 outside this workspace,
//!   per the corpus rule, so re-blessing it means going back to the external
//!   tool rather than running the code under test.
//! - The part that actually leaves reach: an emitted closure artifact — an
//!   attestation, a snapshot already handed to someone — carries the digest of
//!   the code that produced it and is not in this repository at all. Specimen I
//!   is that situation's stand-in; the real binding starts when artifacts are
//!   emitted. Within the tree the mechanism catches drift, which is the failure
//!   that actually happens; it does not catch an author who means it.

// Justification for the restriction-lint allowance, per AGENTS.md rule 4 and the
// precedent in `crates/o7-closure-classifier/tests/frozen_fixtures.rs`: every
// panic path below is this test's own assertion failing, or its own reading of
// specimen I, which is checked into this repository. A specimen that will not
// parse, or that has lost the `implementationDigest` this file exists to check,
// is a corpus defect that must fail loudly — silently skipping it would turn this
// test into the vacuous green it was written to prevent.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use o7_closure_matcher::{
    check_recorded_implementation, recompute_matched, resolve, Candidate, ImplementationCheck,
    MatchError, RecordedImplementation, RecordedMatcher, RecordedQuerySnapshot,
};
use serde_json::Value;
use std::fs;
use std::path::PathBuf;

fn fixture(name: &str) -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/closure-provenance")
        .join(name);
    serde_json::from_str(&fs::read_to_string(&path).expect("reading a frozen specimen"))
        .expect("a frozen specimen is JSON")
}

const SPECIMEN_I: &str = "recorded-implementation-v1.json";

fn specimen_i_snapshot() -> Value {
    fixture(SPECIMEN_I)
        .get("canonical")
        .expect("canonical query snapshot")
        .clone()
}

fn specimen_i_candidates() -> Vec<Candidate> {
    let doc = fixture(SPECIMEN_I);
    let listed: Vec<String> = doc
        .pointer("/canonical/allReturnedSnapshotDigests")
        .and_then(Value::as_array)
        .expect("allReturnedSnapshotDigests")
        .iter()
        .map(|d| d.as_str().expect("digest").to_owned())
        .collect();
    let retained = doc
        .get("candidates")
        .and_then(Value::as_array)
        .expect("candidates")
        .clone();
    listed
        .into_iter()
        .map(|digest| {
            let found = retained
                .iter()
                .find(|c| c.get("canonicalDigest").and_then(Value::as_str) == Some(&digest))
                .unwrap_or_else(|| panic!("no candidate retained for {digest}"));
            Candidate {
                declared_digest: digest,
                snapshot: found.get("canonical").expect("canonical").clone(),
            }
        })
        .collect()
}

/// Public constructors declared in one `impl` block's source text.
///
/// Scans items rather than lines: rustfmt wraps a multi-argument signature, so
/// `pub fn` and `-> Result<Self>` land on different lines and a line-based filter
/// silently finds nothing — reporting "zero constructors" for a type that has
/// one. A structural check that cannot see what it checks is the failure mode
/// this crate is about, arriving in the test that asserts it.
fn public_constructors(source: &str, type_name: &str) -> Vec<String> {
    let marker = format!("impl {type_name} {{");
    let body = source
        .split_once(marker.as_str())
        .unwrap_or_else(|| panic!("{type_name} has an impl block"))
        .1
        .split_once("\n}")
        .expect("the impl closes")
        .0;
    body.match_indices("pub fn ")
        .filter_map(|(at, _)| {
            let item = body.get(at..)?;
            let head = item.get(..item.len().min(400))?;
            let signature = head.split_once('{')?.0;
            (signature.contains("-> Result<Self") || signature.contains("-> Self"))
                .then(|| signature.split_whitespace().collect::<Vec<_>>().join(" "))
        })
        .collect()
}

/// Bind a snapshot to a digest supplied for it, then take the matcher.
fn bind(snapshot: &Value, expected: &str) -> Result<RecordedMatcher, MatchError> {
    RecordedQuerySnapshot::from_canonical(snapshot, expected).map(|q| q.recorded_matcher().clone())
}

/// A specimen's own `canonicalDigest`, computed with rfc8785 0.1.4 outside this
/// workspace. This is what the binding is checked against, and the reason these
/// tests say something the synthetic helpers elsewhere cannot.
fn fixture_digest(name: &str) -> String {
    fixture(name)
        .get("canonicalDigest")
        .and_then(Value::as_str)
        .expect("a specimen carries its canonicalDigest")
        .to_owned()
}

/// For a snapshot this test built or mutated: the digest is derived from the
/// bytes in hand, so it is self-consistent by construction. Used where the
/// subject is the matcher block's shape rather than the binding.
fn self_bound(snapshot: &Value) -> Result<RecordedMatcher, MatchError> {
    let computed = o7_closure_canonical::digest(snapshot).expect("digest");
    bind(snapshot, computed.as_str())
}

/// The expected digest is a literal in a JSON file that this crate does not
/// write. If it ever starts coming from `REGISTRY`, every test below reverts to
/// checking that the tree agrees with itself.
#[test]
fn the_expectation_is_a_literal_in_the_frozen_corpus() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/closure-provenance")
        .join(SPECIMEN_I);
    let raw = fs::read_to_string(&path).expect("read");
    let recorded = fixture(SPECIMEN_I)
        .pointer("/canonical/matcher/implementationDigest")
        .and_then(Value::as_str)
        .expect("specimen I records an implementation digest")
        .to_owned();
    assert!(
        raw.contains(&recorded),
        "the recorded digest must be present as text in the fixture"
    );
    assert!(
        recorded.starts_with("sha256:") && recorded.len() == 71,
        "recorded digest has the frozen syntax: {recorded}"
    );
}

/// Replaying specimen I: the selection reproduces AND the implementation that
/// reproduced it is the one the artifact named.
#[test]
fn replaying_the_recorded_snapshot_binds_the_implementation_that_ran() {
    let snapshot = specimen_i_snapshot();
    let recorded = bind(&snapshot, &fixture_digest(SPECIMEN_I)).expect("recorded matcher");
    let claimed: Vec<String> = snapshot
        .get("matchedSnapshotDigests")
        .and_then(Value::as_array)
        .expect("matchedSnapshotDigests")
        .iter()
        .map(|d| d.as_str().expect("digest").to_owned())
        .collect();

    let replay = recompute_matched(&recorded, &specimen_i_candidates()).expect("replay");
    assert_eq!(
        replay.matched, claimed,
        "specimen I's claim reproduces, so a failure here can only be the binding"
    );
    let RecordedImplementation::Bound(expected) = recorded.implementation() else {
        panic!("specimen I is a version-2 snapshot and must record an implementation");
    };
    assert_eq!(
        replay.implementation,
        ImplementationCheck::Bound {
            digest: expected.clone()
        }
    );
}

/// The discriminating case, and the one RED-3 walks through today: the artifact
/// names an implementation this tree does not resolve to.
///
/// A mutant moves the tree while the record stays put; this test moves the
/// record while the tree stays put. Those are the same comparison approached
/// from opposite sides, and only the first is what actually happens — which is
/// why the RED-3 commit exists as history rather than as an assertion here.
#[test]
fn a_recorded_digest_this_tree_does_not_resolve_to_is_refused() {
    let recorded = bind(&specimen_i_snapshot(), &fixture_digest(SPECIMEN_I)).expect("recorded");
    let entry = resolve(recorded.id(), recorded.version()).expect("resolve");

    let drifted = RecordedImplementation::Bound(
        "sha256:0000000000000000000000000000000000000000000000000000000000000000".to_owned(),
    );
    match check_recorded_implementation(entry, &drifted) {
        Err(MatchError::RecordedImplementationDrift {
            recorded, resolved, ..
        }) => {
            assert_ne!(recorded, resolved);
        }
        other => panic!("drift must be refused, got {other:?}"),
    }
}

/// And drift is refused through the replay path too, not only when called
/// directly — a caller cannot recompute a matched subsequence while ignoring
/// which code recomputed it, because there is no entry point that omits it.
#[test]
fn replay_refuses_drift_rather_than_reporting_it_alongside_a_result() {
    // Built by editing the SNAPSHOT, not the parsed value: RecordedMatcher has
    // no public fields, so a drifted record can only come from a drifted
    // artifact. That is the point — a caller cannot manufacture agreement.
    let mut drifted = specimen_i_snapshot();
    drifted
        .pointer_mut("/matcher")
        .and_then(Value::as_object_mut)
        .expect("matcher object")
        .insert(
            "implementationDigest".to_owned(),
            Value::String(
                "sha256:1111111111111111111111111111111111111111111111111111111111111111"
                    .to_owned(),
            ),
        );
    // A drifted snapshot is a DIFFERENT artifact and is bound to its own digest.
    // The query binding and the implementation binding are separate checks; this
    // test is about the second.
    let recorded = self_bound(&drifted).expect("recorded");
    assert!(matches!(
        recompute_matched(&recorded, &specimen_i_candidates()),
        Err(MatchError::RecordedImplementationDrift { .. })
    ));
}

/// A version-1 snapshot predates the field. That is CANNOT_CHECK — an axis with
/// no evidence — and must never read as a passed one. C, D and G stay at version
/// 1 precisely so this case keeps a witness.
#[test]
fn a_version_one_snapshot_cannot_check_the_implementation() {
    for name in [
        "matcher-candidate-set-v1.json",
        "complete-empty-query-v1.json",
        "incomplete-query-v1.json",
    ] {
        let snapshot = fixture(name).get("canonical").expect("canonical").clone();
        let recorded =
            bind(&snapshot, &fixture_digest(name)).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(
            recorded.implementation(),
            &RecordedImplementation::Unrecorded,
            "{name} is a version-1 snapshot"
        );
        let entry = resolve(recorded.id(), recorded.version()).expect("resolve");
        assert_eq!(
            check_recorded_implementation(entry, recorded.implementation()).expect("check"),
            ImplementationCheck::CannotCheck,
            "{name} carries no implementation evidence, so nothing about the \
             implementation is established — including that it did not drift"
        );
    }
}

/// Corpus-wide rather than specimen-wide: every version-2 query snapshot the
/// corpus holds must bind to an implementation this tree resolves. A future v2
/// specimen extends this check by existing, and deleting the one specimen that
/// currently drives it is visible as coverage going to zero rather than as a
/// test quietly passing over an empty set.
#[test]
fn every_recorded_implementation_in_the_corpus_binds() {
    let dir =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/closure-provenance");
    let mut bound = 0;
    for entry in fs::read_dir(&dir).expect("corpus") {
        let path = entry.expect("dir entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let doc: Value =
            serde_json::from_str(&fs::read_to_string(&path).expect("read")).expect("parse");
        let Some(snapshot) = doc.get("canonical") else {
            continue;
        };
        if snapshot.get("sourceKind").and_then(Value::as_str) != Some("github-query-snapshot") {
            continue;
        }
        let expected = doc
            .get("canonicalDigest")
            .and_then(Value::as_str)
            .unwrap_or_else(|| panic!("{path:?}: no canonicalDigest"));
        let recorded = bind(snapshot, expected).unwrap_or_else(|e| panic!("{path:?}: {e}"));
        let RecordedImplementation::Bound(_) = recorded.implementation() else {
            continue;
        };
        let resolved =
            resolve(recorded.id(), recorded.version()).unwrap_or_else(|e| panic!("{path:?}: {e}"));
        let check = check_recorded_implementation(resolved, recorded.implementation())
            .unwrap_or_else(|e| panic!("{path:?}: {e}"));
        assert!(
            matches!(check, ImplementationCheck::Bound { .. }),
            "{path:?} records an implementation, so the check must bind it"
        );
        bound += 1;
    }
    assert!(
        bound >= 1,
        "no version-2 query snapshot in the corpus records an implementation, so \
         nothing outside this tree has an opinion about what /1 is"
    );
}

/// §13's two shapes are closed key sets. A shape that can borrow a field from
/// its neighbour is not closed, so both directions are refused.
///
/// The refusal now comes from the closed-shape check over the whole snapshot
/// rather than from the matcher parser's own version match: `implementationDigest`
/// is simply absent from the version-1 member table and REQUIRED in the
/// version-2 one, so the general rule produces this specific outcome instead of
/// the two being stated separately. The matcher parser still branches on the
/// version, because it has to in order to extract, and its refusals are now a
/// redundant second statement of the same rule.
#[test]
fn the_two_snapshot_shapes_do_not_borrow_fields_from_each_other() {
    let mut v1_with_digest = fixture("matcher-candidate-set-v1.json")
        .get("canonical")
        .expect("canonical")
        .clone();
    v1_with_digest
        .pointer_mut("/matcher")
        .and_then(Value::as_object_mut)
        .expect("matcher object")
        .insert(
            "implementationDigest".to_owned(),
            Value::String("sha256:00".to_owned()),
        );
    assert!(matches!(
        self_bound(&v1_with_digest),
        Err(MatchError::MalformedQuerySnapshot { .. })
    ));

    let mut v2_without_digest = specimen_i_snapshot();
    v2_without_digest
        .pointer_mut("/matcher")
        .and_then(Value::as_object_mut)
        .expect("matcher object")
        .remove("implementationDigest");
    assert!(matches!(
        self_bound(&v2_without_digest),
        Err(MatchError::MalformedQuerySnapshot { .. })
    ));
}

// ---- The binding is structural, not conventional.

/// `RecordedMatcher` has no public fields and exactly one constructor.
///
/// Every value it holds is something replay is checked against, and §13.1 says
/// nothing being checked may arrive from the party being checked. With public
/// fields that is a convention: parse a snapshot whose claim is false, assign
/// the recomputed list over `matched_snapshot_digests`, and `verify_matched`
/// reports agreement — the same bypass that removing the `claimed` parameter
/// closed, reopened through assignment.
///
/// Asserted against the source because it is a property of the type rather than
/// of any execution, and because the compiler enforcing it today is not evidence
/// that a later `pub` would be noticed. This is the same kind of tripwire as the
/// import allowlist in `ambient_state.rs`, with the same limits.
#[test]
fn the_recorded_matcher_cannot_be_assembled_by_a_caller() {
    let source = std::fs::read_to_string(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/lib.rs"),
    )
    .expect("reading the crate source");

    for ty in ["RecordedMatcher", "RecordedQuerySnapshot"] {
        let decl = format!("pub struct {ty} {{");
        let fields = source
            .split_once(decl.as_str())
            .unwrap_or_else(|| panic!("{ty} is declared"))
            .1
            .split_once("\n}")
            .expect("its declaration closes")
            .0;
        let public: Vec<&str> = fields
            .lines()
            .map(str::trim)
            .filter(|l| l.starts_with("pub "))
            .collect();
        assert!(
            public.is_empty(),
            "{ty} has public fields {public:?}; a caller who can assign to one can \
             manufacture agreement with a claim the artifact never made"
        );
    }

    // RecordedMatcher has NO public constructor. Private fields stopped a caller
    // assembling one; a public `&Value -> RecordedMatcher` still let them mutate a
    // retained snapshot and parse the result — the same forgery one step earlier.
    let matcher_ctors = public_constructors(&source, "RecordedMatcher");
    assert!(
        matcher_ctors.is_empty(),
        "RecordedMatcher must have no public constructor; found {matcher_ctors:?}"
    );

    // The one door that exists takes the expected digest as an argument. Without
    // it, it is the unbound `&Value` entry point wearing a different type name.
    let snapshot_ctors = public_constructors(&source, "RecordedQuerySnapshot");
    assert_eq!(
        snapshot_ctors.len(),
        1,
        "expected exactly one constructor, found {snapshot_ctors:?}"
    );
    let only = snapshot_ctors.first().expect("checked non-empty");
    assert!(
        only.contains("from_canonical"),
        "the one constructor must read an artifact: {only}"
    );
    assert!(
        only.contains("expected_query_digest"),
        "it must take the expected digest, or it is the unbound entry point under a \
         new name: {only}"
    );
}

// ---- The query snapshot joins the content-addressed chain.
//
// Before this, candidates were bound (each candidate's digest recomputed and
// checked against what the query snapshot declared) and the query snapshot was
// bound to nothing. The chain of custody terminated on an unbound object one
// step above the part that was careful, so making RecordedMatcher's fields
// private only moved the forgery earlier: mutate a retained snapshot, parse it,
// and the parsed value is "what the artifact said".
//
// What these witnesses establish, and the limit, stated together so the limit is
// not lost:
//
//     bytes + expected digest, mismatched  ->  REFUSE
//     bytes + expected digest, matching    ->  these are the bytes that digest names
//
// A forged snapshot presented with the digest of that same forgery is internally
// consistent and still passes. This is content binding relative to an
// expectation, not authentication. The expectation's authority belongs to the
// layer that retained it (Slice B), its production to acquisition (Slice C), and
// its authenticity to attestation (Slice D).

/// Specimen I, bound to its own externally computed digest, yields its matcher.
#[test]
fn a_snapshot_that_hashes_to_its_recorded_digest_is_admissible() {
    let bound =
        RecordedQuerySnapshot::from_canonical(&specimen_i_snapshot(), &fixture_digest(SPECIMEN_I))
            .expect("specimen I hashes to its own canonicalDigest");
    assert_eq!(bound.digest(), fixture_digest(SPECIMEN_I));
    assert_eq!(
        bound.recorded_matcher().id(),
        "review-by-expected-author-login"
    );
}

/// The claim cannot be edited under a retained digest.
#[test]
fn mutating_the_claim_under_the_recorded_digest_is_refused() {
    let mut forged = specimen_i_snapshot();
    let recomputed: Vec<Value> = specimen_i_candidates()
        .iter()
        .map(|c| Value::String(c.declared_digest.clone()))
        .collect();
    forged.as_object_mut().expect("snapshot object").insert(
        "matchedSnapshotDigests".to_owned(),
        Value::Array(recomputed),
    );

    match RecordedQuerySnapshot::from_canonical(&forged, &fixture_digest(SPECIMEN_I)) {
        Err(MatchError::QuerySnapshotDigestMismatch { expected, computed }) => {
            assert_eq!(expected, fixture_digest(SPECIMEN_I));
            assert_ne!(computed, expected, "the forgery hashes elsewhere");
        }
        other => panic!("an edited claim under a retained digest must be refused: {other:?}"),
    }
}

/// And the same for the declared candidate sequence — the check covers the whole
/// artifact, not the one field a previous round happened to name.
#[test]
fn mutating_the_declared_candidates_under_the_recorded_digest_is_refused() {
    let mut forged = specimen_i_snapshot();
    forged.as_object_mut().expect("snapshot object").insert(
        "allReturnedSnapshotDigests".to_owned(),
        Value::Array(Vec::new()),
    );

    assert!(
        matches!(
            RecordedQuerySnapshot::from_canonical(&forged, &fixture_digest(SPECIMEN_I)),
            Err(MatchError::QuerySnapshotDigestMismatch { .. })
        ),
        "an edited candidate sequence under a retained digest must be refused"
    );
}

/// The residual, asserted rather than only described: a forgery carrying its own
/// digest passes. Recorded as a test so nobody later reads the two witnesses
/// above and concludes the constructor authenticates anything.
#[test]
fn a_forgery_presented_with_its_own_digest_still_passes() {
    let mut forged = specimen_i_snapshot();
    forged.as_object_mut().expect("snapshot object").insert(
        "allReturnedSnapshotDigests".to_owned(),
        Value::Array(Vec::new()),
    );
    let its_own = o7_closure_canonical::digest(&forged).expect("digest");

    let bound = RecordedQuerySnapshot::from_canonical(&forged, its_own.as_str())
        .expect("self-consistent by construction, and that is the point");
    assert!(
        bound
            .recorded_matcher()
            .all_returned_snapshot_digests()
            .is_empty(),
        "content binding relative to an expectation is not authentication: the \
         authority of the expected digest comes from the layer that retained it"
    );
}
