//! Handlers for `/api/v1/*`. Each one is a thin translation: parse+validate
//! the request, call one `o7-ledger` read method, map the domain result to a
//! DTO. No handler ever touches SQLite directly — that stays entirely inside
//! `o7-ledger`.

use axum::extract::rejection::JsonRejection;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;

use crate::canonical::{child_record_dir, classify_child_record, ChildRecordState};
use crate::cursor;
use crate::dto::{
    CommandAcceptedDto, ConversationDto, EventPageDto, EventsParams, HealthDto, ListParams,
    NewCommandRequestDto, PageDto, RunDto, RunsListParams, API_SCHEMA_VERSION,
    COMMAND_SCHEMA_VERSION,
};
use crate::error::ApiError;
use crate::state::{AppState, ExecutionConfig};
use o7_ledger::Ledger as _;

/// Default page size when a caller doesn't specify one. Small and
/// mobile-appropriate — a caller wanting more pages further back into
/// history, per the same contract [`o7_ledger::SqliteLedger::list_runs`]
/// documents.
const DEFAULT_LIST_LIMIT: usize = 50;

/// Default event-page size. Larger than the listing default: a run's own
/// timeline is usually what a client actually wants to see in full on first
/// load, not paged eagerly.
const DEFAULT_EVENTS_LIMIT: usize = 200;

/// Parse `?status=queued,running` into the ledger's own status enum, so an
/// unrecognized value is a 400 (a client mistake) rather than silently
/// matching nothing.
fn parse_statuses(raw: &str) -> Result<Vec<o7_ledger::RunStatus>, String> {
    raw.split(',')
        .map(|s| {
            o7_ledger::RunStatus::parse(s.trim()).ok_or_else(|| format!("unknown run status {s:?}"))
        })
        .collect()
}

pub(crate) async fn health() -> Json<HealthDto> {
    Json(HealthDto {
        schema_version: API_SCHEMA_VERSION,
        status: "ok",
    })
}

pub(crate) async fn list_conversations(
    State(state): State<AppState>,
    Query(params): Query<ListParams>,
) -> Result<Json<PageDto<ConversationDto>>, ApiError> {
    let before = params
        .before
        .as_deref()
        .map(cursor::decode)
        .transpose()
        .map_err(ApiError::BadRequest)?;
    let limit = params.limit.unwrap_or(DEFAULT_LIST_LIMIT);
    let page = state.ledger.list_conversations(before, limit).await?;
    Ok(Json(PageDto {
        schema_version: API_SCHEMA_VERSION,
        items: page.items.into_iter().map(ConversationDto::from).collect(),
        next_before: page.next_before.as_ref().map(cursor::encode),
    }))
}

pub(crate) async fn get_conversation(
    State(state): State<AppState>,
    Path(conversation_id): Path<String>,
) -> Result<Json<ConversationDto>, ApiError> {
    let id = o7_ledger::ConversationId::from_raw(conversation_id);
    let conversation = state
        .ledger
        .conversation(id)
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok(Json(conversation.into()))
}

pub(crate) async fn conversation_events(
    State(state): State<AppState>,
    Path(conversation_id): Path<String>,
    Query(params): Query<EventsParams>,
) -> Result<Json<EventPageDto>, ApiError> {
    let id = o7_ledger::ConversationId::from_raw(conversation_id);
    // Same existence contract as get_conversation/get_run: an unknown parent
    // is 404, not a 200 with an empty page — a typo'd id and a genuinely
    // empty conversation must not look identical on the wire.
    state
        .ledger
        .conversation(id.clone())
        .await?
        .ok_or(ApiError::NotFound)?;
    let limit = params.limit.unwrap_or(DEFAULT_EVENTS_LIMIT);
    let events = state.ledger.read_events(&id, params.after, limit).await?;
    let next_after = events.last().map(|e| e.sequence);
    Ok(Json(EventPageDto {
        schema_version: API_SCHEMA_VERSION,
        items: events.into_iter().map(Into::into).collect(),
        next_after,
    }))
}

pub(crate) async fn list_runs(
    State(state): State<AppState>,
    Query(params): Query<RunsListParams>,
) -> Result<Json<PageDto<RunDto>>, ApiError> {
    let before = params
        .before
        .as_deref()
        .map(cursor::decode)
        .transpose()
        .map_err(ApiError::BadRequest)?;
    let limit = params.limit.unwrap_or(DEFAULT_LIST_LIMIT);
    let conversation_id = params
        .conversation_id
        .map(o7_ledger::ConversationId::from_raw);
    let statuses = params
        .status
        .as_deref()
        .map(parse_statuses)
        .transpose()
        .map_err(ApiError::BadRequest)?;
    let page = state
        .ledger
        .list_runs(conversation_id, statuses, before, limit)
        .await?;
    Ok(Json(PageDto {
        schema_version: API_SCHEMA_VERSION,
        items: page.items.into_iter().map(RunDto::from).collect(),
        next_before: page.next_before.as_ref().map(cursor::encode),
    }))
}

pub(crate) async fn get_run(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
) -> Result<Json<RunDto>, ApiError> {
    let id = o7_ledger::RunId::from_raw(run_id);
    let run = state.ledger.run(id).await?.ok_or(ApiError::NotFound)?;
    Ok(Json(run.into()))
}

/// Q-Deck R1 (`docs/q-deck/r1-command.md` §8): the command text's own size
/// limit — deliberately small, this is a command, not a file upload.
const MAX_COMMAND_TEXT_BYTES: usize = 8 * 1024;

/// How long a command may sit bound-but-not-yet-dispatched before a retry
/// is allowed to treat it as dead and re-spawn. Generous on purpose: a
/// genuinely in-flight spawn (worktree creation, git operations) must never
/// be raced by an impatient retry — the cost of waiting a little longer to
/// heal a truly stuck command is far smaller than the cost of two
/// processes attaching to the same not-yet-existing run id at once.
/// Overridable via `O7D_STALE_COMMAND_REDRIVE_MS` — production never sets
/// it (the 60s default applies); the process-level E2E test shortens it so
/// the redrive path can be proven without a real 60-second wait.
const STALE_COMMAND_REDRIVE_MS: i64 = 60_000;

fn now_millis() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(elapsed) => i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX),
        Err(_) => 0,
    }
}

fn stale_redrive_threshold_ms() -> i64 {
    std::env::var("O7D_STALE_COMMAND_REDRIVE_MS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(STALE_COMMAND_REDRIVE_MS)
}

fn command_is_stale(command: &o7_ledger::Command) -> bool {
    now_millis().saturating_sub(command.updated_at) >= stale_redrive_threshold_ms()
}

/// Q-Deck R1 second corrective round: the run-id-keyed exclusive lock a
/// spawned `o7 continue` holds for its ENTIRE lifetime (`src/main.rs`'s
/// `continue_run` acquires the SAME path — deliberately duplicated rather
/// than shared across crates for a two-line helper). Same formula as
/// there: `<runs_dir>/.locks/<run_id>.lock`.
pub(crate) fn command_lock_path(runs_dir: &std::path::Path, run_id: &str) -> std::path::PathBuf {
    runs_dir.join(".locks").join(format!("{run_id}.lock"))
}

/// Spawn `o7 recover --ledger <path> --run-dir <dir>` for a child run whose
/// OWN canonical record has just been classified `ValidSealed`, and wait
/// for it to finish — reusing the root binary's EXACT existing catch-up
/// machinery (`catch_up`, re-verify + re-project + apply the sealed
/// verdict + best-effort session backfill) rather than re-deriving any of
/// it here. Best-effort: any failure is logged, never propagated — the
/// caller's own subsequent `mark_command_completed_if_bound` call is a
/// safe no-op if this didn't manage to land the sealed status, and the
/// core safety property (provider never re-invoked for an already-sealed
/// record) holds regardless of whether this reconciliation succeeds.
fn recover_sealed_child(exec: &ExecutionConfig, run_id: &str) {
    let dir = match child_record_dir(exec, run_id) {
        Ok(dir) => dir,
        Err(e) => {
            eprintln!(
                "[o7d] warning: resolving the record directory to recover sealed child {run_id}: {e}"
            );
            return;
        }
    };
    match std::process::Command::new(&exec.o7_bin)
        .arg("recover")
        .arg("--ledger")
        .arg(&exec.ledger_path)
        .arg("--run-dir")
        .arg(&dir)
        .stdin(std::process::Stdio::null())
        .output()
    {
        Ok(output) if output.status.success() => {}
        Ok(output) => eprintln!(
            "[o7d] warning: `o7 recover --run-dir {}` exited non-zero while catching up a sealed \
             child record: {}",
            dir.display(),
            String::from_utf8_lossy(&output.stderr)
        ),
        Err(e) => eprintln!(
            "[o7d] warning: spawning `o7 recover --run-dir {}` failed: {e}",
            dir.display()
        ),
    }
}

/// Q-Deck R1 fourth corrective round: acquire `old_run_id`'s own liveness
/// lock and, if won, hold it for the ENTIRE classify-then-transition
/// decision — closing the window a probe-then-release pattern cannot: a
/// redrive that only checks-then-drops the lock before deciding cannot see
/// a process that hasn't reached its OWN `flock()` call yet (merely slow,
/// not dead), so a genuinely slow original process could still wake up and
/// invoke the provider concurrently with a freshly spawned redrive. Holding
/// the lock through the WHOLE decision — classification, and whichever of
/// recovery or CAS-rebind-plus-spawn follows — makes that impossible: no
/// other process can win this exact lock while this call still holds it.
///
/// Runs synchronously (real, blocking file I/O and, for a sealed record, a
/// blocking subprocess wait) — callers MUST run this via
/// `tokio::task::spawn_blocking`, never inline in an async handler.
///
/// Returns `Ok(current_command)` for every outcome except a canonical
/// record that fails verification/identity checks, which is the one case
/// that must fail closed and surface as a stable error to the caller
/// (`COMMAND_CANONICAL_RECORD_INVALID`) — the command is left exactly as
/// found, discoverable for manual investigation.
fn redrive_or_recover_locked(
    exec: ExecutionConfig,
    ledger: o7_ledger::SqliteLedger,
    command: o7_ledger::Command,
    old_run_id: o7_ledger::RunId,
    conversation_id: String,
    parent_run_id: String,
) -> Result<o7_ledger::Command, String> {
    let lock_path = command_lock_path(&exec.runs_dir, old_run_id.as_str());
    if let Some(parent) = lock_path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            eprintln!(
                "[o7d] warning: creating lock directory {}: {e} — refusing to redrive",
                parent.display()
            );
            return Ok(command);
        }
    }
    // The lock file's own content never matters — only its existence and
    // flock state — so `truncate(false)` is explicit, not an oversight.
    let lock_file = match std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(&lock_path)
    {
        Ok(file) => file,
        Err(e) => {
            eprintln!(
                "[o7d] warning: opening lock file {}: {e} — refusing to redrive",
                lock_path.display()
            );
            return Ok(command);
        }
    };
    let handle = tokio::runtime::Handle::current();
    let _old_child_lock =
        match nix::fcntl::Flock::lock(lock_file, nix::fcntl::FlockArg::LockExclusiveNonblock) {
            Ok(guard) => guard,
            // Q-Deck R1 third corrective round (major finding, preserved):
            // `EWOULDBLOCK` is the ONLY errno that actually means "a live
            // owner holds this lock" — anything else is a genuine,
            // unrelated I/O problem, and refuses immediately (fail closed).
            //
            // Q-Deck R1 fourth corrective round (Part 3, concurrent-retry
            // requirement): the owner holding an `EWOULDBLOCK` lock is not
            // always an external `o7 continue` process — it can just as
            // well be ANOTHER concurrent request to THIS SAME endpoint,
            // already mid-way through its own classify-then-CAS decision
            // for this exact old run id (that is precisely what holding
            // this lock through the whole decision, per Part 2, is FOR).
            // Two truly concurrent same-key retries must converge on the
            // SAME final child run id — so before giving up, briefly poll
            // the command's own current binding: if it moves away from
            // `old_run_id` within a short bound, that is the OTHER
            // request's own CAS having just landed, and THIS request must
            // report that SAME authoritative result, never a stale
            // snapshot of its own. If the binding never moves within the
            // bound, the lock's owner is a genuinely still-working
            // external process — fall back to the original, unchanged
            // command exactly as before.
            Err((_file, errno)) => {
                if errno != nix::errno::Errno::EWOULDBLOCK {
                    eprintln!(
                        "[o7d] warning: checking run {}'s lock failed with {errno} (not \
                         EWOULDBLOCK — not ordinary contention); refusing to redrive until this \
                         is investigated",
                        old_run_id.as_str()
                    );
                    return Ok(command);
                }
                let deadline = std::time::Instant::now() + std::time::Duration::from_millis(500);
                loop {
                    if std::time::Instant::now() >= deadline {
                        return Ok(command);
                    }
                    std::thread::sleep(std::time::Duration::from_millis(15));
                    match handle.block_on(ledger.command(command.command_id.clone())) {
                        Ok(Some(current))
                            if current.child_run_id.as_ref().map(o7_ledger::RunId::as_str)
                                != Some(old_run_id.as_str()) =>
                        {
                            return Ok(current);
                        }
                        _ => continue,
                    }
                }
            }
        };

    // From here on, this call is the EXCLUSIVE, provable owner of the old
    // child's lock — held through classification and whatever transition
    // follows, released only when this function returns.
    let state = classify_child_record(&exec, old_run_id.as_str(), &conversation_id);

    match state {
        ChildRecordState::Invalid(reason) => Err(reason),
        ChildRecordState::ValidSealed => {
            // Case A: the provider already ran to completion. Redrive is
            // forbidden — recover instead (re-verify, catch up the
            // projection, apply the verdict, best-effort session
            // backfill), then conditionally complete the command, still
            // bound to the SAME original child run id throughout.
            recover_sealed_child(&exec, old_run_id.as_str());
            if let Err(e) = handle.block_on(
                ledger.mark_command_completed_if_bound(command.command_id.clone(), old_run_id),
            ) {
                eprintln!(
                    "[o7d] warning: marking command {} completed after sealed-record recovery \
                     failed: {e}",
                    command.command_id
                );
            }
            match handle.block_on(ledger.command(command.command_id.clone())) {
                Ok(Some(current)) => Ok(current),
                _ => Ok(command),
            }
        }
        ChildRecordState::Absent | ChildRecordState::ValidUnsealed => {
            // Case B/C: genuinely never started, or genuinely still
            // running (crashed before sealing) — safe to redrive with a
            // FRESH child run id, exactly once, atomically.
            let fresh = o7_ledger::RunId::generate();
            match handle.block_on(ledger.rebind_command_child_run_if_current(
                command.command_id.clone(),
                old_run_id,
                command.updated_at,
                fresh,
            )) {
                Ok(o7_ledger::RebindOutcome::Rebound(rebound)) => {
                    // Q-Deck R1 fourth corrective round (Part 5): a spawn
                    // failure here must NOT reject the command — the
                    // fresh binding already landed, `status` stays
                    // `started`, and the NEXT same-key retry (once stale
                    // again) will classify this fresh run id (Absent, since
                    // it never got a canonical record) and redrive it
                    // again. The conversation is never permanently wedged
                    // by an ordinary spawn failure.
                    if let Err(e) =
                        spawn_continue(&exec, &conversation_id, &parent_run_id, &rebound)
                    {
                        eprintln!(
                            "[o7d] warning: spawning `o7 continue` after a successful rebind \
                             failed: {e} — command {} stays started, rebound to {}; a later \
                             retry will redrive it again once stale",
                            rebound.command_id,
                            rebound
                                .child_run_id
                                .as_ref()
                                .map(o7_ledger::RunId::as_str)
                                .unwrap_or_default()
                        );
                    }
                    Ok(rebound)
                }
                Ok(
                    o7_ledger::RebindOutcome::LostRace(current)
                    | o7_ledger::RebindOutcome::NotEligible(current),
                ) => Ok(current),
                Ok(o7_ledger::RebindOutcome::NotFound) => Ok(command),
                Err(e) => {
                    eprintln!("[o7d] warning: rebind_command_child_run_if_current failed: {e}");
                    Ok(command)
                }
            }
        }
    }
}

/// `POST /api/v1/conversations/{conversation_id}/commands` (§8/§9.6).
/// Validate → durably accept (`SqliteLedger::create_command`, itself
/// idempotent/stale-parent/concurrency-checked) → durably bind a freshly
/// minted child `RunId` (compare-and-swap safe against a racing retry of
/// the same idempotency key, see `bind_command_child_run`'s doc comment) →
/// spawn `o7 continue` ONLY if this request actually won that bind → 202,
/// without waiting for the spawned process.
///
/// # Errors
/// See the module doc / §8's status-code table: `400` malformed request,
/// `404` unknown conversation or parent, `409` stale parent / idempotency
/// conflict / concurrent command, `422` no continuable provider session,
/// `500` if `o7d` was not started with execution authority configured, or
/// a genuine spawn/storage failure.
pub(crate) async fn create_command(
    State(state): State<AppState>,
    Path(conversation_id): Path<String>,
    body: Result<Json<NewCommandRequestDto>, JsonRejection>,
) -> Result<(StatusCode, Json<CommandAcceptedDto>), ApiError> {
    let exec = state
        .exec
        .as_ref()
        .ok_or(ApiError::Internal("EXEC_NOT_CONFIGURED"))?;

    // A single, uniform 400 for EVERY extraction-time failure — invalid
    // JSON syntax, a field with the wrong type, an unknown field (denied
    // by `NewCommandRequestDto`'s own `deny_unknown_fields`), or the
    // request body exceeding the route's `DefaultBodyLimit`. Without this,
    // axum's own default rejections would answer in a differently-shaped
    // body (and, for an oversized body, an undocumented `413`) — neither
    // matches this endpoint's frozen `ErrorDto`/status-code contract.
    let Json(body) =
        body.map_err(|rejection| ApiError::BadRequest(format!("malformed request: {rejection}")))?;

    match body.schema_version {
        Some(1) => {}
        Some(v) => {
            return Err(ApiError::BadRequest(format!(
                "unsupported schema_version {v} (expected 1)"
            )))
        }
        None => return Err(ApiError::BadRequest("missing schema_version".to_owned())),
    }
    let parent_run_id = body
        .parent_run_id
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ApiError::BadRequest("missing or empty parent_run_id".to_owned()))?;
    let command_text = body
        .command
        .ok_or_else(|| ApiError::BadRequest("missing command".to_owned()))?;
    if command_text.trim().is_empty() {
        return Err(ApiError::BadRequest(
            "command must not be empty or whitespace-only".to_owned(),
        ));
    }
    if command_text.len() > MAX_COMMAND_TEXT_BYTES {
        return Err(ApiError::BadRequest(format!(
            "command exceeds the {MAX_COMMAND_TEXT_BYTES}-byte limit"
        )));
    }
    let idempotency_key = body
        .idempotency_key
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ApiError::BadRequest("missing or empty idempotency_key".to_owned()))?;

    let request = o7_ledger::NewCommand {
        conversation_id: o7_ledger::ConversationId::from_raw(conversation_id.clone()),
        parent_run_id: o7_ledger::RunId::from_raw(parent_run_id.clone()),
        command_text,
    };
    let command = state
        .ledger
        .create_command(
            request,
            o7_ledger::Idempotency {
                key: idempotency_key,
            },
        )
        .await?;

    // The response's own `status` — reported as `accepted` for THIS
    // request's own fresh acceptance regardless of what also happens to
    // succeed synchronously moments later (binding, spawning): a `202`'s
    // job is to confirm DURABLE ACCEPTANCE, matching the frozen contract's
    // response example. A genuine idempotent REPLAY (this same request
    // seen again) instead reports the command's real current status.
    let is_fresh = command.child_run_id.is_none();

    let final_command = match &command.child_run_id {
        // Idempotent replay of a command already bound to a child run.
        // Bound is NOT the same as dispatched-and-alive. Eligible for
        // redrive whenever the child run has no ledger row at all (the
        // process that was going to spawn `o7 continue` died right after
        // binding, or the spawn itself failed) OR its ledger row exists
        // but is NOT terminal (`queued`/`running`/`interrupted` — a crash
        // right after `attach_run` leaves it `queued`/`running`, possibly
        // for a while before anyone runs a plain `o7 recover` to reclassify
        // it `interrupted`; the lock check below is what actually proves
        // safety here, not the ledger status, so there is no need to wait
        // for that reclassification). A run row already in a SEALED
        // terminal status is never touched here — `reconcile_completed_commands`
        // handles the mirror-image "sealed but command bookkeeping didn't
        // land" case instead.
        //
        // A redrive ALWAYS mints a FRESH run id — never reuses the
        // existing one, even when no ledger row for it exists yet: a
        // crashed FIRST attempt may already have durably appended a
        // partial `events.jsonl` for that run id (`RunStarted`, maybe
        // more) before ever reaching the ledger — reusing it would let a
        // second attempt append a SECOND, conflicting `RunStarted` at
        // sequence 1 into the SAME file. A brand new run id's directory
        // is always pristine.
        //
        // Gated by a staleness bound (never race a request that might
        // still be in flight) AND — the actual decision, not a guess —
        // `redrive_or_recover_locked`: it classifies the old child's own
        // canonical record (never trusting ledger status alone — a stale
        // or not-yet-caught-up projection can show `queued`/`running`/
        // `interrupted` for a run whose flat record is already fully
        // sealed) while holding that exact run id's liveness lock for the
        // WHOLE decision, so a merely-slow (not dead) original process can
        // never race a redrive that has already moved on.
        Some(run_id) => {
            let existing_run = state.ledger.run(run_id.clone()).await?;
            let eligible = existing_run.is_none_or(|r| !r.status.is_terminal());
            if eligible && command_is_stale(&command) {
                let exec_owned = exec.clone();
                let ledger = state.ledger.clone();
                let command_for_redrive = command.clone();
                let old_run_id = run_id.clone();
                let conversation_id_for_redrive = conversation_id.clone();
                let parent_run_id_for_redrive = parent_run_id.clone();
                let outcome = tokio::task::spawn_blocking(move || {
                    redrive_or_recover_locked(
                        exec_owned,
                        ledger,
                        command_for_redrive,
                        old_run_id,
                        conversation_id_for_redrive,
                        parent_run_id_for_redrive,
                    )
                })
                .await
                .map_err(|_| ApiError::Internal("REDRIVE_TASK_PANICKED"))?;
                outcome.map_err(|reason| {
                    eprintln!(
                        "[o7d] refusing to redrive command {}: its old child run's own canonical \
                         record failed verification/identity checks: {reason}",
                        command.command_id
                    );
                    ApiError::Internal("COMMAND_CANONICAL_RECORD_INVALID")
                })?
            } else {
                command
            }
        }
        None => {
            let candidate = o7_ledger::RunId::generate();
            let bound = state
                .ledger
                .bind_command_child_run(command.command_id.clone(), candidate.clone())
                .await?;
            let won = bound.child_run_id.as_ref().map(o7_ledger::RunId::as_str)
                == Some(candidate.as_str());
            if won {
                spawn_continue(exec, &conversation_id, &parent_run_id, &bound)
                    .map_err(|_| ApiError::Internal("SPAWN_FAILED"))?;
            }
            bound
        }
    };

    let run_id = final_command
        .child_run_id
        .as_ref()
        .ok_or(ApiError::Internal("COMMAND_BIND_MISSING"))?
        .as_str()
        .to_owned();
    let status = if is_fresh {
        o7_ledger::CommandStatus::Accepted.as_str().to_owned()
    } else {
        final_command.status.as_str().to_owned()
    };

    Ok((
        StatusCode::ACCEPTED,
        Json(CommandAcceptedDto {
            schema_version: COMMAND_SCHEMA_VERSION,
            command_id: final_command.command_id.as_str().to_owned(),
            conversation_id,
            parent_run_id,
            run_id,
            status,
        }),
    ))
}

/// Spawn `o7 continue` for a just-bound command — explicit argv via
/// `std::process::Command`, never a shell (same discipline as
/// `agent::continue_session`, §9.2). The child is detached (never awaited
/// here — §9.6's "respond without waiting for full provider completion")
/// but still reaped via a background blocking task, so a long-lived `o7d`
/// process never accumulates zombie children across many commands.
fn spawn_continue(
    exec: &ExecutionConfig,
    conversation_id: &str,
    parent_run_id: &str,
    command: &o7_ledger::Command,
) -> std::io::Result<()> {
    let mut cmd = std::process::Command::new(&exec.o7_bin);
    cmd.arg("continue")
        .arg("--repo")
        .arg(&exec.repo)
        .arg("--worktree-root")
        .arg(&exec.worktree_root)
        .arg("--runs-dir")
        .arg(&exec.runs_dir)
        .arg("--model")
        .arg(&exec.model)
        .arg("--max-turns")
        .arg(exec.max_turns.to_string())
        .arg("--ledger")
        .arg(&exec.ledger_path)
        .arg("--conversation-id")
        .arg(conversation_id)
        .arg("--parent-run-id")
        .arg(parent_run_id)
        .arg("--command")
        .arg(&command.command_text)
        .arg("--run-id")
        .arg(
            command
                .child_run_id
                .as_ref()
                .map(o7_ledger::RunId::as_str)
                .unwrap_or_default(),
        )
        .arg("--command-id")
        .arg(command.command_id.as_str());
    if let Some(gate) = &exec.gate {
        cmd.arg("--gate").arg(gate);
    }
    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());

    let child = cmd.spawn()?;
    tokio::task::spawn_blocking(move || {
        let mut child = child;
        let _ = child.wait();
    });
    Ok(())
}

/// Matches any `/api/v1/*` path none of the literal routes above claimed —
/// see the `/api/v1/*rest` route's doc comment in `lib.rs` for why this must
/// exist at all.
pub(crate) async fn unknown_api_route() -> ApiError {
    ApiError::NotFound
}
