//! `diag` — template mining for compiler/analyzer diagnostic streams.
//!
//! `path:line: warning: [OWN001] event 'X' is subscribed … keep 'Y' alive`
//! repeats one sentence thousands of times; only the `path:line` head and
//! the quoted identifiers change. The miner would *search* for that
//! redundancy (superlinear, re-tokenizing candidates); this codec *knows*
//! where it lives: cluster lines by their tail with `'…'` spans slotted
//! out, put each repeated template in the legend once, and emit each line
//! as alias + head + slot values. One linear pass, byte roundtrip by
//! construction.
//!
//! Recognized heads: `path:line:` (unix diagnostics) and `path(line,col):`
//! (MSBuild). Lines that don't parse, or whose template never repeats,
//! travel verbatim — a passthrough line can never be mistaken for a row
//! because alias glyphs and the probed sep/slot characters are all chosen
//! to be absent from the original input.
//!
//! CRLF-safe: a line's trailing `\r` rides at the end of its body row —
//! never inside the template, because `container::parse` normalizes a
//! trailing `\r` on legend lines and would silently eat it (Codex review
//! on PR #32). Lines with a *bare* CR anywhere else travel verbatim.

use std::collections::HashMap;

use anyhow::{bail, Context, Result};

use crate::alias::{Alphabet, AliasPool};
use crate::container::{self, Container};
use crate::meter::TokenMeter;

/// Column separator between head and slot values, probed like toon's.
const SEPS: &[(&str, char)] = &[("pipe", '|'), ("tab", '\t'), ("broke", '¦'), ("dot", '·')];
/// Placeholder marking a slot inside a legend template. Kept disjoint from
/// the alias alphabets (`alias::GLYPHS` / `alias::SIGILS`) so a template
/// can never be confused with alias material.
const SLOTS: &[(&str, char)] = &[("quest", '¿'), ("laquo", '«'), ("langle", '‹'), ("degree", '°')];
/// Same bound as the miner's default: legends beyond this stop paying.
const MAX_TEMPLATES: usize = 64;

/// Byte length of a diagnostic head, including its final `:`.
/// `path:line:` → first `:` followed by digits and another `:`;
/// `path(line,col):` → first `(digits,digits):`.
fn head_len(line: &str) -> Option<usize> {
    let bytes = line.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b':' if i > 0 => {
                let mut j = i + 1;
                while bytes.get(j).is_some_and(u8::is_ascii_digit) {
                    j += 1;
                }
                if j > i + 1 && bytes.get(j) == Some(&b':') {
                    return Some(j + 1);
                }
            }
            b'(' if i > 0 => {
                let mut j = i + 1;
                let line_start = j;
                while bytes.get(j).is_some_and(u8::is_ascii_digit) {
                    j += 1;
                }
                if j == line_start || bytes.get(j) != Some(&b',') {
                    continue;
                }
                j += 1;
                let col_start = j;
                while bytes.get(j).is_some_and(u8::is_ascii_digit) {
                    j += 1;
                }
                if j > col_start && bytes.get(j) == Some(&b')') && bytes.get(j + 1) == Some(&b':')
                {
                    return Some(j + 2);
                }
            }
            _ => {}
        }
    }
    None
}

/// A tail split into the literal segments around its `'…'` spans. The
/// template is the segments rejoined with `'<slot>'`; the slots are the
/// quoted contents in order. `None` for unbalanced quotes.
fn split_tail(tail: &str, slot: char) -> Option<(String, Vec<&str>)> {
    let parts: Vec<&str> = tail.split('\'').collect();
    if parts.len() % 2 == 0 {
        return None;
    }
    let mut template = String::new();
    let mut slots = Vec::new();
    for (idx, part) in parts.iter().enumerate() {
        if idx % 2 == 0 {
            template.push_str(part);
        } else {
            template.push('\'');
            template.push(slot);
            template.push('\'');
            slots.push(*part);
        }
    }
    Some((template, slots))
}

enum Line<'a> {
    Row {
        /// The line ended with `\r` (CRLF input); re-attached after the row.
        cr: bool,
        head: &'a str,
        template: String,
        slots: Vec<&'a str>,
    },
    /// Emitted verbatim from the original line.
    Pass,
}

pub fn encode(text: &str, meter: &dyn TokenMeter) -> String {
    if text.is_empty() {
        return container::raw(text);
    }
    let Some(&(sep_name, sep)) = SEPS.iter().find(|(_, ch)| !text.contains(*ch)) else {
        return container::raw(text);
    };
    let Some(&(slot_name, slot)) = SLOTS.iter().find(|(_, ch)| !text.contains(*ch)) else {
        return container::raw(text);
    };
    // The pool must also steer clear of the probed sep/slot characters —
    // exclusion is decided against this augmented text.
    let exclusion = format!("{text}{sep}{slot}");
    let mut pool = AliasPool::build(Alphabet::Auto, meter, &exclusion);

    let ends_with_nl = text.ends_with('\n');
    let mut raw_lines: Vec<&str> = text.split('\n').collect();
    if ends_with_nl {
        raw_lines.pop();
    }

    let parsed: Vec<Line<'_>> = raw_lines
        .iter()
        .map(|&full| {
            let (line, cr) = match full.strip_suffix('\r') {
                Some(stripped) => (stripped, true),
                None => (full, false),
            };
            // A bare CR anywhere else would end up inside a template or
            // slot and get mangled by legend normalization — verbatim.
            if line.contains('\r') {
                return Line::Pass;
            }
            let Some(hl) = head_len(line) else {
                return Line::Pass;
            };
            let (head, tail) = match (line.get(..hl), line.get(hl..)) {
                (Some(h), Some(t)) => (h, t),
                _ => return Line::Pass,
            };
            match split_tail(tail, slot) {
                Some((template, slots)) => Line::Row {
                    cr,
                    head,
                    template,
                    slots,
                },
                None => Line::Pass,
            }
        })
        .collect();

    // Templates pay only when they repeat; hand the cheapest aliases to the
    // biggest (count × len) savers.
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for line in &parsed {
        if let Line::Row { template, .. } = line {
            *counts.entry(template.as_str()).or_insert(0) += 1;
        }
    }
    let mut repeated: Vec<(&str, usize)> = counts
        .into_iter()
        .filter(|&(_, count)| count >= 2)
        .collect();
    repeated.sort_by_key(|&(template, count)| {
        (std::cmp::Reverse(count.saturating_sub(1) * template.len()), template.to_string())
    });

    let mut aliases: HashMap<&str, String> = HashMap::new();
    let mut legend: Vec<String> = Vec::new();
    for &(template, _) in repeated.iter().take(MAX_TEMPLATES) {
        let Some((alias, _)) = pool.take() else { break };
        legend.push(format!("{alias}={template}"));
        aliases.insert(template, alias);
    }
    if legend.is_empty() {
        return container::raw(text);
    }

    let mut body = String::new();
    for (line, &full) in parsed.iter().zip(&raw_lines) {
        match line {
            Line::Row {
                cr,
                head,
                template,
                slots,
            } if aliases.contains_key(template.as_str()) => {
                let alias = aliases.get(template.as_str()).map(String::as_str).unwrap_or_default();
                body.push_str(alias);
                body.push_str(head);
                for s in slots {
                    body.push(sep);
                    body.push_str(s);
                }
                if *cr {
                    body.push('\r');
                }
            }
            // Un-committed template or unparsed line: the original,
            // verbatim (the container body is read verbatim to EOF).
            _ => body.push_str(full),
        }
        body.push('\n');
    }

    let encoded = container::emit(&Container {
        codec: "diag".to_string(),
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

pub fn decode(c: &Container) -> Result<String> {
    let slot_name = c.param("slot").context("diag container missing slot")?;
    let &(_, slot) = SLOTS
        .iter()
        .find(|(name, _)| *name == slot_name)
        .with_context(|| format!("unknown diag slot {slot_name:?}"))?;
    let sep_name = c.param("sep").context("diag container missing sep")?;
    let &(_, sep) = SEPS
        .iter()
        .find(|(name, _)| *name == sep_name)
        .with_context(|| format!("unknown diag sep {sep_name:?}"))?;
    let ends_with_nl = c.param("nl") != Some("0");

    // (alias, template segments) — longest alias first so multi-char sigil
    // aliases are never shadowed by a shorter prefix.
    let mut entries: Vec<(&str, Vec<&str>)> = Vec::with_capacity(c.legend.len());
    for line in &c.legend {
        let Some((alias, template)) = line.split_once('=') else {
            bail!("malformed diag legend line {line:?}");
        };
        entries.push((alias, template.split(slot).collect()));
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
        let want_slots = segs.len().saturating_sub(1);
        let mut fields = rest.split(sep);
        let head = fields.next().unwrap_or_default();
        let slots: Vec<&str> = fields.collect();
        if slots.len() != want_slots {
            bail!(
                "diag row has {} slots, template wants {want_slots}: {line:?}",
                slots.len()
            );
        }
        let mut rebuilt = String::from(head);
        let mut segs_iter = segs.iter();
        rebuilt.push_str(segs_iter.next().copied().unwrap_or_default());
        for (seg, s) in segs_iter.zip(slots) {
            rebuilt.push_str(s);
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
