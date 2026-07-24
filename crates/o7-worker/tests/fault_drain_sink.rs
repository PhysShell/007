//! Re-gate blocker: a trailing-drain fault must NOT be erased when the terminal sink
//! also fails. The drain outcome (a bounded-drain timeout, or a late read error seen
//! only during the post-exit drain) is folded into an EFFECTIVE OutputFailure BEFORE
//! sink-loss precedence is applied, so that when the sink then dies on `Exited` or
//! `CleanupCompleted` the dominating `ObservationFailure` still carries the underlying
//! output fault in its message.
//!
//! Precedence stays CleanupFailure > ObservationFailure > Boundary/Output, but the
//! Boundary/Output fault is preserved for diagnosis, never lost.
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
use o7_worker::WorkerResult;

const OUTER_BOUND: Duration = Duration::from_secs(8);

/// Pull the message out of the terminal result (only the failure variants carry one).
fn message(result: &WorkerResult) -> String {
    match result {
        WorkerResult::FailedToStart(m)
        | WorkerResult::BoundaryFailure(m)
        | WorkerResult::ObservationFailure(m)
        | WorkerResult::OutputFailure(m)
        | WorkerResult::CleanupFailure(m) => m.clone(),
        other => format!("{other:?}"),
    }
}

// Drain TIMES OUT (pending pipe) AND the sink dies on `Exited`. The result is
// ObservationFailure (sink loss dominates the Boundary/Output tier), but the drain
// OutputFailure must be preserved in the message — not silently replaced by a bare
// sink error against a clean `ExitedNormally` base.
#[tokio::test]
async fn drain_timeout_with_sink_failure_on_exited_preserves_output_fault() {
    let boundary = MockBoundary::new().with_pending_stdout();
    let sink = RecordingSink::failing_on_kind("exited");

    let result = tokio::time::timeout(
        OUTER_BOUND,
        run_with(
            child_spec("drain-sink-a", "unused"),
            boundary.boxed(),
            &sink,
        ),
    )
    .await
    .expect("a bounded drain + terminal sink failure must resolve promptly");

    assert_eq!(
        result.kind(),
        "OBSERVATION_FAILURE",
        "a lost sink dominates the drain fault: {result:?}"
    );
    let msg = message(&result);
    assert!(
        msg.contains("underlying fault preserved") && msg.contains("drain"),
        "the drain-timeout OutputFailure must survive in the terminal message: {msg:?}"
    );
    // The sink WAS asked to publish `exited` (it failed on it) — proving the sink loss
    // happened at the terminal observation, not earlier.
    assert!(
        sink.attempted_kinds().contains(&"exited"),
        "attempted: {:?}",
        sink.attempted_kinds()
    );
}

// A late read error is seen ONLY during the post-exit drain AND the sink dies on
// `CleanupCompleted` (the very last observation). Again ObservationFailure dominates,
// but the drain read error must be preserved in the message.
#[tokio::test]
async fn late_drain_read_error_with_sink_failure_on_cleanup_completed_preserves_output_fault() {
    // Leader exits immediately; stdout stays quiet for 120ms (past the exit), then
    // yields a chunk and a fatal read error — so the error is seen in the DRAIN. The
    // sink is healthy through the chunk/`Exited` and fails on `CleanupCompleted`.
    let boundary = MockBoundary::new().with_stdout_error_after_exit(
        vec![b"late-bytes".to_vec()],
        "injected EIO after exit",
        Duration::from_millis(120),
    );
    let sink = RecordingSink::failing_on_kind("cleanup_completed");

    let result = tokio::time::timeout(
        OUTER_BOUND,
        run_with(
            child_spec("drain-sink-b", "unused"),
            boundary.boxed(),
            &sink,
        ),
    )
    .await
    .expect("must resolve promptly");

    assert_eq!(
        result.kind(),
        "OBSERVATION_FAILURE",
        "a lost sink on the final observation dominates: {result:?}"
    );
    let msg = message(&result);
    assert!(
        msg.contains("underlying fault preserved") && msg.contains("injected EIO after exit"),
        "the drain read error must survive in the terminal message: {msg:?}"
    );
    // The bytes read before the error were still delivered before the sink died.
    assert_eq!(sink.stdout(), b"late-bytes");
    assert!(
        sink.attempted_kinds().contains(&"cleanup_completed"),
        "attempted: {:?}",
        sink.attempted_kinds()
    );
}
