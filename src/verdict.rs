use serde::{Deserialize, Serialize};

/// Run verdict.
///
/// Day 1 only `Pass`/`Fail`/`Error` are ever produced (see `reduce`). The
/// remaining variants exist so Phase-2 gates can start emitting them without a
/// schema migration — the serde-versioned, optional-by-construction contract
/// the design locked on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Verdict {
    Pass,
    Fail,
    Error,
    Warn,
    Blocked,
    NotApplicable,
}

impl Verdict {
    /// Reduce per-step verdicts to one overall verdict.
    ///
    /// MVP rule: any `Error` wins → `ERROR`; else any *required* `Fail` → `FAIL`;
    /// else `PASS`. `Warn`/`Blocked`/`NotApplicable` are ignored here until a
    /// gate actually emits them.
    pub fn reduce(steps: &[StepVerdict]) -> Verdict {
        if steps.iter().any(|s| s.verdict == Verdict::Error) {
            return Verdict::Error;
        }
        if steps
            .iter()
            .any(|s| s.required && s.verdict == Verdict::Fail)
        {
            return Verdict::Fail;
        }
        Verdict::Pass
    }
}

/// One gate step's outcome. Written to `gate/verdict.json` and embedded in `meta.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepVerdict {
    pub name: String,
    pub required: bool,
    pub verdict: Verdict,
    pub exit_code: Option<i32>,
    /// Path (relative to the run record dir) of this step's log.
    pub log: String,
}
