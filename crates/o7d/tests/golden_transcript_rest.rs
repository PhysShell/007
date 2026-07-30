//! Q-Deck R0.5 "live-readiness" proof, o7d REST half: seed a real SQLite
//! database with the canonical golden synthetic-run transcript
//! (`tests/support`) through `o7-ledger`'s production write API, then drive
//! the REAL `o7d::router` over it exactly as a real client would — proving
//! the downstream REST contract Q-Deck depends on holds for this transcript,
//! not just for the smaller ad hoc fixtures the pre-existing `tests/api.rs`
//! suite already covers.
//!
//! Invariant for the restriction-lint allowance below: every `unwrap`/
//! `expect`/JSON-field index here is on a response this same test just
//! constructed and asserted the status of — a controlled fixture, not
//! runtime input. A panic means this test's own setup broke, which should
//! fail the test loudly. Matches the precedent in `tests/api.rs`.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod support;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use http_body_util::BodyExt;
use o7_ledger::SqliteLedger;
use serde_json::Value;
use support::{
    apply_golden_transcript, terminal_event_type, GoldenOutcome, GoldenTranscript,
    EXPECTED_EVENT_TYPES,
};
use tower::ServiceExt;

fn expected_types(outcome: GoldenOutcome) -> Vec<&'static str> {
    let mut types = EXPECTED_EVENT_TYPES.to_vec();
    let last = types.len() - 1;
    types[last] = terminal_event_type(outcome);
    types
}

async fn seeded_router_with_transcript(outcome: GoldenOutcome) -> (Router, GoldenTranscript) {
    let dir = tempfile::tempdir().unwrap();
    let ledger = SqliteLedger::open(dir.path().join("ledger.sqlite3")).unwrap();
    std::mem::forget(dir); // the file must outlive this function's stack frame
    let transcript = apply_golden_transcript(&ledger, outcome).await.unwrap();
    let router = o7d::router(ledger);
    (router, transcript)
}

async fn get(router: &Router, uri: &str) -> (StatusCode, Value) {
    let req = Request::builder()
        .uri(uri)
        .body(Body::empty())
        .expect("valid request");
    let resp = router
        .clone()
        .oneshot(req)
        .await
        .expect("router does not fail");
    let status = resp.status();
    let bytes = resp
        .into_body()
        .collect()
        .await
        .expect("readable body")
        .to_bytes();
    let body: Value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or_else(|e| {
            panic!(
                "response body is not valid JSON: {e}: {:?}",
                String::from_utf8_lossy(&bytes)
            )
        })
    };
    (status, body)
}

#[tokio::test]
async fn transcript_run_is_listed() {
    let (router, transcript) = seeded_router_with_transcript(GoldenOutcome::Pass).await;
    let (status, page) = get(&router, "/api/v1/runs").await;
    assert_eq!(status, StatusCode::OK);
    let items = page["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["run_id"], transcript.run.run_id.as_str());
    assert_eq!(items[0]["agent"], "claude");
    assert_eq!(items[0]["role"], "implementer");
}

#[tokio::test]
async fn transcript_run_is_reachable_by_id_with_terminal_status_and_timestamps() {
    let (router, transcript) = seeded_router_with_transcript(GoldenOutcome::Pass).await;
    let (status, run) = get(
        &router,
        &format!("/api/v1/runs/{}", transcript.run.run_id.as_str()),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(run["status"], "completed");
    assert_eq!(run["created_at"], transcript.run.created_at);
    assert_eq!(run["finished_at"], transcript.run.finished_at.unwrap());
}

#[tokio::test]
async fn transcript_error_outcome_run_has_no_finished_at_over_the_wire() {
    // Same honesty check as the ledger-side proof, at the REST boundary:
    // ERROR (interrupted) is resumable, not closed, so `finished_at` must
    // travel as `null`, not be backfilled into looking like a closed run.
    let (router, transcript) = seeded_router_with_transcript(GoldenOutcome::Error).await;
    let (status, run) = get(
        &router,
        &format!("/api/v1/runs/{}", transcript.run.run_id.as_str()),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(run["status"], "interrupted");
    assert!(run["finished_at"].is_null());
}

#[tokio::test]
async fn transcript_conversation_is_reachable_by_id() {
    let (router, transcript) = seeded_router_with_transcript(GoldenOutcome::Pass).await;
    let (status, conv) = get(
        &router,
        &format!(
            "/api/v1/conversations/{}",
            transcript.conversation.conversation_id.as_str()
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        conv["conversation_id"],
        transcript.conversation.conversation_id.as_str()
    );
    assert_eq!(conv["status"], "open");
}

#[tokio::test]
async fn transcript_events_are_available_in_order() {
    let (router, transcript) = seeded_router_with_transcript(GoldenOutcome::Fail).await;
    let (status, page) = get(
        &router,
        &format!(
            "/api/v1/conversations/{}/events",
            transcript.conversation.conversation_id.as_str()
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let items = page["items"].as_array().unwrap();
    let expected = expected_types(GoldenOutcome::Fail);
    assert_eq!(items.len(), expected.len());
    for (i, event_type) in expected.iter().enumerate() {
        assert_eq!(items[i]["sequence"], i as u64 + 1);
        assert_eq!(items[i]["event_type"], *event_type);
    }
}

#[tokio::test]
async fn transcript_events_pagination_loses_nothing() {
    let (router, transcript) = seeded_router_with_transcript(GoldenOutcome::Pass).await;
    let conv_id = transcript.conversation.conversation_id.as_str();

    // A limit smaller than the transcript's 7 events forces at least two
    // pages, exercising the exact `next_after` cursor Q-Deck's own
    // pagination loops (ConversationPage.svelte / eventStream.svelte.ts)
    // depend on.
    let mut collected_types: Vec<String> = Vec::new();
    let mut after: Option<u64> = None;
    loop {
        let uri = match after {
            Some(cursor) => {
                format!("/api/v1/conversations/{conv_id}/events?limit=3&after={cursor}")
            }
            None => format!("/api/v1/conversations/{conv_id}/events?limit=3"),
        };
        let (status, page) = get(&router, &uri).await;
        assert_eq!(status, StatusCode::OK);
        let items = page["items"].as_array().unwrap();
        if items.is_empty() {
            break;
        }
        for item in items {
            collected_types.push(item["event_type"].as_str().unwrap().to_owned());
        }
        after = page["next_after"].as_u64();
        if after.is_none() {
            break;
        }
    }

    assert_eq!(
        collected_types,
        expected_types(GoldenOutcome::Pass),
        "paginating with next_after must lose no event and preserve order"
    );
}

#[tokio::test]
async fn transcript_adjacent_unknown_ids_still_404() {
    // Confidence check specific to this fixture: an id that is NOT the one
    // the transcript created (even though it shares the same UUID shape and
    // sits right next to a real one in the same database) must still 404 —
    // the golden transcript's presence must not loosen the existing 404
    // contract for anything it did not itself create.
    let (router, transcript) = seeded_router_with_transcript(GoldenOutcome::Pass).await;
    let unknown_run = format!("{}-does-not-exist", transcript.run.run_id.as_str());
    let (status, _) = get(&router, &format!("/api/v1/runs/{unknown_run}")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let unknown_conv = format!(
        "{}-does-not-exist",
        transcript.conversation.conversation_id.as_str()
    );
    let (status, body) = get(&router, &format!("/api/v1/conversations/{unknown_conv}")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["code"], "NOT_FOUND");
}
