//! `o7-ledger` — the append-only durable source of truth for 007.
//!
//! It records conversations, runs, run attempts and events, with per-conversation
//! monotonic sequences, cursor replay, idempotency keys, and crash recovery — and
//! nothing else. There are no workers, adapters, Sandboy, worktrees, HTTP, or
//! Cockpit here (those are later PRs). This crate's single job: 007 must reliably
//! remember what happened, even after the rest of the program has died.
//!
//! The SQLite connection is owned inside [`SqliteLedger`] and never handed to a UI
//! or transport. See `docs/architecture/ledger-durability.md` and
//! `docs/architecture/ledger-schema-v1.md`.

use std::time::{SystemTime, UNIX_EPOCH};

pub mod idempotency;
pub mod ids;
pub mod migrations;
pub mod models;
pub mod recovery;
pub mod sqlite;
pub mod transitions;

pub use ids::{AttemptId, ConversationId, EventId, RunId};
pub use models::{
    AttemptStatus, Conversation, ConversationStatus, EventType, Idempotency, IdempotencyRecord,
    NewEvent, NewRun, PersistedEvent, RecoveryState, Run, RunAttempt, RunStatus,
};
pub use sqlite::{SqliteLedger, EVENT_SCHEMA_VERSION, MAX_READ_LIMIT};

/// The core append-only ledger contract: append an event, or replay a
/// conversation's events by cursor. `append_event` returns only after a
/// successful commit.
#[allow(async_fn_in_trait)]
pub trait Ledger {
    /// Append one event. The per-conversation `sequence` and `created_at` are
    /// assigned inside the append transaction and returned on the persisted row.
    ///
    /// # Errors
    /// Propagates storage errors; the event is never observable before commit.
    async fn append_event(&self, event: NewEvent) -> Result<PersistedEvent, LedgerError>;

    /// Read events for a conversation with `sequence > after_sequence`, ordered
    /// ascending, capped at [`MAX_READ_LIMIT`]. `after_sequence = None` starts at
    /// the beginning. An unknown conversation yields an empty list (never an
    /// error masquerading as corruption).
    ///
    /// # Errors
    /// Propagates storage errors.
    async fn read_events(
        &self,
        conversation_id: &ConversationId,
        after_sequence: Option<u64>,
        limit: usize,
    ) -> Result<Vec<PersistedEvent>, LedgerError>;
}

/// Errors surfaced by the ledger. [`LedgerError::code`] gives a stable string
/// code for callers/telemetry.
#[derive(Debug, thiserror::Error)]
pub enum LedgerError {
    /// Underlying SQLite error.
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),

    /// JSON (de)serialization error for an event payload.
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),

    /// Same idempotency `(scope, key)` reused with a different request digest.
    #[error("idempotency conflict: scope={scope} key={key}")]
    IdempotencyConflict { scope: String, key: String },

    /// A state transition that is not permitted.
    #[error("forbidden {entity} transition: {from} -> {to}")]
    ForbiddenTransition {
        entity: &'static str,
        from: &'static str,
        to: &'static str,
    },

    /// A referenced entity was not found.
    #[error("not found: {0}")]
    NotFound(String),

    /// Integrity/consistency violation (failed integrity check, or a stored
    /// value that cannot be interpreted).
    #[error("integrity: {0}")]
    Integrity(String),

    /// A blocking database task failed to join.
    #[error("blocking task join failure")]
    Join,

    /// The connection mutex was poisoned by a panicking holder.
    #[error("ledger lock poisoned")]
    LockPoisoned,
}

impl LedgerError {
    /// A stable, machine-readable code for this error.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::Sqlite(_) => "SQLITE",
            Self::Json(_) => "JSON",
            Self::IdempotencyConflict { .. } => "IDEMPOTENCY_CONFLICT",
            Self::ForbiddenTransition { .. } => "FORBIDDEN_TRANSITION",
            Self::NotFound(_) => "NOT_FOUND",
            Self::Integrity(_) => "INTEGRITY",
            Self::Join => "JOIN",
            Self::LockPoisoned => "LOCK_POISONED",
        }
    }
}

/// Current unix time in milliseconds, clamped to a non-negative `i64`. Used only
/// for metadata (`created_at`/`started_at`/`finished_at`); it is NEVER the cursor
/// — ordering is by the per-conversation `sequence`.
pub(crate) fn now_millis() -> i64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(elapsed) => i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX),
        Err(_) => 0,
    }
}
