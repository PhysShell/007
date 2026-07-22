//! Centralized state-transition validation. Statuses are never compared as bare
//! strings scattered across SQL/CLI — every transition goes through here, and
//! anything not explicitly allowed is rejected (default-deny).

use crate::models::{AttemptStatus, RunStatus};
use crate::LedgerError;

/// Allowed run transitions via the GENERAL path (`start_run`/`complete_run`/…):
/// `queued → running` and `running → {completed, failed, cancelled, interrupted}`.
/// Everything else — including `completed → running`, `failed → completed`,
/// `cancelled → completed` — is forbidden here.
///
/// NOTE: `interrupted → running` is intentionally NOT in this set. Resuming an
/// interrupted run is only possible through
/// [`SqliteLedger::resume_interrupted_run`](crate::SqliteLedger::resume_interrupted_run),
/// which performs that transition atomically together with a new attempt, so
/// `start_run` cannot revive an interrupted run without one.
#[must_use]
pub fn run_transition_allowed(from: RunStatus, to: RunStatus) -> bool {
    use RunStatus::{Cancelled, Completed, Failed, Interrupted, Queued, Running};
    matches!(
        (from, to),
        (Queued, Running)
            | (Running, Completed)
            | (Running, Failed)
            | (Running, Cancelled)
            | (Running, Interrupted)
    )
}

/// Validate a run transition, returning [`LedgerError::ForbiddenTransition`] if
/// it is not permitted.
///
/// # Errors
/// Returns an error when the `(from, to)` pair is not in the allowed set.
pub fn validate_run_transition(from: RunStatus, to: RunStatus) -> Result<(), LedgerError> {
    if run_transition_allowed(from, to) {
        Ok(())
    } else {
        Err(LedgerError::ForbiddenTransition {
            entity: "run",
            from: from.as_str(),
            to: to.as_str(),
        })
    }
}

/// Allowed attempt transitions: an attempt starts `running` and may move to
/// `{completed, failed, cancelled, interrupted}`. Attempts never restart — a
/// new attempt is created instead.
#[must_use]
pub fn attempt_transition_allowed(from: AttemptStatus, to: AttemptStatus) -> bool {
    use AttemptStatus::{Cancelled, Completed, Failed, Interrupted, Running};
    matches!(
        (from, to),
        (Running, Completed) | (Running, Failed) | (Running, Cancelled) | (Running, Interrupted)
    )
}

/// Validate an attempt transition.
///
/// # Errors
/// Returns [`LedgerError::ForbiddenTransition`] when the pair is not allowed.
pub fn validate_attempt_transition(
    from: AttemptStatus,
    to: AttemptStatus,
) -> Result<(), LedgerError> {
    if attempt_transition_allowed(from, to) {
        Ok(())
    } else {
        Err(LedgerError::ForbiddenTransition {
            entity: "attempt",
            from: from.as_str(),
            to: to.as_str(),
        })
    }
}
