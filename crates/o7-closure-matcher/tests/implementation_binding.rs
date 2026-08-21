//! GREEN-2. `(id, version)` is bound to an immutable implementation, and the
//! conformance vectors go back to being what they were correctly named.
//!
//! WHAT RED-2 (782cbaf) ESTABLISHED. A wrong `review-by-expected-author-login`
//! — one that dropped `COMMENTED` reviews — passed the entire suite under an
//! unchanged version and an unchanged conformance digest, because every frozen
//! vector used `APPROVED`. §13.1 requires a new version whenever behaviour
//! changes for **ANY** input, and a digest over results on a finite vector set
//! cannot discharge `ANY`. The finite witness set had been promoted to the
//! identity of a total function.
//!
//! WHAT REPLACES IT.
//!
//! ```text
//! implementation_digest = SHA-256(bytes of the file defining the predicate)
//! ```
//!
//! No enumeration is involved, so there is no input it can miss. Each version's
//! predicate lives alone in `src/matchers/<id>_v<n>.rs`; the registry embeds that
//! file verbatim at compile time and pins its digest.
//!
//! WHY THE HASHED BYTES ARE THE RUNNING CODE. The registry embeds the file and
//! `mod` compiles it, from the same path in the same build. They cannot drift.
//! That link — not a vector set — is what makes the digest an identity, and
//! `the_bound_bytes_are_the_bytes_on_disk` checks the remaining seam.
//!
//! APPEND-ONLY WITHOUT A POLICY. Editing `..._v1.rs` breaks v1's binding; a
//! behaviour change adds `..._v2.rs` and a new entry. Nothing here relies on CI
//! configuration or on a reviewer remembering the rule — the mechanism is the
//! enforcement, which is the only kind that survives.
//!
//! WHAT THIS STILL DOES NOT COVER, STATED PLAINLY. A behaviour change that leaves
//! the bytes alone: `serde_json`'s `pointer()` semantics shifting under it, a
//! compiler change, a target difference. Identical bytes are not identical
//! behaviour across a moving substrate. The conformance vectors are the witness
//! for exactly that residual, which is why they are kept and why they are no
//! longer called an identity. Neither digest subsumes the other and neither is
//! sufficient alone.

// Justification for the restriction-lint allowance, per AGENTS.md rule 4 and the
// precedent in `crates/o7-closure-classifier/tests/frozen_fixtures.rs`: every
// panic path below is this test's own assertion failing, or its own parsing of a
// conformance vector that is a `&'static str` literal in this workspace. Nothing
// here runs against production input.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use o7_closure_canonical::{digest, digest_of_canonical_bytes};
use o7_closure_matcher::{
    resolve, verify_binding, verify_conformance, verify_implementation, MatchError, REGISTRY,
};
use serde_json::{json, Value};

/// Every entry's implementation digest holds over its own embedded bytes.
#[test]
fn every_version_is_bound_to_its_implementation() {
    for entry in REGISTRY {
        verify_implementation(entry)
            .unwrap_or_else(|e| panic!("{}/{}: {e}", entry.id, entry.version));
        verify_binding(entry).unwrap_or_else(|e| panic!("{}/{}: {e}", entry.id, entry.version));
    }
}

/// The seam the compile-time embed leaves open: a stale build. If the file on
/// disk is not the file that was compiled in, this catches it — and if it ever
/// fires, the digest constants must not be touched until the build is clean,
/// because a rebuilt binary and a stale one would disagree about identity.
#[test]
fn the_bound_bytes_are_the_bytes_on_disk() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/matchers");
    for entry in REGISTRY {
        let file = entry.id.replace('-', "_") + "_v" + entry.version + ".rs";
        let on_disk = std::fs::read_to_string(dir.join(&file))
            .unwrap_or_else(|e| panic!("{}/{}: reading {file}: {e}", entry.id, entry.version));
        assert_eq!(
            on_disk, entry.implementation_source,
            "{}/{}: the embedded source is not {file} as it is on disk",
            entry.id, entry.version
        );
        // The naming convention is load-bearing, not cosmetic: it is what lets
        // this check find the file at all.
        assert!(
            digest_of_canonical_bytes(on_disk.as_bytes()).as_str() == entry.implementation_digest,
            "{}/{}: {file} does not hash to the bound digest",
            entry.id,
            entry.version
        );
    }
}

/// No edit escapes the digest, because it covers the whole file. Asserted rather
/// than argued: perturbing any single byte position moves it.
///
/// This is the property a vector set can never have, and it is why the fix was
/// to stop enumerating rather than to enumerate harder.
#[test]
fn any_edit_to_a_bound_implementation_moves_its_digest() {
    for entry in REGISTRY {
        let source = entry.implementation_source;
        let bound = entry.implementation_digest;

        // A byte appended, a byte removed, and a byte flipped at each of a
        // spread of positions across the file.
        let mut mutants = vec![
            format!("{source} "),
            source
                .get(..source.len() - 1)
                .expect("non-empty")
                .to_owned(),
        ];
        for k in 1..8 {
            let at = source.len() * k / 8;
            let at = (0..=at)
                .rev()
                .find(|i| source.is_char_boundary(*i))
                .unwrap_or(0);
            let (head, tail) = source.split_at(at);
            mutants.push(format!("{head}\u{20}{tail}"));
        }

        for mutant in mutants {
            assert_ne!(
                digest_of_canonical_bytes(mutant.as_bytes()).as_str(),
                bound,
                "{}/{}: an edited implementation hashes to the bound digest",
                entry.id,
                entry.version
            );
        }
    }
}

/// Distinct entries are distinct implementations. Two versions sharing one file
/// would make the digest bind neither of them.
#[test]
fn no_two_versions_share_an_implementation() {
    for (i, a) in REGISTRY.iter().enumerate() {
        for b in REGISTRY.iter().skip(i + 1) {
            assert_ne!(
                a.implementation_digest, b.implementation_digest,
                "{}/{} and {}/{} are bound to the same implementation",
                a.id, a.version, b.id, b.version
            );
        }
    }
}

/// RED-2's escape, preserved as a permanent measurement rather than as a claim
/// about a commit nobody will check out.
///
/// The mutant is defined here, in the test, and put through the *same*
/// conformance statement as the real predicate. Both digests come out equal
/// while the two functions disagree on an admissible input. That is the exact
/// shape of the defect, and it stays true forever — which is the point: it is
/// not a bug that got fixed, it is a limit of what a finite witness set can
/// mean. What changed is that identity no longer rests on it.
#[test]
fn the_conformance_digest_is_still_blind_to_out_of_vector_changes() {
    let entry = resolve("review-by-expected-author-login", "1").expect("resolve");

    // The real rule, plus a gate on a `state` value no frozen vector uses.
    fn mutant(candidate: &Value, parameters: &Value) -> Result<bool, MatchError> {
        if candidate.pointer("/state").and_then(Value::as_str) == Some("COMMENTED") {
            return Ok(false);
        }
        let expected = parameters
            .get("expectedAuthorLogin")
            .and_then(Value::as_str)
            .ok_or(MatchError::MalformedCandidate {
                why: "expectedAuthorLogin is not a string",
            })?;
        if candidate.pointer("/sourceKind").and_then(Value::as_str)
            != Some("github-submitted-review")
        {
            return Ok(false);
        }
        Ok(candidate.pointer("/user/login").and_then(Value::as_str) == Some(expected))
    }

    // The two disagree on an input the matcher is squarely for.
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
    assert!((entry.predicate)(&commented, &parameters).expect("real"));
    assert!(!mutant(&commented, &parameters).expect("mutant"));

    // And yet the conformance statement is byte-identical.
    let statement = |f: o7_closure_matcher::MatcherFn| {
        let results: Vec<Value> = entry
            .vectors
            .iter()
            .map(|v| {
                let p: Value = serde_json::from_str(v.parameters).expect("params");
                let c: Value = serde_json::from_str(v.candidate).expect("candidate");
                let got = f(&c, &p).expect("evaluate");
                assert_eq!(got, v.expected, "vector {:?} disagrees", v.name);
                json!({ "name": v.name, "parameters": p, "candidate": c, "result": got })
            })
            .collect();
        digest(&json!({
            "schemaVersion": 1,
            "sourceKind": "closure-matcher-conformance",
            "matcher": { "id": entry.id, "version": entry.version },
            "parameterKeys": entry.parameter_keys,
            "vectors": results,
        }))
        .expect("digest")
    };

    assert_eq!(
        statement(entry.predicate).as_str(),
        statement(mutant).as_str(),
        "the frozen vectors now separate these two; that is an improvement, and it \
         makes this measurement stale rather than wrong — restate it with an input \
         the vector set genuinely misses, because one always exists"
    );
    assert_eq!(
        statement(entry.predicate).as_str(),
        entry.conformance_digest,
        "and it is the digest actually bound to version 1"
    );
}

/// The two checks fail separately, so a moved implementation and a moved result
/// can never be mistaken for each other — or silenced by the same fix.
#[test]
fn the_two_digests_fail_as_two_different_facts() {
    let entry = resolve("review-by-expected-author-login", "1").expect("resolve");

    let wrong_implementation = o7_closure_matcher::MatcherEntry {
        implementation_digest: "sha256:\
            0000000000000000000000000000000000000000000000000000000000000000",
        ..*entry
    };
    assert!(matches!(
        verify_implementation(&wrong_implementation),
        Err(MatchError::ImplementationDigestMismatch { .. })
    ));
    assert!(
        matches!(
            verify_binding(&wrong_implementation),
            Err(MatchError::ImplementationDigestMismatch { .. })
        ),
        "verify_binding must refuse an unbound implementation BEFORE consulting \
         its answers — those answers are answers from something else"
    );
    // Conformance is unaffected: the behaviour did not move, the binding did.
    assert!(verify_conformance(&wrong_implementation).is_ok());

    let wrong_conformance = o7_closure_matcher::MatcherEntry {
        conformance_digest: "sha256:\
            0000000000000000000000000000000000000000000000000000000000000000",
        ..*entry
    };
    assert!(matches!(
        verify_conformance(&wrong_conformance),
        Err(MatchError::ConformanceDigestMismatch { .. })
    ));
    assert!(verify_implementation(&wrong_conformance).is_ok());
}
