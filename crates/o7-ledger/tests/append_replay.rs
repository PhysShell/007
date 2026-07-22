//! Acceptance: create/append/replay, cursor pagination, payload round-trip,
//! foreign-key + forbidden-transition rejection, and rollback atomicity.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use o7_ledger::{ConversationStatus, EventId, Ledger, NewEvent, NewRun, RunStatus, SqliteLedger};

fn run_req(conversation_id: o7_ledger::ConversationId) -> NewRun {
    NewRun {
        conversation_id,
        parent_run_id: None,
        agent: "codex".to_owned(),
        role: "implementer".to_owned(),
    }
}

// (1) Creating conversation and run.
#[tokio::test]
async fn create_conversation_and_run() {
    let ledger = SqliteLedger::open_in_memory().unwrap();
    let conv = ledger.create_conversation(None).await.unwrap();
    assert_eq!(conv.status, ConversationStatus::Open);

    let run = ledger
        .create_run(run_req(conv.conversation_id.clone()), None)
        .await
        .unwrap();
    assert_eq!(run.status, RunStatus::Queued);
    assert_eq!(run.conversation_id, conv.conversation_id);

    let events = ledger
        .read_events(&conv.conversation_id, None, 100)
        .await
        .unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].event_type, "conversation.created");
    assert_eq!(events[0].sequence, 1);
    assert_eq!(events[1].event_type, "run.created");
    assert_eq!(events[1].sequence, 2);
}

// (2) Append and replay by cursor.
#[tokio::test]
async fn append_and_replay_by_cursor() {
    let ledger = SqliteLedger::open_in_memory().unwrap();
    let conv = ledger.create_conversation(None).await.unwrap();
    for n in 1..=3 {
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

    let all = ledger
        .read_events(&conv.conversation_id, None, 100)
        .await
        .unwrap();
    assert_eq!(all.len(), 4, "conversation.created + 3 user messages");
    let seqs: Vec<u64> = all.iter().map(|e| e.sequence).collect();
    assert_eq!(seqs, vec![1, 2, 3, 4]);

    // Cursor is EXCLUSIVE.
    let after_one = ledger
        .read_events(&conv.conversation_id, Some(1), 100)
        .await
        .unwrap();
    assert_eq!(
        after_one.iter().map(|e| e.sequence).collect::<Vec<_>>(),
        vec![2, 3, 4]
    );

    let page = ledger
        .read_events(&conv.conversation_id, Some(2), 1)
        .await
        .unwrap();
    assert_eq!(page.len(), 1);
    assert_eq!(page[0].sequence, 3);
}

// (4) Append into different conversations is independent.
#[tokio::test]
async fn conversations_have_independent_sequences() {
    let ledger = SqliteLedger::open_in_memory().unwrap();
    let a = ledger.create_conversation(None).await.unwrap();
    let b = ledger.create_conversation(None).await.unwrap();
    ledger
        .append_user_message(
            a.conversation_id.clone(),
            serde_json::json!({"x":"a"}),
            None,
            None,
        )
        .await
        .unwrap();
    ledger
        .append_user_message(
            b.conversation_id.clone(),
            serde_json::json!({"x":"b"}),
            None,
            None,
        )
        .await
        .unwrap();

    let ea = ledger
        .read_events(&a.conversation_id, None, 100)
        .await
        .unwrap();
    let eb = ledger
        .read_events(&b.conversation_id, None, 100)
        .await
        .unwrap();
    assert_eq!(
        ea.iter().map(|e| e.sequence).collect::<Vec<_>>(),
        vec![1, 2]
    );
    assert_eq!(
        eb.iter().map(|e| e.sequence).collect::<Vec<_>>(),
        vec![1, 2]
    );
}

// (5) A rolled-back append leaves no half-event and consumes no sequence.
#[tokio::test]
async fn rollback_leaves_no_half_event() {
    let ledger = SqliteLedger::open_in_memory().unwrap();
    let conv = ledger.create_conversation(None).await.unwrap();

    let dup = EventId::from_raw("dup-event-id".to_owned());
    let mk = |eid: EventId| NewEvent {
        event_id: eid,
        conversation_id: conv.conversation_id.clone(),
        run_id: None,
        attempt_id: None,
        event_type: o7_ledger::EventType::SystemNote,
        schema_version: 1,
        payload: serde_json::json!({}),
    };

    // First append succeeds → sequence 2 (after conversation.created seq 1).
    let first = ledger.append_event(mk(dup.clone())).await.unwrap();
    assert_eq!(first.sequence, 2);

    // Re-using the same event_id violates the PK → the whole transaction rolls
    // back. No partial row, and the sequence is not consumed.
    let err = ledger.append_event(mk(dup.clone())).await;
    assert!(err.is_err(), "duplicate event_id must fail");

    // A fresh append gets sequence 3 (no gap from the failed one).
    let third = ledger
        .append_event(mk(EventId::from_raw("fresh".to_owned())))
        .await
        .unwrap();
    assert_eq!(third.sequence, 3);

    let events = ledger
        .read_events(&conv.conversation_id, None, 100)
        .await
        .unwrap();
    assert_eq!(events.len(), 3);
    assert_eq!(
        events.iter().map(|e| e.sequence).collect::<Vec<_>>(),
        vec![1, 2, 3]
    );
}

// (13) Foreign-key violation is blocked.
#[tokio::test]
async fn foreign_key_violation_is_blocked() {
    let ledger = SqliteLedger::open_in_memory().unwrap();
    let orphan = NewEvent {
        event_id: EventId::generate(),
        conversation_id: o7_ledger::ConversationId::from_raw("does-not-exist".to_owned()),
        run_id: None,
        attempt_id: None,
        event_type: o7_ledger::EventType::SystemNote,
        schema_version: 1,
        payload: serde_json::json!({}),
    };
    let err = ledger.append_event(orphan).await;
    assert!(
        err.is_err(),
        "event referencing a missing conversation must fail (FK on)"
    );
}

// (14) A forbidden state transition is blocked.
#[tokio::test]
async fn forbidden_transition_is_blocked() {
    let ledger = SqliteLedger::open_in_memory().unwrap();
    let conv = ledger.create_conversation(None).await.unwrap();
    let run = ledger
        .create_run(run_req(conv.conversation_id.clone()), None)
        .await
        .unwrap();

    // queued -> completed is forbidden.
    let err = ledger.complete_run(run.run_id.clone()).await.unwrap_err();
    assert_eq!(err.code(), "FORBIDDEN_TRANSITION");

    // queued -> running is allowed.
    let running = ledger.start_run(run.run_id.clone()).await.unwrap();
    assert_eq!(running.status, RunStatus::Running);

    ledger.complete_run(run.run_id.clone()).await.unwrap();
    // completed -> running is forbidden.
    let err = ledger.start_run(run.run_id.clone()).await.unwrap_err();
    assert_eq!(err.code(), "FORBIDDEN_TRANSITION");
}

// (17) Pagination loses and duplicates nothing.
#[tokio::test]
async fn pagination_is_lossless_and_dedup() {
    let ledger = SqliteLedger::open_in_memory().unwrap();
    let conv = ledger.create_conversation(None).await.unwrap();
    let total_messages = 250u64;
    for n in 0..total_messages {
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

    let mut collected: Vec<u64> = Vec::new();
    let mut cursor: Option<u64> = None;
    loop {
        let page = ledger
            .read_events(&conv.conversation_id, cursor, 50)
            .await
            .unwrap();
        if page.is_empty() {
            break;
        }
        for e in &page {
            collected.push(e.sequence);
        }
        cursor = page.last().map(|e| e.sequence);
    }

    // 1 conversation.created + 250 user messages.
    let expected: Vec<u64> = (1..=total_messages + 1).collect();
    assert_eq!(collected, expected, "strictly increasing, no gaps, no dups");
}

// (18) Payload round-trips without changing meaning.
#[tokio::test]
async fn payload_round_trips() {
    let ledger = SqliteLedger::open_in_memory().unwrap();
    let conv = ledger.create_conversation(None).await.unwrap();
    let payload = serde_json::json!({
        "a": 1,
        "nested": { "b": [1, 2, 3], "c": "unicode ✓ ключ" },
        "flag": true,
        "empty": null
    });
    let persisted = ledger
        .append_user_message(conv.conversation_id.clone(), payload.clone(), None, None)
        .await
        .unwrap();
    assert_eq!(persisted.payload, payload);

    let read_back = ledger
        .read_events(&conv.conversation_id, Some(1), 10)
        .await
        .unwrap();
    assert_eq!(read_back.len(), 1);
    assert_eq!(read_back[0].payload, payload);
}
