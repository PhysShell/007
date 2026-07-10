//! Extern-legend guarantees: exact-file pinning, fail-closed decode, and
//! collision-safe substitution — the warm dictionary must never be able
//! to reconstruct wrong bytes.

use anyhow::Result;

use qodec::alias::Alphabet;
use qodec::legend::{emit, generate, substitute, ExternLegend};
use qodec::meter::{Bpe, TokenMeter};
use qodec::profile::Profile;
use qodec::{decode, decode_with_extern, encode, CodecKind};

/// ownsharp-shaped diagnostics: heavy repeated stems, varying identifiers.
fn corpus_text(tag: &str) -> String {
    let mut text = String::new();
    for i in 0..16 {
        text.push_str(&format!(
            "../STS_new/SectorTS/Broker/CommonControls/File{i}.xaml.cs:{}: warning: [OWN001] event 'x{i}.PropertyChanged' is subscribed but never unsubscribed; it may keep 'Host' alive {tag}\n",
            40 + i,
        ));
    }
    text
}

#[test]
fn legend_generates_parses_and_pins_bytes() -> Result<()> {
    let meter = Bpe::o200k()?;
    let mut profile = Profile::default();
    profile.learn_from(&corpus_text("alpha"));
    let text = generate(&profile, &meter, 32)?;
    let legend = ExternLegend::parse(&text)?;
    anyhow::ensure!(!legend.entries.is_empty(), "must freeze at least one phrase");
    let again = ExternLegend::parse(&text)?;
    anyhow::ensure!(legend.sum == again.sum, "checksum must be a pure function of bytes");
    Ok(())
}

#[test]
fn ext_roundtrip_is_exact_and_fails_closed() -> Result<()> {
    let meter = Bpe::o200k()?;
    let mut profile = Profile::default();
    profile.learn_from(&corpus_text("alpha"));
    let legend_text = generate(&profile, &meter, 32)?;
    let legend = ExternLegend::parse(&legend_text)?;

    let payload = corpus_text("omega");
    let sub = substitute(&payload, &legend, &meter);
    anyhow::ensure!(!sub.used.is_empty(), "shared stems must substitute");
    anyhow::ensure!(
        meter.count(&sub.text) < meter.count(&payload),
        "substitution itself must already pay"
    );

    let inner = encode(&sub.text, CodecKind::Mine, &meter, Alphabet::Auto);
    let artifact = emit(&inner, &legend, &sub.used);

    // The happy path: exact bytes back with the exact legend.
    let back = decode_with_extern(&artifact, Some(&legend))?;
    anyhow::ensure!(back == payload, "roundtrip with the right legend is byte-exact");

    // No legend: refuse loudly, never guess.
    let plain = decode(&artifact);
    anyhow::ensure!(
        plain.as_ref().err().is_some_and(|e| format!("{e:#}").contains("extern legend")),
        "plain decode must name the missing legend: {plain:?}"
    );

    // A drifted legend file (different bytes -> different sum): refuse.
    let drifted = ExternLegend::parse(&format!("{legend_text}# drift\n"))?;
    let wrong = decode_with_extern(&artifact, Some(&drifted));
    anyhow::ensure!(
        wrong.as_ref().err().is_some_and(|e| format!("{e:#}").contains("mismatch")),
        "checksum drift must refuse: {wrong:?}"
    );
    Ok(())
}

#[test]
fn colliding_glyph_skips_the_entry_and_stays_exact() -> Result<()> {
    let meter = Bpe::o200k()?;
    let mut profile = Profile::default();
    profile.learn_from(&corpus_text("alpha"));
    let legend = ExternLegend::parse(&generate(&profile, &meter, 32)?)?;
    let first_alias = legend
        .entries
        .first()
        .map(|(a, _)| a.clone())
        .unwrap_or_default();

    // The payload naturally contains the first entry's glyph — that entry
    // must be skipped so expansion can never touch pre-existing bytes.
    let payload = format!("{}nat {first_alias} ural\n", corpus_text("omega"));
    let sub = substitute(&payload, &legend, &meter);
    anyhow::ensure!(
        !sub.used.contains(&first_alias),
        "colliding entry must not be applied"
    );
    let inner = encode(&sub.text, CodecKind::Mine, &meter, Alphabet::Auto);
    let artifact = emit(&inner, &legend, &sub.used);
    let back = decode_with_extern(&artifact, Some(&legend))?;
    anyhow::ensure!(back == payload, "collision case stays byte-exact");
    Ok(())
}
