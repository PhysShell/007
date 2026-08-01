//! Q-Deck R1 fifth corrective round: a SCOPED, single-record catch-up
//! primitive.
//!
//! `o7 recover` (no `--run-dir`) is a deliberate, operator-triggered GLOBAL
//! scan: `recover_scan`/`mark_interrupted` reclassify every `running`
//! run/attempt in the whole ledger as `interrupted`, and `stuck_commands`/
//! `reconcile_completed_commands` scan every command. That is exactly right
//! for "a process died, go find everything it left behind" — and exactly
//! wrong as a side effect of a single HTTP request recovering ONE sealed
//! child run: an unrelated, genuinely live run/attempt in a DIFFERENT
//! conversation must never be touched just because this request happened
//! to trigger recovery for its own run.
//!
//! [`catch_up_record`] is the primitive this module exists to expose: it
//! re-verifies and re-applies EXACTLY the one canonical record named by
//! `run_dir` — never anything else — and returns a typed
//! [`CatchUpReceipt`]/[`CatchUpError`] instead of printing to stdout, so a
//! caller (`o7d`, or `o7 recover --run-dir` itself, which now calls this
//! SAME function rather than duplicating its logic) can react to the exact
//! outcome instead of parsing CLI output.

use std::path::Path;

use o7_run::ids::RunId as CanonicalRunId;

use crate::agent::ProviderSessionId;
use crate::events;
use crate::ledger_projector::{ConversationSelector, PendingProjection};
use crate::record::{CommandBinding, LedgerBinding, ProviderSessionReceipt};

/// The exact accepted Command a caller expects this canonical record to
/// belong to. When given, `catch_up_record` verifies it INDEPENDENTLY of
/// (and in addition to) whatever check the caller already performed (e.g.
/// `o7d`'s own `classify_child_record`) — a second, independent check at
/// the point of an actual mutating operation is worth the redundancy: a
/// caller that classified correctly a moment ago is not proof the record
/// hasn't changed, and a future caller of this function may not classify
/// at all.
#[derive(Debug, Clone)]
pub struct ExpectedIdentity {
    pub command_id: String,
    pub conversation_id: String,
    pub parent_run_id: String,
    pub child_run_id: String,
    pub command_text: String,
}

/// What happened to the run's provider-session backfill during this
/// catch-up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionBackfillOutcome {
    /// No canonical `ProviderSessionCaptured` receipt exists for this
    /// record (a legacy record, or an agent call that returned no
    /// session) — verdict catch-up still proceeds; the run stays honestly
    /// non-continuable. `meta.json`'s own `session_id` is NEVER consulted
    /// as a substitute.
    NoCanonicalReceipt,
    /// A canonical receipt exists, and the ledger already held the exact
    /// same session — a safe, idempotent no-op.
    AlreadyPresent,
    /// A canonical receipt existed and the ledger had none — backfilled.
    Backfilled,
}

/// Successful result of [`catch_up_record`] — `o7d` may only complete a
/// command's bookkeeping after receiving one of these (`docs/q-deck/r1-command.md`
/// §7), and only for a receipt whose `sealed` is `true`.
#[derive(Debug, Clone)]
pub struct CatchUpReceipt {
    pub run_id: String,
    pub conversation_id: String,
    pub parent_run_id: Option<String>,
    /// The independently re-derived canonical verdict — `None` if the
    /// stream is not sealed.
    pub verdict: Option<o7_run::state::Verdict>,
    pub sealed: bool,
    pub session_backfill: SessionBackfillOutcome,
    /// Whether the ledger projection (re-applying every canonical event)
    /// completed without a live-projection-sink failure. `false` still
    /// means the CANONICAL record itself verified fully — only the ledger
    /// SINK write is in question, exactly like every other live-projection
    /// write in this codebase.
    pub projection_applied: bool,
}

/// Everything that can make [`catch_up_record`] refuse.
#[derive(Debug, thiserror::Error)]
pub enum CatchUpError {
    /// The canonical record itself failed chain/digest/reducer/artifact
    /// verification (the same check `o7 replay` applies once sealed), or a
    /// structural conflict between an unsealed record and an already-sealed
    /// ledger row.
    #[error("canonical record failed verification: {0}")]
    Verification(String),
    /// The record's own identity (`ledger_binding.json`, or — when an
    /// [`ExpectedIdentity`] was given — `command_binding.json`/the task
    /// artifact's digest) disagrees with what this catch-up expected.
    #[error("record identity disagrees with the expected binding: {0}")]
    IdentityMismatch(String),
    /// The ledger already holds a DIFFERENT provider session than this
    /// record's own canonical receipt — never silently overwritten.
    #[error(
        "ledger already holds a different provider session than this record's canonical receipt: {0}"
    )]
    SessionConflict(String),
    /// An underlying ledger operation (open, attach, project, seal) failed.
    #[error("ledger operation failed: {0}")]
    Ledger(String),
}

fn verification(e: impl std::fmt::Display) -> CatchUpError {
    CatchUpError::Verification(e.to_string())
}
fn identity(e: impl std::fmt::Display) -> CatchUpError {
    CatchUpError::IdentityMismatch(e.to_string())
}
fn ledger_err(e: impl std::fmt::Display) -> CatchUpError {
    CatchUpError::Ledger(e.to_string())
}

/// Re-verify EXACTLY one run's canonical `events.jsonl` and re-apply it to
/// the ledger — no global scan, no `recover_scan`/`mark_interrupted`, no
/// `stuck_commands`/`reconcile_completed_commands`. See the module doc
/// comment for why that distinction matters.
///
/// # Errors
/// [`CatchUpError`] — see its own variants. Never panics; never touches any
/// run/attempt/command other than the one `run_dir` names.
pub fn catch_up_record(
    ledger_path: &Path,
    run_dir: &Path,
    expected_identity: Option<&ExpectedIdentity>,
) -> Result<CatchUpReceipt, CatchUpError> {
    let events_text = std::fs::read_to_string(run_dir.join(events::EVENTS_FILE))
        .map_err(|e| verification(format!("reading canonical events.jsonl: {e}")))?;
    let stream = events::from_jsonl(&events_text).map_err(verification)?;
    let resolver = events::RecordDirResolver {
        base: run_dir.to_path_buf(),
    };
    let (state, _artifacts_verified) =
        o7_run::replay::verify_prefix(&stream, &resolver).map_err(|e| {
            verification(format!(
            "chain/digest/reducer/artifacts — the same check `o7 replay` applies once sealed: {e}"
        ))
        })?;
    let canonical_run_id = stream
        .first()
        .map(|e| e.run_id.clone())
        .ok_or_else(|| verification("events.jsonl has no events to catch up"))?;

    let binding = LedgerBinding::read(run_dir)
        .map_err(|e| ledger_err(format!("reading ledger_binding.json: {e:#}")))?;
    if let Some(b) = &binding {
        if b.schema != 1 {
            return Err(identity(format!(
                "ledger_binding.json has unsupported schema {} (expected 1)",
                b.schema
            )));
        }
        if b.run_id != canonical_run_id.as_str() {
            return Err(identity(format!(
                "ledger_binding.json's run_id {} does not match this record's canonical run_id {}",
                b.run_id,
                canonical_run_id.as_str()
            )));
        }
    }

    let existing_run = {
        let ledger = o7_ledger::SqliteLedger::open(ledger_path).map_err(ledger_err)?;
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| ledger_err(format!("starting the catch-up lookup runtime: {e}")))?;
        rt.block_on(ledger.run(o7_ledger::RunId::from_raw(
            canonical_run_id.as_str().to_owned(),
        )))
        .map_err(ledger_err)?
    };

    if state.verdict.is_none() {
        if let Some(run) = &existing_run {
            if !matches!(
                run.status,
                o7_ledger::RunStatus::Queued
                    | o7_ledger::RunStatus::Running
                    | o7_ledger::RunStatus::Interrupted
            ) {
                return Err(verification(format!(
                    "canonical stream is NOT sealed (no fixed verdict yet), but the existing \
                     ledger run {} is already {:?} — refusing to attach an unsealed prefix on \
                     top of a sealed terminal ledger run",
                    canonical_run_id.as_str(),
                    run.status
                )));
            }
        }
    }

    let (conversation, agent, role, parent_run_id_for_attach) = match &existing_run {
        Some(run) => {
            if let Some(b) = &binding {
                if b.conversation_id != run.conversation_id.as_str() {
                    return Err(identity(
                        "ledger_binding.json's conversation_id disagrees with the existing \
                         ledger run's own conversation_id",
                    ));
                }
                if b.agent != run.agent {
                    return Err(identity(
                        "ledger_binding.json's agent disagrees with the existing ledger run's \
                         own agent",
                    ));
                }
                if b.role != run.role {
                    return Err(identity(
                        "ledger_binding.json's role disagrees with the existing ledger run's \
                         own role",
                    ));
                }
                if b.parent_run_id.as_deref()
                    != run.parent_run_id.as_ref().map(o7_ledger::RunId::as_str)
                {
                    return Err(identity(
                        "ledger_binding.json's parent_run_id disagrees with the existing \
                         ledger run's own parent_run_id",
                    ));
                }
            }
            (
                ConversationSelector::Existing(run.conversation_id.as_str().to_owned()),
                run.agent.clone(),
                run.role.clone(),
                run.parent_run_id.as_ref().map(|p| p.as_str().to_owned()),
            )
        }
        None => {
            let b = binding.as_ref().ok_or_else(|| {
                identity(
                    "no existing ledger run row and no ledger_binding.json — refusing to guess \
                     a conversation",
                )
            })?;
            (
                ConversationSelector::Existing(b.conversation_id.clone()),
                b.agent.clone(),
                b.role.clone(),
                b.parent_run_id.clone(),
            )
        }
    };

    // Q-Deck R1 fifth corrective round (Part 3): when a caller supplies the
    // EXACT Command this record is expected to belong to, verify the
    // canonical `command_binding.json` (made tamper-evident by the
    // `CommandBindingCaptured` event `verify_prefix` already digest-checked
    // above) against it — never the other way around. A record with no
    // command binding at all, when one was expected, is refused outright —
    // "legacy missing identity" is not a legitimate case for a command
    // continuation's own child run.
    if let Some(expected) = expected_identity {
        let cb = CommandBinding::read(run_dir)
            .map_err(|e| ledger_err(format!("reading command_binding.json: {e:#}")))?
            .ok_or_else(|| {
                identity("no command_binding.json present for a command-continuation record")
            })?;
        if cb.schema != 1 {
            return Err(identity(format!(
                "command_binding.json has unsupported schema {} (expected 1)",
                cb.schema
            )));
        }
        if cb.command_id != expected.command_id {
            return Err(identity(
                "command_binding.json's command_id disagrees with the expected command",
            ));
        }
        if cb.conversation_id != expected.conversation_id {
            return Err(identity(
                "command_binding.json's conversation_id disagrees with the expected command",
            ));
        }
        if cb.parent_run_id != expected.parent_run_id {
            return Err(identity(
                "command_binding.json's parent_run_id disagrees with the expected command",
            ));
        }
        if cb.child_run_id != expected.child_run_id || cb.child_run_id != canonical_run_id.as_str()
        {
            return Err(identity(
                "command_binding.json's child_run_id disagrees with the expected command or \
                 this record's own canonical run_id",
            ));
        }
        let expected_digest = o7_run::event::Digest256::of_bytes(expected.command_text.as_bytes());
        if cb.command_sha256 != expected_digest.as_str() {
            return Err(identity(
                "command_binding.json's command_sha256 disagrees with the expected command text",
            ));
        }
        // Redundant corroboration: the canonical `RunStarted.task` artifact
        // (already digest-verified by `verify_prefix` above) must ALSO
        // equal the expected command text's own digest.
        if let Some(task) = &state.task {
            if task.digest.as_str() != expected_digest.as_str() {
                return Err(identity(
                    "the canonical task artifact's digest disagrees with the expected command \
                     text",
                ));
            }
        }
    }

    let parent_canonical_run_id_for_attach = parent_run_id_for_attach
        .map(|p| CanonicalRunId::new(p).map_err(|e| identity(format!("parent_run_id: {e}"))))
        .transpose()?;
    let resolved_conversation_id = match &conversation {
        ConversationSelector::Existing(id) => Some(id.clone()),
        ConversationSelector::New => None,
    };

    let pending = PendingProjection::open(ledger_path, conversation, &canonical_run_id)
        .map_err(|e| ledger_err(format!("opening the ledger for catch-up: {e:#}")))?;
    let projector = pending
        .attach_run(
            &canonical_run_id,
            agent,
            role,
            parent_canonical_run_id_for_attach.as_ref(),
        )
        .map_err(|e| ledger_err(format!("attaching the catch-up projector: {e:#}")))?;

    let mut projection_applied = true;
    for event in &stream {
        if projector.project(event).is_err() {
            projection_applied = false;
        }
    }

    let sealed = state.verdict.is_some();
    if let Some(verdict) = state.verdict {
        if projector.seal_or_repair_interrupted(verdict).is_err() {
            projection_applied = false;
        }
    }

    // Q-Deck R1 fifth corrective round (Part 2): session backfill is now
    // driven ENTIRELY by the canonical `ProviderSessionCaptured` receipt —
    // `meta.json`'s own `session_id` is never consulted. `verify_prefix`
    // above already digest-verified the receipt artifact as part of the
    // SAME chain every other artifact gets, so reading it back here is
    // safe: its bytes are already proven to match the canonical event's
    // own referenced digest.
    let session_backfill = if let Some(receipt_ref) = &state.provider_session_receipt {
        let receipt_bytes = std::fs::read(run_dir.join(&receipt_ref.locator))
            .map_err(|e| ledger_err(format!("re-reading session_receipt.json: {e}")))?;
        let receipt: ProviderSessionReceipt = serde_json::from_slice(&receipt_bytes)
            .map_err(|e| identity(format!("parsing session_receipt.json: {e}")))?;
        if receipt.schema != 1 {
            return Err(identity(format!(
                "session_receipt.json has unsupported schema {} (expected 1)",
                receipt.schema
            )));
        }
        if receipt.run_id != canonical_run_id.as_str() {
            return Err(identity(
                "session_receipt.json's run_id disagrees with this record's own canonical run_id",
            ));
        }
        if receipt.engine != "claude" || receipt.provider != "claude" {
            return Err(identity(
                "session_receipt.json's engine/provider is not a supported continuation path",
            ));
        }
        let session = ProviderSessionId::new(receipt.session_id.clone())
            .map_err(|e| identity(format!("session_receipt.json's own session_id: {e}")))?;

        let current_session = {
            let ledger = o7_ledger::SqliteLedger::open(ledger_path).map_err(ledger_err)?;
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|e| ledger_err(format!("starting the session-backfill runtime: {e}")))?;
            rt.block_on(ledger.run(o7_ledger::RunId::from_raw(
                canonical_run_id.as_str().to_owned(),
            )))
            .map_err(ledger_err)?
            .and_then(|r| r.provider_session_id)
        };

        match current_session {
            None => {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|e| {
                        ledger_err(format!("starting the session-backfill write runtime: {e}"))
                    })?;
                let ledger = o7_ledger::SqliteLedger::open(ledger_path).map_err(ledger_err)?;
                rt.block_on(ledger.set_run_provider_session(
                    o7_ledger::RunId::from_raw(canonical_run_id.as_str().to_owned()),
                    session.as_str().to_owned(),
                ))
                .map_err(ledger_err)?;
                SessionBackfillOutcome::Backfilled
            }
            Some(existing) if existing == session.as_str() => {
                SessionBackfillOutcome::AlreadyPresent
            }
            Some(_) => {
                return Err(CatchUpError::SessionConflict(
                    "the ledger already holds a session for this run that disagrees with its \
                     own canonical receipt — refusing to overwrite"
                        .to_owned(),
                ));
            }
        }
    } else {
        SessionBackfillOutcome::NoCanonicalReceipt
    };

    Ok(CatchUpReceipt {
        run_id: canonical_run_id.as_str().to_owned(),
        conversation_id: resolved_conversation_id.unwrap_or_default(),
        parent_run_id: parent_canonical_run_id_for_attach
            .as_ref()
            .map(|p| p.as_str().to_owned()),
        verdict: if sealed { state.verdict } else { None },
        sealed,
        session_backfill,
        projection_applied,
    })
}
