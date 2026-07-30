//! Shared handler state. `SqliteLedger` is already a cheap `Clone` (an
//! `Arc<Mutex<Connection>>` inside) — no extra wrapping needed for axum's
//! `State` extractor.

#[derive(Clone)]
pub struct AppState {
    pub ledger: o7_ledger::SqliteLedger,
}
