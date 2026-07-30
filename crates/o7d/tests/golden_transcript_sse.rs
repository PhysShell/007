//! Q-Deck R0.5 "live-readiness" proof, o7d SSE half: the golden synthetic-run
//! transcript (`tests/support`) driven through a REAL socket connect ->
//! partial-receive -> disconnect -> reconnect-with-Last-Event-ID cycle, AND
//! (the part the pre-existing `tests/sse.rs` reconnect test does not cover)
//! an actual restart of the `o7d` SERVER **PROCESS** itself — a genuinely
//! separate OS process (the real compiled `o7d` binary via
//! `CARGO_BIN_EXE_o7d`, killed and reaped, then respawned fresh), not a
//! tokio task inside this test's own process — against the SAME on-disk
//! SQLite file, proving a real daemon restart does not corrupt SSE resume.
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

use std::io::{BufRead, BufReader};
use std::net::SocketAddr;
use std::path::Path;
use std::process::{Child, Command, Stdio};
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

/// Spawn the REAL, compiled `o7d` binary as its own OS process — a genuine
/// process, not a task inside THIS test's own process.
/// `env!("CARGO_BIN_EXE_o7d")` is cargo's own guarantee that this crate's
/// `o7d` binary target is built before its integration tests run, and points
/// at that exact executable.
///
/// `--listen 127.0.0.1:0` asks the OS for any free port; the actual bound
/// port is learned by reading o7d's own startup line off its stderr pipe
/// (`main.rs` reports `listener.local_addr()`, not the raw `--listen`
/// argument, specifically so this works). The rest of stderr is drained for
/// the process's whole lifetime in a background thread — see below for why
/// that matters.
fn spawn_o7d_process(db_path: &Path) -> (Child, SocketAddr) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_o7d"))
        .args([
            "serve",
            "--ledger",
            db_path.to_str().expect("utf8 path"),
            "--listen",
            "127.0.0.1:0",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn the real o7d binary");
    let stderr = child.stderr.take().expect("piped stderr");
    let mut reader = BufReader::new(stderr);
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .expect("read o7d's startup line");
    let addr_str = line
        .trim()
        .strip_prefix("o7d: listening on http://")
        .expect("startup line reports the bound address");
    let addr: SocketAddr = addr_str.parse().expect("valid socket addr");

    // Keep draining stderr for the rest of the process's life, in a plain
    // OS thread (not a tokio task — this is deliberately independent of the
    // async runtime and just needs to keep running for the child's
    // lifetime). Dropping the read end here instead would leave the child
    // writing into a pipe with nothing on the other end; `eprintln!` PANICS
    // on any write failure (including the EPIPE that produces), which was
    // silently killing the child moments after startup — a real bug in this
    // test harness, not in `o7d` itself, found while making this test spawn
    // a genuine subprocess.
    std::thread::spawn(move || {
        let mut discarded = String::new();
        while reader.read_line(&mut discarded).unwrap_or(0) > 0 {
            discarded.clear();
        }
    });

    (child, addr)
}

/// Poll a REAL `o7d` process's health endpoint until it answers, or panic
/// after a generous timeout. The bound TCP port is already listening by the
/// time `spawn_o7d_process` returns (the kernel accepts the SYN as soon as
/// `bind`+`listen` has happened, before axum's own accept loop necessarily
/// gets scheduled) — but a fresh connection can still race axum's userspace
/// accept loop actually servicing it, especially on a loaded machine. A real
/// client reasonably retries a brand-new connection instead of assuming the
/// first attempt always lands; this is that retry, not a workaround for
/// something specific to this test harness.
async fn wait_until_healthy(addr: SocketAddr) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        assert!(
            tokio::time::Instant::now() <= deadline,
            "o7d process never became reachable at {addr}"
        );
        if let Ok(mut stream) = tokio::net::TcpStream::connect(addr).await {
            let wrote = stream
                .write_all(
                    b"GET /api/v1/health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
                )
                .await;
            if wrote.is_ok() {
                let mut buf = [0_u8; 64];
                if matches!(stream.read(&mut buf).await, Ok(n) if n > 0) {
                    return;
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
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
        assert_eq!(parsed["schema_version"], o7d::dto::API_SCHEMA_VERSION);
        assert_eq!(parsed["event_type"], *expected_type);
        assert_eq!(parsed["run_id"], transcript.run.run_id.as_str());
    }
}

#[tokio::test]
async fn transcript_interrupted_outcome_frame_is_run_interrupted_not_run_failed() {
    // The interrupted outcome must travel over SSE as `run.interrupted`,
    // never collapsed into `run.failed` — the same honesty check as the
    // ledger/REST proofs, at the SSE wire boundary. Not a "terminal frame":
    // interrupted is a resumable pause, not a sealed outcome (see the
    // resume regression below and in golden_transcript_rest.rs).
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
async fn every_outcomes_terminal_frame_carries_the_right_event_type_over_sse() {
    for outcome in [
        GoldenOutcome::Pass,
        GoldenOutcome::Fail,
        GoldenOutcome::Interrupted,
        GoldenOutcome::Blocked,
        GoldenOutcome::Error,
    ] {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("ledger.sqlite3");
        let ledger = SqliteLedger::open(&db_path).unwrap();
        let transcript = apply_golden_transcript(&ledger, outcome).await.unwrap();
        drop(ledger);

        let (addr, _handle) = spawn_server_on(&db_path).await;
        let path = format!(
            "/api/v1/conversations/{}/events/stream",
            transcript.conversation.conversation_id.as_str()
        );
        let mut client = connect_and_request(addr, &path, None).await;
        let frames = read_n_data_frames(&mut client, 7).await;

        let last_frame = frames.last().expect("7 frames requested");
        let parsed: Value = serde_json::from_str(last_frame.data.as_ref().unwrap())
            .expect("frame data is valid JSON");
        assert_eq!(
            parsed["event_type"],
            outcome_event_type(outcome),
            "outcome={outcome:?}"
        );
        assert_eq!(parsed["run_id"], transcript.run.run_id.as_str());
    }
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

    // First REAL `o7d` process (its own OS process, a genuinely different
    // PID from this test): connect, receive events 1-4, then the process
    // itself is killed and reaped — not just the client disconnecting.
    let (mut child_a, addr_a) = spawn_o7d_process(&db_path);
    wait_until_healthy(addr_a).await;
    let mut client = connect_and_request(addr_a, &path, None).await;
    let received = read_n_data_frames(&mut client, 4).await;
    let last_seen = received.last().and_then(|f| f.id).unwrap();
    assert_eq!(last_seen, 4);
    drop(client);
    child_a.kill().expect("kill the first o7d process");
    child_a.wait().expect("reap the killed o7d process");

    // Second REAL `o7d` process: a brand-new OS process (a new PID),
    // opening the SAME on-disk file fresh — this is what `o7d serve
    // --ledger <path>` actually looks like after a restart, not a reused
    // in-process task or connection.
    let (mut child_b, addr_b) = spawn_o7d_process(&db_path);
    wait_until_healthy(addr_b).await;
    let mut resumed = connect_and_request(addr_b, &path, Some(last_seen)).await;
    let rest = read_n_data_frames(&mut resumed, 3).await;
    let ids: Vec<u64> = rest.iter().map(|f| f.id.unwrap()).collect();
    assert_eq!(
        ids,
        vec![5, 6, 7],
        "a restarted daemon PROCESS reading the same db file must resume with no gap and no duplicate"
    );
    child_b.kill().expect("kill the second o7d process");
    child_b.wait().expect("reap the second o7d process");
}
