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

// An endless ONE-BYTE-per-message producer under a slow-but-within-timeout sink cannot
// reach the byte budget in any reasonable time (that would need hundreds of millions of
// publishes), so the ABSOLUTE total-drain deadline must bound it. The terminal must land
// at approximately `MAX_TRAILING_DRAIN + one sink timeout`, never open-ended.
#[tokio::test]
async fn endless_one_byte_producer_with_slow_sink_is_bounded_by_total_drain_deadline() {
    let boundary = MockBoundary::new().with_infinite_stdout_one_byte();
    // Each output publish takes 20ms — well within the 1s sink timeout, so no publish is
    // ever cancelled; the total-drain deadline is the only thing that can stop this.
    let sink = RecordingSink::delaying_on_kind("output", Duration::from_millis(20));

    let mut spec = child_spec("bp-1byte", "unused");
    spec.output.sink_backpressure_timeout = Duration::from_secs(1);

    let started = std::time::Instant::now();
    let result = tokio::time::timeout(
        o7_worker::supervisor::MAX_TRAILING_DRAIN
            + spec.output.sink_backpressure_timeout
            + Duration::from_secs(3),
        run_with(spec, boundary.boxed(), &sink),
    )
    .await
    .expect("the total-drain deadline must bound a slow endless one-byte producer");
    let elapsed = started.elapsed();

    assert_eq!(
        result.kind(),
        "OUTPUT_FAILURE",
        "the total-drain deadline must fail closed: {result:?}"
    );
    // Bounded by ~ total-drain allowance + one sink timeout (plus scheduling slack).
    assert!(
        elapsed
            <= o7_worker::supervisor::MAX_TRAILING_DRAIN
                + Duration::from_millis(20)
                + Duration::from_secs(2),
        "terminal not bounded by ~total-drain + one sink timeout: {elapsed:?}"
    );
    // And it genuinely reached the deadline (didn't trip the byte budget early): at
    // ~1 byte / 20ms it drained only ~a few hundred bytes, far under the multi-MiB budget.
    assert!(
        elapsed >= o7_worker::supervisor::MAX_TRAILING_DRAIN,
        "must have run until the total-drain deadline: {elapsed:?}"
    );
}

// Trailing output of EXACTLY the byte budget, then EOF, must SUCCEED. Reaching the budget
// is not exceeding it — only strictly more trailing output fails closed. Regression for
// the off-by-one `>=` boundary that rejected legitimate exact-size output before its EOF.
#[tokio::test]
async fn trailing_output_of_exactly_the_budget_then_eof_succeeds() {
    // A small configured buffer keeps the budget close to the fixed pipe allowance.
    let mut spec = child_spec("bp-exact", "unused");
    spec.output.channel_capacity = 1;
    spec.output.max_chunk_bytes = 64 * 1024;
    let budget = spec.output.trailing_drain_byte_budget();

    // Deliver EXACTLY `budget` bytes as a single trailing burst (arriving ~80ms after the
    // leader exit, so it is drained, not consumed in the run loop), then EOF.
    let boundary = MockBoundary::new()
        .with_trailing_stdout(vec![vec![b'x'; budget]], Duration::from_millis(80));
    let sink = RecordingSink::new();

    let result = tokio::time::timeout(OUTER_BOUND, run_with(spec, boundary.boxed(), &sink))
        .await
        .expect("must resolve within bound");

    assert_eq!(
        result,
        WorkerResult::ExitedNormally(0),
        "exactly-budget trailing output then EOF must be a clean exit: {result:?}"
    );
    assert_eq!(sink.stdout().len(), budget, "all budget bytes delivered");
}

// One byte OVER the budget must fail closed — the complement of the exact-budget case.
#[tokio::test]
async fn trailing_output_one_byte_over_the_budget_fails() {
    let mut spec = child_spec("bp-over", "unused");
    spec.output.channel_capacity = 1;
    spec.output.max_chunk_bytes = 64 * 1024;
    let budget = spec.output.trailing_drain_byte_budget();

    let boundary = MockBoundary::new()
        .with_trailing_stdout(vec![vec![b'x'; budget + 1]], Duration::from_millis(80));
    let sink = RecordingSink::new();

    let result = tokio::time::timeout(OUTER_BOUND, run_with(spec, boundary.boxed(), &sink))
        .await
        .expect("must resolve within bound");

    assert_eq!(
        result.kind(),
        "OUTPUT_FAILURE",
        "trailing output exceeding the budget must fail closed: {result:?}"
    );
}
