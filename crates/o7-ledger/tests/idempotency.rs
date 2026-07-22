//! Acceptance (6, 7): idempotent replay returns the prior result; the same key
//! with a different request digest is a conflict that changes nothing.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use o7_ledger::{Idempotency, Ledger, NewRun, SqliteLedger};

fn key(k: &str) -> Option<Idempotency> {
    Some(Idempotency { key: k.to_owned() })
}

// (6) Repeating an idempotent request returns the prior result and does not
// duplicate any state.
#[tokio::test]
async fn idempotent_replay_returns_prior_result() {
    let ledger = SqliteLedger::open_in_memory().unwrap();

    let c1 = ledger.create_conversation(key("conv")).await.unwrap();
    let c2 = ledger.create_conversation(key("conv")).await.unwrap();
    assert_eq!(c1.conversation_id, c2.conversation_id);
    // Exactly one conversation.created — the replay created nothing new.
    let events = ledger
        .read_events(&c1.conversation_id, None, 100)
        .await
        .unwrap();
    assert_eq!(events.len(), 1);

    let req = NewRun {
        conversation_id: c1.conversation_id.clone(),
        parent_run_id: None,
        agent: "codex".to_owned(),
        role: "implementer".to_owned(),
    };
    let r1 = ledger.create_run(req.clone(), key("run")).await.unwrap();
    let r2 = ledger.create_run(req.clone(), key("run")).await.unwrap();
    assert_eq!(r1.run_id, r2.run_id);

    let m1 = ledger
        .append_user_message(
            c1.conversation_id.clone(),
            serde_json::json!({"m":"hi"}),
            None,
            key("msg"),
        )
        .await
        .unwrap();
    let m2 = ledger
        .append_user_message(
            c1.conversation_id.clone(),
            serde_json::json!({"m":"hi"}),
            None,
            key("msg"),
        )
        .await
        .unwrap();
    assert_eq!(m1.event_id, m2.event_id);
    assert_eq!(m1.sequence, m2.sequence);

    // Total events: conversation.created + run.created + user.message = 3.
    let all = ledger
        .read_events(&c1.conversation_id, None, 100)
        .await
        .unwrap();
    assert_eq!(all.len(), 3);
}

// (7) Same key, different request digest → IDEMPOTENCY_CONFLICT, nothing changes.
#[tokio::test]
async fn same_key_different_digest_conflicts() {
    let ledger = SqliteLedger::open_in_memory().unwrap();
    let conv = ledger.create_conversation(None).await.unwrap();

    let req_a = NewRun {
        conversation_id: conv.conversation_id.clone(),
        parent_run_id: None,
        agent: "claude".to_owned(),
        role: "implementer".to_owned(),
    };
    let _run = ledger.create_run(req_a, key("dup")).await.unwrap();

    let req_b = NewRun {
        conversation_id: conv.conversation_id.clone(),
        parent_run_id: None,
        agent: "codex".to_owned(), // different request under the same key
        role: "implementer".to_owned(),
    };
    let err = ledger.create_run(req_b, key("dup")).await.unwrap_err();
    assert_eq!(err.code(), "IDEMPOTENCY_CONFLICT");

    // Exactly one run.created event — the conflicting call created nothing.
    let events = ledger
        .read_events(&conv.conversation_id, None, 100)
        .await
        .unwrap();
    let runs_created = events
        .iter()
        .filter(|e| e.event_type == "run.created")
        .count();
    assert_eq!(runs_created, 1);
}
