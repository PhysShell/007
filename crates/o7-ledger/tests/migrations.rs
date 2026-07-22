//! Acceptance: migrations apply on an empty DB, re-applying is safe, a corrupt
//! DB fails closed, a too-new schema is refused, a DB that only CLAIMS the
//! current version but lacks a table fails closed, and the durability pragmas
//! are actually effective.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use o7_ledger::migrations::CURRENT_SCHEMA_VERSION;
use o7_ledger::SqliteLedger;
use rusqlite::Connection;

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

// A database created by a NEWER schema version is refused (an older binary must
// not write a newer DB).
#[tokio::test]
async fn schema_too_new_fails_closed() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("too-new.db");

    SqliteLedger::open(&path).unwrap();
    {
        let conn = Connection::open(&path).unwrap();
        conn.pragma_update(None, "user_version", CURRENT_SCHEMA_VERSION + 1)
            .unwrap();
    }

    let err = SqliteLedger::open(&path).unwrap_err();
    assert_eq!(err.code(), "SCHEMA_TOO_NEW");
}

// A DB whose user_version CLAIMS the current version but is missing a table
// fails closed (structural schema validation).
#[tokio::test]
async fn claimed_current_version_missing_table_fails_closed() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("claims-v1.db");

    {
        let conn = Connection::open(&path).unwrap();
        // Claim the current version but create NO tables.
        conn.pragma_update(None, "user_version", CURRENT_SCHEMA_VERSION)
            .unwrap();
    }

    let err = SqliteLedger::open(&path).unwrap_err();
    assert_eq!(err.code(), "INTEGRITY");
}

// The durability pragmas are actually effective on a file-backed ledger.
#[tokio::test]
async fn effective_pragmas_are_verified() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pragmas.db");
    let ledger = SqliteLedger::open(&path).unwrap();

    let report = ledger.pragma_report().await.unwrap();
    assert_eq!(report.journal_mode.to_lowercase(), "wal");
    assert!(report.foreign_keys, "foreign_keys must be ON");
    assert_eq!(report.synchronous, 2, "synchronous must be FULL(2)");
}

// A DB with all the right table/column NAMES but WITHOUT the safety constraints
// (composite FKs, the CHECK, the partial unique index) must fail closed — schema
// validation compares full DDL, not just column names.
#[tokio::test]
async fn v1_lookalike_without_constraints_fails_closed() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("lookalike.db");
    {
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(
            "CREATE TABLE conversation (conversation_id TEXT PRIMARY KEY, created_at INTEGER NOT NULL, status TEXT NOT NULL);
             CREATE TABLE run (run_id TEXT PRIMARY KEY, conversation_id TEXT NOT NULL, parent_run_id TEXT, agent TEXT NOT NULL, role TEXT NOT NULL, status TEXT NOT NULL, created_at INTEGER NOT NULL, finished_at INTEGER);
             CREATE TABLE run_attempt (attempt_id TEXT PRIMARY KEY, run_id TEXT NOT NULL, attempt_number INTEGER NOT NULL, status TEXT NOT NULL, started_at INTEGER NOT NULL, finished_at INTEGER);
             CREATE TABLE event (event_id TEXT PRIMARY KEY, conversation_id TEXT NOT NULL, run_id TEXT, attempt_id TEXT, sequence INTEGER NOT NULL, event_type TEXT NOT NULL, schema_version INTEGER NOT NULL, created_at INTEGER NOT NULL, payload_json TEXT NOT NULL);
             CREATE TABLE idempotency_record (scope TEXT NOT NULL, key TEXT NOT NULL, request_digest TEXT NOT NULL, result_reference TEXT NOT NULL, created_at INTEGER NOT NULL, PRIMARY KEY (scope, key));",
        )
        .unwrap();
        conn.pragma_update(None, "user_version", CURRENT_SCHEMA_VERSION)
            .unwrap();
    }

    let err = SqliteLedger::open(&path).unwrap_err();
    assert_eq!(
        err.code(),
        "INTEGRITY",
        "a v1-lookalike missing safety constraints must fail closed"
    );
}

// The too-new guard must run BEFORE any persistent change: a too-new DB left in
// journal_mode=DELETE must be rejected AND left in DELETE (not switched to WAL).
#[tokio::test]
async fn too_new_guard_precedes_persistent_changes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("too-new-delete.db");
    {
        let conn = Connection::open(&path).unwrap();
        conn.pragma_update(None, "user_version", CURRENT_SCHEMA_VERSION + 5)
            .unwrap();
        let mode: String = conn
            .query_row("PRAGMA journal_mode;", [], |r| r.get(0))
            .unwrap();
        assert_eq!(mode.to_lowercase(), "delete", "precondition: DELETE mode");
    }

    let err = SqliteLedger::open(&path).unwrap_err();
    assert_eq!(err.code(), "SCHEMA_TOO_NEW");

    // Must NOT have switched the newer DB to WAL.
    let conn = Connection::open(&path).unwrap();
    let mode: String = conn
        .query_row("PRAGMA journal_mode;", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        mode.to_lowercase(),
        "delete",
        "a too-new DB must be left untouched (not switched to WAL)"
    );
}
