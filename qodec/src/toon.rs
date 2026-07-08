//! `toon` — tabular re-encoding of uniform JSON arrays.
//!
//! `[{"a":1,"b":"x"}, {"a":2,"b":"y"}, ...]` repeats every key N times; a
//! table says the keys once and streams rows. Scope is deliberately narrow
//! and honest: top-level array, ≥2 objects, identical key sets, primitive
//! values only — anything else falls back to `raw`.
//!
//! Cells are compact JSON fragments (strings keep quotes/escapes), so a cell
//! can never contain a raw newline. The column separator is *probed*: the
//! first candidate absent from every fragment wins; separators are named
//! symbolically in the header to keep it parseable.
//!
//! Roundtrip is **semantic**: decode returns canonical compact JSON that is
//! `serde_json::Value`-equal to the input (whitespace and key order are not
//! preserved — BTreeMap key order is canonical).

use anyhow::{bail, Context, Result};
use serde_json::Value;

use crate::container::{self, Container};
use crate::meter::TokenMeter;

const SEPARATORS: &[(&str, char)] = &[("pipe", '|'), ("tab", '\t'), ("broke", '¦'), ("dot", '·')];

pub fn encode(text: &str, meter: &dyn TokenMeter) -> String {
    match try_encode(text, meter) {
        Some(encoded) => encoded,
        None => container::raw(text),
    }
}

fn try_encode(text: &str, meter: &dyn TokenMeter) -> Option<String> {
    let value: Value = serde_json::from_str(text).ok()?;
    let rows = value.as_array()?;
    if rows.len() < 2 {
        return None;
    }

    let mut keys: Option<Vec<&String>> = None;
    for row in rows {
        let obj = row.as_object()?;
        if obj.values().any(|v| v.is_object() || v.is_array()) {
            return None;
        }
        let row_keys: Vec<&String> = obj.keys().collect();
        match &keys {
            None => keys = Some(row_keys),
            Some(k) if *k == row_keys => {}
            Some(_) => return None,
        }
    }
    let keys = keys?;

    let mut fragments: Vec<Vec<String>> = Vec::with_capacity(rows.len());
    for row in rows {
        let obj = row.as_object()?;
        let mut cells = Vec::with_capacity(keys.len());
        for key in &keys {
            cells.push(serde_json::to_string(obj.get(*key)?).ok()?);
        }
        fragments.push(cells);
    }

    let (sep_name, sep) = SEPARATORS
        .iter()
        .find(|(_, ch)| fragments.iter().flatten().all(|f| !f.contains(*ch)))
        .copied()?;

    let keys_line = serde_json::to_string(&keys).ok()?;
    let mut body = String::new();
    body.push_str(&keys_line);
    body.push('\n');
    for cells in &fragments {
        body.push_str(&cells.join(&sep.to_string()));
        body.push('\n');
    }

    let encoded = container::emit(&Container {
        codec: "toon".to_string(),
        params: vec![
            ("sep".to_string(), sep_name.to_string()),
            ("rows".to_string(), fragments.len().to_string()),
        ],
        legend: Vec::new(),
        body,
    });
    (meter.count(&encoded) < meter.count(text)).then_some(encoded)
}

pub fn decode(c: &Container) -> Result<String> {
    let sep_name = c.param("sep").context("toon container missing sep param")?;
    let &(_, sep) = SEPARATORS
        .iter()
        .find(|(name, _)| *name == sep_name)
        .with_context(|| format!("unknown toon separator {sep_name:?}"))?;

    let mut lines = c.body.split('\n');
    let keys_line = lines.next().context("toon body missing keys line")?;
    let keys: Vec<String> = serde_json::from_str(keys_line).context("parsing toon keys line")?;

    let mut rows: Vec<Value> = Vec::new();
    for line in lines {
        if line.is_empty() {
            continue;
        }
        let cells: Vec<&str> = line.split(sep).collect();
        if cells.len() != keys.len() {
            bail!(
                "toon row has {} cells, expected {}: {line:?}",
                cells.len(),
                keys.len()
            );
        }
        let mut obj = serde_json::Map::new();
        for (key, cell) in keys.iter().zip(cells) {
            let cell_value: Value =
                serde_json::from_str(cell).with_context(|| format!("parsing cell {cell:?}"))?;
            obj.insert(key.clone(), cell_value);
        }
        rows.push(Value::Object(obj));
    }
    serde_json::to_string(&Value::Array(rows)).context("serializing decoded toon")
}
