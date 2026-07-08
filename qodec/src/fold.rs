//! `fold` — lossless run-length encoding of consecutive identical lines.
//!
//! Logs and tool output love repeating themselves; a run collapses to the
//! line once plus a `%q1 xN` marker (total N occurrences). Any *original*
//! line starting with `%q1` is escaped with a `%q1 =` prefix so markers, the
//! container boundary, and source text can never be confused.
//!
//! CRLF-safe: lines are split on `\n` only, so `\r` stays part of the line.

use anyhow::{bail, Result};

use crate::container::{self, Container, MAGIC};
use crate::meter::TokenMeter;

pub fn encode(text: &str, meter: &dyn TokenMeter) -> String {
    if text.is_empty() {
        return container::raw(text);
    }
    let ends_with_nl = text.ends_with('\n');
    let mut lines: Vec<&str> = text.split('\n').collect();
    if ends_with_nl {
        lines.pop();
    }

    let mut body = String::new();
    let mut folded_any = false;
    let mut i = 0usize;
    while let Some(&line) = lines.get(i) {
        let mut run = 1usize;
        while lines.get(i + run) == Some(&line) {
            run += 1;
        }
        if line.starts_with(MAGIC) {
            body.push_str(MAGIC);
            body.push_str(" =");
        }
        body.push_str(line);
        body.push('\n');
        if run >= 2 {
            body.push_str(&format!("{MAGIC} x{run}\n"));
            folded_any = true;
        }
        i += run;
    }

    if !folded_any {
        return container::raw(text);
    }

    let encoded = container::emit(&Container {
        codec: "fold".to_string(),
        params: vec![(
            "nl".to_string(),
            if ends_with_nl { "1" } else { "0" }.to_string(),
        )],
        legend: Vec::new(),
        body,
    });
    if meter.count(&encoded) >= meter.count(text) {
        return container::raw(text);
    }
    encoded
}

pub fn decode(c: &Container) -> Result<String> {
    let ends_with_nl = c.param("nl") != Some("0");
    let escape = format!("{MAGIC} =");
    let mut lines: Vec<String> = Vec::new();

    let mut raw_lines: Vec<&str> = c.body.split('\n').collect();
    // Drop the trailing empty element produced by the body's final `\n`.
    if raw_lines.last() == Some(&"") {
        raw_lines.pop();
    }

    for raw_line in raw_lines {
        if let Some(rest) = raw_line.strip_prefix(&escape) {
            // Escaped original line: `%q1 =` + (line starting with `%q1`).
            lines.push(rest.to_string());
            continue;
        }
        if let Some(count) = raw_line
            .strip_prefix(MAGIC)
            .and_then(|r| r.strip_prefix(" x"))
        {
            let n: usize = count.parse()?;
            if n < 2 {
                bail!("fold marker with run < 2");
            }
            let Some(prev) = lines.last().cloned() else {
                bail!("fold marker with no preceding line");
            };
            for _ in 1..n {
                lines.push(prev.clone());
            }
            continue;
        }
        lines.push(raw_line.to_string());
    }

    let mut out = lines.join("\n");
    if ends_with_nl {
        out.push('\n');
    }
    Ok(out)
}
