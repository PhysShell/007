//! `o7d` — the control-plane daemon. The sole owner of the HTTP/SSE read
//! surface over `o7-ledger`. Untrusted clients (Q-Deck) only ever see the
//! versioned DTOs in [`dto`] — never a SQLite handle, a subprocess handle, or
//! a control socket. R0 is read-only: no run creation, no mutation, nothing
//! that changes ledger state lives behind this router.

pub mod cursor;
pub mod dto;
mod error;
mod routes;
mod state;

pub use error::ApiError;
pub use state::AppState;

use axum::routing::get;
use axum::Router;

/// Build the R0 router. Bind it with `axum::serve` (see `main.rs`) or drive
/// it in-process for tests via `tower::ServiceExt::oneshot` / a real listener
/// on `127.0.0.1:0`.
#[must_use]
pub fn router(ledger: o7_ledger::SqliteLedger) -> Router {
    let state = AppState { ledger };
    Router::new()
        .route("/api/v1/health", get(routes::health))
        .route("/api/v1/conversations", get(routes::list_conversations))
        .route(
            "/api/v1/conversations/:conversation_id",
            get(routes::get_conversation),
        )
        .route(
            "/api/v1/conversations/:conversation_id/events",
            get(routes::conversation_events),
        )
        .route("/api/v1/runs", get(routes::list_runs))
        .route("/api/v1/runs/:run_id", get(routes::get_run))
        .with_state(state)
}
