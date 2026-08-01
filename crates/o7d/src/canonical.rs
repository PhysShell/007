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

/// Q-Deck A0 (`docs/q-deck/a0-candidate-state.md` §8): a read-only,
/// best-effort candidate-state projection for `routes::get_run` — never
/// mutates anything, never gates a redrive decision (that stays exactly
/// `classify_child_record`/`o7::recovery::classify_command_child`, above,
/// untouched by this). Returns `(candidate_source_run_id, candidate_tree_oid,
/// materialization_status)`; all `None`/`"not_applicable"` for a run with no
/// canonical record yet, or one that never attempted materialization (a
/// top-level `o7 run`, or a continuation still pre-dispatch).
pub(crate) fn candidate_projection(
    exec: &ExecutionConfig,
    run_id: &str,
) -> (Option<String>, Option<String>, Option<String>) {
    let dir = match child_record_dir(exec, run_id) {
        Ok(dir) => dir,
        Err(e) => {
            return (
                None,
                None,
                Some(format!("failed: resolving record directory: {e}")),
            )
        }
    };
    let text = match std::fs::read_to_string(dir.join("events.jsonl")) {
        Ok(text) => text,
        Err(_) => return (None, None, Some("not_applicable".to_owned())),
    };
    for line in text.lines().rev() {
        if line.trim().is_empty() {
            continue;
        }
        let Ok(event) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        let kind = event.get("kind");
        if kind.and_then(|k| k.get("type")).and_then(|t| t.as_str())
            == Some("candidate_state_materialized")
        {
            let source = kind
                .and_then(|k| k.get("source_run_id"))
                .and_then(|v| v.as_str())
                .map(str::to_owned);
            let oid = kind
                .and_then(|k| k.get("actual_tree_oid"))
                .and_then(|v| v.as_str())
                .map(str::to_owned);
            return (source, oid, Some("materialized".to_owned()));
        }
    }
    (None, None, Some("not_applicable".to_owned()))
}
