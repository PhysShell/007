//! Ledger entities, statuses, and the closed PR-1 event set.

use serde::{Deserialize, Serialize};

use crate::ids::{AttemptId, CommandId, ConversationId, EventId, RunId};

/// Lifecycle status of a conversation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConversationStatus {
    Open,
    Closed,
}

impl ConversationStatus {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Closed => "closed",
        }
    }

    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "open" => Some(Self::Open),
            "closed" => Some(Self::Closed),
            _ => None,
        }
    }
}

/// Lifecycle status of a run. Transitions are enforced centrally in
/// [`crate::transitions`]; never compare these as bare strings in SQL/CLI.
///
/// `Blocked`/`Error` (added in Q-Deck R0.6, see
/// `docs/q-deck/r06-verdict-fidelity.md`) are the ledger's projection of
/// `o7-run::Verdict`'s own sealed `Blocked`/`Error` — both are terminal here
/// exactly as they are sealed there. `Interrupted` is a DIFFERENT concept
/// (unsealed, resumable) and must never be conflated with `Error`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Queued,
    Running,
    Completed,
    Failed,
    Cancelled,
    Interrupted,
    Blocked,
    Error,
}

impl RunStatus {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::Interrupted => "interrupted",
            Self::Blocked => "blocked",
            Self::Error => "error",
        }
    }

    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "queued" => Some(Self::Queued),
            "running" => Some(Self::Running),
            "completed" => Some(Self::Completed),
            "failed" => Some(Self::Failed),
            "cancelled" => Some(Self::Cancelled),
            "interrupted" => Some(Self::Interrupted),
            "blocked" => Some(Self::Blocked),
            "error" => Some(Self::Error),
            _ => None,
        }
    }

    /// A terminal (sealed) status can never transition further.
    /// `Interrupted` is deliberately excluded — it is resumable
    /// (`resume_interrupted_run`), not sealed.
    #[must_use]
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Failed | Self::Cancelled | Self::Blocked | Self::Error
        )
    }
}

/// Lifecycle status of a run attempt. Mirrors the sealed subset of
/// [`RunStatus`] (plus `Interrupted`) so a blocked/errored run's attempt is
/// never mislabeled — see [`crate::sqlite::terminal_attempt_status`], which
/// this enum's variants must stay in lockstep with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttemptStatus {
    Running,
    Completed,
    Failed,
    Cancelled,
    Interrupted,
    Blocked,
    Error,
}

impl AttemptStatus {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::Interrupted => "interrupted",
            Self::Blocked => "blocked",
            Self::Error => "error",
        }
    }

    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "running" => Some(Self::Running),
            "completed" => Some(Self::Completed),
            "failed" => Some(Self::Failed),
            "cancelled" => Some(Self::Cancelled),
            "interrupted" => Some(Self::Interrupted),
            "blocked" => Some(Self::Blocked),
            "error" => Some(Self::Error),
            _ => None,
        }
    }
}

/// The closed set of event types for PR 1 (plus R0.6's `RunBlocked`/
/// `RunErrored`). Claude/Codex-specific events, tool calls, permission
/// modes, model drift, delegation, artifacts and gates are intentionally
/// NOT here — they arrive in PR 4.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventType {
    ConversationCreated,
    RunCreated,
    RunStarted,
    RunCompleted,
    RunFailed,
    RunCancelled,
    RunInterrupted,
    RunBlocked,
    RunErrored,
    UserMessage,
    SystemNote,
}

impl EventType {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ConversationCreated => "conversation.created",
            Self::RunCreated => "run.created",
            Self::RunStarted => "run.started",
            Self::RunCompleted => "run.completed",
            Self::RunFailed => "run.failed",
            Self::RunCancelled => "run.cancelled",
            Self::RunInterrupted => "run.interrupted",
            Self::RunBlocked => "run.blocked",
            Self::RunErrored => "run.errored",
            Self::UserMessage => "user.message",
            Self::SystemNote => "system.note",
        }
    }

    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "conversation.created" => Some(Self::ConversationCreated),
            "run.created" => Some(Self::RunCreated),
            "run.started" => Some(Self::RunStarted),
            "run.completed" => Some(Self::RunCompleted),
            "run.failed" => Some(Self::RunFailed),
            "run.cancelled" => Some(Self::RunCancelled),
            "run.interrupted" => Some(Self::RunInterrupted),
            "run.blocked" => Some(Self::RunBlocked),
            "run.errored" => Some(Self::RunErrored),
            "user.message" => Some(Self::UserMessage),
            "system.note" => Some(Self::SystemNote),
            _ => None,
        }
    }
}

/// A conversation row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Conversation {
    pub conversation_id: ConversationId,
    pub created_at: i64,
    pub status: ConversationStatus,
}

/// A run row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Run {
    pub run_id: RunId,
    pub conversation_id: ConversationId,
    pub parent_run_id: Option<RunId>,
    pub agent: String,
    pub role: String,
    pub status: RunStatus,
    pub created_at: i64,
    pub finished_at: Option<i64>,
    /// The provider's own session identity this run's agent call produced,
    /// if any (Q-Deck R1, `docs/q-deck/r1-command.md` §6/§9.3,
    /// `SCHEMA_V3`) — opaque to this crate (never parsed, never
    /// interpreted), carried only so a FUTURE command can continue this
    /// run's lineage. `None` for a run whose agent call never returned
    /// one, a `--no-ledger` run, or any run that predates this column.
    pub provider_session_id: Option<String>,
}

/// A run-attempt row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunAttempt {
    pub attempt_id: AttemptId,
    pub run_id: RunId,
    pub attempt_number: u32,
    pub status: AttemptStatus,
    pub started_at: i64,
    pub finished_at: Option<i64>,
}

/// A durable command's own lifecycle status (Q-Deck R1,
/// `docs/q-deck/r1-command.md` §2) — NEVER a second verdict authority. This
/// is bookkeeping for the command's OWN acceptance/dispatch state;
/// `Completed` means only "the child run reached a sealed terminal
/// status," read back from that run, never independently decided here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandStatus {
    Accepted,
    Started,
    Completed,
    Rejected,
}

impl CommandStatus {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Accepted => "accepted",
            Self::Started => "started",
            Self::Completed => "completed",
            Self::Rejected => "rejected",
        }
    }

    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "accepted" => Some(Self::Accepted),
            "started" => Some(Self::Started),
            "completed" => Some(Self::Completed),
            "rejected" => Some(Self::Rejected),
            _ => None,
        }
    }
}

/// A durably accepted command row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Command {
    pub command_id: CommandId,
    pub conversation_id: ConversationId,
    pub parent_run_id: RunId,
    pub command_text: String,
    pub status: CommandStatus,
    pub child_run_id: Option<RunId>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Input to [`crate::sqlite::SqliteLedger::create_command`]. Idempotency is
/// mandatory (like [`crate::sqlite::SqliteLedger::create_run_with_id`]) —
/// there is no legitimate "fire and forget, no idempotency key" case for a
/// mutating HTTP endpoint a client may legitimately retry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewCommand {
    pub conversation_id: ConversationId,
    pub parent_run_id: RunId,
    pub command_text: String,
}

/// Input to append an event. The `sequence` and `created_at` are assigned by the
/// ledger inside the append transaction, never by the caller.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewEvent {
    pub event_id: EventId,
    pub conversation_id: ConversationId,
    pub run_id: Option<RunId>,
    pub attempt_id: Option<AttemptId>,
    pub event_type: EventType,
    pub schema_version: u32,
    pub payload: serde_json::Value,
}

/// A persisted event, returned only after a successful commit. `event_type` is
/// the raw stored string (forward-compatible: a value written by a newer schema
/// version still reads back rather than failing).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersistedEvent {
    pub event_id: EventId,
    pub conversation_id: ConversationId,
    pub run_id: Option<RunId>,
    pub attempt_id: Option<AttemptId>,
    pub sequence: u64,
    pub event_type: String,
    pub schema_version: u32,
    pub created_at: i64,
    pub payload: serde_json::Value,
}

/// A persisted idempotency record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdempotencyRecord {
    pub scope: String,
    pub key: String,
    pub request_digest: String,
    pub result_reference: String,
    pub created_at: i64,
}

/// An optional idempotency handle supplied by the caller for the idempotent
/// operations (`create-conversation`, `create-run`, `append-user-message`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Idempotency {
    pub key: String,
}

/// Request to create a run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewRun {
    pub conversation_id: ConversationId,
    pub parent_run_id: Option<RunId>,
    pub agent: String,
    pub role: String,
}

/// A keyset-pagination cursor over a `(created_at, tiebreak)` total order,
/// newest first.
///
/// `created_at` alone is not a safe cursor: two rows can share a millisecond
/// under real load. The tiebreak is SQLite's own `rowid` for the underlying
/// table — deliberately NOT the entity's UUID id: an id tiebreak would order a
/// same-millisecond tie lexicographically, which is arbitrary relative to which
/// row was actually inserted first. `rowid` is monotonic by insertion order for
/// these append-only, never-deleted tables, so it makes same-millisecond ties
/// resolve in their true relative order — "newest first" then means what it
/// says even inside one millisecond, not just between distinct milliseconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListCursor {
    pub created_at: i64,
    pub tiebreak: i64,
}

/// One page of a keyset-paginated, newest-first listing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Page<T> {
    pub items: Vec<T>,
    /// Pass as the next call's `before` to continue further back into
    /// history. `Some` on every non-empty page, including the final partial
    /// one — its presence is NOT proof there are more rows. Exhaustion is
    /// only established when a subsequent call with this cursor returns an
    /// empty page; `None` here means only "this page itself was empty."
    pub next_before: Option<ListCursor>,
}

/// What a recovery scan found: runs/attempts still in `running` at startup —
/// i.e. interrupted by whatever stopped the previous process. The ledger does
/// NOT change their status on its own; the control plane decides.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecoveryState {
    pub interrupted_runs: Vec<RunId>,
    pub interrupted_attempts: Vec<AttemptId>,
}

impl RecoveryState {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.interrupted_runs.is_empty() && self.interrupted_attempts.is_empty()
    }
}
