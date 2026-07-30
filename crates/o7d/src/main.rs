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
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let Command::Serve {
        ledger,
        listen,
        allow_non_loopback,
    } = cli.command;

    if !listen.ip().is_loopback() && !allow_non_loopback {
        anyhow::bail!(
            "refusing to bind non-loopback address {listen}: pass --allow-non-loopback \
             to confirm this is intentional. o7d has no public-internet authentication \
             in R0 — see docs/q-deck/architecture.md."
        );
    }

    let ledger = o7_ledger::SqliteLedger::open(&ledger)?;
    let app = o7d::router(ledger);
    let listener = tokio::net::TcpListener::bind(listen).await?;
    eprintln!("o7d: listening on http://{listen}");
    axum::serve(listener, app).await?;
    Ok(())
}
