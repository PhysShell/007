//! Re-gate blocker: the trailing-drain deadline must NOT override the configured
//! `sink_backpressure_timeout`. The drain bounds only the WAIT for the next
//! pipe/message; a healthy in-flight `ObservationSink::publish()` is governed solely by
//! `sink_backpressure_timeout` and must not be cancelled by the drain. A continuously
//! writing escaped descendant is bounded by a separate message/byte budget.
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

const OUTER_BOUND: Duration = Duration::from_secs(12);

// A trailing chunk whose publish legitimately takes 750ms, under a 2s sink timeout,
// must be delivered — NOT cancelled by the ~500ms drain idle bound. This is the exact
// contract the old code violated (a 500ms drain deadline wrapping the whole publish).
#[tokio::test]
async fn slow_but_within_timeout_trailing_publish_is_delivered_not_drain_cancelled() {
    // Leader exits immediately; a single trailing chunk arrives ~100ms later (during the
    // drain). The sink takes 750ms to publish that chunk; the configured timeout is 2s.
    let boundary = MockBoundary::new()
        .with_trailing_stdout(vec![b"trailing".to_vec()], Duration::from_millis(100));
    let sink = RecordingSink::delaying_on_kind("output", Duration::from_millis(750));

    let mut spec = child_spec("bp-ok", "unused");
    spec.output.sink_backpressure_timeout = Duration::from_secs(2);

    let result = tokio::time::timeout(OUTER_BOUND, run_with(spec, boundary.boxed(), &sink))
        .await
        .expect("must resolve within bound");

    assert_eq!(
        result,
        WorkerResult::ExitedNormally(0),
        "a 750ms publish under a 2s sink timeout must succeed, not be drain-cancelled: {result:?}"
    );
    // The trailing bytes were actually delivered — nothing truncated.
    assert_eq!(sink.stdout(), b"trailing");
}

// A publish SLOWER than its configured timeout is a genuine backpressure failure →
// ObservationFailure (the sink is authoritative), never a silent drop.
#[tokio::test]
async fn trailing_publish_slower_than_sink_timeout_is_observation_failure() {
    let boundary = MockBoundary::new()
        .with_trailing_stdout(vec![b"trailing".to_vec()], Duration::from_millis(50));
    // Sink takes 500ms on output; the configured timeout is only 200ms.
    let sink = RecordingSink::delaying_on_kind("output", Duration::from_millis(500));

    let mut spec = child_spec("bp-slow", "unused");
    spec.output.sink_backpressure_timeout = Duration::from_millis(200);

    let result = tokio::time::timeout(OUTER_BOUND, run_with(spec, boundary.boxed(), &sink))
        .await
        .expect("must resolve within bound");

    assert_eq!(
        result.kind(),
        "OBSERVATION_FAILURE",
        "a publish exceeding its own backpressure timeout must fail closed: {result:?}"
    );
}

// A descendant that keeps WRITING forever (never idle, never EOF) must be bounded by the
// trailing message/byte budget → a bounded OutputFailure, never an infinite drain.
#[tokio::test]
async fn continuously_producing_escaped_stream_is_bounded_output_failure() {
    let boundary = MockBoundary::new().with_infinite_stdout();
    let sink = RecordingSink::new();

    let result = tokio::time::timeout(
        OUTER_BOUND,
        run_with(child_spec("bp-inf", "unused"), boundary.boxed(), &sink),
    )
    .await
    .expect("an endless producer must be bounded by the drain budget, not hang");

    assert_eq!(
        result.kind(),
        "OUTPUT_FAILURE",
        "an endless escaped stream must trip the drain budget: {result:?}"
    );
    assert!(
        !matches!(result, WorkerResult::ExitedNormally(_)),
        "must not read as a clean exit: {result:?}"
    );
}
