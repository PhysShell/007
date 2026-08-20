//! RED-2. The witness that `conformance_digest` is not an implementation binding.
//!
//! WHAT IS WRONG WITH THIS COMMIT. `src/matchers.rs` currently contains a
//! deliberately wrong `review-by-expected-author-login`: a submitted review by
//! the expected author whose `state` is `COMMENTED` no longer matches. Its
//! `version` is still `"1"` and its `conformance_digest` is unchanged from
//! 0f98ac0. Do not build on this commit; the next one reverts the mutation.
//!
//! WHY IT IS HERE. §13.1 froze:
//!
//! > `matcher.version` changes whenever `f`'s behaviour changes for ANY input
//!
//! and 0f98ac0 claimed the conformance digest enforced it. It does not. The
//! digest covers `(id, version, parameterKeys, vectors, results on those
//! vectors)` — a finite observation of `f`, not `f`. Every frozen review vector
//! uses `APPROVED`, so a change gated on `COMMENTED` moves no result, moves no
//! digest, and passes.
//!
//! ```text
//! behaviour changed for an admissible input
//!         ↓
//! no frozen vector covers that input
//!         ↓
//! all vector results unchanged
//!         ↓
//! conformance_digest unchanged
//!         ↓
//! verify_binding() PASS, version still "1"      <- exactly what §13.1 forbids
//! ```
//!
//! The tests below PASS in this commit, and their passing is the defect. A
//! finite witness set was quietly promoted to the identity of a total function
//! — the same substitution this whole effort is named after, wearing a new hat:
//! an artifact certifying the very thing it is being checked against.
//!
//! NO FINITE VECTOR SET FIXES THIS. Adding a `COMMENTED` vector defeats this
//! particular mutation and nothing else; the next one gates on `DISMISSED`.
//! `ANY input` is not provable by enumeration, so the next commit stops trying
//! and binds the implementation's *bytes* instead, leaving the vectors as what
//! they were correctly named all along: a finite witness set for behavioural
//! regression.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use o7_closure_matcher::{resolve, verify_binding};
use serde_json::json;

/// The mutation is real: the predicate's answer for an admissible input is not
/// what it was in 0f98ac0.
#[test]
fn red2_the_behaviour_of_version_1_has_changed_for_an_admissible_input() {
    let entry = resolve("review-by-expected-author-login", "1").expect("resolve");
    let parameters = json!({ "expectedAuthorLogin": "expected-reviewer" });
    let commented = json!({
        "schemaVersion": 1,
        "sourceKind": "github-submitted-review",
        "stableId": "77",
        "user": { "id": "77", "login": "expected-reviewer", "type": "User" },
        "authorAssociation": "NONE",
        "state": "COMMENTED",
        "body": "",
        "submittedAt": "2026-01-01T00:00:00Z",
        "commitId": "1111111111111111111111111111111111111111"
    });

    let got = (entry.predicate)(&commented, &parameters).expect("evaluate");
    assert!(
        !got,
        "the RED-2 mutation is not present; this commit is only meaningful with it"
    );
    // In 0f98ac0 this input returned true. A submitted review by the expected
    // author is exactly what this matcher is for, whatever verdict it carries —
    // and §13's whole point is that the classifier, not the matcher, judges it.
}

/// And the binding does not notice. Same id, same version, same conformance
/// digest, changed behaviour — green.
#[test]
fn red2_the_conformance_binding_stays_green_anyway() {
    let entry = resolve("review-by-expected-author-login", "1").expect("resolve");
    assert!(
        verify_binding(entry).is_ok(),
        "if this now fails, the mechanism was fixed and this RED-2 witness is spent"
    );
    assert_eq!(entry.version, "1", "no version bump was taken");
    assert_eq!(
        entry.conformance_digest,
        "sha256:7ea10c56ced0cc83ac3889750fd2a133584275d39f6f5fe809f744ebf74c5178",
        "and the digest is byte-identical to the one 0f98ac0 froze"
    );
}

/// Not one frozen vector is disturbed — which is the mechanism of the escape,
/// not an incidental fact about it.
#[test]
fn red2_no_frozen_vector_covers_the_changed_input() {
    let entry = resolve("review-by-expected-author-login", "1").expect("resolve");
    for vector in entry.vectors {
        let candidate: serde_json::Value =
            serde_json::from_str(vector.candidate).expect("vector is JSON");
        assert_ne!(
            candidate
                .pointer("/state")
                .and_then(serde_json::Value::as_str),
            Some("COMMENTED"),
            "vector {:?} covers the mutated input, so this witness would not \
             discriminate — pick an input the frozen set genuinely misses",
            vector.name
        );
    }
}
