//! `rules` — the propose/verify loop's substrate: parametric span rules an
//! LLM proposes and the lab verifies by measurement.
//!
//! The niche neither miner covers: a phrase-with-holes that repeats
//! *across different line shapes*. `mine` needs exact literals (the hole
//! breaks it); `tmpl` clusters whole lines by seg shape (different shapes
//! never meet). A rule is a glob template — fixed parts, single-word
//! wildcards between them — applied to spans *inside* lines, anywhere it
//! matches. The loop:
//!
//! 1. `qodec residual` emits a brief of what the codecs left on the table;
//! 2. a proposer (any LLM — out of band, never trusted) drafts rules;
//! 3. `qodec rules verify` keeps only rules that invert byte-exactly and
//!    measure a strict token win on real files;
//! 4. `encode --rules` applies survivors as a pre-pass; the artifact pins
//!    the key file by checksum and decode fails closed without it.
//!
//! A wrong or hostile proposal can therefore waste verification probes,
//! never bytes — the same contract as every other seed in this lab.

use std::path::Path;

use anyhow::{bail, Context, Result};

use crate::container::{self, Container};
use crate::meter::TokenMeter;

const HEADER: &str = "# qodec rules v1";

/// Span delimiters probed against the input: a used rule's occurrence is
/// rewritten to `START alias (SEP value)* END`, so all three glyphs must
/// be absent from the text (and from each other's roles by construction).
const STARTS: &[(&str, char)] = &[("lceil", '⌈'), ("lang", '⟨'), ("lguil", '«')];
const ENDS: &[(&str, char)] = &[("rceil", '⌉'), ("rang", '⟩'), ("rguil", '»')];
const SEPS: &[(&str, char)] = &[("pipe", '|'), ("tab", '\t'), ("broke", '¦'), ("dot", '·')];

/// A parsed rules key: ordered `alias=glob-template` entries plus the
/// checksum of the exact file bytes.
#[derive(Debug)]
pub struct RulesKey {
    pub slot: char,
    /// alias -> fixed parts; single-word wildcards between parts.
    pub entries: Vec<(String, Vec<String>)>,
    pub sum: String,
}

impl RulesKey {
    pub fn load(path: &Path) -> Result<Self> {
        let text =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        Self::parse(&text).with_context(|| format!("parsing {}", path.display()))
    }

    pub fn parse(text: &str) -> Result<Self> {
        let mut lines = text.lines();
        let header = lines.next().map(str::trim).unwrap_or_default();
        let Some(rest) = header.strip_prefix(HEADER) else {
            bail!("not a rules key (first line must start with {HEADER:?})");
        };
        let slot_name = rest
            .trim()
            .strip_prefix("slot=")
            .context("rules header missing slot=<name>")?;
        let &(_, slot) = crate::tmpl::SLOTS
            .iter()
            .find(|(name, _)| *name == slot_name)
            .with_context(|| format!("unknown slot name {slot_name:?} in rules key"))?;
        let mut entries: Vec<(String, Vec<String>)> = Vec::new();
        for line in lines {
            let line = line.trim_end_matches('\r');
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((alias, template)) = line.split_once('=') else {
                bail!("malformed rules line {line:?}");
            };
            if alias.is_empty() || template.is_empty() {
                bail!("empty alias or template in rules line {line:?}");
            }
            // The same trust-boundary rules as the legends: a used alias
            // becomes a container header token, and the flat `used` list
            // is a concatenation — it only tokenizes unambiguously when no
            // alias is a substring of another (duplicates included), so
            // refuse overlaps here where hand-edited keys enter
            // (CodeRabbit, PR #39).
            if alias.chars().any(char::is_whitespace) {
                bail!("alias {alias:?} must not contain whitespace");
            }
            if entries
                .iter()
                .any(|(a, _)| a.contains(alias) || alias.contains(a.as_str()))
            {
                bail!("alias {alias:?} overlaps another alias in the rules key");
            }
            let parts: Vec<String> = template.split(slot).map(str::to_string).collect();
            if parts.iter().any(|p| p.contains('\n') || p.contains('\r')) {
                bail!("rule {alias:?} template must stay on one line");
            }
            // An unanchored wildcard edge (empty first/last part) would
            // make span boundaries ambiguous mid-line; require literal
            // anchors on both ends.
            if parts.first().is_none_or(|p| p.is_empty())
                || parts.last().is_none_or(|p| p.is_empty())
            {
                bail!("rule {alias:?} must start and end with fixed text");
            }
            entries.push((alias.to_string(), parts));
        }
        if entries.is_empty() {
            bail!("rules key has no entries");
        }
        Ok(Self {
            slot,
            sum: fnv1a(text.as_bytes()),
            entries,
        })
    }
}

/// One glob occurrence inside a line: the matched span and its values.
struct SpanMatch<'a> {
    start: usize,
    end: usize,
    values: Vec<&'a str>,
}

/// The next char boundary after `start` — a failed anchor must advance by
/// a whole character, or `line.get(anchor..)` lands mid-UTF-8 and kills
/// the rest of the line's scan (Codex, PR #39).
fn bump(line: &str, start: usize) -> usize {
    start
        + line
            .get(start..)
            .and_then(|rest| rest.chars().next())
            .map_or(1, char::len_utf8)
}

/// Leftmost match of `parts` at or after `from`, within one line. The
/// span is anchored on the first fixed part; wildcards are single word
/// fragments (no whitespace), earliest occurrence — the tmpl glob's
/// discipline applied to spans. A match whose span contains any of the
/// `forbidden` delimiter glyphs is rejected: those are absent from the
/// original input by probing, so any occurrence is an earlier rule's
/// emitted span, and capturing it would nest spans that decode cannot
/// parse (Codex, PR #39).
fn find_span<'a>(
    line: &'a str,
    parts: &[String],
    from: usize,
    forbidden: [char; 3],
) -> Option<SpanMatch<'a>> {
    let first = parts.first()?;
    let mut anchor = from;
    'anchors: while let Some(rel) = line.get(anchor..)?.find(first.as_str()) {
        let start = anchor + rel;
        let mut pos = start + first.len();
        let mut values: Vec<&'a str> = Vec::with_capacity(parts.len().saturating_sub(1));
        for part in parts.iter().skip(1) {
            let rest = line.get(pos..)?;
            let Some(hit) = rest.find(part.as_str()) else {
                anchor = bump(line, start);
                continue 'anchors;
            };
            let value = rest.get(..hit)?;
            if value.contains(char::is_whitespace) {
                anchor = bump(line, start);
                continue 'anchors;
            }
            values.push(value);
            pos += hit + part.len();
        }
        let span = line.get(start..pos)?;
        if forbidden.iter().any(|&ch| span.contains(ch)) {
            anchor = bump(line, start);
            continue 'anchors;
        }
        return Some(SpanMatch {
            start,
            end: pos,
            values,
        });
    }
    None
}

/// The result of applying a rules key to a text.
pub struct Applied {
    pub text: String,
    /// Aliases actually applied, in key (file) order.
    pub used: Vec<String>,
    pub start_name: String,
    pub sep_name: String,
    pub end_name: String,
}

/// Apply every paying rule to `text`. Per rule, every occurrence is
/// rewritten to `START alias (SEP value)* END` and the rewrite must
/// measure a strict token win, or the rule is dropped whole — occurrences
/// are all-or-nothing so decode can rely on `used` alone. Delimiters are
/// probed against the input plus the aliases; rules whose alias occurs in
/// the input are skipped (the positional collision rule).
pub fn apply(text: &str, key: &RulesKey, meter: &dyn TokenMeter) -> Option<Applied> {
    let &(start_name, start) = STARTS.iter().find(|(_, ch)| !text.contains(*ch))?;
    let &(end_name, end) = ENDS.iter().find(|(_, ch)| !text.contains(*ch))?;
    let &(sep_name, sep) = SEPS.iter().find(|(_, ch)| !text.contains(*ch))?;

    let mut current = text.to_string();
    let mut current_tokens = meter.count(&current);
    let mut used: Vec<String> = Vec::new();
    for (alias, parts) in &key.entries {
        if current.contains(alias.as_str())
            || alias.contains(start)
            || alias.contains(end)
            || alias.contains(sep)
        {
            continue;
        }
        let mut rewritten = String::with_capacity(current.len());
        let mut hits = 0usize;
        for (idx, line) in current.split('\n').enumerate() {
            if idx > 0 {
                rewritten.push('\n');
            }
            let mut cursor = 0usize;
            while let Some(m) = find_span(line, parts, cursor, [start, end, sep]) {
                rewritten.push_str(line.get(cursor..m.start).unwrap_or_default());
                rewritten.push(start);
                rewritten.push_str(alias);
                for value in &m.values {
                    rewritten.push(sep);
                    rewritten.push_str(value);
                }
                rewritten.push(end);
                hits += 1;
                cursor = m.end;
            }
            rewritten.push_str(line.get(cursor..).unwrap_or_default());
        }
        if hits == 0 {
            continue;
        }
        let rewritten_tokens = meter.count(&rewritten);
        if rewritten_tokens < current_tokens {
            current = rewritten;
            current_tokens = rewritten_tokens;
            used.push(alias.clone());
        }
    }
    Some(Applied {
        text: current,
        used,
        start_name: start_name.to_string(),
        sep_name: sep_name.to_string(),
        end_name: end_name.to_string(),
    })
}

/// Wrap an inner artifact in the `rules` container that pins the key.
pub fn emit(inner: &str, key: &RulesKey, applied: &Applied) -> String {
    container::emit(&Container {
        codec: "rules".to_string(),
        params: vec![
            ("sum".to_string(), key.sum.clone()),
            ("used".to_string(), applied.used.concat()),
            ("start".to_string(), applied.start_name.clone()),
            ("sep".to_string(), applied.sep_name.clone()),
            ("end".to_string(), applied.end_name.clone()),
        ],
        legend: Vec::new(),
        body: inner.to_string(),
    })
}

/// The encode-side gate, mirroring the phrase legend's: no applied rules →
/// no wrapper; a wrapped artifact must beat the raw floor of the original,
/// else fall back to `raw(original)` — the inner artifact alone would
/// decode to the *rewritten* text.
pub fn wrap_if_used(
    inner: String,
    key: &RulesKey,
    applied: &Applied,
    meter: &dyn TokenMeter,
    original: &str,
) -> String {
    if applied.used.is_empty() {
        return inner;
    }
    let wrapped = emit(&inner, key, applied);
    if meter.count(&wrapped) < meter.count(&container::raw(original)) {
        wrapped
    } else {
        container::raw(original)
    }
}

/// Expand a `rules` container body (already inner-decoded) back to the
/// original bytes. Fail closed: the exact key or nothing.
pub fn expand(c: &Container, decoded_inner: &str, key: Option<&RulesKey>) -> Result<String> {
    let sum = c.param("sum").context("rules container missing sum")?;
    let Some(key) = key else {
        bail!(
            "artifact was encoded against a rules key (sum={sum}); \
             pass --rules with that exact file"
        );
    };
    if key.sum != sum {
        bail!(
            "rules key mismatch: artifact pins sum={sum}, file has {} — \
             refusing to reconstruct wrong bytes",
            key.sum
        );
    }
    let start = named(STARTS, c.param("start").context("rules missing start")?)?;
    let end = named(ENDS, c.param("end").context("rules missing end")?)?;
    let sep = named(SEPS, c.param("sep").context("rules missing sep")?)?;
    let used = c.param("used").unwrap_or_default();
    expand_spans(decoded_inner, key, start, end, sep, used)
}

/// The span-inversion core, shared by decode and `rules verify`: every
/// `START alias (SEP value)* END` span interleaves back through the key.
pub fn expand_spans(
    text: &str,
    key: &RulesKey,
    start: char,
    end: char,
    sep: char,
    used: &str,
) -> Result<String> {
    // `used` is a concatenation; substring `contains` would let an unused
    // alias ride on a recorded one (CodeRabbit, PR #39). Tokenize it into
    // discrete aliases — parse refuses overlapping aliases, so the greedy
    // longest-first scan is unambiguous — and fail closed on any residue.
    let mut known: Vec<&str> = key.entries.iter().map(|(a, _)| a.as_str()).collect();
    known.sort_by_key(|a| std::cmp::Reverse(a.len()));
    let mut used_set: Vec<&str> = Vec::new();
    let mut rest_used = used;
    while !rest_used.is_empty() {
        let Some(&hit) = known.iter().find(|&&a| rest_used.starts_with(a)) else {
            bail!("used list has an unknown residue near {rest_used:.16?}");
        };
        used_set.push(hit);
        rest_used = rest_used.get(hit.len()..).unwrap_or_default();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    loop {
        let Some(pos) = rest.find(start) else {
            out.push_str(rest);
            return Ok(out);
        };
        out.push_str(rest.get(..pos).unwrap_or_default());
        let after = rest.get(pos + start.len_utf8()..).unwrap_or_default();
        let close = after
            .find(end)
            .with_context(|| format!("unterminated rules span near {after:.40?}"))?;
        let span = after.get(..close).unwrap_or_default();
        let mut fields = span.split(sep);
        let alias = fields.next().unwrap_or_default();
        if !used_set.contains(&alias) {
            bail!("rules span references alias {alias:?} outside the used list");
        }
        let (_, parts) = key
            .entries
            .iter()
            .find(|(a, _)| a == alias)
            .with_context(|| format!("alias {alias:?} not in the rules key"))?;
        let values: Vec<&str> = fields.collect();
        if values.len() + 1 != parts.len() {
            bail!(
                "rules span for {alias:?} has {} values, template wants {}",
                values.len(),
                parts.len().saturating_sub(1),
            );
        }
        let mut parts_iter = parts.iter();
        out.push_str(parts_iter.next().map(String::as_str).unwrap_or_default());
        for (part, value) in parts_iter.zip(values) {
            out.push_str(value);
            out.push_str(part);
        }
        rest = after.get(close + end.len_utf8()..).unwrap_or_default();
    }
}

/// Delimiter chars by their recorded names — `rules verify` needs the same
/// resolution decode uses.
pub fn delimiters(applied: &Applied) -> Result<(char, char, char)> {
    Ok((
        named(STARTS, &applied.start_name)?,
        named(ENDS, &applied.end_name)?,
        named(SEPS, &applied.sep_name)?,
    ))
}

fn named(table: &[(&str, char)], name: &str) -> Result<char> {
    table
        .iter()
        .find(|(n, _)| *n == name)
        .map(|&(_, ch)| ch)
        .with_context(|| format!("unknown rules delimiter {name:?}"))
}

/// FNV-1a 64, same fingerprint as the legends.
fn fnv1a(bytes: &[u8]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}
