//! Versioned schema migrations, tracked with SQLite's `user_version` pragma.
//! Applying migrations is idempotent: already-applied versions are skipped, an
//! empty database gets the full set, and re-running is a no-op. A database whose
//! `user_version` is NEWER than this build supports is refused (an older binary
//! must never write a newer schema). After migrating, the FULL live schema —
//! tables, indexes, foreign keys, CHECK constraints, the partial unique index —
//! is compared against a fresh reference built by running every migration in
//! order on an empty in-memory database, so a database that merely CLAIMS the
//! current version but is missing a safety constraint (not just a column) fails
//! closed.

use std::collections::BTreeMap;

use rusqlite::{Connection, TransactionBehavior};

use crate::LedgerError;

/// Highest schema version this build knows about.
pub const CURRENT_SCHEMA_VERSION: u32 = 3;

/// Ordered `(version, sql)` migrations. Never edit a SHIPPED migration in place —
/// add a new one.
const MIGRATIONS: &[(u32, &str)] = &[(1, SCHEMA_V1), (2, SCHEMA_V2), (3, SCHEMA_V3)];

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

/// Q-Deck R0.6 (`docs/q-deck/r06-verdict-fidelity.md`): rebuild `run` and
/// `run_attempt` with explicit `CHECK` constraints enumerating their now-closed
/// status vocabularies (`blocked`/`error` added alongside the existing set).
/// `event.event_type` is deliberately NOT constrained — it stays
/// forward-compatible by design (see its doc comment in `models.rs`); only
/// `run.status`/`run_attempt.status` are tightly governed by the central
/// transition tables and get this hardening.
///
/// SQLite has no `ALTER TABLE ADD CONSTRAINT` — a `CHECK` constraint requires
/// the standard rebuild sequence (create new → copy → drop old → rename).
/// `run_attempt`'s `FOREIGN KEY (run_id) REFERENCES run(run_id)`, and
/// `event`'s FKs to both tables, are automatically rewritten to the renamed
/// table by SQLite's own `ALTER TABLE ... RENAME TO` (not `legacy_alter_table`)
/// — this migration does not touch `event` at all. Must run with
/// `foreign_keys` OFF around it (see [`apply`]'s per-migration toggle) since
/// toggling that pragma inside an active transaction is a no-op, and the
/// interim state (old table dropped, FK-referencing tables briefly pointing
/// at a table that doesn't exist yet under that name) would otherwise trip
/// enforcement mid-rebuild.
const SCHEMA_V2: &str = "
CREATE TABLE run_v2 (
    run_id          TEXT PRIMARY KEY,
    conversation_id TEXT NOT NULL,
    parent_run_id   TEXT,
    agent           TEXT NOT NULL,
    role            TEXT NOT NULL,
    status          TEXT NOT NULL CHECK (status IN (
        'queued','running','completed','failed','cancelled','interrupted','blocked','error'
    )),
    created_at      INTEGER NOT NULL,
    finished_at     INTEGER,
    UNIQUE(conversation_id, run_id),
    FOREIGN KEY (conversation_id) REFERENCES conversation(conversation_id),
    FOREIGN KEY (conversation_id, parent_run_id) REFERENCES run_v2(conversation_id, run_id)
);
INSERT INTO run_v2 (run_id, conversation_id, parent_run_id, agent, role, status, created_at, finished_at)
    SELECT run_id, conversation_id, parent_run_id, agent, role, status, created_at, finished_at FROM run;
DROP TABLE run;
ALTER TABLE run_v2 RENAME TO run;
CREATE INDEX idx_run_conversation ON run(conversation_id);

CREATE TABLE run_attempt_v2 (
    attempt_id     TEXT PRIMARY KEY,
    run_id         TEXT NOT NULL,
    attempt_number INTEGER NOT NULL,
    status         TEXT NOT NULL CHECK (status IN (
        'running','completed','failed','cancelled','interrupted','blocked','error'
    )),
    started_at     INTEGER NOT NULL,
    finished_at    INTEGER,
    UNIQUE(run_id, attempt_number),
    UNIQUE(run_id, attempt_id),
    FOREIGN KEY (run_id) REFERENCES run(run_id)
);
INSERT INTO run_attempt_v2 (attempt_id, run_id, attempt_number, status, started_at, finished_at)
    SELECT attempt_id, run_id, attempt_number, status, started_at, finished_at FROM run_attempt;
DROP TABLE run_attempt;
ALTER TABLE run_attempt_v2 RENAME TO run_attempt;
CREATE UNIQUE INDEX idx_one_running_attempt ON run_attempt(run_id) WHERE status = 'running';
";

/// Q-Deck R1 (`docs/q-deck/r1-command.md` §9.3): durable command acceptance
/// and provider-session lineage. `run.provider_session_id` is a plain
/// nullable `ADD COLUMN` — no CHECK constraint is added to `run`, so this
/// does not need SCHEMA_V2's rebuild-and-copy dance. `command` is a brand
/// new table; its own `status` vocabulary is CHECK-constrained from the
/// start (nothing to migrate away from, unlike `run`/`run_attempt` in
/// SCHEMA_V2).
const SCHEMA_V3: &str = "
ALTER TABLE run ADD COLUMN provider_session_id TEXT;

CREATE TABLE command (
    command_id      TEXT PRIMARY KEY,
    conversation_id TEXT NOT NULL,
    parent_run_id   TEXT NOT NULL,
    command_text    TEXT NOT NULL,
    status          TEXT NOT NULL CHECK (status IN (
        'accepted','started','completed','rejected'
    )),
    child_run_id    TEXT,
    created_at      INTEGER NOT NULL,
    updated_at      INTEGER NOT NULL,
    FOREIGN KEY (conversation_id) REFERENCES conversation(conversation_id),
    FOREIGN KEY (conversation_id, parent_run_id) REFERENCES run(conversation_id, run_id)
    -- Deliberately NO FK on child_run_id -> run.run_id: bind_command_child_run
    -- durably records the freshly minted child run id BEFORE that run's own
    -- ledger row exists (the whole point of the ordering rule in
    -- docs/q-deck/r1-command.md §3 — durable command->run binding must
    -- precede the provider invocation that eventually creates the child's
    -- RunStarted). A synchronous FK here would make that exact, required
    -- ordering impossible. Referential correctness is instead the
    -- application layer's job (SqliteLedger::bind_command_child_run only
    -- ever writes an id it just minted itself).
);
CREATE INDEX idx_command_conversation ON command(conversation_id);
";

/// Expose one migration's raw SQL by version, for tests that need to build a
/// specific historical schema shape directly (e.g. proving a migration FROM
/// that exact shape preserves data) without hand-duplicating DDL text that
/// would silently drift from the real thing. Not used by `apply` itself
/// (which iterates `MIGRATIONS` directly) — purely a test-support seam.
#[must_use]
pub fn migration_sql(version: u32) -> Option<&'static str> {
    MIGRATIONS
        .iter()
        .find(|(v, _)| *v == version)
        .map(|(_, sql)| *sql)
}

/// Read the currently-applied schema version.
///
/// # Errors
/// Propagates any SQLite error.
pub fn current_version(conn: &Connection) -> Result<u32, LedgerError> {
    let v: i64 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
    Ok(u32::try_from(v).unwrap_or(0))
}

/// Rows returned by `PRAGMA foreign_key_check`, one per violation (empty means
/// no violations). Column 0 is the table with the dangling reference.
fn foreign_key_violations(conn: &Connection) -> Result<Vec<String>, LedgerError> {
    let mut stmt = conn.prepare("PRAGMA foreign_key_check;")?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

/// Apply all pending migrations. Refuses a database newer than this build.
/// Safe to call on every open. Each migration runs in its OWN transaction —
/// not one shared transaction for every pending version — specifically so a
/// table-rebuild migration can toggle `PRAGMA foreign_keys` around itself
/// (a no-op inside a transaction, so it must happen outside one) and verify
/// `foreign_key_check` is clean before committing. A crash between two
/// migrations leaves `user_version` at the last one that fully committed —
/// itself a completely valid, self-consistent state (`apply` simply resumes
/// from there on the next open), not a half-migrated one.
///
/// # Errors
/// [`LedgerError::SchemaTooNew`] if the DB is newer than supported;
/// [`LedgerError::Integrity`] if a rebuild migration introduces a foreign key
/// violation; SQLite errors (each migration's own transaction rolls back so a
/// partial migration is never left behind).
pub fn apply(conn: &mut Connection) -> Result<(), LedgerError> {
    let start = current_version(conn)?;
    if start > CURRENT_SCHEMA_VERSION {
        return Err(LedgerError::SchemaTooNew {
            found: start,
            supported: CURRENT_SCHEMA_VERSION,
        });
    }
    for (version, sql) in MIGRATIONS {
        if i64::from(*version) <= i64::from(start) {
            continue;
        }
        // Foreign key pragma changes are no-ops inside a transaction, so this
        // has to happen at the connection level, before starting one.
        conn.execute_batch("PRAGMA foreign_keys = OFF;")?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute_batch(sql)?;
        // pragma_update cannot bind parameters; the version is a trusted constant.
        tx.pragma_update(None, "user_version", *version)?;
        let violations = foreign_key_violations(&tx)?;
        if !violations.is_empty() {
            return Err(LedgerError::Integrity(format!(
                "migration to v{version} introduced foreign key violations in: {violations:?}"
            )));
        }
        tx.commit()?;
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;
    }
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

/// Attest the LIVE schema against a fresh reference built by running every
/// migration in order on an empty in-memory database — exactly what a brand
/// new database ends up looking like — requiring EXACT equality of the full
/// object set — every table, index, trigger and view, with matching DDL
/// (columns, foreign keys, CHECK constraints, the partial unique index). A
/// missing, differing, OR **unexpected** object fails closed. The last case
/// matters: an extra trigger/view could silently subvert append-only
/// guarantees, so anything not in the reference is rejected.
///
/// # Errors
/// [`LedgerError::Integrity`] on any missing, differing, or unexpected object;
/// SQLite errors.
pub fn validate_schema(conn: &Connection) -> Result<(), LedgerError> {
    let mut reference = Connection::open_in_memory()?;
    apply(&mut reference)?;
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
