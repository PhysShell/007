//! The MVP ↔ `o7-run` stitch: `o7 run` emits the canonical event stream, the
//! pure reducer is the VERDICT AUTHORITY, and `o7 replay` re-verifies a stored
//! record independently.
//!
//! Before this module, `o7 run` computed its verdict with the ad-hoc
//! `Verdict::reduce` while the exhaustively-specified `o7-run` reducer (1402
//! lines of transition table) sat unwired. Now every run:
//!
//! 1. declares its obligations up front (`build_contract` — gates with
//!    requirement/applicability fixed pre-run, the agent obligation, the
//!    runner environment);
//! 2. records what actually happened as a digest-chained `events.jsonl`
//!    (`build_events`: `RunStarted → AgentStarted/AgentExited →
//!    PatchCaptured → Gate* → RunSealed`);
//! 3. takes its verdict from `reduce_all` over that stream
//!    (`canonical_verdict`) — so an undischarged required obligation is
//!    `Blocked`, and a non-clean agent exit is `Error`, by the reducer's
//!    doctrine, not this module's opinion;
//! 4. can be re-verified forever by `o7 replay` (`replay_record`): chain
//!    continuity, per-event digests, artifact content digests, and an
//!    independent verdict recomputation checked against `meta.json`.
//!
//! Deliberately NOT here (next slices): the `o7-ledger` append path and the
//! `o7-ledger`/`o7-run` id unification, and per-step live timestamps (events
//! are minted post-hoc from step results; timestamps are metadata, never the
//! ordering key).

use anyhow::{anyhow, bail, Context, Result};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use o7_run::event::{
    AgentObligation, AgentOutcome, ArtifactKind, ArtifactRef, Digest256, GateApplicability,
    GateObligation, GateOutcome, GateRequirement, RunContract, RunEvent, RunEventKind,
    RUN_EVENT_SCHEMA_VERSION,
};
use o7_run::ids::{GateId, RunEventId, RunId};
use o7_run::reduce::reduce_all;
use o7_run::replay::{replay_verify, ArtifactError, ArtifactResolver, ReplayReport};
use o7_run::state::Verdict as CanonicalVerdict;

use crate::agent::AgentRun;
use crate::gate::GateManifest;
use crate::record::RunMeta;
use crate::verdict::{StepVerdict, Verdict};

/// The environment `o7 run` executes gates in. The MVP is WSL2/Linux by
/// design (README); a waiver in the manifest is a waiver FOR this
/// environment, which is exactly how the reducer validates it.
pub const RUNNER_ENVIRONMENT: &str = "linux";

/// The canonical event stream file inside a run record.
pub const EVENTS_FILE: &str = "events.jsonl";

/// Build the pre-execution obligation contract from the gate manifest.
///
/// Requirement and applicability are fixed HERE, before anything runs — the
/// reducer forbids inventing them after the fact:
///
/// - a step this host can run → `Applicable`;
/// - an `env = "windows"` step with a non-blank `waive_reason` →
///   `Waived { environment: RUNNER_ENVIRONMENT, reason }` (the one legitimate
///   skip; the reducer rejects a blank reason itself);
/// - an `env = "windows"` step with NO waiver → `Applicable` on purpose: it
///   cannot run here, no gate events will be emitted for it, and the reducer
///   scores a required, applicable, unrun gate `Blocked` — the same fail-closed
///   verdict `gate.rs` records per-step.
///
/// # Errors
/// An invalid manifest (`GateManifest::validate`: blank/duplicate step names,
/// sanitized-log-name collisions — each would let one step's outcome or
/// evidence be attributed to another) or an id-minting failure. Call this
/// BEFORE the agent or any gate runs — an invalid manifest must cost nothing.
pub fn build_contract(manifest: &GateManifest) -> Result<RunContract> {
    manifest.validate()?;
    let mut gates = Vec::new();
    for step in &manifest.gate {
        let gate = GateId::new(step.name.clone())
            .map_err(|e| anyhow!("gate step name invalid as a gate id: {e}"))?;
        let requirement = if step.required {
            GateRequirement::Required
        } else {
            GateRequirement::Optional
        };
        let applicability = match (&step.env, &step.waive_reason) {
            (Some(env), Some(reason)) if env == "windows" && !reason.trim().is_empty() => {
                GateApplicability::Waived {
                    environment: RUNNER_ENVIRONMENT.to_string(),
                    reason: reason.clone(),
                }
            }
            _ => GateApplicability::Applicable,
        };
        gates.push(GateObligation {
            gate,
            requirement,
            applicability,
        });
    }
    Ok(RunContract {
        gate_obligations: gates,
        policy_obligations: Vec::new(),
        sandbox_requirements: Vec::new(),
        agent: AgentObligation::Required,
        runner_environment: RUNNER_ENVIRONMENT.to_string(),
    })
}

/// An artifact reference over in-memory bytes: locator + content digest.
#[must_use]
pub fn artifact(kind: ArtifactKind, locator: &str, bytes: &[u8]) -> ArtifactRef {
    ArtifactRef {
        kind,
        locator: locator.to_string(),
        digest: Digest256::of_bytes(bytes),
    }
}

/// Mints digest-chained events with per-run monotonic sequence numbers.
struct EventChain {
    run_id: RunId,
    seq: u64,
    prev: Digest256,
    out: Vec<RunEvent>,
}

impl EventChain {
    fn new(run_id: RunId) -> Self {
        EventChain {
            run_id,
            seq: 0,
            prev: Digest256::genesis(),
            out: Vec::new(),
        }
    }

    fn push(&mut self, kind: RunEventKind) -> Result<()> {
        let event_id = RunEventId::new(format!("{}-e{}", self.run_id.as_str(), self.seq))
            .map_err(|e| anyhow!("minting event id: {e}"))?;
        let timestamp_millis = i64::try_from(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .context("system clock before epoch")?
                .as_millis(),
        )
        .context("timestamp overflows i64")?;
        let mut event = RunEvent {
            event_id,
            schema_version: RUN_EVENT_SCHEMA_VERSION,
            run_id: self.run_id.clone(),
            sequence: self.seq,
            previous_event_digest: self.prev.clone(),
            event_digest: Digest256::genesis(),
            timestamp_millis,
            kind,
        };
        event.event_digest = event.compute_digest();
        self.prev = event.event_digest.clone();
        self.seq = self.seq.checked_add(1).context("event sequence overflow")?;
        self.out.push(event);
        Ok(())
    }
}

/// Build the full canonical stream for one completed `o7 run`.
///
/// Events are minted post-hoc from the step results — the stream is the
/// canonical RECORD of the run, sequenced in execution order; timestamps are
/// metadata, never the ordering key (the envelope's own rule). Skipped steps
/// (windows-blocked or waived) emit NO gate events — the contract carries
/// their obligation, and the reducer scores them from the contract alone.
///
/// # Errors
/// Id minting, a gate log that cannot be read back, or stream construction
/// overflow — each a harness error, never a verdict.
pub fn build_events(
    run_id: &str,
    contract: RunContract,
    task: &ArtifactRef,
    diff: &ArtifactRef,
    agent: &AgentRun,
    steps: &[StepVerdict],
    record_dir: &Path,
) -> Result<Vec<RunEvent>> {
    let rid = RunId::new(run_id.to_string()).map_err(|e| anyhow!("minting run id: {e}"))?;
    let mut chain = EventChain::new(rid);

    chain.push(RunEventKind::RunStarted {
        contract,
        task: task.clone(),
    })?;
    chain.push(RunEventKind::AgentStarted)?;
    chain.push(RunEventKind::AgentExited {
        outcome: agent_outcome(agent),
    })?;
    chain.push(RunEventKind::PatchCaptured {
        patch: diff.clone(),
    })?;

    for step in steps {
        if matches!(step.verdict, Verdict::Blocked | Verdict::NotApplicable) {
            continue; // never executed: the contract speaks for it
        }
        let gate = GateId::new(step.name.clone())
            .map_err(|e| anyhow!("gate step name invalid as a gate id: {e}"))?;
        let log = if step.log.is_empty() {
            None
        } else {
            let bytes = std::fs::read(record_dir.join(&step.log))
                .with_context(|| format!("reading back gate log {}", step.log))?;
            Some(artifact(ArtifactKind::GateLog, &step.log, &bytes))
        };
        chain.push(RunEventKind::GateStarted { gate: gate.clone() })?;
        chain.push(RunEventKind::GateFinished {
            gate,
            outcome: gate_outcome(step.verdict),
            log,
        })?;
    }

    chain.push(RunEventKind::RunSealed)?;
    Ok(chain.out)
}

/// Map an executed step's verdict to the observed gate outcome.
fn gate_outcome(v: Verdict) -> GateOutcome {
    match v {
        Verdict::Pass => GateOutcome::Pass,
        Verdict::Fail => GateOutcome::Fail,
        Verdict::Warn => GateOutcome::Warn,
        Verdict::Blocked => GateOutcome::Blocked,
        Verdict::NotApplicable => GateOutcome::NotApplicable,
        Verdict::Error => GateOutcome::Error,
    }
}

/// The agent's terminal outcome. `exit_code: None` on unix means the process
/// was signal-terminated, but the MVP does not capture WHICH signal — that is
/// a boundary observation gap, reported as such (the reducer scores any
/// non-clean outcome `Error`, so honesty costs nothing here).
fn agent_outcome(agent: &AgentRun) -> AgentOutcome {
    match agent.exit_code {
        Some(code) => AgentOutcome::ExitedNormally { code },
        None => AgentOutcome::BoundaryFailure {
            reason: "agent exit status carried no exit code (signal-terminated; signal not captured by the MVP)".to_string(),
        },
    }
}

/// Reduce the stream and return the canonical run verdict — the authority
/// `o7 run` records in `meta.json`.
///
/// # Errors
/// A structurally invalid stream (a bug in `build_events`, never a domain
/// outcome) or an unsealed run.
pub fn canonical_verdict(events: &[RunEvent]) -> Result<Verdict> {
    let state = reduce_all(events).map_err(|e| anyhow!("canonical reduction failed: {e}"))?;
    let v = state
        .verdict
        .ok_or_else(|| anyhow!("canonical stream is not sealed — no verdict"))?;
    Ok(from_canonical(v))
}

fn from_canonical(v: CanonicalVerdict) -> Verdict {
    match v {
        CanonicalVerdict::Pass => Verdict::Pass,
        CanonicalVerdict::Fail => Verdict::Fail,
        CanonicalVerdict::Blocked => Verdict::Blocked,
        CanonicalVerdict::Error => Verdict::Error,
    }
}

fn to_canonical(v: Verdict) -> Result<CanonicalVerdict> {
    Ok(match v {
        Verdict::Pass => CanonicalVerdict::Pass,
        Verdict::Fail => CanonicalVerdict::Fail,
        Verdict::Blocked => CanonicalVerdict::Blocked,
        Verdict::Error => CanonicalVerdict::Error,
        Verdict::Warn | Verdict::NotApplicable => {
            bail!("meta verdict {v:?} is not a canonical run verdict")
        }
    })
}

/// Serialize a stream as one canonical JSON event per line.
///
/// # Errors
/// Serialization failure (should be unreachable for well-formed events).
pub fn to_jsonl(events: &[RunEvent]) -> Result<String> {
    let mut s = String::new();
    for e in events {
        s.push_str(&serde_json::to_string(e).context("serializing run event")?);
        s.push('\n');
    }
    Ok(s)
}

/// Parse an `events.jsonl` stream (untrusted input: every id re-validates
/// through the typed constructors on deserialize).
///
/// # Errors
/// Any malformed line, with its 1-based line number.
pub fn from_jsonl(text: &str) -> Result<Vec<RunEvent>> {
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let e: RunEvent = serde_json::from_str(line)
            .with_context(|| format!("events.jsonl line {}", i.saturating_add(1)))?;
        out.push(e);
    }
    Ok(out)
}

/// Resolves artifact locators relative to a run-record directory, so replay
/// can digest-check `task.md`, `diff.patch`, and `gate/*.log` in place.
///
/// A replayed record is UNTRUSTED input, so locators are confined to the
/// record directory before any I/O (Codex P1 on #67): every path component
/// must be a plain name — an absolute locator, a `..`, a `.`, or a prefix
/// component is rejected, so a crafted record can neither read files outside
/// the record (`/etc/...`) nor stall replay on a device file (`/dev/zero`).
/// The check is lexical; a symlink planted INSIDE a record can still point
/// out, which is out of the MVP threat model (records are produced by this
/// harness) and recorded here so it is a decision, not an oversight.
pub struct RecordDirResolver {
    pub base: PathBuf,
}

fn confined(locator: &str) -> bool {
    let p = Path::new(locator);
    !locator.is_empty()
        && p.components()
            .all(|c| matches!(c, std::path::Component::Normal(_)))
}

impl ArtifactResolver for RecordDirResolver {
    fn resolve(&self, a: &ArtifactRef) -> Result<Vec<u8>, ArtifactError> {
        if !confined(&a.locator) {
            return Err(ArtifactError {
                locator: a.locator.clone(),
                reason: "locator escapes the record directory (absolute, `..`, or empty) — refused before I/O".to_string(),
            });
        }
        std::fs::read(self.base.join(&a.locator)).map_err(|e| ArtifactError {
            locator: a.locator.clone(),
            reason: e.to_string(),
        })
    }
}

/// Independently re-verify a stored run record: chain continuity, per-event
/// digests, artifact content digests, and a verdict recomputation checked
/// against the stored `meta.json`.
///
/// # Errors
/// A missing/legacy record (no `events.jsonl`), any tamper evidence, or a
/// verdict mismatch — each reported, never smoothed over.
pub fn replay_record(dir: &Path) -> Result<ReplayReport> {
    let events_text = std::fs::read_to_string(dir.join(EVENTS_FILE)).with_context(|| {
        format!(
            "reading {} — a record without a canonical stream is legacy and not replayable",
            dir.join(EVENTS_FILE).display()
        )
    })?;
    let events = from_jsonl(&events_text)?;
    let meta_text = std::fs::read_to_string(dir.join("meta.json"))
        .with_context(|| format!("reading {}", dir.join("meta.json").display()))?;
    let meta: RunMeta = serde_json::from_str(&meta_text).context("parsing meta.json")?;
    let stored = to_canonical(meta.verdict)?;
    let resolver = RecordDirResolver {
        base: dir.to_path_buf(),
    };
    replay_verify(&events, &resolver, stored)
        .map_err(|e| anyhow!("replay verification failed: {e}"))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic, clippy::indexing_slicing)]
mod tests {
    use super::*;
    use crate::gate::GateManifest;

    fn agent_ok() -> AgentRun {
        AgentRun {
            stdout: String::new(),
            exit_code: Some(0),
        }
    }

    fn step(name: &str, required: bool, verdict: Verdict, waived: Option<&str>) -> StepVerdict {
        StepVerdict {
            name: name.into(),
            required,
            verdict,
            exit_code: None,
            log: String::new(),
            waived: waived.map(str::to_string),
        }
    }

    fn events_for(manifest_toml: &str, agent: &AgentRun, steps: &[StepVerdict]) -> Vec<RunEvent> {
        let m = GateManifest::parse(manifest_toml).unwrap();
        let contract = build_contract(&m).unwrap();
        let task = artifact(ArtifactKind::Task, "task.md", b"do the thing");
        let diff = artifact(ArtifactKind::Diff, "diff.patch", b"");
        build_events(
            "test-run",
            contract,
            &task,
            &diff,
            agent,
            steps,
            Path::new("/nonexistent-unused"),
        )
        .unwrap()
    }

    #[test]
    fn happy_path_reduces_pass_and_chains() {
        let events = events_for(
            r#"
            [[gate]]
            name = "unit"
            cmd = "true"
            "#,
            &agent_ok(),
            &[step("unit", true, Verdict::Pass, None)],
        );
        // digest chain is continuous and self-consistent
        let mut prev = Digest256::genesis();
        for e in &events {
            assert!(e.digest_is_consistent());
            assert_eq!(e.previous_event_digest, prev);
            prev = e.event_digest.clone();
        }
        assert_eq!(canonical_verdict(&events).unwrap(), Verdict::Pass);
    }

    /// The stitched twin of the gate.rs false-green fix: an unwaived
    /// `env = "windows"` required step emits no events, and the CANONICAL
    /// reducer scores the run `Blocked` from the contract alone.
    #[test]
    fn unwaived_windows_required_gate_blocks_canonically() {
        let events = events_for(
            r#"
            [[gate]]
            name = "clrmd-heap"
            cmd = "run.ps1"
            env = "windows"
            "#,
            &agent_ok(),
            &[step("clrmd-heap", true, Verdict::Blocked, None)],
        );
        assert_eq!(canonical_verdict(&events).unwrap(), Verdict::Blocked);
    }

    /// A pre-declared waiver is honored by the reducer end-to-end.
    #[test]
    fn waived_windows_gate_passes_canonically() {
        let events = events_for(
            r#"
            [[gate]]
            name = "unit"
            cmd = "true"

            [[gate]]
            name = "clrmd-heap"
            cmd = "run.ps1"
            env = "windows"
            waive_reason = "windows stand only; Phase 2"
            "#,
            &agent_ok(),
            &[
                step("unit", true, Verdict::Pass, None),
                step(
                    "clrmd-heap",
                    true,
                    Verdict::NotApplicable,
                    Some("windows stand only; Phase 2"),
                ),
            ],
        );
        assert_eq!(canonical_verdict(&events).unwrap(), Verdict::Pass);
    }

    /// The reducer closes the OTHER latent false green the ad-hoc reduce
    /// never saw: an agent that died (non-zero exit) with green gates is
    /// `Error`, not `Pass` — nothing downstream of a dead agent is a
    /// trustworthy positive.
    #[test]
    fn agent_failure_is_error_even_with_green_gates() {
        let agent = AgentRun {
            stdout: String::new(),
            exit_code: Some(1),
        };
        let events = events_for(
            r#"
            [[gate]]
            name = "unit"
            cmd = "true"
            "#,
            &agent,
            &[step("unit", true, Verdict::Pass, None)],
        );
        assert_eq!(canonical_verdict(&events).unwrap(), Verdict::Error);
    }

    #[test]
    fn required_gate_fail_is_fail() {
        let events = events_for(
            r#"
            [[gate]]
            name = "unit"
            cmd = "false"
            "#,
            &agent_ok(),
            &[step("unit", true, Verdict::Fail, None)],
        );
        assert_eq!(canonical_verdict(&events).unwrap(), Verdict::Fail);
    }

    #[test]
    fn duplicate_step_names_are_rejected_loudly() {
        let m = GateManifest::parse(
            r#"
            [[gate]]
            name = "unit"
            cmd = "true"

            [[gate]]
            name = "unit"
            cmd = "false"
            "#,
        )
        .unwrap();
        assert!(build_contract(&m).is_err());
    }

    /// Replay input is untrusted: locators that would escape the record
    /// directory are refused BEFORE any I/O (Codex P1 on #67).
    #[test]
    fn resolver_confines_locators_to_the_record_dir() {
        let r = RecordDirResolver {
            base: std::env::temp_dir(),
        };
        let bad = ["/etc/hostname", "../secret", "a/../../x", "", "/dev/zero"];
        for locator in bad {
            let a = ArtifactRef {
                kind: ArtifactKind::GateLog,
                locator: locator.into(),
                digest: Digest256::genesis(),
            };
            let err = r.resolve(&a).unwrap_err();
            assert!(
                err.reason.contains("refused before I/O"),
                "locator {locator:?} must be refused by confinement, got: {}",
                err.reason
            );
        }
        assert!(confined("gate/unit.log") && confined("task.md"));
    }

    /// An invalid manifest is rejected at contract build — i.e. BEFORE the
    /// agent or any gate runs, now that main builds the contract first
    /// (Codex P2 on #67).
    #[test]
    fn sanitized_log_collision_is_rejected_at_contract_build() {
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
        assert!(build_contract(&m).is_err());
    }

    #[test]
    fn jsonl_round_trips() {
        let events = events_for(
            r#"
            [[gate]]
            name = "unit"
            cmd = "true"
            "#,
            &agent_ok(),
            &[step("unit", true, Verdict::Pass, None)],
        );
        let text = to_jsonl(&events).unwrap();
        let back = from_jsonl(&text).unwrap();
        assert_eq!(events, back);
    }

    /// Full record round-trip on disk: write artifacts + events + meta, then
    /// `replay_record` independently verifies chain, artifacts, and verdict.
    #[test]
    fn replay_verifies_a_written_record() {
        let dir = std::env::temp_dir().join(format!("o7-events-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("gate")).unwrap();
        std::fs::write(dir.join("task.md"), b"do the thing").unwrap();
        std::fs::write(dir.join("diff.patch"), b"").unwrap();
        std::fs::write(dir.join("gate/unit.log"), b"ok\n").unwrap();

        let m = GateManifest::parse(
            r#"
            [[gate]]
            name = "unit"
            cmd = "true"
            "#,
        )
        .unwrap();
        let contract = build_contract(&m).unwrap();
        let task = artifact(ArtifactKind::Task, "task.md", b"do the thing");
        let diff = artifact(ArtifactKind::Diff, "diff.patch", b"");
        let steps = [StepVerdict {
            name: "unit".into(),
            required: true,
            verdict: Verdict::Pass,
            exit_code: Some(0),
            log: "gate/unit.log".into(),
            waived: None,
        }];
        let events = build_events(
            "test-run",
            contract,
            &task,
            &diff,
            &agent_ok(),
            &steps,
            &dir,
        )
        .unwrap();
        let verdict = canonical_verdict(&events).unwrap();
        std::fs::write(dir.join(EVENTS_FILE), to_jsonl(&events).unwrap()).unwrap();

        let meta = RunMeta {
            schema: 1,
            kind: "run".into(),
            run_id: "test-run".into(),
            target: "t".into(),
            repo: "r".into(),
            base_commit: "c".into(),
            engine: "claude".into(),
            model: "m".into(),
            verdict,
            steps: steps.to_vec(),
            agent_exit_code: Some(0),
            session_id: None,
            cost_usd: None,
            started_at: None,
            finished_at: None,
        };
        std::fs::write(
            dir.join("meta.json"),
            serde_json::to_string_pretty(&meta).unwrap(),
        )
        .unwrap();

        let report = replay_record(&dir).unwrap();
        assert_eq!(report.events_verified, events.len() as u64);
        assert!(report.artifacts_verified >= 3); // task + diff + gate log

        // tamper with an artifact -> replay fails loudly
        std::fs::write(dir.join("gate/unit.log"), b"tampered\n").unwrap();
        assert!(replay_record(&dir).is_err());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
