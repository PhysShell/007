//! Versioned schema migrations, tracked with SQLite's `user_version` pragma.
//! Applying migrations is idempotent: already-applied versions are skipped, an
//! empty database gets the full set, and re-running is a no-op.

use rusqlite::{Connection, TransactionBehavior};

use crate::LedgerError;

/// Highest schema version this build knows about.
pub const CURRENT_SCHEMA_VERSION: u32 = 1;

/// Ordered `(version, sql)` migrations. Never edit a shipped migration in place —
/// add a new one.
const MIGRATIONS: &[(u32, &str)] = &[(1, SCHEMA_V1)];

const SCHEMA_V1: &str = "
CREATE TABLE conversation (
    conversation_id TEXT PRIMARY KEY,
    created_at      INTEGER NOT NULL,
    status          TEXT NOT NULL
);

CREATE TABLE run (
    run_id          TEXT PRIMARY KEY,
    conversation_id TEXT NOT NULL REFERENCES conversation(conversation_id),
    parent_run_id   TEXT REFERENCES run(run_id),
    agent           TEXT NOT NULL,
    role            TEXT NOT NULL,
    status          TEXT NOT NULL,
    created_at      INTEGER NOT NULL,
    finished_at     INTEGER
);
CREATE INDEX idx_run_conversation ON run(conversation_id);

CREATE TABLE run_attempt (
    attempt_id     TEXT PRIMARY KEY,
    run_id         TEXT NOT NULL REFERENCES run(run_id),
    attempt_number INTEGER NOT NULL,
    status         TEXT NOT NULL,
    started_at     INTEGER NOT NULL,
    finished_at    INTEGER,
    UNIQUE(run_id, attempt_number)
);

CREATE TABLE event (
    event_id        TEXT PRIMARY KEY,
    conversation_id TEXT NOT NULL REFERENCES conversation(conversation_id),
    run_id          TEXT REFERENCES run(run_id),
    attempt_id      TEXT REFERENCES run_attempt(attempt_id),
    sequence        INTEGER NOT NULL,
    event_type      TEXT NOT NULL,
    schema_version  INTEGER NOT NULL,
    created_at      INTEGER NOT NULL,
    payload_json    TEXT NOT NULL,
    UNIQUE(conversation_id, sequence)
);
CREATE INDEX idx_event_conversation_sequence ON event(conversation_id, sequence);

CREATE TABLE idempotency_record (
    scope            TEXT NOT NULL,
    key              TEXT NOT NULL,
    request_digest   TEXT NOT NULL,
    result_reference TEXT NOT NULL,
    created_at       INTEGER NOT NULL,
    PRIMARY KEY (scope, key)
);
";

/// Read the currently-applied schema version.
///
/// # Errors
/// Propagates any SQLite error.
pub fn current_version(conn: &Connection) -> Result<u32, LedgerError> {
    let v: i64 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
    Ok(u32::try_from(v).unwrap_or(0))
}

/// Apply all pending migrations in a single transaction. Safe to call on every
/// open: an up-to-date database is left untouched.
///
/// # Errors
/// Propagates SQLite errors; the transaction rolls back on failure so a partial
/// migration is never left behind.
pub fn apply(conn: &mut Connection) -> Result<(), LedgerError> {
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let start: i64 = tx.pragma_query_value(None, "user_version", |row| row.get(0))?;
    for (version, sql) in MIGRATIONS {
        if i64::from(*version) > start {
            tx.execute_batch(sql)?;
            // pragma_update cannot bind parameters; the version is a trusted constant.
            tx.pragma_update(None, "user_version", *version)?;
        }
    }
    tx.commit()?;
    Ok(())
}
