use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::Path;
use std::process::Command;

use crate::verdict::{StepVerdict, Verdict};

/// `.007/gate.toml` — the per-target-repo gate manifest.
///
/// Unknown fields are tolerated on purpose (serde ignores them; no
/// `deny_unknown_fields`) so the manifest can grow without breaking an older
/// `o7`. `schema` is bumped only on a genuinely breaking change.
#[derive(Debug, Deserialize)]
pub struct GateManifest {
    #[allow(dead_code)]
    #[serde(default = "default_schema")]
    pub schema: u32,
    #[serde(default)]
    pub gate: Vec<GateStep>,
}

fn default_schema() -> u32 {
    1
}

#[derive(Debug, Clone, Deserialize)]
pub struct GateStep {
    pub name: String,
    pub cmd: String,
    #[serde(default = "default_true")]
    pub required: bool,
    /// `Some("windows")` → run on the Windows host (Phase 2, OwnAudit gates).
    /// `None`/other → run via bash in the worktree (the MVP path).
    #[serde(default)]
    pub env: Option<String>,
    /// Pre-declared, auditable reason this step may be legitimately skipped on
    /// a host that cannot run it (the `o7-run` `GateApplicability::Waived`
    /// doctrine: a waiver is the ONLY way a required gate may be skipped).
    /// Absent → an env-skipped REQUIRED step scores the run `BLOCKED`, never
    /// `PASS` (fail closed). Additive: older manifests parse unchanged.
    #[serde(default)]
    pub waive_reason: Option<String>,
}

fn default_true() -> bool {
    true
}

impl GateManifest {
    pub fn load(path: &Path) -> Result<GateManifest> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("reading gate manifest {}", path.display()))?;
        Self::parse(&text).with_context(|| format!("parsing gate manifest {}", path.display()))
    }

    /// Parse a gate manifest from TOML text. Entry point for fuzzing the
    /// untrusted-config parser without touching the filesystem.
    pub fn parse(text: &str) -> Result<GateManifest> {
        toml::from_str(text).context("parsing gate manifest TOML")
    }

    /// Validate the manifest BEFORE anything runs (Codex P2s on #67): a blank
    /// or duplicate step name would let one step's outcome be attributed to
    /// another (canonical gate ids must be unique), and two distinct names
    /// that sanitize to the same log filename (`a.b` vs `a-b`) would silently
    /// overwrite the first step's log — replay would then "verify" the
    /// surviving log as evidence for BOTH gates. Fail loud, before the agent
    /// or any gate command spends anything.
    ///
    /// # Errors
    /// The first blank name, duplicate name, or sanitized-log collision found.
    pub fn validate(&self) -> Result<()> {
        let mut names: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
        let mut logs: std::collections::BTreeMap<String, &str> = std::collections::BTreeMap::new();
        for step in &self.gate {
            if step.name.trim().is_empty() {
                anyhow::bail!("gate manifest has a step with a blank name");
            }
            if !names.insert(step.name.as_str()) {
                anyhow::bail!(
                    "gate manifest has duplicate step name '{}' — canonical gate ids must be unique",
                    step.name
                );
            }
            let log = sanitize(&step.name);
            if let Some(prev) = logs.insert(log.clone(), step.name.as_str()) {
                anyhow::bail!(
                    "gate steps '{prev}' and '{}' both sanitize to log '{log}.log' — \
                     the second would overwrite the first's evidence; rename one",
                    step.name
                );
            }
        }
        Ok(())
    }

    /// Run every step in `workdir`, writing per-step logs into `gate_out`.
    /// Returns per-step verdicts (feed to `Verdict::reduce`).
    ///
    /// Validates the manifest first (`validate`) — the log-collision check
    /// must hold on the execution path even for callers that skipped the
    /// pre-run contract build.
    ///
    /// This batch form is unchanged behavior-wise since before Q-Deck R0.7 —
    /// the live-ingress caller (`main.rs::execute`, only when `--ledger` is
    /// given) iterates `self.gate` and calls [`Self::run_one_step`] directly
    /// instead, so it can mint+project a canonical event immediately after
    /// each step rather than only after every step has already finished.
    pub fn run(&self, workdir: &Path, gate_out: &Path) -> Result<Vec<StepVerdict>> {
        self.validate()?;
        std::fs::create_dir_all(gate_out)?;
        let mut out = Vec::new();
        for step in &self.gate {
            out.push(self.run_one_step(step, workdir, gate_out)?);
        }
        Ok(out)
    }

    /// Run exactly one step, writing its log into `gate_out` if it actually
    /// executes. Extracted from [`Self::run`]'s loop body so a live caller
    /// can mint a canonical event immediately after each step, not only
    /// after the whole manifest has finished.
    ///
    /// # Errors
    /// Writing the step's log file fails.
    pub fn run_one_step(
        &self,
        step: &GateStep,
        workdir: &Path,
        gate_out: &Path,
    ) -> Result<StepVerdict> {
        // MVP exercises the unix/bash path only. `env == "windows"` (OwnAudit's
        // FlaUI/ClrMD/Roslyn gates on the host) is Phase 2 — the step cannot run
        // here. The step's OWN `required` flag is preserved (the old code demoted
        // it to `false`, which let `reduce` score a manifest of skipped required
        // gates as PASS — a false green): with a pre-declared waiver the skip is
        // legitimate (`NotApplicable` + the auditable reason); without one, a
        // required step scores `Blocked` — existence of a step is not execution.
        if step.env.as_deref() == Some("windows") {
            let (verdict, waived) = match &step.waive_reason {
                Some(reason) if !reason.trim().is_empty() => {
                    eprintln!("[o7] gate '{}' env=windows — waived: {}", step.name, reason);
                    (Verdict::NotApplicable, Some(reason.clone()))
                }
                _ => {
                    eprintln!(
                        "[o7] gate '{}' env=windows — cannot run here and carries \
                         no waiver: BLOCKED (declare `waive_reason` to skip \
                         legitimately)",
                        step.name
                    );
                    (Verdict::Blocked, None)
                }
            };
            return Ok(StepVerdict {
                name: step.name.clone(),
                required: step.required,
                verdict,
                exit_code: None,
                log: String::new(),
                waived,
            });
        }

        println!("[o7]   gate: {} :: {}", step.name, step.cmd);
        let result = Command::new("bash")
            .arg("-lc")
            .arg(&step.cmd)
            .current_dir(workdir)
            .output();

        let (verdict, exit_code, combined) = match result {
            Ok(o) => {
                let mut buf = String::new();
                buf.push_str(&String::from_utf8_lossy(&o.stdout));
                buf.push_str(&String::from_utf8_lossy(&o.stderr));
                let v = if o.status.success() {
                    Verdict::Pass
                } else {
                    Verdict::Fail
                };
                (v, o.status.code(), buf)
            }
            Err(e) => (Verdict::Error, None, format!("failed to spawn bash: {e}")),
        };

        let log_name = format!("{}.log", sanitize(&step.name));
        std::fs::write(gate_out.join(&log_name), &combined)?;

        Ok(StepVerdict {
            name: step.name.clone(),
            required: step.required,
            verdict,
            exit_code,
            log: format!("gate/{log_name}"),
            waived: None,
        })
    }
}

fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic, clippy::indexing_slicing)]
mod tests {
    use super::*;
    use crate::verdict::Verdict;

    #[test]
    fn manifest_parses_waive_reason_and_defaults() {
        let m = GateManifest::parse(
            r#"
            [[gate]]
            name = "build"
            cmd = "true"

            [[gate]]
            name = "flaui"
            cmd = "run.ps1"
            env = "windows"
            waive_reason = "FlaUI needs the Windows stand (OwnAudit Phase 2)"
            "#,
        )
        .unwrap();
        assert_eq!(m.gate.len(), 2);
        assert!(m.gate[0].required, "required defaults to true");
        assert!(m.gate[0].waive_reason.is_none());
        assert_eq!(m.gate[1].env.as_deref(), Some("windows"));
        assert!(m.gate[1].waive_reason.is_some());
    }

    fn run_manifest(toml: &str) -> Vec<StepVerdict> {
        let m = GateManifest::parse(toml).unwrap();
        let dir = std::env::temp_dir().join(format!(
            "o7-gate-test-{}-{}",
            std::process::id(),
            std::thread::current()
                .name()
                .unwrap_or("t")
                .replace(':', "-")
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let steps = m.run(&dir, &dir.join("gate")).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        steps
    }

    /// THE false-green regression (007 audit): a manifest whose only required
    /// step is `env = "windows"` used to demote the step to `required: false`
    /// with `NotApplicable`, and reduce to PASS with zero checks executed.
    /// Now the step keeps its declared `required`, scores `Blocked` without a
    /// waiver, and the run reduces to `Blocked`.
    #[test]
    fn unwaived_windows_required_step_blocks_the_run() {
        let steps = run_manifest(
            r#"
            [[gate]]
            name = "clrmd-heap"
            cmd = "run.ps1"
            env = "windows"
            "#,
        );
        assert_eq!(steps.len(), 1);
        assert!(steps[0].required, "declared required must be preserved");
        assert_eq!(steps[0].verdict, Verdict::Blocked);
        assert!(steps[0].waived.is_none());
        assert_eq!(Verdict::reduce(&steps), Verdict::Blocked);
    }

    /// With a pre-declared waiver the same skip is legitimate: NotApplicable,
    /// the reason is carried in the record, and the run can PASS.
    #[test]
    fn waived_windows_required_step_passes_the_run() {
        let steps = run_manifest(
            r#"
            [[gate]]
            name = "unit"
            cmd = "true"

            [[gate]]
            name = "clrmd-heap"
            cmd = "run.ps1"
            env = "windows"
            waive_reason = "windows stand only; tracked for Phase 2"
            "#,
        );
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0].verdict, Verdict::Pass);
        assert_eq!(steps[1].verdict, Verdict::NotApplicable);
        assert!(steps[1].required);
        assert_eq!(
            steps[1].waived.as_deref(),
            Some("windows stand only; tracked for Phase 2")
        );
        assert_eq!(Verdict::reduce(&steps), Verdict::Pass);
    }

    /// A blank waiver is not a waiver (the o7-run `EmptyWaiverReason` rule).
    #[test]
    fn blank_waive_reason_is_not_a_waiver() {
        let steps = run_manifest(
            r#"
            [[gate]]
            name = "clrmd-heap"
            cmd = "run.ps1"
            env = "windows"
            waive_reason = "  "
            "#,
        );
        assert_eq!(steps[0].verdict, Verdict::Blocked);
        assert!(steps[0].waived.is_none());
        assert_eq!(Verdict::reduce(&steps), Verdict::Blocked);
    }

    /// An OPTIONAL windows step without a waiver stays advisory: it is
    /// recorded `Blocked` (it could not run) but never blocks the run.
    #[test]
    fn optional_windows_step_never_blocks() {
        let steps = run_manifest(
            r#"
            [[gate]]
            name = "extra-profiling"
            cmd = "run.ps1"
            env = "windows"
            required = false
            "#,
        );
        assert!(!steps[0].required);
        assert_eq!(Verdict::reduce(&steps), Verdict::Pass);
    }

    /// Distinct names colliding after log-name sanitization (`a.b` vs `a-b`)
    /// would overwrite the first step's evidence and let replay verify the
    /// surviving log for BOTH gates — `run` refuses before executing anything
    /// (Codex P2 on #67).
    #[test]
    fn sanitized_log_collision_is_rejected_before_execution() {
        let m = GateManifest::parse(
            r#"
            [[gate]]
            name = "a.b"
            cmd = "true"

            [[gate]]
            name = "a-b"
            cmd = "true"
            "#,
        )
        .unwrap();
        let dir = std::env::temp_dir();
        assert!(m.run(&dir, &dir.join("o7-collision-test")).is_err());
        // and duplicates / blank names are rejected by the same validator
        assert!(GateManifest::parse(
            r#"
            [[gate]]
            name = "x"
            cmd = "true"

            [[gate]]
            name = "x"
            cmd = "false"
            "#,
        )
        .unwrap()
        .validate()
        .is_err());
        assert!(GateManifest::parse(
            r#"
            [[gate]]
            name = "  "
            cmd = "true"
            "#,
        )
        .unwrap()
        .validate()
        .is_err());
    }

    /// The executed path still works end-to-end: pass + fail + verdicts.
    #[test]
    fn executed_steps_score_pass_and_fail() {
        let steps = run_manifest(
            r#"
            [[gate]]
            name = "ok"
            cmd = "true"

            [[gate]]
            name = "boom"
            cmd = "false"
            "#,
        );
        assert_eq!(steps[0].verdict, Verdict::Pass);
        assert_eq!(steps[1].verdict, Verdict::Fail);
        assert_eq!(Verdict::reduce(&steps), Verdict::Fail);
    }
}
