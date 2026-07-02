use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::verdict::{StepVerdict, Verdict};

/// Canonical run record metadata (`meta.json`).
///
/// serde-versioned with optional/future fields skipped when empty — the
/// forward-compat contract that lets consensus/memory bolt on later with no
/// migration. Bump `schema` only on a breaking change.
#[derive(Debug, Serialize, Deserialize)]
pub struct RunMeta {
    pub schema: u32,
    pub kind: String,
    pub run_id: String,
    pub target: String,
    pub repo: String,
    pub base_commit: String,
    pub engine: String,
    pub model: String,
    pub verdict: Verdict,
    pub steps: Vec<StepVerdict>,
    pub agent_exit_code: Option<i32>,

    // --- optional / Phase-2 (extracted from claude JSON, timings, consensus) ---
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub cost_usd: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub started_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub finished_at: Option<String>,
}

/// A run record directory in `007`'s private store: `runs/<target>/<run-id>/`.
pub struct RunRecord {
    pub dir: PathBuf,
}

impl RunRecord {
    pub fn create(runs_dir: &Path, target: &str, run_id: &str) -> Result<RunRecord> {
        let dir = runs_dir.join(target).join(run_id);
        std::fs::create_dir_all(&dir)?;
        Ok(RunRecord { dir })
    }

    pub fn gate_dir(&self) -> PathBuf {
        self.dir.join("gate")
    }

    pub fn write_task(&self, task: &str) -> Result<()> {
        std::fs::write(self.dir.join("task.md"), task)?;
        Ok(())
    }

    pub fn write_agent_stdout(&self, s: &str) -> Result<()> {
        std::fs::write(self.dir.join("agent.stdout"), s)?;
        Ok(())
    }

    pub fn write_diff(&self, d: &str) -> Result<()> {
        std::fs::write(self.dir.join("diff.patch"), d)?;
        Ok(())
    }

    pub fn write_text(&self, name: &str, s: &str) -> Result<()> {
        std::fs::write(self.dir.join(name), s)?;
        Ok(())
    }

    pub fn write_json<T: Serialize>(&self, name: &str, v: &T) -> Result<()> {
        std::fs::write(self.dir.join(name), serde_json::to_string_pretty(v)?)?;
        Ok(())
    }

    pub fn write_meta(&self, meta: &RunMeta) -> Result<()> {
        std::fs::write(
            self.dir.join("meta.json"),
            serde_json::to_string_pretty(meta)?,
        )?;
        std::fs::write(
            self.gate_dir().join("verdict.json"),
            serde_json::to_string_pretty(&meta.steps)?,
        )?;
        Ok(())
    }
}
