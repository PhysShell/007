//! RED-3. The witness that `implementation_digest` is not yet an *immutable*
//! binding — only a consistency check between two fields that move together.
//!
//! WHAT IS WRONG WITH THIS COMMIT. `review-by-expected-author-login/1` has been
//! changed to refuse a `DISMISSED` review by the expected author, and the
//! `implementation_digest` constant beside it has been recomputed to match. The
//! version is still `"1"`. Do not build on this commit; the next one reverts it.
//!
//! WHY RED-2 DID NOT COVER THIS. RED-2 showed a one-field edit escaping a digest
//! over a finite vector set, and 48548ef fixed that by hashing the implementation
//! bytes. But the authoritative expected value lives in `src/matchers.rs`,
//! two lines from the `include_str!` that supplies the bytes it judges:
//!
//! ```text
//! implementation_source: include_str!("matchers/review_..._v1.rs"),   <- the bytes
//! implementation_digest: "sha256:59ea...",                            <- the verdict
//! ```
//!
//! `verify_implementation()` hashes the first and compares it to the second. Both
//! are editable in one commit, so the check confirms that two current fields agree
//! and says nothing about whether `/1` still means what `/1` meant. The claim in
//! 48548ef that this was "append-only, enforced by the digest rather than by
//! policy" was stronger than the mechanism: the digest refuses an implementation
//! edit that forgets to update the digest, and permits one that remembers.
//!
//! ```text
//! artifact:  "my implementation digest is D"
//! checker:   hash(bytes) == artifact.D
//!         ↓
//! two current fields agree            <- established
//! this version is what it always was  <- NOT established
//! ```
//!
//! Which is the effort's own recurring defect, third hat: an artifact certifying
//! the very thing it is being checked against. RED-2 moved the certificate from
//! a sample of behaviour to the bytes; it did not move the *authority* anywhere.
//!
//! WHERE THE AUTHORITY HAS TO LIVE. Not in a second copy of the constant in this
//! tree — that is the same file one directory over. It has to be a record written
//! by a different act at a different time: a durable artifact that says which
//! implementation it actually ran. The next commit puts `implementationDigest`
//! into the query snapshot's matcher block, so a closure artifact carries the
//! identity of the code that produced it and cannot be re-blessed by later code
//! claiming the same `/1`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use o7_closure_matcher::{resolve, verify_binding, REGISTRY};
use serde_json::{json, Value};

fn dismissed_review_by_expected_author() -> Value {
    json!({
        "schemaVersion": 1,
        "sourceKind": "github-submitted-review",
        "stableId": "88",
        "user": { "id": "88", "login": "expected-reviewer", "type": "User" },
        "authorAssociation": "NONE",
        "state": "DISMISSED",
        "body": "",
        "submittedAt": "2026-01-01T00:00:00Z",
        "commitId": "2222222222222222222222222222222222222222"
    })
}

/// Half one of the mutation is real: `/1` answers differently than it did at
/// a82f065 for an input the matcher is squarely responsible for.
#[test]
fn red3_the_behaviour_of_version_1_has_changed_again() {
    let entry = resolve("review-by-expected-author-login", "1").expect("resolve");
    let got = (entry.predicate)(
        &dismissed_review_by_expected_author(),
        &json!({ "expectedAuthorLogin": "expected-reviewer" }),
    )
    .expect("evaluate");
    assert!(
        !got,
        "the RED-3 mutation is not present; this commit is only meaningful with it"
    );
}

/// Half two: the digest was recomputed, so the implementation binding agrees
/// with the implementation it is supposed to be freezing.
#[test]
fn red3_the_whole_binding_stays_green_anyway() {
    let entry = resolve("review-by-expected-author-login", "1").expect("resolve");
    assert!(
        verify_binding(entry).is_ok(),
        "if this now fails, the mechanism was fixed and this RED-3 witness is spent"
    );
    assert_eq!(entry.version, "1", "no version bump was taken");
}

/// The reason it stays green: nothing outside `src/matchers.rs` has an opinion
/// about what `/1`'s implementation digest is. The corpus records a matcher's
/// name, version and parameters — never the implementation it ran.
#[test]
fn red3_no_durable_artifact_records_which_implementation_ran() {
    use std::fs;
    use std::path::Path;

    let base =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/closure-provenance");
    let mut matcher_blocks = 0;
    for entry in fs::read_dir(base).expect("corpus") {
        let path = entry.expect("dir entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let doc: Value =
            serde_json::from_str(&fs::read_to_string(&path).expect("read")).expect("parse");
        let mut stack = vec![doc];
        while let Some(node) = stack.pop() {
            match node {
                Value::Object(map) => {
                    if let Some(Value::Object(m)) = map.get("matcher") {
                        matcher_blocks += 1;
                        assert!(
                            !m.contains_key("implementationDigest"),
                            "{path:?} records an implementation digest, so this RED-3 \
                             witness is spent and the gap it documents is closed"
                        );
                    }
                    stack.extend(map.into_iter().map(|(_, v)| v));
                }
                Value::Array(items) => stack.extend(items),
                _ => {}
            }
        }
    }
    assert!(
        matcher_blocks >= 3,
        "expected the corpus's query snapshots, saw {matcher_blocks} matcher blocks"
    );
}

/// And no frozen vector covers `DISMISSED`, so the conformance half is quiet too
/// — the mutation clears both digests, not just the weaker one.
#[test]
fn red3_no_frozen_vector_covers_the_changed_input() {
    for entry in REGISTRY {
        for vector in entry.vectors {
            let candidate: Value = serde_json::from_str(vector.candidate).expect("vector is JSON");
            assert_ne!(
                candidate.pointer("/state").and_then(Value::as_str),
                Some("DISMISSED"),
                "vector {:?} covers the mutated input, so this witness would not \
                 discriminate — pick an input the frozen set genuinely misses",
                vector.name
            );
        }
    }
}
