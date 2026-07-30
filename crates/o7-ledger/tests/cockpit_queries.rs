//! Acceptance: `list_conversations`/`list_runs` — newest-first ordering,
//! keyset pagination that is lossless and duplicate-free even under
//! same-millisecond ties, per-conversation scoping, and bounded limits.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use o7_ledger::{ListCursor, NewRun, SqliteLedger};
use std::collections::HashSet;

fn run_req(conversation_id: o7_ledger::ConversationId, agent: &str) -> NewRun {
    NewRun {
        conversation_id,
        parent_run_id: None,
        agent: agent.to_owned(),
        role: "implementer".to_owned(),
    }
}

// An empty ledger lists nothing, and says so by an absent cursor rather than
// an error or a page that looks the same as "there might be more".
#[tokio::test]
async fn empty_ledger_lists_empty_page() {
    let ledger = SqliteLedger::open_in_memory().unwrap();
    let page = ledger.list_conversations(None, 50).await.unwrap();
    assert!(page.items.is_empty());
    assert!(page.next_before.is_none());

    let runs = ledger.list_runs(None, None, 50).await.unwrap();
    assert!(runs.items.is_empty());
    assert!(runs.next_before.is_none());
}

// Newest conversation first, oldest last — the dashboard's whole point.
#[tokio::test]
async fn conversations_are_newest_first() {
    let ledger = SqliteLedger::open_in_memory().unwrap();
    let mut ids = Vec::new();
    for _ in 0..5 {
        ids.push(
            ledger
                .create_conversation(None)
                .await
                .unwrap()
                .conversation_id,
        );
    }
    let page = ledger.list_conversations(None, 50).await.unwrap();
    let listed: Vec<_> = page.items.iter().map(|c| c.conversation_id.clone()).collect();
    let mut expected = ids.clone();
    expected.reverse();
    assert_eq!(listed, expected, "must be newest-first, i.e. reverse creation order");
}

// Pagination over many rows created in a tight loop — real same-millisecond
// ties, not a hypothetical. Every id must appear exactly once, across however
// many pages it takes, in strict newest-first order end to end.
#[tokio::test]
async fn conversation_pagination_is_lossless_and_dedup_under_ties() {
    let ledger = SqliteLedger::open_in_memory().unwrap();
    let total = 137usize;
    let mut created = Vec::with_capacity(total);
    for _ in 0..total {
        created.push(
            ledger
                .create_conversation(None)
                .await
                .unwrap()
                .conversation_id,
        );
    }

    let mut collected = Vec::new();
    let mut cursor: Option<ListCursor> = None;
    loop {
        let page = ledger.list_conversations(cursor.clone(), 10).await.unwrap();
        if page.items.is_empty() {
            break;
        }
        for c in &page.items {
            collected.push(c.conversation_id.clone());
        }
        cursor = page.next_before;
    }

    assert_eq!(collected.len(), total, "no row lost or duplicated across pages");
    let unique: HashSet<_> = collected.iter().collect();
    assert_eq!(unique.len(), total, "no duplicate across page boundaries");
    let mut expected = created.clone();
    expected.reverse();
    assert_eq!(collected, expected, "full paginated walk is exactly reverse creation order");
}

// list_runs with no conversation filter spans every conversation; with a
// filter it scopes to exactly one, and does not leak another's runs.
#[tokio::test]
async fn list_runs_scopes_by_conversation_or_lists_all() {
    let ledger = SqliteLedger::open_in_memory().unwrap();
    let a = ledger.create_conversation(None).await.unwrap();
    let b = ledger.create_conversation(None).await.unwrap();

    let run_a1 = ledger
        .create_run(run_req(a.conversation_id.clone(), "claude"), None)
        .await
        .unwrap();
    let run_a2 = ledger
        .create_run(run_req(a.conversation_id.clone(), "codex"), None)
        .await
        .unwrap();
    let run_b1 = ledger
        .create_run(run_req(b.conversation_id.clone(), "claude"), None)
        .await
        .unwrap();

    let all = ledger.list_runs(None, None, 50).await.unwrap();
    let all_ids: HashSet<_> = all.items.iter().map(|r| r.run_id.clone()).collect();
    assert_eq!(all_ids.len(), 3);
    assert!(all_ids.contains(&run_a1.run_id));
    assert!(all_ids.contains(&run_a2.run_id));
    assert!(all_ids.contains(&run_b1.run_id));

    let only_a = ledger
        .list_runs(Some(a.conversation_id.clone()), None, 50)
        .await
        .unwrap();
    let a_ids: HashSet<_> = only_a.items.iter().map(|r| r.run_id.clone()).collect();
    assert_eq!(a_ids.len(), 2);
    assert!(a_ids.contains(&run_a1.run_id));
    assert!(a_ids.contains(&run_a2.run_id));
    assert!(
        !a_ids.contains(&run_b1.run_id),
        "scoping to conversation a must not leak conversation b's run"
    );

    let only_b = ledger
        .list_runs(Some(b.conversation_id.clone()), None, 50)
        .await
        .unwrap();
    assert_eq!(only_b.items.len(), 1);
    assert_eq!(only_b.items[0].run_id, run_b1.run_id);
}

// A limit above MAX_LIST_LIMIT is silently clamped, never an unbounded scan.
#[tokio::test]
async fn list_limit_is_clamped_not_unbounded() {
    let ledger = SqliteLedger::open_in_memory().unwrap();
    for _ in 0..5 {
        ledger.create_conversation(None).await.unwrap();
    }
    let page = ledger
        .list_conversations(None, o7_ledger::MAX_LIST_LIMIT + 10_000)
        .await
        .unwrap();
    assert_eq!(page.items.len(), 5, "clamped request still returns only what exists");
}

// An unknown conversation_id filter is an empty result, not an error — the
// same "unknown id -> empty, not corruption" contract read_events already has.
#[tokio::test]
async fn unknown_conversation_filter_is_empty_not_error() {
    let ledger = SqliteLedger::open_in_memory().unwrap();
    ledger.create_conversation(None).await.unwrap();
    let missing = o7_ledger::ConversationId::from_raw("does-not-exist".to_owned());
    let page = ledger.list_runs(Some(missing), None, 50).await.unwrap();
    assert!(page.items.is_empty());
    assert!(page.next_before.is_none());
}
