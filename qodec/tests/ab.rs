//! Grader integrity tests — the A/B evidence is only as strong as the
//! string matching underneath it (Codex review on PR #28).

use anyhow::Result;

use qodec::ab::{grade, parse_questions};

const QUESTIONS: &str = r#"[
  {"id":"q1","question":"error count?","accept":["2","two"]},
  {"id":"q2","question":"line?","accept":["14"]},
  {"id":"q3","question":"code?","accept":["CS1061"]}
]"#;

#[test]
fn numeric_accepts_match_only_at_digit_boundaries() -> Result<()> {
    let questions = parse_questions(QUESTIONS)?;

    // Correct answers pass, including digits embedded in prose/punctuation.
    let good = r#"{"q1": "2 errors", "q2": "line 14.", "q3": "error CS1061"}"#;
    let rows = grade(&questions, good)?;
    anyhow::ensure!(rows.iter().all(|r| r.correct), "good answers must pass");

    // Substring lookalikes must fail: 20 ⊅ 2, 142 ⊅ 14.
    let bad = r#"{"q1": "20 warnings", "q2": "142", "q3": "CS8618"}"#;
    let rows = grade(&questions, bad)?;
    anyhow::ensure!(
        rows.iter().all(|r| !r.correct),
        "lookalike answers must fail: {:?}",
        rows.iter().map(|r| (&r.id, r.correct)).collect::<Vec<_>>()
    );

    // Word alternatives still work via plain substring matching.
    let words = r#"{"q1": "two"}"#;
    let rows = grade(&questions, words)?;
    anyhow::ensure!(
        rows.iter()
            .find(|r| r.id == "q1")
            .is_some_and(|r| r.correct),
        "word alternative must pass"
    );
    Ok(())
}

#[test]
fn parse_rejects_empty_and_whitespace_accepts() -> Result<()> {
    for bad in [
        r#"[{"id":"q1","question":"?","accept":[]}]"#,
        r#"[{"id":"q1","question":"?","accept":[""]}]"#,
        r#"[{"id":"q1","question":"?","accept":["  "]}]"#,
    ] {
        anyhow::ensure!(
            parse_questions(bad).is_err(),
            "must reject degenerate accept list: {bad}"
        );
    }
    Ok(())
}

#[test]
fn grade_extracts_json_from_chatty_output() -> Result<()> {
    let questions = parse_questions(QUESTIONS)?;
    let chatty = "Sure! Here are my answers:\n{\"q1\": \"2\", \"q2\": \"14\", \"q3\": \"CS1061\"}\nHope that helps.";
    let rows = grade(&questions, chatty)?;
    anyhow::ensure!(
        rows.iter().all(|r| r.correct),
        "chatty wrapper must not break grading"
    );
    Ok(())
}
