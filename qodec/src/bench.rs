//! Corpus benchmark: every codec × every sample, measured and verified.
//!
//! Two savings figures per row:
//! * **cold** — the whole encoded artifact (header + legend + body) vs the
//!   original. What you pay when the legend travels inside the message.
//! * **warm** — body only. What you pay when the legend/key lives in a stable
//!   cached prompt prefix (CLAUDE.md, system prompt) and is amortized.
//!
//! Every row is roundtripped: `byte` = exact bytes back, `sem` = JSON
//! Value-equality (toon's canonical form), `FAIL` = a bug worth a test case.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde_json::Value;

use crate::alias::Alphabet;
use crate::container;
use crate::meter::TokenMeter;
use crate::{decode, encode, CodecKind};

pub struct BenchRow {
    pub sample: String,
    pub codec: &'static str,
    pub outcome: String,
    pub bytes_in: usize,
    pub bytes_out: usize,
    pub tokens_in: usize,
    pub tokens_cold: usize,
    pub tokens_warm: usize,
    pub roundtrip: &'static str,
}

pub fn run(corpus: &Path, meter: &dyn TokenMeter, alphabet: Alphabet) -> Result<Vec<BenchRow>> {
    let mut files: Vec<_> = fs::read_dir(corpus)
        .with_context(|| format!("reading corpus dir {}", corpus.display()))?
        .filter_map(std::result::Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_file())
        .collect();
    files.sort();

    let mut rows = Vec::new();
    for path in files {
        let text =
            fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
        let sample = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let tokens_in = meter.count(&text);

        for kind in [
            CodecKind::Fold,
            CodecKind::Toon,
            CodecKind::Mine,
            CodecKind::Squeeze,
        ] {
            let encoded = encode(&text, kind, meter, alphabet);
            let tokens_cold = meter.count(&encoded);
            let tokens_warm = tokens_cold.saturating_sub(container::overhead(&encoded, meter));
            let outcome = container::parse(&encoded)
                .map(|c| c.codec)
                .unwrap_or_else(|_| "?".to_string());

            let roundtrip = match decode(&encoded) {
                Ok(back) if back == text => "byte",
                Ok(back) if json_equal(&back, &text) => "sem",
                Ok(_) => "FAIL",
                Err(_) => "FAIL",
            };

            rows.push(BenchRow {
                sample: sample.clone(),
                codec: kind.label(),
                outcome,
                bytes_in: text.len(),
                bytes_out: encoded.len(),
                tokens_in,
                tokens_cold,
                tokens_warm,
                roundtrip,
            });
        }
    }
    Ok(rows)
}

fn json_equal(a: &str, b: &str) -> bool {
    match (
        serde_json::from_str::<Value>(a),
        serde_json::from_str::<Value>(b),
    ) {
        (Ok(va), Ok(vb)) => va == vb,
        _ => false,
    }
}

pub fn markdown(rows: &[BenchRow], meter_name: &str, alphabet: &str) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "meter: `{meter_name}`  alphabet: `{alphabet}`  \
         cold = full artifact, warm = body only (legend amortized in cached prefix)\n\n"
    ));
    out.push_str(
        "| sample | codec | result | tok in | tok cold | cold Δ | tok warm | warm Δ | roundtrip |\n\
         |---|---|---|---:|---:|---:|---:|---:|---|\n",
    );
    for r in rows {
        let pct = |after: usize| -> String {
            if r.tokens_in == 0 {
                return "-".to_string();
            }
            let delta = 100.0 * (r.tokens_in as f64 - after as f64) / r.tokens_in as f64;
            format!("{delta:+.1}%")
        };
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} | {} | {} | {} |\n",
            r.sample,
            r.codec,
            r.outcome,
            r.tokens_in,
            r.tokens_cold,
            pct(r.tokens_cold),
            r.tokens_warm,
            pct(r.tokens_warm),
            r.roundtrip,
        ));
    }
    out
}
