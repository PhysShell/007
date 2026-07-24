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

#[cfg(test)]
mod tests {
    use super::WorkerState::{Cancelling, Created, Exited, FailedToStart, Running, Starting};

    #[test]
    fn documented_transitions_are_allowed() {
        assert!(Created.can_transition_to(Starting));
        assert!(Starting.can_transition_to(Running));
        assert!(Starting.can_transition_to(FailedToStart));
        assert!(Starting.can_transition_to(Cancelling));
        assert!(Running.can_transition_to(Cancelling));
        assert!(Running.can_transition_to(Exited));
        assert!(Cancelling.can_transition_to(Exited));
    }

    #[test]
    fn undocumented_transitions_are_denied() {
        // Must pass through Starting; no skipping, no going back, no leaving a
        // terminal state.
        assert!(!Created.can_transition_to(Running));
        assert!(!Created.can_transition_to(Exited));
        assert!(!Running.can_transition_to(Starting));
        assert!(!Cancelling.can_transition_to(Running));
        assert!(!Exited.can_transition_to(Running));
        assert!(!FailedToStart.can_transition_to(Exited));
    }

    #[test]
    fn only_exited_and_failed_are_terminal() {
        assert!(Exited.is_terminal());
        assert!(FailedToStart.is_terminal());
        assert!(!Created.is_terminal());
        assert!(!Starting.is_terminal());
        assert!(!Running.is_terminal());
        assert!(!Cancelling.is_terminal());
    }
}
