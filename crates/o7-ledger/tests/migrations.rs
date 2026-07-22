//! Acceptance (11, 12, 16): migrations apply on an empty DB, re-applying is
//! safe, and a corrupt/unreadable database fails closed.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use o7_ledger::migrations::CURRENT_SCHEMA_VERSION;
use o7_ledger::SqliteLedger;

// (11) Migrations apply on an empty database (schema is usable afterwards).
#[tokio::test]
async fn migration_on_empty_database() {
    assert_eq!(CURRENT_SCHEMA_VERSION, 1);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fresh.db");

    let ledger = SqliteLedger::open(&path).unwrap();
    // If the schema was applied, creating entities works.
    let conv = ledger.create_conversation(None).await.unwrap();
    ledger
        .create_run(
            o7_ledger::NewRun {
                conversation_id: conv.conversation_id.clone(),
                parent_run_id: None,
                agent: "codex".to_owned(),
                role: "implementer".to_owned(),
            },
            None,
        )
        .await
        .unwrap();
}

// (12) Re-applying migrations (re-opening) is safe and preserves data.
#[tokio::test]
async fn reapplying_migrations_is_safe() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("reopen.db");

    let conv_id;
    {
        let ledger = SqliteLedger::open(&path).unwrap();
        let conv = ledger.create_conversation(None).await.unwrap();
        conv_id = conv.conversation_id.clone();
    }
    // Second open runs migrations::apply again — must be a no-op, not an error.
    let ledger = SqliteLedger::open(&path).unwrap();
    let conv = ledger.conversation(conv_id.clone()).await.unwrap();
    assert!(conv.is_some(), "data from before must survive a re-open");
    // Still usable.
    ledger.create_conversation(None).await.unwrap();
}

// (16) A corrupt/unreadable database fails closed at open().
#[tokio::test]
async fn corrupt_database_fails_closed() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("corrupt.db");
    // Not a valid SQLite file (header is not "SQLite format 3\0").
    std::fs::write(
        &path,
        b"this is definitely not a sqlite database \x00\x01\x02\x03",
    )
    .unwrap();

    let result = SqliteLedger::open(&path);
    assert!(
        result.is_err(),
        "opening a corrupt database must fail closed"
    );
}
