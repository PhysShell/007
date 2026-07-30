//! Q-Deck R0.7 (`docs/q-deck/r07-live-ingress.md`): projects `o7-run`'s
//! canonical `RunEvent` stream into `o7-ledger`, live, as each event is
//! minted — never a second reducer, never a recomputed verdict.
//!
//! A thin SYNC facade over `o7-ledger`'s async API (an internal
//! current-thread tokio runtime): the root `o7 run` CLI stays exactly as
//! synchronous as it is today for everything except this one new sink —
//! `execute()` never needs `async`/`.await` of its own.

use std::path::Path;

use anyhow::{Context, Result};

use o7_ledger::{
    AttemptId, ConversationId, Idempotency, NewRun, RunId as LedgerRunId, SqliteLedger,
};
use o7_run::event::{RunEvent, RunEventKind};
use o7_run::ids::RunId as CanonicalRunId;
use o7_run::state::Verdict as CanonicalVerdict;

/// How the projector resolves the ledger conversation for a run (section
/// 2.4: never an implicit "most recent conversation").
pub enum ConversationSelector {
    /// No `--conversation-id` given: create a fresh ledger conversation.
    New,
    /// `--conversation-id <id>` given: must already resolve to a real
    /// conversation, or `LiveLedgerProjector::start` fails loudly — never
    /// silently creates one under the caller-supplied id.
    Existing(String),
}

/// Owns exactly one live run's projection into `o7-ledger`.
pub struct LiveLedgerProjector {
    rt: tokio::runtime::Runtime,
    ledger: SqliteLedger,
    conversation_id: ConversationId,
    run_id: LedgerRunId,
    attempt_id: AttemptId,
}

impl LiveLedgerProjector {
    /// Open `ledger_path`, resolve/create the conversation, and create the
    /// ledger run sharing `run_id` verbatim with `o7-run`'s own canonical
    /// stream (never a second, ledger-generated identity). Must be called
    /// once, before the run's first canonical event.
    ///
    /// # Errors
    /// The ledger path cannot be opened / fails schema attestation; a given
    /// `--conversation-id` does not resolve to a real conversation; any
    /// underlying ledger error.
    pub fn start(
        ledger_path: &Path,
        conversation: ConversationSelector,
        run_id: &CanonicalRunId,
        agent: String,
        role: String,
    ) -> Result<Self> {
        let ledger = SqliteLedger::open(ledger_path)
            .with_context(|| format!("opening ledger at {}", ledger_path.display()))?;
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .context("starting the ledger projector's runtime")?;

        let conversation_id = match conversation {
            ConversationSelector::New => {
                rt.block_on(ledger.create_conversation(None))
                    .context("creating a new ledger conversation")?
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

        let shared_run_id = LedgerRunId::from_raw(run_id.as_str().to_owned());
        let run = rt
            .block_on(ledger.create_run_with_id(
                NewRun {
                    conversation_id: conversation_id.clone(),
                    parent_run_id: None,
                    agent,
                    role,
                },
                shared_run_id,
                Idempotency {
                    key: run_id.as_str().to_owned(),
                },
            ))
            .with_context(|| format!("creating ledger run {}", run_id.as_str()))?;

        rt.block_on(ledger.start_run(run.run_id.clone()))
            .with_context(|| format!("starting ledger run {}", run_id.as_str()))?;

        let attempt = rt
            .block_on(ledger.create_attempt(run.run_id.clone()))
            .with_context(|| format!("creating ledger attempt for run {}", run_id.as_str()))?;

        Ok(Self {
            rt,
            ledger,
            conversation_id,
            run_id: run.run_id,
            attempt_id: attempt.attempt_id,
        })
    }

    /// Project one canonical event, in stream order. `RunStarted` and
    /// `RunSealed` are no-ops here — `start` already created/started the
    /// run, and the terminal status is applied only by `seal`/`interrupt`,
    /// never inferred from a stream event — so it is safe to call this for
    /// every event in the stream, including the first and last, uniformly.
    ///
    /// Idempotent: safe to call twice for the same event (keyed by this
    /// run's id + the event's own canonical `sequence`).
    ///
    /// # Errors
    /// Any underlying ledger error.
    pub fn project(&self, event: &RunEvent) -> Result<()> {
        let (kind_name, detail) = match &event.kind {
            RunEventKind::RunStarted { .. } | RunEventKind::RunSealed => return Ok(()),
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
            "canonical_sequence": event.sequence,
            "canonical_event_digest": event.event_digest.as_str(),
            "canonical_schema_version": event.schema_version,
            "detail": detail,
        });

        self.rt
            .block_on(self.ledger.append_system_note(
                self.conversation_id.clone(),
                Some(self.run_id.clone()),
                Some(self.attempt_id.clone()),
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
    /// exactly once, after the `RunSealed` event has been projected, with
    /// the verdict `o7_run::reduce::reduce_all` already produced. Never
    /// recomputed here — this is a pure application of a decision already
    /// made by the canonical reducer. `o7-ledger`'s own `set_run_status`
    /// finishes this run's running attempt with the matching
    /// `AttemptStatus` as part of the same transaction, so no separate
    /// attempt call is needed.
    ///
    /// # Errors
    /// Any underlying ledger error.
    pub fn seal(&self, verdict: CanonicalVerdict) -> Result<()> {
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

    /// A small, separate (non-nested) runtime purely for these tests' own
    /// direct ledger reads used to verify what the projector — which runs
    /// its OWN internal runtime — actually did. Never used concurrently
    /// with a projector call in the same test.
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
    fn project_skips_run_started_and_run_sealed_and_is_idempotent_for_the_rest() {
        let path = tmp_ledger_path("project-idempotent");
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
        // conversation.created + run.created + run.started + exactly ONE
        // system.note for AgentStarted (RunStarted/RunSealed are no-ops in
        // `project`, and the second pass over the whole stream must not
        // have duplicated the AgentStarted note).
        let notes: Vec<_> = all_events
            .iter()
            .filter(|e| e.event_type == "system.note")
            .collect();
        assert_eq!(
            notes.len(),
            1,
            "re-projecting the same stream must not duplicate events: {all_events:?}"
        );
        let _ = std::fs::remove_file(&path);
    }
}
