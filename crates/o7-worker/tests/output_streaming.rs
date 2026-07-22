//! Acceptance 10-14: independent byte streams, non-UTF-8, trailing output, bounds.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod common;

use std::time::Duration;

use common::*;
use o7_worker::OutputPolicy;

// (10) stdout is streamed in chunks with nothing lost or reordered.
#[tokio::test]
async fn stdout_streams_without_loss() {
    let sink = RecordingSink::new();
    run_to_completion(child_spec("out-large", "print_large"), &sink).await;
    let payload = extract_payload(&sink.stdout()).expect("payload");
    assert_eq!(payload, large_pattern(LARGE_LEN));
    // It really was split into multiple chunks.
    assert!(sink.count("output") >= 2, "expected multiple chunks");
}

// (11) stderr is streamed independently of stdout.
#[tokio::test]
async fn stderr_is_independent() {
    let sink = RecordingSink::new();
    run_to_completion(child_spec("both", "print_both"), &sink).await;
    assert_eq!(extract_payload(&sink.stdout()).unwrap(), b"stdout-side");
    assert_eq!(extract_payload(&sink.stderr()).unwrap(), b"stderr-side");
}

// (12) Non-UTF-8 bytes are preserved verbatim.
#[tokio::test]
async fn non_utf8_output_is_preserved() {
    let sink = RecordingSink::new();
    run_to_completion(child_spec("nonutf8", "print_nonutf8"), &sink).await;
    assert_eq!(extract_payload(&sink.stdout()).unwrap(), NON_UTF8);
}

// (13) The last output produced right before exit is read.
#[tokio::test]
async fn trailing_output_before_exit_is_read() {
    let sink = RecordingSink::new();
    let result = run_to_completion(child_spec("trailing", "print_stdout"), &sink).await;
    assert_eq!(result.kind(), "EXITED_NORMALLY");
    assert_eq!(extract_payload(&sink.stdout()).unwrap(), b"default-payload");
}

// (14) A tiny bounded channel still delivers all output (backpressure, no loss,
// bounded memory).
#[tokio::test]
async fn output_channel_is_bounded_and_lossless() {
    let mut spec = child_spec("bounded", "print_large");
    spec.output = OutputPolicy {
        max_chunk_bytes: 16,
        channel_capacity: 1,
        sink_backpressure_timeout: Duration::from_secs(30),
        max_trailing_bytes: None,
    };
    let sink = RecordingSink::new();
    let result = run_to_completion(spec, &sink).await;
    assert_eq!(result.kind(), "EXITED_NORMALLY");
    assert_eq!(
        extract_payload(&sink.stdout()).unwrap(),
        large_pattern(LARGE_LEN)
    );
}
