//! Acceptance 26-28: heartbeats mean supervisor liveness — they flow during
//! silence, stop after the terminal, and silence is never treated as a hang.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod common;

use std::time::Duration;

use common::*;
use o7_worker::WorkerResult;

/// Generous but bounded ceiling for readiness waits — heartbeats tick on the order of
/// 100ms; this only guards against a genuine hang, never a timing assumption.
const READY_TIMEOUT: Duration = Duration::from_secs(10);

// (26) A silent process still produces heartbeats.
#[tokio::test]
async fn silent_process_produces_heartbeats() {
    let sink = RecordingSink::new();
    let (handle, join) = start(child_spec("hb26", "sleep"), &sink);
    // Wait until at least two heartbeats have actually been observed before cancelling,
    // instead of assuming they arrived within a fixed sleep.
    sink.wait_for_kind_count("heartbeat", 2, READY_TIMEOUT)
        .await
        .unwrap();
    handle.cancel().await;
    join.join().await;
    assert!(
        sink.heartbeats() >= 2,
        "got {} heartbeats",
        sink.heartbeats()
    );
}

// (27) Heartbeats stop after the terminal state.
#[tokio::test]
async fn heartbeats_stop_after_exit() {
    let sink = RecordingSink::new();
    run_to_completion(child_spec("hb27", "exit0"), &sink).await;
    let before = sink.heartbeats();
    // This sleep is NOT a readiness wait and is intentionally left as-is: the worker has
    // already reached its terminal, and the test asserts the ABSENCE of any further
    // heartbeat over a time window. There is no event to await here — the property is
    // "nothing happens during this interval" — so a bounded time window is exactly right.
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(
        sink.heartbeats(),
        before,
        "no heartbeats after the terminal"
    );
}

// (28) A process with no stdout is NOT treated as a hang — it only terminates on
// an explicit cancel, never on its own from silence.
#[tokio::test]
async fn silence_is_not_a_hang() {
    let sink = RecordingSink::new();
    let (handle, join) = start(child_spec("hb28", "sleep"), &sink);
    // Wait until the worker has proven itself alive during silence (at least one
    // heartbeat) before cancelling.
    sink.wait_for_kind_count("heartbeat", 1, READY_TIMEOUT)
        .await
        .unwrap();
    handle.cancel().await;
    let result = join.join().await;
    // If silence had auto-terminated the worker it would be Exited*, not Cancelled.
    assert!(
        matches!(
            result,
            WorkerResult::CancelledGracefully | WorkerResult::CancelledForcefully
        ),
        "got {result:?}"
    );
    assert!(sink.heartbeats() >= 1, "worker was alive during silence");
}
