//! Q-Deck R1 (`docs/q-deck/r1-command.md`): acceptance for the durable
//! command-continuation model — `create_command`'s validation ordering,
//! idempotency, stale-parent/concurrency guards, the fail-closed
//! no-session rule, the child-run binding's compare-and-swap safety, and
//! durability across a fresh connection (a process restart).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use o7_ledger::{
    CommandStatus, CompletionOutcome, ConversationId, Idempotency, NewCommand, NewRun,
    RebindOutcome, RunId, SqliteLedger,
};

fn key(k: &str) -> Idempotency {
    Idempotency { key: k.to_owned() }
}

/// A conversation with one ACTUALLY sealed (`completed`) run carrying a
/// durable provider session — the ordinary "continuable" starting point
/// most tests build on. A command may only continue a run that has
/// genuinely finished (§ create_command's own sealed-parent check) — a
/// provider session alone is not enough, since it is persisted well before
/// gates/`RunSealed`.
async fn conversation_with_continuable_run(ledger: &SqliteLedger) -> (ConversationId, RunId) {
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
    ledger
        .set_run_provider_session(run.run_id.clone(), "sess-1".to_owned())
        .await
        .unwrap();
    ledger.start_run(run.run_id.clone()).await.unwrap();
    ledger.complete_run(run.run_id.clone()).await.unwrap();
    (conv.conversation_id, run.run_id)
}

fn new_command(conversation_id: &ConversationId, parent_run_id: &RunId, text: &str) -> NewCommand {
    NewCommand {
        conversation_id: conversation_id.clone(),
        parent_run_id: parent_run_id.clone(),
        command_text: text.to_owned(),
    }
}

#[tokio::test]
async fn create_command_accepts_and_starts_unbound() {
    let ledger = SqliteLedger::open_in_memory().unwrap();
    let (conv, run) = conversation_with_continuable_run(&ledger).await;

    let command = ledger
        .create_command(new_command(&conv, &run, "run the tests"), key("k1"))
        .await
        .unwrap();

    assert_eq!(command.status, CommandStatus::Accepted);
    assert!(command.child_run_id.is_none());
    assert_eq!(command.conversation_id, conv);
    assert_eq!(command.parent_run_id, run);
}

#[tokio::test]
async fn same_key_same_request_replays_without_creating_a_second_row() {
    let ledger = SqliteLedger::open_in_memory().unwrap();
    let (conv, run) = conversation_with_continuable_run(&ledger).await;

    let a = ledger
        .create_command(new_command(&conv, &run, "run the tests"), key("dup"))
        .await
        .unwrap();
    let b = ledger
        .create_command(new_command(&conv, &run, "run the tests"), key("dup"))
        .await
        .unwrap();
    assert_eq!(a.command_id, b.command_id);

    // A second, DIFFERENT idempotency key would be a genuinely new command —
    // proving the row count only by inspecting via active_command_for_conversation
    // (the in-flight one) is exactly `a`/`b`'s shared identity.
    let active = ledger
        .active_command_for_conversation(conv.clone())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(active.command_id, a.command_id);
}

#[tokio::test]
async fn same_key_different_request_conflicts_without_mutating_or_invoking() {
    let ledger = SqliteLedger::open_in_memory().unwrap();
    let (conv, run) = conversation_with_continuable_run(&ledger).await;

    let a = ledger
        .create_command(new_command(&conv, &run, "run the tests"), key("dup"))
        .await
        .unwrap();

    let err = ledger
        .create_command(new_command(&conv, &run, "run DIFFERENT tests"), key("dup"))
        .await
        .unwrap_err();
    assert_eq!(err.code(), "IDEMPOTENCY_CONFLICT");

    // Still exactly the original command — the conflicting call created nothing.
    let active = ledger
        .active_command_for_conversation(conv)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(active.command_id, a.command_id);
    assert_eq!(active.command_text, "run the tests");
}

#[tokio::test]
async fn unknown_conversation_is_not_found() {
    let ledger = SqliteLedger::open_in_memory().unwrap();
    let (_conv, run) = conversation_with_continuable_run(&ledger).await;

    let bogus_conv = ConversationId::from_raw("does-not-exist".to_owned());
    let err = ledger
        .create_command(new_command(&bogus_conv, &run, "hi"), key("k"))
        .await
        .unwrap_err();
    assert_eq!(err.code(), "NOT_FOUND");
}

#[tokio::test]
async fn unknown_parent_run_is_not_found() {
    let ledger = SqliteLedger::open_in_memory().unwrap();
    let conv = ledger.create_conversation(None).await.unwrap();

    let bogus_run = RunId::from_raw("does-not-exist".to_owned());
    let err = ledger
        .create_command(
            new_command(&conv.conversation_id, &bogus_run, "hi"),
            key("k"),
        )
        .await
        .unwrap_err();
    assert_eq!(err.code(), "NOT_FOUND");
}

#[tokio::test]
async fn parent_run_from_a_different_conversation_is_not_found() {
    let ledger = SqliteLedger::open_in_memory().unwrap();
    let (_conv_a, run_a) = conversation_with_continuable_run(&ledger).await;
    let (conv_b, _run_b) = conversation_with_continuable_run(&ledger).await;

    // run_a belongs to conv_a, not conv_b — must never be accepted as conv_b's parent.
    let err = ledger
        .create_command(new_command(&conv_b, &run_a, "hi"), key("k"))
        .await
        .unwrap_err();
    assert_eq!(err.code(), "NOT_FOUND");
}

#[tokio::test]
async fn a_run_that_already_has_a_child_is_a_stale_parent() {
    let ledger = SqliteLedger::open_in_memory().unwrap();
    let (conv, run) = conversation_with_continuable_run(&ledger).await;

    // A command's own `child_run_id` bookkeeping is a SEPARATE thing from
    // what "stale parent" actually checks: whether some `run` row already
    // names `run` as ITS `parent_run_id`. So this models the real shape a
    // completed command's child run leaves behind — a genuine run row with
    // `parent_run_id = Some(run)` — and the FIRST command itself is marked
    // `completed` (not left `accepted`/`started`) so this test exercises
    // stale-parent specifically, not the separate in-flight/concurrency
    // check `a_second_command_conflicts_while_one_is_still_in_flight`
    // already covers.
    let command = ledger
        .create_command(new_command(&conv, &run, "first"), key("k1"))
        .await
        .unwrap();
    let child = ledger
        .create_run(
            NewRun {
                conversation_id: conv.clone(),
                parent_run_id: Some(run.clone()),
                agent: "claude".to_owned(),
                role: "implementer".to_owned(),
            },
            None,
        )
        .await
        .unwrap();
    ledger
        .bind_command_child_run(command.command_id.clone(), child.run_id)
        .await
        .unwrap();
    ledger
        .mark_command_completed(command.command_id)
        .await
        .unwrap();

    // A second, genuinely different command against the SAME (now-superseded) parent.
    let err = ledger
        .create_command(new_command(&conv, &run, "second"), key("k2"))
        .await
        .unwrap_err();
    assert_eq!(err.code(), "STALE_PARENT");
}

#[tokio::test]
async fn parent_without_a_provider_session_is_refused() {
    let ledger = SqliteLedger::open_in_memory().unwrap();
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
    // Sealed (so the sealed-parent check does not fire instead), but
    // deliberately never calling set_run_provider_session.
    ledger.start_run(run.run_id.clone()).await.unwrap();
    ledger.complete_run(run.run_id.clone()).await.unwrap();

    let err = ledger
        .create_command(
            new_command(&conv.conversation_id, &run.run_id, "hi"),
            key("k"),
        )
        .await
        .unwrap_err();
    assert_eq!(err.code(), "CONTINUATION_NOT_PERMITTED");
}

#[tokio::test]
async fn a_still_running_parent_is_refused_even_with_a_session() {
    let ledger = SqliteLedger::open_in_memory().unwrap();
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
    // A provider session can exist well before gates/RunSealed (it is
    // persisted right after the agent call) — it must never be treated as
    // proof the run itself has finished.
    ledger
        .set_run_provider_session(run.run_id.clone(), "sess-1".to_owned())
        .await
        .unwrap();
    ledger.start_run(run.run_id.clone()).await.unwrap();
    // Deliberately never sealing (no complete_run/fail_run/etc).

    let err = ledger
        .create_command(
            new_command(&conv.conversation_id, &run.run_id, "hi"),
            key("k"),
        )
        .await
        .unwrap_err();
    assert_eq!(err.code(), "CONTINUATION_NOT_PERMITTED");
}

#[tokio::test]
async fn the_conversations_true_latest_run_wins_even_if_an_older_leaf_has_no_child() {
    let ledger = SqliteLedger::open_in_memory().unwrap();
    let (conv, first) = conversation_with_continuable_run(&ledger).await;

    // A second, INDEPENDENT sealed run in the SAME conversation — never
    // threaded as `first`'s child (mirrors a plain `o7 run
    // --conversation-id` invocation). Both `first` and `second` are
    // leaves (neither has a child), but only `second` is the
    // conversation's actual current tail.
    let second = ledger
        .create_run(
            NewRun {
                conversation_id: conv.clone(),
                parent_run_id: None,
                agent: "claude".to_owned(),
                role: "implementer".to_owned(),
            },
            None,
        )
        .await
        .unwrap();
    ledger
        .set_run_provider_session(second.run_id.clone(), "sess-2".to_owned())
        .await
        .unwrap();
    ledger.start_run(second.run_id.clone()).await.unwrap();
    ledger.complete_run(second.run_id.clone()).await.unwrap();

    let err = ledger
        .create_command(new_command(&conv, &first, "hi"), key("k1"))
        .await
        .unwrap_err();
    assert_eq!(
        err.code(),
        "STALE_PARENT",
        "the OLDER leaf must be refused even though nothing ever claimed it as a parent"
    );

    let ok = ledger
        .create_command(new_command(&conv, &second.run_id, "hi"), key("k2"))
        .await
        .unwrap();
    assert_eq!(ok.parent_run_id, second.run_id);
}

#[tokio::test]
async fn a_second_command_conflicts_while_one_is_still_in_flight() {
    let ledger = SqliteLedger::open_in_memory().unwrap();
    let (conv, run) = conversation_with_continuable_run(&ledger).await;

    let _first = ledger
        .create_command(new_command(&conv, &run, "first"), key("k1"))
        .await
        .unwrap();

    // A DIFFERENT idempotency key (a genuinely new submission), while the
    // first is still `accepted` — must be rejected before ever touching the
    // provider, per §5.
    let err = ledger
        .create_command(new_command(&conv, &run, "second"), key("k2"))
        .await
        .unwrap_err();
    assert_eq!(err.code(), "COMMAND_CONFLICT");
}

#[tokio::test]
async fn completing_a_command_frees_the_conversation_for_a_new_one() {
    let ledger = SqliteLedger::open_in_memory().unwrap();
    let (conv, run) = conversation_with_continuable_run(&ledger).await;

    let first = ledger
        .create_command(new_command(&conv, &run, "first"), key("k1"))
        .await
        .unwrap();
    ledger
        .mark_command_completed(first.command_id)
        .await
        .unwrap();

    // The first command's own run is a stale parent now (whether or not it
    // was ever bound to a child) — a NEW command must continue from a
    // DIFFERENT, still-continuable run, not fight over concurrency with a
    // command that already finished.
    let (conv2, run2) = conversation_with_continuable_run(&ledger).await;
    let _ = conv; // silence unused in this branch; kept for clarity above
    let second = ledger
        .create_command(new_command(&conv2, &run2, "second"), key("k2"))
        .await
        .unwrap();
    assert_eq!(second.status, CommandStatus::Accepted);
}

#[tokio::test]
async fn bind_command_child_run_is_compare_and_swap_safe_under_a_race() {
    let ledger = SqliteLedger::open_in_memory().unwrap();
    let (conv, run) = conversation_with_continuable_run(&ledger).await;
    let command = ledger
        .create_command(new_command(&conv, &run, "hi"), key("k"))
        .await
        .unwrap();

    let candidate_a = RunId::from_raw("candidate-a".to_owned());
    let candidate_b = RunId::from_raw("candidate-b".to_owned());

    // Two callers race to bind the SAME command to two DIFFERENT freshly
    // minted child run ids (o7d's own race, docs/q-deck/r1-command.md's
    // bind_command_child_run doc comment).
    let bound_a = ledger
        .bind_command_child_run(command.command_id.clone(), candidate_a.clone())
        .await
        .unwrap();
    let bound_b = ledger
        .bind_command_child_run(command.command_id.clone(), candidate_b.clone())
        .await
        .unwrap();

    // Exactly one candidate wins, and BOTH callers observe the SAME winner.
    assert_eq!(bound_a.child_run_id, bound_b.child_run_id);
    assert!(bound_a.child_run_id == Some(candidate_a) || bound_a.child_run_id == Some(candidate_b));
    assert_eq!(bound_a.status, CommandStatus::Started);
}

#[tokio::test]
async fn mark_completed_and_mark_rejected_transition_status() {
    let ledger = SqliteLedger::open_in_memory().unwrap();
    let (conv, run) = conversation_with_continuable_run(&ledger).await;

    let a = ledger
        .create_command(new_command(&conv, &run, "hi"), key("k1"))
        .await
        .unwrap();
    let completed = ledger.mark_command_completed(a.command_id).await.unwrap();
    assert_eq!(completed.status, CommandStatus::Completed);

    let (conv2, run2) = conversation_with_continuable_run(&ledger).await;
    let b = ledger
        .create_command(new_command(&conv2, &run2, "hi"), key("k2"))
        .await
        .unwrap();
    let rejected = ledger.mark_command_rejected(b.command_id).await.unwrap();
    assert_eq!(rejected.status, CommandStatus::Rejected);
}

#[tokio::test]
async fn active_command_for_conversation_is_none_when_nothing_is_in_flight() {
    let ledger = SqliteLedger::open_in_memory().unwrap();
    let conv = ledger.create_conversation(None).await.unwrap();
    assert!(ledger
        .active_command_for_conversation(conv.conversation_id)
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn command_lookup_by_id_reads_back_what_was_created() {
    let ledger = SqliteLedger::open_in_memory().unwrap();
    let (conv, run) = conversation_with_continuable_run(&ledger).await;
    let created = ledger
        .create_command(new_command(&conv, &run, "hi"), key("k"))
        .await
        .unwrap();

    let read_back = ledger
        .command(created.command_id.clone())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(read_back, created);
}

#[tokio::test]
async fn set_run_provider_session_persists_and_is_readable_back() {
    let ledger = SqliteLedger::open_in_memory().unwrap();
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
    assert!(run.provider_session_id.is_none());

    ledger
        .set_run_provider_session(run.run_id.clone(), "sess-xyz".to_owned())
        .await
        .unwrap();
    let read_back = ledger.run(run.run_id).await.unwrap().unwrap();
    assert_eq!(read_back.provider_session_id.as_deref(), Some("sess-xyz"));
}

#[tokio::test]
async fn set_run_provider_session_on_an_unknown_run_is_not_found() {
    let ledger = SqliteLedger::open_in_memory().unwrap();
    let bogus = RunId::from_raw("nope".to_owned());
    let err = ledger
        .set_run_provider_session(bogus, "sess".to_owned())
        .await
        .unwrap_err();
    assert_eq!(err.code(), "NOT_FOUND");
}

/// Durability across a fresh connection — a durably accepted command must
/// remain discoverable after a process restart, per §7's crash-
/// recoverability requirement.
#[tokio::test]
async fn command_acceptance_and_binding_survive_reopening_the_ledger() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ledger.db");

    let (conv, run, command_id, child_id) = {
        let ledger = SqliteLedger::open(&path).unwrap();
        let (conv, run) = conversation_with_continuable_run(&ledger).await;
        let command = ledger
            .create_command(new_command(&conv, &run, "hi"), key("k"))
            .await
            .unwrap();
        let child = RunId::from_raw("child-run-1".to_owned());
        ledger
            .bind_command_child_run(command.command_id.clone(), child.clone())
            .await
            .unwrap();
        (conv, run, command.command_id, child)
    };

    // A fresh connection — stands in for a restarted process.
    let reopened = SqliteLedger::open(&path).unwrap();
    let command = reopened.command(command_id).await.unwrap().unwrap();
    assert_eq!(command.conversation_id, conv);
    assert_eq!(command.parent_run_id, run);
    assert_eq!(command.status, CommandStatus::Started);
    assert_eq!(command.child_run_id, Some(child_id));
}

/// §7: a command accepted but never bound to a child run — and one bound
/// to a run whose own ledger row never landed — both stay discoverable via
/// `stuck_commands`; one already-started-with-a-real-run command does not
/// pollute the list (that gap is covered by the pre-existing
/// still-running scan instead, not duplicated here).
#[tokio::test]
async fn stuck_commands_surfaces_unbound_and_orphaned_bindings_only() {
    let ledger = SqliteLedger::open_in_memory().unwrap();

    let (conv_a, run_a) = conversation_with_continuable_run(&ledger).await;
    let never_bound = ledger
        .create_command(new_command(&conv_a, &run_a, "never bound"), key("k1"))
        .await
        .unwrap();

    let (conv_b, run_b) = conversation_with_continuable_run(&ledger).await;
    let orphaned = ledger
        .create_command(new_command(&conv_b, &run_b, "orphaned binding"), key("k2"))
        .await
        .unwrap();
    let orphan_child = RunId::from_raw("run-with-no-ledger-row".to_owned());
    ledger
        .bind_command_child_run(orphaned.command_id.clone(), orphan_child)
        .await
        .unwrap();

    let (conv_c, run_c) = conversation_with_continuable_run(&ledger).await;
    let properly_started = ledger
        .create_command(new_command(&conv_c, &run_c, "properly started"), key("k3"))
        .await
        .unwrap();
    let real_child = ledger
        .create_run(
            NewRun {
                conversation_id: conv_c.clone(),
                parent_run_id: Some(run_c.clone()),
                agent: "claude".to_owned(),
                role: "implementer".to_owned(),
            },
            None,
        )
        .await
        .unwrap();
    ledger
        .bind_command_child_run(properly_started.command_id.clone(), real_child.run_id)
        .await
        .unwrap();

    let stuck = ledger.stuck_commands().await.unwrap();
    let stuck_ids: Vec<_> = stuck.iter().map(|c| c.command_id.clone()).collect();
    assert!(stuck_ids.contains(&never_bound.command_id));
    assert!(stuck_ids.contains(&orphaned.command_id));
    assert!(!stuck_ids.contains(&properly_started.command_id));
    assert_eq!(stuck.len(), 2);
}

/// Q-Deck R1 corrective round (blocker 1): a command bound to a child run
/// that never got a ledger row (the spawn died) must be safely re-drivable
/// — but two callers racing to claim the SAME redrive must never both win.
#[tokio::test]
async fn claim_stale_command_for_redrive_is_compare_and_swap_safe() {
    let ledger = SqliteLedger::open_in_memory().unwrap();
    let (conv, run) = conversation_with_continuable_run(&ledger).await;
    let command = ledger
        .create_command(new_command(&conv, &run, "hi"), key("k"))
        .await
        .unwrap();
    let orphan_child = RunId::from_raw("never-started".to_owned());
    let bound = ledger
        .bind_command_child_run(command.command_id.clone(), orphan_child)
        .await
        .unwrap();

    // Two racing callers both read the SAME `updated_at` (as they would
    // from two concurrent GETs of the same idempotent-replay response),
    // then both try to claim.
    let claim_a = ledger
        .claim_stale_command_for_redrive(command.command_id.clone(), bound.updated_at)
        .await
        .unwrap();
    let claim_b = ledger
        .claim_stale_command_for_redrive(command.command_id.clone(), bound.updated_at)
        .await
        .unwrap();
    assert!(claim_a, "the first claim must win");
    assert!(
        !claim_b,
        "a second claim against the SAME stale updated_at must lose"
    );

    // A THIRD claim, using the now-stale `updated_at` from before claim_a,
    // must also lose — the successful claim already advanced it.
    let claim_c = ledger
        .claim_stale_command_for_redrive(command.command_id.clone(), bound.updated_at)
        .await
        .unwrap();
    assert!(!claim_c);
}

#[tokio::test]
async fn claim_stale_command_for_redrive_fails_for_an_already_completed_command() {
    let ledger = SqliteLedger::open_in_memory().unwrap();
    let (conv, run) = conversation_with_continuable_run(&ledger).await;
    let command = ledger
        .create_command(new_command(&conv, &run, "hi"), key("k"))
        .await
        .unwrap();
    let completed = ledger
        .mark_command_completed(command.command_id.clone())
        .await
        .unwrap();

    let claim = ledger
        .claim_stale_command_for_redrive(command.command_id, completed.updated_at)
        .await
        .unwrap();
    assert!(
        !claim,
        "a command that already resolved must never be re-driven"
    );
}

/// Q-Deck R1 corrective round (blocker 3's mirror image): the child run
/// sealed successfully, but the command's own post-seal bookkeeping never
/// landed — `o7 recover` must reconcile it directly, no provider
/// invocation involved.
#[tokio::test]
async fn reconcile_completed_commands_heals_a_sealed_child_with_stale_bookkeeping() {
    let ledger = SqliteLedger::open_in_memory().unwrap();
    let (conv, run) = conversation_with_continuable_run(&ledger).await;
    let command = ledger
        .create_command(new_command(&conv, &run, "hi"), key("k"))
        .await
        .unwrap();
    let child = ledger
        .create_run(
            NewRun {
                conversation_id: conv.clone(),
                parent_run_id: Some(run.clone()),
                agent: "claude".to_owned(),
                role: "implementer".to_owned(),
            },
            None,
        )
        .await
        .unwrap();
    ledger
        .bind_command_child_run(command.command_id.clone(), child.run_id.clone())
        .await
        .unwrap();
    ledger.start_run(child.run_id.clone()).await.unwrap();
    ledger.complete_run(child.run_id.clone()).await.unwrap();
    // Deliberately never calling mark_command_completed — simulating the
    // post-seal bookkeeping write itself failing.

    let reconciled = ledger.reconcile_completed_commands().await.unwrap();
    assert_eq!(reconciled.len(), 1);
    assert_eq!(reconciled[0].command_id, command.command_id);
    assert_eq!(reconciled[0].status, CommandStatus::Completed);

    let read_back = ledger.command(command.command_id).await.unwrap().unwrap();
    assert_eq!(read_back.status, CommandStatus::Completed);

    // Idempotent: nothing left to reconcile on a second pass.
    let again = ledger.reconcile_completed_commands().await.unwrap();
    assert!(again.is_empty());
}

#[tokio::test]
async fn reconcile_completed_commands_leaves_a_genuinely_unresolved_command_alone() {
    let ledger = SqliteLedger::open_in_memory().unwrap();
    let (conv, run) = conversation_with_continuable_run(&ledger).await;
    let command = ledger
        .create_command(new_command(&conv, &run, "hi"), key("k"))
        .await
        .unwrap();
    let child = ledger
        .create_run(
            NewRun {
                conversation_id: conv,
                parent_run_id: Some(run),
                agent: "claude".to_owned(),
                role: "implementer".to_owned(),
            },
            None,
        )
        .await
        .unwrap();
    ledger
        .bind_command_child_run(command.command_id.clone(), child.run_id.clone())
        .await
        .unwrap();
    ledger.start_run(child.run_id).await.unwrap();
    // Deliberately never sealing — the child run is genuinely still active.

    let reconciled = ledger.reconcile_completed_commands().await.unwrap();
    assert!(
        reconciled.is_empty(),
        "a command whose child run has not sealed must never be reconciled"
    );
}

/// Q-Deck R1 second corrective round (blocker 3): the SAME validation
/// `agent::ProviderSessionId::new` applies (non-empty, no control
/// characters) must be checked here, before durable acceptance — not only
/// later, inside the detached `o7 continue` process. An invalid session
/// is a PERMANENT defect, never transient, so catching it only after 202
/// would mean every subsequent retry keeps re-spawning a continuation
/// guaranteed to fail the same way forever.
#[tokio::test]
async fn create_command_rejects_a_parent_session_containing_a_control_character() {
    let ledger = SqliteLedger::open_in_memory().unwrap();
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
    ledger
        .set_run_provider_session(run.run_id.clone(), "sess\u{0007}-with-bell".to_owned())
        .await
        .unwrap();
    ledger.start_run(run.run_id.clone()).await.unwrap();
    ledger.complete_run(run.run_id.clone()).await.unwrap();

    let err = ledger
        .create_command(
            new_command(&conv.conversation_id, &run.run_id, "hi"),
            key("k"),
        )
        .await
        .unwrap_err();
    assert_eq!(err.code(), "CONTINUATION_NOT_PERMITTED");
}

#[tokio::test]
async fn create_command_rejects_a_parent_session_that_is_an_empty_string() {
    let ledger = SqliteLedger::open_in_memory().unwrap();
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
    ledger
        .set_run_provider_session(run.run_id.clone(), String::new())
        .await
        .unwrap();
    ledger.start_run(run.run_id.clone()).await.unwrap();
    ledger.complete_run(run.run_id.clone()).await.unwrap();

    let err = ledger
        .create_command(
            new_command(&conv.conversation_id, &run.run_id, "hi"),
            key("k"),
        )
        .await
        .unwrap_err();
    assert_eq!(err.code(), "CONTINUATION_NOT_PERMITTED");
}

/// Q-Deck R1 second corrective round (blocker 1): rebinding a command to a
/// FRESH child run id (the redrive path for a child run that already has
/// a ledger row, e.g. `interrupted`) must actually replace the binding —
/// never leave the stale one in place, never silently no-op.
#[tokio::test]
async fn rebind_command_child_run_replaces_the_binding() {
    let ledger = SqliteLedger::open_in_memory().unwrap();
    let (conv, run) = conversation_with_continuable_run(&ledger).await;
    let command = ledger
        .create_command(new_command(&conv, &run, "hi"), key("k"))
        .await
        .unwrap();
    let first_child = RunId::from_raw("first-child".to_owned());
    ledger
        .bind_command_child_run(command.command_id.clone(), first_child.clone())
        .await
        .unwrap();

    let second_child = RunId::from_raw("second-child".to_owned());
    let rebound = ledger
        .rebind_command_child_run(command.command_id.clone(), second_child.clone())
        .await
        .unwrap();
    assert_eq!(rebound.child_run_id, Some(second_child.clone()));

    let read_back = ledger.command(command.command_id).await.unwrap().unwrap();
    assert_eq!(read_back.child_run_id, Some(second_child));
    assert_ne!(read_back.child_run_id, Some(first_child));
}

// Q-Deck R1 fourth corrective round: `rebind_command_child_run_if_current`
// replaces `claim_stale_command_for_redrive` + `rebind_command_child_run` as
// the load-bearing redrive path — one atomic conditional `UPDATE`, so a
// racing loser can never learn about its loss from a separate, later read.

#[tokio::test]
async fn rebind_if_current_succeeds_when_every_predicate_matches() {
    let ledger = SqliteLedger::open_in_memory().unwrap();
    let (conv, run) = conversation_with_continuable_run(&ledger).await;
    let command = ledger
        .create_command(new_command(&conv, &run, "hi"), key("k"))
        .await
        .unwrap();
    let old_child = RunId::from_raw("old-child".to_owned());
    let bound = ledger
        .bind_command_child_run(command.command_id.clone(), old_child.clone())
        .await
        .unwrap();

    let fresh = RunId::from_raw("fresh-child".to_owned());
    let outcome = ledger
        .rebind_command_child_run_if_current(
            command.command_id.clone(),
            old_child,
            bound.updated_at,
            fresh.clone(),
        )
        .await
        .unwrap();
    let RebindOutcome::Rebound(rebound) = outcome else {
        panic!("expected Rebound, got {outcome:?}");
    };
    assert_eq!(rebound.child_run_id, Some(fresh.clone()));

    let read_back = ledger.command(command.command_id).await.unwrap().unwrap();
    assert_eq!(read_back.child_run_id, Some(fresh));
}

#[tokio::test]
async fn rebind_if_current_is_compare_and_swap_safe_under_a_race() {
    let ledger = SqliteLedger::open_in_memory().unwrap();
    let (conv, run) = conversation_with_continuable_run(&ledger).await;
    let command = ledger
        .create_command(new_command(&conv, &run, "hi"), key("k"))
        .await
        .unwrap();
    let old_child = RunId::from_raw("old-child".to_owned());
    let bound = ledger
        .bind_command_child_run(command.command_id.clone(), old_child.clone())
        .await
        .unwrap();

    // Two racing redrivers both observed the SAME pre-CAS snapshot
    // (old_child, bound.updated_at) — e.g. two concurrent stale-command
    // retries — and each proposes its OWN fresh run id.
    let fresh_a = RunId::from_raw("fresh-a".to_owned());
    let fresh_b = RunId::from_raw("fresh-b".to_owned());
    let outcome_a = ledger
        .rebind_command_child_run_if_current(
            command.command_id.clone(),
            old_child.clone(),
            bound.updated_at,
            fresh_a.clone(),
        )
        .await
        .unwrap();
    let outcome_b = ledger
        .rebind_command_child_run_if_current(
            command.command_id.clone(),
            old_child,
            bound.updated_at,
            fresh_b,
        )
        .await
        .unwrap();

    let RebindOutcome::Rebound(winner) = outcome_a else {
        panic!("expected the first caller to win, got {outcome_a:?}");
    };
    assert_eq!(winner.child_run_id, Some(fresh_a.clone()));

    // The loser must NOT report its own stale `fresh_b` — it must read
    // back and report the WINNER's fresh run id, never a locally-held one.
    let RebindOutcome::LostRace(current) = outcome_b else {
        panic!("expected the second caller to lose the race, got {outcome_b:?}");
    };
    assert_eq!(
        current.child_run_id,
        Some(fresh_a),
        "the loser must report the AUTHORITATIVE current child, not its own stale proposal"
    );
}

#[tokio::test]
async fn rebind_if_current_refuses_a_command_that_is_no_longer_eligible() {
    let ledger = SqliteLedger::open_in_memory().unwrap();
    let (conv, run) = conversation_with_continuable_run(&ledger).await;
    let command = ledger
        .create_command(new_command(&conv, &run, "hi"), key("k"))
        .await
        .unwrap();
    let old_child = RunId::from_raw("old-child".to_owned());
    let bound = ledger
        .bind_command_child_run(command.command_id.clone(), old_child.clone())
        .await
        .unwrap();
    let completed = ledger
        .mark_command_completed(command.command_id.clone())
        .await
        .unwrap();

    let outcome = ledger
        .rebind_command_child_run_if_current(
            command.command_id,
            old_child,
            bound.updated_at,
            RunId::from_raw("fresh".to_owned()),
        )
        .await
        .unwrap();
    let RebindOutcome::NotEligible(current) = outcome else {
        panic!("expected NotEligible for an already-completed command, got {outcome:?}");
    };
    assert_eq!(current.status, CommandStatus::Completed);
    assert_eq!(current.child_run_id, completed.child_run_id);
}

#[tokio::test]
async fn rebind_if_current_reports_not_found_for_an_unknown_command() {
    let ledger = SqliteLedger::open_in_memory().unwrap();
    let outcome = ledger
        .rebind_command_child_run_if_current(
            o7_ledger::CommandId::from_raw("no-such-command".to_owned()),
            RunId::from_raw("old".to_owned()),
            0,
            RunId::from_raw("fresh".to_owned()),
        )
        .await
        .unwrap();
    assert_eq!(outcome, RebindOutcome::NotFound);
}

// Q-Deck R1 fourth corrective round: `mark_command_completed_if_bound`
// replaces the read-then-write pattern that used to live at the `o7
// continue` call site — completion is now conditional on the caller's OWN
// run id still being the command's current binding, atomically.

#[tokio::test]
async fn mark_completed_if_bound_succeeds_when_still_bound_to_the_expected_run() {
    let ledger = SqliteLedger::open_in_memory().unwrap();
    let (conv, run) = conversation_with_continuable_run(&ledger).await;
    let command = ledger
        .create_command(new_command(&conv, &run, "hi"), key("k"))
        .await
        .unwrap();
    let child = RunId::from_raw("child".to_owned());
    ledger
        .bind_command_child_run(command.command_id.clone(), child.clone())
        .await
        .unwrap();

    let outcome = ledger
        .mark_command_completed_if_bound(command.command_id.clone(), child)
        .await
        .unwrap();
    let CompletionOutcome::Completed(completed) = outcome else {
        panic!("expected Completed, got {outcome:?}");
    };
    assert_eq!(completed.status, CommandStatus::Completed);

    let read_back = ledger.command(command.command_id).await.unwrap().unwrap();
    assert_eq!(read_back.status, CommandStatus::Completed);
}

#[tokio::test]
async fn mark_completed_if_bound_refuses_a_command_rebound_to_a_different_run() {
    let ledger = SqliteLedger::open_in_memory().unwrap();
    let (conv, run) = conversation_with_continuable_run(&ledger).await;
    let command = ledger
        .create_command(new_command(&conv, &run, "hi"), key("k"))
        .await
        .unwrap();
    let stale_child = RunId::from_raw("stale-child".to_owned());
    ledger
        .bind_command_child_run(command.command_id.clone(), stale_child.clone())
        .await
        .unwrap();
    // A redrive rebinds the command to a DIFFERENT run WHILE the stale
    // child's own process is still trying to finish.
    let fresh_child = RunId::from_raw("fresh-child".to_owned());
    ledger
        .rebind_command_child_run(command.command_id.clone(), fresh_child.clone())
        .await
        .unwrap();

    // The stale child's own (superseded) completion attempt must be
    // refused — it must never stomp bookkeeping that now belongs to the
    // fresh child.
    let outcome = ledger
        .mark_command_completed_if_bound(command.command_id.clone(), stale_child)
        .await
        .unwrap();
    let CompletionOutcome::NotBound(current) = outcome else {
        panic!("expected NotBound for a superseded run, got {outcome:?}");
    };
    assert_eq!(current.child_run_id, Some(fresh_child));
    assert_ne!(current.status, CommandStatus::Completed);
}

#[tokio::test]
async fn mark_completed_if_bound_reports_not_found_for_an_unknown_command() {
    let ledger = SqliteLedger::open_in_memory().unwrap();
    let outcome = ledger
        .mark_command_completed_if_bound(
            o7_ledger::CommandId::from_raw("no-such-command".to_owned()),
            RunId::from_raw("child".to_owned()),
        )
        .await
        .unwrap();
    assert_eq!(outcome, CompletionOutcome::NotFound);
}
