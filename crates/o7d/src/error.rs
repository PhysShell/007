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
        Self::Internal(e.code())
    }
}
