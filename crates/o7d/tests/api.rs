//! Router-level HTTP tests: real `http::Request`s driven through the actual
//! `axum::Router` via `tower::Service::oneshot` — real routing, extraction,
//! and JSON serialization, not handler functions called directly — over a
//! ledger populated only through `o7-ledger`'s own public write APIs.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use http_body_util::BodyExt;
use o7_ledger::{NewRun, SqliteLedger};
use serde_json::Value;
use tower::ServiceExt;

async fn seeded_router() -> (Router, o7_ledger::Conversation, o7_ledger::Run) {
    let dir = tempfile::tempdir().unwrap();
    let ledger = SqliteLedger::open(dir.path().join("ledger.sqlite3")).unwrap();
    std::mem::forget(dir); // the file must outlive this function's stack frame

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
    for n in 0..5 {
        ledger
            .append_user_message(
                conv.conversation_id.clone(),
                serde_json::json!({ "n": n }),
                None,
                None,
            )
            .await
            .unwrap();
    }
    let router = o7d::router(ledger);
    (router, conv, run)
}

async fn get(router: &Router, uri: &str) -> (StatusCode, Value) {
    let req = Request::builder()
        .uri(uri)
        .body(Body::empty())
        .expect("valid request");
    let resp = router.clone().oneshot(req).await.expect("router does not fail");
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
            panic!("response body is not valid JSON: {e}: {:?}", String::from_utf8_lossy(&bytes))
        })
    };
    (status, body)
}

#[tokio::test]
async fn health_reports_ok() {
    let (router, ..) = seeded_router().await;
    let (status, body) = get(&router, "/api/v1/health").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "ok");
    assert_eq!(body["schema_version"], 1);
}

#[tokio::test]
async fn list_and_get_conversation_round_trip() {
    let (router, conv, _run) = seeded_router().await;

    let (status, list) = get(&router, "/api/v1/conversations").await;
    assert_eq!(status, StatusCode::OK);
    let items = list["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["conversation_id"], conv.conversation_id.as_str());
    assert_eq!(items[0]["status"], "open");
    // A populated page's `next_before` is not itself proof there is nothing
    // older (see o7-ledger's list_conversations doc comment) — the real proof
    // is that paging with it comes back empty.
    if let Some(cursor) = list["next_before"].as_str() {
        let (status, next) = get(&router, &format!("/api/v1/conversations?before={cursor}")).await;
        assert_eq!(status, StatusCode::OK);
        assert!(
            next["items"].as_array().unwrap().is_empty(),
            "the single conversation was already returned; paging past it must be empty"
        );
    }

    let (status, one) = get(
        &router,
        &format!("/api/v1/conversations/{}", conv.conversation_id.as_str()),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(one["conversation_id"], conv.conversation_id.as_str());
}

#[tokio::test]
async fn unknown_conversation_is_404() {
    let (router, ..) = seeded_router().await;
    let (status, body) = get(&router, "/api/v1/conversations/does-not-exist").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["code"], "NOT_FOUND");
}

#[tokio::test]
async fn conversation_events_cursor_pagination() {
    let (router, conv, _run) = seeded_router().await;

    // conversation.created + run.created + 5 user messages = 7 events.
    let (status, page) = get(
        &router,
        &format!(
            "/api/v1/conversations/{}/events?limit=3",
            conv.conversation_id.as_str()
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let items = page["items"].as_array().unwrap();
    assert_eq!(items.len(), 3);
    assert_eq!(items[0]["sequence"], 1);
    assert_eq!(items[2]["sequence"], 3);
    let next_after = page["next_after"].as_u64().unwrap();
    assert_eq!(next_after, 3);

    let (status2, page2) = get(
        &router,
        &format!(
            "/api/v1/conversations/{}/events?after={next_after}",
            conv.conversation_id.as_str()
        ),
    )
    .await;
    assert_eq!(status2, StatusCode::OK);
    let items2 = page2["items"].as_array().unwrap();
    assert_eq!(items2.len(), 4, "remaining 4 of 7 total events");
    assert_eq!(items2[0]["sequence"], 4);
    assert_eq!(items2[3]["sequence"], 7);
}

#[tokio::test]
async fn malformed_cursor_is_400() {
    let (router, ..) = seeded_router().await;
    let (status, body) = get(&router, "/api/v1/conversations?before=not-a-cursor").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "BAD_REQUEST");
}

#[tokio::test]
async fn list_runs_global_and_scoped_by_conversation() {
    let (router, conv, run) = seeded_router().await;

    let (status, all) = get(&router, "/api/v1/runs").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(all["items"].as_array().unwrap().len(), 1);

    let (status, scoped) = get(
        &router,
        &format!("/api/v1/runs?conversation_id={}", conv.conversation_id.as_str()),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let scoped_items = scoped["items"].as_array().unwrap();
    assert_eq!(scoped_items.len(), 1);
    assert_eq!(scoped_items[0]["run_id"], run.run_id.as_str());

    let (status, one) = get(&router, &format!("/api/v1/runs/{}", run.run_id.as_str())).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(one["run_id"], run.run_id.as_str());
    assert_eq!(one["status"], "queued");
}

#[tokio::test]
async fn unrelated_conversation_never_leaks_into_scoped_run_listing() {
    let dir = tempfile::tempdir().unwrap();
    let ledger = SqliteLedger::open(dir.path().join("ledger.sqlite3")).unwrap();
    std::mem::forget(dir);
    let conv = ledger.create_conversation(None).await.unwrap();
    let other = ledger.create_conversation(None).await.unwrap();
    ledger
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
    let router = o7d::router(ledger);

    let (status, empty) = get(
        &router,
        &format!("/api/v1/runs?conversation_id={}", other.conversation_id.as_str()),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(empty["items"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn unknown_run_is_404() {
    let (router, ..) = seeded_router().await;
    let (status, _) = get(&router, "/api/v1/runs/does-not-exist").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
