//! Worker lifecycle state machine.
//!
//! ```text
//! Created → Starting → Running → Cancelling → Exited
//!                 │        └────────────────→ Exited
//!                 ├──────────────────────────→ FailedToStart
//!                 └──────────────────────────→ Cancelling
//! ```
//! Terminal states are `Exited` and `FailedToStart`. Any transition not listed is
//! rejected. The supervisor produces exactly ONE terminal result regardless of
//! how many terminating events race.

/// The worker's lifecycle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerState {
    Created,
    Starting,
    Running,
    Cancelling,
    Exited,
    FailedToStart,
}

impl WorkerState {
    /// A terminal state has no outgoing transitions.
    #[must_use]
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Exited | Self::FailedToStart)
    }

    /// Whether `self → to` is a permitted transition (default-deny).
    #[must_use]
    pub fn can_transition_to(self, to: WorkerState) -> bool {
        use WorkerState::{Cancelling, Created, Exited, FailedToStart, Running, Starting};
        matches!(
            (self, to),
            (Created, Starting)
                | (Starting, Running)
                | (Starting, FailedToStart)
                | (Starting, Cancelling)
                | (Running, Cancelling)
                | (Running, Exited)
                | (Cancelling, Exited)
        )
    }
}
