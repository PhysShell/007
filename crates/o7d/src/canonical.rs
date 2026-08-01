//! Q-Deck R1 fourth/fifth/sixth corrective rounds: classify an existing
//! command's bound child run's own canonical record BEFORE ever deciding to
//! redrive or recover it.
//!
//! Ledger status alone can never authorize re-invoking the provider a
//! second time — a stale or not-yet-caught-up ledger projection can show
//! `queued`/`running`/`interrupted` for a child run whose canonical
//! `events.jsonl` is already fully sealed. Nor is a record's mere
//! `run_id`/`conversation_id` match enough (fifth corrective round): the
//! record must be tied to the EXACT accepted `Command` that dispatched it.
//! Nor, once dispatched, is an unsealed outcome alone enough to redrive
//! (sixth corrective round): the actual classification logic —
//! [`ChildRecordState`]/[`DispatchProgress`] and the check itself — lives in
//! the root `o7` crate's `recovery` module (`o7::recovery::classify_command_child`)
//! so `o7d`'s own redrive decision and `o7 recover`'s read-only operator
//! discovery reporting share ONE classifier, never two that could drift.
//! This module is now a thin, `o7d`-specific wrapper: it resolves the
//! record directory from server-owned execution configuration and the
//! already-durably-bound `run_id`, then delegates.

use std::path::PathBuf;

pub(crate) use o7::recovery::{ChildRecordState, DispatchProgress};

use crate::state::ExecutionConfig;

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

/// Classify `run_id`'s own canonical record against the EXACT accepted
/// `command` this redrive/recovery decision is about — the shared decision
/// point behind Q-Deck R1's redrive path
/// (`routes.rs::redrive_or_recover_locked`).
pub(crate) fn classify_child_record(
    exec: &ExecutionConfig,
    run_id: &str,
    command: &o7_ledger::Command,
) -> ChildRecordState {
    match child_record_dir(exec, run_id) {
        Ok(dir) => o7::recovery::classify_command_child(&dir, run_id, command),
        Err(e) => ChildRecordState::Invalid(format!("resolving the record directory: {e}")),
    }
}
