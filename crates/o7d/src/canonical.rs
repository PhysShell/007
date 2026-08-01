//! Q-Deck R1 fourth corrective round: classify an existing command's bound
//! child run's own canonical record BEFORE ever deciding to redrive it.
//!
//! Ledger status alone can never authorize re-invoking the provider a
//! second time — a stale or not-yet-caught-up ledger projection can show
//! `queued`/`running`/`interrupted` for a child run whose canonical
//! `events.jsonl` is already fully sealed. This module answers that one
//! question — is the flat record itself sealed, still in progress, absent,
//! or invalid? — using the SAME chain/digest/reducer/artifact verification
//! [`o7_run::replay::classify_record`] applies for every other caller in
//! this codebase (`o7 replay`, `o7 recover --run-dir`), never a second,
//! lighter-weight parser.

use std::path::PathBuf;

use crate::state::ExecutionConfig;

/// The canonical run-event stream's file name inside a run record — same
/// literal the root `o7` binary's `events::EVENTS_FILE` names (kept as a
/// plain literal here rather than a cross-crate constant: this module reads
/// nothing else about the record's on-disk layout that isn't already
/// re-derived from `o7-run`'s own shared [`o7_run::replay::RecordDirResolver`]).
const EVENTS_FILE: &str = "events.jsonl";

/// Mirrors the root `o7` binary's `record::LedgerBinding` — deliberately
/// duplicated rather than shared across crates (same call this codebase
/// already made for `command_lock_path`, a two-line helper): `o7d` must
/// never depend on the root `o7` BINARY crate (it isn't a library), and
/// pulling in the handful of fields this module actually reads is simpler
/// than restructuring where `LedgerBinding` lives.
#[derive(Debug, serde::Deserialize)]
struct LedgerBindingFile {
    schema: u32,
    run_id: String,
    conversation_id: String,
    agent: String,
    role: String,
}

/// The schema `o7 continue`'s own `LedgerBinding`/`RunMeta` writers always
/// use — see `src/record.rs` in the root crate.
const SUPPORTED_BINDING_SCHEMA: u32 = 1;

/// The only agent/role a command continuation's own canonical record is
/// ever durably bound to (`continue_execute`, root crate) — a record
/// claiming anything else is not a supported continuation path, tampered
/// or otherwise foreign.
const SUPPORTED_AGENT: &str = "claude";
const SUPPORTED_ROLE: &str = "implementer";

/// The result of classifying a bound child run's own canonical record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ChildRecordState {
    /// No canonical stream at all — never started (or the record was
    /// never durably created), safe to treat as redrivable.
    Absent,
    /// A valid, fully verified prefix with no fixed verdict yet — genuinely
    /// still in progress or crashed before sealing; safe to redrive with a
    /// fresh id once its process is provably dead.
    ValidUnsealed,
    /// A valid, fully verified, SEALED record — the provider already ran
    /// to completion. MUST be recovered, never redriven.
    ValidSealed,
    /// The record is non-empty but fails verification, or its identity
    /// disagrees with the binding this redrive decision is about (a
    /// mismatched `run_id`, a foreign/missing `ledger_binding.json`, an
    /// unsupported schema, or an agent/role outside the one supported
    /// continuation path). MUST fail closed — never treated as either
    /// "never started" or "already sealed".
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

/// Classify `run_id`'s own canonical record — the shared decision point
/// behind Q-Deck R1's redrive path (`routes.rs::redrive_or_recover_locked`).
///
/// `expected_conversation_id` is the conversation the COMMAND being
/// redriven belongs to (never taken from the record itself) — the
/// record's own `ledger_binding.json` must agree with it before its
/// evidence is trusted for anything.
pub(crate) fn classify_child_record(
    exec: &ExecutionConfig,
    run_id: &str,
    expected_conversation_id: &str,
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
        // the same "no canonical events" case `classify_record` itself
        // reports as `NoCanonicalStream`, checked here too so a truly
        // empty file never has to reach the verifier to be treated as
        // absent.
        return ChildRecordState::Absent;
    }
    if events.first().map(|e| e.run_id.as_str()) != Some(run_id) {
        return ChildRecordState::Invalid(format!(
            "canonical record's own run_id does not match the bound child run id {run_id}"
        ));
    }

    let resolver = o7_run::replay::RecordDirResolver { base: dir.clone() };
    let sealed = match o7_run::replay::classify_record(&events, &resolver) {
        o7_run::replay::RecordVerdict::NoCanonicalStream => return ChildRecordState::Absent,
        o7_run::replay::RecordVerdict::Invalid(e) => {
            return ChildRecordState::Invalid(e.to_string())
        }
        o7_run::replay::RecordVerdict::ValidUnsealed => false,
        o7_run::replay::RecordVerdict::ValidSealed(_) => true,
    };

    // Q-Deck R1 fourth corrective round: a genuine command-continuation
    // record ALWAYS durably writes `ledger_binding.json` before `RunStarted`
    // (`continue_execute`, root crate, unconditionally) — its absence here,
    // once the canonical stream itself is non-empty and independently
    // verified, is itself a red flag (a foreign or hand-assembled record),
    // never a legitimate case to fall back to guessing identity for.
    let binding_text = match std::fs::read_to_string(dir.join("ledger_binding.json")) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return ChildRecordState::Invalid(
                "ledger_binding.json is missing for a non-empty canonical record".to_owned(),
            )
        }
        Err(e) => return ChildRecordState::Invalid(format!("reading ledger_binding.json: {e}")),
    };
    let binding: LedgerBindingFile = match serde_json::from_str(&binding_text) {
        Ok(binding) => binding,
        Err(e) => return ChildRecordState::Invalid(format!("parsing ledger_binding.json: {e}")),
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
    if binding.conversation_id != expected_conversation_id {
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

    if sealed {
        ChildRecordState::ValidSealed
    } else {
        ChildRecordState::ValidUnsealed
    }
}
