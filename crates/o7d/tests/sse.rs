//! SSE stream tests. Two styles, matched to what each needs to prove:
//!
//! - initial replay + cursor precedence: drive the router in-process via
//!   `tower::ServiceExt::oneshot` and read a bounded number of body frames —
//!   no real socket needed, and reading only N frames then dropping the body
//!   IS a client going away, so this doubles as the "stops without panicking
//!   on early drop" proof.
//! - the required reconnect transcript: a REAL socket, because "disconnect"
//!   has to mean an actual TCP close, not just dropping an in-memory value.
//!   Written by hand over `tokio::net::TcpStream` — no HTTP client crate.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use axum::body::Body;
use axum::http::Request;
use http_body_util::BodyExt;
use o7_ledger::{Ledger, NewEvent, SqliteLedger};
use std::net::SocketAddr;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tower::ServiceExt;

/// One parsed SSE frame: `(id, data)`. `id` is `None` for a bare heartbeat
/// comment (those carry no `id:`/`data:` line at all).
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

/// Read frames from an in-process router response body until `n` *data*
/// frames (heartbeats don't count) have been parsed, bounded by a timeout so
/// a bug that stops emitting anything fails the test instead of hanging it.
async fn collect_data_frames(body: Body, n: usize) -> Vec<SseFrame> {
    let mut body = body;
    let mut buf = String::new();
    let mut out = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    while out.len() < n {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        assert!(
            remaining > Duration::ZERO,
            "timed out waiting for {n} SSE data frames"
        );
        let frame = tokio::time::timeout(remaining, body.frame())
            .await
            .expect("did not time out")
            .expect("body has more frames")
            .expect("frame is not an error");
        if let Ok(bytes) = frame.into_data() {
            buf.push_str(&String::from_utf8_lossy(&bytes));
        }
        while let Some(pos) = buf.find("\n\n") {
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

async fn seeded_router_with_events(n: u64) -> (axum::Router, o7_ledger::ConversationId) {
    let dir = tempfile::tempdir().unwrap();
    let ledger = SqliteLedger::open(dir.path().join("ledger.sqlite3")).unwrap();
    std::mem::forget(dir);
    let conv = ledger.create_conversation(None).await.unwrap();
    for i in 0..n {
        ledger
            .append_user_message(
                conv.conversation_id.clone(),
                serde_json::json!({ "i": i }),
                None,
                None,
            )
            .await
            .unwrap();
    }
    let router = o7d::router(ledger);
    (router, conv.conversation_id)
}

#[tokio::test]
async fn stream_replays_existing_events_in_order() {
    let (router, conv_id) = seeded_router_with_events(3).await;
    // conversation.created (seq 1) + 3 user messages (seq 2,3,4).
    let req = Request::builder()
        .uri(format!("/api/v1/conversations/{conv_id}/events/stream"))
        .body(Body::empty())
        .unwrap();
    let resp = router.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), 200);
    let frames = collect_data_frames(resp.into_body(), 4).await;
    let ids: Vec<u64> = frames.iter().map(|f| f.id.unwrap()).collect();
    assert_eq!(
        ids,
        vec![1, 2, 3, 4],
        "initial replay must be gapless and in order"
    );
}

#[tokio::test]
async fn after_query_param_resumes_from_that_cursor() {
    let (router, conv_id) = seeded_router_with_events(3).await;
    let req = Request::builder()
        .uri(format!(
            "/api/v1/conversations/{conv_id}/events/stream?after=2"
        ))
        .body(Body::empty())
        .unwrap();
    let resp = router.oneshot(req).await.unwrap();
    let frames = collect_data_frames(resp.into_body(), 2).await;
    let ids: Vec<u64> = frames.iter().map(|f| f.id.unwrap()).collect();
    assert_eq!(ids, vec![3, 4]);
}

#[tokio::test]
async fn last_event_id_header_wins_over_after_query_param() {
    let (router, conv_id) = seeded_router_with_events(3).await;
    let req = Request::builder()
        .uri(format!(
            "/api/v1/conversations/{conv_id}/events/stream?after=1"
        ))
        .header("last-event-id", "3")
        .body(Body::empty())
        .unwrap();
    let resp = router.oneshot(req).await.unwrap();
    // If `after=1` had won, the first id would be 2. Last-Event-ID: 3 must win.
    let frames = collect_data_frames(resp.into_body(), 1).await;
    assert_eq!(
        frames.first().and_then(|f| f.id),
        Some(4),
        "Last-Event-ID must take precedence over ?after="
    );
}

// --- real-socket reconnect transcript -------------------------------------

async fn spawn_server(ledger: SqliteLedger) -> SocketAddr {
    let app = o7d::router(ledger);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    addr
}

/// Connect, send a minimal HTTP/1.1 GET (optionally with `Last-Event-ID`),
/// and return the open socket positioned right after the HTTP response
/// headers — ready to read SSE frames off.
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

    // Consume the HTTP response headers (up to the blank line) so the caller
    // starts reading exactly at the SSE body.
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

/// Read `n` SSE data frames (skipping heartbeat comments) off an open socket.
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
        // HTTP/1.1 chunked encoding wraps each write in `<hex-size>\r\n...\r\n`;
        // axum::serve uses chunked transfer for a streaming body with no
        // known length. Strip the chunk framing before parsing SSE blocks.
        let text = String::from_utf8_lossy(chunk.get(..read).unwrap_or(&[]));
        for line in dechunk(&text) {
            buf.push_str(&line);
        }
        while let Some(pos) = buf.find("\n\n") {
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

/// Minimal HTTP chunked-transfer-encoding decoder: strips `<hex-size>\r\n`
/// framing lines, passing the actual payload bytes through. Not a general
/// chunked-decoder (doesn't handle chunks split across reads) — sufficient
/// here because SSE frames are small and each `read()` above reliably lands
/// on a chunk boundary at this payload size in practice; a real client would
/// use a proper HTTP library, which is exactly what production `EventSource`
/// implementations already do.
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
            // Not a chunk-size line (already inside a chunk's payload, or a
            // trailing partial line) — pass the remainder through verbatim.
            out.push(rest.to_owned());
            break;
        }
        let after_size = &rest[nl + 2..];
        // Chunk-size 0 marks end of the body; nothing meaningful follows.
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

#[tokio::test]
async fn reconnect_with_last_event_id_receives_exactly_the_missed_events() {
    let dir = tempfile::tempdir().unwrap();
    let ledger = SqliteLedger::open(dir.path().join("ledger.sqlite3")).unwrap();
    std::mem::forget(dir);
    let conv = ledger.create_conversation(None).await.unwrap();
    let addr = spawn_server(ledger.clone()).await;

    // connect -> receive sequence N (N = 1, the conversation.created event).
    let mut first = connect_and_request(
        addr,
        &format!(
            "/api/v1/conversations/{}/events/stream",
            conv.conversation_id.as_str()
        ),
        None,
    )
    .await;
    let received = read_n_data_frames(&mut first, 1).await;
    let n = received
        .first()
        .and_then(|f| f.id)
        .expect("first frame carries a sequence id");
    assert_eq!(n, 1);

    // disconnect: close the socket for real.
    drop(first);

    // append N+1, N+2 while no client is connected.
    for i in 0..2 {
        ledger
            .append_user_message(
                conv.conversation_id.clone(),
                serde_json::json!({ "i": i }),
                None,
                None,
            )
            .await
            .unwrap();
    }

    // reconnect with Last-Event-ID: N -> receive exactly N+1, N+2.
    let mut second = connect_and_request(
        addr,
        &format!(
            "/api/v1/conversations/{}/events/stream",
            conv.conversation_id.as_str()
        ),
        Some(n),
    )
    .await;
    let after_reconnect = read_n_data_frames(&mut second, 2).await;
    let ids: Vec<u64> = after_reconnect.iter().map(|f| f.id.unwrap()).collect();
    assert_eq!(
        ids,
        vec![n + 1, n + 2],
        "reconnect must yield exactly the missed events, no dup, no gap"
    );
}

#[tokio::test]
async fn appending_after_client_disconnect_does_not_panic_the_server() {
    let dir = tempfile::tempdir().unwrap();
    let ledger = SqliteLedger::open(dir.path().join("ledger.sqlite3")).unwrap();
    std::mem::forget(dir);
    let conv = ledger.create_conversation(None).await.unwrap();
    let addr = spawn_server(ledger.clone()).await;

    let mut client = connect_and_request(
        addr,
        &format!(
            "/api/v1/conversations/{}/events/stream",
            conv.conversation_id.as_str()
        ),
        None,
    )
    .await;
    let _ = read_n_data_frames(&mut client, 1).await;
    drop(client);

    // Give the server a moment to notice the disconnect and stop polling,
    // then append — if the disconnected stream's poll loop were somehow
    // still holding the ledger or panicking, this append would hang or the
    // health check below would fail.
    tokio::time::sleep(Duration::from_millis(200)).await;
    ledger
        .append_event(NewEvent {
            event_id: o7_ledger::EventId::generate(),
            conversation_id: conv.conversation_id.clone(),
            run_id: None,
            attempt_id: None,
            event_type: o7_ledger::EventType::SystemNote,
            schema_version: 1,
            payload: serde_json::json!({}),
        })
        .await
        .unwrap();

    let mut health = tokio::net::TcpStream::connect(addr).await.unwrap();
    health
        .write_all(b"GET /api/v1/health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .await
        .unwrap();
    let mut resp = Vec::new();
    health.read_to_end(&mut resp).await.unwrap();
    assert!(
        String::from_utf8_lossy(&resp).starts_with("HTTP/1.1 200"),
        "server must still be alive and serving after a client disconnected mid-stream"
    );
}
