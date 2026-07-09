//! Slice selection semantics — dropping records is the whole lever, so
//! descent, clause matching and kept/total accounting must be exact, and
//! the output must stay valid toon food (one uniform array).

use anyhow::{Context, Result};
use serde_json::Value;

use qodec::alias::Alphabet;
use qodec::meter::Bpe;
use qodec::slice::{slice, Clause};
use qodec::{decode, encode, CodecKind};

/// An envelope mimicking the real findings.json shape: metadata object on
/// top, one uniform record array under a key.
const DOC: &str = r#"{
  "coverage": {"tools": ["codeql", "own-check"]},
  "findings": [
    {"tool":"codeql","path":"src/Data/Query.cs","line":112,"rule":"CS-SQLI","suppressed":false,"resource":null,"message":"possible SQL injection via string concat"},
    {"tool":"own-check","path":"src/UI/Main.cs","line":40,"rule":"OC-7","suppressed":true,"resource":null,"message":"unused local variable"},
    {"tool":"codeql","path":"lib/Legacy/Broker.cs","line":7,"rule":"CS8618","suppressed":false,"resource":null,"message":"non-nullable field is uninitialized"}
  ]
}"#;

fn clauses(specs: &[&str]) -> Result<Vec<Clause>> {
    specs.iter().map(|s| Clause::parse(s)).collect()
}

fn kept(doc: &str, key: &str, specs: &[&str]) -> Result<usize> {
    Ok(slice(doc, key, &clauses(specs)?)?.kept)
}

#[test]
fn descends_and_filters() -> Result<()> {
    let s = slice(DOC, "findings", &clauses(&["tool=codeql"])?)?;
    anyhow::ensure!(
        s.kept == 2 && s.total == 3,
        "expected 2/3, got {}/{}",
        s.kept,
        s.total
    );
    let arr: Value = serde_json::from_str(&s.body)?;
    let arr = arr.as_array().context("slice body must be an array")?;
    anyhow::ensure!(
        arr.len() == 2
            && arr
                .iter()
                .all(|r| r.get("tool").and_then(Value::as_str) == Some("codeql")),
        "every survivor must match the clause: {}",
        s.body
    );
    Ok(())
}

#[test]
fn clause_semantics_cover_field_types() -> Result<()> {
    // Bools and nulls compare by their JSON text, numbers likewise,
    // strings by content; `~` is substring; missing fields fail `=`/`~`
    // and pass `!=`.
    for (specs, want) in [
        (&["suppressed!=true"][..], 2),
        (&["line=40"][..], 1),
        (&["message~injection"][..], 1),
        (&["resource=null"][..], 3),
        (&["tool=codeql", "path~Legacy"][..], 1),
        (&["nosuch=x"][..], 0),
        (&["nosuch!=x"][..], 3),
    ] {
        let got = kept(DOC, "findings", specs)?;
        anyhow::ensure!(got == want, "{specs:?}: expected {want} kept, got {got}");
    }
    Ok(())
}

#[test]
fn first_operator_wins_and_bad_clauses_error() -> Result<()> {
    // The operator is the first `=`/`~`, so values may contain operators.
    let doc = r#"[{"message":"xa=bz","path":"a~b"}]"#;
    anyhow::ensure!(kept(doc, "", &["message~a=b"])? == 1, "needle with `=`");
    anyhow::ensure!(kept(doc, "", &["path=a~b"])? == 1, "value with `~`");

    for bad in ["novalue", "=x", "!=x"] {
        anyhow::ensure!(
            Clause::parse(bad).is_err(),
            "clause {bad:?} must be rejected"
        );
    }
    Ok(())
}

#[test]
fn root_array_and_bom_are_accepted() -> Result<()> {
    // Windows tooling loves a UTF-8 BOM; the document root may itself be
    // the array (no --key).
    let doc = "\u{feff}[{\"a\":1},{\"a\":2}]";
    let s = slice(doc, "", &clauses(&["a!=1"])?)?;
    anyhow::ensure!(
        s.kept == 1 && s.total == 2,
        "expected 1/2, got {}/{}",
        s.kept,
        s.total
    );
    Ok(())
}

#[test]
fn wrong_paths_fail_with_context() -> Result<()> {
    let not_array = slice(DOC, "coverage", &[]);
    anyhow::ensure!(
        not_array
            .as_ref()
            .err()
            .is_some_and(|e| e.to_string().contains("not an array")),
        "descending to an object must name the shape problem: {not_array:?}"
    );
    let missing = slice(DOC, "findings.nope", &[]);
    anyhow::ensure!(
        missing
            .as_ref()
            .err()
            .is_some_and(|e| e.to_string().contains("nope")),
        "a missing step must be named: {missing:?}"
    );
    Ok(())
}

#[test]
fn slice_output_is_toon_food() -> Result<()> {
    // The point of slicing an envelope: the result is one uniform array,
    // which toon turns into a keys-once table. Repetitive records so the
    // table reliably beats the raw JSON on tokens.
    let mut records = Vec::new();
    for i in 0..8 {
        records.push(format!(
            r#"{{"tool":"codeql","path":"src/Legacy.UI/ViewModels/Model{i}.cs","line":{i},"rule":"CS8618","suppressed":false,"message":"non-nullable field is uninitialized"}}"#
        ));
    }
    let doc = format!(r#"{{"meta":{{"n":8}},"findings":[{}]}}"#, records.join(","));

    let s = slice(&doc, "findings", &[])?;
    let meter = Bpe::o200k()?;
    let encoded = encode(&s.body, CodecKind::Toon, &meter, Alphabet::Auto);
    anyhow::ensure!(
        encoded.starts_with("%q1 toon"),
        "uniform slice must toon-encode, got: {}",
        encoded.lines().next().unwrap_or("")
    );
    let back: Value = serde_json::from_str(&decode(&encoded)?)?;
    let original: Value = serde_json::from_str(&s.body)?;
    anyhow::ensure!(back == original, "toon roundtrip must be value-equal");
    Ok(())
}
