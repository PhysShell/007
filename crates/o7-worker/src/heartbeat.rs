//! Heartbeat policy.
//!
//! A heartbeat means **the supervisor is alive and owns a live process** — NOT
//! that the process is doing useful work. A silent compiler/verifier/model is
//! perfectly alive, so heartbeats are driven by a monotonic timer and are
//! completely independent of stdout/stderr activity. The absence of output is
//! never treated as a hang, and any hang/timeout policy belongs to a future
//! manager/o7d, not to the worker itself.

use std::time::Duration;

/// Heartbeat cadence. Timing is measured on the monotonic clock (`Instant`),
/// never wall-clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeartbeatPolicy {
    pub enabled: bool,
    pub interval: Duration,
}

impl Default for HeartbeatPolicy {
    fn default() -> Self {
        Self {
            enabled: true,
            interval: Duration::from_secs(1),
        }
    }
}
