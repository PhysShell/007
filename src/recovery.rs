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

/// The canonical run-event stream's file name inside a run record.
const EVENTS_FILE: &str = "events.jsonl";

/// The only agent/role a command continuation's own canonical record is
/// ever durably bound to (`continue_execute`, this same crate) — a record
/// claiming anything else is not a supported continuation path, tampered
/// or otherwise foreign.
const SUPPORTED_AGENT: &str = "claude";
const SUPPORTED_ROLE: &str = "implementer";
const SUPPORTED_BINDING_SCHEMA: u32 = 1;

/// Q-Deck R1 sixth corrective round: how far an unsealed record's own
/// verified/reduced canonical state has progressed past the durable
/// dispatch boundary (`AgentStarted` — see its own doc comment in
/// `o7_run::event::RunEventKind` for exactly why that event, and only that
/// event, is conservative enough to serve as this boundary). Derived
/// ENTIRELY from already-reduced `RunState` fields — never by searching
/// `events.jsonl` text — because the reducer has already done the one
/// thing that matters here: proven the prefix is genuine.
///
/// Ordered most-to-least advanced; a caller only needs the FURTHEST
/// progress observed; each stage is strictly more evidence than the last:
/// - [`Self::AgentStarted`] — a live provider invocation *may* now have
///   happened. Absence of `AgentExited` is NOT proof it didn't.
/// - [`Self::AgentExited`] — the provider ran and returned SOME outcome.
/// - [`Self::ProviderSessionCaptured`] — that, plus a session identity was
///   durably captured.
/// - [`Self::PostProviderWork`] — a patch and/or gate steps were captured
///   after the provider — the furthest an unsealed record can get short of
///   `RunSealed` itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DispatchProgress {
    AgentStarted,
    AgentExited,
    ProviderSessionCaptured,
    PostProviderWork,
}

impl std::fmt::Display for DispatchProgress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::AgentStarted => "agent_started",
            Self::AgentExited => "agent_exited",
            Self::ProviderSessionCaptured => "provider_session_captured",
            Self::PostProviderWork => "post_provider_work",
        };
        f.write_str(s)
    }
}

/// Reads the furthest [`DispatchProgress`] an already-verified/reduced
/// `RunState` shows — `None` means the durable dispatch boundary
/// (`AgentStarted`) has not been reached at all, the ONLY case safe to
/// redrive automatically.
fn dispatch_progress(state: &o7_run::state::RunState) -> Option<DispatchProgress> {
    if state.patch.is_some() || !state.gates.is_empty() {
        Some(DispatchProgress::PostProviderWork)
    } else if state.provider_session_receipt.is_some() {
        Some(DispatchProgress::ProviderSessionCaptured)
    } else if matches!(state.agent, o7_run::state::AgentLifecycle::Exited { .. }) {
        Some(DispatchProgress::AgentExited)
    } else if matches!(state.agent, o7_run::state::AgentLifecycle::Started) {
        Some(DispatchProgress::AgentStarted)
    } else {
        None
    }
}

/// The result of classifying a bound child run's own canonical record
/// against the EXACT accepted Command it is expected to belong to — the
/// shared decision point behind Q-Deck R1's redrive path
/// (`crates/o7d/src/routes.rs::redrive_or_recover_locked`) AND `o7
/// recover`'s own read-only operator-discovery reporting (never a second,
/// diverging classifier for either caller).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChildRecordState {
    /// No canonical stream at all — never started (or the record was
    /// never durably created), safe to treat as redrivable.
    Absent,
    /// A valid, fully verified prefix with no fixed verdict yet, whose
    /// identity matches the exact Command this decision is about, AND
    /// whose canonical state has not yet reached the durable dispatch
    /// boundary (`AgentStarted`) — the provider has certainly NOT been
    /// invoked for this record. Safe to redrive with a fresh id once its
    /// process is provably dead.
    ValidUnsealedPreDispatch,
    /// Q-Deck R1 sixth corrective round: a valid, fully verified prefix
    /// with no fixed verdict yet, whose canonical state HAS reached or
    /// passed the durable dispatch boundary — the provider MAY already
    /// have been invoked for this exact run id, and an unsealed outcome
    /// alone can never prove otherwise (no `AgentExited`, no session
    /// receipt, and no provider output are proof of non-invocation; their
    /// ABSENCE is exactly the ambiguity, not evidence against it). MUST
    /// NEVER be automatically redriven — doing so risks a second, real
    /// provider invocation for a request that may already have one in
    /// flight or completed silently. Fails closed; requires manual
    /// resolution.
    ValidUnsealedDispatchAmbiguous { progress: DispatchProgress },
    /// A valid, fully verified, SEALED record whose identity matches the
    /// exact Command this decision is about — the provider already ran to
    /// completion. MUST be recovered, never redriven.
    ValidSealed,
    /// The record is non-empty but fails verification, OR its identity
    /// disagrees with the EXACT Command this decision is about (a
    /// mismatched canonical `run_id`, a foreign/missing
    /// `ledger_binding.json`/`command_binding.json`, a wrong `command_id`,
    /// a wrong `parent_run_id`, command TEXT that disagrees with the
    /// canonical task artifact, an unsupported schema, or an agent/role
    /// outside the one supported continuation path). MUST fail closed —
    /// never treated as either "never started" or "already sealed", no
    /// matter how plausible the record otherwise looks.
    Invalid(String),
}

fn parse_events(text: &str) -> Result<Vec<o7_run::event::RunEvent>, String> {
    let mut events = Vec::new();
    for (i, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<o7_run::event::RunEvent>(line) {
            Ok(event) => events.push(event),
            Err(e) => return Err(format!("events.jsonl line {}: {e}", i.saturating_add(1))),
        }
    }
    Ok(events)
}

/// Classify `run_id`'s own canonical record (living in `dir`) against the
/// EXACT accepted `command` this decision is about. Never returns an
/// `Err` — every failure is a typed [`ChildRecordState::Invalid`], so a
/// caller can pattern-match one result.
#[must_use]
pub fn classify_command_child(
    dir: &Path,
    run_id: &str,
    command: &o7_ledger::Command,
) -> ChildRecordState {
    let events_text = match std::fs::read_to_string(dir.join(EVENTS_FILE)) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return ChildRecordState::Absent,
        Err(e) => {
            return ChildRecordState::Invalid(format!(
                "reading {}: {e}",
                dir.join(EVENTS_FILE).display()
            ))
        }
    };
    let events = match parse_events(&events_text) {
        Ok(events) => events,
        Err(reason) => return ChildRecordState::Invalid(reason),
    };
    if events.is_empty() {
        // An empty (or whitespace-only) file is not a canonical stream —
        // the same "no canonical events" case the shared verifier itself
        // reports, checked here too so a truly empty file never has to
        // reach it to be treated as absent.
        return ChildRecordState::Absent;
    }
    if events.first().map(|e| e.run_id.as_str()) != Some(run_id) {
        return ChildRecordState::Invalid(format!(
            "canonical record's own run_id does not match the bound child run id {run_id}"
        ));
    }

    // The SAME chain/digest/reducer/artifact verification `classify_record`
    // itself is built on — called directly here (not through that
    // convenience wrapper) because this classification needs the full
    // reduced state (`task`, `command_binding`), not merely its summary.
    let resolver = events::RecordDirResolver {
        base: dir.to_path_buf(),
    };
    let (state, _artifacts_verified) = match o7_run::replay::verify_prefix(&events, &resolver) {
        Ok(ok) => ok,
        Err(e) => return ChildRecordState::Invalid(e.to_string()),
    };
    let sealed = state.verdict.is_some();

    // Q-Deck R1 fourth corrective round: a genuine command-continuation
    // record ALWAYS durably writes `ledger_binding.json` before `RunStarted`
    // (`continue_execute`, unconditionally) — its absence here, once the
    // canonical stream itself is non-empty and independently verified, is
    // itself a red flag (a foreign or hand-assembled record), never a
    // legitimate case to fall back to guessing identity for.
    let binding = match LedgerBinding::read(dir) {
        Ok(Some(b)) => b,
        Ok(None) => {
            return ChildRecordState::Invalid(
                "ledger_binding.json is missing for a non-empty canonical record".to_owned(),
            )
        }
        Err(e) => return ChildRecordState::Invalid(format!("reading ledger_binding.json: {e:#}")),
    };
    if binding.schema != SUPPORTED_BINDING_SCHEMA {
        return ChildRecordState::Invalid(format!(
            "ledger_binding.json has unsupported schema {} (expected {SUPPORTED_BINDING_SCHEMA})",
            binding.schema
        ));
    }
    if binding.run_id != run_id {
        return ChildRecordState::Invalid(
            "ledger_binding.json's run_id does not match the bound child run id".to_owned(),
        );
    }
    if binding.conversation_id != command.conversation_id.as_str() {
        return ChildRecordState::Invalid(
            "ledger_binding.json's conversation_id does not match this command's conversation"
                .to_owned(),
        );
    }
    if binding.agent != SUPPORTED_AGENT || binding.role != SUPPORTED_ROLE {
        return ChildRecordState::Invalid(
            "ledger_binding.json's agent/role is not a supported continuation path".to_owned(),
        );
    }
    if binding.parent_run_id.as_deref() != Some(command.parent_run_id.as_str()) {
        return ChildRecordState::Invalid(
            "ledger_binding.json's parent_run_id does not match this command's parent_run_id"
                .to_owned(),
        );
    }

    // Q-Deck R1 fifth corrective round (Part 3): the record must be tied to
    // the EXACT accepted Command, not merely "a run in the right
    // conversation with the right parent" — a `command_binding.json`,
    // itself made tamper-evident by the canonical `CommandBindingCaptured`
    // event `verify_prefix` above already digest-verified.
    let command_binding = match CommandBinding::read(dir) {
        Ok(Some(b)) => b,
        Ok(None) => {
            return ChildRecordState::Invalid(
                "command_binding.json is missing for a non-empty canonical record".to_owned(),
            )
        }
        Err(e) => return ChildRecordState::Invalid(format!("reading command_binding.json: {e:#}")),
    };
    if command_binding.schema != SUPPORTED_BINDING_SCHEMA {
        return ChildRecordState::Invalid(format!(
            "command_binding.json has unsupported schema {} (expected {SUPPORTED_BINDING_SCHEMA})",
            command_binding.schema
        ));
    }
    if command_binding.command_id != command.command_id.as_str() {
        return ChildRecordState::Invalid(
            "command_binding.json's command_id does not match the expected command".to_owned(),
        );
    }
    if command_binding.conversation_id != command.conversation_id.as_str() {
        return ChildRecordState::Invalid(
            "command_binding.json's conversation_id does not match the expected command".to_owned(),
        );
    }
    if command_binding.parent_run_id != command.parent_run_id.as_str() {
        return ChildRecordState::Invalid(
            "command_binding.json's parent_run_id does not match the expected command".to_owned(),
        );
    }
    if command_binding.child_run_id != run_id {
        return ChildRecordState::Invalid(
            "command_binding.json's child_run_id does not match the bound child run id".to_owned(),
        );
    }
    let expected_digest = o7_run::event::Digest256::of_bytes(command.command_text.as_bytes());
    if command_binding.command_sha256 != expected_digest.as_str() {
        return ChildRecordState::Invalid(
            "command_binding.json's command_sha256 does not match this command's own text"
                .to_owned(),
        );
    }
    // Redundant corroboration: the canonical `RunStarted.task` artifact
    // (already digest-verified by `verify_prefix` above, since `task` is
    // one of the artifacts every sealed OR unsealed prefix has resolved and
    // content-checked) must ALSO equal this command's own text, both by
    // declared digest AND by reading its actual bytes back.
    match &state.task {
        Some(task) => {
            if task.digest.as_str() != expected_digest.as_str() {
                return ChildRecordState::Invalid(
                    "the canonical task artifact's digest does not match this command's own text"
                        .to_owned(),
                );
            }
            match std::fs::read(dir.join(&task.locator)) {
                Ok(bytes) if bytes == command.command_text.as_bytes() => {}
                Ok(_) => {
                    return ChildRecordState::Invalid(
                        "the canonical task artifact's own bytes do not match this command's \
                         text"
                            .to_owned(),
                    )
                }
                Err(e) => {
                    return ChildRecordState::Invalid(format!(
                        "re-reading the canonical task artifact: {e}"
                    ))
                }
            }
        }
        None => {
            return ChildRecordState::Invalid(
                "canonical record has no task artifact to corroborate the command text".to_owned(),
            )
        }
    }

    if sealed {
        ChildRecordState::ValidSealed
    } else {
        match dispatch_progress(&state) {
            None => ChildRecordState::ValidUnsealedPreDispatch,
            Some(progress) => ChildRecordState::ValidUnsealedDispatchAmbiguous { progress },
        }
    }
}

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
