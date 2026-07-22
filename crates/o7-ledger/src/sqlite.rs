//! SQLite-backed ledger. The connection is owned here and never handed to a UI
//! or transport. Every event's per-conversation `sequence` is allocated and the
//! row inserted inside ONE `IMMEDIATE` transaction, so concurrent appends to a
//! conversation get a gap-free, duplicate-free 1,2,3,… sequence and a rolled-back
//! transaction leaves no half-event.

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};

use crate::idempotency::{
    self, IdemOutcome, SCOPE_APPEND_USER_MESSAGE, SCOPE_CREATE_CONVERSATION, SCOPE_CREATE_RUN,
};
use crate::migrations;
use crate::models::{
    Conversation, ConversationStatus, EventType, Idempotency, NewEvent, NewRun, PersistedEvent,
    RecoveryState, Run, RunAttempt, RunStatus,
};
use crate::transitions::{validate_attempt_transition, validate_run_transition};
use crate::{now_millis, AttemptStatus, EventId, Ledger, LedgerError};

/// Hard upper bound on how many events a single [`read_events`](Ledger::read_events)
/// call may return, so a caller can never request an unbounded scan.
pub const MAX_READ_LIMIT: usize = 1000;

/// Schema version stamped on events emitted by the ledger's own lifecycle methods.
pub const EVENT_SCHEMA_VERSION: u32 = 1;

const BUSY_TIMEOUT_MS: u64 = 5000;

/// A durable, append-only ledger backed by a single SQLite database.
#[derive(Clone)]
pub struct SqliteLedger {
    conn: Arc<Mutex<Connection>>,
}

impl SqliteLedger {
    /// Open (creating if absent) a file-backed ledger: set WAL + `FULL` sync +
    /// foreign keys + a bounded busy timeout, run a cheap integrity check, then
    /// apply migrations. Does NOT touch any run/attempt status — recovery is the
    /// caller's explicit decision (see [`recover_scan`](Self::recover_scan)).
    ///
    /// # Errors
    /// Fails closed on an unreadable/corrupt database, a failed integrity check,
    /// or a migration error.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, LedgerError> {
        let conn = Connection::open(path)?;
        Self::init(conn)
    }

    /// Open an in-memory ledger (for tests/logic). WAL/durability pragmas that do
    /// not apply to `:memory:` are simply inert.
    ///
    /// # Errors
    /// Propagates SQLite/migration errors.
    pub fn open_in_memory() -> Result<Self, LedgerError> {
        let conn = Connection::open_in_memory()?;
        Self::init(conn)
    }

    fn init(conn: Connection) -> Result<Self, LedgerError> {
        conn.busy_timeout(Duration::from_millis(BUSY_TIMEOUT_MS))?;
        conn.execute_batch("PRAGMA foreign_keys = ON; PRAGMA synchronous = FULL;")?;
        // journal_mode returns the resulting mode as a row ("wal", or "memory"
        // for an in-memory db); read it rather than pragma_update.
        let _mode: String = conn.query_row("PRAGMA journal_mode = WAL;", [], |row| row.get(0))?;
        // Cheaper justified variant of integrity_check; still detects corruption.
        let check: String = conn.query_row("PRAGMA quick_check;", [], |row| row.get(0))?;
        if check != "ok" {
            return Err(LedgerError::Integrity(check));
        }
        let mut conn = conn;
        migrations::apply(&mut conn)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    async fn with_tx<T, F>(&self, work: F) -> Result<T, LedgerError>
    where
        T: Send + 'static,
        F: FnOnce(&Transaction<'_>) -> Result<T, LedgerError> + Send + 'static,
    {
        let conn = Arc::clone(&self.conn);
        tokio::task::spawn_blocking(move || -> Result<T, LedgerError> {
            let mut guard = conn.lock().map_err(|_| LedgerError::LockPoisoned)?;
            let tx = guard.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let out = work(&tx)?;
            tx.commit()?;
            Ok(out)
        })
        .await
        .map_err(|_| LedgerError::Join)?
    }

    async fn with_conn<T, F>(&self, work: F) -> Result<T, LedgerError>
    where
        T: Send + 'static,
        F: FnOnce(&Connection) -> Result<T, LedgerError> + Send + 'static,
    {
        let conn = Arc::clone(&self.conn);
        tokio::task::spawn_blocking(move || -> Result<T, LedgerError> {
            let guard = conn.lock().map_err(|_| LedgerError::LockPoisoned)?;
            work(&guard)
        })
        .await
        .map_err(|_| LedgerError::Join)?
    }

    /// Create a conversation (and emit `conversation.created`). Optionally
    /// idempotent under scope `create-conversation`.
    ///
    /// # Errors
    /// SQLite errors; [`LedgerError::IdempotencyConflict`] if the key was used
    /// for a different request.
    pub async fn create_conversation(
        &self,
        idempotency: Option<Idempotency>,
    ) -> Result<Conversation, LedgerError> {
        // The create-conversation request carries no parameters, so its digest is
        // a constant: the same key always maps to the same conversation.
        let request_digest = idempotency::digest_bytes(b"create-conversation:v1");
        self.with_tx(move |tx| {
            if let Some(idem) = &idempotency {
                if let IdemOutcome::Replayed(reference) =
                    idempotency::check(tx, SCOPE_CREATE_CONVERSATION, &idem.key, &request_digest)?
                {
                    return load_conversation(tx, &reference)?
                        .ok_or_else(|| LedgerError::NotFound(format!("conversation {reference}")));
                }
            }
            let now = now_millis();
            let conversation = Conversation {
                conversation_id: crate::ConversationId::generate(),
                created_at: now,
                status: ConversationStatus::Open,
            };
            tx.execute(
                "INSERT INTO conversation (conversation_id, created_at, status) VALUES (?1, ?2, ?3)",
                params![
                    conversation.conversation_id.as_str(),
                    now,
                    conversation.status.as_str()
                ],
            )?;
            emit_event(
                tx,
                &NewEvent {
                    event_id: EventId::generate(),
                    conversation_id: conversation.conversation_id.clone(),
                    run_id: None,
                    attempt_id: None,
                    event_type: EventType::ConversationCreated,
                    schema_version: EVENT_SCHEMA_VERSION,
                    payload: serde_json::json!({}),
                },
                now,
            )?;
            if let Some(idem) = &idempotency {
                idempotency::record(
                    tx,
                    SCOPE_CREATE_CONVERSATION,
                    &idem.key,
                    &request_digest,
                    conversation.conversation_id.as_str(),
                    now,
                )?;
            }
            Ok(conversation)
        })
        .await
    }

    /// Create a run in `queued` state (and emit `run.created`). Optionally
    /// idempotent under scope `create-run`.
    ///
    /// # Errors
    /// SQLite/foreign-key errors (e.g. unknown conversation); idempotency conflict.
    pub async fn create_run(
        &self,
        request: NewRun,
        idempotency: Option<Idempotency>,
    ) -> Result<Run, LedgerError> {
        let request_digest = idempotency::digest_bytes(&serde_json::to_vec(&request)?);
        self.with_tx(move |tx| {
            if let Some(idem) = &idempotency {
                if let IdemOutcome::Replayed(reference) =
                    idempotency::check(tx, SCOPE_CREATE_RUN, &idem.key, &request_digest)?
                {
                    return load_run(tx, &reference)?
                        .ok_or_else(|| LedgerError::NotFound(format!("run {reference}")));
                }
            }
            let now = now_millis();
            let run = Run {
                run_id: crate::RunId::generate(),
                conversation_id: request.conversation_id.clone(),
                parent_run_id: request.parent_run_id.clone(),
                agent: request.agent.clone(),
                role: request.role.clone(),
                status: RunStatus::Queued,
                created_at: now,
                finished_at: None,
            };
            tx.execute(
                "INSERT INTO run (run_id, conversation_id, parent_run_id, agent, role, status, created_at, finished_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL)",
                params![
                    run.run_id.as_str(),
                    run.conversation_id.as_str(),
                    run.parent_run_id.as_ref().map(crate::RunId::as_str),
                    run.agent,
                    run.role,
                    run.status.as_str(),
                    now,
                ],
            )?;
            emit_event(
                tx,
                &NewEvent {
                    event_id: EventId::generate(),
                    conversation_id: run.conversation_id.clone(),
                    run_id: Some(run.run_id.clone()),
                    attempt_id: None,
                    event_type: EventType::RunCreated,
                    schema_version: EVENT_SCHEMA_VERSION,
                    payload: serde_json::json!({ "agent": run.agent, "role": run.role }),
                },
                now,
            )?;
            if let Some(idem) = &idempotency {
                idempotency::record(
                    tx,
                    SCOPE_CREATE_RUN,
                    &idem.key,
                    &request_digest,
                    run.run_id.as_str(),
                    now,
                )?;
            }
            Ok(run)
        })
        .await
    }

    /// Transition a run `queued → running` and emit `run.started`.
    ///
    /// # Errors
    /// [`LedgerError::NotFound`] / [`LedgerError::ForbiddenTransition`] / SQLite.
    pub async fn start_run(&self, run_id: crate::RunId) -> Result<Run, LedgerError> {
        self.set_run_status(run_id, RunStatus::Running, EventType::RunStarted)
            .await
    }

    /// Transition `running → completed` and emit `run.completed`.
    /// # Errors
    /// See [`start_run`](Self::start_run).
    pub async fn complete_run(&self, run_id: crate::RunId) -> Result<Run, LedgerError> {
        self.set_run_status(run_id, RunStatus::Completed, EventType::RunCompleted)
            .await
    }

    /// Transition `running → failed` and emit `run.failed`.
    /// # Errors
    /// See [`start_run`](Self::start_run).
    pub async fn fail_run(&self, run_id: crate::RunId) -> Result<Run, LedgerError> {
        self.set_run_status(run_id, RunStatus::Failed, EventType::RunFailed)
            .await
    }

    /// Transition `running → cancelled` and emit `run.cancelled`.
    /// # Errors
    /// See [`start_run`](Self::start_run).
    pub async fn cancel_run(&self, run_id: crate::RunId) -> Result<Run, LedgerError> {
        self.set_run_status(run_id, RunStatus::Cancelled, EventType::RunCancelled)
            .await
    }

    /// Transition `running → interrupted` and emit `run.interrupted`.
    /// # Errors
    /// See [`start_run`](Self::start_run).
    pub async fn interrupt_run(&self, run_id: crate::RunId) -> Result<Run, LedgerError> {
        self.set_run_status(run_id, RunStatus::Interrupted, EventType::RunInterrupted)
            .await
    }

    async fn set_run_status(
        &self,
        run_id: crate::RunId,
        target: RunStatus,
        event_type: EventType,
    ) -> Result<Run, LedgerError> {
        self.with_tx(move |tx| {
            let current = load_run(tx, run_id.as_str())?
                .ok_or_else(|| LedgerError::NotFound(format!("run {run_id}")))?;
            validate_run_transition(current.status, target)?;
            let now = now_millis();
            let finished_at = if target.is_terminal() {
                Some(now)
            } else {
                None
            };
            tx.execute(
                "UPDATE run SET status = ?1, finished_at = ?2 WHERE run_id = ?3",
                params![target.as_str(), finished_at, run_id.as_str()],
            )?;
            emit_event(
                tx,
                &NewEvent {
                    event_id: EventId::generate(),
                    conversation_id: current.conversation_id.clone(),
                    run_id: Some(run_id.clone()),
                    attempt_id: None,
                    event_type,
                    schema_version: EVENT_SCHEMA_VERSION,
                    payload: serde_json::json!({}),
                },
                now,
            )?;
            load_run(tx, run_id.as_str())?
                .ok_or_else(|| LedgerError::NotFound(format!("run {run_id}")))
        })
        .await
    }

    /// Create the next attempt for a run (status `running`).
    ///
    /// # Errors
    /// [`LedgerError::NotFound`] if the run does not exist; SQLite errors.
    pub async fn create_attempt(&self, run_id: crate::RunId) -> Result<RunAttempt, LedgerError> {
        self.with_tx(move |tx| {
            if load_run(tx, run_id.as_str())?.is_none() {
                return Err(LedgerError::NotFound(format!("run {run_id}")));
            }
            let next_number: i64 = tx.query_row(
                "SELECT COALESCE(MAX(attempt_number), 0) + 1 FROM run_attempt WHERE run_id = ?1",
                params![run_id.as_str()],
                |row| row.get(0),
            )?;
            let now = now_millis();
            let attempt = RunAttempt {
                attempt_id: crate::AttemptId::generate(),
                run_id: run_id.clone(),
                attempt_number: u32::try_from(next_number)
                    .map_err(|_| LedgerError::Integrity("attempt_number overflow".to_owned()))?,
                status: AttemptStatus::Running,
                started_at: now,
                finished_at: None,
            };
            tx.execute(
                "INSERT INTO run_attempt (attempt_id, run_id, attempt_number, status, started_at, finished_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, NULL)",
                params![
                    attempt.attempt_id.as_str(),
                    attempt.run_id.as_str(),
                    next_number,
                    attempt.status.as_str(),
                    now,
                ],
            )?;
            Ok(attempt)
        })
        .await
    }

    /// Transition an attempt from `running` to a terminal/interrupted status.
    ///
    /// # Errors
    /// [`LedgerError::NotFound`] / [`LedgerError::ForbiddenTransition`] / SQLite.
    pub async fn set_attempt_status(
        &self,
        attempt_id: crate::AttemptId,
        target: AttemptStatus,
    ) -> Result<RunAttempt, LedgerError> {
        self.with_tx(move |tx| {
            let current = load_attempt(tx, attempt_id.as_str())?
                .ok_or_else(|| LedgerError::NotFound(format!("attempt {attempt_id}")))?;
            validate_attempt_transition(current.status, target)?;
            let now = now_millis();
            let finished_at = match target {
                AttemptStatus::Running => None,
                _ => Some(now),
            };
            tx.execute(
                "UPDATE run_attempt SET status = ?1, finished_at = ?2 WHERE attempt_id = ?3",
                params![target.as_str(), finished_at, attempt_id.as_str()],
            )?;
            load_attempt(tx, attempt_id.as_str())?
                .ok_or_else(|| LedgerError::NotFound(format!("attempt {attempt_id}")))
        })
        .await
    }

    /// Append a `user.message` event, optionally idempotent under scope
    /// `append-user-message`.
    ///
    /// # Errors
    /// SQLite/foreign-key errors; idempotency conflict.
    pub async fn append_user_message(
        &self,
        conversation_id: crate::ConversationId,
        payload: serde_json::Value,
        run_id: Option<crate::RunId>,
        idempotency: Option<Idempotency>,
    ) -> Result<PersistedEvent, LedgerError> {
        let digest_input = serde_json::json!({
            "conversation_id": conversation_id.as_str(),
            "run_id": run_id.as_ref().map(crate::RunId::as_str),
            "payload": payload,
        });
        let request_digest = idempotency::digest_bytes(&serde_json::to_vec(&digest_input)?);
        self.with_tx(move |tx| {
            if let Some(idem) = &idempotency {
                if let IdemOutcome::Replayed(reference) =
                    idempotency::check(tx, SCOPE_APPEND_USER_MESSAGE, &idem.key, &request_digest)?
                {
                    return load_event(tx, &reference)?
                        .ok_or_else(|| LedgerError::NotFound(format!("event {reference}")));
                }
            }
            let now = now_millis();
            let event = emit_event(
                tx,
                &NewEvent {
                    event_id: EventId::generate(),
                    conversation_id: conversation_id.clone(),
                    run_id: run_id.clone(),
                    attempt_id: None,
                    event_type: EventType::UserMessage,
                    schema_version: EVENT_SCHEMA_VERSION,
                    payload: payload.clone(),
                },
                now,
            )?;
            if let Some(idem) = &idempotency {
                idempotency::record(
                    tx,
                    SCOPE_APPEND_USER_MESSAGE,
                    &idem.key,
                    &request_digest,
                    event.event_id.as_str(),
                    now,
                )?;
            }
            Ok(event)
        })
        .await
    }

    /// Scan for runs/attempts still `running` — i.e. interrupted by whatever
    /// stopped the previous process. Read-only: nothing is mutated.
    ///
    /// # Errors
    /// Propagates SQLite errors.
    pub async fn recover_scan(&self) -> Result<RecoveryState, LedgerError> {
        self.with_conn(crate::recovery::scan).await
    }

    /// Explicitly mark the runs/attempts found by a recovery scan as
    /// `interrupted`, in one transaction. This is the caller's decision, never a
    /// side effect of opening the ledger.
    ///
    /// # Errors
    /// [`LedgerError::ForbiddenTransition`] if any is no longer `running`; SQLite.
    pub async fn mark_interrupted(&self, state: RecoveryState) -> Result<(), LedgerError> {
        self.with_tx(move |tx| {
            for run_id in &state.interrupted_runs {
                let current = load_run(tx, run_id.as_str())?
                    .ok_or_else(|| LedgerError::NotFound(format!("run {run_id}")))?;
                validate_run_transition(current.status, RunStatus::Interrupted)?;
                let now = now_millis();
                tx.execute(
                    "UPDATE run SET status = 'interrupted' WHERE run_id = ?1",
                    params![run_id.as_str()],
                )?;
                emit_event(
                    tx,
                    &NewEvent {
                        event_id: EventId::generate(),
                        conversation_id: current.conversation_id.clone(),
                        run_id: Some(run_id.clone()),
                        attempt_id: None,
                        event_type: EventType::RunInterrupted,
                        schema_version: EVENT_SCHEMA_VERSION,
                        payload: serde_json::json!({ "reason": "recovery" }),
                    },
                    now,
                )?;
            }
            for attempt_id in &state.interrupted_attempts {
                let current = load_attempt(tx, attempt_id.as_str())?
                    .ok_or_else(|| LedgerError::NotFound(format!("attempt {attempt_id}")))?;
                validate_attempt_transition(current.status, AttemptStatus::Interrupted)?;
                let now = now_millis();
                tx.execute(
                    "UPDATE run_attempt SET status = 'interrupted', finished_at = ?1 WHERE attempt_id = ?2",
                    params![now, attempt_id.as_str()],
                )?;
            }
            Ok(())
        })
        .await
    }

    /// Load a conversation by id (read-only), if present.
    /// # Errors
    /// Propagates SQLite errors.
    pub async fn conversation(
        &self,
        conversation_id: crate::ConversationId,
    ) -> Result<Option<Conversation>, LedgerError> {
        self.with_conn(move |conn| load_conversation(conn, conversation_id.as_str()))
            .await
    }

    /// Load a run by id (read-only), if present.
    /// # Errors
    /// Propagates SQLite errors.
    pub async fn run(&self, run_id: crate::RunId) -> Result<Option<Run>, LedgerError> {
        self.with_conn(move |conn| load_run(conn, run_id.as_str()))
            .await
    }
}

impl Ledger for SqliteLedger {
    async fn append_event(&self, event: NewEvent) -> Result<PersistedEvent, LedgerError> {
        self.with_tx(move |tx| {
            let now = now_millis();
            emit_event(tx, &event, now)
        })
        .await
    }

    async fn read_events(
        &self,
        conversation_id: &crate::ConversationId,
        after_sequence: Option<u64>,
        limit: usize,
    ) -> Result<Vec<PersistedEvent>, LedgerError> {
        let conversation = conversation_id.clone();
        let capped = limit.min(MAX_READ_LIMIT);
        self.with_conn(move |conn| {
            let after = i64::try_from(after_sequence.unwrap_or(0)).unwrap_or(i64::MAX);
            let cap = i64::try_from(capped).unwrap_or(0);
            let mut stmt = conn.prepare(
                "SELECT event_id, conversation_id, run_id, attempt_id, sequence, event_type, \
                 schema_version, created_at, payload_json \
                 FROM event WHERE conversation_id = ?1 AND sequence > ?2 \
                 ORDER BY sequence ASC LIMIT ?3",
            )?;
            let rows = stmt.query_map(params![conversation.as_str(), after, cap], map_event_row)?;
            let mut out = Vec::new();
            for row in rows {
                out.push(persisted_event_from_raw(row?)?);
            }
            Ok(out)
        })
        .await
    }
}

// ---- sync helpers (operate on a Connection or an open Transaction) ----

/// Allocate the next per-conversation sequence and insert the event, returning
/// the persisted row. Called inside an `IMMEDIATE` transaction only.
fn emit_event(
    tx: &Transaction<'_>,
    event: &NewEvent,
    created_at: i64,
) -> Result<PersistedEvent, LedgerError> {
    let max: i64 = tx.query_row(
        "SELECT COALESCE(MAX(sequence), 0) FROM event WHERE conversation_id = ?1",
        params![event.conversation_id.as_str()],
        |row| row.get(0),
    )?;
    let next = max
        .checked_add(1)
        .ok_or_else(|| LedgerError::Integrity("sequence overflow".to_owned()))?;
    let payload_json = serde_json::to_string(&event.payload)?;
    tx.execute(
        "INSERT INTO event (event_id, conversation_id, run_id, attempt_id, sequence, event_type, \
         schema_version, created_at, payload_json) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            event.event_id.as_str(),
            event.conversation_id.as_str(),
            event.run_id.as_ref().map(crate::RunId::as_str),
            event.attempt_id.as_ref().map(crate::AttemptId::as_str),
            next,
            event.event_type.as_str(),
            event.schema_version,
            created_at,
            payload_json,
        ],
    )?;
    let sequence =
        u64::try_from(next).map_err(|_| LedgerError::Integrity("negative sequence".to_owned()))?;
    Ok(PersistedEvent {
        event_id: event.event_id.clone(),
        conversation_id: event.conversation_id.clone(),
        run_id: event.run_id.clone(),
        attempt_id: event.attempt_id.clone(),
        sequence,
        event_type: event.event_type.as_str().to_owned(),
        schema_version: event.schema_version,
        created_at,
        payload: event.payload.clone(),
    })
}

type RawEvent = (
    String,
    String,
    Option<String>,
    Option<String>,
    i64,
    String,
    i64,
    i64,
    String,
);

fn map_event_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<RawEvent> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
        row.get(6)?,
        row.get(7)?,
        row.get(8)?,
    ))
}

fn persisted_event_from_raw(raw: RawEvent) -> Result<PersistedEvent, LedgerError> {
    let (
        event_id,
        conversation_id,
        run_id,
        attempt_id,
        sequence,
        event_type,
        schema_version,
        created_at,
        payload_json,
    ) = raw;
    Ok(PersistedEvent {
        event_id: EventId::from_raw(event_id),
        conversation_id: crate::ConversationId::from_raw(conversation_id),
        run_id: run_id.map(crate::RunId::from_raw),
        attempt_id: attempt_id.map(crate::AttemptId::from_raw),
        sequence: u64::try_from(sequence)
            .map_err(|_| LedgerError::Integrity("negative sequence".to_owned()))?,
        event_type,
        schema_version: u32::try_from(schema_version)
            .map_err(|_| LedgerError::Integrity("negative schema_version".to_owned()))?,
        created_at,
        payload: serde_json::from_str(&payload_json)?,
    })
}

fn load_conversation(
    conn: &Connection,
    conversation_id: &str,
) -> Result<Option<Conversation>, LedgerError> {
    let row = conn
        .query_row(
            "SELECT conversation_id, created_at, status FROM conversation WHERE conversation_id = ?1",
            params![conversation_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?, row.get::<_, String>(2)?)),
        )
        .optional()?;
    match row {
        None => Ok(None),
        Some((id, created_at, status)) => Ok(Some(Conversation {
            conversation_id: crate::ConversationId::from_raw(id),
            created_at,
            status: ConversationStatus::parse(&status).ok_or_else(|| {
                LedgerError::Integrity(format!("bad conversation status {status}"))
            })?,
        })),
    }
}

fn load_run(conn: &Connection, run_id: &str) -> Result<Option<Run>, LedgerError> {
    let row = conn
        .query_row(
            "SELECT run_id, conversation_id, parent_run_id, agent, role, status, created_at, finished_at \
             FROM run WHERE run_id = ?1",
            params![run_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, i64>(6)?,
                    row.get::<_, Option<i64>>(7)?,
                ))
            },
        )
        .optional()?;
    match row {
        None => Ok(None),
        Some((id, conv, parent, agent, role, status, created_at, finished_at)) => Ok(Some(Run {
            run_id: crate::RunId::from_raw(id),
            conversation_id: crate::ConversationId::from_raw(conv),
            parent_run_id: parent.map(crate::RunId::from_raw),
            agent,
            role,
            status: RunStatus::parse(&status)
                .ok_or_else(|| LedgerError::Integrity(format!("bad run status {status}")))?,
            created_at,
            finished_at,
        })),
    }
}

fn load_attempt(conn: &Connection, attempt_id: &str) -> Result<Option<RunAttempt>, LedgerError> {
    let row = conn
        .query_row(
            "SELECT attempt_id, run_id, attempt_number, status, started_at, finished_at \
             FROM run_attempt WHERE attempt_id = ?1",
            params![attempt_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, Option<i64>>(5)?,
                ))
            },
        )
        .optional()?;
    match row {
        None => Ok(None),
        Some((id, run_id, number, status, started_at, finished_at)) => Ok(Some(RunAttempt {
            attempt_id: crate::AttemptId::from_raw(id),
            run_id: crate::RunId::from_raw(run_id),
            attempt_number: u32::try_from(number)
                .map_err(|_| LedgerError::Integrity("negative attempt_number".to_owned()))?,
            status: AttemptStatus::parse(&status)
                .ok_or_else(|| LedgerError::Integrity(format!("bad attempt status {status}")))?,
            started_at,
            finished_at,
        })),
    }
}

fn load_event(conn: &Connection, event_id: &str) -> Result<Option<PersistedEvent>, LedgerError> {
    let row = conn
        .query_row(
            "SELECT event_id, conversation_id, run_id, attempt_id, sequence, event_type, \
             schema_version, created_at, payload_json FROM event WHERE event_id = ?1",
            params![event_id],
            map_event_row,
        )
        .optional()?;
    match row {
        None => Ok(None),
        Some(raw) => Ok(Some(persisted_event_from_raw(raw)?)),
    }
}
