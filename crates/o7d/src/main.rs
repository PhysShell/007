//! `o7d serve` — open the ledger, bind, and expose the R0 read-only API.

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
    } = cli.command;

    if !listen.ip().is_loopback() && !allow_non_loopback {
        anyhow::bail!(
            "refusing to bind non-loopback address {listen}: pass --allow-non-loopback \
             to confirm this is intentional. o7d has no public-internet authentication \
             in R0 — see docs/q-deck/architecture.md."
        );
    }

    let ledger = o7_ledger::SqliteLedger::open(&ledger)?;
    let app = o7d::app(ledger, static_dir.as_deref());
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
    axum::serve(listener, app).await?;
    Ok(())
}
