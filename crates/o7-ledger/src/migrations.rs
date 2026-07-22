//! Versioned schema migrations, tracked with SQLite's `user_version` pragma.
//! Applying migrations is idempotent: already-applied versions are skipped, an
//! empty database gets the full set, and re-running is a no-op. A database whose
//! `user_version` is NEWER than this build supports is refused (an older binary
//! must never write a newer schema). After migrating, the FULL live schema —
//! tables, indexes, foreign keys, CHECK constraints, the partial unique index —
//! is compared against a fresh reference built from this build's SCHEMA_V1, so a
//! database that merely CLAIMS the current version but is missing a safety
//! constraint (not just a column) fails closed.

use std::collections::BTreeMap;

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
    UNIQUE(conversation_id, run_id),
    FOREIGN KEY (conversation_id) REFERENCES conversation(conversation_id),
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
    UNIQUE(run_id, attempt_id),
    FOREIGN KEY (run_id) REFERENCES run(run_id)
);
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
    CHECK (attempt_id IS NULL OR run_id IS NOT NULL),
    FOREIGN KEY (conversation_id) REFERENCES conversation(conversation_id),
    FOREIGN KEY (conversation_id, run_id) REFERENCES run(conversation_id, run_id),
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

/// Collapse whitespace so incidental formatting differences don't cause false
/// mismatches; structural differences (a missing FK/CHECK/index) still show.
fn normalize_ddl(sql: &str) -> String {
    sql.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Read every user-defined `sqlite_master` object — tables, indexes, TRIGGERS and
/// VIEWS — keyed by name and whitespace-normalized. Triggers/views matter because
/// an unexpected trigger could silently sabotage the append-only ledger (e.g.
/// delete a row right after it is inserted). Internal `sqlite_%` objects (incl.
/// auto-indexes from PK/UNIQUE, whose `sql` is NULL) are excluded — the
/// constraints they back live inside the table DDL, which IS compared, and users
/// cannot create `sqlite_`-prefixed objects.
fn schema_objects(conn: &Connection) -> Result<BTreeMap<String, String>, LedgerError> {
    let mut stmt = conn.prepare(
        "SELECT name, sql FROM sqlite_master \
         WHERE type IN ('table', 'index', 'trigger', 'view') \
         AND sql IS NOT NULL AND name NOT LIKE 'sqlite_%'",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    let mut map = BTreeMap::new();
    for row in rows {
        let (name, sql) = row?;
        map.insert(name, normalize_ddl(&sql));
    }
    Ok(map)
}

/// Attest the LIVE schema against a fresh reference built from this build's
/// `SCHEMA_V1`, requiring EXACT equality of the full object set — every table,
/// index, trigger and view, with matching DDL (columns, foreign keys, CHECK
/// constraints, the partial unique index). A missing, differing, OR **unexpected**
/// object fails closed. The last case matters: an extra trigger/view could
/// silently subvert append-only guarantees, so anything not in the reference is
/// rejected.
///
/// # Errors
/// [`LedgerError::Integrity`] on any missing, differing, or unexpected object;
/// SQLite errors.
pub fn validate_schema(conn: &Connection) -> Result<(), LedgerError> {
    let reference = Connection::open_in_memory()?;
    reference.execute_batch(SCHEMA_V1)?;
    let expected = schema_objects(&reference)?;
    let actual = schema_objects(conn)?;

    // Reject any object the reference does not define (extra trigger/view/table/…).
    for name in actual.keys() {
        if !expected.contains_key(name) {
            return Err(LedgerError::Integrity(format!(
                "unexpected schema object `{name}` (possible tampering)"
            )));
        }
    }
    // Every expected object must be present and identical.
    for (name, want) in &expected {
        match actual.get(name) {
            None => {
                return Err(LedgerError::Integrity(format!(
                    "schema is missing object `{name}`"
                )))
            }
            Some(have) if have != want => {
                return Err(LedgerError::Integrity(format!(
                    "schema object `{name}` does not match the expected v{CURRENT_SCHEMA_VERSION} definition"
                )))
            }
            Some(_) => {}
        }
    }
    Ok(())
}
