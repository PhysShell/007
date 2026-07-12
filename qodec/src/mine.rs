//! `mine` — token-aware dictionary miner. The heart of the lab.
//!
//! LZ78 in spirit, but the cost function is the *live tokenizer*, not bytes:
//! repeated exact spans are replaced by probed aliases, and a span is only
//! committed when the measured whole-text token count actually drops by more
//! than the legend line it adds. Gain is measured, never modeled — BPE merge
//! boundaries make byte-level estimates lie.
//!
//! Losslessness argument: alias chars are absent from the original input
//! (checked at pool build), pool entries are unique, and sigil aliases use
//! fixed-width indices so no alias is a prefix of another. Therefore at each
//! encode step k the alias `a_k` cannot already occur in the text, and
//! `replace(a_k -> phrase_k)` exactly inverts `replace(phrase_k -> a_k)`.
//! Decoding in reverse commit order then inverts the whole sequence — which
//! also makes *nested* dictionary entries legal: a later phrase may contain
//! an earlier alias, and it expands on a later decode pass.

use std::collections::HashMap;

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
/// How many unused pool glyphs to probe in the committed phrase's context.
const GLYPH_PROBE_K: usize = 8;

/// Candidate discovery strategy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MinerKind {
    /// Word-boundary n-grams + separator-aligned prefixes. Fast, misses
    /// repeats that straddle boundaries.
    Words,
    /// Word candidates ∪ suffix-automaton candidates (`sam.rs`), half the
    /// probe budget each. Pure SAM ranking drowns the budget in nested
    /// variants of one giant repeat (measured: stack traces fell from +18.8%
    /// to +6.8% cold); the union keeps word-tally diversity *and* the
    /// boundary-straddling repeats only the automaton can see. CPU-heavier —
    /// the classic BWT-lineage trade.
    Deep,
}

pub struct MineOptions {
    pub alphabet: Alphabet,
    /// A dictionary entry is committed only when measured net gain (tokens)
    /// exceeds this.
    pub min_gain: i64,
    pub max_entries: usize,
    pub miner: MinerKind,
    /// Profile-learned phrases, probed ahead of discovery each round.
    /// Seeds only reorder the queue — every commit still passes the same
    /// measured acceptance, so a stale seed wastes a probe, never bytes.
    pub seeds: Vec<String>,
    /// Trained probe ranker (`qodec train`). When present, candidates come
    /// from a wider pool re-ranked by predicted gain instead of the
    /// count×len heuristic. Ordering only — acceptance stays measured.
    pub ranker: Option<crate::rank::Ranker>,
    /// Measured probes per round. The knob the ranker earns its keep on:
    /// a good ranking keeps the ratio at a fraction of the probes.
    pub probe_budget: usize,
    /// Eval-only lexical guard (the `squeeze-guarded` / GF ablation arm): when
    /// set, candidate phrases that span a guarded lexical class (paths, code
    /// spans, `::`/snake/Camel identifiers, grep markers) are never aliased.
    /// A diagnostic global guard recognised without task/gold — NOT protected
    /// spans. Default false, so `mine`/`squeeze` are unchanged.
    pub guard_lexical: bool,
}

/// Generic lexical classes GF never lets the miner alias — recognised purely
/// from surface form, with no knowledge of the task or gold answer. Purity of
/// the factor matters more than compression ratio, so this over-guards rather
/// than risk hiding an identifier inside a glyph.
pub fn is_guarded_lexical(phrase: &str) -> bool {
    if phrase.contains('`') || phrase.contains('»') || phrase.contains("::") || phrase.contains('/') {
        return true; // backtick span, grep marker, `::` path, or any path separator
    }
    const EXTS: [&str; 10] =
        [".rs", ".cs", ".md", ".toml", ".json", ".lock", ".jinja", ".py", ".txt", ".yaml"];
    if EXTS.iter().any(|e| phrase.contains(e)) {
        return true; // filename.extension token
    }
    let chars: Vec<char> = phrase.chars().collect();
    for i in 1..chars.len() {
        // camel / pascal hump: a lowercase immediately followed by an uppercase
        if chars[i].is_ascii_uppercase() && chars[i - 1].is_ascii_lowercase() {
            return true;
        }
        // snake_case: alnum '_' alnum
        if chars[i] == '_'
            && chars[i - 1].is_ascii_alphanumeric()
            && i + 1 < chars.len()
            && chars[i + 1].is_ascii_alphanumeric()
        {
            return true;
        }
    }
    false
}

impl Default for MineOptions {
    fn default() -> Self {
        Self {
            alphabet: Alphabet::Auto,
            min_gain: 0,
            max_entries: 64,
            miner: MinerKind::Words,
            seeds: Vec::new(),
            ranker: None,
            probe_budget: SCORE_BUDGET,
            guard_lexical: false,
        }
    }
}

pub fn encode(text: &str, meter: &dyn TokenMeter, opts: &MineOptions) -> String {
    if text.is_empty() {
        return container::raw(text);
    }

    let mut pool = AliasPool::build(opts.alphabet, meter, text);
    let mut legend: Vec<(String, String)> = Vec::new();
    let mut current = text.to_string();
    let mut current_tokens = meter.count(&current) as i64;

    while legend.len() < opts.max_entries {
        // Candidates are ranked with a provisional glyph; the committed
        // phrase then gets the glyph that tokenizes cheapest in its real
        // context — standalone glyph cost lies mid-row (PR #34).
        let Some((alias, _alias_cost)) = pool.peek() else {
            break;
        };

        let mut best: Option<(String, String, i64)> = None; // (phrase, replaced, gain)
        let mut queue: Vec<String> = opts
            .seeds
            .iter()
            .filter(|s| !s.is_empty() && current.contains(s.as_str()))
            .cloned()
            .collect();
        for candidate in probe_queue(&current, opts) {
            if !queue.contains(&candidate) {
                queue.push(candidate);
            }
        }
        for phrase in queue {
            if opts.guard_lexical && is_guarded_lexical(&phrase) {
                continue; // GF: never alias a guarded lexical span
            }
            let replaced = current.replace(&phrase, &alias);
            let legend_line = format!("{alias}={phrase}\n");
            let gain =
                current_tokens - meter.count(&replaced) as i64 - meter.count(&legend_line) as i64;
            if best.as_ref().is_none_or(|(_, _, g)| gain > *g) {
                best = Some((phrase, replaced, gain));
            }
        }

        match best {
            Some((phrase, _, gain)) if gain > opts.min_gain => {
                let (before, after) = local_context(&current, &phrase);
                let Some((alias, _)) = pool.take_best_for(meter, before, after, GLYPH_PROBE_K)
                else {
                    break;
                };
                // The commit decision re-measures exactly with the chosen
                // glyph — the provisional estimate never decides alone.
                let replaced = current.replace(&phrase, &alias);
                let legend_line = format!("{alias}={phrase}\n");
                let gain = current_tokens
                    - meter.count(&replaced) as i64
                    - meter.count(&legend_line) as i64;
                if gain <= opts.min_gain {
                    break;
                }
                current_tokens = meter.count(&replaced) as i64;
                current = replaced;
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

/// The probe queue for one round: repeated spans ranked either by the
/// cheap count×len proxy, or — with a trained ranker — by predicted gain
/// over a 4× wider pool, truncated to the probe budget. Occurrence counts
/// are approximate (per-line tallies); the miner's commit decision
/// re-measures the real replacement, so a bad rank only wastes a probe.
///
/// Two candidate families:
/// * word-boundary n-grams (exact slices between word starts/ends);
/// * **segment prefixes inside words** — paths, namespaces and identifiers
///   repeat as *prefixes* (`src/a/b/One.cs` vs `src/a/b/Two.cs`), which whole
///   words never expose. This is the cheap end of the BWT/suffix-array
///   insight: repetition ignores token boundaries, so the miner must too.
fn probe_queue(text: &str, opts: &MineOptions) -> Vec<String> {
    let budget = opts.probe_budget.max(1);
    let pool_size = if opts.ranker.is_some() {
        budget.saturating_mul(4)
    } else {
        budget
    };
    let mut pool = candidate_pool(text, opts.miner, pool_size);
    if let Some(ranker) = &opts.ranker {
        // Predicted-gain order; ties break on the candidate bytes so the
        // queue is deterministic regardless of pool iteration order.
        let mut scored: Vec<(f64, String)> = pool
            .drain(..)
            .map(|(phrase, count)| {
                let score = ranker.score(&crate::rank::features(&phrase, count));
                (score, phrase)
            })
            .collect();
        scored.sort_by(|a, b| b.0.total_cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
        return scored
            .into_iter()
            .take(budget)
            .map(|(_, phrase)| phrase)
            .collect();
    }
    pool.truncate(budget);
    pool.into_iter().map(|(phrase, _)| phrase).collect()
}

/// Candidates with occurrence estimates, heuristic-ranked, deduplicated.
fn candidate_pool(text: &str, miner: MinerKind, want: usize) -> Vec<(String, usize)> {
    if miner == MinerKind::Deep {
        // Each family keeps at least one candidate — an odd or tiny budget
        // must never starve the round to zero probes (Codex, PR #38). The
        // callers truncate to the real budget after merging.
        let half = want.div_ceil(2);
        let mut merged = word_candidates_counted(text, half);
        for c in crate::sam::repeated_substrings(
            text,
            MIN_CANDIDATE_CHARS,
            MAX_CANDIDATE_CHARS,
            half,
        ) {
            if !merged.iter().any(|(t, _)| *t == c.text) {
                merged.push((c.text, c.count));
            }
        }
        return merged;
    }
    word_candidates_counted(text, want)
}

/// The word/prefix candidate family, exposed for `qodec learn`: the same
/// spans the miner would probe are what a profile should remember.
pub(crate) fn learn_phrases(text: &str, budget: usize) -> Vec<String> {
    word_candidates_counted(text, budget)
        .into_iter()
        .map(|(phrase, _)| phrase)
        .collect()
}

/// One training pass for the probe ranker (`qodec train`): measure the
/// real first-round gain of every pool candidate against the pristine
/// text and record (features, gain) into the stats. Uses a provisional
/// alias exactly like the miner's ranking probes, and draws candidates
/// from the *deep* pool — words ∪ SAM — so the model learns the full
/// distribution it will later rank, not just the word family
/// (CodeRabbit, PR #38).
pub fn train_pass(
    text: &str,
    meter: &dyn TokenMeter,
    stats: &mut crate::rank::Stats,
    budget: usize,
) -> usize {
    if text.is_empty() {
        return 0;
    }
    let pool = AliasPool::build(Alphabet::Auto, meter, text);
    let Some((alias, _)) = pool.peek() else {
        return 0;
    };
    let base = meter.count(text) as i64;
    let mut observed = 0usize;
    for (phrase, count) in candidate_pool(text, MinerKind::Deep, budget) {
        let replaced = text.replace(&phrase, &alias);
        let legend_line = format!("{alias}={phrase}\n");
        let gain = base - meter.count(&replaced) as i64 - meter.count(&legend_line) as i64;
        stats.observe(&crate::rank::features(&phrase, count), gain as f64);
        observed += 1;
    }
    observed
}

/// Same-line context around the first occurrence of `phrase` — the window
/// a glyph is probed in. BPE merges are local, so the line neighborhood is
/// the honest proxy; the commit itself still re-measures the whole text.
fn local_context<'a>(text: &'a str, phrase: &str) -> (&'a str, &'a str) {
    const WIN: usize = 12;
    let Some(pos) = text.find(phrase) else {
        return ("", "");
    };
    let before_all = text.get(..pos).unwrap_or_default();
    let after_all = text.get(pos + phrase.len()..).unwrap_or_default();
    let before_line = before_all
        .rfind('\n')
        .map_or(before_all, |i| before_all.get(i + 1..).unwrap_or_default());
    let after_line = after_all
        .find('\n')
        .map_or(after_all, |i| after_all.get(..i).unwrap_or_default());
    (tail_chars(before_line, WIN), head_chars(after_line, WIN))
}

fn head_chars(s: &str, n: usize) -> &str {
    match s.char_indices().nth(n) {
        Some((idx, _)) => s.get(..idx).unwrap_or(s),
        None => s,
    }
}

fn tail_chars(s: &str, n: usize) -> &str {
    let total = s.chars().count();
    if total <= n {
        return s;
    }
    match s.char_indices().nth(total - n) {
        Some((idx, _)) => s.get(idx..).unwrap_or(s),
        None => s,
    }
}

fn word_candidates_counted(text: &str, budget: usize) -> Vec<(String, usize)> {
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
                if i == j {
                    for prefix in segment_prefixes(span) {
                        *tally.entry(prefix).or_insert(0) += 1;
                    }
                }
            }
        }
    }

    let mut ranked: Vec<(&str, usize)> =
        tally.into_iter().filter(|(_, count)| *count >= 2).collect();
    ranked.sort_by(|a, b| (b.1 * b.0.len(), b.0).cmp(&(a.1 * a.0.len(), a.0)));
    ranked
        .into_iter()
        .take(budget)
        .map(|(span, count)| (span.to_string(), count))
        .collect()
}

/// Prefixes of a word that end right after a separator (`/`, `\`, `.`, `:`),
/// e.g. `rust/src/lib.rs:12:` → `rust/`, `rust/src/`, `rust/src/lib.`, ….
/// These are exactly the shared-prefix repeats that whole-word candidates
/// miss on file listings, greps, stack frames and namespaces.
fn segment_prefixes(word: &str) -> impl Iterator<Item = &str> {
    word.char_indices()
        .filter(|(_, ch)| matches!(ch, '/' | '\\' | '.' | ':'))
        .filter_map(move |(idx, ch)| word.get(..idx + ch.len_utf8()))
        .filter(|p| p.len() >= MIN_CANDIDATE_CHARS && p.len() <= MAX_CANDIDATE_CHARS)
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
