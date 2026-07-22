//! Acceptance: cross-entity referential integrity the ledger must enforce ITSELF
//! (independent foreign keys are not enough): an event may not reference a run
//! from another conversation; an event's attempt must belong to its run; a
//! parent run must live in the same conversation.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use o7_ledger::{ConversationId, EventId, EventType, Ledger, NewEvent, NewRun, SqliteLedger};

fn run_req(conversation_id: ConversationId, parent: Option<o7_ledger::RunId>) -> NewRun {
    NewRun {
        conversation_id,
        parent_run_id: parent,
        agent: "codex".to_owned(),
        role: "implementer".to_owned(),
    }
}

// An event with conversation_id=A referencing a run that lives in conversation B
// must be rejected.
#[tokio::test]
async fn event_cannot_reference_run_from_another_conversation() {
    let ledger = SqliteLedger::open_in_memory().unwrap();
    let a = ledger.create_conversation(None).await.unwrap();
    let b = ledger.create_conversation(None).await.unwrap();
    let run_in_b = ledger
        .create_run(run_req(b.conversation_id.clone(), None), None)
        .await
        .unwrap();

    let bad = NewEvent {
        event_id: EventId::generate(),
        conversation_id: a.conversation_id.clone(), // conversation A …
        run_id: Some(run_in_b.run_id.clone()),      // … but a run from B
        attempt_id: None,
        event_type: EventType::SystemNote,
        schema_version: 1,
        payload: serde_json::json!({}),
    };
    assert!(
        ledger.append_event(bad).await.is_err(),
        "event.conversation_id/run_id must be a matching pair"
    );
}

// An event whose attempt_id belongs to a different run than its run_id must be
// rejected.
#[tokio::test]
async fn event_attempt_must_belong_to_its_run() {
    let ledger = SqliteLedger::open_in_memory().unwrap();
    let conv = ledger.create_conversation(None).await.unwrap();
    let run1 = ledger
        .create_run(run_req(conv.conversation_id.clone(), None), None)
        .await
        .unwrap();
    let run2 = ledger
        .create_run(run_req(conv.conversation_id.clone(), None), None)
        .await
        .unwrap();
    ledger.start_run(run2.run_id.clone()).await.unwrap();
    let attempt_of_run2 = ledger.create_attempt(run2.run_id.clone()).await.unwrap();

    let bad = NewEvent {
        event_id: EventId::generate(),
        conversation_id: conv.conversation_id.clone(),
        run_id: Some(run1.run_id.clone()), // run 1 …
        attempt_id: Some(attempt_of_run2.attempt_id.clone()), // … attempt of run 2
        event_type: EventType::SystemNote,
        schema_version: 1,
        payload: serde_json::json!({}),
    };
    assert!(
        ledger.append_event(bad).await.is_err(),
        "event.run_id/attempt_id must be a matching pair"
    );
}

// A parent run in a different conversation must be rejected.
#[tokio::test]
async fn parent_run_must_be_in_same_conversation() {
    let ledger = SqliteLedger::open_in_memory().unwrap();
    let a = ledger.create_conversation(None).await.unwrap();
    let b = ledger.create_conversation(None).await.unwrap();
    let parent_in_a = ledger
        .create_run(run_req(a.conversation_id.clone(), None), None)
        .await
        .unwrap();

    let cross = ledger
        .create_run(
            run_req(b.conversation_id.clone(), Some(parent_in_a.run_id.clone())),
            None,
        )
        .await;
    assert!(
        cross.is_err(),
        "a run's parent must live in the same conversation"
    );
}
