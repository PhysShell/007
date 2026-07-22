//! Acceptance (9, 10): real process aborts. Instead of shipping a helper binary,
//! the parent RE-EXECS this test binary to run one of the `#[ignore]`d "child"
//! tests below, reads its `READY <id>` line, then SIGKILLs it — a genuine crash,
//! not a simulated `return Err`.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::io::{BufRead as _, BufReader, Write as _};
use std::process::{Child, Command, Stdio};
use std::thread::sleep;
use std::time::Duration;

use o7_ledger::{ConversationId, Ledger, SqliteLedger};

const ENV_DB: &str = "O7_LEDGER_CRASH_DB";
const ENV_ID: &str = "O7_LEDGER_CRASH_ID";

/// Re-exec this test binary to run one ignored child test, and read its
/// `READY <id>` line. Returns the still-running child and the id — the caller
/// SIGKILLs and waits on it (that is the whole point of the test).
#[allow(clippy::zombie_processes)]
fn spawn_child(child_test: &str, db: &str, id: Option<&str>) -> (Child, String) {
    let exe = std::env::current_exe().unwrap();
    let mut cmd = Command::new(exe);
    cmd.args(["--ignored", "--exact", "--nocapture", child_test])
        .env(ENV_DB, db)
        .stdout(Stdio::piped());
    if let Some(id) = id {
        cmd.env(ENV_ID, id);
    }
    let mut child = cmd.spawn().unwrap();
    let stdout = child.stdout.take().unwrap();
    let mut reader = BufReader::new(stdout);
    let mut line = String::new();
    loop {
        line.clear();
        let read = reader.read_line(&mut line).unwrap();
        assert!(read != 0, "child exited before printing READY");
        if let Some(rest) = line.trim().strip_prefix("READY ") {
            return (child, rest.to_owned());
        }
        // otherwise skip libtest preamble lines ("running 1 test", blanks, …)
    }
}

fn kill(mut child: Child) {
    // Child::kill sends SIGKILL on Unix — a real, uncatchable abort.
    child.kill().unwrap();
    let _ = child.wait().unwrap();
}

// (9) A SIGKILL AFTER commit does not lose the event.
#[tokio::test]
async fn kill_after_commit_preserves_event() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ledger.db");
    let path_str = path.to_str().unwrap();

    let (child, id) = spawn_child("child_commit_then_wait", path_str, None);
    kill(child);

    let ledger = SqliteLedger::open(&path).unwrap();
    let events = ledger
        .read_events(&ConversationId::from_raw(id), None, 100)
        .await
        .unwrap();
    assert!(
        events
            .iter()
            .any(|e| e.event_type == "conversation.created"),
        "a committed conversation must survive a post-commit SIGKILL"
    );
}

// (10) A SIGKILL BEFORE commit leaves no partial record.
#[tokio::test]
async fn kill_before_commit_leaves_no_partial() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ledger.db");
    let path_str = path.to_str().unwrap();

    // Parent migrates the schema first so the child's raw connection can insert.
    let setup = SqliteLedger::open(&path).unwrap();
    drop(setup);

    let chosen = "pre-chosen-uncommitted-id";
    let (child, id) = spawn_child("child_insert_before_commit", path_str, Some(chosen));
    assert_eq!(id, chosen);
    kill(child);

    let ledger = SqliteLedger::open(&path).unwrap();
    let conversation = ledger
        .conversation(ConversationId::from_raw(chosen.to_owned()))
        .await
        .unwrap();
    assert!(
        conversation.is_none(),
        "an uncommitted insert must be absent after a pre-commit SIGKILL"
    );
}

// ---- children (spawned by re-exec; #[ignore]d so they never run in a normal
//      `cargo test`). Each prints `READY <id>` and then blocks forever. ----

fn announce(id: &str) {
    println!("READY {id}");
    std::io::stdout().flush().unwrap();
}

fn wait_forever() -> ! {
    loop {
        sleep(Duration::from_millis(200));
    }
}

#[tokio::test]
#[ignore = "spawned as a subprocess by kill_after_commit_preserves_event"]
async fn child_commit_then_wait() {
    let db = std::env::var(ENV_DB).unwrap();
    let ledger = SqliteLedger::open(&db).unwrap();
    let conversation = ledger.create_conversation(None).await.unwrap(); // COMMITS
    announce(&conversation.conversation_id.to_string());
    wait_forever();
}

#[tokio::test]
#[ignore = "spawned as a subprocess by kill_before_commit_leaves_no_partial"]
async fn child_insert_before_commit() {
    use rusqlite::{Connection, TransactionBehavior};
    let db = std::env::var(ENV_DB).unwrap();
    let id = std::env::var(ENV_ID).unwrap();
    let mut conn = Connection::open(&db).unwrap();
    conn.busy_timeout(Duration::from_millis(5000)).unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON; PRAGMA synchronous = FULL;")
        .unwrap();
    let _mode: String = conn
        .query_row("PRAGMA journal_mode = WAL;", [], |r| r.get(0))
        .unwrap();
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    tx.execute(
        "INSERT INTO conversation (conversation_id, created_at, status) VALUES (?1, 0, 'open')",
        [&id],
    )
    .unwrap();
    announce(&id);
    // Hold the OPEN (uncommitted) transaction until we are killed.
    let _hold = tx;
    wait_forever();
}
