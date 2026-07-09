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
fn sigil_mode_commits_multiple_entries() -> Result<()> {
    // CodeRabbit review on PR #26: char-level reservation blocked the shared
    // sigil after the first commit, capping sigil mode at one dictionary
    // entry. Fixed-width indices (§00..§99) make the pool fully usable.
    let meter = Bpe::o200k()?;
    let mut text = String::new();
    for i in 0..12 {
        text.push_str(&format!(
            "component_{} at System.Threading.Dispatcher.Invoke handles slot_{} without ConfigureAwait(false)\n",
            i % 3,
            i % 4
        ));
    }
    let encoded = encode(&text, CodecKind::Mine, &meter, Alphabet::Sigil);
    let c = qodec::container::parse(&encoded)?;
    anyhow::ensure!(
        c.codec == "mine",
        "expected mine container, got {}",
        c.codec
    );
    anyhow::ensure!(
        c.legend.len() >= 2,
        "sigil pool must supply multiple aliases, got {} entries",
        c.legend.len()
    );
    roundtrip_bytes(&text, CodecKind::Mine, &meter)?;
    // Force the sigil+digit ambiguity: alias directly followed by a literal
    // digit (segment prefix ends at '/', digit starts the next segment).
    let mut hostile = String::new();
    for i in 0..10 {
        hostile.push_str(&format!(
            "assets/generated/2026/{i}/report.bin assets/generated/2026/{i}/index.bin\n"
        ));
    }
    roundtrip_bytes(&hostile, CodecKind::Mine, &meter)
}

#[test]
fn deep_beats_words_on_boundary_straddling_repeats() -> Result<()> {
    // The suffix-automaton miner's reason to exist: repeats that no word or
    // separator boundary exposes — here a shared stem *inside* snake_case
    // identifiers that differ only in their numeric tail.
    let meter = Bpe::o200k()?;
    let mut text = String::new();
    for i in 0..20 {
        text.push_str(&format!(
            "measurement_batch_processor_run{i} emitted checkpoint_fingerprint_hash{i}\n"
        ));
    }
    let words = encode(&text, CodecKind::Mine, &meter, Alphabet::Auto);
    let deep = encode(&text, CodecKind::Deep, &meter, Alphabet::Auto);
    anyhow::ensure!(deep.starts_with("%q1 mine"), "expected mine container");
    anyhow::ensure!(
        meter.count(&deep) < meter.count(&words),
        "deep ({}) must beat words ({}) on straddling repeats",
        meter.count(&deep),
        meter.count(&words)
    );
    roundtrip_bytes(&text, CodecKind::Deep, &meter)?;
    // Hostile shapes: CRLF (candidates must not swallow the `\r`), sigils
    // in input, alias-adjacent digits.
    let hostile = "alpha_beta_gamma_delta7 §00 tail\r\nalpha_beta_gamma_delta8 tail\r\n".repeat(4);
    roundtrip_bytes(&hostile, CodecKind::Deep, &meter)
}

#[test]
fn squeeze_never_loses_to_the_raw_floor() -> Result<()> {
    // CodeRabbit review on PR #26: squeeze compared stage2 only against
    // stage1, so mining a raw container's overhead could beat stage1 while
    // still losing to the original text. Squeeze must fall back to raw.
    let meter = Bpe::o200k()?;
    for text in ["tiny\n", "a b\n", "{\"k\": 1}\n"] {
        let encoded = encode(text, CodecKind::Squeeze, &meter, Alphabet::Auto);
        let raw_floor = meter.count(&qodec::container::raw(text));
        anyhow::ensure!(
            meter.count(&encoded) <= raw_floor,
            "squeeze artifact for {text:?} exceeds the raw floor"
        );
        roundtrip_bytes(text, CodecKind::Squeeze, &meter)?;
    }
    Ok(())
}

#[test]
fn parse_tolerates_crlf_converted_artifact() -> Result<()> {
    // CodeRabbit review on PR #26: a container converted to CRLF in transit
    // (git autocrlf, clipboards) failed to parse entirely. Header, legend
    // and boundary now strip an optional trailing `\r`; the mine body is
    // verbatim, so decode yields the CRLF-converted original.
    let meter = Bpe::o200k()?;
    let mut text = String::new();
    for name in [
        "Alpha", "Beta", "Gamma", "Delta", "Epsilon", "Zeta", "Eta", "Theta",
    ] {
        text.push_str(&format!(
            "src/Legacy.UI/ViewModels/{name}ViewModel.cs uses ConfigureAwait(false) and CancellationToken cancellationToken\n"
        ));
    }
    let encoded = encode(&text, CodecKind::Mine, &meter, Alphabet::Auto);
    anyhow::ensure!(encoded.starts_with("%q1 mine"), "expected mine container");
    let converted = encoded.replace('\n', "\r\n");
    let back = decode(&converted)?;
    anyhow::ensure!(
        back == text.replace('\n', "\r\n"),
        "CRLF-converted artifact must decode to the CRLF-converted original"
    );
    Ok(())
}

#[test]
fn toon_preserves_empty_object_rows() -> Result<()> {
    // Codex review finding on PR #26: `[{}, {}, ...]` encodes as empty row
    // lines after the `[]` keys line; decode must not confuse them with the
    // trailing-newline artifact and collapse the array to `[]`.
    let meter = Bpe::o200k()?;
    let text = serde_json::to_string(&vec![serde_json::json!({}); 120])?;
    let encoded = encode(&text, CodecKind::Toon, &meter, Alphabet::Auto);
    let back = decode(&encoded)?;
    let a: serde_json::Value = serde_json::from_str(&back)?;
    let b: serde_json::Value = serde_json::from_str(&text)?;
    anyhow::ensure!(a == b, "empty-object rows must survive: {back:?}");

    // Pin the decode path directly, independent of encode acceptance.
    let container = "%q1 toon sep=pipe rows=3\n%q1 body\n[]\n\n\n\n";
    let direct = decode(container)?;
    anyhow::ensure!(
        direct == "[{},{},{}]",
        "hand-built empty-rows container decoded to {direct:?}"
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
    fn prop_mine_fold_squeeze_roundtrip(text in "[ -~\n§¤码引']{0,400}") {
        let meter = Approx;
        for kind in [CodecKind::Mine, CodecKind::Deep, CodecKind::Fold, CodecKind::Grep, CodecKind::Diag, CodecKind::Squeeze] {
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
