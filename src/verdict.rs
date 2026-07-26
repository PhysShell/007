use serde::{Deserialize, Serialize};

/// Run verdict.
///
/// The MVP produces `Pass`/`Fail`/`Error`/`Blocked` (see `reduce`); `Warn` and
/// `NotApplicable` appear only as per-STEP verdicts. The variants are
/// serde-versioned and optional-by-construction, so a gate emitting a new one
/// is not a schema migration.
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
    /// Precedence mirrors the canonical reducer (`o7-run`, `Error > Fail >
    /// Blocked > Pass`):
    ///
    /// - any `Error` → `ERROR` (nothing can be trusted);
    /// - else any *required* `Fail` → `FAIL` (a definitive negative);
    /// - else any *required* obligation that was NOT discharged — a step whose
    ///   verdict is `Blocked` or `NotApplicable` — and NOT waived → `BLOCKED`.
    ///   A required gate that never executed can never reduce to `PASS`; the
    ///   only legitimate skip is a pre-declared, auditable waiver
    ///   (`waive_reason` in the gate manifest), exactly the `o7-run`
    ///   `GateApplicability::Waived` doctrine;
    /// - else `PASS`. `Warn` is a passing step with a note and never blocks.
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
        if steps.iter().any(|s| {
            s.required
                && s.waived.is_none()
                && matches!(s.verdict, Verdict::Blocked | Verdict::NotApplicable)
        }) {
            return Verdict::Blocked;
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
    /// The auditable reason this step was legitimately skipped (a pre-declared
    /// waiver from the gate manifest), or `None` when the step ran — or was
    /// skipped WITHOUT a waiver, which `reduce` scores as `BLOCKED` for a
    /// required step. Additive: absent in older records (serde default).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub waived: Option<String>,
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic, clippy::indexing_slicing)]
mod tests {
    use super::*;

    fn step(required: bool, verdict: Verdict, waived: Option<&str>) -> StepVerdict {
        StepVerdict {
            name: "s".into(),
            required,
            verdict,
            exit_code: None,
            log: String::new(),
            waived: waived.map(str::to_string),
        }
    }

    #[test]
    fn all_pass_reduces_pass() {
        let steps = [
            step(true, Verdict::Pass, None),
            step(false, Verdict::Pass, None),
        ];
        assert_eq!(Verdict::reduce(&steps), Verdict::Pass);
    }

    #[test]
    fn error_outranks_everything() {
        let steps = [
            step(true, Verdict::Fail, None),
            step(false, Verdict::Error, None),
            step(true, Verdict::NotApplicable, None),
        ];
        assert_eq!(Verdict::reduce(&steps), Verdict::Error);
    }

    #[test]
    fn required_fail_outranks_blocked() {
        let steps = [
            step(true, Verdict::Fail, None),
            step(true, Verdict::NotApplicable, None),
        ];
        assert_eq!(Verdict::reduce(&steps), Verdict::Fail);
    }

    #[test]
    fn optional_fail_does_not_fail_the_run() {
        let steps = [
            step(false, Verdict::Fail, None),
            step(true, Verdict::Pass, None),
        ];
        assert_eq!(Verdict::reduce(&steps), Verdict::Pass);
    }

    /// THE false-green (007 audit / o7-run doctrine): a required gate that
    /// never executed can never reduce to `PASS`. Before the fix, a manifest
    /// whose only required steps were `env = "windows"` reduced to `PASS`
    /// with zero checks executed.
    #[test]
    fn required_undischarged_step_blocks() {
        let steps = [step(true, Verdict::NotApplicable, None)];
        assert_eq!(Verdict::reduce(&steps), Verdict::Blocked);
        let steps = [step(true, Verdict::Blocked, None)];
        assert_eq!(Verdict::reduce(&steps), Verdict::Blocked);
    }

    /// The ONLY legitimate skip of a required step: a pre-declared, auditable
    /// waiver. With one, the step does not block.
    #[test]
    fn waived_required_step_does_not_block() {
        let steps = [
            step(
                true,
                Verdict::NotApplicable,
                Some("windows-only; OwnAudit Phase 2"),
            ),
            step(true, Verdict::Pass, None),
        ];
        assert_eq!(Verdict::reduce(&steps), Verdict::Pass);
    }

    /// An OPTIONAL undischarged step never blocks — with or without a waiver.
    #[test]
    fn optional_undischarged_step_never_blocks() {
        let steps = [step(false, Verdict::NotApplicable, None)];
        assert_eq!(Verdict::reduce(&steps), Verdict::Pass);
    }

    /// A waiver excuses the skip, not a failure: a required step that RAN and
    /// failed still fails the run even if a waiver reason is present.
    #[test]
    fn waiver_does_not_excuse_a_real_fail() {
        let steps = [step(true, Verdict::Fail, Some("stale waiver"))];
        assert_eq!(Verdict::reduce(&steps), Verdict::Fail);
    }

    #[test]
    fn warn_never_blocks() {
        let steps = [step(true, Verdict::Warn, None)];
        assert_eq!(Verdict::reduce(&steps), Verdict::Pass);
    }

    /// Older records (no `waived` field) deserialize with `waived: None`.
    #[test]
    fn step_verdict_deserializes_without_waived_field() {
        let legacy = r#"{"name":"build","required":true,"verdict":"PASS","exit_code":0,"log":"gate/build.log"}"#;
        let s: StepVerdict = serde_json::from_str(legacy).unwrap();
        assert!(s.waived.is_none());
    }
}
