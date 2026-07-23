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

// A descendant that keeps WRITING forever, under an EXPLICIT `max_trailing_bytes` cap,
// must trip that cap → a bounded OutputFailure, never an infinite drain.
#[tokio::test]
async fn continuously_producing_escaped_stream_trips_explicit_byte_cap() {
    let boundary = MockBoundary::new().with_infinite_stdout();
    let sink = RecordingSink::new();

    let mut spec = child_spec("bp-inf", "unused");
    spec.output.max_trailing_bytes = Some(256 * 1024); // deliberate 256 KiB ceiling

    let result = tokio::time::timeout(OUTER_BOUND, run_with(spec, boundary.boxed(), &sink))
        .await
        .expect("an endless producer under an explicit cap must be bounded, not hang");

    assert_eq!(
        result.kind(),
        "OUTPUT_FAILURE",
        "an endless escaped stream must trip the explicit byte cap: {result:?}"
    );
    assert!(
        !matches!(result, WorkerResult::ExitedNormally(_)),
        "must not read as a clean exit: {result:?}"
    );
}

// Blocker regression: a LEGITIMATE finite stream far LARGER than the previously-inferred
// budget (channel_capacity*max_chunk_bytes + 2 MiB), followed by EOF, must be a CLEAN
// exit under the default policy — it is NOT an endless escaped writer. The old code
// wrongly failed this because the finite data exceeded a hidden capacity assumption.
#[tokio::test]
async fn large_finite_trailing_stream_then_eof_is_clean_under_default_policy() {
    // Under the OLD inferred budget this config gave 16 MiB + 2 MiB = 18 MiB; a 20 MiB
    // finite stream exceeded it and was misclassified. Default policy has NO byte cap.
    let mut spec = child_spec("bp-finite-big", "unused");
    spec.output.channel_capacity = 1;
    spec.output.max_chunk_bytes = 16 * 1024 * 1024;
    assert_eq!(
        spec.output.max_trailing_bytes, None,
        "default: no inferred cap"
    );

    let twenty_mib = 20 * 1024 * 1024;
    let boundary = MockBoundary::new()
        .with_trailing_stdout(vec![vec![b'x'; twenty_mib]], Duration::from_millis(80));
    let sink = RecordingSink::new();

    let result = tokio::time::timeout(OUTER_BOUND, run_with(spec, boundary.boxed(), &sink))
        .await
        .expect("must resolve within bound");

    assert_eq!(
        result,
        WorkerResult::ExitedNormally(0),
        "a large finite trailing stream + EOF must be a clean exit, not an escaped writer: {result:?}"
    );
    assert_eq!(
        sink.stdout().len(),
        twenty_mib,
        "all finite bytes delivered"
    );
}

// Exact total-drain wall-clock: an endless ONE-BYTE-per-message producer (no explicit
// byte cap) under a slow-but-within-timeout sink is bounded ONLY by the absolute
// total-drain deadline. Because the per-message recv wait is capped by the REMAINING
// total time (not a full idle timeout on top), the terminal lands within
// `MAX_TRAILING_DRAIN + one sink timeout` — proving the tightened receive bound.
#[tokio::test]
async fn endless_one_byte_producer_bounded_exactly_by_total_drain_plus_one_sink_timeout() {
    let boundary = MockBoundary::new().with_infinite_stdout_one_byte();
    // Each output publish takes 20ms — well within the 1s sink timeout, so no publish is
    // ever cancelled; the total-drain deadline is the only thing that can stop this.
    let sink = RecordingSink::delaying_on_kind("output", Duration::from_millis(20));

    let mut spec = child_spec("bp-1byte", "unused");
    spec.output.sink_backpressure_timeout = Duration::from_secs(1);
    // No byte cap: the time deadline is the sole bound under test.
    assert_eq!(spec.output.max_trailing_bytes, None);

    let max_drain = o7_worker::supervisor::MAX_TRAILING_DRAIN;
    let sink_timeout = spec.output.sink_backpressure_timeout;

    let started = std::time::Instant::now();
    let result = tokio::time::timeout(
        max_drain + sink_timeout + Duration::from_secs(3),
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
    // Genuinely ran until the deadline...
    assert!(
        elapsed >= max_drain,
        "must reach the total-drain deadline: {elapsed:?}"
    );
    // ...and did NOT overshoot by a full extra idle timeout: the ceiling is
    // MAX_TRAILING_DRAIN + one sink timeout (+ scheduling/spawn slack), NOT + idle timeout.
    assert!(
        elapsed <= max_drain + sink_timeout + Duration::from_secs(1),
        "terminal not bounded by ~total-drain + one sink timeout: {elapsed:?}"
    );
}

// Trailing output of EXACTLY the explicit cap, then EOF, must SUCCEED and deliver exactly
// `cap` bytes. Reaching the cap is not exceeding it.
#[tokio::test]
async fn trailing_output_of_exactly_the_cap_then_eof_succeeds() {
    let cap = 1000;
    let mut spec = child_spec("bp-exact", "unused");
    spec.output.max_trailing_bytes = Some(cap);
    // `max_chunk_bytes` >= the chunk, so it is ONE atomic OutputChunk (not split).
    spec.output.max_chunk_bytes = 64 * 1024;

    let boundary =
        MockBoundary::new().with_trailing_stdout(vec![vec![b'x'; cap]], Duration::from_millis(80));
    let sink = RecordingSink::new();

    let result = tokio::time::timeout(OUTER_BOUND, run_with(spec, boundary.boxed(), &sink))
        .await
        .expect("must resolve within bound");

    assert_eq!(
        result,
        WorkerResult::ExitedNormally(0),
        "exactly-cap trailing output then EOF must be a clean exit: {result:?}"
    );
    assert_eq!(sink.stdout().len(), cap, "exactly cap bytes delivered");
}

// A single oversized chunk (cap+1 as ONE atomic OutputChunk) must fail closed AND deliver
// ZERO bytes from it — the cap is ENFORCED before publish, not a post-hoc alarm.
#[tokio::test]
async fn single_oversized_chunk_is_withheld_entirely() {
    let cap = 1000;
    let mut spec = child_spec("bp-over", "unused");
    spec.output.max_trailing_bytes = Some(cap);
    spec.output.max_chunk_bytes = 64 * 1024; // the whole 1500-byte chunk is one OutputChunk

    let boundary =
        MockBoundary::new().with_trailing_stdout(vec![vec![b'x'; 1500]], Duration::from_millis(80));
    let sink = RecordingSink::new();

    let result = tokio::time::timeout(OUTER_BOUND, run_with(spec, boundary.boxed(), &sink))
        .await
        .expect("must resolve within bound");

    assert_eq!(
        result.kind(),
        "OUTPUT_FAILURE",
        "an over-cap chunk must fail closed: {result:?}"
    );
    assert_eq!(
        sink.stdout().len(),
        0,
        "the over-cap chunk must NOT be published (zero bytes delivered from it)"
    );
}

// Several chunks where the LAST crosses the cap: earlier chunks remain, the crossing
// chunk is withheld entirely.
#[tokio::test]
async fn crossing_chunk_is_withheld_earlier_chunks_remain() {
    let cap = 1000;
    let mut spec = child_spec("bp-cross", "unused");
    spec.output.max_trailing_bytes = Some(cap);
    spec.output.max_chunk_bytes = 64 * 1024; // each 600-byte Vec is one atomic OutputChunk

    // 600 (ok, total 600) then 600 (would be 1200 > 1000 → withheld).
    let boundary = MockBoundary::new().with_trailing_stdout(
        vec![vec![b'a'; 600], vec![b'b'; 600]],
        Duration::from_millis(80),
    );
    let sink = RecordingSink::new();

    let result = tokio::time::timeout(OUTER_BOUND, run_with(spec, boundary.boxed(), &sink))
        .await
        .expect("must resolve within bound");

    assert_eq!(
        result.kind(),
        "OUTPUT_FAILURE",
        "the crossing chunk must fail closed: {result:?}"
    );
    // Only the first 600-byte chunk was delivered; the crossing chunk was withheld whole.
    assert_eq!(
        sink.stdout(),
        vec![b'a'; 600],
        "earlier chunk kept, crossing chunk withheld entirely"
    );
}
