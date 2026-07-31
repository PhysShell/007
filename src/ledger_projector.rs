//! Q-Deck R0.7 (`docs/q-deck/r07-live-ingress.md`): projects `o7-run`'s
//! canonical `RunEvent` stream into `o7-ledger`, live, as each event is
//! minted — never a second reducer, never a recomputed verdict.
//!
//! A thin SYNC facade over `o7-ledger`'s async API (an internal
//! current-thread tokio runtime): the root `o7 run` CLI stays exactly as
//! synchronous as it is today for everything except this one new sink —
//! `execute_live` never needs `async`/`.await` of its own.
//!
//! **Ordering invariant this module does not enforce itself, but exists to
//! make possible**: a canonical event must be durably appended to
//! `events.jsonl` (the caller's job — see `main.rs::execute_live`) BEFORE
//! [`LiveLedgerProjector::project`] is ever called for it. The ledger must
//! never contain a projection of an event that isn't yet part of the
//! on-disk canonical record — otherwise a crash between the two leaves a
//! read model whose "source" was never actually written.
//!
//! **Restart safety**: every operation here is idempotent. Opening/attaching
//! (`PendingProjection::open` + `attach_run`), projecting an event
//! (`project`), and sealing (`seal`) are all safe to retry after a crash —
//! attaching to an existing run/attempt instead of erroring or creating a
//! duplicate, and no-op'ing (or conflicting loudly) on an already-applied
//! terminal status rather than blindly re-applying it.

use std::path::Path;

use anyhow::{Context, Result};

use o7_ledger::{
    AttemptId, ConversationId, Idempotency, NewRun, RunId as LedgerRunId, RunStatus, SqliteLedger,
};
use o7_run::event::{RunEvent, RunEventKind};
use o7_run::ids::RunId as CanonicalRunId;
use o7_run::state::Verdict as CanonicalVerdict;

/// How the projector resolves the ledger conversation for a run (section
/// 2.4: never an implicit "most recent conversation").
pub enum ConversationSelector {
    /// No `--conversation-id` given: create a fresh ledger conversation.
    /// Idempotent — retrying `PendingProjection::open` for the SAME
    /// `run_id` resolves to the SAME conversation, not a second one
    /// (keyed deterministically off the run id itself, since exactly one
    /// conversation is created per run under this selector).
    New,
    /// `--conversation-id <id>` given: must already resolve to a real
    /// conversation, or `PendingProjection::open` fails loudly — never
    /// silently creates one under the caller-supplied id.
    Existing(String),
}

/// Phase 1: the ledger opened and the conversation resolved, but no run/
/// attempt created yet and no `running` status reported. Safe to do before
/// the worktree exists (`docs/q-deck/r07-live-ingress.md`'s ordering
/// contract: only the RUN's lifecycle must wait for durable `RunStarted`,
/// not the conversation).
pub struct PendingProjection {
    ledger: SqliteLedger,
    rt: tokio::runtime::Runtime,
    conversation_id: ConversationId,
}

impl PendingProjection {
    /// Open `ledger_path` and resolve/create the conversation.
    ///
    /// # Errors
    /// The ledger path cannot be opened / fails schema attestation, or a
    /// given `--conversation-id` does not resolve to a real conversation.
    pub fn open(
        ledger_path: &Path,
        conversation: ConversationSelector,
        run_id: &CanonicalRunId,
    ) -> Result<Self> {
        let ledger = SqliteLedger::open(ledger_path)
            .with_context(|| format!("opening ledger at {}", ledger_path.display()))?;
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .context("starting the ledger projector's runtime")?;

        let conversation_id = match conversation {
            ConversationSelector::New => {
                rt.block_on(ledger.create_conversation(Some(Idempotency {
                    key: format!("conversation-for-run:{}", run_id.as_str()),
                })))
                .context("creating (or re-attaching to) this run's ledger conversation")?
                .conversation_id
            }
            ConversationSelector::Existing(id) => {
                let conv_id = ConversationId::from_raw(id.clone());
                let found = rt
                    .block_on(ledger.conversation(conv_id.clone()))
                    .with_context(|| format!("resolving --conversation-id {id}"))?;
                match found {
                    Some(_) => conv_id,
                    None => anyhow::bail!(
                        "--conversation-id {id} does not resolve to an existing ledger \
                         conversation — refusing to guess or create one under a caller-\
                         supplied id"
                    ),
                }
            }
        };

        Ok(Self {
            ledger,
            rt,
            conversation_id,
        })
    }

    #[must_use]
    pub fn conversation_id(&self) -> &ConversationId {
        &self.conversation_id
    }

    /// Phase 2: attach to (or, if this is genuinely the first time, create)
    /// the ledger run and its attempt, and report `running`. Call this ONLY
    /// after the canonical `RunStarted` event has been durably appended to
    /// `events.jsonl` — never before.
    ///
    /// Idempotent/restart-safe: if the run already exists (a resumed
    /// projector after a crash, or `o7 recover`'s catch-up), attaches to it
    /// instead of erroring or creating a duplicate — `start_run` is skipped
    /// if the run is no longer `Queued`, and the existing `running` attempt
    /// is reused instead of creating a second one. A run already terminal
    /// or interrupted is attached read-only to its most recent attempt
    /// (`SqliteLedger::latest_attempt`, regardless of status) — never
    /// `None` — because `append_system_note`'s idempotency digest includes
    /// `attempt_id`, so re-projecting an already-applied event must see the
    /// SAME `attempt_id` it was originally recorded under.
    ///
    /// # Errors
    /// Any underlying ledger error.
    pub fn attach_run(
        self,
        run_id: &CanonicalRunId,
        agent: String,
        role: String,
    ) -> Result<LiveLedgerProjector> {
        let shared_run_id = LedgerRunId::from_raw(run_id.as_str().to_owned());
        let run = self
            .rt
            .block_on(self.ledger.create_run_with_id(
                NewRun {
                    conversation_id: self.conversation_id.clone(),
                    parent_run_id: None,
                    agent,
                    role,
                },
                shared_run_id,
                Idempotency {
                    key: run_id.as_str().to_owned(),
                },
            ))
            .with_context(|| format!("creating (or re-attaching to) ledger run {run_id}"))?;
        // The just-returned row's own conversation_id is authoritative
        // (matters on the ordinary idempotent-replay path, where this call
        // resolved the SAME conversation the run was originally created
        // under, so this is simply that value echoed back). NOTE: this is
        // NOT a substitute for resolving the correct `ConversationSelector`
        // before calling `attach_run` — if the selector passed to
        // `PendingProjection::open` disagrees with the run's real
        // conversation_id, `create_run_with_id`'s idempotency digest (which
        // includes conversation_id) mismatches and this call fails loudly
        // with a conflict before this line is ever reached. Callers that may
        // be attaching to an ALREADY-existing run (catch-up) must look the
        // run up first and pass its real conversation — see
        // `main.rs::catch_up`.
        let conversation_id = run.conversation_id.clone();

        if run.status == RunStatus::Queued {
            self.rt
                .block_on(self.ledger.start_run(run.run_id.clone()))
                .with_context(|| format!("starting ledger run {run_id}"))?;
        }
        let live = matches!(run.status, RunStatus::Queued | RunStatus::Running);

        let attempt_id = if live {
            match self
                .rt
                .block_on(self.ledger.running_attempt(run.run_id.clone()))
                .with_context(|| format!("looking up the running attempt for run {run_id}"))?
            {
                Some(a) => Some(a.attempt_id),
                None => Some(
                    self.rt
                        .block_on(self.ledger.create_attempt(run.run_id.clone()))
                        .with_context(|| format!("creating an attempt for run {run_id}"))?
                        .attempt_id,
                ),
            }
        } else {
            // Already terminal or interrupted — attaching read-only, e.g.
            // to re-project an idempotent-safe missing tail without
            // claiming a live attempt that no longer exists. Reuse the
            // SAME attempt_id already-projected events used (not `None`):
            // `append_system_note`'s idempotency digest includes
            // `attempt_id`, so re-projecting an already-applied event under
            // a *different* attempt_id than it was originally recorded with
            // would misfire as a conflict instead of replaying cleanly.
            self.rt
                .block_on(self.ledger.latest_attempt(run.run_id.clone()))
                .with_context(|| format!("looking up the most recent attempt for run {run_id}"))?
                .map(|a| a.attempt_id)
        };

        Ok(LiveLedgerProjector {
            rt: self.rt,
            ledger: self.ledger,
            conversation_id,
            run_id: run.run_id,
            attempt_id,
        })
    }
}

/// Owns exactly one live run's projection into `o7-ledger`.
pub struct LiveLedgerProjector {
    rt: tokio::runtime::Runtime,
    ledger: SqliteLedger,
    conversation_id: ConversationId,
    run_id: LedgerRunId,
    attempt_id: Option<AttemptId>,
}

impl LiveLedgerProjector {
    /// Convenience one-shot combining `PendingProjection::open` +
    /// `attach_run` — for callers (catch-up, tests) where the canonical
    /// `RunStarted` is already known to be durable (or where there is no
    /// separate pre-worktree phase to split out of).
    ///
    /// # Errors
    /// See `PendingProjection::open` and `attach_run`.
    pub fn start(
        ledger_path: &Path,
        conversation: ConversationSelector,
        run_id: &CanonicalRunId,
        agent: String,
        role: String,
    ) -> Result<Self> {
        PendingProjection::open(ledger_path, conversation, run_id)?.attach_run(run_id, agent, role)
    }

    #[must_use]
    pub fn conversation_id(&self) -> &ConversationId {
        &self.conversation_id
    }

    /// Project one canonical event's provenance — `run_id`, canonical
    /// `sequence`, `event_digest`, `schema_version`, and `kind` — as a
    /// durable, idempotent `system.note`. Called for EVERY event in the
    /// stream, including `RunStarted` and `RunSealed`: dedicated ledger
    /// lifecycle events (`run.created`/`run.started`/`run.completed`/etc,
    /// applied separately by `attach_run`/`seal`) stay the authoritative
    /// status transitions, but every canonical source event — first and
    /// last included — still gets a durably correlated provenance record,
    /// so which exact `RunEvent` produced a given ledger transition is
    /// never lost.
    ///
    /// Idempotent: safe to call twice for the same event (keyed by this
    /// run's id + the event's own canonical `sequence`).
    ///
    /// # Errors
    /// Any underlying ledger error. Callers should report, not abort the
    /// run, on failure here — a sink issue is not a canonical-record issue.
    pub fn project(&self, event: &RunEvent) -> Result<()> {
        let (kind_name, detail) = match &event.kind {
            RunEventKind::RunStarted { contract, task } => (
                "run_started",
                serde_json::json!({ "contract": contract, "task": task }),
            ),
            RunEventKind::RunSealed => ("run_sealed", serde_json::json!({})),
            RunEventKind::WorktreeCreated { worktree } => (
                "worktree_created",
                serde_json::json!({ "worktree": worktree }),
            ),
            RunEventKind::AgentStarted => ("agent_started", serde_json::json!({})),
            RunEventKind::AgentExited { outcome } => {
                ("agent_exited", serde_json::json!({ "outcome": outcome }))
            }
            RunEventKind::PatchCaptured { patch } => {
                ("patch_captured", serde_json::json!({ "patch": patch }))
            }
            RunEventKind::PolicyChecked { policy, outcome } => (
                "policy_checked",
                serde_json::json!({ "policy": policy, "outcome": outcome }),
            ),
            RunEventKind::GateStarted { gate } => {
                ("gate_started", serde_json::json!({ "gate": gate.as_str() }))
            }
            RunEventKind::GateFinished { gate, outcome, log } => (
                "gate_finished",
                serde_json::json!({ "gate": gate.as_str(), "outcome": outcome, "log": log }),
            ),
            RunEventKind::SandboxEvidenceCaptured {
                subject,
                policy_digest,
                report,
                outcome,
            } => (
                "sandbox_evidence_captured",
                serde_json::json!({
                    "subject": subject,
                    "policy_digest": policy_digest.as_str(),
                    "report": report,
                    "outcome": outcome,
                }),
            ),
        };

        let payload = serde_json::json!({
            "canonical_kind": kind_name,
            "canonical_run_id": event.run_id.as_str(),
            "canonical_sequence": event.sequence,
            "canonical_event_digest": event.event_digest.as_str(),
            "canonical_schema_version": event.schema_version,
            "detail": detail,
        });

        self.rt
            .block_on(self.ledger.append_system_note(
                self.conversation_id.clone(),
                Some(self.run_id.clone()),
                self.attempt_id.clone(),
                payload,
                Idempotency {
                    key: format!("{}:{}", self.run_id.as_str(), event.sequence),
                },
            ))
            .with_context(|| {
                format!(
                    "projecting canonical event {kind_name} (seq {}) for run {}",
                    event.sequence,
                    self.run_id.as_str()
                )
            })?;
        Ok(())
    }

    /// Apply the terminal status from the sealed canonical verdict — call
    /// once, after the `RunSealed` event has been projected, with the
    /// verdict `o7_run::reduce::reduce_all` already produced. Never
    /// recomputed here.
    ///
    /// Idempotent: if the run is already sealed with EXACTLY this verdict's
    /// mapped status (a retried/resumed seal call), this is a no-op. If the
    /// run is already sealed (or interrupted) with a **different** status,
    /// this is a hard conflict — a terminal outcome must never be silently
    /// overwritten.
    ///
    /// # Errors
    /// A status conflict, or any underlying ledger error.
    pub fn seal(&self, verdict: CanonicalVerdict) -> Result<()> {
        let target = match verdict {
            CanonicalVerdict::Pass => RunStatus::Completed,
            CanonicalVerdict::Fail => RunStatus::Failed,
            CanonicalVerdict::Blocked => RunStatus::Blocked,
            CanonicalVerdict::Error => RunStatus::Error,
        };
        let current = self
            .rt
            .block_on(self.ledger.run(self.run_id.clone()))
            .with_context(|| format!("reading run {} before sealing", self.run_id.as_str()))?
            .with_context(|| format!("run {} not found", self.run_id.as_str()))?;

        if current.status == target {
            return Ok(()); // already sealed with this exact verdict — no-op.
        }
        if is_settled(current.status) {
            anyhow::bail!(
                "run {} is already {:?}, cannot seal it as {target:?} — a settled run's \
                 outcome must never be silently overwritten",
                self.run_id.as_str(),
                current.status
            );
        }

        let run_id = self.run_id.clone();
        match verdict {
            CanonicalVerdict::Pass => self.rt.block_on(self.ledger.complete_run(run_id)),
            CanonicalVerdict::Fail => self.rt.block_on(self.ledger.fail_run(run_id)),
            CanonicalVerdict::Blocked => self.rt.block_on(self.ledger.block_run(run_id)),
            CanonicalVerdict::Error => self.rt.block_on(self.ledger.error_run(run_id)),
        }
        .with_context(|| format!("sealing ledger run {} as {verdict:?}", self.run_id.as_str()))?;
        Ok(())
    }

    /// Mark this run `Interrupted` instead of sealing it with a verdict —
    /// the process/execution stopped before a sealed canonical verdict was
    /// reached. Unsealed and resumable; never `Error` (Q-Deck R0.6's frozen
    /// distinction, `docs/q-deck/r06-verdict-fidelity.md`).
    ///
    /// # Errors
    /// Any underlying ledger error.
    pub fn interrupt(&self) -> Result<()> {
        self.rt
            .block_on(self.ledger.interrupt_run(self.run_id.clone()))
            .with_context(|| format!("interrupting ledger run {}", self.run_id.as_str()))?;
        Ok(())
    }
}

/// `Cancelled`/`Completed`/`Failed`/`Blocked`/`Error` are all sealed;
/// `Interrupted` is not sealed but is equally "settled" for `seal`'s
/// conflict check — a run that is already interrupted must not be sealed
/// out from under it either, since that would be exactly the same kind of
/// silent-overwrite `seal`'s equality/conflict check exists to prevent.
fn is_settled(status: RunStatus) -> bool {
    matches!(
        status,
        RunStatus::Completed
            | RunStatus::Failed
            | RunStatus::Cancelled
            | RunStatus::Blocked
            | RunStatus::Error
            | RunStatus::Interrupted
    )
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
mod tests {
    use super::*;
    use o7_ledger::Ledger as _;
    use o7_run::event::{ArtifactKind, ArtifactRef, Digest256, RunContract};
    use o7_run::ids::RunEventId;

    fn tmp_ledger_path(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "o7-ledger-projector-test-{}-{}-{name}",
            std::process::id(),
            std::thread::current()
                .name()
                .unwrap_or("t")
                .replace([':', '/'], "-"),
        ));
        let _ = std::fs::remove_file(&dir);
        dir
    }

    fn contract() -> RunContract {
        RunContract {
            gate_obligations: Vec::new(),
            policy_obligations: Vec::new(),
            sandbox_requirements: Vec::new(),
            agent: o7_run::event::AgentObligation::Required,
            runner_environment: "linux".to_string(),
        }
    }

    fn artifact(bytes: &[u8]) -> ArtifactRef {
        ArtifactRef {
            kind: ArtifactKind::Task,
            locator: "task.md".to_string(),
            digest: Digest256::of_bytes(bytes),
        }
    }

    /// A minimal, self-built 3-event canonical stream (RunStarted,
    /// AgentStarted, RunSealed) — enough to exercise the projector without
    /// pulling in a full gate manifest/agent run.
    fn tiny_stream(canonical_run_id: &CanonicalRunId) -> Vec<RunEvent> {
        let mut prev = Digest256::genesis();
        let mut seq = 0u64;
        let mut mint = |kind: RunEventKind| {
            let mut e = RunEvent {
                event_id: RunEventId::new(format!("{}-e{seq}", canonical_run_id.as_str())).unwrap(),
                schema_version: o7_run::event::RUN_EVENT_SCHEMA_VERSION,
                run_id: canonical_run_id.clone(),
                sequence: seq,
                previous_event_digest: prev.clone(),
                event_digest: Digest256::genesis(),
                timestamp_millis: 0,
                kind,
            };
            e.event_digest = e.compute_digest();
            prev = e.event_digest.clone();
            seq += 1;
            e
        };
        vec![
            mint(RunEventKind::RunStarted {
                contract: contract(),
                task: artifact(b"task"),
            }),
            mint(RunEventKind::AgentStarted),
            mint(RunEventKind::RunSealed),
        ]
    }

    fn verify_rt() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
    }

    #[test]
    fn start_creates_a_run_sharing_the_canonical_run_id() {
        let path = tmp_ledger_path("shares-id");
        let run_id = CanonicalRunId::new("canon-run-1".to_string()).unwrap();
        let projector = LiveLedgerProjector::start(
            &path,
            ConversationSelector::New,
            &run_id,
            "claude".to_string(),
            "implementer".to_string(),
        )
        .unwrap();

        let ledger = o7_ledger::SqliteLedger::open(&path).unwrap();
        let fetched = verify_rt()
            .block_on(ledger.run(o7_ledger::RunId::from_raw(run_id.as_str().to_string())))
            .unwrap()
            .unwrap();
        assert_eq!(fetched.run_id.as_str(), run_id.as_str());
        assert_eq!(fetched.status, o7_ledger::RunStatus::Running);
        drop(projector);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn existing_conversation_selector_fails_loudly_on_an_unknown_id() {
        let path = tmp_ledger_path("unknown-conv");
        let run_id = CanonicalRunId::new("canon-run-2".to_string()).unwrap();
        let err = match LiveLedgerProjector::start(
            &path,
            ConversationSelector::Existing("does-not-exist".to_string()),
            &run_id,
            "claude".to_string(),
            "implementer".to_string(),
        ) {
            Ok(_) => panic!("expected an error for an unknown --conversation-id"),
            Err(e) => e,
        };
        assert!(err.to_string().contains("does not resolve"));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn seal_maps_every_verdict_to_the_frozen_ledger_status() {
        for (verdict, expected) in [
            (CanonicalVerdict::Pass, o7_ledger::RunStatus::Completed),
            (CanonicalVerdict::Fail, o7_ledger::RunStatus::Failed),
            (CanonicalVerdict::Blocked, o7_ledger::RunStatus::Blocked),
            (CanonicalVerdict::Error, o7_ledger::RunStatus::Error),
        ] {
            let path = tmp_ledger_path(&format!("seal-{verdict:?}"));
            let run_id = CanonicalRunId::new(format!("canon-run-seal-{verdict:?}")).unwrap();
            let projector = LiveLedgerProjector::start(
                &path,
                ConversationSelector::New,
                &run_id,
                "claude".to_string(),
                "implementer".to_string(),
            )
            .unwrap();
            projector.seal(verdict).unwrap();

            let ledger = o7_ledger::SqliteLedger::open(&path).unwrap();
            let fetched = verify_rt()
                .block_on(ledger.run(o7_ledger::RunId::from_raw(run_id.as_str().to_string())))
                .unwrap()
                .unwrap();
            assert_eq!(fetched.status, expected, "verdict={verdict:?}");
            assert!(fetched.finished_at.is_some(), "sealed runs finish");
            let _ = std::fs::remove_file(&path);
        }
    }

    #[test]
    fn sealing_twice_with_the_same_verdict_is_an_idempotent_no_op() {
        let path = tmp_ledger_path("seal-idempotent");
        let run_id = CanonicalRunId::new("canon-run-seal-idem".to_string()).unwrap();
        let projector = LiveLedgerProjector::start(
            &path,
            ConversationSelector::New,
            &run_id,
            "claude".to_string(),
            "implementer".to_string(),
        )
        .unwrap();
        projector.seal(CanonicalVerdict::Pass).unwrap();
        // A resumed process re-sealing after a crash right after the first
        // seal succeeded must not error.
        projector.seal(CanonicalVerdict::Pass).unwrap();

        let ledger = o7_ledger::SqliteLedger::open(&path).unwrap();
        let fetched = verify_rt()
            .block_on(ledger.run(o7_ledger::RunId::from_raw(run_id.as_str().to_string())))
            .unwrap()
            .unwrap();
        assert_eq!(fetched.status, o7_ledger::RunStatus::Completed);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn sealing_with_a_different_verdict_than_already_recorded_conflicts_loudly() {
        let path = tmp_ledger_path("seal-conflict");
        let run_id = CanonicalRunId::new("canon-run-seal-conflict".to_string()).unwrap();
        let projector = LiveLedgerProjector::start(
            &path,
            ConversationSelector::New,
            &run_id,
            "claude".to_string(),
            "implementer".to_string(),
        )
        .unwrap();
        projector.seal(CanonicalVerdict::Pass).unwrap();

        let err = projector.seal(CanonicalVerdict::Fail).unwrap_err();
        assert!(
            err.to_string()
                .contains("must never be silently overwritten"),
            "got: {err}"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn interrupt_is_not_a_verdict() {
        let path = tmp_ledger_path("interrupt");
        let run_id = CanonicalRunId::new("canon-run-interrupt".to_string()).unwrap();
        let projector = LiveLedgerProjector::start(
            &path,
            ConversationSelector::New,
            &run_id,
            "claude".to_string(),
            "implementer".to_string(),
        )
        .unwrap();
        projector.interrupt().unwrap();

        let ledger = o7_ledger::SqliteLedger::open(&path).unwrap();
        let fetched = verify_rt()
            .block_on(ledger.run(o7_ledger::RunId::from_raw(run_id.as_str().to_string())))
            .unwrap()
            .unwrap();
        assert_eq!(fetched.status, o7_ledger::RunStatus::Interrupted);
        assert!(
            fetched.finished_at.is_none(),
            "interrupted is unsealed — finished_at must stay unset"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn project_carries_provenance_for_every_kind_including_first_and_last_and_is_idempotent() {
        let path = tmp_ledger_path("project-provenance");
        let run_id = CanonicalRunId::new("canon-run-project".to_string()).unwrap();
        let projector = LiveLedgerProjector::start(
            &path,
            ConversationSelector::New,
            &run_id,
            "claude".to_string(),
            "implementer".to_string(),
        )
        .unwrap();

        let stream = tiny_stream(&run_id);
        for event in &stream {
            projector.project(event).unwrap();
        }
        // Re-project the whole stream again — simulates recovery re-applying
        // an already-projected prefix after a crash.
        for event in &stream {
            projector.project(event).unwrap();
        }

        let ledger = o7_ledger::SqliteLedger::open(&path).unwrap();
        let rt = verify_rt();
        let ledger_run = rt
            .block_on(ledger.run(o7_ledger::RunId::from_raw(run_id.as_str().to_string())))
            .unwrap()
            .unwrap();
        let all_events = rt
            .block_on(ledger.read_events(&ledger_run.conversation_id, None, 100))
            .unwrap();
        let notes: Vec<_> = all_events
            .iter()
            .filter(|e| e.event_type == "system.note")
            .collect();
        // RunStarted, AgentStarted, RunSealed — one note each, not
        // duplicated by the second pass, and RunStarted/RunSealed are NOT
        // skipped (unlike the pre-corrective-review behavior).
        assert_eq!(
            notes.len(),
            3,
            "every canonical event gets exactly one provenance note, including first/last: {all_events:?}"
        );
        let kinds: Vec<&str> = notes
            .iter()
            .map(|e| e.payload["canonical_kind"].as_str().unwrap())
            .collect();
        assert_eq!(kinds, vec!["run_started", "agent_started", "run_sealed"]);
        for note in &notes {
            assert!(note.payload["canonical_event_digest"].as_str().is_some());
            assert!(note.payload["canonical_sequence"].as_u64().is_some());
        }
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_resumed_projector_attaches_to_the_existing_run_and_running_attempt_instead_of_duplicating()
    {
        let path = tmp_ledger_path("resume-attach");
        let run_id = CanonicalRunId::new("canon-run-resume".to_string()).unwrap();

        let first = LiveLedgerProjector::start(
            &path,
            ConversationSelector::New,
            &run_id,
            "claude".to_string(),
            "implementer".to_string(),
        )
        .unwrap();
        let first_attempt = first.attempt_id.clone();
        let first_conversation = first.conversation_id().clone();
        drop(first); // simulates the process crashing here

        // A fresh process (e.g. `o7 recover`'s catch-up, or the projector
        // simply being reconstructed) resumes against the same run id.
        let resumed = LiveLedgerProjector::start(
            &path,
            ConversationSelector::New,
            &run_id,
            "claude".to_string(),
            "implementer".to_string(),
        )
        .unwrap();

        assert_eq!(
            resumed.attempt_id, first_attempt,
            "must attach to the SAME running attempt, not create a second one"
        );
        assert_eq!(resumed.conversation_id(), &first_conversation);

        let ledger = o7_ledger::SqliteLedger::open(&path).unwrap();
        let rt = verify_rt();
        let runs = rt
            .block_on(ledger.list_runs(Some(first_conversation), None, None, 50))
            .unwrap();
        assert_eq!(
            runs.items.len(),
            1,
            "resuming must never create a second run"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn attaching_to_an_already_terminal_run_reuses_its_original_attempt_id() {
        let path = tmp_ledger_path("resume-terminal");
        let run_id = CanonicalRunId::new("canon-run-resume-terminal".to_string()).unwrap();

        let first = LiveLedgerProjector::start(
            &path,
            ConversationSelector::New,
            &run_id,
            "claude".to_string(),
            "implementer".to_string(),
        )
        .unwrap();
        let original_attempt = first.attempt_id.clone();
        assert!(original_attempt.is_some());
        first.seal(CanonicalVerdict::Pass).unwrap();

        // Catch-up runs again after the run already sealed (e.g. re-applying
        // a missing tail whose events were all already idempotently
        // applied) — must attach to the SAME original attempt_id, not
        // `None`: `append_system_note`'s idempotency digest includes
        // `attempt_id`, so a `None` here would make replaying an
        // already-projected event (recorded under `Some(original_attempt)`)
        // misfire as a conflict instead of a no-op.
        let resumed = LiveLedgerProjector::start(
            &path,
            ConversationSelector::New,
            &run_id,
            "claude".to_string(),
            "implementer".to_string(),
        )
        .unwrap();
        assert_eq!(resumed.attempt_id, original_attempt);
        // Re-sealing with the SAME verdict is still a safe no-op.
        resumed.seal(CanonicalVerdict::Pass).unwrap();
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_terminal_runs_already_projected_events_replay_idempotently_after_resume() {
        let path = tmp_ledger_path("resume-terminal-reproject");
        let run_id = CanonicalRunId::new("canon-run-resume-reproject".to_string()).unwrap();
        let stream = tiny_stream(&run_id);

        let first = LiveLedgerProjector::start(
            &path,
            ConversationSelector::New,
            &run_id,
            "claude".to_string(),
            "implementer".to_string(),
        )
        .unwrap();
        for event in &stream {
            first.project(event).unwrap();
        }
        first.seal(CanonicalVerdict::Pass).unwrap();
        drop(first); // simulates the process crashing right after sealing

        // A resumed projector (e.g. `o7 recover`'s catch-up) re-projecting
        // the SAME already-applied stream against an already-terminal run
        // must be a genuine no-op, not an idempotency conflict — this is
        // the exact case a naive `attempt_id: None` attach broke.
        let resumed = LiveLedgerProjector::start(
            &path,
            ConversationSelector::New,
            &run_id,
            "claude".to_string(),
            "implementer".to_string(),
        )
        .unwrap();
        for event in &stream {
            resumed.project(event).unwrap();
        }

        let ledger = o7_ledger::SqliteLedger::open(&path).unwrap();
        let rt = verify_rt();
        let ledger_run = rt
            .block_on(ledger.run(o7_ledger::RunId::from_raw(run_id.as_str().to_string())))
            .unwrap()
            .unwrap();
        let all_events = rt
            .block_on(ledger.read_events(&ledger_run.conversation_id, None, 100))
            .unwrap();
        let notes = all_events
            .iter()
            .filter(|e| e.event_type == "system.note")
            .count();
        assert_eq!(
            notes,
            stream.len(),
            "re-projecting an already-applied stream after resume must not duplicate: {all_events:?}"
        );
        let _ = std::fs::remove_file(&path);
    }
}
