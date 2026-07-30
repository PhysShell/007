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

use axum::routing::{any, get};
use axum::Router;
use tower_http::services::{ServeDir, ServeFile};

/// Build the full production app: the `/api/v1/*` router plus, if
/// `static_dir` is given, the built Q-Deck shell served from the same
/// origin. Every literal `/api/v1/*` route below is matched exactly and
/// takes priority over both the wildcard 404 route and the static fallback;
/// anything else under `/api/v1/` that isn't one of those literals hits the
/// wildcard route and gets a JSON 404 (see `router`'s doc comment) — it must
/// never reach `static_dir`, or a typo'd/unimplemented API path would answer
/// with a 200 and the SPA shell instead of a 404. Anything outside `/api/v1/`
/// entirely falls through to a file in `static_dir`, and anything THAT
/// doesn't match falls back to `static_dir/index.html` — a client-side route
/// like `/runs/abc123` has no file of its own, so the SPA shell loads and its
/// own router (not this one) decides what the path means.
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
///
/// The trailing `/api/v1/*rest` route is a scoped catch-all: axum matches a
/// literal segment before a wildcard, so it only ever fires for an `/api/v1/`
/// path that isn't one of the literal routes above, and answers a plain JSON
/// 404 (`routes::unknown_api_route`) rather than letting the request fall
/// through to `app`'s static-file fallback — a real gap in an earlier
/// version, where an unmatched `/api/v1/*` path fell all the way through to
/// the SPA shell and got served with a 200 instead of a 404.
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
        .route("/api/v1/*rest", any(routes::unknown_api_route))
        .with_state(state)
}
