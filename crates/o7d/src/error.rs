//! The one place a handler's failure becomes an HTTP response. A storage
//! error must reach the client AS an error — never silently reshaped into an
//! empty list or a 404, which is exactly the "storage errors do not
//! masquerade as empty data" contract this whole API exists to keep honest.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;

use crate::dto::{ErrorDto, API_SCHEMA_VERSION};

#[derive(Debug)]
pub enum ApiError {
    /// A requested entity does not exist.
    NotFound,
    /// The request itself was malformed (a bad cursor, an unparseable limit).
    BadRequest(String),
    /// Q-Deck R1 (`docs/q-deck/r1-command.md` §8): a `409` — stale parent,
    /// an idempotency key reused with a different request, or a second
    /// command already in flight for this conversation.
    Conflict(&'static str, String),
    /// R1 §8: a `422` — the parent run has no valid, durable provider
    /// session to continue from (the fail-closed rule).
    UnprocessableEntity(&'static str, String),
    /// The ledger failed. The client sees a stable machine `code` (from
    /// [`o7_ledger::LedgerError::code`]) and a generic message — the
    /// underlying `Display` (which can carry raw SQLite text) stays out of
    /// the wire response even on a private network; only the code crosses.
    Internal(&'static str),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code, error) = match self {
            Self::NotFound => (StatusCode::NOT_FOUND, "NOT_FOUND", "not found".to_owned()),
            Self::BadRequest(msg) => (StatusCode::BAD_REQUEST, "BAD_REQUEST", msg),
            Self::Conflict(code, msg) => (StatusCode::CONFLICT, code, msg),
            Self::UnprocessableEntity(code, msg) => (StatusCode::UNPROCESSABLE_ENTITY, code, msg),
            Self::Internal(code) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                code,
                "internal error".to_owned(),
            ),
        };
        let body = Json(ErrorDto {
            schema_version: API_SCHEMA_VERSION,
            error,
            code,
        });
        (status, body).into_response()
    }
}

impl From<o7_ledger::LedgerError> for ApiError {
    fn from(e: o7_ledger::LedgerError) -> Self {
        // Every R0 read-only route only ever sees Sqlite/Json/other storage
        // failures here (its own `NotFound` handling is done at the
        // `Option` level, never by propagating `LedgerError::NotFound`) —
        // so mapping the Q-Deck R1 command variants precisely is a pure,
        // risk-free addition, not a behavior change for any existing route
        // (`docs/q-deck/r1-command.md` §8).
        match &e {
            o7_ledger::LedgerError::NotFound(_) => Self::NotFound,
            o7_ledger::LedgerError::IdempotencyConflict { .. } => {
                Self::Conflict(e.code(), e.to_string())
            }
            o7_ledger::LedgerError::StaleParent(_) => Self::Conflict(e.code(), e.to_string()),
            o7_ledger::LedgerError::CommandConflict(_) => Self::Conflict(e.code(), e.to_string()),
            o7_ledger::LedgerError::ContinuationNotPermitted(_) => {
                Self::UnprocessableEntity(e.code(), e.to_string())
            }
            _ => Self::Internal(e.code()),
        }
    }
}
