//! `mine` — token-aware dictionary miner. The heart of the lab.
//!
//! LZ78 in spirit, but the cost function is the *live tokenizer*, not bytes:
//! repeated exact spans are replaced by probed aliases, and a span is only
//! committed when the measured whole-text token count actually drops by more
//! than the legend line it adds. Gain is measured, never modeled — BPE merge
//! boundaries make byte-level estimates lie.
//!
//! Losslessness argument: aliases are built from chars absent from the input
//! (checked at pool build) and disjoint from each other (reserved set), and a
//! later candidate never contains an earlier alias's chars. Decoding replaces
//! aliases back in reverse commit order, which exactly inverts the sequence
//! of `str::replace` calls.

use std::collections::HashMap;
use std::collections::HashSet;

use anyhow::{bail, Result};

use crate::alias::{AliasPool, Alphabet};
use crate::container::{self, Container};
use crate::meter::TokenMeter;

/// Candidate spans start/end on word boundaries within a single line, so the
/// exact slice (including inner whitespace) is preserved.
const MAX_WORDS: usize = 8;
const MIN_CANDIDATE_CHARS: usize = 6;
const MAX_CANDIDATE_CHARS: usize = 200;
/// Exact-measure at most this many top-ranked candidates per round.
const SCORE_BUDGET: usize = 40;

pub struct MineOptions {
    pub alphabet: Alphabet,
    /// A dictionary entry is committed only when measured net gain (tokens)
    /// exceeds this.
    pub min_gain: i64,
    pub max_entries: usize,
}

impl Default for MineOptions {
    fn default() -> Self {
        Self {
            alphabet: Alphabet::Auto,
            min_gain: 0,
            max_entries: 64,
        }
    }
}

pub fn encode(text: &str, meter: &dyn TokenMeter, opts: &MineOptions) -> String {
    if text.is_empty() {
        return container::raw(text);
    }

    let mut pool = AliasPool::build(opts.alphabet, meter, text);
    let mut reserved: HashSet<char> = HashSet::new();
    let mut legend: Vec<(String, String)> = Vec::new();
    let mut current = text.to_string();
    let mut current_tokens = meter.count(&current) as i64;

    while legend.len() < opts.max_entries {
        let Some((alias, _alias_cost)) = pool.take(&reserved) else {
            break;
        };

        let mut best: Option<(String, String, i64)> = None; // (phrase, replaced, gain)
        for phrase in ranked_candidates(&current, &reserved) {
            let replaced = current.replace(&phrase, &alias);
            let legend_line = format!("{alias}={phrase}\n");
            let gain =
                current_tokens - meter.count(&replaced) as i64 - meter.count(&legend_line) as i64;
            if best.as_ref().is_none_or(|(_, _, g)| gain > *g) {
                best = Some((phrase, replaced, gain));
            }
        }

        match best {
            Some((phrase, replaced, gain)) if gain > opts.min_gain => {
                current_tokens = meter.count(&replaced) as i64;
                current = replaced;
                reserved.extend(alias.chars());
                legend.push((alias, phrase));
            }
            _ => break,
        }
    }

    if legend.is_empty() {
        return container::raw(text);
    }

    let encoded = container::emit(&Container {
        codec: "mine".to_string(),
        params: vec![("n".to_string(), legend.len().to_string())],
        legend: legend.iter().map(|(a, p)| format!("{a}={p}")).collect(),
        body: current,
    });

    // Global acceptance: the whole artifact must beat the original.
    if meter.count(&encoded) >= meter.count(text) {
        return container::raw(text);
    }
    encoded
}

pub fn decode(c: &Container) -> Result<String> {
    let mut entries: Vec<(&str, &str)> = Vec::with_capacity(c.legend.len());
    for line in &c.legend {
        let Some((alias, phrase)) = line.split_once('=') else {
            bail!("malformed mine legend line {line:?}");
        };
        entries.push((alias, phrase));
    }
    let mut out = c.body.clone();
    for (alias, phrase) in entries.iter().rev() {
        out = out.replace(alias, phrase);
    }
    Ok(out)
}

/// Collect repeated word-boundary spans, rank by a cheap (count × len) proxy,
/// return up to `SCORE_BUDGET` for exact measurement. Occurrence counts here
/// are approximate (per-line n-gram tallies); the miner's commit decision
/// re-measures the real replacement, so a bad rank only wastes a probe.
fn ranked_candidates(text: &str, reserved: &HashSet<char>) -> Vec<String> {
    let mut tally: HashMap<&str, usize> = HashMap::new();

    for line in text.split('\n') {
        let words = word_spans(line);
        for i in 0..words.len() {
            let Some(&(start, _)) = words.get(i) else {
                continue;
            };
            for j in i..words.len().min(i + MAX_WORDS) {
                let Some(&(_, end)) = words.get(j) else {
                    continue;
                };
                let Some(span) = line.get(start..end) else {
                    continue;
                };
                if span.len() < MIN_CANDIDATE_CHARS {
                    continue;
                }
                if span.len() > MAX_CANDIDATE_CHARS {
                    break;
                }
                *tally.entry(span).or_insert(0) += 1;
            }
        }
    }

    let mut ranked: Vec<(&str, usize)> = tally
        .into_iter()
        .filter(|(span, count)| *count >= 2 && !span.chars().any(|ch| reserved.contains(&ch)))
        .collect();
    ranked.sort_by(|a, b| (b.1 * b.0.len(), b.0).cmp(&(a.1 * a.0.len(), a.0)));
    ranked
        .into_iter()
        .take(SCORE_BUDGET)
        .map(|(span, _)| span.to_string())
        .collect()
}

/// Byte ranges of whitespace-delimited words (char-boundary safe).
fn word_spans(line: &str) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut start: Option<usize> = None;
    for (idx, ch) in line.char_indices() {
        if ch.is_whitespace() {
            if let Some(s) = start.take() {
                spans.push((s, idx));
            }
        } else if start.is_none() {
            start = Some(idx);
        }
    }
    if let Some(s) = start {
        spans.push((s, line.len()));
    }
    spans
}
