//! Re-gate blocker: `BoundaryProcess::wait()` carries a CANCEL-SAFETY contract (now
//! documented on the trait). The supervisor polls `wait()` inside a `tokio::select!`
//! and DROPS the pending future whenever another branch (heartbeat, output, cancel)
//! wins, recreating it next iteration. A leader exit that becomes ready while the
//! future is not being polled must still be observed by a later `wait()` call.
//!
//! This adversarially exercises that contract: a boundary whose leader exits at an
//! ABSOLUTE deadline (cancel-safe `sleep_until`) under a very dense heartbeat that
//! forces the supervisor to drop and recreate `wait()` dozens of times before the
//! deadline. A cancel-safe `wait()` still reports the exit; a NON-cancel-safe one
//! (a relative timer restarted on each drop) would postpone the exit forever and this
//! test would hang.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod common;

use std::time::Duration;

use common::mock::MockBoundary;
use common::*;
use o7_worker::{HeartbeatPolicy, WorkerResult};

const OUTER_BOUND: Duration = Duration::from_secs(8);

#[tokio::test]
async fn wait_is_cancel_safe_across_repeated_select_drops() {
    // Leader exits 250ms after spawn. Heartbeats every 5ms → ~50 select iterations
    // before the deadline, each dropping and recreating the pending `wait()` future.
    let boundary = MockBoundary::new().with_leader_exit_after(Duration::from_millis(250));
    let sink = RecordingSink::new();

    let mut spec = child_spec("cs-wait", "unused");
    spec.heartbeat = HeartbeatPolicy {
        enabled: true,
        interval: Duration::from_millis(5),
    };

    let result = tokio::time::timeout(OUTER_BOUND, run_with(spec, boundary.boxed(), &sink))
        .await
        .expect(
            "a cancel-safe wait() must still observe the exit; a hang means the contract broke",
        );

    assert_eq!(
        result,
        WorkerResult::ExitedNormally(0),
        "the leader exit must be observed despite many wait() drops/recreations: {result:?}"
    );
    // Prove the drops actually happened: the heartbeat fired many times (each an
    // iteration where a non-wait branch won and the wait() future was dropped).
    assert!(
        sink.heartbeats() >= 10,
        "expected many select iterations (heartbeats) before the exit, got {}",
        sink.heartbeats()
    );
}
