//! `mosaic` — the measured-segmentation meta-codec: byte-exact roundtrip, a
//! fail-closed length-prefixed container, and the load-bearing finding that the
//! shortest-path DP declines to segment when whole-payload `squeeze` already
//! wins (it always can — `tmpl` is itself a per-line router with one shared
//! legend). See `docs/token-codec.md`, "mosaic" section.

use anyhow::Result;

use qodec::alias::Alphabet;
use qodec::meter::{Bpe, TokenMeter};
use qodec::{container, decode, encode, mosaic, CodecKind};

fn mixed_payload() -> String {
    let mut s = String::new();
    s.push_str("Intro prose describing the run; unique, not repetitive.\n\n");
    for f in ["Alpha", "Beta", "Gamma", "Delta"] {
        for ln in [12, 18, 41] {
            s.push_str(&format!(
                "src/Legacy.UI/ViewModels/{f}ViewModel.cs({ln},4): warning CS8618: \
                 Non-nullable property must contain a non-null value.\n"
            ));
        }
    }
    s.push('\n');
    for _ in 0..6 {
        s.push_str(
            "   at System.Runtime.CompilerServices.TaskAwaiter.HandleNonSuccessAndDebuggerNotification(Task task)\n",
        );
    }
    s
}

#[test]
fn mosaic_roundtrips_mixed_payload_byte_exact() -> Result<()> {
    let meter = Bpe::o200k()?;
    let text = mixed_payload();
    let encoded = encode(&text, CodecKind::Mosaic, &meter, Alphabet::Auto);
    let back = decode(&encoded)?;
    anyhow::ensure!(back == text, "mosaic must be byte-exact");
    // It must at least clear the raw floor (final acceptance guarantees it).
    anyhow::ensure!(
        meter.count(&encoded) <= meter.count(&container::raw(&text)),
        "mosaic artifact exceeds the raw floor"
    );
    Ok(())
}

#[test]
fn mosaic_container_framing_roundtrips() -> Result<()> {
    // Emit three sibling artifacts by hand and confirm the length-prefixed
    // envelope splits them back and concatenates their decodes.
    let segs = vec![
        container::raw("first segment\n"),
        container::raw("second\nsegment\n"),
        container::raw("third"),
    ];
    let artifact = mosaic::emit(&segs);
    anyhow::ensure!(
        artifact.starts_with("%q1 mosaic n=3"),
        "expected a 3-segment mosaic header, got: {:?}",
        artifact.lines().next()
    );
    let back = decode(&artifact)?;
    anyhow::ensure!(
        back == "first segment\nsecond\nsegment\nthird",
        "framing roundtrip mismatch: {back:?}"
    );
    Ok(())
}

#[test]
fn mosaic_decode_fails_closed_on_corruption() -> Result<()> {
    let segs = vec![container::raw("aaa"), container::raw("bbbbb")];
    let good = mosaic::emit(&segs);

    // Trailing garbage after the last segment: the split loop tries to read a
    // length line from the leftovers and refuses.
    let trailing = format!("{good}garbage-with-no-newline");
    anyhow::ensure!(
        decode(&trailing).is_err(),
        "trailing garbage must fail closed"
    );

    // Header count that disagrees with the body's segment count.
    let miscount = good.replacen("n=2", "n=3", 1);
    anyhow::ensure!(
        decode(&miscount).is_err(),
        "segment count mismatch must fail closed"
    );

    // A length prefix that overruns the body.
    let c = container::parse(&good)?;
    let overrun = container::emit(&container::Container {
        codec: "mosaic".to_string(),
        params: vec![("n".to_string(), "1".to_string())],
        legend: Vec::new(),
        body: format!("999999\n{}", c.body.get(2..).unwrap_or_default()),
    });
    anyhow::ensure!(
        decode(&overrun).is_err(),
        "segment length overrun must fail closed"
    );
    Ok(())
}

#[test]
fn mosaic_declines_to_segment_when_whole_payload_wins() -> Result<()> {
    // The kill-criterion finding as a regression test: on a uniform repetitive
    // payload a single whole-payload structural codec is optimal, so the DP
    // must choose exactly one segment rather than fragmenting (which would pay
    // a per-segment container tax for nothing).
    let meter = Bpe::o200k()?;
    let mut text = String::new();
    for i in 0..40 {
        text.push_str(&format!(
            "src/svc/Worker{}.cs({},4): warning CS8618: Non-nullable field 'x{}' uninitialized\n",
            i % 3,
            10 + i,
            i % 7
        ));
    }
    let stage1 = mosaic::encode(&text, &meter);
    let c = container::parse(&stage1)?;
    anyhow::ensure!(c.codec == "mosaic", "expected a mosaic container");
    let segs = mosaic::split(&c)?;
    anyhow::ensure!(
        segs.len() == 1,
        "DP over-fragmented a uniform payload into {} segments",
        segs.len()
    );
    Ok(())
}

#[test]
fn mosaic_never_loses_to_the_raw_floor() -> Result<()> {
    let meter = Bpe::o200k()?;
    for text in [
        "tiny\n",
        "a b\n",
        "{\"k\": 1}\n",
        "unique prose line only\n",
    ] {
        let encoded = encode(text, CodecKind::Mosaic, &meter, Alphabet::Auto);
        let raw_floor = meter.count(&container::raw(text));
        anyhow::ensure!(
            meter.count(&encoded) <= raw_floor,
            "mosaic artifact for {text:?} exceeds the raw floor"
        );
        anyhow::ensure!(decode(&encoded)? == text, "mosaic roundtrip for {text:?}");
    }
    Ok(())
}

#[test]
fn mosaic_survives_hostile_and_tiny_input() -> Result<()> {
    let meter = Bpe::o200k()?;
    // %q1-shaped lines, sigils, CRLF, no trailing newline: the same hostile
    // shapes the other codecs are pinned against.
    let hostile = "§0 already here ¤ µ 码引路\r\n%q1 body\r\n%q1 x3\r\n\
                   repeated hostile line with enough words to mine maybe\r\n\
                   repeated hostile line with enough words to mine maybe";
    anyhow::ensure!(
        decode(&encode(hostile, CodecKind::Mosaic, &meter, Alphabet::Auto))? == hostile,
        "mosaic must survive hostile input byte-exact"
    );
    for text in ["", "x", "\n", "\r\n"] {
        anyhow::ensure!(
            decode(&encode(text, CodecKind::Mosaic, &meter, Alphabet::Auto))? == text,
            "mosaic roundtrip for {text:?}"
        );
    }
    Ok(())
}
