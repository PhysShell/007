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
use std::sync::atomic::{AtomicU64, Ordering};

pub(crate) use o7::recovery::{ChildRecordState, DispatchProgress};

use crate::state::ExecutionConfig;

/// Q-Deck A0 corrective round 5 (Codex P1 / CodeRabbit Major, Part 2B): the
/// maximum `events.jsonl` size this server will read for an HTTP-triggered
/// candidate replay (admission preflight, run-detail projection) — checked
/// via `metadata` BEFORE any read, never after. A canonical record this
/// large is already pathological; refusing it here bounds the worst case
/// for an unauthenticated or low-trust caller hammering either endpoint.
const MAX_EVENTS_JSONL_BYTES: u64 = 8 * 1024 * 1024;

/// The maximum size of any SINGLE resolved artifact (a patch, a gate log,
/// a candidate receipt) during one HTTP-triggered candidate replay.
const MAX_ARTIFACT_BYTES: u64 = 64 * 1024 * 1024;

/// The maximum TOTAL bytes hydrated (summed across every artifact resolved)
/// during one HTTP-triggered candidate replay — bounds a record with many
/// individually-small-enough artifacts that would otherwise sum to an
/// unbounded amount of memory.
const MAX_TOTAL_HYDRATED_BYTES: u64 = 128 * 1024 * 1024;

/// Q-Deck A0 corrective round 5: an [`o7_run::replay::ArtifactResolver`]
/// that enforces [`MAX_ARTIFACT_BYTES`]/[`MAX_TOTAL_HYDRATED_BYTES`] before
/// ever reading an artifact's bytes — scoped to ONLY the two HTTP-triggered
/// candidate-replay call sites in this module (`parent_candidate_state_usable`,
/// `candidate_projection`). Deliberately NOT applied to the shared
/// `o7::events::RecordDirResolver` every other consumer (`o7 replay`, `o7
/// recover`, this server's own pre-redrive classification) uses — those
/// are operator-invoked, not reachable by an untrusted HTTP caller, so an
/// unbounded read there is a different, already-accepted risk profile this
/// round does not change.
struct BoundedRecordDirResolver {
    base: PathBuf,
    total_read: AtomicU64,
}

impl BoundedRecordDirResolver {
    fn new(base: PathBuf) -> Self {
        Self {
            base,
            total_read: AtomicU64::new(0),
        }
    }
}

fn confined_locator(locator: &str) -> bool {
    let p = std::path::Path::new(locator);
    !locator.is_empty()
        && p.components()
            .all(|c| matches!(c, std::path::Component::Normal(_)))
}

impl o7_run::replay::ArtifactResolver for BoundedRecordDirResolver {
    fn resolve(
        &self,
        a: &o7_run::event::ArtifactRef,
    ) -> Result<Vec<u8>, o7_run::replay::ArtifactError> {
        let fail = |reason: String| o7_run::replay::ArtifactError {
            locator: a.locator.clone(),
            reason,
        };
        if !confined_locator(&a.locator) {
            return Err(fail(
                "locator escapes the record directory (absolute, `..`, or empty) — refused \
                 before I/O"
                    .to_owned(),
            ));
        }
        let path = self.base.join(&a.locator);
        let size = std::fs::metadata(&path)
            .map_err(|e| fail(format!("stat before read: {e}")))?
            .len();
        if size > MAX_ARTIFACT_BYTES {
            return Err(fail(format!(
                "artifact is {size} bytes, exceeding the {MAX_ARTIFACT_BYTES}-byte per-artifact \
                 limit for an HTTP-triggered candidate replay"
            )));
        }
        let running_total = self.total_read.fetch_add(size, Ordering::Relaxed) + size;
        if running_total > MAX_TOTAL_HYDRATED_BYTES {
            return Err(fail(format!(
                "total hydrated bytes for this replay would reach {running_total}, exceeding \
                 the {MAX_TOTAL_HYDRATED_BYTES}-byte total budget for an HTTP-triggered \
                 candidate replay"
            )));
        }
        std::fs::read(&path).map_err(|e| fail(e.to_string()))
    }
}

/// Read `dir/events.jsonl`, refusing (before any read) a file larger than
/// [`MAX_EVENTS_JSONL_BYTES`] — Q-Deck A0 corrective round 5, Part 2B.
fn read_bounded_events_jsonl(dir: &std::path::Path) -> std::io::Result<String> {
    let path = dir.join("events.jsonl");
    let size = std::fs::metadata(&path)?.len();
    if size > MAX_EVENTS_JSONL_BYTES {
        return Err(std::io::Error::other(format!(
            "events.jsonl is {size} bytes, exceeding the {MAX_EVENTS_JSONL_BYTES}-byte limit \
             for an HTTP-triggered candidate replay"
        )));
    }
    std::fs::read_to_string(path)
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

/// Q-Deck A0 corrective round 3 (Codex P1, Part 1): whether `id` is valid
/// as the run-id component of a filesystem path — non-empty, not
/// absolute, no `.`/`..`, no embedded separator: exactly one normal
/// `std::path::Component`. `Path::new(id).components()` already collapses
/// `.`/redundant separators and classifies `..`/an absolute root as their
/// own non-`Normal` variants, so this is a complete, exhaustive check, not
/// a denylist of specific substrings.
///
/// `pub(crate)`, not merely a private helper inside [`child_record_dir`]:
/// `routes::create_command` calls this directly too, so a structurally
/// invalid `parent_run_id` gets its own stable `400`, distinct from a
/// well-formed-but-nonexistent one (`404`) or a well-formed, existing, but
/// candidate-unusable one (`409`) — three different failure shapes that
/// must not collapse into the same status code.
#[must_use]
pub(crate) fn is_confined_run_id_component(id: &str) -> bool {
    let mut components = std::path::Path::new(id).components();
    matches!(components.next(), Some(std::path::Component::Normal(_)))
        && components.next().is_none()
}

fn confine_run_id_component(id: &str) -> std::io::Result<()> {
    if is_confined_run_id_component(id) {
        Ok(())
    } else {
        Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("run id {id:?} is not a valid single path component"),
        ))
    }
}

/// The exact record directory a child run's canonical stream lives in —
/// computed ONLY from server-owned execution configuration (`exec`) and
/// `run_id`, never from anything else in the HTTP request. Q-Deck A0
/// corrective round 3 (Codex P1, Part 1): `run_id` is confined to a
/// single normal path component BEFORE it is ever joined into a path —
/// checked HERE, inside the one shared path-construction primitive every
/// caller goes through (including `parent_candidate_state_usable`, whose
/// own `run_id` comes directly from raw, not-yet-ledger-validated HTTP
/// request text), so no caller can forget it and no filesystem I/O is
/// ever attempted against an unconfined path.
pub(crate) fn child_record_dir(exec: &ExecutionConfig, run_id: &str) -> std::io::Result<PathBuf> {
    confine_run_id_component(run_id)?;
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

/// Q-Deck A0 corrective round 2 (Part 5); repository-bound corrective
/// round 3 (Codex P1, Part 4): whether `parent_run_id`'s own canonical
/// record proves it is USABLE as a command-continuation parent — checked
/// BEFORE a new command is ever durably created, not merely discovered
/// later when `o7 continue` itself fails after acceptance. A parent that
/// predates Q-Deck A0 (a genuine R1-only run, sealed and otherwise
/// perfectly valid, but with no `candidate_state` contract obligation at
/// all) is EXPLICITLY not usable — A0 makes a valid, semantically
/// verified candidate receipt mandatory for every continuation once live,
/// never a silent fallback to R1's old zero-carryover behavior. Returns
/// `Err(reason)` (a SERVER-side diagnostic string, never sent to the
/// client verbatim — same discipline as `ApiError::Internal`'s stable
/// code) for every one of: no canonical record at all; full replay
/// failure (chain/digest/reducer/artifact/candidate-semantic — the SAME
/// authoritative `verify_prefix` every other consumer uses); not sealed;
/// no candidate-state contract obligation; no candidate-state evidence
/// captured; the parent's own candidate contract was captured against a
/// DIFFERENT repository than this server is configured for (Part 4 — a
/// server pointed at a different checkout sharing the same `target`
/// basename must never treat that checkout's old record as usable). A
/// `Blocked`/`Fail`/`Error` sealed verdict is itself a legitimate,
/// replay-valid record (§8: canonical replay validity is not the same
/// question as continuation eligibility) — its own verdict is untouched
/// by this check; only the SEPARATE candidate-continuation-eligibility
/// question is answered here.
///
/// # Errors
/// A diagnostic `String` (server-side only) describing why the parent is
/// not usable.
pub(crate) fn parent_candidate_state_usable(
    exec: &ExecutionConfig,
    parent_run_id: &str,
) -> Result<(), String> {
    let dir = child_record_dir(exec, parent_run_id)
        .map_err(|e| format!("resolving parent record directory: {e}"))?;
    let text = read_bounded_events_jsonl(&dir)
        .map_err(|e| format!("reading the parent's canonical record: {e}"))?;
    let events = o7::events::from_jsonl(&text)
        .map_err(|e| format!("parsing the parent's canonical record: {e}"))?;
    let resolver = BoundedRecordDirResolver::new(dir);
    let (state, _artifacts_verified) = o7_run::replay::verify_prefix(&events, &resolver)
        .map_err(|e| format!("the parent's canonical record failed full replay: {e}"))?;
    if state.verdict.is_none() {
        return Err("the parent run is not sealed".to_owned());
    }
    let Some(obligation) = state
        .contract
        .as_ref()
        .and_then(|c| c.candidate_state.as_ref())
    else {
        return Err(
            "the parent run predates Q-Deck A0 (no candidate_state contract obligation) — a \
             valid candidate receipt is mandatory for every command continuation"
                .to_owned(),
        );
    };
    if state.candidate_state.is_none() {
        return Err(
            "the parent run declares a candidate_state obligation but never captured a receipt"
                .to_owned(),
        );
    }
    // Q-Deck A0 corrective round 3 (Codex P1, Part 4): the parent's OWN
    // verified candidate contract must have been captured against THIS
    // exact server's configured repository — `verify_prefix` (above)
    // already proved the CAPTURED RECEIPT agrees with its OWN contract
    // (internal self-consistency); it has no way to know whether that
    // contract's `repository_id` matches the repository THIS server is
    // configured for right now. Without this check, pointing the server
    // at a different checkout sharing the same `target` basename would
    // let a stale record from the OLD repository pass admission — the
    // mismatch would only surface later, in
    // `materialize_parent_candidate_state`, after the command already
    // durably attached a child run.
    let this_repo_id = o7::worktree::repository_identity(&exec.repo)
        .map_err(|e| format!("resolving this server's own configured repository identity: {e}"))?;
    if obligation.repository_id != this_repo_id {
        return Err(
            "the parent's candidate-state contract was captured against a different \
             repository than this server is currently configured for"
                .to_owned(),
        );
    }
    // `verify_prefix` (above) already ran full candidate semantic
    // verification on `state.candidate_state` as part of its own authoritative
    // pass — its mere presence here, post-success, IS the proof it is usable.
    Ok(())
}

/// Q-Deck A0 (`docs/q-deck/a0-candidate-state.md` §8), corrected corrective
/// round 2 (Part 3): a read-only, best-effort candidate-state projection
/// for `routes::get_run` — never mutates anything, never gates a redrive
/// decision (that stays exactly `classify_child_record`/
/// `o7::recovery::classify_command_child`, above, untouched by this).
///
/// The ORIGINAL implementation of this function had two defects, both fixed
/// here: (1) it read the raw JSON's `actual_tree_oid` field, a name that
/// stopped existing when corrective round 1 reshaped
/// `CandidateStateMaterialized` to carry `materialized_tree_oid` instead —
/// this projection's own tree-OID field had been silently, permanently
/// `None` ever since; (2) it trusted whatever the last matching raw JSON
/// line claimed, with NO chain/digest/reducer/artifact/candidate-semantic
/// verification at all — a tampered or truncated record could be projected
/// as "materialized" even though full replay would reject it. Both are
/// fixed by running the SAME authoritative `o7_run::replay::verify_prefix`
/// every other production consumer uses, and reading the tree OID back out
/// of the typed, already-verified `RunState` — never raw JSON. A run whose
/// full replay fails is projected as `"verification_failed"`, never
/// `"materialized"` — this projection must not claim semantic lineage is
/// verified when it is not.
///
/// Returns `(candidate_source_run_id, candidate_tree_oid,
/// materialization_status)`; all `None`/`"not_applicable"` for a run with no
/// canonical record yet, or one that never attempted materialization (a
/// top-level `o7 run`, or a continuation still pre-dispatch).
pub(crate) fn candidate_projection(
    exec: &ExecutionConfig,
    run_id: &str,
) -> (Option<String>, Option<String>, Option<String>) {
    let dir = match child_record_dir(exec, run_id) {
        Ok(e) => e,
        Err(e) => {
            eprintln!(
                "[o7d] candidate projection for run {run_id}: resolving record directory: {e}"
            );
            return (None, None, Some("failed".to_owned()));
        }
    };
    // Q-Deck A0 corrective round 5 (CodeRabbit Major, Part 4): only a
    // missing record (a run with no canonical record yet, or one that
    // never captured/materialized anything) projects as `not_applicable`.
    // Any OTHER I/O error (permission denied, a genuine device/read
    // failure) is a `failed` status, not silently folded into the exact
    // same "there's simply nothing here" value.
    let text = match read_bounded_events_jsonl(&dir) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return (None, None, Some("not_applicable".to_owned()))
        }
        Err(e) => {
            eprintln!("[o7d] candidate projection for run {run_id}: reading events.jsonl: {e}");
            return (None, None, Some("failed".to_owned()));
        }
    };
    let events = match o7::events::from_jsonl(&text) {
        Ok(events) => events,
        Err(e) => {
            eprintln!("[o7d] candidate projection for run {run_id}: malformed events.jsonl: {e}");
            return (None, None, Some("verification_failed".to_owned()));
        }
    };
    let resolver = BoundedRecordDirResolver::new(dir);
    let (state, _artifacts_verified) = match o7_run::replay::verify_prefix(&events, &resolver) {
        Ok(ok) => ok,
        Err(e) => {
            eprintln!("[o7d] candidate projection for run {run_id}: replay failed: {e}");
            return (None, None, Some("verification_failed".to_owned()));
        }
    };
    match &state.candidate_materialized {
        Some(mat) => (
            Some(mat.source_run_id.as_str().to_owned()),
            Some(mat.materialized_tree_oid.clone()),
            Some("materialized".to_owned()),
        ),
        None => (None, None, Some("not_applicable".to_owned())),
    }
}
