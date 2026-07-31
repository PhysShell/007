//! Q-Deck R0.7 (`docs/q-deck/r07-live-ingress.md`): `append_system_note` is
//! the vehicle for projecting canonical `o7-run` event kinds that have no
//! dedicated `o7-ledger::EventType` of their own. Its idempotency is
//! mandatory, unlike `append_user_message`'s — these tests prove the
//! duplicate-projection case a crash-and-retry live projector actually hits.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use o7_ledger::{ConversationId, Idempotency, Ledger, SqliteLedger};

async fn conversation(ledger: &SqliteLedger) -> ConversationId {
    ledger
        .create_conversation(None)
        .await
        .unwrap()
        .conversation_id
}

#[tokio::test]
async fn appends_a_system_note_scoped_to_a_run() {
    let ledger = SqliteLedger::open_in_memory().unwrap();
    let conv = conversation(&ledger).await;
    let run = ledger
        .create_run(
            o7_ledger::NewRun {
                conversation_id: conv.clone(),
                parent_run_id: None,
                agent: "claude".to_owned(),
                role: "implementer".to_owned(),
            },
            None,
        )
        .await
        .unwrap();

    let event = ledger
        .append_system_note(
            conv.clone(),
            Some(run.run_id.clone()),
            None,
            serde_json::json!({"canonical_kind": "agent_started", "canonical_sequence": 1}),
            Idempotency {
                key: format!("{}:1", run.run_id.as_str()),
            },
        )
        .await
        .unwrap();

    assert_eq!(event.event_type, "system.note");
    assert_eq!(event.run_id, Some(run.run_id));
}

#[tokio::test]
async fn retrying_the_same_canonical_event_is_a_no_op() {
    let ledger = SqliteLedger::open_in_memory().unwrap();
    let conv = conversation(&ledger).await;
    let idem = Idempotency {
        key: "run-x:3".to_owned(),
    };
    let payload = serde_json::json!({"canonical_kind": "gate_started", "canonical_sequence": 3});

    let first = ledger
        .append_system_note(conv.clone(), None, None, payload.clone(), idem.clone())
        .await
        .unwrap();
    let second = ledger
        .append_system_note(conv.clone(), None, None, payload, idem)
        .await
        .unwrap();

    assert_eq!(first.event_id, second.event_id);
    let events = ledger.read_events(&conv, None, 100).await.unwrap();
    // conversation.created (from `conversation()` above) + exactly one
    // system.note — the retried call must not have appended a second one.
    assert_eq!(
        events.len(),
        2,
        "a retried canonical event must never be projected twice"
    );
}

#[tokio::test]
async fn same_key_different_payload_conflicts() {
    let ledger = SqliteLedger::open_in_memory().unwrap();
    let conv = conversation(&ledger).await;
    let idem = Idempotency {
        key: "run-y:1".to_owned(),
    };

    ledger
        .append_system_note(
            conv.clone(),
            None,
            None,
            serde_json::json!({"canonical_kind": "agent_started"}),
            idem.clone(),
        )
        .await
        .unwrap();

    let err = ledger
        .append_system_note(
            conv,
            None,
            None,
            serde_json::json!({"canonical_kind": "agent_exited"}),
            idem,
        )
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        o7_ledger::LedgerError::IdempotencyConflict { .. }
    ));
}
