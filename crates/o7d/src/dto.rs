//! Transport DTOs. Every response is schema-versioned and derived from an
//! `o7-ledger` domain type — never a bare pass-through of it. A ledger model
//! gaining or renaming a field must not silently reshape the wire contract,
//! and a ledger internal (a raw ids.rs newtype, a `rowid`) must never leak
//! across this boundary undocumented.

use serde::{Deserialize, Serialize};

/// The `/api/v1` contract version. Bump on any breaking response-shape change
/// — including a new value inside an existing string field, whenever a
/// client's own type for that field is a closed union it uses for control
/// flow rather than a value it just displays. `RunStatus` is exactly that
/// case in Q-Deck: `isSealedRun` decides whether to keep polling by exact
/// match against a fixed set of strings, so a server that starts emitting
/// `"blocked"`/`"error"` without a version bump would have an old,
/// not-yet-upgraded client accept the response (same version number, so
/// `checkSchema` lets it through) and then silently poll a sealed run
/// forever, having never recognized the new value as sealed. `event_type` is
/// the *other* kind of field — Q-Deck never branches on its exact value, so
/// widening it stays additive without a bump. Bumped 1 → 2 for Q-Deck R0.6
/// (`docs/q-deck/r06-verdict-fidelity.md`) for exactly this reason.
pub const API_SCHEMA_VERSION: u32 = 2;

#[derive(Debug, Serialize)]
pub struct HealthDto {
    pub schema_version: u32,
    pub status: &'static str,
}

#[derive(Debug, Serialize)]
pub struct ConversationDto {
    pub schema_version: u32,
    pub conversation_id: String,
    pub created_at: i64,
    pub status: String,
}

impl From<o7_ledger::Conversation> for ConversationDto {
    fn from(c: o7_ledger::Conversation) -> Self {
        Self {
            schema_version: API_SCHEMA_VERSION,
            conversation_id: c.conversation_id.as_str().to_owned(),
            created_at: c.created_at,
            status: c.status.as_str().to_owned(),
        }
    }
}

/// `status` is a bare pass-through of `o7_ledger::RunStatus::as_str()` — Q-Deck
/// R0.6's new `"blocked"`/`"error"` values needed no change to this struct or
/// to `routes.rs`: a wider ledger vocabulary flows through automatically
/// without recomputation. The wire contract as a whole still needed an
/// `API_SCHEMA_VERSION` bump, though — not for this DTO's shape, but because
/// Q-Deck's `RunStatus` is a closed union its client logic branches on (see
/// `API_SCHEMA_VERSION`'s doc comment). See
/// `docs/q-deck/r06-verdict-fidelity.md` and
/// `crates/o7d/tests/verdict_fidelity.rs`.
#[derive(Debug, Serialize)]
pub struct RunDto {
    pub schema_version: u32,
    pub run_id: String,
    pub conversation_id: String,
    pub parent_run_id: Option<String>,
    pub agent: String,
    pub role: String,
    pub status: String,
    pub created_at: i64,
    pub finished_at: Option<i64>,
}

impl From<o7_ledger::Run> for RunDto {
    fn from(r: o7_ledger::Run) -> Self {
        Self {
            schema_version: API_SCHEMA_VERSION,
            run_id: r.run_id.as_str().to_owned(),
            conversation_id: r.conversation_id.as_str().to_owned(),
            parent_run_id: r.parent_run_id.map(|p| p.as_str().to_owned()),
            agent: r.agent,
            role: r.role,
            status: r.status.as_str().to_owned(),
            created_at: r.created_at,
            finished_at: r.finished_at,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct EventDto {
    pub schema_version: u32,
    pub event_id: String,
    pub conversation_id: String,
    pub run_id: Option<String>,
    pub attempt_id: Option<String>,
    /// The durable per-conversation cursor — the SSE `id` (task #4) is this
    /// same number, so a client's cursor is one concept everywhere, not a
    /// REST-shaped one here and a differently-shaped one on the stream.
    pub sequence: u64,
    pub event_type: String,
    pub created_at: i64,
    pub payload: serde_json::Value,
}

impl From<o7_ledger::PersistedEvent> for EventDto {
    fn from(e: o7_ledger::PersistedEvent) -> Self {
        Self {
            schema_version: API_SCHEMA_VERSION,
            event_id: e.event_id.as_str().to_owned(),
            conversation_id: e.conversation_id.as_str().to_owned(),
            run_id: e.run_id.map(|r| r.as_str().to_owned()),
            attempt_id: e.attempt_id.map(|a| a.as_str().to_owned()),
            sequence: e.sequence,
            event_type: e.event_type,
            created_at: e.created_at,
            payload: e.payload,
        }
    }
}

/// A keyset-paginated page, newest first (conversations/runs listings).
#[derive(Debug, Serialize)]
pub struct PageDto<T> {
    pub schema_version: u32,
    pub items: Vec<T>,
    /// Opaque continuation token for the next (older) page. Pass back as
    /// `before`. Absent means this page reached the oldest row — but per the
    /// underlying ledger's contract, a full page is not itself proof of that;
    /// keep paging with `before` until a page comes back with empty `items`.
    pub next_before: Option<String>,
}

/// A cursor-paginated page of one conversation's events, ascending.
#[derive(Debug, Serialize)]
pub struct EventPageDto {
    pub schema_version: u32,
    pub items: Vec<EventDto>,
    /// Pass as `after` to continue. Absent only when this page was empty.
    pub next_after: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct ErrorDto {
    pub schema_version: u32,
    pub error: String,
    pub code: &'static str,
}

#[derive(Debug, Deserialize, Default)]
pub struct ListParams {
    pub limit: Option<usize>,
    pub before: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct RunsListParams {
    pub limit: Option<usize>,
    pub before: Option<String>,
    pub conversation_id: Option<String>,
    /// Comma-separated `RunStatus` values, e.g. `?status=queued,running`.
    /// Absent means any status.
    pub status: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct EventsParams {
    pub after: Option<u64>,
    pub limit: Option<usize>,
}
