//! Q-Deck R0.7 (`docs/q-deck/r07-live-ingress.md`): `create_run_with_id` is
//! the fix for a real blocker found auditing the live-ingress seam —
//! `create_run` mints its own `RunId`, which would silently give a live-
//! projected run a second, random ledger identity instead of sharing the
//! canonical `o7-run::RunId`. These tests prove the shared-id path end to
//! end, and that plain `create_run` is untouched.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use o7_ledger::{ConversationId, Idempotency, NewRun, RunId, RunStatus, SqliteLedger};

fn run_req(conversation_id: ConversationId) -> NewRun {
    NewRun {
        conversation_id,
        parent_run_id: None,
        agent: "claude".to_owned(),
        role: "implementer".to_owned(),
    }
}

#[tokio::test]
async fn create_run_with_id_uses_exactly_the_given_id() {
    let ledger = SqliteLedger::open_in_memory().unwrap();
    let conv = ledger.create_conversation(None).await.unwrap();
    let run_id = RunId::from_raw("o7-run-canonical-id-1".to_owned());

    let run = ledger
        .create_run_with_id(
            run_req(conv.conversation_id.clone()),
            run_id.clone(),
            Idempotency {
                key: run_id.as_str().to_owned(),
            },
        )
        .await
        .unwrap();

    assert_eq!(run.run_id, run_id);
    assert_eq!(run.status, RunStatus::Queued);

    // Reachable by that exact id through the ordinary read path — no second,
    // ledger-generated id exists anywhere for this run.
    let fetched = ledger.run(run_id.clone()).await.unwrap().unwrap();
    assert_eq!(fetched.run_id, run_id);
}

#[tokio::test]
async fn retrying_create_run_with_id_under_the_same_key_is_a_no_op() {
    let ledger = SqliteLedger::open_in_memory().unwrap();
    let conv = ledger.create_conversation(None).await.unwrap();
    let run_id = RunId::from_raw("o7-run-canonical-id-2".to_owned());
    let idem = Idempotency {
        key: run_id.as_str().to_owned(),
    };

    let first = ledger
        .create_run_with_id(
            run_req(conv.conversation_id.clone()),
            run_id.clone(),
            idem.clone(),
        )
        .await
        .unwrap();

    // Simulates recovery re-applying the same canonical RunStarted event
    // after a crash between the insert and the caller's own bookkeeping —
    // must return the SAME run, not create a second one or error.
    let second = ledger
        .create_run_with_id(run_req(conv.conversation_id.clone()), run_id.clone(), idem)
        .await
        .unwrap();

    assert_eq!(first, second);

    let runs = ledger
        .list_runs(Some(conv.conversation_id), None, None, 50)
        .await
        .unwrap();
    assert_eq!(
        runs.items.len(),
        1,
        "a retried create_run_with_id must never leave a second run behind"
    );
}

#[tokio::test]
async fn create_run_with_id_conflicts_on_the_same_key_with_different_content() {
    let ledger = SqliteLedger::open_in_memory().unwrap();
    let conv = ledger.create_conversation(None).await.unwrap();
    let run_id = RunId::from_raw("o7-run-canonical-id-3".to_owned());
    let key = "shared-idem-key".to_owned();

    ledger
        .create_run_with_id(
            run_req(conv.conversation_id.clone()),
            run_id,
            Idempotency { key: key.clone() },
        )
        .await
        .unwrap();

    // Same idempotency key, a DIFFERENT run_id this time — must never be
    // silently accepted as "the same request replayed" (that would let one
    // key secretly reassign an existing ledger run to a different RunId).
    let other_id = RunId::from_raw("o7-run-canonical-id-3-DIFFERENT".to_owned());
    let err = ledger
        .create_run_with_id(run_req(conv.conversation_id), other_id, Idempotency { key })
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        o7_ledger::LedgerError::IdempotencyConflict { .. }
    ));
}

#[tokio::test]
async fn create_run_still_mints_its_own_id_unaffected_by_create_run_with_id_existing() {
    // Plain create_run's own contract (pre-R0.7) must be completely
    // unaffected by create_run_with_id sharing its internal implementation.
    let ledger = SqliteLedger::open_in_memory().unwrap();
    let conv = ledger.create_conversation(None).await.unwrap();

    let a = ledger
        .create_run(run_req(conv.conversation_id.clone()), None)
        .await
        .unwrap();
    let b = ledger
        .create_run(run_req(conv.conversation_id), None)
        .await
        .unwrap();

    assert_ne!(
        a.run_id, b.run_id,
        "create_run still mints a fresh id each time"
    );
}
