//! Acceptance (3): parallel appends into one conversation — through SEPARATE
//! connections to the same file — get a unique, gap-free, strictly increasing
//! sequence.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use o7_ledger::{Ledger, SqliteLedger};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn parallel_appends_get_unique_increasing_sequence() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ledger.db");

    let setup = SqliteLedger::open(&path).unwrap();
    let conv = setup.create_conversation(None).await.unwrap();
    drop(setup);

    let writers = 8usize;
    let per_writer = 25u64;

    let mut handles = Vec::new();
    for _ in 0..writers {
        let p = path.clone();
        let conv_id = conv.conversation_id.clone();
        handles.push(tokio::spawn(async move {
            // Each writer is its OWN connection to the same file — the invariant
            // is enforced at the database (IMMEDIATE txn + busy_timeout), not just
            // by an in-process lock.
            let ledger = SqliteLedger::open(&p).unwrap();
            for n in 0..per_writer {
                ledger
                    .append_user_message(conv_id.clone(), serde_json::json!({ "n": n }), None, None)
                    .await
                    .unwrap();
            }
        }));
    }
    for handle in handles {
        handle.await.unwrap();
    }

    let reader = SqliteLedger::open(&path).unwrap();
    let mut seqs: Vec<u64> = Vec::new();
    let mut cursor: Option<u64> = None;
    loop {
        let page = reader
            .read_events(&conv.conversation_id, cursor, 1000)
            .await
            .unwrap();
        if page.is_empty() {
            break;
        }
        for e in &page {
            seqs.push(e.sequence);
        }
        cursor = page.last().map(|e| e.sequence);
    }

    let expected_total = 1 + (writers as u64) * per_writer; // conversation.created + all msgs
    let expected: Vec<u64> = (1..=expected_total).collect();
    assert_eq!(
        seqs, expected,
        "concurrent appends must yield 1..N with no dups and no gaps"
    );
}
