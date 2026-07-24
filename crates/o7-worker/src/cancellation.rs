//! Cancellation policy. Cancellation itself is idempotent and drives the whole
//! boundary-owned process set to termination (see the supervisor); this type only
//! carries the tunable grace period.

use std::time::Duration;

/// How cancellation escalates: a graceful stop (SIGTERM to the group) is sent
/// first, and after `graceful_timeout` any survivors get a forceful stop (SIGKILL
/// to the group).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CancellationPolicy {
    pub graceful_timeout: Duration,
}

impl Default for CancellationPolicy {
    fn default() -> Self {
        Self {
            graceful_timeout: Duration::from_secs(5),
        }
    }
}
