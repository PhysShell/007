//! Format-specialized codecs (`grep`, `diag`) — the byte-roundtrip
//! guarantee and the honest-refusal behavior, on realistic shapes.

use anyhow::Result;

use qodec::alias::Alphabet;
use qodec::meter::{Bpe, TokenMeter};
use qodec::{decode, encode, CodecKind};

fn roundtrip(text: &str, kind: CodecKind, meter: &dyn TokenMeter) -> Result<String> {
    let encoded = encode(text, kind, meter, Alphabet::Auto);
    let back = decode(&encoded)?;
    anyhow::ensure!(
        back == text,
        "byte roundtrip failed for {:?}",
        kind.label()
    );
    Ok(encoded)
}

#[test]
fn grep_groups_rg_output() -> Result<()> {
    let meter = Bpe::o200k()?;
    let mut text = String::new();
    for file in ["src/Broker/Transport/Session.cs", "src/Broker/Handlers/Inbox.cs"] {
        for line in [14, 88, 129, 245, 310] {
            text.push_str(&format!(
                "{file}:{line}:        obj.PropertyChanged += new PropertyChangedEventHandler(OnChanged);\n"
            ));
        }
    }
    let encoded = roundtrip(&text, CodecKind::Grep, &meter)?;
    anyhow::ensure!(encoded.starts_with("%q1 grep"), "expected grep container");
    anyhow::ensure!(
        meter.count(&encoded) < meter.count(&text),
        "grouping repeated paths must reduce tokens"
    );
    Ok(())
}

#[test]
fn grep_windows_paths_and_mixed_lines() -> Result<()> {
    // Drive-letter colons must not split the path; unparsed lines ride in
    // passthrough sections between groups, byte-exact.
    let meter = Bpe::o200k()?;
    let mut text = String::new();
    for file in ["A", "B"] {
        for line in [12, 99, 245, 371, 402, 518, 633, 780] {
            text.push_str(&format!(
                "C:\\Repos\\STS_new\\SectorTS\\Broker\\CommonControls\\{file}.xaml.cs:{line}:hit\n"
            ));
        }
        text.push_str("-- separator with no match format --\n");
    }
    let encoded = roundtrip(&text, CodecKind::Grep, &meter)?;
    anyhow::ensure!(encoded.starts_with("%q1 grep"), "expected grep container");
    let decoded_head = encoded
        .lines()
        .nth(2)
        .unwrap_or_default();
    anyhow::ensure!(
        decoded_head.ends_with("A.xaml.cs"),
        "group header must carry the whole Windows path, got {decoded_head:?}"
    );
    Ok(())
}

#[test]
fn grep_refuses_prose() -> Result<()> {
    let meter = Bpe::o200k()?;
    let text = "no matcher output here, just a sentence.\nand another one.\n";
    let encoded = roundtrip(text, CodecKind::Grep, &meter)?;
    anyhow::ensure!(
        encoded.starts_with("%q1 raw"),
        "no repeated paths -> honest raw fallback"
    );
    Ok(())
}

#[test]
fn diag_templates_quoted_identifiers() -> Result<()> {
    // The ownsharp shape: one sentence, thousands of repeats, only the
    // head and the quoted identifiers vary.
    let meter = Bpe::o200k()?;
    let mut text = String::new();
    for (file, line, ev, host) in [
        ("Broker/AmountWindow.xaml.cs", 72, "fGoods.PropertyChanged", "AmountWindow"),
        ("Broker/eDeclarant.xaml.cs", 69, "data.PropertyChanged", "eDeclarant"),
        ("Broker/eDeclarant.xaml.cs", 236, "fThis.DataSource.PropertyChanged", "eDeclarant"),
        ("Broker/Estimate.xaml.cs", 41, "model.PropertyChanged", "Estimate"),
    ] {
        text.push_str(&format!(
            "../STS_new/SectorTS/{file}:{line}: warning: [OWN001] event '{ev}' is subscribed but never unsubscribed; it may keep '{host}' alive (possible leak) [resource: subscription token]\n"
        ));
    }
    let encoded = roundtrip(&text, CodecKind::Diag, &meter)?;
    anyhow::ensure!(encoded.starts_with("%q1 diag"), "expected diag container");
    anyhow::ensure!(
        meter.count(&encoded) < meter.count(&text),
        "repeated template must reduce tokens"
    );
    Ok(())
}

#[test]
fn diag_msbuild_heads_and_passthrough() -> Result<()> {
    let meter = Bpe::o200k()?;
    let mut text = String::from("Build started 12.07.2026\n");
    for (file, l, c) in [("Views\\Main.xaml.cs", 88, 13), ("Views\\Edit.xaml.cs", 41, 9), ("Core\\Db.cs", 7, 22)] {
        text.push_str(&format!(
            "  C:\\Repos\\STS_new\\{file}({l},{c}): warning CS8618: Non-nullable field 'fBroker' must contain a non-null value [C:\\Repos\\STS_new\\STS.csproj]\n"
        ));
    }
    text.push_str("    3 Warning(s)\n");
    let encoded = roundtrip(&text, CodecKind::Diag, &meter)?;
    anyhow::ensure!(encoded.starts_with("%q1 diag"), "expected diag container");
    Ok(())
}

#[test]
fn diag_crlf_commits_and_roundtrips() -> Result<()> {
    // container::parse strips a trailing \r from legend lines, so a CRLF
    // line ending must never reach the template (Codex review on PR #32).
    let meter = Bpe::o200k()?;
    let mut text = String::new();
    for i in 0..12 {
        text.push_str(&format!(
            "../STS_new/Broker/File{}.xaml.cs:{}: warning: [OWN001] event 'obj{i}.PropertyChanged' is subscribed but never unsubscribed; it may keep 'Host{}' alive\r\n",
            i % 3,
            40 + i,
            i % 3,
        ));
    }
    let encoded = roundtrip(&text, CodecKind::Diag, &meter)?;
    anyhow::ensure!(
        encoded.starts_with("%q1 diag"),
        "CRLF diagnostics must still commit"
    );
    Ok(())
}

#[test]
fn diag_bare_cr_lines_travel_verbatim() -> Result<()> {
    // A CR that is not part of a CRLF ending would land inside a template
    // or slot; such lines must pass through untouched.
    let meter = Bpe::o200k()?;
    let mut text = String::from("a.cs:1: warning 'we\rird' value\n");
    for i in 0..12 {
        text.push_str(&format!(
            "src/Broker/Handlers/b.cs:{i}: warning: [OWN007] local variable 'x{i}' is assigned but its value is never used\n"
        ));
    }
    let encoded = roundtrip(&text, CodecKind::Diag, &meter)?;
    anyhow::ensure!(encoded.starts_with("%q1 diag"), "clean lines still commit");
    Ok(())
}

#[test]
fn grep_crlf_roundtrips() -> Result<()> {
    let meter = Bpe::o200k()?;
    let mut text = String::new();
    for file in ["A", "B"] {
        for i in 0..10 {
            text.push_str(&format!("src/Broker/{file}.cs:{i}:obj.PropertyChanged += handler;\r\n"));
        }
    }
    let encoded = roundtrip(&text, CodecKind::Grep, &meter)?;
    anyhow::ensure!(encoded.starts_with("%q1 grep"), "CRLF rg output must still commit");
    Ok(())
}

#[test]
fn diag_unbalanced_quotes_and_prose_fall_back() -> Result<()> {
    let meter = Bpe::o200k()?;
    // Unbalanced quote in a tail -> that line rides verbatim; nothing
    // repeats -> the whole artifact honestly refuses.
    let text = "a.cs:1: it's got an odd quote\nplain prose line\n";
    let encoded = roundtrip(text, CodecKind::Diag, &meter)?;
    anyhow::ensure!(
        encoded.starts_with("%q1 raw"),
        "nothing template-able -> raw"
    );
    Ok(())
}
