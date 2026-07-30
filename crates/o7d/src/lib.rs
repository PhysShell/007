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
mod stream;

pub use error::ApiError;
pub use state::AppState;

use std::path::Path;

use axum::routing::get;
use axum::Router;
use tower_http::services::{ServeDir, ServeFile};

/// Build the full production app: the `/api/v1/*` router plus, if
/// `static_dir` is given, the built Q-Deck shell served from the same
/// origin. `/api/v1/*` takes priority (it's registered first and matched
/// exactly); anything else falls through to a file in `static_dir`, and
/// anything THAT doesn't match falls back to `static_dir/index.html` — a
/// client-side route like `/runs/abc123` has no file of its own, so the SPA
/// shell loads and its own router (not this one) decides what the path means.
///
/// `static_dir = None` serves the API alone (used in dev, where Vite's dev
/// server serves the shell and proxies `/api` to this process instead).
pub fn app(ledger: o7_ledger::SqliteLedger, static_dir: Option<&Path>) -> Router {
    let api = router(ledger);
    match static_dir {
        Some(dir) => {
            let index = dir.join("index.html");
            // Plain `.fallback`, not `.not_found_service`: the latter forces
            // every fallback response's status to 404, which is right for a
            // "here's a 404 page" fallback but wrong here — serving
            // `index.html` for a client-side route must answer 200 (a
            // "not found" status on the page the client is about to render
            // could make an `EventSource`/`fetch` layer, or the client
            // itself, treat the shell load as failed).
            let serve_dir = ServeDir::new(dir).fallback(ServeFile::new(index));
            api.fallback_service(serve_dir)
        }
        None => api,
    }
}

/// Build the R0 API router alone. Bind it with `axum::serve` (see `main.rs`,
/// via [`app`]) or drive it in-process for tests via
/// `tower::ServiceExt::oneshot` / a real listener on `127.0.0.1:0`.
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
        .route(
            "/api/v1/conversations/:conversation_id/events/stream",
            get(stream::conversation_events_stream),
        )
        .route("/api/v1/runs", get(routes::list_runs))
        .route("/api/v1/runs/:run_id", get(routes::get_run))
        .with_state(state)
}
