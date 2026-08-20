//! Conformance of the canonicalizer against evidence frozen before it existed.
//!
//! `tests/fixtures/closure-provenance/README.md` and its redaction counterpart
//! preregistered every digest constant in those corpora, computed with an
//! implementation outside this repository (rfc8785 0.1.4 from PyPI) precisely so
//! that the constants would not be produced by the code they validate. This file
//! is the other half of that arrangement.
//!
//! WHAT THIS PROVES, AND WHAT IT DOES NOT. Measured before writing the
//! implementation: for every canonical object across both corpora, JCS output
//! and plain sorted-compact JSON are **byte-identical**. Those corpora are pure
//! ASCII with integer numbers and no control characters, so they exercise none of
//! the behaviour that distinguishes JCS from a naive serializer.
//!
//! So the frozen constants are a NECESSARY condition, not a sufficient one. They
//! catch a canonicalizer that disagrees with the world; they cannot catch one that
//! is merely sorted-compact JSON wearing the name. `jcs_specific.rs` carries the
//! vectors that discriminate, because the corpus's own claim about what it
//! validates is exactly the kind of statement this project does not take on trust.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use o7_closure_canonical::{canonicalize, digest, Digest};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

fn fixtures(dir: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(dir)
}

/// Every `canonical` object paired with the `canonicalDigest` beside it.
fn frozen_pairs(v: &Value, out: &mut Vec<(Value, String)>) {
    match v {
        Value::Object(map) => {
            if let (Some(c), Some(Value::String(d))) =
                (map.get("canonical"), map.get("canonicalDigest"))
            {
                out.push((c.clone(), d.clone()));
            }
            for value in map.values() {
                frozen_pairs(value, out);
            }
        }
        Value::Array(items) => {
            for item in items {
                frozen_pairs(item, out);
            }
        }
        _ => {}
    }
}

fn all_frozen_pairs() -> Vec<(String, Value, String)> {
    let mut out = Vec::new();
    for dir in ["closure-provenance", "closure-redaction"] {
        for entry in fs::read_dir(fixtures(dir)).expect("reading a frozen corpus") {
            let path = entry.expect("dir entry").path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("?")
                .to_owned();
            let doc: Value = serde_json::from_str(&fs::read_to_string(&path).expect("reading"))
                .expect("parsing");
            let mut pairs = Vec::new();
            frozen_pairs(&doc, &mut pairs);
            for (c, d) in pairs {
                out.push((format!("{dir}/{name}"), c, d));
            }
        }
    }
    out
}

#[test]
fn every_frozen_digest_recomputes() {
    let pairs = all_frozen_pairs();
    assert!(
        pairs.len() >= 26,
        "expected the frozen corpora to yield at least 26 digest-bearing canonical \
         objects, found {}",
        pairs.len()
    );
    for (source, canonical, expected) in &pairs {
        let got = digest(canonical).expect("canonicalizing a frozen object");
        assert_eq!(
            got.as_str(),
            expected,
            "{source}: this canonicalizer disagrees with a constant frozen before it existed"
        );
    }
}

/// The assessment records in the redaction corpus are digested too, and they are
/// not reachable through the `canonical`/`canonicalDigest` pairing above.
#[test]
fn every_frozen_assessment_digest_recomputes() {
    let mut checked = 0;
    for entry in fs::read_dir(fixtures("closure-redaction")).expect("reading") {
        let path = entry.expect("dir entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let doc: Value =
            serde_json::from_str(&fs::read_to_string(&path).expect("reading")).expect("parsing");
        let scopes: Vec<&Value> = match doc.get("variants").and_then(Value::as_array) {
            Some(vs) => vs.iter().collect(),
            None => vec![&doc],
        };
        for scope in scopes {
            let (Some(a), Some(Value::String(d))) =
                (scope.get("assessment"), scope.get("assessmentDigest"))
            else {
                continue;
            };
            assert_eq!(
                digest(a).expect("canonicalizing an assessment").as_str(),
                d,
                "{path:?}: assessment digest disagrees with the frozen constant"
            );
            checked += 1;
        }
    }
    assert!(
        checked >= 12,
        "expected every specimen's assessment, saw {checked}"
    );
}

/// The frozen syntax, and the refusal of everything adjacent to it.
#[test]
fn digest_syntax_is_exactly_the_frozen_one() {
    let d = digest(&serde_json::json!({"a": 1})).expect("digest");
    assert!(d.as_str().starts_with("sha256:"));
    assert_eq!(d.as_str().len(), 71);
    assert_eq!(Digest::parse(d.as_str()).expect("roundtrip"), d);

    for bad in [
        "abc",
        "sha256:",
        "sha512:0000000000000000000000000000000000000000000000000000000000000000",
        "sha256:000000000000000000000000000000000000000000000000000000000000000",
        "sha256:ABCDEF0000000000000000000000000000000000000000000000000000000000",
    ] {
        assert!(Digest::parse(bad).is_err(), "{bad:?} should be refused");
    }
}

/// §7 says the corpus contains no non-integer numbers, and this crate refuses
/// them rather than approximating ECMAScript `Number::toString`. Both halves are
/// asserted: the refusal works, and the frozen corpora never trip it.
#[test]
fn non_integer_numbers_are_refused_and_the_corpus_has_none() {
    assert!(canonicalize(&serde_json::json!({"x": 1.5})).is_err());
    assert!(canonicalize(&serde_json::json!([1.0])).is_err());
    for (source, canonical, _) in all_frozen_pairs() {
        assert!(
            canonicalize(&canonical).is_ok(),
            "{source}: a frozen object contains a value V1 cannot canonicalize"
        );
    }
}
