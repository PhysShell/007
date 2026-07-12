//! The adapter envelope's load-bearing invariant: whatever `content` the
//! adapter hands back — a mined artifact or a verbatim passthrough — decoding
//! it recovers the original bytes. The interop bench routes every payload
//! through this contract, so a break here silently corrupts a whole run.

use anyhow::Result;
use std::path::Path;

use qodec::adapter::adapt;
use qodec::alias::Alphabet;
use qodec::meter::{Bpe, TokenMeter};
use qodec::{decode, encode, CodecKind};

fn corpus(name: &str) -> Result<String> {
    Ok(std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus").join(name),
    )?)
}

#[test]
fn content_decodes_to_original_across_the_corpus() -> Result<()> {
    let meter = Bpe::o200k()?;
    // Byte-exact corpora only. findings.json roundtrips through `toon`, whose
    // guarantee is Value-equal (semantic), covered separately below.
    for name in [
        "build-log.txt",
        "stacktrace.txt",
        "rg-output.txt",
        "git-diff.txt",
        "prose.md", // the honest-fallback control
    ] {
        let text = corpus(name)?;
        let artifact = encode(&text, CodecKind::Squeeze, &meter, Alphabet::Auto);
        for passthrough in [false, true] {
            let out = adapt(&text, &artifact, &meter, passthrough);
            let back = decode(&out.content)?;
            anyhow::ensure!(
                back == text,
                "{name}: adapter content (passthrough={passthrough}, codec={}) \
                 did not decode to the original",
                out.codec,
            );
            // The adapter must never claim a win it did not measure.
            anyhow::ensure!(
                out.is_win() == (out.tokens_out < out.tokens_in),
                "{name}: is_win disagrees with the token counts",
            );
        }
    }
    Ok(())
}

#[test]
fn json_content_decodes_value_equal() -> Result<()> {
    // findings.json takes squeeze's toon path — decode is Value-equal, which
    // the interop harness treats as a lossless win for structured payloads.
    let meter = Bpe::o200k()?;
    let text = corpus("findings.json")?;
    let artifact = encode(&text, CodecKind::Squeeze, &meter, Alphabet::Auto);
    let out = adapt(&text, &artifact, &meter, true);
    let back = decode(&out.content)?;
    let a: serde_json::Value = serde_json::from_str(&back)?;
    let b: serde_json::Value = serde_json::from_str(&text)?;
    anyhow::ensure!(a == b, "adapter content must decode Value-equal to the input");
    Ok(())
}

#[test]
fn passthrough_beats_the_raw_floor_on_unique_prose() -> Result<()> {
    // The reason passthrough exists: on residue with no repetition the raw
    // container is a strict token loss, and passthrough must erase it.
    let meter = Bpe::o200k()?;
    let text = corpus("prose.md")?;
    let artifact = encode(&text, CodecKind::Squeeze, &meter, Alphabet::Auto);
    let blind = adapt(&text, &artifact, &meter, false);
    let passed = adapt(&text, &artifact, &meter, true);
    anyhow::ensure!(
        blind.tokens_out >= blind.tokens_in,
        "control: prose should not mine (blind cost {} vs in {})",
        blind.tokens_out,
        blind.tokens_in,
    );
    anyhow::ensure!(
        passed.tokens_out == passed.tokens_in && !passed.encoded,
        "passthrough must return the input untouched on the prose control",
    );
    anyhow::ensure!(
        meter.count(&passed.content) < meter.count(&blind.content),
        "passthrough ({}) must cost fewer tokens than the raw container ({})",
        meter.count(&passed.content),
        meter.count(&blind.content),
    );
    Ok(())
}
