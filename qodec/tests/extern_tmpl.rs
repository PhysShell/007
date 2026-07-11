//! Extern template legend — the tmpl counterpart of the phrase legend.
//! Guarantees under test: exact-file pinning with fail-closed decode, a
//! strict whole-artifact win before any key is demanded, per-template
//! payback, and the collision rules that keep expansion away from bytes
//! the encoder did not emit.

use anyhow::Result;

use qodec::alias::Alphabet;
use qodec::legend::{generate_templates, TemplateLegend};
use qodec::meter::{Bpe, TokenMeter};
use qodec::profile::Profile;
use qodec::tmpl::encode_extern;
use qodec::{decode, decode_with_keys, encode, CodecKind, Keys};

/// Same two same-shape families as the seeding tests: separate files teach
/// the profile two clean one-slot templates.
fn family(kind: &str, noun: &str, n: usize) -> String {
    let mut text = String::new();
    for i in 0..n {
        text.push_str(&format!("worker thread pool {kind} {noun}{i} spawned\n"));
    }
    text
}

fn learned_legend(meter: &dyn TokenMeter) -> Result<TemplateLegend> {
    let mut profile = Profile::default();
    profile.learn_from(&family("delta", "task", 16));
    profile.learn_from(&family("epsilon", "job", 16));
    let text = generate_templates(&profile, meter, 32)?;
    TemplateLegend::parse(&text)
}

#[test]
fn template_legend_generates_parses_and_pins_bytes() -> Result<()> {
    let meter = Bpe::o200k()?;
    let mut profile = Profile::default();
    profile.learn_from(&family("delta", "task", 16));
    let text = generate_templates(&profile, &meter, 32)?;
    anyhow::ensure!(
        text.lines().next().is_some_and(|l| l.contains("slot=")),
        "header must declare the slot by name: {text:?}"
    );
    let legend = TemplateLegend::parse(&text)?;
    anyhow::ensure!(!legend.entries.is_empty(), "must freeze at least one template");
    anyhow::ensure!(
        legend
            .entries
            .iter()
            .any(|(_, parts)| parts.first().is_some_and(|p| p == "worker thread pool delta ")),
        "the learned template must survive the file roundtrip: {:?}",
        legend.entries,
    );
    let again = TemplateLegend::parse(&text)?;
    anyhow::ensure!(legend.sum == again.sum, "checksum is a pure function of bytes");
    Ok(())
}

#[test]
fn extern_tmpl_roundtrips_beats_plain_and_fails_closed() -> Result<()> {
    let meter = Bpe::o200k()?;
    let legend = learned_legend(&meter)?;

    // Disjoint payload from the same families, interleaved.
    let payload: String = family("delta", "task", 12)
        .lines()
        .zip(family("epsilon", "job", 12).lines())
        .flat_map(|(a, b)| [a, "\n", b, "\n"])
        .collect();

    let artifact = encode_extern(&payload, &meter, &legend);
    anyhow::ensure!(
        artifact.starts_with("%q1 tmpl") && artifact.contains("ext="),
        "extern encode must commit with the ext param: {artifact:?}"
    );
    let plain = encode(&payload, CodecKind::Tmpl, &meter, Alphabet::Auto);
    anyhow::ensure!(
        meter.count(&artifact) < meter.count(&plain),
        "no in-artifact legend lines must measure strictly smaller: ext {} vs plain {}",
        meter.count(&artifact),
        meter.count(&plain),
    );

    // Happy path: exact bytes back with the exact key.
    let keys = Keys {
        templates: Some(&legend),
        ..Keys::default()
    };
    let back = decode_with_keys(&artifact, &keys)?;
    anyhow::ensure!(back == payload, "roundtrip with the right key is byte-exact");

    // No key: refuse loudly, never guess.
    let refused = decode(&artifact);
    anyhow::ensure!(
        refused
            .as_ref()
            .err()
            .is_some_and(|e| format!("{e:#}").contains("extern template legend")),
        "keyless decode must name the missing key: {refused:?}"
    );

    // A drifted file (different bytes -> different sum): refuse.
    let drifted_text = format!("{}# drift\n", regenerate(&meter)?);
    let drifted = TemplateLegend::parse(&drifted_text)?;
    let wrong = decode_with_keys(
        &artifact,
        &Keys {
            templates: Some(&drifted),
            ..Keys::default()
        },
    );
    anyhow::ensure!(
        wrong.as_ref().err().is_some_and(|e| format!("{e:#}").contains("mismatch")),
        "checksum drift must refuse: {wrong:?}"
    );
    Ok(())
}

/// Regenerate the same legend text (for the drift case, so entries match
/// but bytes differ by an appended comment).
fn regenerate(meter: &dyn TokenMeter) -> Result<String> {
    let mut profile = Profile::default();
    profile.learn_from(&family("delta", "task", 16));
    profile.learn_from(&family("epsilon", "job", 16));
    generate_templates(&profile, meter, 32)
}

#[test]
fn colliding_alias_skips_the_entry_and_stays_plain_or_exact() -> Result<()> {
    let meter = Bpe::o200k()?;
    let legend = learned_legend(&meter)?;
    let first_alias = legend
        .entries
        .first()
        .map(|(a, _)| a.clone())
        .unwrap_or_default();

    // The payload naturally contains the first entry's alias glyph — that
    // entry must be skipped; whatever comes out still roundtrips exactly.
    let payload = format!(
        "{}nat {first_alias} ural\n",
        family("delta", "task", 12)
    );
    let artifact = encode_extern(&payload, &meter, &legend);
    if artifact.contains("ext=") {
        let used = artifact
            .lines()
            .next()
            .unwrap_or_default()
            .split_whitespace()
            .find_map(|p| p.strip_prefix("used="))
            .unwrap_or_default()
            .to_string();
        anyhow::ensure!(
            !used.contains(&first_alias),
            "colliding alias must not be used: {artifact:?}"
        );
    }
    let keys = Keys {
        templates: Some(&legend),
        ..Keys::default()
    };
    let back = decode_with_keys(&artifact, &keys)?;
    anyhow::ensure!(back == payload, "collision case stays byte-exact");
    Ok(())
}

#[test]
fn stale_template_legend_demands_no_key() -> Result<()> {
    // A legend that matches nothing must leave a plain artifact: no ext
    // param, decodable without any file.
    let meter = Bpe::o200k()?;
    let legend = learned_legend(&meter)?;
    let prose = "the meeting moved to thursday because the room was double booked\n\
                 the meeting moved to friday because the projector was broken\n"
        .repeat(4);
    let artifact = encode_extern(&prose, &meter, &legend);
    anyhow::ensure!(
        !artifact.contains("ext="),
        "unmatched legend must not wrap: {artifact:?}"
    );
    let back = decode(&artifact)?;
    anyhow::ensure!(back == prose, "plain decode must reconstruct the payload");
    Ok(())
}

#[test]
fn template_that_does_not_pay_rides_verbatim() -> Result<()> {
    // A hand-built template that is nearly all slots: rows would carry the
    // whole line plus separators, so the per-cluster gate must refuse it
    // and the artifact must stay plain (no ext=).
    let meter = Bpe::o200k()?;
    let text = "# qodec extern templates v1 slot=quest\n码=¿ ¿ ¿ ¿ ¿ ¿\n";
    let legend = TemplateLegend::parse(text)?;
    let payload = "alpha beta gamma delta epsilon zeta\n\
                   uno dos tres cuatro cinco seis\n\
                   red orange yellow green blue violet\n"
        .repeat(3);
    let artifact = encode_extern(&payload, &meter, &legend);
    anyhow::ensure!(
        !artifact.contains("ext="),
        "an all-slot template must not pay its way in: {artifact:?}"
    );
    let back = decode(&artifact)?;
    anyhow::ensure!(back == payload, "gated artifact stays byte-exact");
    Ok(())
}

#[test]
fn duplicate_alias_and_bad_header_are_rejected() -> Result<()> {
    let dup = "# qodec extern templates v1 slot=quest\n码=a ¿ b\n码=c ¿ d\n";
    anyhow::ensure!(
        TemplateLegend::parse(dup)
            .err()
            .is_some_and(|e| format!("{e:#}").contains("duplicate")),
        "duplicate alias must refuse"
    );
    let noslot = "# qodec extern templates v1\n码=a ¿ b\n";
    anyhow::ensure!(
        TemplateLegend::parse(noslot).is_err(),
        "missing slot declaration must refuse"
    );
    let phrase_file = "# qodec extern legend v1\n码=phrase\n";
    anyhow::ensure!(
        TemplateLegend::parse(phrase_file).is_err(),
        "a phrase legend is not a template legend"
    );
    // A used alias rides in the space-separated container header, so a
    // hand-edited alias with whitespace must refuse at parse — the trust
    // boundary — not surface later as a malformed-header decode error.
    let spaced = "# qodec extern templates v1 slot=quest\nmy alias=a ¿ b\n";
    anyhow::ensure!(
        TemplateLegend::parse(spaced)
            .err()
            .is_some_and(|e| format!("{e:#}").contains("whitespace")),
        "whitespace alias must refuse"
    );
    Ok(())
}

#[test]
fn extern_rows_survive_crlf() -> Result<()> {
    let meter = Bpe::o200k()?;
    let legend = learned_legend(&meter)?;
    let payload = family("delta", "task", 12).replace('\n', "\r\n");
    let artifact = encode_extern(&payload, &meter, &legend);
    let keys = Keys {
        templates: Some(&legend),
        ..Keys::default()
    };
    let back = decode_with_keys(&artifact, &keys)?;
    anyhow::ensure!(back == payload, "CRLF extern rows must roundtrip byte-exactly");
    Ok(())
}
