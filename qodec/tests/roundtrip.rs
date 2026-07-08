//! Roundtrip guarantees — the property that makes this a codec and not a
//! paraphrase: decode(encode(x)) == x (byte-exact for mine/fold/raw,
//! Value-equal for toon).

use anyhow::Result;
use proptest::prelude::*;

use qodec::alias::Alphabet;
use qodec::meter::{Approx, Bpe, TokenMeter};
use qodec::{decode, encode, CodecKind};

fn roundtrip_bytes(text: &str, kind: CodecKind, meter: &dyn TokenMeter) -> Result<()> {
    let encoded = encode(text, kind, meter, Alphabet::Auto);
    let back = decode(&encoded)?;
    anyhow::ensure!(
        back == text,
        "byte roundtrip failed for {:?}: {:?} -> {:?}",
        kind.label(),
        text,
        back
    );
    Ok(())
}

#[test]
fn mine_roundtrips_repetitive_text() -> Result<()> {
    let meter = Bpe::o200k()?;
    // Small-but-not-tiny: dictionary gains must also amortize the container
    // header (~13 tokens), which a 3-line sample legitimately fails to do.
    let mut text = String::new();
    for name in ["A", "B", "C", "D", "E", "F", "G", "H", "I", "J"] {
        text.push_str(&format!(
            "src/Legacy.UI/ViewModels/{name}ViewModel.cs uses ConfigureAwait(false) and CancellationToken cancellationToken\n"
        ));
    }
    let text = text.as_str();
    let encoded = encode(text, CodecKind::Mine, &meter, Alphabet::Auto);
    anyhow::ensure!(encoded.starts_with("%q1 mine"), "expected mine container");
    anyhow::ensure!(
        meter.count(&encoded) < meter.count(text),
        "mine must reduce tokens on repetitive text"
    );
    roundtrip_bytes(text, CodecKind::Mine, &meter)
}

#[test]
fn mine_falls_back_on_unique_prose() -> Result<()> {
    let meter = Bpe::o200k()?;
    let text = "one two three four five six seven eight nine ten.\n";
    let encoded = encode(text, CodecKind::Mine, &meter, Alphabet::Auto);
    anyhow::ensure!(encoded.starts_with("%q1 raw"), "expected raw fallback");
    roundtrip_bytes(text, CodecKind::Mine, &meter)
}

#[test]
fn mine_survives_hostile_input() -> Result<()> {
    let meter = Bpe::o200k()?;
    // Sigil chars in input, %q1-looking lines, CRLF, no trailing newline.
    let text = "§0 already here ¤ µ 码引路\r\n%q1 body\r\n%q1 x3\r\n\
                repeated hostile line with enough words to mine maybe\r\n\
                repeated hostile line with enough words to mine maybe";
    roundtrip_bytes(text, CodecKind::Mine, &meter)?;
    roundtrip_bytes(text, CodecKind::Fold, &meter)?;
    roundtrip_bytes(text, CodecKind::Squeeze, &meter)
}

#[test]
fn fold_roundtrips_runs_and_escapes() -> Result<()> {
    let meter = Bpe::o200k()?;
    let line = "  CSC : warning CS8618: Non-nullable property must contain a non-null value.";
    let mut text = String::new();
    for _ in 0..7 {
        text.push_str(line);
        text.push('\n');
    }
    text.push_str("%q1 x9\n"); // hostile: looks like our own marker
    text.push_str("tail without repeat\n");
    let encoded = encode(&text, CodecKind::Fold, &meter, Alphabet::Auto);
    anyhow::ensure!(encoded.starts_with("%q1 fold"), "expected fold container");
    roundtrip_bytes(&text, CodecKind::Fold, &meter)
}

#[test]
fn fold_handles_missing_trailing_newline_and_empty_runs() -> Result<()> {
    let meter = Bpe::o200k()?;
    let text = "a\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\na"; // long blank run, no trailing \n
    roundtrip_bytes(text, CodecKind::Fold, &meter)
}

#[test]
fn toon_roundtrips_uniform_array_semantically() -> Result<()> {
    let meter = Bpe::o200k()?;
    let text = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/findings.json"),
    )?;
    let encoded = encode(&text, CodecKind::Toon, &meter, Alphabet::Auto);
    anyhow::ensure!(encoded.starts_with("%q1 toon"), "expected toon container");
    let back = decode(&encoded)?;
    let a: serde_json::Value = serde_json::from_str(&back)?;
    let b: serde_json::Value = serde_json::from_str(&text)?;
    anyhow::ensure!(a == b, "toon must be Value-equal");
    anyhow::ensure!(
        meter.count(&encoded) < meter.count(&text),
        "toon must reduce tokens on a uniform array"
    );
    Ok(())
}

#[test]
fn toon_falls_back_on_non_uniform_json() -> Result<()> {
    let meter = Bpe::o200k()?;
    let text = r#"[{"a":1},{"a":1,"b":2}]"#;
    let encoded = encode(text, CodecKind::Toon, &meter, Alphabet::Auto);
    anyhow::ensure!(encoded.starts_with("%q1 raw"), "expected raw fallback");
    roundtrip_bytes(text, CodecKind::Toon, &meter)
}

#[test]
fn squeeze_unwraps_pipeline() -> Result<()> {
    let meter = Bpe::o200k()?;
    let text = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/stacktrace.txt"),
    )?;
    roundtrip_bytes(&text, CodecKind::Squeeze, &meter)
}

#[test]
fn empty_and_tiny_inputs() -> Result<()> {
    let meter = Bpe::o200k()?;
    for text in ["", "x", "\n", "\r\n"] {
        for kind in [
            CodecKind::Mine,
            CodecKind::Fold,
            CodecKind::Toon,
            CodecKind::Squeeze,
        ] {
            roundtrip_bytes(text, kind, &meter)?;
        }
    }
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn prop_mine_fold_squeeze_roundtrip(text in "[ -~\n§¤码引]{0,400}") {
        let meter = Approx;
        for kind in [CodecKind::Mine, CodecKind::Fold, CodecKind::Squeeze] {
            let encoded = encode(&text, kind, &meter, Alphabet::Auto);
            let back = decode(&encoded).map_err(|e| {
                TestCaseError::fail(format!("decode error for {}: {e}", kind.label()))
            })?;
            prop_assert_eq!(&back, &text, "codec {}", kind.label());
        }
    }

    #[test]
    fn prop_toon_roundtrip_uniform(rows in proptest::collection::vec((0i64..1000, "[a-z ]{0,12}", proptest::bool::ANY), 2..20)) {
        let array: Vec<serde_json::Value> = rows
            .iter()
            .map(|(n, s, b)| serde_json::json!({"n": n, "s": s, "b": b}))
            .collect();
        let text = serde_json::to_string(&array).map_err(|e| TestCaseError::fail(e.to_string()))?;
        let encoded = encode(&text, CodecKind::Toon, &Approx, Alphabet::Auto);
        let back = decode(&encoded).map_err(|e| TestCaseError::fail(e.to_string()))?;
        let a: serde_json::Value = serde_json::from_str(&back).map_err(|e| TestCaseError::fail(e.to_string()))?;
        let b: serde_json::Value = serde_json::from_str(&text).map_err(|e| TestCaseError::fail(e.to_string()))?;
        prop_assert_eq!(a, b);
    }
}
