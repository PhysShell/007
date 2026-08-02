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
    self, IdemOutcome, SCOPE_APPEND_SYSTEM_NOTE, SCOPE_APPEND_USER_MESSAGE, SCOPE_CREATE_COMMAND,
    SCOPE_CREATE_CONVERSATION, SCOPE_CREATE_RUN, SCOPE_CREATE_RUN_WITH_ID,
};
use crate::migrations;
use crate::models::{
    Command, CommandStatus, Conversation, ConversationStatus, EventType, Idempotency, ListCursor,
    NewCommand, NewEvent, NewRun, Page, PersistedEvent, RecoveryState, Run, RunAttempt, RunStatus,
};
use crate::transitions::{validate_attempt_transition, validate_run_transition};
use crate::{now_millis, AttemptStatus, EventId, Ledger, LedgerError};

/// Hard upper bound on how many events a single [`read_events`](Ledger::read_events)
/// call may return, so a caller can never request an unbounded scan.
pub const MAX_READ_LIMIT: usize = 1000;

/// Hard upper bound on rows per call to [`SqliteLedger::list_conversations`] or
/// [`SqliteLedger::list_runs`]. Smaller than [`MAX_READ_LIMIT`]: these rows are
/// for UI display (a cockpit page), not event-replay throughput — a caller
/// wanting more pages further back into history.
pub const MAX_LIST_LIMIT: usize = 200;

/// Schema version stamped on events emitted by the ledger's own lifecycle methods.
pub const EVENT_SCHEMA_VERSION: u32 = 1;

const BUSY_TIMEOUT_MS: u64 = 5000;

/// The effective durability pragmas of an open ledger.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PragmaReport {
    pub journal_mode: String,
    pub foreign_keys: bool,
    /// SQLite `synchronous` level: 0=OFF, 1=NORMAL, 2=FULL, 3=EXTRA.
    pub synchronous: i64,
}

/// A durable, append-only ledger backed by a single SQLite database.
#[derive(Clone)]
pub struct SqliteLedger {
    conn: Arc<Mutex<Connection>>,
}

impl std::fmt::Debug for SqliteLedger {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SqliteLedger").finish_non_exhaustive()
    }
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
        Self::init(conn, true)
    }

    /// Open an in-memory ledger (for tests/logic). WAL/durability pragmas that do
    /// not apply to `:memory:` are simply inert, so WAL/synchronous are not
    /// asserted here (foreign keys still are).
    ///
    /// # Errors
    /// Propagates SQLite/migration errors.
    pub fn open_in_memory() -> Result<Self, LedgerError> {
        let conn = Connection::open_in_memory()?;
        Self::init(conn, false)
    }

    fn init(conn: Connection, expect_wal: bool) -> Result<Self, LedgerError> {
        conn.busy_timeout(Duration::from_millis(BUSY_TIMEOUT_MS))?;
        // Refuse a too-new database BEFORE any PERSISTENT change. Reading
        // user_version does not touch the file; `journal_mode = WAL` below DOES
        // (and creates -wal/-shm). An old binary must not even switch a newer DB
        // to WAL, so this guard runs first. (busy_timeout / foreign_keys /
        // synchronous are per-connection and do not modify the file.)
        let existing = migrations::current_version(&conn)?;
        if existing > migrations::CURRENT_SCHEMA_VERSION {
            return Err(LedgerError::SchemaTooNew {
                found: existing,
                supported: migrations::CURRENT_SCHEMA_VERSION,
            });
        }
        conn.execute_batch("PRAGMA foreign_keys = ON; PRAGMA synchronous = FULL;")?;
        // journal_mode returns the RESULTING mode ("wal", or "memory" for an
        // in-memory db). Verify the pragmas actually took effect — otherwise the
        // durability documentation would be a lie on a filesystem without WAL.
        let journal: String = conn.query_row("PRAGMA journal_mode = WAL;", [], |row| row.get(0))?;
        verify_effective_pragmas(&conn, expect_wal, &journal)?;

        let mut conn = conn;
        // Then migrations, a cheap integrity check, and a structural schema check
        // (catches a DB that merely claims the current user_version but differs).
        migrations::apply(&mut conn)?;
        let check: String = conn.query_row("PRAGMA quick_check;", [], |row| row.get(0))?;
        if check != "ok" {
            return Err(LedgerError::Integrity(check));
        }
        migrations::validate_schema(&conn)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Report the effective durability pragmas (for tests/diagnostics).
    ///
    /// # Errors
    /// Propagates SQLite errors.
    pub async fn pragma_report(&self) -> Result<PragmaReport, LedgerError> {
        self.with_conn(|conn| {
            let journal_mode: String = conn.query_row("PRAGMA journal_mode;", [], |r| r.get(0))?;
            let foreign_keys: i64 = conn.query_row("PRAGMA foreign_keys;", [], |r| r.get(0))?;
            let synchronous: i64 = conn.query_row("PRAGMA synchronous;", [], |r| r.get(0))?;
            Ok(PragmaReport {
                journal_mode,
                foreign_keys: foreign_keys == 1,
                synchronous,
            })
        })
        .await
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
    /// idempotent under scope `create-run`. The ledger mints its own `RunId`
    /// — for a run that must share an id with an external canonical stream,
    /// see [`Self::create_run_with_id`].
    ///
    /// # Errors
    /// SQLite/foreign-key errors (e.g. unknown conversation); idempotency conflict.
    pub async fn create_run(
        &self,
        request: NewRun,
        idempotency: Option<Idempotency>,
    ) -> Result<Run, LedgerError> {
        // Unchanged from before create_run_with_id existed: the digest is
        // computed over `request` alone, exactly as any already-persisted
        // idempotency record for this scope was — a ledger file upgraded
        // in place must keep replaying old create-run retries correctly.
        let request_digest = idempotency::digest_bytes(&serde_json::to_vec(&request)?);
        self.create_run_inner(request, None, SCOPE_CREATE_RUN, idempotency, request_digest)
            .await
    }

    /// Like [`Self::create_run`], but the run uses exactly `run_id` instead
    /// of a ledger-generated one — for live-ingress projection (Q-Deck R0.7,
    /// `docs/q-deck/r07-live-ingress.md`), where one physical run must carry
    /// the same `RunId` in `o7-run`'s canonical stream and in the ledger.
    ///
    /// `idempotency` is mandatory here (unlike `create_run`'s optional one):
    /// the natural, and required, idempotency key for this call is `run_id`
    /// itself — a caller that retries after a crash between this insert and
    /// its own bookkeeping must replay under the exact same key so a second
    /// call for the same `run_id` returns the existing row (via the ordinary
    /// idempotent-replay path) rather than hitting a raw `UNIQUE` constraint
    /// error on the primary key.
    ///
    /// # Errors
    /// SQLite/foreign-key errors (e.g. unknown conversation); idempotency
    /// conflict (the same `idempotency.key` reused with a different request,
    /// including a different `run_id` — this is what protects against a
    /// caller ever using this call to secretly reassign an existing ledger
    /// run to a different `RunId`).
    pub async fn create_run_with_id(
        &self,
        request: NewRun,
        run_id: crate::RunId,
        idempotency: Idempotency,
    ) -> Result<Run, LedgerError> {
        // A new scope with no pre-existing persisted records, so this digest
        // scheme (including run_id, unlike create_run's) is free to differ
        // from create_run's without any upgrade-compatibility concern.
        let digest_input = serde_json::json!({
            "conversation_id": request.conversation_id.as_str(),
            "parent_run_id": request.parent_run_id.as_ref().map(crate::RunId::as_str),
            "agent": request.agent,
            "role": request.role,
            "run_id": run_id.as_str(),
        });
        let request_digest = idempotency::digest_bytes(&serde_json::to_vec(&digest_input)?);
        self.create_run_inner(
            request,
            Some(run_id),
            SCOPE_CREATE_RUN_WITH_ID,
            Some(idempotency),
            request_digest,
        )
        .await
    }

    async fn create_run_inner(
        &self,
        request: NewRun,
        run_id: Option<crate::RunId>,
        scope: &'static str,
        idempotency: Option<Idempotency>,
        request_digest: String,
    ) -> Result<Run, LedgerError> {
        self.with_tx(move |tx| {
            if let Some(idem) = &idempotency {
                if let IdemOutcome::Replayed(reference) =
                    idempotency::check(tx, scope, &idem.key, &request_digest)?
                {
                    return load_run(tx, &reference)?
                        .ok_or_else(|| LedgerError::NotFound(format!("run {reference}")));
                }
            }
            let now = now_millis();
            let run = Run {
                run_id: run_id.clone().unwrap_or_else(crate::RunId::generate),
                conversation_id: request.conversation_id.clone(),
                parent_run_id: request.parent_run_id.clone(),
                agent: request.agent.clone(),
                role: request.role.clone(),
                status: RunStatus::Queued,
                created_at: now,
                finished_at: None,
                provider_session_id: None,
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
                    scope,
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

    /// Transition `running → blocked` and emit `run.blocked`. Sealed: the
    /// ledger projection of `o7-run::Verdict::Blocked`.
    /// # Errors
    /// See [`start_run`](Self::start_run).
    pub async fn block_run(&self, run_id: crate::RunId) -> Result<Run, LedgerError> {
        self.set_run_status(run_id, RunStatus::Blocked, EventType::RunBlocked)
            .await
    }

    /// Transition `running → error` and emit `run.errored`. Sealed: the
    /// ledger projection of `o7-run::Verdict::Error` — NOT the same concept
    /// as `interrupted` (see `docs/q-deck/r06-verdict-fidelity.md`).
    /// # Errors
    /// See [`start_run`](Self::start_run).
    pub async fn error_run(&self, run_id: crate::RunId) -> Result<Run, LedgerError> {
        self.set_run_status(run_id, RunStatus::Error, EventType::RunErrored)
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
            // A run leaving `running` finishes its (at most one) running attempt,
            // so a terminal/interrupted run never leaves a dangling running attempt.
            if target.is_terminal() || target == RunStatus::Interrupted {
                let attempt_status = terminal_attempt_status(target);
                tx.execute(
                    "UPDATE run_attempt SET status = ?1, finished_at = ?2 \
                     WHERE run_id = ?3 AND status = 'running'",
                    params![attempt_status.as_str(), now, run_id.as_str()],
                )?;
            }
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
            let run = load_run(tx, run_id.as_str())?
                .ok_or_else(|| LedgerError::NotFound(format!("run {run_id}")))?;
            // An attempt only exists while its run is running.
            if run.status != RunStatus::Running {
                return Err(LedgerError::InvalidState(format!(
                    "cannot create an attempt: run {run_id} is {}, not running",
                    run.status.as_str()
                )));
            }
            // At most one running attempt per run (also enforced by a partial
            // unique index; this gives a clean typed error before hitting it).
            let running: i64 = tx.query_row(
                "SELECT count(*) FROM run_attempt WHERE run_id = ?1 AND status = 'running'",
                params![run_id.as_str()],
                |row| row.get(0),
            )?;
            if running > 0 {
                return Err(LedgerError::InvalidState(format!(
                    "run {run_id} already has a running attempt"
                )));
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

    /// Atomically resume an interrupted run: transition it `interrupted →
    /// running` AND create a new running attempt, in one transaction. Any
    /// lingering running attempt of the run is first marked `interrupted`.
    /// Returns the new attempt.
    ///
    /// # Errors
    /// [`LedgerError::NotFound`] if the run is missing; [`LedgerError::InvalidState`]
    /// if the run is not `interrupted`; SQLite errors.
    pub async fn resume_interrupted_run(
        &self,
        run_id: crate::RunId,
    ) -> Result<RunAttempt, LedgerError> {
        self.with_tx(move |tx| {
            let current = load_run(tx, run_id.as_str())?
                .ok_or_else(|| LedgerError::NotFound(format!("run {run_id}")))?;
            if current.status != RunStatus::Interrupted {
                return Err(LedgerError::InvalidState(format!(
                    "cannot resume: run {run_id} is {}, not interrupted",
                    current.status.as_str()
                )));
            }
            let now = now_millis();
            // Defensively close any still-running attempt so the new one does not
            // collide with the one-running-attempt invariant.
            tx.execute(
                "UPDATE run_attempt SET status = 'interrupted', finished_at = ?1 \
                 WHERE run_id = ?2 AND status = 'running'",
                params![now, run_id.as_str()],
            )?;
            // interrupted -> running. This is the ONLY code path allowed to make
            // this transition (the general run-transition table forbids it, so
            // start_run cannot revive an interrupted run); the precondition is the
            // explicit `status == Interrupted` check above.
            tx.execute(
                "UPDATE run SET status = 'running', finished_at = NULL WHERE run_id = ?1",
                params![run_id.as_str()],
            )?;
            emit_event(
                tx,
                &NewEvent {
                    event_id: EventId::generate(),
                    conversation_id: current.conversation_id.clone(),
                    run_id: Some(run_id.clone()),
                    attempt_id: None,
                    event_type: EventType::RunStarted,
                    schema_version: EVENT_SCHEMA_VERSION,
                    payload: serde_json::json!({ "resumed": true }),
                },
                now,
            )?;
            let next_number: i64 = tx.query_row(
                "SELECT COALESCE(MAX(attempt_number), 0) + 1 FROM run_attempt WHERE run_id = ?1",
                params![run_id.as_str()],
                |row| row.get(0),
            )?;
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

    /// Repair a run stuck `interrupted` to its TRUE canonical terminal status
    /// (`completed`/`failed`/`blocked`/`error`) — Q-Deck R0.7's third
    /// corrective round. `interrupted` is otherwise an immovable settled
    /// dead-end for every ordinary API (`complete_run`/`fail_run`/etc, and
    /// the general transition table above), by design — a real crash mid-run
    /// must never be silently reclassified as a clean terminal outcome.
    ///
    /// But a plain `o7 recover` (the still-running → `interrupted` scan) can
    /// legitimately race a `RunSealed` that already landed durably in
    /// `events.jsonl` but never made it into the ledger before the crash —
    /// the run IS genuinely finished, just misclassified. This method is the
    /// ONE narrow, explicit exception, mirroring
    /// [`Self::resume_interrupted_run`]'s precedent of a precondition-gated
    /// bypass of the general table rather than a loosening of it: the
    /// precondition here is `current.status == Interrupted` (checked, not
    /// assumed), and the caller (`o7 recover --run-dir`'s catch-up, via
    /// `LiveLedgerProjector::seal_or_repair_interrupted` — never the live
    /// path's own ordinary `seal()`) is trusted to have independently
    /// verified a genuinely sealed canonical stream first.
    ///
    /// Atomically repairs the run's own most-recent `interrupted` attempt to
    /// the matching terminal attempt status too — a fourth re-gate found the
    /// first version of this method left the run and its attempt internally
    /// inconsistent (`run = completed`, attempt still `interrupted`).
    ///
    /// # Errors
    /// [`LedgerError::NotFound`] if the run is missing; [`LedgerError::InvalidState`]
    /// if the run is not currently `interrupted`, or if `target` is not one of the four
    /// sealed terminal statuses; SQLite errors.
    pub async fn repair_interrupted_run_to_terminal(
        &self,
        run_id: crate::RunId,
        target: RunStatus,
    ) -> Result<Run, LedgerError> {
        let event_type = match target {
            RunStatus::Completed => EventType::RunCompleted,
            RunStatus::Failed => EventType::RunFailed,
            RunStatus::Blocked => EventType::RunBlocked,
            RunStatus::Error => EventType::RunErrored,
            _ => {
                return Err(LedgerError::InvalidState(format!(
                    "repair target must be a sealed terminal status (completed/failed/\
                     blocked/error), got {}",
                    target.as_str()
                )));
            }
        };
        self.with_tx(move |tx| {
            let current = load_run(tx, run_id.as_str())?
                .ok_or_else(|| LedgerError::NotFound(format!("run {run_id}")))?;
            if current.status != RunStatus::Interrupted {
                return Err(LedgerError::InvalidState(format!(
                    "cannot repair: run {run_id} is {}, not interrupted",
                    current.status.as_str()
                )));
            }
            let now = now_millis();
            tx.execute(
                "UPDATE run SET status = ?1, finished_at = ?2 WHERE run_id = ?3",
                params![target.as_str(), now, run_id.as_str()],
            )?;
            // An ordinary terminal transition (`set_run_status`) atomically
            // closes the run's own attempt to a MATCHING terminal status —
            // repair must too, or it leaves an internally inconsistent
            // ledger (`run = completed`, its own attempt still
            // `interrupted`). Plain recovery's `mark_interrupted` closed
            // exactly one attempt alongside this run (the one that was
            // `running` at the time), so the most recent `interrupted`
            // attempt — not any earlier, already-settled one from a prior
            // attempt cycle — is the one this repair concerns.
            tx.execute(
                "UPDATE run_attempt SET status = ?1, finished_at = ?2 \
                 WHERE attempt_id = (
                     SELECT attempt_id FROM run_attempt \
                     WHERE run_id = ?3 AND status = 'interrupted' \
                     ORDER BY attempt_number DESC LIMIT 1
                 )",
                params![
                    terminal_attempt_status(target).as_str(),
                    now,
                    run_id.as_str()
                ],
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
                    payload: serde_json::json!({ "repaired_from": "interrupted" }),
                },
                now,
            )?;
            load_run(tx, run_id.as_str())?
                .ok_or_else(|| LedgerError::NotFound(format!("run {run_id}")))
        })
        .await
    }

    /// Durably record ONE user-accepted command (Q-Deck R1,
    /// `docs/q-deck/r1-command.md` §3/§4/§5/§6) — validated, idempotent,
    /// concurrency- and staleness-checked, all inside ONE `IMMEDIATE`
    /// transaction. No provider process is ever started as a side effect
    /// of this call — it only durably records ACCEPTANCE; the caller
    /// starts the provider afterward (§3's ordering).
    ///
    /// Validates, in order: the conversation exists; `parent_run_id`
    /// belongs to it; `parent_run_id` is the conversation's current tail
    /// (no run already has it as `parent_run_id`); no other
    /// `accepted`/`started` command exists for this conversation; the
    /// parent run has a durable, valid `provider_session_id`.
    ///
    /// # Errors
    /// [`LedgerError::NotFound`] for an unknown conversation or parent run;
    /// [`LedgerError::StaleParent`] if `parent_run_id` is not the tail;
    /// [`LedgerError::CommandConflict`] if another command is already
    /// in flight for this conversation; [`LedgerError::IdempotencyConflict`]
    /// if `idempotency.key` was already used for a DIFFERENT request;
    /// [`LedgerError::ContinuationNotPermitted`] if the parent has no valid
    /// provider session; SQLite errors.
    /// Q-Deck A0 corrective round 2 (Part 5.3): a pure, read-only peek —
    /// `true` iff `key` has ANY prior idempotency record in
    /// [`crate::idempotency::SCOPE_CREATE_COMMAND`], regardless of whether
    /// its stored digest would match a NEW request (a digest mismatch is
    /// itself [`LedgerError::IdempotencyConflict`], which — like a genuine
    /// replay — must skip every fresh-acceptance business check, including
    /// `o7d`'s own new parent-candidate-usability preflight, exactly as it
    /// already skips `create_command`'s OWN internal checks). Lets a caller
    /// decide, BEFORE calling [`Self::create_command`], whether this
    /// request is a genuinely fresh acceptance (run the preflight) or not
    /// (the ledger's own existing idempotency handling is authoritative;
    /// re-validating business rules against a record whose acceptance-time
    /// answer may have been produced under this key already would risk
    /// disagreeing with a decision already durably made).
    ///
    /// # Errors
    /// Propagates SQLite errors.
    pub async fn command_idempotency_key_seen(&self, key: String) -> Result<bool, LedgerError> {
        self.with_conn(move |conn| {
            let seen: Option<i64> = conn
                .query_row(
                    "SELECT 1 FROM idempotency_record WHERE scope = ?1 AND key = ?2",
                    params![crate::idempotency::SCOPE_CREATE_COMMAND, key],
                    |row| row.get(0),
                )
                .optional()?;
            Ok(seen.is_some())
        })
        .await
    }

    pub async fn create_command(
        &self,
        request: NewCommand,
        idempotency: Idempotency,
    ) -> Result<Command, LedgerError> {
        let digest_input = serde_json::json!({
            "conversation_id": request.conversation_id.as_str(),
            "parent_run_id": request.parent_run_id.as_str(),
            "command_text": request.command_text,
        });
        let request_digest = idempotency::digest_bytes(&serde_json::to_vec(&digest_input)?);
        self.with_tx(move |tx| {
            if let IdemOutcome::Replayed(reference) =
                idempotency::check(tx, SCOPE_CREATE_COMMAND, &idempotency.key, &request_digest)?
            {
                return load_command(tx, &reference)?
                    .ok_or_else(|| LedgerError::NotFound(format!("command {reference}")));
            }

            load_conversation(tx, request.conversation_id.as_str())?.ok_or_else(|| {
                LedgerError::NotFound(format!("conversation {}", request.conversation_id))
            })?;
            let parent = load_run(tx, request.parent_run_id.as_str())?
                .ok_or_else(|| LedgerError::NotFound(format!("run {}", request.parent_run_id)))?;
            if parent.conversation_id != request.conversation_id {
                return Err(LedgerError::NotFound(format!(
                    "run {} does not belong to conversation {}",
                    request.parent_run_id, request.conversation_id
                )));
            }
            // A command never continues a run that hasn't finished its own
            // work yet — "provider session exists" alone proves nothing
            // about whether gates/RunSealed ever happened (the session is
            // persisted right after the agent call, well before either).
            // Continuing a still-`running`/`queued`/`interrupted` parent
            // would let a NEW child run's own writes race the original
            // run's own still-in-flight worktree/canonical-record work.
            if !parent.status.is_terminal() {
                return Err(LedgerError::ContinuationNotPermitted(format!(
                    "run {} is not sealed yet (status {:?}) — a command can only continue a \
                     run that has finished",
                    request.parent_run_id, parent.status
                )));
            }
            // "No other run already claims you as ITS parent" only proves
            // `parent_run_id` is a LEAF — it does not prove it is the
            // conversation's actual current tail. A conversation can hold
            // more than one independent leaf at once (e.g. two plain `o7
            // run --conversation-id` invocations, neither ever threaded as
            // the other's child) — every leaf but the most recently
            // created run in the conversation is still stale. `rowid` is
            // SQLite's own monotonic insertion order, the same tiebreak
            // `list_runs` already uses for its own newest-first ordering.
            let latest_run_id: String = tx.query_row(
                "SELECT run_id FROM run WHERE conversation_id = ?1 \
                 ORDER BY created_at DESC, rowid DESC LIMIT 1",
                params![request.conversation_id.as_str()],
                |row| row.get(0),
            )?;
            if latest_run_id != request.parent_run_id.as_str() {
                return Err(LedgerError::StaleParent(format!(
                    "run {} is not conversation {}'s current tail (that is run {latest_run_id}) \
                     — it is no longer valid to continue from",
                    request.parent_run_id, request.conversation_id
                )));
            }
            let in_flight: i64 = tx.query_row(
                "SELECT COUNT(*) FROM command WHERE conversation_id = ?1 \
                 AND status IN ('accepted','started')",
                params![request.conversation_id.as_str()],
                |row| row.get(0),
            )?;
            if in_flight > 0 {
                return Err(LedgerError::CommandConflict(format!(
                    "conversation {} already has a command in flight",
                    request.conversation_id
                )));
            }
            match &parent.provider_session_id {
                None => {
                    return Err(LedgerError::ContinuationNotPermitted(format!(
                        "run {} has no durable provider session to continue from",
                        request.parent_run_id
                    )));
                }
                // Independent re-gate on PR #90, blocker 3: the ORIGINAL
                // validation (`agent::ProviderSessionId::new` — non-empty,
                // no control characters) only ran inside the detached `o7
                // continue` process, AFTER durable acceptance and the
                // `202` response. A structurally invalid session is not a
                // transient failure — it never becomes valid — so every
                // subsequent retry (once past the redrive checks below)
                // would just keep re-spawning a continuation guaranteed to
                // fail the same way forever. `o7-ledger` cannot depend on
                // the root crate's `agent` module (wrong dependency
                // direction), so the SAME two rules are checked here,
                // directly, before the command is ever durably accepted —
                // never after.
                Some(session) if session.is_empty() || session.chars().any(char::is_control) => {
                    return Err(LedgerError::ContinuationNotPermitted(format!(
                        "run {}'s durable provider session fails validation (empty or contains \
                         a control character) — refusing to continue from a corrupt session",
                        request.parent_run_id
                    )));
                }
                Some(_) => {}
            }

            let now = now_millis();
            let command = Command {
                command_id: crate::CommandId::generate(),
                conversation_id: request.conversation_id.clone(),
                parent_run_id: request.parent_run_id.clone(),
                command_text: request.command_text.clone(),
                status: CommandStatus::Accepted,
                child_run_id: None,
                created_at: now,
                updated_at: now,
            };
            tx.execute(
                "INSERT INTO command (command_id, conversation_id, parent_run_id, command_text, \
                 status, child_run_id, created_at, updated_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, NULL, ?6, ?6)",
                params![
                    command.command_id.as_str(),
                    command.conversation_id.as_str(),
                    command.parent_run_id.as_str(),
                    command.command_text,
                    command.status.as_str(),
                    now,
                ],
            )?;
            idempotency::record(
                tx,
                SCOPE_CREATE_COMMAND,
                &idempotency.key,
                &request_digest,
                command.command_id.as_str(),
                now,
            )?;
            Ok(command)
        })
        .await
    }

    /// Bind a durably accepted command to its allocated child `RunId` —
    /// call once, right after minting the child run's identity, before
    /// starting the provider continuation (§3's ordering: this binding
    /// must be durable before the provider is invoked).
    ///
    /// Compare-and-swap, not a blind write: the `UPDATE` only takes effect
    /// `WHERE child_run_id IS NULL`. Two callers racing to bind the SAME
    /// command (e.g. `o7d` handling a retried idempotent request before the
    /// first attempt's response landed) both call this; exactly one's write
    /// matches inside SQLite's own `IMMEDIATE`-transaction writer
    /// serialization, and BOTH calls return the SAME already-bound
    /// [`Command`] afterward. The caller must compare the returned
    /// `child_run_id` against the `run_id` it proposed — a mismatch means
    /// it lost the race and must NOT spawn a second provider continuation.
    ///
    /// # Errors
    /// [`LedgerError::NotFound`] if the command is missing; SQLite errors.
    pub async fn bind_command_child_run(
        &self,
        command_id: crate::CommandId,
        run_id: crate::RunId,
    ) -> Result<Command, LedgerError> {
        self.with_tx(move |tx| {
            load_command(tx, command_id.as_str())?
                .ok_or_else(|| LedgerError::NotFound(format!("command {command_id}")))?;
            let now = now_millis();
            tx.execute(
                "UPDATE command SET child_run_id = ?1, status = 'started', updated_at = ?2 \
                 WHERE command_id = ?3 AND child_run_id IS NULL",
                params![run_id.as_str(), now, command_id.as_str()],
            )?;
            load_command(tx, command_id.as_str())?
                .ok_or_else(|| LedgerError::NotFound(format!("command {command_id}")))
        })
        .await
    }

    /// Mark a command `completed` — its child run reached a sealed
    /// terminal status. This is bookkeeping only; the child run's OWN
    /// ledger row remains the sole verdict authority (§2).
    ///
    /// # Errors
    /// [`LedgerError::NotFound`] if the command is missing; SQLite errors.
    pub async fn mark_command_completed(
        &self,
        command_id: crate::CommandId,
    ) -> Result<Command, LedgerError> {
        self.set_command_status(command_id, CommandStatus::Completed)
            .await
    }

    /// Mark a command `rejected` — durably accepted but never dispatched
    /// to a provider (e.g. the child run could not be started at all).
    ///
    /// # Errors
    /// [`LedgerError::NotFound`] if the command is missing; SQLite errors.
    pub async fn mark_command_rejected(
        &self,
        command_id: crate::CommandId,
    ) -> Result<Command, LedgerError> {
        self.set_command_status(command_id, CommandStatus::Rejected)
            .await
    }

    async fn set_command_status(
        &self,
        command_id: crate::CommandId,
        status: CommandStatus,
    ) -> Result<Command, LedgerError> {
        self.with_tx(move |tx| {
            load_command(tx, command_id.as_str())?
                .ok_or_else(|| LedgerError::NotFound(format!("command {command_id}")))?;
            let now = now_millis();
            tx.execute(
                "UPDATE command SET status = ?1, updated_at = ?2 WHERE command_id = ?3",
                params![status.as_str(), now, command_id.as_str()],
            )?;
            load_command(tx, command_id.as_str())?
                .ok_or_else(|| LedgerError::NotFound(format!("command {command_id}")))
        })
        .await
    }

    /// The conversation's currently `accepted`/`started` command, if any —
    /// the read side of §5's concurrency check, and also what a recovery
    /// scan uses to find a stuck command (§7).
    ///
    /// # Errors
    /// Propagates SQLite errors.
    pub async fn active_command_for_conversation(
        &self,
        conversation_id: crate::ConversationId,
    ) -> Result<Option<Command>, LedgerError> {
        self.with_conn(move |conn| {
            let id: Option<String> = conn
                .query_row(
                    "SELECT command_id FROM command WHERE conversation_id = ?1 \
                     AND status IN ('accepted','started') \
                     ORDER BY created_at DESC LIMIT 1",
                    params![conversation_id.as_str()],
                    |row| row.get(0),
                )
                .optional()?;
            match id {
                None => Ok(None),
                Some(id) => load_command(conn, &id),
            }
        })
        .await
    }

    /// Every command still `accepted`/`started`, across every conversation
    /// (`docs/q-deck/r1-command.md` §7) — `o7 recover`'s read side for
    /// making a stuck command DISCOVERABLE. Three shapes, all covered:
    /// `child_run_id` unset (never bound); bound to a run with no ledger
    /// row at all (the process crashed between `bind_command_child_run`
    /// and the child's own durable `attach_run`); or bound to a run that
    /// IS a real ledger row but is `interrupted` — the pre-existing
    /// still-`running`/`queued` scan (`recover_scan`/`mark_interrupted`)
    /// already transitioned it there, but that scan has no concept of
    /// `command` rows, so nothing else ever surfaces this one. A command
    /// whose child run is `queued`/`running` (genuinely still active, or
    /// merely not yet scanned) is deliberately NOT returned here — this
    /// method only surfaces cases already CONFIRMED dead by an existing
    /// mechanism, never a guess.
    ///
    /// # Errors
    /// Propagates SQLite errors.
    pub async fn stuck_commands(&self) -> Result<Vec<Command>, LedgerError> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT c.command_id FROM command c \
                 LEFT JOIN run r ON r.run_id = c.child_run_id \
                 WHERE c.status IN ('accepted','started') \
                 AND (c.child_run_id IS NULL OR r.run_id IS NULL OR r.status = 'interrupted') \
                 ORDER BY c.created_at ASC",
            )?;
            let ids = stmt
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            ids.iter()
                .map(|id| {
                    load_command(conn, id)?
                        .ok_or_else(|| LedgerError::Integrity(format!("command {id} vanished")))
                })
                .collect()
        })
        .await
    }

    /// Q-Deck R1 corrective round: a command's `bind_command_child_run`
    /// happens BEFORE the child's own `RunStarted` — if the process that
    /// was going to spawn the provider continuation dies right after
    /// binding (or the spawn itself fails), the command is durably `started`
    /// with a `child_run_id` that will never get a ledger row, and (per
    /// `create_command`'s stale-parent check) permanently blocks the
    /// conversation. A caller (`o7d`, on a retried idempotent request) that
    /// wants to re-spawn for such a command must FIRST win this claim —
    /// compare-and-swap on `updated_at`, exactly like `bind_command_child_run`
    /// compare-and-swaps on `child_run_id`. Two callers racing to redrive
    /// the SAME command: only the one whose `expected_updated_at` still
    /// matches the stored value wins (`true`); the other sees `false` and
    /// must NOT spawn — otherwise two processes could both attach to the
    /// same not-yet-existing child run id and corrupt its `events.jsonl`
    /// with interleaved writes.
    ///
    /// # Errors
    /// Propagates SQLite errors.
    pub async fn claim_stale_command_for_redrive(
        &self,
        command_id: crate::CommandId,
        expected_updated_at: i64,
    ) -> Result<bool, LedgerError> {
        self.with_tx(move |tx| {
            // `now_millis()`'s resolution is only 1ms — two claims against
            // the SAME `expected_updated_at`, both landing in the same
            // millisecond (entirely possible for two fast, back-to-back
            // in-process calls, and exactly what an earlier version of
            // this method got wrong), must never both write the identical
            // "new" value and let a second claim's `WHERE updated_at = ?`
            // spuriously still match. `.max(expected_updated_at + 1)`
            // guarantees the written value always differs from what a
            // second claimant is comparing against, regardless of clock
            // resolution — falling back to a plain +1 bump only in that
            // same-millisecond edge case, never in the ordinary one.
            let now = now_millis().max(expected_updated_at.saturating_add(1));
            let changed = tx.execute(
                "UPDATE command SET updated_at = ?1 \
                 WHERE command_id = ?2 AND updated_at = ?3 AND status IN ('accepted','started')",
                params![now, command_id.as_str(), expected_updated_at],
            )?;
            Ok(changed == 1)
        })
        .await
    }

    /// Q-Deck R1 second corrective round: rebind an already-`started`
    /// command to a FRESH child `RunId`, after its previous child run was
    /// confirmed `interrupted` (a genuinely dead process — never guessed;
    /// `recover_scan`/`mark_interrupted` already made that determination).
    /// The interrupted run itself is never resumed here — it stays exactly
    /// as it is, a dead-end historical record; a brand new child run
    /// replaces it as the command's live attempt. Callers MUST already
    /// hold exclusive ownership of this redrive (this is a plain,
    /// unconditional write — the caller's own `claim_stale_command_for_redrive`
    /// call, or an equivalent liveness proof, is the actual safety
    /// mechanism, not this method).
    ///
    /// # Errors
    /// [`LedgerError::NotFound`] if the command is missing; SQLite errors.
    pub async fn rebind_command_child_run(
        &self,
        command_id: crate::CommandId,
        new_run_id: crate::RunId,
    ) -> Result<Command, LedgerError> {
        self.with_tx(move |tx| {
            load_command(tx, command_id.as_str())?
                .ok_or_else(|| LedgerError::NotFound(format!("command {command_id}")))?;
            let now = now_millis();
            tx.execute(
                "UPDATE command SET child_run_id = ?1, updated_at = ?2 WHERE command_id = ?3",
                params![new_run_id.as_str(), now, command_id.as_str()],
            )?;
            load_command(tx, command_id.as_str())?
                .ok_or_else(|| LedgerError::NotFound(format!("command {command_id}")))
        })
        .await
    }

    /// Q-Deck R1 fourth corrective round: rebind a command's `child_run_id`
    /// to a FRESH run id as ONE atomic conditional `UPDATE` — replacing
    /// [`Self::claim_stale_command_for_redrive`] followed by
    /// [`Self::rebind_command_child_run`] as the load-bearing redrive path.
    /// That two-step sequence left a window between the claim and the
    /// rebind where the claim's own success said nothing about whether the
    /// command was STILL bound to the exact old run id the caller had just
    /// classified — collapsing both checks into the SAME `UPDATE`'s `WHERE`
    /// clause removes that window entirely: the write can only land if
    /// every precondition the caller observed is still true at the instant
    /// of the write, not merely at the instant it was read.
    ///
    /// Predicates (all four, in the same `WHERE`): `command_id` matches;
    /// `status` is `accepted` or `started` (never rebind a
    /// `completed`/`rejected` command); `child_run_id` equals
    /// `expected_old_run_id`; `updated_at` equals `expected_updated_at`.
    /// Two callers racing to redrive the SAME command: at most one's
    /// `UPDATE` can match (SQLite's own `IMMEDIATE`-transaction writer
    /// serialization), so at most one [`RebindOutcome::Rebound`] is ever
    /// produced — the loser reads back the CURRENT row and must use ITS
    /// `child_run_id`, never the stale one it originally proposed rebinding
    /// away from (see [`RebindOutcome`]'s own doc comment).
    ///
    /// # Errors
    /// Propagates SQLite errors.
    pub async fn rebind_command_child_run_if_current(
        &self,
        command_id: crate::CommandId,
        expected_old_run_id: crate::RunId,
        expected_updated_at: i64,
        fresh_run_id: crate::RunId,
    ) -> Result<crate::models::RebindOutcome, LedgerError> {
        use crate::models::RebindOutcome;
        self.with_tx(move |tx| {
            let Some(current) = load_command(tx, command_id.as_str())? else {
                return Ok(RebindOutcome::NotFound);
            };
            if !matches!(
                current.status,
                CommandStatus::Accepted | CommandStatus::Started
            ) {
                return Ok(RebindOutcome::NotEligible(current));
            }
            if current.child_run_id.as_ref().map(crate::RunId::as_str)
                != Some(expected_old_run_id.as_str())
                || current.updated_at != expected_updated_at
            {
                return Ok(RebindOutcome::LostRace(current));
            }
            // Same same-millisecond safeguard `claim_stale_command_for_redrive`
            // uses: the written `updated_at` must always differ from what a
            // second, racing caller's own `expected_updated_at` compares
            // against, regardless of clock resolution.
            let now = now_millis().max(expected_updated_at.saturating_add(1));
            let changed = tx.execute(
                "UPDATE command SET child_run_id = ?1, updated_at = ?2 \
                 WHERE command_id = ?3 AND status IN ('accepted','started') \
                 AND child_run_id = ?4 AND updated_at = ?5",
                params![
                    fresh_run_id.as_str(),
                    now,
                    command_id.as_str(),
                    expected_old_run_id.as_str(),
                    expected_updated_at,
                ],
            )?;
            if changed != 1 {
                // Another writer's transaction committed between the read
                // above and this UPDATE — re-read to report the truth.
                let current = load_command(tx, command_id.as_str())?.ok_or_else(|| {
                    LedgerError::Integrity(format!("command {command_id} vanished"))
                })?;
                return Ok(RebindOutcome::LostRace(current));
            }
            let updated = load_command(tx, command_id.as_str())?
                .ok_or_else(|| LedgerError::Integrity(format!("command {command_id} vanished")))?;
            Ok(RebindOutcome::Rebound(updated))
        })
        .await
    }

    /// Q-Deck R1 fourth corrective round: mark a command `completed` ONLY
    /// if it is still bound to `expected_child_run_id` — one atomic
    /// conditional `UPDATE`, not a read-then-write. Required for BOTH a
    /// normal continuation's own post-seal finish (`o7 continue`) and
    /// `o7d`'s sealed-canonical-record recovery path (Q-Deck R1 §7): in
    /// either case, a superseded attempt (one a redrive has since rebound
    /// away from) must never be able to complete bookkeeping that by now
    /// belongs to a different, later attempt.
    ///
    /// Predicates: `status` is `accepted` or `started`; `child_run_id`
    /// equals `expected_child_run_id`.
    ///
    /// # Errors
    /// Propagates SQLite errors.
    pub async fn mark_command_completed_if_bound(
        &self,
        command_id: crate::CommandId,
        expected_child_run_id: crate::RunId,
    ) -> Result<crate::models::CompletionOutcome, LedgerError> {
        use crate::models::CompletionOutcome;
        self.with_tx(move |tx| {
            let Some(current) = load_command(tx, command_id.as_str())? else {
                return Ok(CompletionOutcome::NotFound);
            };
            let bound = matches!(
                current.status,
                CommandStatus::Accepted | CommandStatus::Started
            ) && current.child_run_id.as_ref().map(crate::RunId::as_str)
                == Some(expected_child_run_id.as_str());
            if !bound {
                return Ok(CompletionOutcome::NotBound(current));
            }
            let now = now_millis();
            let changed = tx.execute(
                "UPDATE command SET status = 'completed', updated_at = ?1 \
                 WHERE command_id = ?2 AND status IN ('accepted','started') \
                 AND child_run_id = ?3",
                params![now, command_id.as_str(), expected_child_run_id.as_str()],
            )?;
            if changed != 1 {
                let current = load_command(tx, command_id.as_str())?.ok_or_else(|| {
                    LedgerError::Integrity(format!("command {command_id} vanished"))
                })?;
                return Ok(CompletionOutcome::NotBound(current));
            }
            let updated = load_command(tx, command_id.as_str())?
                .ok_or_else(|| LedgerError::Integrity(format!("command {command_id} vanished")))?;
            Ok(CompletionOutcome::Completed(updated))
        })
        .await
    }

    /// Q-Deck R1 fifth corrective round: like
    /// [`Self::mark_command_completed_if_bound`], but ALSO requires — in
    /// the SAME transaction — that the bound child run itself actually
    /// exists, is in a permitted TERMINAL status, and belongs to the same
    /// conversation/parent as the command. This is the primitive `o7d`'s
    /// sealed-canonical-record recovery path uses (Q-Deck R1 §7): a scoped
    /// catch-up's own typed success receipt is necessary but not
    /// sufficient on its own — the ledger row it just wrote (or failed to
    /// write) is re-checked here, atomically, at the moment of completion,
    /// rather than trusted from an earlier, separate read. A failed catch-up,
    /// or one whose ledger write didn't actually land a terminal status,
    /// must never complete the command's bookkeeping — this predicate is
    /// what enforces that at the storage layer, not merely at the caller's
    /// own control flow.
    ///
    /// Predicates (all in the SAME conditional `UPDATE`'s surrounding
    /// checks): `command.status IN ('accepted','started')`;
    /// `command.child_run_id == expected_child_run_id`; the child run
    /// EXISTS; the child run's own `status` is a permitted terminal status
    /// (`RunStatus::is_terminal`); the child run's `conversation_id`
    /// equals the command's; the child run's `parent_run_id` equals the
    /// command's `parent_run_id`.
    ///
    /// # Errors
    /// Propagates SQLite errors.
    pub async fn mark_command_completed_if_bound_and_terminal(
        &self,
        command_id: crate::CommandId,
        expected_child_run_id: crate::RunId,
    ) -> Result<crate::models::CompletionOutcome, LedgerError> {
        use crate::models::CompletionOutcome;
        self.with_tx(move |tx| {
            let Some(current) = load_command(tx, command_id.as_str())? else {
                return Ok(CompletionOutcome::NotFound);
            };
            let bound = matches!(
                current.status,
                CommandStatus::Accepted | CommandStatus::Started
            ) && current.child_run_id.as_ref().map(crate::RunId::as_str)
                == Some(expected_child_run_id.as_str());
            if !bound {
                return Ok(CompletionOutcome::NotBound(current));
            }
            let Some(child_run) = load_run(tx, expected_child_run_id.as_str())? else {
                return Ok(CompletionOutcome::NotBound(current));
            };
            let child_ok = child_run.status.is_terminal()
                && child_run.conversation_id == current.conversation_id
                && child_run.parent_run_id.as_ref().map(crate::RunId::as_str)
                    == Some(current.parent_run_id.as_str());
            if !child_ok {
                return Ok(CompletionOutcome::NotBound(current));
            }
            let now = now_millis();
            let changed = tx.execute(
                "UPDATE command SET status = 'completed', updated_at = ?1 \
                 WHERE command_id = ?2 AND status IN ('accepted','started') \
                 AND child_run_id = ?3",
                params![now, command_id.as_str(), expected_child_run_id.as_str()],
            )?;
            if changed != 1 {
                let current = load_command(tx, command_id.as_str())?.ok_or_else(|| {
                    LedgerError::Integrity(format!("command {command_id} vanished"))
                })?;
                return Ok(CompletionOutcome::NotBound(current));
            }
            let updated = load_command(tx, command_id.as_str())?
                .ok_or_else(|| LedgerError::Integrity(format!("command {command_id} vanished")))?;
            Ok(CompletionOutcome::Completed(updated))
        })
        .await
    }

    /// Q-Deck R1 corrective round: a command's post-seal bookkeeping
    /// (`mark_command_completed`, called after the child run's OWN
    /// canonical stream already sealed) is itself a best-effort ledger
    /// write in `o7 continue` — if IT fails, the child run is genuinely
    /// done but the command row is left stuck `accepted`/`started`
    /// forever, again blocking the conversation. Unlike a stuck UNBOUND
    /// command (`stuck_commands`, above), this case needs no provider
    /// invocation to repair — the child run already reached a sealed
    /// terminal status, so reconciling is pure, side-effect-free
    /// bookkeeping. `o7 recover` calls this unconditionally; returns every
    /// command it just repaired.
    ///
    /// # Errors
    /// Propagates SQLite errors.
    pub async fn reconcile_completed_commands(&self) -> Result<Vec<Command>, LedgerError> {
        self.with_tx(|tx| {
            let mut stmt = tx.prepare(
                "SELECT c.command_id FROM command c \
                 JOIN run r ON r.run_id = c.child_run_id \
                 WHERE c.status IN ('accepted','started') \
                 AND r.status IN ('completed','failed','cancelled','blocked','error')",
            )?;
            let ids = stmt
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            let now = now_millis();
            let mut repaired = Vec::with_capacity(ids.len());
            for id in &ids {
                tx.execute(
                    "UPDATE command SET status = 'completed', updated_at = ?1 \
                     WHERE command_id = ?2",
                    params![now, id],
                )?;
                repaired.push(
                    load_command(tx, id)?
                        .ok_or_else(|| LedgerError::Integrity(format!("command {id} vanished")))?,
                );
            }
            Ok(repaired)
        })
        .await
    }

    /// Read a command by id (read-only), if present.
    ///
    /// # Errors
    /// Propagates SQLite errors.
    pub async fn command(
        &self,
        command_id: crate::CommandId,
    ) -> Result<Option<Command>, LedgerError> {
        self.with_conn(move |conn| load_command(conn, command_id.as_str()))
            .await
    }

    /// Durably persist a run's provider session identity (Q-Deck R1 §9.1) —
    /// call once, after the agent call that produced it succeeds. Never
    /// interprets or validates the string beyond storing it — that is
    /// `agent::ProviderSessionId::new`'s job, in the root crate, on both
    /// the write side (before this call) and the strict read-back side
    /// (§6's fail-closed check, which lives in `create_command` above).
    ///
    /// # Errors
    /// [`LedgerError::NotFound`] if the run is missing; SQLite errors.
    pub async fn set_run_provider_session(
        &self,
        run_id: crate::RunId,
        session_id: String,
    ) -> Result<Run, LedgerError> {
        self.with_tx(move |tx| {
            load_run(tx, run_id.as_str())?
                .ok_or_else(|| LedgerError::NotFound(format!("run {run_id}")))?;
            tx.execute(
                "UPDATE run SET provider_session_id = ?1 WHERE run_id = ?2",
                params![session_id, run_id.as_str()],
            )?;
            load_run(tx, run_id.as_str())?
                .ok_or_else(|| LedgerError::NotFound(format!("run {run_id}")))
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

    /// Append a `system.note` event, scoped to a run/attempt. Idempotency is
    /// mandatory (unlike `append_user_message`'s optional one): this exists
    /// for Q-Deck R0.7's live-ingress projector
    /// (`docs/q-deck/r07-live-ingress.md`), where the natural and required
    /// idempotency key is the source canonical event's own `{run_id}:
    /// {sequence}` — a source event must project at most once, ever, and a
    /// caller without a key for that has a bug, not an optional nicety.
    ///
    /// This is the vehicle for projecting `o7-run` canonical event kinds
    /// that have no dedicated `o7-ledger` `EventType` of their own
    /// (`AgentStarted`, `GateFinished`, etc. — deliberately not added to
    /// `EventType` here; see `docs/q-deck/r07-live-ingress.md`'s projector
    /// section) — `payload` carries the canonical kind, sequence, digest,
    /// and schema version, plus kind-specific detail, so the same generic
    /// event/payload/SSE path every other event already uses displays it,
    /// with no new wire concept.
    ///
    /// # Errors
    /// SQLite/foreign-key errors; idempotency conflict.
    pub async fn append_system_note(
        &self,
        conversation_id: crate::ConversationId,
        run_id: Option<crate::RunId>,
        attempt_id: Option<crate::AttemptId>,
        payload: serde_json::Value,
        idempotency: Idempotency,
    ) -> Result<PersistedEvent, LedgerError> {
        let digest_input = serde_json::json!({
            "conversation_id": conversation_id.as_str(),
            "run_id": run_id.as_ref().map(crate::RunId::as_str),
            "attempt_id": attempt_id.as_ref().map(crate::AttemptId::as_str),
            "payload": payload,
        });
        let request_digest = idempotency::digest_bytes(&serde_json::to_vec(&digest_input)?);
        self.with_tx(move |tx| {
            if let IdemOutcome::Replayed(reference) = idempotency::check(
                tx,
                SCOPE_APPEND_SYSTEM_NOTE,
                &idempotency.key,
                &request_digest,
            )? {
                return load_event(tx, &reference)?
                    .ok_or_else(|| LedgerError::NotFound(format!("event {reference}")));
            }
            let now = now_millis();
            let event = emit_event(
                tx,
                &NewEvent {
                    event_id: EventId::generate(),
                    conversation_id: conversation_id.clone(),
                    run_id: run_id.clone(),
                    attempt_id: attempt_id.clone(),
                    event_type: EventType::SystemNote,
                    schema_version: EVENT_SCHEMA_VERSION,
                    payload: payload.clone(),
                },
                now,
            )?;
            idempotency::record(
                tx,
                SCOPE_APPEND_SYSTEM_NOTE,
                &idempotency.key,
                &request_digest,
                event.event_id.as_str(),
                now,
            )?;
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
                // Close EVERY running attempt of this run — not only the ones the
                // caller happened to list. The snapshot may be stale or partial;
                // the ledger, holding the write transaction, is authoritative, so
                // a run can never be left `interrupted` with a `running` attempt.
                tx.execute(
                    "UPDATE run_attempt SET status = 'interrupted', finished_at = ?1 \
                     WHERE run_id = ?2 AND status = 'running'",
                    params![now, run_id.as_str()],
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
            // Any explicitly-listed attempts whose run was NOT in the run list:
            // close them too, but tolerate ones already closed above (idempotent).
            for attempt_id in &state.interrupted_attempts {
                let current = load_attempt(tx, attempt_id.as_str())?
                    .ok_or_else(|| LedgerError::NotFound(format!("attempt {attempt_id}")))?;
                if current.status == AttemptStatus::Running {
                    let now = now_millis();
                    tx.execute(
                        "UPDATE run_attempt SET status = 'interrupted', finished_at = ?1 \
                         WHERE attempt_id = ?2 AND status = 'running'",
                        params![now, attempt_id.as_str()],
                    )?;
                }
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

    /// Load an attempt by id (read-only), if present.
    /// # Errors
    /// Propagates SQLite errors.
    pub async fn attempt(
        &self,
        attempt_id: crate::AttemptId,
    ) -> Result<Option<RunAttempt>, LedgerError> {
        self.with_conn(move |conn| load_attempt(conn, attempt_id.as_str()))
            .await
    }

    /// The run's currently-`running` attempt, if any (read-only). At most
    /// one exists per run by construction (`create_attempt`'s own
    /// partial-unique-index invariant). For Q-Deck R0.7's live-ingress
    /// projector: after a process restart, the in-memory `attempt_id` from
    /// the crashed process is gone — this is how a resumed projector finds
    /// the attempt it should keep projecting into instead of creating a
    /// second one.
    ///
    /// # Errors
    /// Propagates SQLite errors.
    pub async fn running_attempt(
        &self,
        run_id: crate::RunId,
    ) -> Result<Option<RunAttempt>, LedgerError> {
        self.with_conn(move |conn| {
            let id: Option<String> = conn
                .query_row(
                    "SELECT attempt_id FROM run_attempt WHERE run_id = ?1 AND status = 'running'",
                    params![run_id.as_str()],
                    |row| row.get(0),
                )
                .optional()?;
            match id {
                None => Ok(None),
                Some(id) => load_attempt(conn, &id),
            }
        })
        .await
    }

    /// The run's most recent attempt (read-only), **regardless of status** —
    /// unlike [`Self::running_attempt`], this also finds a `completed`/
    /// `failed`/`interrupted` attempt. For Q-Deck R0.7's `o7 recover`
    /// catch-up: re-projecting an already-terminal run's canonical stream
    /// must reuse the SAME `attempt_id` its events were originally recorded
    /// under, because [`Self::append_system_note`]'s idempotency digest
    /// includes `attempt_id` — attaching read-only with a DIFFERENT
    /// `attempt_id` (e.g. `None`) than the one already-projected events used
    /// would make a byte-identical re-projection of an already-applied event
    /// fail as an idempotency conflict, not replay it.
    ///
    /// # Errors
    /// Propagates SQLite errors.
    pub async fn latest_attempt(
        &self,
        run_id: crate::RunId,
    ) -> Result<Option<RunAttempt>, LedgerError> {
        self.with_conn(move |conn| {
            let id: Option<String> = conn
                .query_row(
                    "SELECT attempt_id FROM run_attempt WHERE run_id = ?1 \
                     ORDER BY attempt_number DESC LIMIT 1",
                    params![run_id.as_str()],
                    |row| row.get(0),
                )
                .optional()?;
            match id {
                None => Ok(None),
                Some(id) => load_attempt(conn, &id),
            }
        })
        .await
    }

    /// List conversations newest-first, keyset-paginated by `(created_at, id)`.
    /// `before = None` starts at the most recent conversation. Same exhaustion
    /// contract as [`Ledger::read_events`]: keep calling with
    /// `before = page.next_before` until a page comes back empty — a page
    /// exactly `limit` long is not itself proof there is nothing older.
    ///
    /// # Errors
    /// Propagates SQLite errors.
    pub async fn list_conversations(
        &self,
        before: Option<ListCursor>,
        limit: usize,
    ) -> Result<Page<Conversation>, LedgerError> {
        let capped = limit.min(MAX_LIST_LIMIT);
        self.with_conn(move |conn| {
            let cursor_ts = before.as_ref().map(|c| c.created_at);
            let cursor_tiebreak = before.as_ref().map(|c| c.tiebreak);
            let cap = i64::try_from(capped).unwrap_or(0);
            let mut stmt = conn.prepare(
                "SELECT rowid, conversation_id, created_at, status FROM conversation \
                 WHERE ?1 IS NULL OR created_at < ?1 OR (created_at = ?1 AND rowid < ?2) \
                 ORDER BY created_at DESC, rowid DESC LIMIT ?3",
            )?;
            let rows = stmt.query_map(params![cursor_ts, cursor_tiebreak, cap], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })?;
            let mut items = Vec::new();
            let mut last_key: Option<(i64, i64)> = None;
            for row in rows {
                let (rowid, id, created_at, status) = row?;
                last_key = Some((created_at, rowid));
                items.push(Conversation {
                    conversation_id: crate::ConversationId::from_raw(id),
                    created_at,
                    status: ConversationStatus::parse(&status).ok_or_else(|| {
                        LedgerError::Integrity(format!("bad conversation status {status}"))
                    })?,
                });
            }
            let next_before = last_key.map(|(created_at, tiebreak)| ListCursor {
                created_at,
                tiebreak,
            });
            Ok(Page { items, next_before })
        })
        .await
    }

    /// List runs newest-first, keyset-paginated by `(created_at, id)`, optionally
    /// scoped to one conversation and/or filtered to a set of statuses.
    /// `conversation_id = None` lists across every conversation (the cockpit
    /// dashboard's "all runs" view); `Some(id)` scopes to one conversation's
    /// runs (the conversation page's run list). `statuses = None` matches any
    /// status; `Some(&[...])` restricts to exactly those — the dashboard's
    /// "active" section uses this (`[Queued, Running]`) so correctness does
    /// not depend on an active run happening to fall within the newest page:
    /// the filter is applied by the database, not by the caller hoping a
    /// bounded page was enough. Same exhaustion contract as
    /// [`Self::list_conversations`].
    ///
    /// # Errors
    /// Propagates SQLite errors.
    pub async fn list_runs(
        &self,
        conversation_id: Option<crate::ConversationId>,
        statuses: Option<Vec<RunStatus>>,
        before: Option<ListCursor>,
        limit: usize,
    ) -> Result<Page<Run>, LedgerError> {
        let capped = limit.min(MAX_LIST_LIMIT);
        self.with_conn(move |conn| {
            // Built dynamically because the status set has variable arity —
            // an `IN (?,?,...)` clause can't be expressed with a fixed-arity
            // `params![]` call. Every value is still a bound parameter, never
            // string-interpolated, so this stays fully parameterized despite
            // the variable shape.
            let mut sql = String::from(
                "SELECT rowid, run_id, conversation_id, parent_run_id, agent, role, status, \
                 created_at, finished_at, provider_session_id FROM run WHERE 1=1",
            );
            let mut bound: Vec<Box<dyn rusqlite::ToSql + Send>> = Vec::new();

            if let Some(conv) = &conversation_id {
                sql.push_str(" AND conversation_id = ?");
                bound.push(Box::new(conv.as_str().to_owned()));
            }
            if let Some(statuses) = &statuses {
                let placeholders = vec!["?"; statuses.len()].join(",");
                sql.push_str(&format!(" AND status IN ({placeholders})"));
                for s in statuses {
                    bound.push(Box::new(s.as_str().to_owned()));
                }
            }
            if let Some(before) = &before {
                sql.push_str(" AND (created_at < ? OR (created_at = ? AND rowid < ?))");
                bound.push(Box::new(before.created_at));
                bound.push(Box::new(before.created_at));
                bound.push(Box::new(before.tiebreak));
            }
            sql.push_str(" ORDER BY created_at DESC, rowid DESC LIMIT ?");
            let cap = i64::try_from(capped).unwrap_or(0);
            bound.push(Box::new(cap));

            let mut stmt = conn.prepare(&sql)?;
            let param_refs: Vec<&dyn rusqlite::ToSql> = bound
                .iter()
                .map(|b| b.as_ref() as &dyn rusqlite::ToSql)
                .collect();
            let rows = stmt.query_map(param_refs.as_slice(), |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, i64>(7)?,
                    row.get::<_, Option<i64>>(8)?,
                    row.get::<_, Option<String>>(9)?,
                ))
            })?;
            let mut items = Vec::new();
            let mut last_key: Option<(i64, i64)> = None;
            for row in rows {
                let (
                    rowid,
                    id,
                    conv,
                    parent,
                    agent,
                    role,
                    status,
                    created_at,
                    finished_at,
                    session,
                ) = row?;
                last_key = Some((created_at, rowid));
                items.push(Run {
                    run_id: crate::RunId::from_raw(id),
                    conversation_id: crate::ConversationId::from_raw(conv),
                    parent_run_id: parent.map(crate::RunId::from_raw),
                    agent,
                    role,
                    status: RunStatus::parse(&status).ok_or_else(|| {
                        LedgerError::Integrity(format!("bad run status {status}"))
                    })?,
                    created_at,
                    finished_at,
                    provider_session_id: session,
                });
            }
            let next_before = last_key.map(|(created_at, tiebreak)| ListCursor {
                created_at,
                tiebreak,
            });
            Ok(Page { items, next_before })
        })
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

/// Verify the durability pragmas actually took effect. Foreign keys must always
/// be on; WAL + `synchronous = FULL` are asserted for file-backed databases
/// (they are inert, hence not asserted, for `:memory:`).
fn verify_effective_pragmas(
    conn: &Connection,
    expect_wal: bool,
    journal_mode: &str,
) -> Result<(), LedgerError> {
    let foreign_keys: i64 = conn.query_row("PRAGMA foreign_keys;", [], |row| row.get(0))?;
    if foreign_keys != 1 {
        return Err(LedgerError::Integrity(
            "foreign_keys is not enabled".to_owned(),
        ));
    }
    if expect_wal {
        if !journal_mode.eq_ignore_ascii_case("wal") {
            return Err(LedgerError::Integrity(format!(
                "journal_mode is `{journal_mode}`, expected wal (durability requires WAL)"
            )));
        }
        let synchronous: i64 = conn.query_row("PRAGMA synchronous;", [], |row| row.get(0))?;
        if synchronous != 2 {
            return Err(LedgerError::Integrity(format!(
                "synchronous is {synchronous}, expected FULL(2)"
            )));
        }
    }
    Ok(())
}

/// Map a terminal/interrupted RUN status to the attempt status its running
/// attempt should receive. Only called when the run target is terminal or
/// interrupted.
// Only ever called with a target that is terminal or `Interrupted` (see the
// call site's guard) — `Queued`/`Running` never reach here. Every other
// variant is listed explicitly, not behind a catch-all: a catch-all here
// would silently mislabel e.g. a blocked/errored run's attempt as
// `Completed`, exactly the meaning-collapse this ledger exists to prevent.
fn terminal_attempt_status(run_target: RunStatus) -> AttemptStatus {
    match run_target {
        RunStatus::Completed => AttemptStatus::Completed,
        RunStatus::Failed => AttemptStatus::Failed,
        RunStatus::Cancelled => AttemptStatus::Cancelled,
        RunStatus::Interrupted => AttemptStatus::Interrupted,
        RunStatus::Blocked => AttemptStatus::Blocked,
        RunStatus::Error => AttemptStatus::Error,
        RunStatus::Queued | RunStatus::Running => AttemptStatus::Completed,
    }
}

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

fn load_command(conn: &Connection, command_id: &str) -> Result<Option<Command>, LedgerError> {
    let row = conn
        .query_row(
            "SELECT command_id, conversation_id, parent_run_id, command_text, status, \
             child_run_id, created_at, updated_at \
             FROM command WHERE command_id = ?1",
            params![command_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, i64>(6)?,
                    row.get::<_, i64>(7)?,
                ))
            },
        )
        .optional()?;
    match row {
        None => Ok(None),
        Some((id, conv, parent, text, status, child, created_at, updated_at)) => {
            Ok(Some(Command {
                command_id: crate::CommandId::from_raw(id),
                conversation_id: crate::ConversationId::from_raw(conv),
                parent_run_id: crate::RunId::from_raw(parent),
                command_text: text,
                status: CommandStatus::parse(&status).ok_or_else(|| {
                    LedgerError::Integrity(format!("bad command status {status}"))
                })?,
                child_run_id: child.map(crate::RunId::from_raw),
                created_at,
                updated_at,
            }))
        }
    }
}

fn load_run(conn: &Connection, run_id: &str) -> Result<Option<Run>, LedgerError> {
    let row = conn
        .query_row(
            "SELECT run_id, conversation_id, parent_run_id, agent, role, status, created_at, \
             finished_at, provider_session_id \
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
                    row.get::<_, Option<String>>(8)?,
                ))
            },
        )
        .optional()?;
    match row {
        None => Ok(None),
        Some((id, conv, parent, agent, role, status, created_at, finished_at, session)) => {
            Ok(Some(Run {
                run_id: crate::RunId::from_raw(id),
                conversation_id: crate::ConversationId::from_raw(conv),
                parent_run_id: parent.map(crate::RunId::from_raw),
                agent,
                role,
                status: RunStatus::parse(&status)
                    .ok_or_else(|| LedgerError::Integrity(format!("bad run status {status}")))?,
                created_at,
                finished_at,
                provider_session_id: session,
            }))
        }
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
