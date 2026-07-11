//! The propose/verify loop's substrate — parametric span rules. Under
//! test: the glob-span applies and inverts byte-exactly (the verifier's
//! core), rules pay only measured, the artifact pins the key and fails
//! closed, and the niche claim is real: a phrase-with-holes repeating
//! across *different line shapes*, which neither mine nor tmpl covers.

use anyhow::{Context, Result};

use qodec::alias::Alphabet;
use qodec::meter::{Bpe, TokenMeter};
use qodec::rules::{apply, delimiters, expand_spans, RulesKey};
use qodec::{decode, decode_with_keys, encode, CodecKind, Keys};

/// The cross-shape corpus: the same parametric phrase embedded in lines of
/// *different* seg shapes, so tmpl never clusters them, and with a varying
/// identifier inside, so mine has no exact literal.
fn cross_shape(n: usize) -> String {
    let mut text = String::new();
    for i in 0..n {
        text.push_str(&format!(
            "warn: it may outlive and keep 'Window{i}' alive (possible leak) [resource: token]\n"
        ));
        text.push_str(&format!(
            "note: reviewed 2026-07-0{} — dispose chain unclear, it may outlive and keep 'Panel{i}' alive (possible leak) so flagging for triage\n",
            1 + i % 9,
        ));
        text.push_str(&format!("checked {i} refs\n"));
    }
    text
}

const KEY_TEXT: &str = "# qodec rules v1 slot=quest\n\
                        码=it may outlive and keep '¿' alive (possible leak)\n";

/// The raw-container floor the wrapper gate measures against.
fn container_raw_len_probe(text: &str) -> String {
    format!("%q1 raw\n{text}")
}

#[test]
fn rules_key_parses_and_guards_its_boundaries() -> Result<()> {
    let key = RulesKey::parse(KEY_TEXT)?;
    anyhow::ensure!(key.entries.len() == 1 && key.slot == '¿', "one entry, quest slot");
    let again = RulesKey::parse(KEY_TEXT)?;
    anyhow::ensure!(key.sum == again.sum, "checksum is a pure function of bytes");

    for (bad, why) in [
        ("# qodec rules v1 slot=quest\n码=¿ tail\n", "wildcard start"),
        ("# qodec rules v1 slot=quest\n码=head ¿\n", "wildcard end"),
        ("# qodec rules v1 slot=quest\nmy alias=a ¿ b\n", "whitespace alias"),
        ("# qodec rules v1 slot=quest\n码=a ¿ b\n码=c ¿ d\n", "duplicate alias"),
        ("# qodec rules v1\n码=a ¿ b\n", "missing slot"),
    ] {
        anyhow::ensure!(RulesKey::parse(bad).is_err(), "{why} must refuse");
    }
    Ok(())
}

#[test]
fn spans_apply_and_invert_byte_exactly_across_line_shapes() -> Result<()> {
    let meter = Bpe::o200k()?;
    let key = RulesKey::parse(KEY_TEXT)?;
    let text = cross_shape(10);
    let applied = apply(&text, &key, &meter).context("delimiters available")?;
    anyhow::ensure!(applied.used == ["码"], "the rule must pay and apply");
    anyhow::ensure!(
        meter.count(&applied.text) < meter.count(&text),
        "the rewrite itself must already win tokens"
    );
    // Both shapes rewrote: the span text is gone from every line.
    anyhow::ensure!(
        !applied.text.contains("it may outlive"),
        "every occurrence must be rewritten: {:.200}",
        applied.text,
    );
    let (start, end, sep) = delimiters(&applied)?;
    let back = expand_spans(&applied.text, &key, start, end, sep, &applied.used.concat())?;
    anyhow::ensure!(back == text, "span inversion is byte-exact");
    Ok(())
}

#[test]
fn rules_artifact_beats_plain_and_fails_closed() -> Result<()> {
    let meter = Bpe::o200k()?;
    let key = RulesKey::parse(KEY_TEXT)?;
    let text = cross_shape(10);

    // The full pre-pass + codec + wrapper flow, as cmd_encode runs it.
    // Honesty note: no "beats plain squeeze" claim here — the miners can
    // take a one-hole phrase as *two* literals, and whether the rule's
    // bracket overhead beats mine's in-artifact legend is a per-payload
    // measurement, not an invariant (the warm/extern comparison is where
    // rules earn; the design doc records the live numbers).
    let applied = apply(&text, &key, &meter).context("delimiters available")?;
    let inner = encode(&applied.text, CodecKind::Squeeze, &meter, Alphabet::Auto);
    let artifact = qodec::rules::wrap_if_used(inner, &key, &applied, &meter, &text);
    anyhow::ensure!(
        artifact.starts_with("%q1 rules"),
        "wrapper must engage: {artifact:.60}"
    );
    anyhow::ensure!(
        meter.count(&artifact) < meter.count(&container_raw_len_probe(&text)),
        "the wrapped artifact must clear the raw floor"
    );

    let keys = Keys {
        rules: Some(&key),
        ..Keys::default()
    };
    let back = decode_with_keys(&artifact, &keys)?;
    anyhow::ensure!(back == text, "keyed decode is byte-exact");

    let refused = decode(&artifact);
    anyhow::ensure!(
        refused
            .as_ref()
            .err()
            .is_some_and(|e| format!("{e:#}").contains("rules key")),
        "keyless decode must name the missing key: {refused:?}"
    );
    let drifted = RulesKey::parse(&format!("{KEY_TEXT}# drift\n"))?;
    let wrong = decode_with_keys(
        &artifact,
        &Keys {
            rules: Some(&drifted),
            ..Keys::default()
        },
    );
    anyhow::ensure!(
        wrong.as_ref().err().is_some_and(|e| format!("{e:#}").contains("mismatch")),
        "checksum drift must refuse: {wrong:?}"
    );
    Ok(())
}

#[test]
fn unpaying_or_unmatched_rules_add_no_wrapper() -> Result<()> {
    let meter = Bpe::o200k()?;
    let key = RulesKey::parse(KEY_TEXT)?;
    let prose = "nothing here resembles the pattern at all\n".repeat(4);
    let applied = apply(&prose, &key, &meter).context("delimiters available")?;
    anyhow::ensure!(applied.used.is_empty() && applied.text == prose, "no match, no rewrite");
    let inner = encode(&prose, CodecKind::Squeeze, &meter, Alphabet::Auto);
    let artifact = qodec::rules::wrap_if_used(inner.clone(), &key, &applied, &meter, &prose);
    anyhow::ensure!(artifact == inner, "no used rules -> no key demand");
    let back = decode(&artifact)?;
    anyhow::ensure!(back == prose, "plain decode reconstructs");
    Ok(())
}

#[test]
fn alias_collision_and_crlf_stay_exact() -> Result<()> {
    let meter = Bpe::o200k()?;
    let key = RulesKey::parse(KEY_TEXT)?;
    // Payload contains the alias glyph — the rule must be skipped whole.
    let colliding = format!("{}natural 码 here\n", cross_shape(4));
    let applied = apply(&colliding, &key, &meter).context("delimiters available")?;
    anyhow::ensure!(applied.used.is_empty(), "colliding alias must skip the rule");

    // CRLF: spans sit inside CR-terminated lines untouched by rewriting.
    let crlf = cross_shape(6).replace('\n', "\r\n");
    let applied = apply(&crlf, &key, &meter).context("delimiters available")?;
    anyhow::ensure!(applied.used == ["码"], "CRLF payload must still apply");
    let (start, end, sep) = delimiters(&applied)?;
    let back = expand_spans(&applied.text, &key, start, end, sep, &applied.used.concat())?;
    anyhow::ensure!(back == crlf, "CRLF span inversion is byte-exact");
    Ok(())
}

#[test]
fn sequential_rules_never_capture_earlier_spans() -> Result<()> {
    // The second rule's wildcard could swallow the first's emitted `⌈码|x⌉` —
    // whitespace-free, so the old matcher accepted it and decode then
    // broke on the nested delimiters (Codex, PR #39). A match whose span
    // contains a delimiter glyph must be rejected instead.
    let meter = Bpe::o200k()?;
    let key = RulesKey::parse(
        "# qodec rules v1 slot=quest\n\
         码=initialization of module '¿' completed with all invariants intact and resources pinned\n\
         引=BEGIN transaction ¿ committed after full quorum acknowledgement END\n",
    )?;
    let mut text = String::new();
    for i in 0..6 {
        // The first rule's phrase sits exactly where the second's wildcard
        // looks: in raw text the second cannot match there (whitespace in
        // the gap), but after the first rewrites, the gap becomes the
        // whitespace-free ⌈码|…⌉ bytes.
        text.push_str(&format!(
            "BEGIN transaction initialization of module 'Mod{i}' completed with all invariants intact and resources pinned committed after full quorum acknowledgement END\n"
        ));
        // The second rule's own paying habitat, no first-rule span inside.
        text.push_str(&format!(
            "BEGIN transaction tx{i} committed after full quorum acknowledgement END\n"
        ));
    }
    let applied = apply(&text, &key, &meter).context("delimiters available")?;
    anyhow::ensure!(
        applied.used.contains(&"码".to_string()) && applied.used.contains(&"引".to_string()),
        "both rules must pay on their own habitats: {:?}",
        applied.used
    );
    let (start, end, sep) = delimiters(&applied)?;
    let back = expand_spans(&applied.text, &key, start, end, sep, &applied.used.concat())?;
    anyhow::ensure!(back == text, "composed application must invert byte-exactly");
    Ok(())
}

#[test]
fn failed_multibyte_anchor_advances_by_a_whole_char() -> Result<()> {
    // The first fixed part starts with a multibyte char; the first
    // occurrence fails (whitespace in the wildcard), and the old +1 byte
    // bump landed mid-UTF-8, killing the line's remaining scan — the
    // valid second occurrence was lost (Codex, PR #39).
    let meter = Bpe::o200k()?;
    let key = RulesKey::parse(
        "# qodec rules v1 slot=quest\n\
         码=протокол сеанса '¿' завершён без потерь и расхождений по контрольным суммам\n",
    )?;
    let mut text = String::new();
    for i in 0..8 {
        // The first anchor hit fails (whitespace lands in the wildcard),
        // the second is valid — reachable only if the failed anchor
        // advances by a whole char.
        text.push_str(&format!(
            "протокол сеанса 'сбой в кавычках' начат; протокол сеанса 'запись{i}.данные' завершён без потерь и расхождений по контрольным суммам\n"
        ));
    }
    let applied = apply(&text, &key, &meter).context("delimiters available")?;
    anyhow::ensure!(
        applied.used == ["码"],
        "the second, valid occurrence must still match: {:?}",
        applied.used
    );
    anyhow::ensure!(
        !applied.text.contains("завершён без потерь"),
        "every valid occurrence must be rewritten"
    );
    let (start, end, sep) = delimiters(&applied)?;
    let back = expand_spans(&applied.text, &key, start, end, sep, &applied.used.concat())?;
    anyhow::ensure!(back == text, "multibyte-anchored spans invert byte-exactly");
    Ok(())
}

#[test]
fn overlapping_aliases_refuse_and_used_tokenizes_exactly() -> Result<()> {
    // `used` is a concatenation; substring membership would let an unused
    // alias ride on a recorded one (CodeRabbit, PR #39). Overlapping
    // aliases are refused at the trust boundary, and expansion tokenizes
    // `used` against the key, failing closed on unknown residue.
    let overlapping = "# qodec rules v1 slot=quest\nR1=alpha ¿ beta\nR12=gamma ¿ delta\n";
    anyhow::ensure!(
        RulesKey::parse(overlapping)
            .err()
            .is_some_and(|e| format!("{e:#}").contains("overlaps")),
        "prefix-overlapping aliases must refuse at parse"
    );

    let key = RulesKey::parse(KEY_TEXT)?;
    // A span referencing an alias while `used` records something else
    // entirely must bail on the residue, not silently pass.
    let bogus = expand_spans("⌈码|X⌉", &key, '⌈', '⌉', '|', "zz");
    anyhow::ensure!(
        bogus
            .err()
            .is_some_and(|e| format!("{e:#}").contains("residue")),
        "unknown used residue must fail closed"
    );
    Ok(())
}
