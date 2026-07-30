//! Q-Deck R0.6 (`docs/q-deck/r06-verdict-fidelity.md`): proves the new sealed
//! `blocked`/`error` run outcomes flow through o7d's REST and SSE surface
//! exactly as they are in the ledger — no recomputation, no schema bump.
//! `RunDto.status`/`EventDto.event_type` are bare-string pass-throughs of the
//! ledger value already, so no `dto.rs`/`routes.rs` code changed for this;
//! these tests exist to prove that claim rather than just assert it.
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

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use http_body_util::BodyExt;
use o7_ledger::{NewRun, SqliteLedger};
use serde_json::Value;
use tower::ServiceExt;

async fn seeded_router_with_outcome(
    outcome: &str,
) -> (Router, o7_ledger::Conversation, o7_ledger::Run) {
    let dir = tempfile::tempdir().unwrap();
    let ledger = SqliteLedger::open(dir.path().join("ledger.sqlite3")).unwrap();
    std::mem::forget(dir);

    let conv = ledger.create_conversation(None).await.unwrap();
    let run = ledger
        .create_run(
            NewRun {
                conversation_id: conv.conversation_id.clone(),
                parent_run_id: None,
                agent: "claude".to_owned(),
                role: "implementer".to_owned(),
            },
            None,
        )
        .await
        .unwrap();
    ledger.start_run(run.run_id.clone()).await.unwrap();
    let run = match outcome {
        "blocked" => ledger.block_run(run.run_id.clone()).await.unwrap(),
        "error" => ledger.error_run(run.run_id.clone()).await.unwrap(),
        other => panic!("unknown outcome {other:?}"),
    };
    let router = o7d::router(ledger);
    (router, conv, run)
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
    let body: Value = serde_json::from_slice(&bytes).unwrap_or_else(|e| {
        panic!(
            "response body is not valid JSON: {e}: {:?}",
            String::from_utf8_lossy(&bytes)
        )
    });
    (status, body)
}

#[tokio::test]
async fn blocked_and_error_runs_are_listed_and_reachable_by_id() {
    for outcome in ["blocked", "error"] {
        let (router, _conv, run) = seeded_router_with_outcome(outcome).await;

        let (status, one) = get(&router, &format!("/api/v1/runs/{}", run.run_id.as_str())).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(one["status"], outcome, "outcome={outcome}");
        assert_eq!(
            one["schema_version"], 1,
            "no schema bump for a new status value"
        );
        assert!(one["finished_at"].is_number(), "blocked/error are sealed");

        let (status, list) = get(&router, &format!("/api/v1/runs?status={outcome}")).await;
        assert_eq!(status, StatusCode::OK);
        let items = list["items"].as_array().unwrap();
        assert_eq!(items.len(), 1, "?status={outcome} must filter correctly");
        assert_eq!(items[0]["run_id"], run.run_id.as_str());
    }
}

#[tokio::test]
async fn blocked_and_error_events_travel_with_the_right_event_type() {
    for (outcome, expected_event_type) in [("blocked", "run.blocked"), ("error", "run.errored")] {
        let (router, conv, run) = seeded_router_with_outcome(outcome).await;

        let (status, page) = get(
            &router,
            &format!(
                "/api/v1/conversations/{}/events",
                conv.conversation_id.as_str()
            ),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let items = page["items"].as_array().unwrap();
        let terminal_event = items
            .iter()
            .find(|e| {
                e["run_id"] == run.run_id.as_str()
                    && e["event_type"] != "run.created"
                    && e["event_type"] != "run.started"
            })
            .expect("the outcome event must be present");
        assert_eq!(terminal_event["event_type"], expected_event_type);
        assert_eq!(terminal_event["schema_version"], 1);
    }
}

#[tokio::test]
async fn an_unrecognized_status_filter_still_400s_not_silently_matches_nothing() {
    let (router, _conv, _run) = seeded_router_with_outcome("blocked").await;
    let (status, body) = get(&router, "/api/v1/runs?status=banana").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "BAD_REQUEST");
}
