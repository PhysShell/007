//! Gate blocker #2: policy values that would panic the supervisor AFTER it owns a
//! live process (a zero `mpsc::channel` capacity, a zero `interval`) — or that
//! would let it allocate absurd buffers — must be rejected as `FailedToStart`
//! BEFORE anything is spawned. No spawn is requested, so nothing can leak.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod common;

use std::time::Duration;

use common::*;
use o7_worker::spec::{MAX_CHANNEL_CAPACITY, MAX_CHUNK_BYTES};
use o7_worker::{HeartbeatPolicy, WorkerResult};

/// Assert that `spec` fails to start before spawn — no `SpawnRequested`, no
/// `Spawned`, and definitely not a success.
async fn assert_rejected_before_spawn(spec: o7_worker::WorkerSpec) {
    let sink = RecordingSink::new();
    let result = run_to_completion(spec, &sink).await;

    assert_eq!(result.kind(), "FAILED_TO_START", "got {result:?}");
    assert!(
        !matches!(
            result,
            WorkerResult::ExitedNormally(_) | WorkerResult::ExitedBySignal(_)
        ),
        "an invalid policy must never run: {result:?}"
    );
    let kinds = sink.kinds();
    assert!(
        !kinds.contains(&"spawn_requested"),
        "must fail before requesting spawn: {kinds:?}"
    );
    assert!(
        !kinds.contains(&"spawned"),
        "nothing may be spawned: {kinds:?}"
    );
}

#[tokio::test]
async fn zero_channel_capacity_fails_before_spawn() {
    // `mpsc::channel(0)` would panic the supervisor task after spawn.
    let mut spec = child_spec("cap-zero", "exit0");
    spec.output.channel_capacity = 0;
    assert_rejected_before_spawn(spec).await;
}

#[tokio::test]
async fn oversized_channel_capacity_fails_before_spawn() {
    let mut spec = child_spec("cap-big", "exit0");
    spec.output.channel_capacity = MAX_CHANNEL_CAPACITY + 1;
    assert_rejected_before_spawn(spec).await;
}

#[tokio::test]
async fn zero_chunk_size_fails_before_spawn() {
    let mut spec = child_spec("chunk-zero", "exit0");
    spec.output.max_chunk_bytes = 0;
    assert_rejected_before_spawn(spec).await;
}

#[tokio::test]
async fn oversized_chunk_size_fails_before_spawn() {
    let mut spec = child_spec("chunk-big", "exit0");
    spec.output.max_chunk_bytes = MAX_CHUNK_BYTES + 1;
    assert_rejected_before_spawn(spec).await;
}

#[tokio::test]
async fn zero_heartbeat_interval_fails_before_spawn() {
    // `tokio::time::interval(Duration::ZERO)` would panic the supervisor task.
    let mut spec = child_spec("hb-zero", "exit0");
    spec.heartbeat = HeartbeatPolicy {
        enabled: true,
        interval: Duration::ZERO,
    };
    assert_rejected_before_spawn(spec).await;
}

#[tokio::test]
async fn zero_heartbeat_interval_ok_when_disabled() {
    // A zero interval is harmless when heartbeats are off — the timer is never
    // constructed, so this must still run.
    let mut spec = child_spec("hb-zero-off", "exit0");
    spec.heartbeat = HeartbeatPolicy {
        enabled: false,
        interval: Duration::ZERO,
    };
    let sink = RecordingSink::new();
    let result = run_to_completion(spec, &sink).await;
    assert_eq!(result, WorkerResult::ExitedNormally(0), "got {result:?}");
    assert_eq!(sink.heartbeats(), 0, "no heartbeats when disabled");
}
