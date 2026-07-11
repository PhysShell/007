//! `tmpl` — Drain-style template mining for arbitrary line-based logs.
//!
//! `diag` needs a diagnostic head and quoted identifiers; real build logs
//! vary in unquoted positions (`Restoring packages for C:\…\A.csproj…`).
//! This codec learns the templates instead: lines split into word and
//! whitespace segments, cluster when their skeletons agree (same segment
//! shape, whitespace byte-equal, ≥ SIMILARITY of words equal — the Drain
//! idea without the parse tree), and the positions that vary become slots.
//! A committed template goes to the legend once; each member line becomes
//! alias + slot values. One linear-ish pass (clusters per bucket are
//! capped), byte roundtrip by construction.
//!
//! CRLF-safe the same way `diag` is: a line's trailing `\r` rides at the
//! end of its body row, never inside the template — `container::parse`
//! normalizes a trailing `\r` on legend lines (Codex review on PR #32).
//! Lines with a bare CR anywhere else travel verbatim.

use std::collections::HashMap;

use anyhow::{bail, Context, Result};

use crate::alias::{Alphabet, AliasPool};
use crate::container::{self, Container};
use crate::meter::TokenMeter;

/// Column separator between slot values, probed against the whole input.
const SEPS: &[(&str, char)] = &[("pipe", '|'), ("tab", '\t'), ("broke", '¦'), ("dot", '·')];
/// Placeholder for a wildcard position inside a legend template. Shared
/// with the extern template legend, which declares its own pick by name.
pub(crate) const SLOTS: &[(&str, char)] =
    &[("quest", '¿'), ("laquo", '«'), ("langle", '‹'), ("degree", '°')];
/// Same legend bound as the miners.
const MAX_TEMPLATES: usize = 64;
/// Fraction of word positions that must match to join a cluster.
const SIMILARITY: f64 = 0.6;
/// New skeletons beyond this per bucket pass through — keeps the clustering
/// pass linear on hostile input.
const MAX_CLUSTERS_PER_BUCKET: usize = 64;

/// A line split into alternating whitespace/word segments. Whitespace is
/// part of the skeleton (byte-equal or no cluster); words may vary.
struct Split<'a> {
    /// Even indices whitespace (possibly empty at the edges), odd = words.
    segs: Vec<&'a str>,
    /// The line ended with `\r` (CRLF input); re-attached after the row.
    cr: bool,
}

fn split_line(full: &str) -> Option<Split<'_>> {
    let (line, cr) = match full.strip_suffix('\r') {
        Some(stripped) => (stripped, true),
        None => (full, false),
    };
    if line.contains('\r') {
        return None; // bare CR — verbatim passthrough
    }
    let mut segs: Vec<&str> = Vec::new();
    let mut rest = line;
    loop {
        let ws_end = rest
            .char_indices()
            .find(|(_, c)| !c.is_whitespace())
            .map_or(rest.len(), |(i, _)| i);
        let (ws, tail) = rest.split_at_checked(ws_end)?;
        segs.push(ws);
        if tail.is_empty() {
            break;
        }
        let word_end = tail
            .char_indices()
            .find(|(_, c)| c.is_whitespace())
            .map_or(tail.len(), |(i, _)| i);
        let (word, tail2) = tail.split_at_checked(word_end)?;
        segs.push(word);
        rest = tail2;
        if rest.is_empty() {
            segs.push("");
            break;
        }
    }
    Some(Split { segs, cr })
}

/// One growing template: fixed words (`Some`) vs wildcards (`None`) at odd
/// (word) positions; whitespace at even positions is byte-fixed.
struct Cluster<'a> {
    segs: Vec<Option<&'a str>>,
    members: Vec<usize>,
    /// Profile-seeded template: fixed positions are exact-match and never
    /// erode, so its legend line stays byte-identical to the profile's —
    /// the property that makes artifacts diff-stable across runs.
    sealed: bool,
    /// Which seed this sealed cluster came from — `encode_extern` maps it
    /// back to the extern file's alias through this index.
    seed_idx: Option<usize>,
}

impl<'a> Cluster<'a> {
    fn seed(split: &Split<'a>, line_idx: usize) -> Self {
        Self {
            segs: split.segs.iter().map(|s| Some(*s)).collect(),
            members: vec![line_idx],
            sealed: false,
            seed_idx: None,
        }
    }

    fn from_profile(segs: &'a [Option<String>], seed_idx: usize) -> Self {
        Self {
            segs: segs.iter().map(Option::as_deref).collect(),
            members: Vec::new(),
            sealed: true,
            seed_idx: Some(seed_idx),
        }
    }

    /// Word-position agreement against a candidate with the same shape;
    /// whitespace mismatch disqualifies outright. Sealed templates demand
    /// every fixed word exactly (wildcards free) — a hit outranks any
    /// same-run cluster because seeds sit first in the bucket.
    fn score(&self, split: &Split<'a>) -> Option<f64> {
        let mut words = 0usize;
        let mut hits = 0usize;
        for (idx, (mine, theirs)) in self.segs.iter().zip(&split.segs).enumerate() {
            if idx % 2 == 0 {
                if *mine != Some(*theirs) {
                    return None;
                }
                continue;
            }
            words += 1;
            match mine {
                Some(word) if *word == *theirs => hits += 1,
                Some(_) if self.sealed => return None,
                _ => {}
            }
        }
        if self.sealed {
            return (words > 0).then_some(1.0);
        }
        // A no-word skeleton (blank-ish line) never templates.
        (words > 0).then(|| hits as f64 / words as f64)
    }

    fn absorb(&mut self, split: &Split<'a>, line_idx: usize) {
        if !self.sealed {
            for (mine, theirs) in self.segs.iter_mut().zip(&split.segs) {
                if *mine != Some(*theirs) {
                    *mine = None;
                }
            }
        }
        self.members.push(line_idx);
    }

    /// (template with slot markers, wildcard positions), fixed at the end
    /// of the pass — members are re-encoded against this final shape.
    fn template(&self, slot: char) -> (String, Vec<usize>) {
        let mut out = String::new();
        let mut wild = Vec::new();
        for (idx, seg) in self.segs.iter().enumerate() {
            match seg {
                Some(text) => out.push_str(text),
                None => {
                    out.push(slot);
                    wild.push(idx);
                }
            }
        }
        (out, wild)
    }
}

/// Rebuild a profile template (fixed parts, wildcards between them) into
/// the seg shape clustering works on. Parts re-split exactly as the
/// original lines did — whitespace/word boundaries are a pure function of
/// the bytes — so the reconstruction is faithful. Templates a line-based
/// legend cannot carry (embedded newlines or any CR) are refused.
fn seed_to_segs(parts: &[String]) -> Option<Vec<Option<String>>> {
    let mut segs: Vec<Option<String>> = Vec::new();
    for (idx, part) in parts.iter().enumerate() {
        if part.contains('\n') {
            return None;
        }
        if idx > 0 {
            segs.push(None); // the wildcard word between consecutive parts
        }
        let split = split_line(part)?;
        if split.cr {
            return None;
        }
        segs.extend(split.segs.iter().map(|s| Some((*s).to_string())));
    }
    // Each part splits to an odd ws-word-…-ws run, so alternation survives
    // concatenation; a template with no word position at all is useless.
    (segs.len() / 2 > 0).then_some(segs)
}

/// Bucket by segment shape, grow clusters greedily — first fit above the
/// similarity bar wins, in arrival order (deterministic). Shared by encode
/// and `qodec learn`. Seeded templates enter their buckets first, in
/// profile (weight) order, so a matching line lands on the known template
/// before any same-run cluster can claim it.
fn build_clusters<'a>(
    splits: &[Option<Split<'a>>],
    seeds: &'a [Vec<Option<String>>],
) -> Vec<Cluster<'a>> {
    let mut buckets: HashMap<usize, Vec<Cluster<'a>>> = HashMap::new();
    for (idx, seed) in seeds.iter().enumerate() {
        buckets
            .entry(seed.len())
            .or_default()
            .push(Cluster::from_profile(seed, idx));
    }
    for (idx, split) in splits.iter().enumerate() {
        let Some(split) = split else { continue };
        let words = split.segs.len() / 2;
        if words == 0 {
            continue;
        }
        let clusters = buckets.entry(split.segs.len()).or_default();
        let found = clusters
            .iter()
            .position(|c| c.score(split).is_some_and(|s| s >= SIMILARITY));
        match found {
            Some(pos) => {
                if let Some(cluster) = clusters.get_mut(pos) {
                    cluster.absorb(split, idx);
                }
            }
            None if clusters.len() < MAX_CLUSTERS_PER_BUCKET => {
                clusters.push(Cluster::seed(split, idx));
            }
            None => {}
        }
    }
    buckets.into_values().flatten().collect()
}

/// Learned templates as (fixed parts, member count) — wildcards sit between
/// consecutive parts. Profile food for `qodec learn`.
pub(crate) fn learn_templates(text: &str) -> Vec<(Vec<String>, usize)> {
    let mut raw_lines: Vec<&str> = text.split('\n').collect();
    if text.ends_with('\n') {
        raw_lines.pop();
    }
    let splits: Vec<Option<Split<'_>>> = raw_lines.iter().map(|&l| split_line(l)).collect();
    let mut out: Vec<(Vec<String>, usize)> = Vec::new();
    for cluster in build_clusters(&splits, &[]) {
        if cluster.members.len() < 2 {
            continue;
        }
        let mut parts = vec![String::new()];
        for seg in &cluster.segs {
            match seg {
                Some(fixed) => {
                    if let Some(part) = parts.last_mut() {
                        part.push_str(fixed);
                    }
                }
                None => parts.push(String::new()),
            }
        }
        out.push((parts, cluster.members.len()));
    }
    out.sort_by(|a, b| {
        let wa = a.1 * a.0.iter().map(String::len).sum::<usize>();
        let wb = b.1 * b.0.iter().map(String::len).sum::<usize>();
        wb.cmp(&wa).then(a.0.cmp(&b.0))
    });
    out
}

pub fn encode(text: &str, meter: &dyn TokenMeter) -> String {
    encode_seeded(text, meter, &[])
}

/// `encode` pre-shaped by profile templates (`qodec learn`). The seeded
/// pass competes with the plain one by measurement — a chance-agreeing
/// position that the plain pass keeps fixed makes shorter rows, so seeds
/// must *win or tie* to be taken (tie goes to seeded: byte-stable profile
/// legends are worth having at equal cost). Stale seeds can therefore
/// waste a pass, never tokens or bytes.
pub(crate) fn encode_seeded(
    text: &str,
    meter: &dyn TokenMeter,
    templates: &[Vec<String>],
) -> String {
    let plain = encode_pass(text, meter, &[]);
    let seeds: Vec<Vec<Option<String>>> = templates
        .iter()
        .filter_map(|parts| seed_to_segs(parts))
        .take(MAX_TEMPLATES)
        .collect();
    if seeds.is_empty() {
        return plain;
    }
    let seeded = encode_pass(text, meter, &seeds);
    if meter.count(&seeded) <= meter.count(&plain) {
        seeded
    } else {
        plain
    }
}

fn encode_pass(text: &str, meter: &dyn TokenMeter, seeds: &[Vec<Option<String>>]) -> String {
    if text.is_empty() {
        return container::raw(text);
    }
    let Some(&(sep_name, sep)) = SEPS.iter().find(|(_, ch)| !text.contains(*ch)) else {
        return container::raw(text);
    };
    // The slot char splits *legend* templates on decode, and seeded
    // templates put profile bytes into the legend — so the slot must be
    // absent from the seeds too, not just the input.
    let seed_blob: String = seeds
        .iter()
        .flat_map(|segs| segs.iter().flatten())
        .map(String::as_str)
        .collect();
    let Some(&(slot_name, slot)) = SLOTS
        .iter()
        .find(|(_, ch)| !text.contains(*ch) && !seed_blob.contains(*ch))
    else {
        return container::raw(text);
    };
    let exclusion = format!("{text}{sep}{slot}");
    let mut pool = AliasPool::build(Alphabet::Auto, meter, &exclusion);

    let ends_with_nl = text.ends_with('\n');
    let mut raw_lines: Vec<&str> = text.split('\n').collect();
    if ends_with_nl {
        raw_lines.pop();
    }

    let splits: Vec<Option<Split<'_>>> = raw_lines.iter().map(|&l| split_line(l)).collect();
    let clusters = build_clusters(&splits, seeds);

    // Rank repeated clusters by saved fixed text, hand out cheap aliases.
    let mut repeated: Vec<&Cluster<'_>> = clusters
        .iter()
        .filter(|c| c.members.len() >= 2)
        .collect();
    repeated.sort_by_key(|c| {
        let fixed: usize = c.segs.iter().flatten().map(|s| s.len()).sum();
        (
            std::cmp::Reverse((c.members.len() - 1) * fixed),
            c.members.first().copied().unwrap_or_default(),
        )
    });

    // line index -> (alias, wildcard positions)
    let mut rows: HashMap<usize, (String, Vec<usize>)> = HashMap::new();
    let mut legend: Vec<String> = Vec::new();
    for cluster in repeated.iter().take(MAX_TEMPLATES) {
        let Some((alias, _)) = pool.take() else { break };
        let (template, wild) = cluster.template(slot);
        legend.push(format!("{alias}={template}"));
        for &line_idx in &cluster.members {
            rows.insert(line_idx, (alias.clone(), wild.clone()));
        }
    }
    if legend.is_empty() {
        return container::raw(text);
    }

    let mut body = String::new();
    for (idx, &full) in raw_lines.iter().enumerate() {
        match (rows.get(&idx), splits.get(idx).and_then(Option::as_ref)) {
            (Some((alias, wild)), Some(split)) => {
                body.push_str(alias);
                for &pos in wild {
                    body.push(sep);
                    body.push_str(split.segs.get(pos).copied().unwrap_or_default());
                }
                if split.cr {
                    body.push('\r');
                }
            }
            _ => body.push_str(full),
        }
        body.push('\n');
    }

    let encoded = container::emit(&Container {
        codec: "tmpl".to_string(),
        params: vec![
            ("sep".to_string(), sep_name.to_string()),
            ("slot".to_string(), slot_name.to_string()),
            ("n".to_string(), legend.len().to_string()),
            (
                "nl".to_string(),
                if ends_with_nl { "1" } else { "0" }.to_string(),
            ),
        ],
        legend,
        body,
    });
    if meter.count(&encoded) < meter.count(text) {
        encoded
    } else {
        container::raw(text)
    }
}

/// `encode` against an extern template legend (`qodec legend --templates`):
/// lines matching a frozen template emit rows that reference the *file's*
/// alias, and `ext=`/`used=` params pin the file by checksum — the legend
/// line itself costs nothing in-artifact, which is exactly what cross-file
/// templates need to stop losing to chance-agreement ones. Self-learned
/// clusters still commit inline like plain tmpl. Guarantees:
/// - each used extern template must beat the lines it replaces (a template
///   whose slot values carry the whole line is passed through);
/// - the whole artifact must beat the plain one *strictly* — a tie goes to
///   plain, which demands no key;
/// - an extern alias occurring anywhere in the input skips its entry, so
///   expansion can never touch pre-existing bytes.
pub fn encode_extern(
    text: &str,
    meter: &dyn TokenMeter,
    legend: &crate::legend::TemplateLegend,
) -> String {
    let plain = encode(text, meter);
    if text.is_empty() {
        return plain;
    }
    let Some(&(sep_name, sep)) = SEPS.iter().find(|(_, ch)| !text.contains(*ch)) else {
        return plain;
    };
    let Some(&(slot_name, slot)) = SLOTS.iter().find(|(_, ch)| !text.contains(*ch)) else {
        return plain;
    };
    // Usable entries, kept parallel: seeds[i] rebuilds entries with alias
    // aliases[i]. The file's slot char never appears in the artifact.
    let mut aliases: Vec<&str> = Vec::new();
    let mut seeds: Vec<Vec<Option<String>>> = Vec::new();
    for (alias, parts) in &legend.entries {
        if text.contains(alias.as_str()) || alias.contains(sep) || alias.contains(slot) {
            continue;
        }
        let Some(segs) = seed_to_segs(parts) else { continue };
        aliases.push(alias);
        seeds.push(segs);
    }
    if seeds.is_empty() {
        return plain;
    }
    // Self-learned aliases must never collide with the file's.
    let exclusion = format!("{text}{sep}{slot}{}", aliases.concat());
    let mut pool = AliasPool::build(Alphabet::Auto, meter, &exclusion);

    let ends_with_nl = text.ends_with('\n');
    let mut raw_lines: Vec<&str> = text.split('\n').collect();
    if ends_with_nl {
        raw_lines.pop();
    }
    let splits: Vec<Option<Split<'_>>> = raw_lines.iter().map(|&l| split_line(l)).collect();
    let clusters = build_clusters(&splits, &seeds);

    // Extern clusters pay from the first member — their legend is already
    // in the reader's cached prefix. Rows render up front so the
    // per-cluster gate can measure them against the lines they replace.
    let mut ext_rows: HashMap<usize, String> = HashMap::new();
    let mut used: Vec<usize> = Vec::new();
    for cluster in &clusters {
        let Some(seed_idx) = cluster.seed_idx else { continue };
        if cluster.members.is_empty() {
            continue;
        }
        let Some(alias) = aliases.get(seed_idx) else { continue };
        let (_, wild) = cluster.template(slot);
        let mut row_tokens = 0usize;
        let mut line_tokens = 0usize;
        let mut cluster_rows: Vec<(usize, String)> = Vec::new();
        for &line_idx in &cluster.members {
            let Some(Some(split)) = splits.get(line_idx) else { continue };
            let mut row = String::from(*alias);
            for &pos in &wild {
                row.push(sep);
                row.push_str(split.segs.get(pos).copied().unwrap_or_default());
            }
            if split.cr {
                row.push('\r');
            }
            row_tokens += meter.count(&row);
            line_tokens += meter.count(raw_lines.get(line_idx).copied().unwrap_or_default());
            cluster_rows.push((line_idx, row));
        }
        if row_tokens >= line_tokens {
            continue; // members ride verbatim instead
        }
        ext_rows.extend(cluster_rows);
        used.push(seed_idx);
    }
    if used.is_empty() {
        return plain;
    }
    // `used` in file order — deterministic regardless of bucket iteration.
    used.sort_unstable();
    let used_aliases: String = used
        .iter()
        .filter_map(|&i| aliases.get(i).copied())
        .collect();

    // Self-learned clusters commit inline exactly like the plain pass.
    let mut repeated: Vec<&Cluster<'_>> = clusters
        .iter()
        .filter(|c| c.seed_idx.is_none() && c.members.len() >= 2)
        .collect();
    repeated.sort_by_key(|c| {
        let fixed: usize = c.segs.iter().flatten().map(|s| s.len()).sum();
        (
            std::cmp::Reverse((c.members.len() - 1) * fixed),
            c.members.first().copied().unwrap_or_default(),
        )
    });
    let mut rows: HashMap<usize, (String, Vec<usize>)> = HashMap::new();
    let mut inline_legend: Vec<String> = Vec::new();
    for cluster in repeated.iter().take(MAX_TEMPLATES) {
        let Some((alias, _)) = pool.take() else { break };
        let (template, wild) = cluster.template(slot);
        inline_legend.push(format!("{alias}={template}"));
        for &line_idx in &cluster.members {
            rows.insert(line_idx, (alias.clone(), wild.clone()));
        }
    }

    let mut body = String::new();
    for (idx, &full) in raw_lines.iter().enumerate() {
        if let Some(row) = ext_rows.get(&idx) {
            body.push_str(row);
            body.push('\n');
            continue;
        }
        match (rows.get(&idx), splits.get(idx).and_then(Option::as_ref)) {
            (Some((alias, wild)), Some(split)) => {
                body.push_str(alias);
                for &pos in wild {
                    body.push(sep);
                    body.push_str(split.segs.get(pos).copied().unwrap_or_default());
                }
                if split.cr {
                    body.push('\r');
                }
            }
            _ => body.push_str(full),
        }
        body.push('\n');
    }

    let candidate = container::emit(&Container {
        codec: "tmpl".to_string(),
        params: vec![
            ("sep".to_string(), sep_name.to_string()),
            ("slot".to_string(), slot_name.to_string()),
            ("n".to_string(), inline_legend.len().to_string()),
            (
                "nl".to_string(),
                if ends_with_nl { "1" } else { "0" }.to_string(),
            ),
            ("ext".to_string(), legend.sum.clone()),
            ("used".to_string(), used_aliases),
        ],
        legend: inline_legend,
        body,
    });
    if meter.count(&candidate) < meter.count(&plain) {
        candidate
    } else {
        plain
    }
}

pub fn decode(c: &Container, templates: Option<&crate::legend::TemplateLegend>) -> Result<String> {
    let slot_name = c.param("slot").context("tmpl container missing slot")?;
    let &(_, slot) = SLOTS
        .iter()
        .find(|(name, _)| *name == slot_name)
        .with_context(|| format!("unknown tmpl slot {slot_name:?}"))?;
    let sep_name = c.param("sep").context("tmpl container missing sep")?;
    let &(_, sep) = SEPS
        .iter()
        .find(|(name, _)| *name == sep_name)
        .with_context(|| format!("unknown tmpl sep {sep_name:?}"))?;
    let ends_with_nl = c.param("nl") != Some("0");

    let mut entries: Vec<(&str, Vec<&str>)> = Vec::with_capacity(c.legend.len());
    for line in &c.legend {
        let Some((alias, template)) = line.split_once('=') else {
            bail!("malformed tmpl legend line {line:?}");
        };
        entries.push((alias, template.split(slot).collect()));
    }
    // An `ext=` param pins an extern template legend: rows may reference
    // its aliases without any in-artifact legend line. Fail closed — the
    // exact file or nothing — and only admit aliases the encoder recorded
    // in `used`, so a file entry can never touch rows it did not emit.
    if let Some(sum) = c.param("ext") {
        let Some(legend) = templates else {
            bail!(
                "artifact pins an extern template legend (ext={sum}); \
                 pass --extern-templates with that exact file"
            );
        };
        if legend.sum != sum {
            bail!(
                "extern template legend mismatch: artifact pins ext={sum}, file has {} — \
                 refusing to reconstruct wrong bytes",
                legend.sum
            );
        }
        let used = c.param("used").unwrap_or_default();
        for (alias, parts) in &legend.entries {
            if used.contains(alias.as_str()) {
                entries.push((alias.as_str(), parts.iter().map(String::as_str).collect()));
            }
        }
    }
    entries.sort_by_key(|(alias, _)| std::cmp::Reverse(alias.len()));

    let mut lines: Vec<&str> = c.body.split('\n').collect();
    if lines.last() == Some(&"") {
        lines.pop();
    }

    let mut out: Vec<String> = Vec::with_capacity(lines.len());
    for line in lines {
        let hit = entries
            .iter()
            .find_map(|(alias, segs)| line.strip_prefix(alias).map(|rest| (rest, segs)));
        let Some((rest, segs)) = hit else {
            out.push(line.to_string());
            continue;
        };
        let (rest, cr) = match rest.strip_suffix('\r') {
            Some(stripped) => (stripped, true),
            None => (rest, false),
        };
        let want = segs.len().saturating_sub(1);
        let values: Vec<&str> = if want == 0 {
            if !rest.is_empty() {
                bail!("tmpl row for slotless template must be empty: {line:?}");
            }
            Vec::new()
        } else {
            let values: Vec<&str> = rest
                .strip_prefix(sep)
                .with_context(|| format!("tmpl row missing separator: {line:?}"))?
                .split(sep)
                .collect();
            if values.len() != want {
                bail!("tmpl row has {} slots, template wants {want}: {line:?}", values.len());
            }
            values
        };
        let mut rebuilt = String::new();
        let mut segs_iter = segs.iter();
        rebuilt.push_str(segs_iter.next().copied().unwrap_or_default());
        for (seg, value) in segs_iter.zip(values) {
            rebuilt.push_str(value);
            rebuilt.push_str(seg);
        }
        if cr {
            rebuilt.push('\r');
        }
        out.push(rebuilt);
    }
    let mut text = out.join("\n");
    if ends_with_nl {
        text.push('\n');
    }
    Ok(text)
}
