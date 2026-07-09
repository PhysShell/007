//! `slice` — select a record slice out of a bigger JSON document.
//!
//! Report-style JSON (findings.json and friends) wraps one huge uniform
//! array in an envelope object, and an agent rarely needs all of it. The
//! biggest token lever is *not sending records at all* — ahead of any
//! codec: a 26 MB findings.json is ~7 M tokens raw, the relevant slice is
//! usually a couple of orders of magnitude smaller. Descend to the array
//! by dotted key path, keep records matching every clause, and emit a
//! compact JSON array — ready to feed raw or pipe into
//! `encode --codec toon` (the records keep their one uniform shape).
//!
//! Output is canonical compact JSON (`serde_json` values, BTreeMap key
//! order) — the same value-equal convention as `toon`'s decode, not a
//! byte-exact span of the input.

use anyhow::{bail, Context, Result};
use serde_json::Value;

/// One `--where` filter. A record must match every clause to survive.
#[derive(Debug)]
pub struct Clause {
    key: String,
    op: Op,
    value: String,
}

#[derive(Debug)]
enum Op {
    /// `key=value` — the field's fragment equals the value.
    Eq,
    /// `key!=value` — the field's fragment differs (missing fields differ).
    Ne,
    /// `key~needle` — the field's fragment contains the substring.
    Has,
}

impl Clause {
    /// Parse `key=value`, `key!=value` or `key~substring`. The operator is
    /// the *first* `=` or `~` in the clause (a `!` immediately before `=`
    /// negates), so values are free to contain any operator characters.
    pub fn parse(s: &str) -> Result<Self> {
        let pos = s.find(['=', '~']).with_context(|| {
            format!("clause {s:?} needs `key=value`, `key!=value` or `key~substring`")
        })?;
        let raw_key = s.get(..pos).unwrap_or_default();
        let value = s.get(pos + 1..).unwrap_or_default().to_string();
        let tilde = s.get(pos..pos + 1) == Some("~");
        let (key, op) = match (tilde, raw_key.strip_suffix('!')) {
            (true, _) => (raw_key, Op::Has),
            (false, Some(negated)) => (negated, Op::Ne),
            (false, None) => (raw_key, Op::Eq),
        };
        if key.is_empty() {
            bail!("empty key in clause {s:?}");
        }
        Ok(Self {
            key: key.to_string(),
            op,
            value,
        })
    }

    fn matches(&self, record: &Value) -> bool {
        let field = record.as_object().and_then(|obj| obj.get(&self.key));
        match (&self.op, field.map(fragment)) {
            (Op::Eq, Some(f)) => f == self.value,
            (Op::Ne, Some(f)) => f != self.value,
            (Op::Has, Some(f)) => f.contains(&self.value),
            // A missing field equals nothing — and so differs from everything.
            (Op::Eq | Op::Has, None) => false,
            (Op::Ne, None) => true,
        }
    }
}

/// A field's comparison form: strings by their content (no quotes),
/// everything else by compact JSON text (`42`, `true`, `null`, nested
/// values as their JSON).
fn fragment(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// The selected slice: serialized survivors plus exact accounting —
/// dropping records is the point, so kept/total must never be guessed.
#[derive(Debug)]
pub struct Slice {
    /// Compact JSON array of the surviving records.
    pub body: String,
    pub kept: usize,
    pub total: usize,
}

/// Parse `doc`, walk down `path` (dotted keys; empty = the document root
/// is already the array), filter with `clauses`.
pub fn slice(doc: &str, path: &str, clauses: &[Clause]) -> Result<Slice> {
    // Windows tooling loves a UTF-8 BOM, which serde_json rejects.
    let doc = doc.strip_prefix('\u{feff}').unwrap_or(doc);
    let root: Value = serde_json::from_str(doc).context("parsing input JSON")?;
    let mut node = &root;
    for step in path.split('.').filter(|s| !s.is_empty()) {
        // An all-digit step on an array is an index (SARIF: `runs.0.results`);
        // objects always win by key, so `{"0": …}` stays addressable.
        let next = match (node, step.parse::<usize>()) {
            (Value::Array(items), Ok(idx)) => items.get(idx),
            _ => node.get(step),
        };
        node = next.with_context(|| format!("no key {step:?} walking down {path:?}"))?;
    }
    let records = node.as_array().with_context(|| {
        format!(
            "value at {:?} is {}, not an array of records",
            if path.is_empty() { "<root>" } else { path },
            kind_name(node),
        )
    })?;
    let survivors: Vec<&Value> = records
        .iter()
        .filter(|r| clauses.iter().all(|c| c.matches(r)))
        .collect();
    let body = serde_json::to_string(&survivors).context("serializing slice")?;
    Ok(Slice {
        body,
        kept: survivors.len(),
        total: records.len(),
    })
}

fn kind_name(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "a bool",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Array(_) => "an array",
        Value::Object(_) => "an object",
    }
}
