//! The `%q1` container — a self-describing envelope whose header block *is*
//! the decryption key.
//!
//! ```text
//! %q1 mine n=3 nl=1          <- header: codec + params
//! 码=src/Legacy.UI/ViewModels/  <- legend lines (the key material)
//! §0=System.NullReferenceException
//! %q1 body                   <- boundary
//! ...encoded body, verbatim to EOF...
//! ```
//!
//! Design constraints, all load-bearing:
//! * ASCII markers only — `%q1` costs ~2 tokens; fancy brackets like `⟦` cost 3+.
//! * Legend lines always contain `=` before any bare `body` word, so they can
//!   never be confused with the boundary line.
//! * The body is read verbatim to EOF; no body escaping is ever needed here
//!   (codecs that emit line markers into the body do their own escaping).
//! * Decoding is a total function over well-formed containers: a `raw`
//!   container carries any input unchanged, so `encode` can always fall back
//!   without breaking `decode`.

use anyhow::{bail, Context, Result};

pub const MAGIC: &str = "%q1";

#[derive(Debug, Clone)]
pub struct Container {
    pub codec: String,
    pub params: Vec<(String, String)>,
    pub legend: Vec<String>,
    pub body: String,
}

impl Container {
    pub fn param(&self, key: &str) -> Option<&str> {
        self.params
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }
}

pub fn emit(c: &Container) -> String {
    let mut out = String::new();
    out.push_str(MAGIC);
    out.push(' ');
    out.push_str(&c.codec);
    for (k, v) in &c.params {
        out.push(' ');
        out.push_str(k);
        out.push('=');
        out.push_str(v);
    }
    out.push('\n');
    for line in &c.legend {
        out.push_str(line);
        out.push('\n');
    }
    out.push_str(MAGIC);
    out.push_str(" body\n");
    out.push_str(&c.body);
    out
}

pub fn is_container(text: &str) -> bool {
    parse(text).is_ok()
}

pub fn parse(text: &str) -> Result<Container> {
    let boundary = format!("{MAGIC} body");
    let mut offset = 0usize;
    let mut header: Option<(String, Vec<(String, String)>)> = None;
    let mut legend: Vec<String> = Vec::new();

    for line in text.split_inclusive('\n') {
        let start = offset;
        offset += line.len();
        let trimmed = line.strip_suffix('\n').unwrap_or(line);
        // Tolerate CRLF-converted artifacts (git autocrlf, clipboards):
        // emit never writes `\r`, and no legend value can end with one, so
        // stripping it only rescues externally-converted containers.
        let trimmed = trimmed.strip_suffix('\r').unwrap_or(trimmed);

        if header.is_none() {
            let rest = trimmed
                .strip_prefix(MAGIC)
                .and_then(|r| r.strip_prefix(' '))
                .with_context(|| format!("not a {MAGIC} container (bad first line)"))?;
            let mut words = rest.split(' ');
            let codec = words.next().unwrap_or_default().to_string();
            if codec.is_empty() || codec == "body" {
                bail!("not a {MAGIC} container (missing codec name)");
            }
            let mut params = Vec::new();
            for w in words {
                let Some((k, v)) = w.split_once('=') else {
                    bail!("malformed header param {w:?}");
                };
                params.push((k.to_string(), v.to_string()));
            }
            header = Some((codec, params));
            continue;
        }

        if trimmed == boundary {
            let body_start = start + line.len();
            let body = text.get(body_start..).unwrap_or_default().to_string();
            let (codec, params) = header.unwrap_or_default();
            return Ok(Container {
                codec,
                params,
                legend,
                body,
            });
        }
        legend.push(trimmed.to_string());
    }
    bail!("unterminated {MAGIC} container (no `{boundary}` line)")
}

/// Wrap arbitrary text unchanged — the universal fallback.
pub fn raw(text: &str) -> String {
    emit(&Container {
        codec: "raw".to_string(),
        params: Vec::new(),
        legend: Vec::new(),
        body: text.to_string(),
    })
}

/// Tokens spent on header + legend (everything before the body). This is the
/// part a stable "decryption key" can amortize into a cached prompt prefix.
pub fn overhead(text: &str, meter: &dyn crate::meter::TokenMeter) -> usize {
    match parse(text) {
        Ok(c) => {
            let body_len = c.body.len();
            let head = text
                .get(..text.len().saturating_sub(body_len))
                .unwrap_or(text);
            meter.count(head)
        }
        Err(_) => 0,
    }
}
