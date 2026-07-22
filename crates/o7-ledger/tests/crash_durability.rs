//! Acceptance (9, 10): real process aborts. A helper subprocess is spawned,
//! reaches a controlled point, prints `READY <id>`, and is SIGKILLed by the
//! parent — a genuine crash, not a simulated `return Err`.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::io::{BufRead as _, BufReader};
use std::process::{Command, Stdio};

use o7_ledger::{ConversationId, Ledger, SqliteLedger};

const HELPER: &str = env!("CARGO_BIN_EXE_ledger-crash-helper");

/// Spawn the helper, read its `READY <id>` line, then SIGKILL it. Returns the id.
fn spawn_until_ready_then_kill(args: &[&str]) -> String {
    let mut child = Command::new(HELPER)
        .args(args)
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let stdout = child.stdout.take().unwrap();
    let mut reader = BufReader::new(stdout);
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    assert!(
        line.starts_with("READY "),
        "unexpected helper output: {line:?}"
    );
    let id = line.trim().strip_prefix("READY ").unwrap().to_owned();
    // Child::kill sends SIGKILL on Unix — a real, uncatchable process abort.
    child.kill().unwrap();
    let _ = child.wait().unwrap();
    id
}

// (9) A SIGKILL AFTER commit does not lose the event.
#[tokio::test]
async fn kill_after_commit_preserves_event() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ledger.db");
    let path_str = path.to_str().unwrap();

    let id = spawn_until_ready_then_kill(&["commit", path_str]);

    let ledger = SqliteLedger::open(&path).unwrap();
    let events = ledger
        .read_events(&ConversationId::from_raw(id), None, 100)
        .await
        .unwrap();
    assert!(
        events
            .iter()
            .any(|e| e.event_type == "conversation.created"),
        "committed conversation must survive a post-commit kill"
    );
}

// (10) A SIGKILL BEFORE commit leaves no partial record.
#[tokio::test]
async fn kill_before_commit_leaves_no_partial() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ledger.db");
    let path_str = path.to_str().unwrap();

    // Parent migrates the schema first so the helper's raw connection can insert.
    let setup = SqliteLedger::open(&path).unwrap();
    drop(setup);

    let chosen_id = "pre-chosen-uncommitted-id";
    let id = spawn_until_ready_then_kill(&["before-commit", path_str, chosen_id]);
    assert_eq!(id, chosen_id);

    let ledger = SqliteLedger::open(&path).unwrap();
    let conversation = ledger
        .conversation(ConversationId::from_raw(chosen_id.to_owned()))
        .await
        .unwrap();
    assert!(
        conversation.is_none(),
        "an uncommitted insert must be absent after a pre-commit kill"
    );
}
