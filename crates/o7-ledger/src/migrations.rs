//! Versioned schema migrations, tracked with SQLite's `user_version` pragma.
//! Applying migrations is idempotent: already-applied versions are skipped, an
//! empty database gets the full set, and re-running is a no-op. A database whose
//! `user_version` is NEWER than this build supports is refused (an older binary
//! must never write a newer schema). After migrating, the actual schema is
//! validated so a database that merely CLAIMS to be v1 but lacks a table/column
//! fails closed.

use rusqlite::{Connection, TransactionBehavior};

use crate::LedgerError;

/// Highest schema version this build knows about.
pub const CURRENT_SCHEMA_VERSION: u32 = 1;

/// Ordered `(version, sql)` migrations. Never edit a SHIPPED migration in place —
/// add a new one. (v1 has not shipped outside this PR, so it is still authored
/// here directly.)
const MIGRATIONS: &[(u32, &str)] = &[(1, SCHEMA_V1)];

const SCHEMA_V1: &str = "
CREATE TABLE conversation (
    conversation_id TEXT PRIMARY KEY,
    created_at      INTEGER NOT NULL,
    status          TEXT NOT NULL
);

CREATE TABLE run (
    run_id          TEXT PRIMARY KEY,
    conversation_id TEXT NOT NULL,
    parent_run_id   TEXT,
    agent           TEXT NOT NULL,
    role            TEXT NOT NULL,
    status          TEXT NOT NULL,
    created_at      INTEGER NOT NULL,
    finished_at     INTEGER,
    -- target for the composite foreign keys below (conversation-scoped identity)
    UNIQUE(conversation_id, run_id),
    FOREIGN KEY (conversation_id) REFERENCES conversation(conversation_id),
    -- a parent run MUST live in the same conversation
    FOREIGN KEY (conversation_id, parent_run_id) REFERENCES run(conversation_id, run_id)
);
CREATE INDEX idx_run_conversation ON run(conversation_id);

CREATE TABLE run_attempt (
    attempt_id     TEXT PRIMARY KEY,
    run_id         TEXT NOT NULL,
    attempt_number INTEGER NOT NULL,
    status         TEXT NOT NULL,
    started_at     INTEGER NOT NULL,
    finished_at    INTEGER,
    UNIQUE(run_id, attempt_number),
    -- target for the composite (run_id, attempt_id) foreign key on event
    UNIQUE(run_id, attempt_id),
    FOREIGN KEY (run_id) REFERENCES run(run_id)
);
-- At most ONE running attempt per run.
CREATE UNIQUE INDEX idx_one_running_attempt ON run_attempt(run_id) WHERE status = 'running';

CREATE TABLE event (
    event_id        TEXT PRIMARY KEY,
    conversation_id TEXT NOT NULL,
    run_id          TEXT,
    attempt_id      TEXT,
    sequence        INTEGER NOT NULL,
    event_type      TEXT NOT NULL,
    schema_version  INTEGER NOT NULL,
    created_at      INTEGER NOT NULL,
    payload_json    TEXT NOT NULL,
    UNIQUE(conversation_id, sequence),
    -- an attempt_id is only meaningful together with its run_id
    CHECK (attempt_id IS NULL OR run_id IS NOT NULL),
    FOREIGN KEY (conversation_id) REFERENCES conversation(conversation_id),
    -- the referenced run MUST belong to this event's conversation
    FOREIGN KEY (conversation_id, run_id) REFERENCES run(conversation_id, run_id),
    -- the referenced attempt MUST belong to this event's run
    FOREIGN KEY (run_id, attempt_id) REFERENCES run_attempt(run_id, attempt_id)
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

/// Expected tables and their columns for the current schema. Used by
/// [`validate_schema`] so a database that merely claims the current
/// `user_version` but lacks a table/column is rejected.
const EXPECTED_SCHEMA: &[(&str, &[&str])] = &[
    ("conversation", &["conversation_id", "created_at", "status"]),
    (
        "run",
        &[
            "run_id",
            "conversation_id",
            "parent_run_id",
            "agent",
            "role",
            "status",
            "created_at",
            "finished_at",
        ],
    ),
    (
        "run_attempt",
        &[
            "attempt_id",
            "run_id",
            "attempt_number",
            "status",
            "started_at",
            "finished_at",
        ],
    ),
    (
        "event",
        &[
            "event_id",
            "conversation_id",
            "run_id",
            "attempt_id",
            "sequence",
            "event_type",
            "schema_version",
            "created_at",
            "payload_json",
        ],
    ),
    (
        "idempotency_record",
        &[
            "scope",
            "key",
            "request_digest",
            "result_reference",
            "created_at",
        ],
    ),
];

/// Read the currently-applied schema version.
///
/// # Errors
/// Propagates any SQLite error.
pub fn current_version(conn: &Connection) -> Result<u32, LedgerError> {
    let v: i64 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
    Ok(u32::try_from(v).unwrap_or(0))
}

/// Apply all pending migrations in a single transaction. Refuses a database
/// newer than this build. Safe to call on every open.
///
/// # Errors
/// [`LedgerError::SchemaTooNew`] if the DB is newer than supported; SQLite errors
/// (the transaction rolls back so a partial migration is never left behind).
pub fn apply(conn: &mut Connection) -> Result<(), LedgerError> {
    let start = current_version(conn)?;
    if start > CURRENT_SCHEMA_VERSION {
        return Err(LedgerError::SchemaTooNew {
            found: start,
            supported: CURRENT_SCHEMA_VERSION,
        });
    }
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    for (version, sql) in MIGRATIONS {
        if i64::from(*version) > i64::from(start) {
            tx.execute_batch(sql)?;
            // pragma_update cannot bind parameters; the version is a trusted constant.
            tx.pragma_update(None, "user_version", *version)?;
        }
    }
    tx.commit()?;
    Ok(())
}

/// Verify the live schema matches [`EXPECTED_SCHEMA`] — every expected table and
/// column must exist. Catches a database that claims the current `user_version`
/// but is structurally incomplete.
///
/// # Errors
/// [`LedgerError::Integrity`] on a missing table or column; SQLite errors.
pub fn validate_schema(conn: &Connection) -> Result<(), LedgerError> {
    for (table, columns) in EXPECTED_SCHEMA {
        let exists: i64 = conn.query_row(
            "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
            [table],
            |row| row.get(0),
        )?;
        if exists == 0 {
            return Err(LedgerError::Integrity(format!("missing table `{table}`")));
        }

        let mut present = std::collections::BTreeSet::new();
        {
            // `table` is one of our trusted constants, not user input.
            let mut stmt = conn.prepare(&format!("PRAGMA table_info(\"{table}\")"))?;
            let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
            for row in rows {
                present.insert(row?);
            }
        }
        for column in *columns {
            if !present.contains(*column) {
                return Err(LedgerError::Integrity(format!(
                    "table `{table}` is missing column `{column}`"
                )));
            }
        }
    }
    Ok(())
}
