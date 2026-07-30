//! Handlers for `/api/v1/*`. Each one is a thin translation: parse+validate
//! the request, call one `o7-ledger` read method, map the domain result to a
//! DTO. No handler ever touches SQLite directly — that stays entirely inside
//! `o7-ledger`.

use axum::extract::{Path, Query, State};
use axum::Json;

use crate::cursor;
use crate::dto::{
    ConversationDto, EventPageDto, EventsParams, HealthDto, ListParams, PageDto, RunDto,
    RunsListParams, API_SCHEMA_VERSION,
};
use crate::error::ApiError;
use crate::state::AppState;
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
