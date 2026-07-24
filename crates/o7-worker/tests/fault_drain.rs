//! Gate re-gate blocker #3: a read error seen ONLY during the post-exit drain — the
//! leader has already exited, then the reader produces bytes followed by EIO — must
//! still be `OutputFailure`, never `ExitedNormally`. The drain must not discard it.
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

#[tokio::test]
async fn read_error_during_post_exit_drain_is_output_failure() {
    // Leader exits immediately (code 0). stdout stays quiet for 150ms — past the exit
    // — then yields a chunk and a fatal read error, so the error is seen in the DRAIN,
    // not the run loop.
    let boundary = MockBoundary::new().with_stdout_error_after_exit(
        vec![b"late-bytes".to_vec()],
        "injected EIO after exit",
        Duration::from_millis(150),
    );
    let sink = RecordingSink::new();

    let result = run_with(child_spec("drain3", "unused"), boundary.boxed(), &sink).await;

    assert_eq!(
        result.kind(),
        "OUTPUT_FAILURE",
        "leader exited cleanly but output was lost during the drain: {result:?}"
    );
    assert!(
        !matches!(result, WorkerResult::ExitedNormally(_)),
        "must not read as a clean exit: {result:?}"
    );
    // The bytes read before the error are still delivered faithfully.
    assert_eq!(sink.stdout(), b"late-bytes");
    assert!(sink.has("supervisor_failed"), "obs: {:?}", sink.kinds());
}
