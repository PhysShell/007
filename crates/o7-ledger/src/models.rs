//! Ledger entities, statuses, and the closed PR-1 event set.

use serde::{Deserialize, Serialize};

use crate::ids::{AttemptId, ConversationId, EventId, RunId};

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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Queued,
    Running,
    Completed,
    Failed,
    Cancelled,
    Interrupted,
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
            _ => None,
        }
    }

    /// A terminal status can never transition further.
    #[must_use]
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }
}

/// Lifecycle status of a run attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttemptStatus {
    Running,
    Completed,
    Failed,
    Cancelled,
    Interrupted,
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
            _ => None,
        }
    }
}

/// The closed set of event types for PR 1. Claude/Codex-specific events, tool
/// calls, permission modes, model drift, delegation, artifacts and gates are
/// intentionally NOT here — they arrive in PR 4.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventType {
    ConversationCreated,
    RunCreated,
    RunStarted,
    RunCompleted,
    RunFailed,
    RunCancelled,
    RunInterrupted,
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
