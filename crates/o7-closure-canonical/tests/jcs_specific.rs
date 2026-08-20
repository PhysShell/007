//! The behaviour the frozen corpora cannot test.
//!
//! `frozen_constants.rs` proves this canonicalizer agrees with 26 digests
//! computed outside this repository before it existed. That is a necessary
//! condition and not a sufficient one: measured directly, JCS output and plain
//! sorted-compact JSON are byte-identical for every object in those corpora,
//! because they are pure ASCII with safe integers and no exotic keys.
//!
//! So a canonicalizer that is merely `serde_json::to_string` would pass every
//! test in that file. This one carries the vectors that separate them.
//!
//! WHERE THE EXPECTATIONS COME FROM. Every expected byte string and digest below
//! was produced by rfc8785 0.1.4 from PyPI, the same independent implementation
//! that produced the frozen corpora's constants — not by running this crate and
//! recording what it did. An expectation generated from the code under test
//! proves the code is self-consistent and nothing else.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use o7_closure_canonical::{canonicalize, digest, CanonicalError};
use serde_json::{json, Value};

fn canonical_str(v: &Value) -> String {
    String::from_utf8(canonicalize(v).expect("canonicalizing")).expect("utf-8")
}

/// RFC 8785 §3.2.3 sorts member names by UTF-16 code units. Rust's `String: Ord`
/// sorts by UTF-8 bytes, which equals code-point order. The two disagree exactly
/// when one key is astral and the other is in U+E000..U+FFFF, because an astral
/// character's first UTF-16 unit is a surrogate in 0xD800..0xDBFF and therefore
/// sorts BELOW a BMP character that sorts above it by code point.
///
/// This is the one case in the whole crate where a plausible wrong implementation
/// and a right one produce different bytes for realistic input, so it is the
/// vector that earns its keep.
#[test]
fn member_names_sort_by_utf16_code_units_not_code_points() {
    let bmp_max = char::from_u32(0xFFFF).expect("U+FFFF");
    let astral = char::from_u32(0x10000).expect("U+10000");
    let v = json!({ bmp_max.to_string(): "bmp", astral.to_string(): "astral" });

    let ours = canonical_str(&v);
    let astral_at = ours.find(astral).expect("astral key present");
    let bmp_at = ours.find(bmp_max).expect("bmp key present");
    assert!(
        astral_at < bmp_at,
        "UTF-16 order puts the astral key first; got {ours:?}"
    );

    // The naive implementation this vector exists to exclude.
    let naive = serde_json::to_string(&v).expect("serde_json");
    assert_ne!(
        ours, naive,
        "if these agree the vector has stopped discriminating and must be replaced"
    );

    assert_eq!(
        digest(&v).expect("digest").as_str(),
        "sha256:2f6d82bf059f1d83eb08822530a8d9ac983348f59938895779e603878895179a"
    );
}

/// RFC 8785 §3.2.2.2. Two-character escapes where they exist, `\u00xx` with
/// lowercase hex for the other control characters, and everything else literal —
/// notably `/` unescaped and non-ASCII not turned into `\u` sequences.
#[test]
fn strings_escape_exactly_what_jcs_escapes() {
    let e_acute = char::from_u32(0xE9).expect("U+00E9");
    let cjk = char::from_u32(0x4E2D).expect("U+4E2D");
    let s = format!("\u{8}\t\n\u{c}\r\"\\/{e_acute}{cjk}");
    let v = json!({ "s": s });

    assert_eq!(
        canonical_str(&v),
        format!(r#"{{"s":"\b\t\n\f\r\"\\/{e_acute}{cjk}"}}"#),
        "escapes must be the JCS set, with / and non-ASCII left alone"
    );
    assert_eq!(
        digest(&v).expect("digest").as_str(),
        "sha256:097a71f84ee9180e8bc1a268836041adb60fa1f2ce687666be0d6b1b2f163b5e"
    );
}

/// Control characters without a two-character escape, including NUL and the
/// vertical tab that JSON deliberately gives no shorthand.
#[test]
fn bare_control_characters_use_lowercase_four_digit_escapes() {
    let ctl = format!("{}{}{}", '\u{0}', '\u{b}', '\u{1f}');
    let v = json!({ "ctl": ctl });
    let expected = "{\"ctl\":\"".to_owned() + "\\u0000\\u000b\\u001f" + "\"}";
    assert_eq!(canonical_str(&v), expected);
    assert_eq!(
        digest(&v).expect("digest").as_str(),
        "sha256:9d66cb5c9104cc47d7d05c2fc80b322630ab9a48b9f9a065da460c85df483444"
    );
}

/// Scalars, empty containers and nesting, with no whitespace anywhere.
#[test]
fn scalars_and_containers_render_without_whitespace() {
    let v = json!({
        "b": true,
        "n": null,
        "z": 0,
        "neg": -1,
        "safe_max": 9_007_199_254_740_991_i64,
        "arr": [1, [2, {"k": "v"}]],
        "empty_obj": {},
        "empty_arr": [],
    });
    assert_eq!(
        canonical_str(&v),
        r#"{"arr":[1,[2,{"k":"v"}]],"b":true,"empty_arr":[],"empty_obj":{},"n":null,"neg":-1,"safe_max":9007199254740991,"z":0}"#
    );
    assert_eq!(
        digest(&v).expect("digest").as_str(),
        "sha256:27419ba6b538bf3209a57a74097d6743a3e9979e887b8c69abef589e54d78d77"
    );
}

/// The defect differential testing found before this crate shipped. rfc8785
/// refuses 9007199254740993 as outside the safe integer domain; an earlier
/// revision of this crate emitted the decimal and would have produced a digest
/// no conforming verifier reproduces.
#[test]
fn integers_outside_the_safe_domain_are_refused() {
    let one_past = json!({ "n": 9_007_199_254_740_992_i64 });
    let two_past = json!({ "n": 9_007_199_254_740_993_i64 });
    let negative = json!({ "n": -9_007_199_254_740_992_i64 });

    for v in [&one_past, &two_past, &negative] {
        assert!(
            matches!(
                canonicalize(v),
                Err(CanonicalError::IntegerOutsideSafeDomain { .. })
            ),
            "an integer outside +/-(2^53 - 1) must be refused, not rendered: {v}"
        );
    }
    // The boundary itself is inside the domain.
    assert!(canonicalize(&json!({ "n": 9_007_199_254_740_991_i64 })).is_ok());
    assert!(canonicalize(&json!({ "n": -9_007_199_254_740_991_i64 })).is_ok());
}

/// A measured property of the corpora, asserted so it cannot quietly stop being
/// true. If a future fixture gains an astral key, an unsafe integer or an exotic
/// escape, this fails — and the claim in `frozen_constants.rs` about what those
/// constants can and cannot prove has to be rewritten rather than left standing.
#[test]
fn the_frozen_corpora_still_cannot_discriminate_jcs_from_sorted_compact_json() {
    use std::fs;
    use std::path::Path;

    let mut compared = 0;
    for dir in ["closure-provenance", "closure-redaction"] {
        let base = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures")
            .join(dir);
        for entry in fs::read_dir(base).expect("reading a corpus") {
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
                        if let Some(c) = map.get("canonical") {
                            assert_eq!(
                                canonical_str(c),
                                serde_json::to_string(c).expect("serde_json"),
                                "{path:?}: a frozen object now DISTINGUISHES JCS from \
                                 sorted-compact JSON. That is good news for the corpus \
                                 and makes this test's premise stale — update the claim \
                                 in frozen_constants.rs rather than deleting this."
                            );
                            compared += 1;
                        }
                        stack.extend(map.into_iter().map(|(_, v)| v));
                    }
                    Value::Array(items) => stack.extend(items),
                    _ => {}
                }
            }
        }
    }
    assert!(
        compared >= 26,
        "expected the corpora's canonical objects, saw {compared}"
    );
}
