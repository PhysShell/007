//! Acceptance 23, 29, 35: drop-triggers-cleanup, two workers stay isolated, and
//! observations arrive in a valid order.
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

// (23) Dropping the handle initiates cancellation + cleanup (observed via join).
#[tokio::test]
async fn drop_handle_initiates_cleanup() {
    let sink = RecordingSink::new();
    let (handle, join) = start(child_spec("r23", "sleep"), &sink);
    // Reach Running first so there is a live process to tear down (and a
    // CleanupCompleted to observe); dropping before spawn is the no-process path.
    tokio::time::sleep(Duration::from_millis(250)).await;
    drop(handle); // no explicit cancel — Drop must request it
    let result = join.join().await;
    assert!(
        matches!(
            result,
            WorkerResult::CancelledGracefully | WorkerResult::CancelledForcefully
        ),
        "got {result:?}"
    );
    assert!(sink.has("cleanup_completed"));
}

// (29) Two workers do not mix output or process ownership.
#[tokio::test]
async fn two_workers_do_not_mix() {
    let mut spec_a = child_spec("A", "print_then_sleep");
    set_env(&mut spec_a, ENV_PAYLOAD, "AAAA-worker-a");
    let mut spec_b = child_spec("B", "print_then_sleep");
    set_env(&mut spec_b, ENV_PAYLOAD, "BBBB-worker-b");

    let sink_a = RecordingSink::new();
    let sink_b = RecordingSink::new();
    let (ha, ja) = start(spec_a, &sink_a);
    let (hb, jb) = start(spec_b, &sink_b);
    tokio::time::sleep(Duration::from_millis(250)).await;
    ha.cancel().await;
    hb.cancel().await;
    ja.join().await;
    jb.join().await;

    assert_eq!(extract_payload(&sink_a.stdout()).unwrap(), b"AAAA-worker-a");
    assert_eq!(extract_payload(&sink_b.stdout()).unwrap(), b"BBBB-worker-b");
    let pa = sink_a.spawned_identity().unwrap().pid;
    let pb = sink_b.spawned_identity().unwrap().pid;
    assert_ne!(pa, pb, "distinct processes");
}

// (35) Observations arrive in a valid order.
#[tokio::test]
async fn observations_are_ordered() {
    let sink = RecordingSink::new();
    run_to_completion(child_spec("r35", "print_stdout"), &sink).await;
    let kinds = sink.kinds();

    assert_eq!(kinds.first(), Some(&"boundary_attested"), "obs: {kinds:?}");
    let pos = |k: &str| kinds.iter().position(|x| *x == k);
    let spawn_requested = pos("spawn_requested").unwrap();
    let spawned = pos("spawned").unwrap();
    let first_output = pos("output").unwrap();
    let exited = pos("exited").unwrap();
    let cleanup = pos("cleanup_completed").unwrap();

    assert!(spawn_requested < spawned, "obs: {kinds:?}");
    assert!(spawned < first_output, "obs: {kinds:?}");
    assert!(exited < cleanup, "obs: {kinds:?}");
    assert_eq!(kinds.last(), Some(&"cleanup_completed"), "obs: {kinds:?}");
}
