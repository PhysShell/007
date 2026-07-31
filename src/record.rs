use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::io::Write as _;
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

/// Durably records which ledger conversation THIS run's live projection
/// resolved to, before the ledger run row itself necessarily exists (Q-Deck
/// R0.7, `docs/q-deck/r07-live-ingress.md` §2.7 second corrective round).
///
/// `PendingProjection::open` (phase 1) resolves — and, for a fresh
/// conversation, actually CREATES — the conversation before the worktree
/// even exists, but the ledger RUN row isn't created until `attach_run`,
/// well after. If the process crashes in between, `o7 recover`'s catch-up
/// finds no run row and, without this record, has no way to tell whether
/// the original invocation used `--conversation-id <explicit>` or asked for
/// a new one — guessing wrong either creates a phantom second conversation
/// or fails outright. This file is written durably, from the SAME resolved
/// `conversation_id` the live run itself is about to attach into, before
/// canonical `RunStarted` is even minted, so catch-up can read the real
/// answer instead of guessing.
#[derive(Debug, Serialize, Deserialize)]
pub struct LedgerBinding {
    pub schema: u32,
    pub run_id: String,
    pub conversation_id: String,
    pub agent: String,
    pub role: String,
}

const LEDGER_BINDING_FILE: &str = "ledger_binding.json";

impl LedgerBinding {
    /// Durably write this run's ledger-binding record.
    ///
    /// # Errors
    /// Any I/O failure opening, writing, or flushing the file.
    pub fn write_durable(&self, rec: &RunRecord) -> Result<()> {
        let bytes = serde_json::to_vec(self).context("serializing ledger_binding.json")?;
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(rec.dir.join(LEDGER_BINDING_FILE))
            .context("opening ledger_binding.json for a durable write")?;
        f.write_all(&bytes).context("writing ledger_binding.json")?;
        f.sync_data()
            .context("flushing ledger_binding.json to disk")?;
        Ok(())
    }

    /// Read a run directory's ledger-binding record, if one was ever
    /// written — absent for a `--ledger`-less run, or (should never happen
    /// going forward) a run that crashed before this file's own durable
    /// write landed.
    ///
    /// # Errors
    /// The file exists but fails to read or parse.
    pub fn read(run_dir: &Path) -> Result<Option<Self>> {
        let path = run_dir.join(LEDGER_BINDING_FILE);
        if !path.exists() {
            return Ok(None);
        }
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        let binding: Self =
            serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
        Ok(Some(binding))
    }
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

    /// Like [`Self::write_task`], but durable (`write_all` + `sync_data`) —
    /// for the live-ledger path (Q-Deck R0.7,
    /// `docs/q-deck/r07-live-ingress.md`), where the canonical `RunStarted`
    /// event durably references `task.md` by digest BEFORE the agent ever
    /// runs. `write_task`'s plain `std::fs::write` has no such ordering
    /// guarantee, so a crash between it and the (already-durable)
    /// `RunStarted` write could leave a durably-recorded reference to a
    /// `task.md` that doesn't exist on disk yet.
    ///
    /// # Errors
    /// Any I/O failure opening, writing, or flushing the file.
    pub fn write_task_durable(&self, task: &str) -> Result<()> {
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(self.dir.join("task.md"))
            .context("opening task.md for a durable write")?;
        f.write_all(task.as_bytes()).context("writing task.md")?;
        f.sync_data().context("flushing task.md to disk")?;
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
