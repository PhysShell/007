//! Q-Deck R1 fourth/fifth corrective rounds: classify an existing command's
//! bound child run's own canonical record BEFORE ever deciding to redrive
//! or recover it.
//!
//! Ledger status alone can never authorize re-invoking the provider a
//! second time — a stale or not-yet-caught-up ledger projection can show
//! `queued`/`running`/`interrupted` for a child run whose canonical
//! `events.jsonl` is already fully sealed. Nor is a record's mere
//! `run_id`/`conversation_id` match enough (fifth corrective round): the
//! record must be tied to the EXACT accepted `Command` that dispatched it —
//! `command_id`, `parent_run_id`, and the command TEXT itself, corroborated
//! twice (the canonical task artifact's own bytes, and its declared digest)
//! — never merely "some run in this conversation with a plausible shape".
//!
//! This module answers one question — is the flat record absent, a valid
//! unsealed prefix, a valid sealed record, or invalid — using the SAME
//! chain/digest/reducer/artifact verification (`o7_run::replay::verify_prefix`)
//! every other caller in this codebase (`o7 replay`, `o7 recover --run-dir`)
//! is built on, never a second, lighter-weight parser. `classify_record`'s
//! own convenience wrapper doesn't expose the full reduced state this
//! module needs (the canonical `task`/`command_binding` artifacts), so this
//! calls `verify_prefix` directly — the SAME primitive, not a substitute.

use std::path::PathBuf;

use o7::record::{CommandBinding, LedgerBinding};

use crate::state::ExecutionConfig;

/// The canonical run-event stream's file name inside a run record — same
/// literal the root `o7` binary's `events::EVENTS_FILE` names.
const EVENTS_FILE: &str = "events.jsonl";

/// The only agent/role a command continuation's own canonical record is
/// ever durably bound to (`continue_execute`, root crate) — a record
/// claiming anything else is not a supported continuation path, tampered
/// or otherwise foreign.
const SUPPORTED_AGENT: &str = "claude";
const SUPPORTED_ROLE: &str = "implementer";
const SUPPORTED_BINDING_SCHEMA: u32 = 1;

/// The result of classifying a bound child run's own canonical record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ChildRecordState {
    /// No canonical stream at all — never started (or the record was
    /// never durably created), safe to treat as redrivable.
    Absent,
    /// A valid, fully verified prefix with no fixed verdict yet, whose
    /// identity matches the exact Command this decision is about — safe to
    /// redrive with a fresh id once its process is provably dead.
    ValidUnsealed,
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

/// The run-store `target` label a command's child run record lives under —
/// `o7d` never passes `--target` when spawning `o7 continue`
/// (`spawn_continue`), so this MUST replicate `continue_run`'s own
/// default-target derivation exactly (`src/main.rs`, root crate): the
/// canonicalized configured repo's own final path component.
fn child_target(exec: &ExecutionConfig) -> std::io::Result<String> {
    let repo = exec.repo.canonicalize()?;
    Ok(repo
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "target".to_owned()))
}

/// The exact record directory a child run's canonical stream lives in —
/// computed ONLY from server-owned execution configuration (`exec`) and the
/// already-durably-bound `run_id`, never from anything in the HTTP request.
pub(crate) fn child_record_dir(exec: &ExecutionConfig, run_id: &str) -> std::io::Result<PathBuf> {
    Ok(exec.runs_dir.join(child_target(exec)?).join(run_id))
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

/// Classify `run_id`'s own canonical record against the EXACT accepted
/// `command` this redrive/recovery decision is about — the shared decision
/// point behind Q-Deck R1's redrive path
/// (`routes.rs::redrive_or_recover_locked`).
pub(crate) fn classify_child_record(
    exec: &ExecutionConfig,
    run_id: &str,
    command: &o7_ledger::Command,
) -> ChildRecordState {
    let dir = match child_record_dir(exec, run_id) {
        Ok(d) => d,
        Err(e) => return ChildRecordState::Invalid(format!("resolving the record directory: {e}")),
    };

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
    let resolver = o7_run::replay::RecordDirResolver { base: dir.clone() };
    let (state, _artifacts_verified) = match o7_run::replay::verify_prefix(&events, &resolver) {
        Ok(ok) => ok,
        Err(e) => return ChildRecordState::Invalid(e.to_string()),
    };
    let sealed = state.verdict.is_some();

    // Q-Deck R1 fourth corrective round: a genuine command-continuation
    // record ALWAYS durably writes `ledger_binding.json` before `RunStarted`
    // (`continue_execute`, root crate, unconditionally) — its absence here,
    // once the canonical stream itself is non-empty and independently
    // verified, is itself a red flag (a foreign or hand-assembled record),
    // never a legitimate case to fall back to guessing identity for.
    let binding = match LedgerBinding::read(&dir) {
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
    let command_binding = match CommandBinding::read(&dir) {
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
    // one of the artifacts every sealed OR unsealed prefix has resolved
    // and content-checked) must ALSO equal this command's own text, both
    // by declared digest AND by reading its actual bytes back.
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
        ChildRecordState::ValidUnsealed
    }
}
