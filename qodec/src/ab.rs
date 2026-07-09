//! Comprehension A/B harness — the go/no-go experiment.
//!
//! Lossless-to-the-decoder ≠ legible-to-the-model, so the codec's real gate
//! is: does a fresh model context answer questions about an *encoded*
//! payload as well as about the raw one? This module keeps the deterministic
//! ends of that experiment — building the paired prompts and grading the
//! answers — while the model invocations stay outside (subagents, `o7
//! judge`, or a human with two browser tabs), per the repo's "LLM does not
//! execute trusted actions" discipline.
//!
//! Question files are JSON arrays:
//! ```json
//! [{"id":"q1","question":"...","accept":["substr","alt-substr"]}]
//! ```
//! An answer is correct when any `accept` entry occurs case-insensitively in
//! the answer string for that id. Keep accept substrings distinctive
//! (identifiers, filenames, codes) — this is deliberately dumb string
//! matching, not an LLM judge.

use anyhow::{bail, Context, Result};
use serde_json::Value;

pub struct Question {
    pub id: String,
    pub question: String,
    pub accept: Vec<String>,
}

pub fn parse_questions(text: &str) -> Result<Vec<Question>> {
    let value: Value = serde_json::from_str(text).context("parsing questions JSON")?;
    let items = value
        .as_array()
        .context("questions file must be a JSON array")?;
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        let id = item
            .get("id")
            .and_then(Value::as_str)
            .context("question missing id")?
            .to_string();
        let question = item
            .get("question")
            .and_then(Value::as_str)
            .context("question missing question")?
            .to_string();
        let accept: Vec<String> = item
            .get("accept")
            .and_then(Value::as_array)
            .context("question missing accept array")?
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect();
        if accept.is_empty() {
            bail!("question {id} has an empty accept list");
        }
        // An empty or whitespace-only accept entry would match nearly any
        // answer via `contains` and silently inflate scores — fail loudly
        // at parse time instead (CodeRabbit review on PR #28).
        if accept.iter().any(|a| a.trim().is_empty()) {
            bail!("question {id} has an empty or whitespace-only accept entry");
        }
        out.push(Question {
            id,
            question,
            accept,
        });
    }
    Ok(out)
}

/// Build one self-contained prompt. `payload_intro` explains what the model
/// is looking at (empty for raw text; the notation brief for encoded).
pub fn prompt(payload_intro: &str, payload: &str, questions: &[Question]) -> String {
    let mut out = String::new();
    out.push_str(
        "Answer the questions below using ONLY the payload in this message.\n\
         Do not use any tools, do not read any files, do not guess beyond the\n\
         payload. Reply with ONLY a JSON object mapping each question id to a\n\
         short answer string, e.g. {\"q1\": \"...\", \"q2\": \"...\"}.\n\n",
    );
    if !payload_intro.is_empty() {
        out.push_str(payload_intro);
        out.push_str("\n\n");
    }
    out.push_str("# Payload\n\n");
    out.push_str(payload);
    out.push_str("\n\n# Questions\n\n");
    for q in questions {
        out.push_str(&format!("{}: {}\n", q.id, q.question));
    }
    out
}

/// The notation brief for encoded payloads — same contract `probe` teaches.
pub fn notation_brief() -> &'static str {
    "The payload is encoded as a `%q1` container. Format: first line\n\
     `%q1 <codec> ...` (parameters), then legend lines `<alias>=<phrase>`\n\
     (each alias is a short stand-in for that exact phrase), then a\n\
     `%q1 body` line, then the body. `%q1 xN` after a line means that line\n\
     occurs N times in total. A `toon` body is a table: first line is a JSON\n\
     array of keys, each following line is one object, cells are JSON values\n\
     joined by the separator named in the header. Mentally decode the body\n\
     via the legend before answering; never emit alias characters in answers."
}

pub struct GradeRow {
    pub id: String,
    pub answer: String,
    pub correct: bool,
}

/// Purely numeric accepts match only at digit boundaries, so `"2"` passes
/// `"2"` or `"2 errors"` but not `"20 warnings"`, and `"14"` passes
/// `"line 14"` but not `"142"`. Everything else stays plain case-insensitive
/// substring matching. (Codex review on PR #28: `contains` alone lets short
/// numeric accepts overstate comprehension scores.)
fn accept_matches(answer_lower: &str, accept: &str) -> bool {
    let a = accept.to_lowercase();
    if a.is_empty() {
        return false;
    }
    if !a.chars().all(|c| c.is_ascii_digit()) {
        return answer_lower.contains(&a);
    }
    let bytes = answer_lower.as_bytes();
    let mut from = 0usize;
    while let Some(pos) = answer_lower.get(from..).and_then(|s| s.find(&a)) {
        let start = from + pos;
        let end = start + a.len();
        let prev_is_digit = start > 0 && bytes.get(start - 1).is_some_and(|b| b.is_ascii_digit());
        let next_is_digit = bytes.get(end).is_some_and(|b| b.is_ascii_digit());
        if !prev_is_digit && !next_is_digit {
            return true;
        }
        from = start + 1;
    }
    false
}

/// Tolerant answer extraction: takes the outermost `{...}` block so chatty
/// model output around the JSON does not fail the grade.
pub fn grade(questions: &[Question], answers_text: &str) -> Result<Vec<GradeRow>> {
    let start = answers_text
        .find('{')
        .context("no JSON object in answers")?;
    let end = answers_text
        .rfind('}')
        .context("no JSON object in answers")?;
    let slice = answers_text
        .get(start..=end)
        .context("malformed answers slice")?;
    let answers: Value = serde_json::from_str(slice).context("parsing answers JSON")?;

    let mut rows = Vec::with_capacity(questions.len());
    for q in questions {
        let answer = answers
            .get(&q.id)
            .map(|v| match v {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            })
            .unwrap_or_default();
        let lower = answer.to_lowercase();
        let correct = q.accept.iter().any(|a| accept_matches(&lower, a));
        rows.push(GradeRow {
            id: q.id.clone(),
            answer,
            correct,
        });
    }
    Ok(rows)
}
