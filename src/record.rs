use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::io::Write as _;
use std::path::{Path, PathBuf};

use crate::verdict::{StepVerdict, Verdict};

/// Q-Deck A0 corrective round 5 (CodeRabbit durability finding): `sync_data`
/// on a file makes its CONTENTS durable; it says nothing about the
/// containing directory's own entry for that file. If the machine loses
/// power after a durable artifact file's own `sync_data()` but before this
/// directory's own metadata reaches disk, the run directory can come back
/// after a crash with the file's own bytes gone — not merely stale — even
/// though the write itself reported success. Opening the directory
/// read-only and `fsync`-ing that file descriptor is the standard way to
/// force its own entries durable on Linux (the only platform this project
/// targets — every other confinement primitive in `src/worktree.rs`
/// already depends on Linux-specific `rustix`/`O_NOFOLLOW`/`openat`
/// semantics, so this is not a NEW portability boundary).
///
/// Crash ordering this establishes: after `write_*_durable` returns `Ok`,
/// both the artifact file's contents AND its directory entry are durable.
/// The canonical event that references the artifact (by digest) is
/// appended, and itself synced, strictly AFTER this call at every real
/// call site — so a crash between the two still leaves a replay-safe
/// state: the artifact exists and is intact, but the record's own
/// events.jsonl simply does not reference it yet (indistinguishable from
/// "this artifact was never durably written," which every existing
/// recovery/replay path already treats as an incomplete-prefix record, not
/// a corrupt one).
fn sync_dir(dir: &Path) -> Result<()> {
    std::fs::File::open(dir)
        .and_then(|f| f.sync_all())
        .with_context(|| format!("syncing directory {}", dir.display()))
}

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
    /// Q-Deck R1 fourth corrective round: the run this one continues from
    /// (`None` for an ordinary top-level `o7 run --ledger`) — the ONLY
    /// durable, pre-`attach_run` source `catch_up` can resolve this from
    /// when no ledger run row exists yet. Its absence (an earlier-schema
    /// record) deserializes as `None` via `#[serde(default)]` — never a
    /// hard parse failure, since schema 1 predates this field.
    #[serde(default)]
    pub parent_run_id: Option<String>,
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
        sync_dir(&rec.dir)
            .context("syncing the run directory after writing ledger_binding.json")?;
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

/// Q-Deck R1 fifth corrective round: the canonical, durable receipt of a
/// successful agent response's provider session identity
/// (`session_receipt.json`) — referenced by digest from the canonical
/// `ProviderSessionCaptured` event (`o7_run::event::RunEventKind`), so
/// recovery can trust it exactly as much as any other digest-verified
/// artifact. `meta.json`'s own `session_id` field is NOT this — it is not
/// part of the digest-chained canonical stream, so nothing about a clean
/// replay ever proved it belonged to this run; this receipt is what
/// replaces it as recovery's session-backfill authority.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderSessionReceipt {
    pub schema: u32,
    pub run_id: String,
    pub engine: String,
    pub provider: String,
    pub model: String,
    pub session_id: String,
}

pub const PROVIDER_SESSION_RECEIPT_FILE: &str = "session_receipt.json";

/// Q-Deck R1 fifth corrective round: the canonical, durable receipt binding
/// a command-continuation child run to the EXACT accepted Command that
/// dispatched it (`command_binding.json`) — referenced by digest from the
/// canonical `CommandBindingCaptured` event. Written durably BEFORE
/// `RunStarted`, exactly like `LedgerBinding`, so a crash between this
/// write and the canonical event landing still leaves the identity
/// recoverable; the canonical event is what makes it tamper-evident rather
/// than a plain mutable sidecar.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandBinding {
    pub schema: u32,
    pub command_id: String,
    pub conversation_id: String,
    pub parent_run_id: String,
    pub child_run_id: String,
    pub command_sha256: String,
}

const COMMAND_BINDING_FILE: &str = "command_binding.json";

impl CommandBinding {
    /// Durably write this run's command-binding record.
    ///
    /// # Errors
    /// Any I/O failure opening, writing, or flushing the file.
    pub fn write_durable(&self, rec: &RunRecord) -> Result<()> {
        let bytes = serde_json::to_vec(self).context("serializing command_binding.json")?;
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(rec.dir.join(COMMAND_BINDING_FILE))
            .context("opening command_binding.json for a durable write")?;
        f.write_all(&bytes)
            .context("writing command_binding.json")?;
        f.sync_data()
            .context("flushing command_binding.json to disk")?;
        sync_dir(&rec.dir)
            .context("syncing the run directory after writing command_binding.json")?;
        Ok(())
    }

    /// Read a run directory's command-binding record, if one was ever
    /// written — absent for a plain `o7 run` (no command to bind).
    ///
    /// # Errors
    /// The file exists but fails to read or parse.
    pub fn read(run_dir: &Path) -> Result<Option<Self>> {
        let path = run_dir.join(COMMAND_BINDING_FILE);
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

/// Q-Deck A0 corrective round 1 (`docs/q-deck/a0-candidate-state.md`): the
/// typed candidate-state receipt content itself now lives in
/// `o7_run::CandidateStateReceiptV1` (so `o7-run`'s own replay/semantic-
/// verification layer can deserialize and cross-check it without depending
/// on this root crate) — this module only keeps the well-known file names
/// every candidate-state artifact locator uses. The raw cumulative patch
/// lives alongside the receipt as a separate file (`CANDIDATE_PATCH_FILE`)
/// — a SEPARATE artifact from R1's own `diff.patch` (this run's diff
/// against its OWN `--base`, unrelated to conversation-wide cumulative
/// continuity).
pub const CANDIDATE_STATE_RECEIPT_FILE: &str = "candidate_state_receipt.json";
pub const CANDIDATE_PATCH_FILE: &str = "candidate.patch";
/// The locator a command-continuation child copies its PARENT's own
/// candidate-state receipt/patch to, inside the CHILD's own run directory —
/// so replaying the child alone never depends on the parent's directory
/// still existing (`docs/q-deck/a0-candidate-state.md` §2/§3).
pub const PARENT_CANDIDATE_RECEIPT_FILE: &str = "parent_candidate_receipt.json";
pub const PARENT_CANDIDATE_PATCH_FILE: &str = "parent_candidate.patch";

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
        sync_dir(&self.dir).context("syncing the run directory after writing task.md")?;
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

    /// Durable (`write_all` + `sync_data`) raw-byte write of a named file in
    /// this run's own directory — Q-Deck A0's own copy-parent's-receipt-
    /// verbatim and write-candidate-patch-before-its-referencing-event uses,
    /// same crash-window discipline as `write_task_durable`.
    ///
    /// # Errors
    /// Any I/O failure opening, writing, or flushing the file.
    pub fn write_bytes_durable(&self, name: &str, bytes: &[u8]) -> Result<()> {
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(self.dir.join(name))
            .with_context(|| format!("opening {name} for a durable write"))?;
        f.write_all(bytes)
            .with_context(|| format!("writing {name}"))?;
        f.sync_data()
            .with_context(|| format!("flushing {name} to disk"))?;
        sync_dir(&self.dir)
            .with_context(|| format!("syncing the run directory after writing {name}"))?;
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

/// Q-Deck A0 corrective round 5 (CodeRabbit durability finding): focused
/// unit tests for the new directory-fsync durability step.
// Q-Deck A0 corrective round 8 (fresh CodeRabbit finding): every
// `unwrap`/`expect` in this module operates ONLY on this test's own
// tempdir paths and freshly written fixture files — never on untrusted
// input reaching this process from a real caller. A panic here is this
// test failing loudly on its own broken assumption, not a production
// fault path this lint would otherwise guard.
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod durability_tests {
    use super::*;

    #[test]
    fn sync_dir_succeeds_for_a_real_directory() {
        let dir = tempfile::tempdir().unwrap();
        sync_dir(dir.path()).expect("syncing a real, existing directory must succeed");
    }

    #[test]
    fn sync_dir_fails_closed_for_a_nonexistent_directory() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("does-not-exist");
        assert!(
            sync_dir(&missing).is_err(),
            "syncing a nonexistent directory must fail, not silently succeed"
        );
    }

    /// `write_task_durable`/`write_bytes_durable`/`LedgerBinding::write_durable`/
    /// `CommandBinding::write_durable` all now sync their containing
    /// directory after the file-level `sync_data()` — confirms this
    /// doesn't change their own observable behavior (content still
    /// correct, still succeeds) for the ordinary case.
    #[test]
    fn durable_writes_still_produce_correct_content_after_the_directory_sync() {
        let runs_dir = tempfile::tempdir().unwrap();
        let rec = RunRecord::create(runs_dir.path(), "target", "run-1").unwrap();

        rec.write_task_durable("do the thing").unwrap();
        assert_eq!(
            std::fs::read_to_string(rec.dir.join("task.md")).unwrap(),
            "do the thing"
        );

        rec.write_bytes_durable("candidate.patch", b"patch bytes")
            .unwrap();
        assert_eq!(
            std::fs::read(rec.dir.join("candidate.patch")).unwrap(),
            b"patch bytes"
        );

        LedgerBinding {
            schema: 1,
            run_id: "run-1".to_owned(),
            conversation_id: "conv-1".to_owned(),
            agent: "claude".to_owned(),
            role: "implementer".to_owned(),
            parent_run_id: None,
        }
        .write_durable(&rec)
        .unwrap();
        assert!(rec.dir.join(LEDGER_BINDING_FILE).exists());

        CommandBinding {
            schema: 1,
            command_id: "cmd-1".to_owned(),
            conversation_id: "conv-1".to_owned(),
            parent_run_id: "parent-1".to_owned(),
            child_run_id: "run-1".to_owned(),
            command_sha256: "d".repeat(64),
        }
        .write_durable(&rec)
        .unwrap();
        assert!(rec.dir.join(COMMAND_BINDING_FILE).exists());
    }
}
