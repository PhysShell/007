//! `grep` — group `path:line[:col]:text` matcher output by file path.
//!
//! rg/grep with line numbers repeats the path on every hit; runs of hits in
//! the same file collapse to the path once plus the per-line remainder —
//! the same shape as `rg --heading`, which models read natively. Only
//! *consecutive* runs are grouped, so original line order (and bytes)
//! survive: decode re-prefixes each member line and is byte-exact.
//!
//! Body microformat: a probed marker character (absent from the whole
//! input, so no escaping can ever be needed) starts every section —
//! `<mark><path>` opens a path group, a bare `<mark>` opens a verbatim
//! passthrough section for lines that did not parse or did not repeat.

use anyhow::{bail, Context, Result};

use crate::container::{self, Container};
use crate::meter::TokenMeter;

/// Section markers, probed in order: the first char absent from the input
/// wins. Named symbolically in the header to keep the container parseable.
const MARKS: &[(&str, char)] = &[
    ("raquo", '»'),
    ("pilcrow", '¶'),
    ("broke", '¦'),
    ("dot", '·'),
    ("sect", '§'),
];

/// Byte offset of the `:` that ends the path in `path:line[:col]:text` —
/// the first `:` followed by one or more digits and another `:`. Parse
/// failures only cost grouping, never correctness: unparsed lines ride in
/// passthrough sections verbatim.
fn path_end(line: &str) -> Option<usize> {
    let bytes = line.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if b != b':' || i == 0 {
            continue;
        }
        let mut j = i + 1;
        while bytes.get(j).is_some_and(u8::is_ascii_digit) {
            j += 1;
        }
        if j > i + 1 && bytes.get(j) == Some(&b':') {
            return Some(i);
        }
    }
    None
}

pub fn encode(text: &str, meter: &dyn TokenMeter) -> String {
    if text.is_empty() {
        return container::raw(text);
    }
    let Some(&(mark_name, mark)) = MARKS
        .iter()
        .find(|(_, ch)| !text.contains(*ch))
    else {
        return container::raw(text);
    };

    let ends_with_nl = text.ends_with('\n');
    let mut lines: Vec<&str> = text.split('\n').collect();
    if ends_with_nl {
        lines.pop();
    }

    // A group only pays when the path repeats; singles and unparsed lines
    // travel verbatim in passthrough sections.
    let paths: Vec<Option<&str>> = lines
        .iter()
        .map(|l| path_end(l).and_then(|i| l.get(..i)))
        .collect();

    let mut body = String::new();
    let mut grouped_any = false;
    let mut i = 0usize;
    let mut in_pass = false;
    while let Some(&line) = lines.get(i) {
        let path = paths.get(i).copied().flatten();
        let run = match path {
            Some(p) => {
                let mut n = 1;
                while paths.get(i + n).copied().flatten() == Some(p) {
                    n += 1;
                }
                n
            }
            None => 1,
        };
        match path {
            Some(p) if run >= 2 => {
                grouped_any = true;
                in_pass = false;
                body.push(mark);
                body.push_str(p);
                body.push('\n');
                for k in i..i + run {
                    let member = lines
                        .get(k)
                        .and_then(|l| l.get(p.len() + 1..))
                        .unwrap_or_default();
                    body.push_str(member);
                    body.push('\n');
                }
                i += run;
            }
            _ => {
                if !in_pass {
                    body.push(mark);
                    body.push('\n');
                    in_pass = true;
                }
                body.push_str(line);
                body.push('\n');
                i += 1;
            }
        }
    }
    if !grouped_any {
        return container::raw(text);
    }

    let encoded = container::emit(&Container {
        codec: "grep".to_string(),
        params: vec![
            ("mark".to_string(), mark_name.to_string()),
            (
                "nl".to_string(),
                if ends_with_nl { "1" } else { "0" }.to_string(),
            ),
        ],
        legend: Vec::new(),
        body,
    });
    if meter.count(&encoded) < meter.count(text) {
        encoded
    } else {
        container::raw(text)
    }
}

pub fn decode(c: &Container) -> Result<String> {
    let mark_name = c.param("mark").context("grep container missing mark")?;
    let &(_, mark) = MARKS
        .iter()
        .find(|(name, _)| *name == mark_name)
        .with_context(|| format!("unknown grep mark {mark_name:?}"))?;
    let ends_with_nl = c.param("nl") != Some("0");

    let mut lines: Vec<&str> = c.body.split('\n').collect();
    if lines.last() == Some(&"") {
        lines.pop();
    }

    let mut out: Vec<String> = Vec::new();
    // None = passthrough section, Some(path) = group section.
    let mut section: Option<&str> = None;
    let mut seen_header = false;
    for line in lines {
        if let Some(rest) = line.strip_prefix(mark) {
            seen_header = true;
            section = (!rest.is_empty()).then_some(rest);
            continue;
        }
        if !seen_header {
            bail!("grep body must start with a section marker");
        }
        match section {
            Some(path) => out.push(format!("{path}:{line}")),
            None => out.push(line.to_string()),
        }
    }
    let mut text = out.join("\n");
    if ends_with_nl {
        text.push('\n');
    }
    Ok(text)
}
