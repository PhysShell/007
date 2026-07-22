//! Test-only helper spawned as a real subprocess by the crash-durability test.
//! It reaches a controlled point, prints a `READY <id>` line, then blocks
//! forever so the parent can SIGKILL it — a genuine process abort, not a
//! simulated `return Err`.
//!
//! Usage:
//!   ledger-crash-helper commit <db_path>
//!       Open the ledger, create a conversation (which COMMITS), print its id,
//!       then wait. After the parent kills it, the conversation must persist.
//!   ledger-crash-helper before-commit <db_path> <conversation_id>
//!       Open a raw connection, BEGIN IMMEDIATE, INSERT the conversation but do
//!       NOT commit, then wait. After the kill, the row must be ABSENT.

#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::io::Write as _;
use std::thread::sleep;
use std::time::Duration;

use rusqlite::{Connection, TransactionBehavior};

use o7_ledger::SqliteLedger;

fn main() {
    if let Err(err) = run() {
        eprintln!("crash-helper error: {err}");
        std::process::exit(2);
    }
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    let mode = args.get(1).ok_or("missing mode")?;
    let db = args.get(2).ok_or("missing db path")?;
    match mode.as_str() {
        "commit" => commit_then_wait(db),
        "before-commit" => {
            let id = args.get(3).ok_or("missing conversation_id")?;
            insert_then_wait_before_commit(db, id)
        }
        other => Err(format!("unknown mode {other}")),
    }
}

fn commit_then_wait(db: &str) -> Result<(), String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .map_err(|e| e.to_string())?;
    let ledger = SqliteLedger::open(db).map_err(|e| e.to_string())?;
    let conversation = runtime
        .block_on(ledger.create_conversation(None))
        .map_err(|e| e.to_string())?;
    announce(&conversation.conversation_id.to_string())?;
    wait_forever()
}

fn insert_then_wait_before_commit(db: &str, conversation_id: &str) -> Result<(), String> {
    let mut conn = Connection::open(db).map_err(|e| e.to_string())?;
    conn.busy_timeout(Duration::from_millis(5000))
        .map_err(|e| e.to_string())?;
    conn.execute_batch("PRAGMA foreign_keys = ON; PRAGMA synchronous = FULL;")
        .map_err(|e| e.to_string())?;
    let _mode: String = conn
        .query_row("PRAGMA journal_mode = WAL;", [], |row| row.get(0))
        .map_err(|e| e.to_string())?;
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|e| e.to_string())?;
    tx.execute(
        "INSERT INTO conversation (conversation_id, created_at, status) VALUES (?1, 0, 'open')",
        [conversation_id],
    )
    .map_err(|e| e.to_string())?;
    // Announce, then hold the OPEN (uncommitted) transaction until we are killed.
    announce(conversation_id)?;
    // `tx` is intentionally not committed and kept alive across the wait.
    let _hold = tx;
    wait_forever()
}

fn announce(id: &str) -> Result<(), String> {
    println!("READY {id}");
    std::io::stdout().flush().map_err(|e| e.to_string())
}

fn wait_forever() -> Result<(), String> {
    loop {
        sleep(Duration::from_millis(200));
    }
}
