//! `o7d serve` — open the ledger, bind, and expose the R0 read-only API,
//! plus (when configured) Q-Deck R1's command-continuation mutation route.

use std::net::SocketAddr;
use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "o7d")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run the daemon: HTTP/SSE read surface over an `o7-ledger` database.
    Serve {
        /// Path to the ledger's SQLite file.
        #[arg(long)]
        ledger: PathBuf,
        /// Address to listen on. Defaults to loopback-only.
        #[arg(long, default_value = "127.0.0.1:4170")]
        listen: SocketAddr,
        /// Required to bind a non-loopback address. R0 has no public-internet
        /// authentication story (see docs/q-deck/architecture.md) — a
        /// non-loopback bind must be an explicit choice, not a config typo.
        #[arg(long)]
        allow_non_loopback: bool,
        /// Path to Q-Deck's built static assets (`apps/q-deck/dist`). When
        /// given, this process serves the shell same-origin with the API —
        /// o7d's documented production role. Omit in dev, where Vite's own
        /// dev server serves the shell and proxies `/api` here instead.
        #[arg(long)]
        static_dir: Option<PathBuf>,
        /// Q-Deck R1 (`docs/q-deck/r1-command.md` §9.4): the target repo a
        /// command's child run continues in. Fixed, server-side execution
        /// authority — never taken from an HTTP request. Enables the `POST
        /// .../commands` mutation route; omitted, that route always answers
        /// `500` naming the missing configuration (R0's read-only posture
        /// is otherwise unchanged).
        #[arg(long)]
        repo: Option<PathBuf>,
        /// Worktree root for a command's child run (mirrors `o7 run
        /// --worktree-root`).
        #[arg(long, default_value = ".worktrees")]
        worktree_root: PathBuf,
        /// Private run store root for a command's child run (mirrors `o7
        /// run --runs-dir`).
        #[arg(long, default_value = "runs")]
        runs_dir: PathBuf,
        /// Gate manifest for a command's child run (default: `<repo>/.007/
        /// gate.toml`, mirrors `o7 run --gate`).
        #[arg(long)]
        gate: Option<PathBuf>,
        /// The `o7` executable to spawn for `o7 continue` — a bare name is
        /// resolved via `PATH`; an explicit path pins exactly which build
        /// runs (used by the process-level E2E test).
        #[arg(long, default_value = "o7")]
        o7_bin: PathBuf,
        /// Fixed model for a command's continuation — never client-supplied
        /// (§8's forbidden-inputs list).
        #[arg(long, default_value = "opus")]
        model: String,
        /// Max agent turns for a command's continuation.
        #[arg(long, default_value_t = 12)]
        max_turns: u32,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let Command::Serve {
        ledger,
        listen,
        allow_non_loopback,
        static_dir,
        repo,
        worktree_root,
        runs_dir,
        gate,
        o7_bin,
        model,
        max_turns,
    } = cli.command;

    if !listen.ip().is_loopback() && !allow_non_loopback {
        anyhow::bail!(
            "refusing to bind non-loopback address {listen}: pass --allow-non-loopback \
             to confirm this is intentional. o7d has no public-internet authentication \
             in R0 — see docs/q-deck/architecture.md."
        );
    }

    let exec = repo.map(|repo| o7d::ExecutionConfig {
        o7_bin,
        ledger_path: ledger.clone(),
        repo,
        worktree_root,
        runs_dir,
        gate,
        model,
        max_turns,
    });

    let ledger_conn = o7_ledger::SqliteLedger::open(&ledger)?;
    let app = o7d::app_with_exec(ledger_conn, static_dir.as_deref(), exec.clone());
    let listener = tokio::net::TcpListener::bind(listen).await?;
    // Report the ACTUAL bound address, not the `--listen` argument as typed:
    // for `--listen 127.0.0.1:0` (ask the OS for any free port) those are
    // different, and printing the raw `0` back is a useless log line no
    // caller can actually connect to — a real, latent bug surfaced while
    // building Q-Deck R0.5's real-subprocess daemon-restart test, which
    // needs to parse this line to learn which port a `--listen ...:0`
    // instance actually bound.
    let bound_addr = listener.local_addr()?;
    eprintln!("o7d: listening on http://{bound_addr}");
    if static_dir.is_none() {
        eprintln!("o7d: no --static-dir given — serving /api/v1 only (dev mode)");
    }
    if exec.is_none() {
        eprintln!("o7d: no --repo given — command-continuation mutation route disabled (500)");
    }
    axum::serve(listener, app).await?;
    Ok(())
}
