//! Eval-only ablation codecs (identity / structural / fold-grep-guarded) and
//! the miner's lexical guard. The factor arms must be lossless, and — the load-
//! bearing property — `squeeze` itself must be byte-for-byte unchanged and the
//! guarded arm (VG) must never alias a guarded lexical span.

use anyhow::Result;

use qodec::alias::Alphabet;
use qodec::meter::Bpe;
use qodec::mine::is_guarded_lexical;
use qodec::{decode, encode, CodecKind};

const SAMPLE: &str = "\
»src/_derive/mod.rs
9://! Derive tutorial for clap_derive::ValueParser
26://! see clap_builder/src/builder/value_parser.rs
»src/_derive/implicit.rs
4://! pub fn value_parser() -> ValueParser
7://! ImplicitValueParser wraps ValueParser
»src/_derive/mod.rs
9://! Derive tutorial for clap_derive::ValueParser
";

fn meter() -> Bpe {
    Bpe::o200k().expect("o200k")
}

fn roundtrips(kind: CodecKind) -> Result<()> {
    let m = meter();
    let art = encode(SAMPLE, kind, &m, Alphabet::Auto);
    let back = decode(&art)?;
    anyhow::ensure!(back == SAMPLE, "{:?} roundtrip: {:?}", kind.label(), back);
    Ok(())
}

#[test]
fn identity_is_byte_exact_and_frames() -> Result<()> {
    let art = encode(SAMPLE, CodecKind::Identity, &meter(), Alphabet::Auto);
    assert!(art.starts_with("%q1 identity"), "identity header: {art:?}");
    assert_eq!(decode(&art)?, SAMPLE);
    Ok(())
}

#[test]
fn structural_and_guarded_roundtrip() -> Result<()> {
    roundtrips(CodecKind::Structural)?;
    roundtrips(CodecKind::FoldGrepGuarded)?;
    Ok(())
}

#[test]
fn structural_carries_no_alias_legend() {
    // fold/grep only — verbatim, so no `glyph=phrase` legend lines.
    let art = encode(SAMPLE, CodecKind::Structural, &meter(), Alphabet::Auto);
    for line in art.lines() {
        assert!(
            !(line.contains('=') && !line.starts_with("%q1") && line.chars().next().is_some_and(|c| !c.is_ascii())),
            "structural emitted an alias-like line: {line:?}"
        );
    }
}

#[test]
fn vg_never_aliases_a_guarded_span() {
    let art = encode(SAMPLE, CodecKind::FoldGrepGuarded, &meter(), Alphabet::Auto);
    // Every legend phrase in GF must be a non-guarded span.
    for line in art.lines() {
        if line.starts_with("%q1") {
            continue;
        }
        if let Some((_alias, phrase)) = line.split_once('=') {
            assert!(
                !is_guarded_lexical(phrase),
                "VG aliased a guarded span: {phrase:?}"
            );
        }
    }
    // Guarded tokens stay verbatim.
    assert!(art.contains("value_parser.rs"), "path not verbatim in GF");
    assert!(art.contains("ValueParser"), "identifier not verbatim in GF");
}

#[test]
fn squeeze_is_unchanged_by_the_guard() -> Result<()> {
    // Production squeeze must not be affected by the guard machinery, and must
    // still roundtrip. fold-grep-guarded (VG) is a separate shelf, not squeeze+guard.
    let m = meter();
    let sq = encode(SAMPLE, CodecKind::Squeeze, &m, Alphabet::Auto);
    assert_eq!(decode(&sq)?, SAMPLE);
    // Determinism: same input → same artifact.
    assert_eq!(sq, encode(SAMPLE, CodecKind::Squeeze, &m, Alphabet::Auto));
    Ok(())
}

#[test]
fn guard_predicate_classes() {
    for g in [
        "value_parser",              // snake_case
        "ValueParser",               // PascalCase hump
        "getValue",                  // camelCase hump
        "clap::ValueParser",         // :: path
        "src/_derive/mod.rs",        // path + extension
        "»src/",                     // grep marker
        "`code`",                    // backtick span
        "mod.rs",                    // filename.extension
    ] {
        assert!(is_guarded_lexical(g), "should guard: {g:?}");
    }
    for ok in ["9 warnings", "error CS1061", "Log Summary", "the value"] {
        assert!(!is_guarded_lexical(ok), "should not guard: {ok:?}");
    }
}
