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
}

impl<'a> Cluster<'a> {
    fn seed(split: &Split<'a>, line_idx: usize) -> Self {
        Self {
            segs: split.segs.iter().map(|s| Some(*s)).collect(),
            members: vec![line_idx],
        }
    }

    /// Word-position agreement against a candidate with the same shape;
    /// whitespace mismatch disqualifies outright.
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
            if *mine == Some(*theirs) {
                hits += 1;
            }
        }
        // A no-word skeleton (blank-ish line) never templates.
        (words > 0).then(|| hits as f64 / words as f64)
    }

    fn absorb(&mut self, split: &Split<'a>, line_idx: usize) {
        for (mine, theirs) in self.segs.iter_mut().zip(&split.segs) {
            if *mine != Some(*theirs) {
                *mine = None;
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

/// Byte length of the char-wise common prefix of two strings — always a
/// char boundary of both.
fn common_prefix_bytes(a: &str, b: &str) -> usize {
    let mut end = 0usize;
    for (ca, cb) in a.chars().zip(b.chars()) {
        if ca != cb {
            break;
        }
        end += ca.len_utf8();
    }
    end
}

/// Byte length of the char-wise common suffix of two strings.
fn common_suffix_bytes(a: &str, b: &str) -> usize {
    let mut end = 0usize;
    for (ca, cb) in a.chars().rev().zip(b.chars().rev()) {
        if ca != cb {
            break;
        }
        end += ca.len_utf8();
    }
    end
}

/// Common prefix/suffix byte lengths over all words. The suffix is
/// computed on what remains after the prefix, so the two never overlap on
/// any member — `word[pre..len-suf]` is always a valid, in-order slice.
fn common_affixes(words: &[&str]) -> (usize, usize) {
    let Some(&first) = words.first() else {
        return (0, 0);
    };
    let mut pre = first.len();
    for &w in words.iter().skip(1) {
        pre = pre.min(common_prefix_bytes(first, w));
    }
    let first_rest = first.get(pre..).unwrap_or_default();
    let mut suf = first_rest.len();
    for &w in words.iter().skip(1) {
        let rest = w.get(pre..).unwrap_or_default();
        suf = suf.min(common_suffix_bytes(first_rest, rest));
    }
    (pre, suf)
}

/// One emitted row: the cluster alias plus, per wildcard, the seg position
/// and how many bytes of the word the template already carries as its
/// common prefix/suffix (0/0 = the bare whole-word slot).
struct RowPlan {
    alias: String,
    slots: Vec<(usize, usize, usize)>,
}

fn render_row(plan: &RowPlan, split: &Split<'_>, sep: char) -> String {
    let mut row = plan.alias.clone();
    for &(pos, pre, suf) in &plan.slots {
        row.push(sep);
        let word = split.segs.get(pos).copied().unwrap_or_default();
        let end = word.len().saturating_sub(suf);
        row.push_str(word.get(pre..end).unwrap_or_default());
    }
    if split.cr {
        row.push('\r');
    }
    row
}

/// Pick the cluster's emitted template: bare whole-word wildcards, or
/// wildcards refined with the members' common prefix/suffix pulled into
/// the template — whichever measures cheaper over the legend line plus all
/// rows. Sub-word slots need no decode change: the affixes simply become
/// part of the template's fixed parts.
fn choose_template(
    cluster: &Cluster<'_>,
    splits: &[Option<Split<'_>>],
    slot: char,
    sep: char,
    alias: &str,
    meter: &dyn TokenMeter,
) -> (String, Vec<(usize, usize, usize)>) {
    let (bare, wild) = cluster.template(slot);
    let bare_slots: Vec<(usize, usize, usize)> = wild.iter().map(|&p| (p, 0, 0)).collect();
    let member_splits: Vec<&Split<'_>> = cluster
        .members
        .iter()
        .filter_map(|&idx| splits.get(idx).and_then(Option::as_ref))
        .collect();
    let mut refined_slots: Vec<(usize, usize, usize)> = Vec::with_capacity(wild.len());
    for &pos in &wild {
        let words: Vec<&str> = member_splits
            .iter()
            .map(|s| s.segs.get(pos).copied().unwrap_or_default())
            .collect();
        let (pre, suf) = common_affixes(&words);
        refined_slots.push((pos, pre, suf));
    }
    if refined_slots.iter().all(|&(_, pre, suf)| pre == 0 && suf == 0) {
        return (bare, bare_slots);
    }
    // Refined template: each wildcard becomes prefix + slot + suffix, the
    // affix bytes taken from any member (they are common by construction).
    let mut refined = String::new();
    let mut wild_iter = refined_slots.iter();
    for (idx, seg) in cluster.segs.iter().enumerate() {
        match seg {
            Some(text) => refined.push_str(text),
            None => {
                let &(pos, pre, suf) = wild_iter.next().unwrap_or(&(idx, 0, 0));
                let word = member_splits
                    .first()
                    .and_then(|s| s.segs.get(pos))
                    .copied()
                    .unwrap_or_default();
                let end = word.len().saturating_sub(suf);
                refined.push_str(word.get(..pre).unwrap_or_default());
                refined.push(slot);
                refined.push_str(word.get(end..).unwrap_or_default());
            }
        }
    }
    // The gate: legend line + every row, measured both ways.
    let cost = |template: &str, slots: &[(usize, usize, usize)]| -> usize {
        let plan = RowPlan {
            alias: alias.to_string(),
            slots: slots.to_vec(),
        };
        let mut total = meter.count(&format!("{alias}={template}"));
        for split in &member_splits {
            total += meter.count(&render_row(&plan, split, sep));
        }
        total
    };
    if cost(&refined, &refined_slots) < cost(&bare, &bare_slots) {
        (refined, refined_slots)
    } else {
        (bare, bare_slots)
    }
}

/// Match a full (CR-stripped) line against template parts: the parts must
/// cover the line in order, and every gap — one slot value — must be a
/// single word fragment (no whitespace). Interior gaps take the earliest
/// occurrence of the next part; once whitespace enters the gap no later
/// occurrence can help, so the scan is linear and deterministic. *Any*
/// consistent assignment roundtrips — decode is interleave(parts, values)
/// — so occurrence choice affects only which bytes land in which value.
/// This is how frozen templates (profile seeds, extern files) match lines
/// now that their parts may start or end mid-word (sub-word slots).
fn glob_match<'a>(line: &'a str, parts: &[String]) -> Option<Vec<&'a str>> {
    let (first, rest_parts) = parts.split_first()?;
    let mut rest = line.strip_prefix(first.as_str())?;
    if rest_parts.is_empty() {
        // Slotless template: the line must be the template, byte for byte.
        return rest.is_empty().then_some(Vec::new());
    }
    let mut values: Vec<&'a str> = Vec::with_capacity(rest_parts.len());
    for (idx, part) in rest_parts.iter().enumerate() {
        let last = idx + 1 == rest_parts.len();
        if last {
            // The final part must close the line exactly.
            let value_len = rest.len().checked_sub(part.len())?;
            let (value, tail) = rest.split_at_checked(value_len)?;
            if tail != part.as_str() || value.contains(char::is_whitespace) {
                return None;
            }
            values.push(value);
            rest = "";
        } else {
            if part.is_empty() {
                // Two adjacent slots — unlearnable and ambiguous; refuse.
                return None;
            }
            let pos = rest.find(part.as_str())?;
            let value = rest.get(..pos)?;
            if value.contains(char::is_whitespace) {
                return None;
            }
            values.push(value);
            rest = rest.get(pos + part.len()..)?;
        }
    }
    debug_assert!(rest.is_empty());
    Some(values)
}

/// Bucket by segment shape, grow clusters greedily — first fit above the
/// similarity bar wins, in arrival order (deterministic). Shared by encode
/// and `qodec learn`.
fn build_clusters<'a>(splits: &[Option<Split<'a>>]) -> Vec<Cluster<'a>> {
    let mut buckets: HashMap<usize, Vec<Cluster<'a>>> = HashMap::new();
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
    for cluster in build_clusters(&splits) {
        if cluster.members.len() < 2 {
            continue;
        }
        // Two shapes per cluster. The *bare* one keeps whole-word slots —
        // general across files, and its long parts are what seed_phrases
        // feeds the miners. The *refined* one bakes the members' common
        // prefix/suffix into the parts (the encoder's sub-word move) —
        // corpus-specific but far cheaper per row when it matches.
        // Consumers glob-match in weight order, so the longer refined
        // template is tried first and the bare one catches the rest; an
        // affix that was mere corpus coincidence costs nothing but bytes
        // in the profile, because every use is still measured.
        let member_splits: Vec<&Split<'_>> = cluster
            .members
            .iter()
            .filter_map(|&i| splits.get(i).and_then(Option::as_ref))
            .collect();
        let mut bare = vec![String::new()];
        let mut refined = vec![String::new()];
        for (idx, seg) in cluster.segs.iter().enumerate() {
            match seg {
                Some(fixed) => {
                    if let Some(part) = bare.last_mut() {
                        part.push_str(fixed);
                    }
                    if let Some(part) = refined.last_mut() {
                        part.push_str(fixed);
                    }
                }
                None => {
                    bare.push(String::new());
                    let words: Vec<&str> = member_splits
                        .iter()
                        .map(|s| s.segs.get(idx).copied().unwrap_or_default())
                        .collect();
                    let (pre, suf) = common_affixes(&words);
                    let word = words.first().copied().unwrap_or_default();
                    let end = word.len().saturating_sub(suf);
                    if let Some(part) = refined.last_mut() {
                        part.push_str(word.get(..pre).unwrap_or_default());
                    }
                    refined.push(word.get(end..).unwrap_or_default().to_string());
                }
            }
        }
        if refined != bare {
            out.push((refined, cluster.members.len()));
        }
        out.push((bare, cluster.members.len()));
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

/// A line claimed by a frozen template before clustering: which template,
/// the slot values glob matching extracted, and the CRLF flag.
struct Claim<'a> {
    template_idx: usize,
    values: Vec<&'a str>,
    cr: bool,
}

/// Pre-match lines against frozen templates (profile seeds or extern
/// entries) in template (weight/file) order — the first match claims the
/// line. Claimed lines skip clustering entirely; that is the priority the
/// sealed buckets used to provide, without a parallel cluster machinery.
fn claim_lines<'a>(raw_lines: &[&'a str], templates: &[Vec<String>]) -> HashMap<usize, Claim<'a>> {
    let mut claims = HashMap::new();
    if templates.is_empty() {
        return claims;
    }
    for (idx, &full) in raw_lines.iter().enumerate() {
        let (line, cr) = match full.strip_suffix('\r') {
            Some(stripped) => (stripped, true),
            None => (full, false),
        };
        if line.contains('\r') {
            continue; // bare CR rides verbatim, as everywhere in tmpl
        }
        for (template_idx, parts) in templates.iter().enumerate() {
            if let Some(values) = glob_match(line, parts) {
                claims.insert(
                    idx,
                    Claim {
                        template_idx,
                        values,
                        cr,
                    },
                );
                break;
            }
        }
    }
    claims
}

/// Frozen templates a line-based legend can carry: no line breaks, no CR,
/// at least one part.
fn usable_templates(templates: &[Vec<String>]) -> Vec<Vec<String>> {
    templates
        .iter()
        .filter(|parts| {
            !parts.is_empty()
                && parts
                    .iter()
                    .all(|p| !p.contains('\n') && !p.contains('\r'))
        })
        .take(MAX_TEMPLATES)
        .cloned()
        .collect()
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
    let seeds = usable_templates(templates);
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

fn encode_pass(text: &str, meter: &dyn TokenMeter, seeds: &[Vec<String>]) -> String {
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
        .flat_map(|parts| parts.iter())
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

    // Seeded groups claim their lines first; a group of one releases its
    // line back to clustering (a legend line for a single row never pays
    // in-artifact).
    let claims = claim_lines(&raw_lines, seeds);
    let mut groups: HashMap<usize, Vec<usize>> = HashMap::new();
    for (&line_idx, claim) in &claims {
        groups.entry(claim.template_idx).or_default().push(line_idx);
    }
    groups.retain(|_, members| members.len() >= 2);
    for members in groups.values_mut() {
        members.sort_unstable();
    }
    let taken: std::collections::HashSet<usize> =
        groups.values().flatten().copied().collect();

    let splits: Vec<Option<Split<'_>>> = raw_lines
        .iter()
        .enumerate()
        .map(|(idx, &l)| {
            if taken.contains(&idx) {
                None
            } else {
                split_line(l)
            }
        })
        .collect();
    let clusters = build_clusters(&splits);

    // Seeded groups first, in seed (weight) order: their legend lines are
    // the profile's template bytes, verbatim.
    let mut seeded_rows: HashMap<usize, String> = HashMap::new();
    let mut legend: Vec<String> = Vec::new();
    let mut group_ids: Vec<usize> = groups.keys().copied().collect();
    group_ids.sort_unstable();
    for template_idx in group_ids {
        if legend.len() >= MAX_TEMPLATES {
            break;
        }
        let (Some(members), Some(parts)) = (groups.get(&template_idx), seeds.get(template_idx))
        else {
            continue;
        };
        let Some((alias, _)) = pool.take() else { break };
        legend.push(format!("{alias}={}", parts.join(&slot.to_string())));
        for &line_idx in members {
            let Some(claim) = claims.get(&line_idx) else { continue };
            let mut row = alias.clone();
            for value in &claim.values {
                row.push(sep);
                row.push_str(value);
            }
            if claim.cr {
                row.push('\r');
            }
            seeded_rows.insert(line_idx, row);
        }
    }

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

    // line index -> row plan (alias + per-wildcard trim amounts)
    let mut rows: HashMap<usize, usize> = HashMap::new();
    let mut plans: Vec<RowPlan> = Vec::new();
    let self_budget = MAX_TEMPLATES.saturating_sub(legend.len());
    for cluster in repeated.iter().take(self_budget) {
        let Some((alias, _)) = pool.take() else { break };
        let (template, slots) = choose_template(cluster, &splits, slot, sep, &alias, meter);
        legend.push(format!("{alias}={template}"));
        let plan_idx = plans.len();
        plans.push(RowPlan { alias, slots });
        for &line_idx in &cluster.members {
            rows.insert(line_idx, plan_idx);
        }
    }
    if legend.is_empty() {
        return container::raw(text);
    }

    let mut body = String::new();
    for (idx, &full) in raw_lines.iter().enumerate() {
        if let Some(row) = seeded_rows.get(&idx) {
            body.push_str(row);
            body.push('\n');
            continue;
        }
        let plan = rows.get(&idx).and_then(|&i| plans.get(i));
        match (plan, splits.get(idx).and_then(Option::as_ref)) {
            (Some(plan), Some(split)) => body.push_str(&render_row(plan, split, sep)),
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
    // Usable entries, kept parallel: templates[i] carries alias aliases[i].
    // The file's slot char never appears in the artifact.
    let mut aliases: Vec<&str> = Vec::new();
    let mut templates: Vec<Vec<String>> = Vec::new();
    for (alias, parts) in &legend.entries {
        if text.contains(alias.as_str()) || alias.contains(sep) || alias.contains(slot) {
            continue;
        }
        if parts.is_empty()
            || parts
                .iter()
                .any(|p| p.contains('\n') || p.contains('\r'))
        {
            continue;
        }
        aliases.push(alias);
        templates.push(parts.clone());
    }
    if templates.is_empty() {
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

    // Glob pre-match against the file's templates (sub-word parts and all).
    // Extern groups pay from the first member — their legend is already in
    // the reader's cached prefix — but each group must still beat the
    // lines it replaces; a refused group releases its lines to clustering.
    let claims = claim_lines(&raw_lines, &templates);
    let mut groups: HashMap<usize, Vec<usize>> = HashMap::new();
    for (&line_idx, claim) in &claims {
        groups.entry(claim.template_idx).or_default().push(line_idx);
    }
    for members in groups.values_mut() {
        members.sort_unstable();
    }
    let mut ext_rows: HashMap<usize, String> = HashMap::new();
    let mut used: Vec<usize> = Vec::new();
    let mut group_ids: Vec<usize> = groups.keys().copied().collect();
    group_ids.sort_unstable();
    for template_idx in group_ids {
        let (Some(members), Some(alias)) =
            (groups.get(&template_idx), aliases.get(template_idx))
        else {
            continue;
        };
        let mut row_tokens = 0usize;
        let mut line_tokens = 0usize;
        let mut group_rows: Vec<(usize, String)> = Vec::new();
        for &line_idx in members {
            let Some(claim) = claims.get(&line_idx) else { continue };
            let mut row = (*alias).to_string();
            for value in &claim.values {
                row.push(sep);
                row.push_str(value);
            }
            if claim.cr {
                row.push('\r');
            }
            row_tokens += meter.count(&row);
            line_tokens += meter.count(raw_lines.get(line_idx).copied().unwrap_or_default());
            group_rows.push((line_idx, row));
        }
        if group_rows.is_empty() || row_tokens >= line_tokens {
            continue; // members go back to clustering instead
        }
        ext_rows.extend(group_rows);
        used.push(template_idx);
    }
    if used.is_empty() {
        return plain;
    }
    // `used` is already in file order — group_ids were sorted.
    let used_aliases: String = used
        .iter()
        .filter_map(|&i| aliases.get(i).copied())
        .collect();

    let splits: Vec<Option<Split<'_>>> = raw_lines
        .iter()
        .enumerate()
        .map(|(idx, &l)| {
            if ext_rows.contains_key(&idx) {
                None
            } else {
                split_line(l)
            }
        })
        .collect();
    let clusters = build_clusters(&splits);

    // Self-learned clusters commit inline exactly like the plain pass.
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
    let mut rows: HashMap<usize, usize> = HashMap::new();
    let mut plans: Vec<RowPlan> = Vec::new();
    let mut inline_legend: Vec<String> = Vec::new();
    for cluster in repeated.iter().take(MAX_TEMPLATES) {
        let Some((alias, _)) = pool.take() else { break };
        let (template, slots) = choose_template(cluster, &splits, slot, sep, &alias, meter);
        inline_legend.push(format!("{alias}={template}"));
        let plan_idx = plans.len();
        plans.push(RowPlan { alias, slots });
        for &line_idx in &cluster.members {
            rows.insert(line_idx, plan_idx);
        }
    }

    let mut body = String::new();
    for (idx, &full) in raw_lines.iter().enumerate() {
        if let Some(row) = ext_rows.get(&idx) {
            body.push_str(row);
            body.push('\n');
            continue;
        }
        let plan = rows.get(&idx).and_then(|&i| plans.get(i));
        match (plan, splits.get(idx).and_then(Option::as_ref)) {
            (Some(plan), Some(split)) => body.push_str(&render_row(plan, split, sep)),
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
