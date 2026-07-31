//! Shared handler state. `SqliteLedger` is already a cheap `Clone` (an
//! `Arc<Mutex<Connection>>` inside) — no extra wrapping needed for axum's
//! `State` extractor.

use std::path::PathBuf;

#[derive(Clone)]
pub struct AppState {
    pub ledger: o7_ledger::SqliteLedger,
    /// Q-Deck R1 (`docs/q-deck/r1-command.md` §9.4): fixed, server-side
    /// authority for spawning a command's child run continuation. `None`
    /// keeps `o7d` in its original R0 read-only posture — the mutation
    /// route is still registered (so a caller gets an honest `500` naming
    /// the missing configuration, never a bare 404) but never spawns
    /// anything.
    pub exec: Option<ExecutionConfig>,
}

/// Everything the command-continuation mutation route needs to spawn `o7
/// continue` — set once at `o7d serve` startup, NEVER from an HTTP request
/// (`docs/q-deck/r1-command.md` §8's forbidden-inputs list: no repo,
/// worktree path, ledger path, model, or permission mode may come from the
/// client).
#[derive(Clone)]
pub struct ExecutionConfig {
    /// The `o7` executable to spawn — a fixed path (or bare name resolved
    /// via `PATH`), never client-supplied.
    pub o7_bin: PathBuf,
    /// The ledger's own file path — `o7 continue` opens it independently
    /// (a separate process, a separate connection), not the `SqliteLedger`
    /// handle `o7d` itself holds.
    pub ledger_path: PathBuf,
    pub repo: PathBuf,
    pub worktree_root: PathBuf,
    pub runs_dir: PathBuf,
    pub gate: Option<PathBuf>,
    /// Fixed, server-chosen model — an arbitrary model is on §8's
    /// never-accepted-from-the-client list.
    pub model: String,
    pub max_turns: u32,
}
