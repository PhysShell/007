//! Gate blocker #4: a stdout/stderr read error must be surfaced as a fatal
//! `OutputFailure`, never silently swallowed as EOF. A boundary stream that yields
//! bytes and then an I/O error must not produce a successful terminal result.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod common;

use common::mock::MockBoundary;
use common::*;
use o7_worker::WorkerResult;

#[tokio::test]
async fn stdout_read_error_is_a_fatal_output_failure() {
    // stdout delivers one real chunk, then a hard read error. The leader "stays
    // alive" until force-stopped, so the read error — not a natural exit — is what
    // ends the run.
    let boundary = MockBoundary::new().with_stdout_then_read_error(
        vec![b"partial-output-before-error".to_vec()],
        "injected EIO",
    );
    let sink = RecordingSink::new();

    let result = run_with(child_spec("out-fail", "unused"), boundary.boxed(), &sink).await;

    assert_eq!(result.kind(), "OUTPUT_FAILURE", "got {result:?}");
    assert!(
        !matches!(
            result,
            WorkerResult::ExitedNormally(_) | WorkerResult::ExitedBySignal(_)
        ),
        "lost output must never read as a clean exit: {result:?}"
    );
    // The error text is carried through, not discarded.
    assert!(
        format!("{result:?}").contains("injected EIO"),
        "got {result:?}"
    );

    // The bytes read BEFORE the error were still delivered faithfully.
    assert_eq!(sink.stdout(), b"partial-output-before-error");

    // A live sink is told the supervisor failed.
    assert!(sink.has("supervisor_failed"), "obs: {:?}", sink.kinds());
}
