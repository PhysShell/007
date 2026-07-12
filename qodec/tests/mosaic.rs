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

    // A monstrous segment count from an untrusted header must be rejected
    // before it sizes an allocation — not OOM or panic.
    let bomb = "%q1 mosaic n=999999999999999\n%q1 body\n0\n";
    anyhow::ensure!(
        decode(bomb).is_err(),
        "an unreasonable segment count must fail closed, not allocate"
    );

    // A nested mosaic segment must be refused (stack-exhaustion guard).
    let nested = mosaic::emit(std::slice::from_ref(&good));
    anyhow::ensure!(
        decode(&nested).is_err(),
        "nested mosaic segments must be rejected"
    );
    Ok(())
}

fn uniform_diagnostics(rows: usize) -> String {
    let mut text = String::new();
    for i in 0..rows {
        text.push_str(&format!(
            "src/svc/Worker{}.cs({},4): warning CS8618: Non-nullable field 'x{}' uninitialized\n",
            i % 3,
            10 + i,
            i % 7
        ));
    }
    text
}

#[test]
fn mosaic_identity_elides_a_single_segment() -> Result<()> {
    // The load-bearing fix: when the router declines to segment, the result is
    // the bare winning codec — no `%q1 mosaic` envelope — so "no split" costs
    // exactly what the plain codec costs, never a self-inflicted container tax.
    let meter = Bpe::o200k()?;
    let text = uniform_diagnostics(40);
    let encoded = mosaic::encode(&text, &meter);
    let c = container::parse(&encoded)?;
    anyhow::ensure!(
        c.codec != "mosaic",
        "a single-segment result must be emitted bare, got a {} envelope",
        c.codec
    );
    // And it must not cost more than the whole-payload structural baseline.
    let baseline = [
        CodecKind::Fold,
        CodecKind::Grep,
        CodecKind::Diag,
        CodecKind::Tmpl,
    ]
    .iter()
    .map(|k| meter.count(&encode(&text, *k, &meter, Alphabet::Auto)))
    .min()
    .unwrap_or(usize::MAX);
    anyhow::ensure!(
        meter.count(&encoded) <= baseline,
        "elided mosaic ({}) exceeds the whole-span baseline ({baseline})",
        meter.count(&encoded)
    );
    Ok(())
}

#[test]
fn all_span_dp_declines_to_segment() -> Result<()> {
    // The honest kill criterion: run the all-span additive DP (every span, not
    // the geometric grid) and inspect its *pre-arbitration* choice — not the
    // baseline-clamped result, which is optimal by construction and would prove
    // nothing. On these payloads (including ones built to favour segmentation:
    // disjoint-vocab regions; format-specific diag+rg blocks) the DP itself
    // declines to split, so the negative is the DP's verdict, not a fallback.
    let meter = Bpe::o200k()?;
    let payloads = [
        ("mixed", mixed_payload()),
        ("uniform40", uniform_diagnostics(40)),
        ("hetero", hetero_disjoint_vocab()),
        ("format2", format_specific_regions()),
    ];
    for (name, text) in payloads {
        let report = mosaic::all_span_dp(&text, &meter, &[])
            .ok_or_else(|| anyhow::anyhow!("payload {name} exceeds the DP bound"))?;
        anyhow::ensure!(
            decode(&report.artifact)? == text,
            "DP artifact roundtrip for {name}"
        );
        // The strong claim, provable only via the pre-arbitration report: the
        // DP *chose* a single segment (no split), and it exactly ties the
        // whole-span baseline.
        anyhow::ensure!(
            report.segments == 1,
            "all-span DP for {name} split into {} segments (additive {}, exact {})",
            report.segments,
            report.additive_cost,
            report.exact_tokens
        );
        anyhow::ensure!(
            report.exact_tokens == report.baseline_tokens,
            "single-segment DP for {name} ({}) must equal the baseline ({})",
            report.exact_tokens,
            report.baseline_tokens
        );
        eprintln!(
            "all_span_dp[{name}]: segments={} exact={} baseline={}",
            report.segments, report.exact_tokens, report.baseline_tokens
        );
    }
    Ok(())
}

#[test]
fn mosaic_routing_stage_honours_template_seeds() -> Result<()> {
    // Point-3 regression: `best_span` must seed its `tmpl` candidate with the
    // profile templates, else a learned profile changes squeeze's candidates
    // but not mosaic's and the "mosaic == squeeze" claim holds only for the
    // empty profile. A frozen template that eats a whole line makes the seeded
    // routing strictly beat the unseeded one on a payload tmpl clusters poorly.
    let meter = Bpe::o200k()?;
    // Two same-shape families sharing most of their words — first-fit tmpl
    // merges them into a weak two-slot cluster; a sealed template per family
    // clusters them cleanly.
    let mut text = String::new();
    for i in 0..12 {
        text.push_str(&format!(
            "event alpha region {} committed offset {} to durable log\n",
            i % 4,
            1000 + i
        ));
        text.push_str(&format!(
            "event beta region {} committed offset {} to durable log\n",
            i % 4,
            2000 + i
        ));
    }
    let templates = vec![
        vec![
            "event alpha region ".to_string(),
            " committed offset ".to_string(),
            " to durable log".to_string(),
        ],
        vec![
            "event beta region ".to_string(),
            " committed offset ".to_string(),
            " to durable log".to_string(),
        ],
    ];
    let plain = mosaic::encode_seeded(&text, &meter, &[]);
    let seeded = mosaic::encode_seeded(&text, &meter, &templates);
    anyhow::ensure!(decode(&seeded)? == text, "seeded mosaic must roundtrip");
    anyhow::ensure!(
        meter.count(&seeded) <= meter.count(&plain),
        "seeded routing ({}) must not lose to unseeded ({}) — seeds are ignored",
        meter.count(&seeded),
        meter.count(&plain)
    );
    Ok(())
}

/// Two regions with *disjoint* vocabularies (diag-shaped + rg-shaped) framing
/// unique prose — the best case for segmentation, since global mining cannot
/// bridge the regions and each wants a different structural codec.
fn hetero_disjoint_vocab() -> String {
    let mut s = String::new();
    for f in ["Parser", "Lexer", "Emitter", "Optimizer", "Linker"] {
        for ln in [12, 18, 41] {
            s.push_str(&format!(
                "compiler/frontend/{f}Stage.rs:{ln}: warning: unused variable `scratch_buffer_{ln}`\n"
            ));
        }
    }
    s.push('\n');
    for f in ["billing", "invoice", "ledger", "payroll"] {
        for ln in [7, 33, 88] {
            s.push_str(&format!(
                "services/finance/{f}.py:{ln}:    total = compute_gross_amount(transaction_batch)\n"
            ));
        }
    }
    s.push_str("\nOwnership of the finance module moves to the platform squad next quarter.\n");
    s
}

/// A large diagnostic block (diag's specialty, one distinct message per line so
/// tmpl clusters poorly) followed by a large ripgrep block (grep's specialty).
fn format_specific_regions() -> String {
    // `.get(i % len)` rather than `[i % len]`: the tree denies index-slicing.
    let nth = |arr: &[&'static str], i: usize| -> &'static str {
        arr.get(i % arr.len().max(1)).copied().unwrap_or_default()
    };
    let mut s = String::new();
    let codes = ["CS8618", "CS8602", "CS0168", "CS0219", "CS4014"];
    let msgs = [
        "Non-nullable field must contain a non-null value",
        "Dereference of a possibly null reference",
        "The variable is declared but never used",
        "The variable is assigned but its value is never used",
        "Because this call is not awaited execution continues",
    ];
    let diag_names = ["Alpha", "Beta", "Gamma", "Delta"];
    for i in 0..24 {
        s.push_str(&format!(
            "src/Legacy.UI/ViewModels/{}ViewModel.cs({},{}): warning {}: {} 'field_{}'\n",
            nth(&diag_names, i),
            10 + i,
            5 + i % 7,
            nth(&codes, i),
            nth(&msgs, i),
            i % 9
        ));
    }
    let grep_names = ["parser", "lexer", "emitter"];
    for i in 0..24 {
        s.push_str(&format!(
            "tooling/analysis/{}_pass.rs:{}:{}:    let node_index = resolve_symbol_reference(scope_table, ident);\n",
            nth(&grep_names, i),
            100 + i,
            3 + i % 5
        ));
    }
    s
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
