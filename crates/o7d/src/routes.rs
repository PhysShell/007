//! Handlers for `/api/v1/*`. Each one is a thin translation: parse+validate
//! the request, call one `o7-ledger` read method, map the domain result to a
//! DTO. No handler ever touches SQLite directly — that stays entirely inside
//! `o7-ledger`.

use axum::extract::rejection::JsonRejection;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;

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
fn command_lock_path(runs_dir: &std::path::Path, run_id: &str) -> std::path::PathBuf {
    runs_dir.join(".locks").join(format!("{run_id}.lock"))
}

/// `true` only if this call itself just acquired (and immediately
/// released) `run_id`'s lock — i.e. NO live process currently holds it.
/// This is a real liveness proof (`flock` auto-releases on process death
/// regardless of how the process died), not a wall-clock guess: a
/// staleness timer alone cannot distinguish "dead" from "just slow
/// creating a worktree past the threshold," and racing a genuinely slow
/// (not dead) `o7 continue` would spawn a second process onto the same
/// run id, corrupting its `events.jsonl` with interleaved writes.
/// `Err`/any I/O failure is treated as "cannot prove it's safe" by the
/// caller (fail closed — never spawn).
fn no_live_process_holds(runs_dir: &std::path::Path, run_id: &str) -> std::io::Result<bool> {
    let path = command_lock_path(runs_dir, run_id);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    // The lock file's own content never matters — only its existence and
    // flock state — so `truncate(false)` is explicit, not an oversight.
    let file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(&path)?;
    Ok(nix::fcntl::Flock::lock(file, nix::fcntl::FlockArg::LockExclusiveNonblock).is_ok())
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
        // still be in flight) AND — the actual safety proof, not a guess —
        // `no_live_process_holds`, which only reports true after ACTUALLY
        // acquiring the run id's own lock file itself (`flock` auto-
        // releases on process death regardless of how it died — a
        // wall-clock timer alone cannot tell "dead" from "just slow").
        // Only then does a compare-and-swap claim decide the single
        // winner among any racing retries.
        Some(run_id) => {
            let existing_run = state.ledger.run(run_id.clone()).await?;
            let eligible = existing_run.is_none_or(|r| !r.status.is_terminal());
            if eligible && command_is_stale(&command) {
                match no_live_process_holds(&exec.runs_dir, run_id.as_str()) {
                    Ok(true) => {
                        if state
                            .ledger
                            .claim_stale_command_for_redrive(
                                command.command_id.clone(),
                                command.updated_at,
                            )
                            .await?
                        {
                            let fresh = o7_ledger::RunId::generate();
                            let rebound = state
                                .ledger
                                .rebind_command_child_run(command.command_id.clone(), fresh)
                                .await?;
                            spawn_continue(exec, &conversation_id, &parent_run_id, &rebound)
                                .map_err(|_| ApiError::Internal("SPAWN_FAILED"))?;
                            rebound
                        } else {
                            command
                        }
                    }
                    // A live process genuinely still holds the lock — it
                    // is provably still working; never race it. An I/O
                    // failure checking the lock is treated the same way:
                    // fail closed, never spawn.
                    Ok(false) | Err(_) => command,
                }
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
