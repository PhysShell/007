//! Q-Deck R0.5 "live-readiness" proof, o7d SSE half: the golden synthetic-run
//! transcript (`tests/support`) driven through a REAL socket connect ->
//! partial-receive -> disconnect -> reconnect-with-Last-Event-ID cycle, AND
//! (the part the pre-existing `tests/sse.rs` reconnect test does not cover)
//! an actual restart of the `o7d` SERVER PROCESS itself — not just the
//! client — against the SAME on-disk SQLite file, proving a real daemon
//! restart does not corrupt SSE resume.
//!
//! Invariant for the restriction-lint allowance below: every `unwrap`/
//! `expect`/index here operates on this test's own controlled fixtures (a
//! just-seeded ledger, a frame this test just parsed and is asserting the
//! shape of) — a panic here is this test's own setup failing loudly, not a
//! runtime condition. Matches the precedent in `tests/sse.rs`.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod support;

use std::net::SocketAddr;
use std::path::Path;
use std::time::Duration;

use o7_ledger::SqliteLedger;
use serde_json::Value;
use support::{apply_golden_transcript, outcome_event_type, GoldenOutcome, EXPECTED_EVENT_TYPES};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

fn expected_types(outcome: GoldenOutcome) -> Vec<&'static str> {
    let mut types = EXPECTED_EVENT_TYPES.to_vec();
    let last = types.len() - 1;
    types[last] = outcome_event_type(outcome);
    types
}

/// One parsed SSE frame: `(id, data)`. `id` is `None` for a bare heartbeat
/// comment.
#[derive(Debug, PartialEq, Eq)]
struct SseFrame {
    id: Option<u64>,
    data: Option<String>,
}

fn parse_sse_block(block: &str) -> SseFrame {
    let mut id = None;
    let mut data = None;
    for line in block.lines() {
        if let Some(rest) = line.strip_prefix("id:") {
            id = rest.trim().parse::<u64>().ok();
        } else if let Some(rest) = line.strip_prefix("data:") {
            data = Some(rest.trim().to_owned());
        }
    }
    SseFrame { id, data }
}

async fn spawn_server_on(db_path: &Path) -> (SocketAddr, tokio::task::JoinHandle<()>) {
    let ledger = SqliteLedger::open(db_path).unwrap();
    let app = o7d::router(ledger);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    (addr, handle)
}

async fn connect_and_request(
    addr: SocketAddr,
    path: &str,
    last_event_id: Option<u64>,
) -> tokio::net::TcpStream {
    let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    let mut req = format!("GET {path} HTTP/1.1\r\nHost: localhost\r\n");
    if let Some(id) = last_event_id {
        req.push_str(&format!("Last-Event-ID: {id}\r\n"));
    }
    req.push_str("Connection: keep-alive\r\n\r\n");
    stream.write_all(req.as_bytes()).await.unwrap();

    let mut buf = Vec::new();
    let mut byte = [0_u8; 1];
    loop {
        stream.read_exact(&mut byte).await.unwrap();
        buf.push(byte[0]);
        if buf.ends_with(b"\r\n\r\n") {
            break;
        }
    }
    stream
}

/// Minimal HTTP chunked-transfer-encoding decoder — same approach as
/// `tests/sse.rs`'s (not shared across test-binary crates; see that file's
/// doc comment for the exact caveats this simplified version also has).
fn dechunk(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    loop {
        let Some(nl) = rest.find("\r\n") else {
            if !rest.is_empty() {
                out.push(rest.to_owned());
            }
            break;
        };
        let size_line = &rest[..nl];
        if usize::from_str_radix(size_line.trim(), 16).is_err() {
            out.push(rest.to_owned());
            break;
        }
        let after_size = &rest[nl + 2..];
        if size_line.trim() == "0" {
            break;
        }
        let Some(chunk_end) = after_size.find("\r\n") else {
            out.push(after_size.to_owned());
            break;
        };
        out.push(after_size[..chunk_end].to_owned());
        rest = &after_size[chunk_end + 2..];
    }
    out
}

async fn read_n_data_frames(stream: &mut tokio::net::TcpStream, n: usize) -> Vec<SseFrame> {
    let mut buf = String::new();
    let mut chunk = [0_u8; 4096];
    let mut out = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    while out.len() < n {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        assert!(
            remaining > Duration::ZERO,
            "timed out waiting for {n} SSE data frames"
        );
        let read = tokio::time::timeout(remaining, stream.read(&mut chunk))
            .await
            .expect("did not time out")
            .expect("socket read succeeds");
        assert!(read > 0, "socket closed before {n} frames arrived");
        let text = String::from_utf8_lossy(chunk.get(..read).unwrap_or(&[]));
        for line in dechunk(&text) {
            buf.push_str(&line);
        }
        // Bounded by `out.len() < n` too, not just the outer loop: several
        // events can already be buffered in a single `read()` call (e.g. the
        // whole golden transcript already existed before the client
        // connected, so the first poll finds and emits all of it at once) —
        // draining every complete frame found here unconditionally would
        // silently read past the `n` this test asked for and past the point
        // where it means to simulate a disconnect.
        while out.len() < n {
            let Some(pos) = buf.find("\n\n") else { break };
            let block = buf[..pos].to_owned();
            buf.drain(..pos + 2);
            let parsed = parse_sse_block(&block);
            if parsed.data.is_some() {
                out.push(parsed);
            }
        }
    }
    out
}

#[tokio::test]
async fn transcript_reconnect_with_last_event_id_yields_exactly_the_missed_tail() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("ledger.sqlite3");
    let ledger = SqliteLedger::open(&db_path).unwrap();
    let transcript = apply_golden_transcript(&ledger, GoldenOutcome::Pass)
        .await
        .unwrap();
    drop(ledger); // release the connection before the server opens its own

    let (addr, _handle) = spawn_server_on(&db_path).await;
    let path = format!(
        "/api/v1/conversations/{}/events/stream",
        transcript.conversation.conversation_id.as_str()
    );

    // Connect fresh, receive the first 3 of 7 events, then disconnect for
    // real (an actual TCP close, not just dropping an in-memory value).
    let mut first = connect_and_request(addr, &path, None).await;
    let received = read_n_data_frames(&mut first, 3).await;
    let ids: Vec<u64> = received.iter().map(|f| f.id.unwrap()).collect();
    assert_eq!(ids, vec![1, 2, 3]);
    drop(first);

    // Reconnect with Last-Event-ID: 3 -> must yield exactly 4,5,6,7, no gap,
    // no duplicate of 1-3, in order.
    let mut second = connect_and_request(addr, &path, Some(3)).await;
    let rest = read_n_data_frames(&mut second, 4).await;
    let ids: Vec<u64> = rest.iter().map(|f| f.id.unwrap()).collect();
    assert_eq!(
        ids,
        vec![4, 5, 6, 7],
        "reconnect must yield exactly the missed tail: no gap, no duplicate"
    );

    // Every frame still carries the schema_version the wire contract
    // promises, its event_type in the exact expected order, and the correct
    // run_id — for this specific transcript's data, not a new capability, a
    // confirmation the existing contract holds here too.
    let expected = &expected_types(GoldenOutcome::Pass)[3..];
    for (frame, expected_type) in rest.iter().zip(expected) {
        let data = frame.data.as_ref().unwrap();
        let parsed: Value = serde_json::from_str(data).expect("frame data is valid JSON");
        assert_eq!(parsed["schema_version"], 1);
        assert_eq!(parsed["event_type"], *expected_type);
        assert_eq!(parsed["run_id"], transcript.run.run_id.as_str());
    }
}

#[tokio::test]
async fn transcript_interrupted_outcome_frame_is_run_interrupted_not_run_failed() {
    // The interrupted outcome must travel over SSE as `run.interrupted`, never
    // collapsed into `run.failed` — the same honesty check as the ledger/REST
    // proofs, at the SSE wire boundary.
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("ledger.sqlite3");
    let ledger = SqliteLedger::open(&db_path).unwrap();
    let transcript = apply_golden_transcript(&ledger, GoldenOutcome::Interrupted)
        .await
        .unwrap();
    drop(ledger);

    let (addr, _handle) = spawn_server_on(&db_path).await;
    let path = format!(
        "/api/v1/conversations/{}/events/stream",
        transcript.conversation.conversation_id.as_str()
    );
    let mut client = connect_and_request(addr, &path, None).await;
    let frames = read_n_data_frames(&mut client, 7).await;

    let last_frame = frames.last().expect("7 frames requested");
    let parsed: Value =
        serde_json::from_str(last_frame.data.as_ref().unwrap()).expect("frame data is valid JSON");
    assert_eq!(
        parsed["event_type"],
        outcome_event_type(GoldenOutcome::Interrupted)
    );
    assert_eq!(parsed["event_type"], "run.interrupted");
    assert_eq!(parsed["run_id"], transcript.run.run_id.as_str());
}

#[tokio::test]
async fn transcript_resume_survives_a_real_daemon_restart_against_the_same_db() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("ledger.sqlite3");
    let ledger = SqliteLedger::open(&db_path).unwrap();
    let transcript = apply_golden_transcript(&ledger, GoldenOutcome::Fail)
        .await
        .unwrap();
    drop(ledger);

    let path = format!(
        "/api/v1/conversations/{}/events/stream",
        transcript.conversation.conversation_id.as_str()
    );

    // First "daemon process": connect, receive events 1-4, then this
    // process itself goes away (not just the client) — `.abort()` on the
    // task's own JoinHandle, standing in for the daemon being stopped.
    let (addr_a, handle_a) = spawn_server_on(&db_path).await;
    let mut client = connect_and_request(addr_a, &path, None).await;
    let received = read_n_data_frames(&mut client, 4).await;
    let last_seen = received.last().and_then(|f| f.id).unwrap();
    assert_eq!(last_seen, 4);
    drop(client);
    handle_a.abort();

    // Second "daemon process": a brand-new `SqliteLedger::open` of the SAME
    // file, a brand-new router, a brand-new listener on a new port — this is
    // what `o7d serve --ledger <path>` looks like after a real restart, not
    // a reused in-process value.
    let (addr_b, _handle_b) = spawn_server_on(&db_path).await;
    let mut resumed = connect_and_request(addr_b, &path, Some(last_seen)).await;
    let rest = read_n_data_frames(&mut resumed, 3).await;
    let ids: Vec<u64> = rest.iter().map(|f| f.id.unwrap()).collect();
    assert_eq!(
        ids,
        vec![5, 6, 7],
        "a restarted daemon reading the same db file must resume with no gap and no duplicate"
    );
}
