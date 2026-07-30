//! Shared golden synthetic-run transcript builder for Q-Deck R0.5's
//! live-readiness proof. The canonical shape of this transcript is documented
//! once in `docs/q-deck/r05-live-readiness.md`; this module is a *replay* of
//! that shape through `o7-ledger`'s own production write API — never a
//! hand-inserted row, never a bypass of the normal append path. It is
//! mirrored (not shared via production code, since a test-only helper is not
//! a real downstream seam) in `crates/o7d/tests/support.rs` for o7d's own
//! REST/SSE proof — keep both in sync if this transcript's shape ever
//! changes.
//!
//! Every payload is marked `"synthetic": true` with a `"source"` tag so nothing
//! written by this fixture can ever be mistaken for real provider output —
//! this transcript proves the ledger/o7d/Q-Deck downstream contract, it does
//! not stand in for a production provider integration.

use o7_ledger::{
    Conversation, EventId, EventType, Ledger as _, LedgerError, NewEvent, NewRun, Run, SqliteLedger,
};

/// Which existing, already-canonical terminal state the transcript ends in.
/// Not a new status: R0.5 borrows `o7_ledger::RunStatus`'s own existing
/// distinction between an assessed failure (`failed`) and an involuntary
/// abort (`interrupted`) to stand in for a "FAIL" vs "ERROR" outcome, exactly
/// because the two are already canonically different in production code —
/// nothing new is invented, and ERROR is never collapsed into FAIL.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GoldenOutcome {
    /// Ends in `RunStatus::Completed`.
    Pass,
    /// Ends in `RunStatus::Failed`.
    Fail,
    /// Ends in `RunStatus::Interrupted`. NOTE: `RunStatus::is_terminal()`
    /// deliberately does NOT include `Interrupted` (an interrupted run is
    /// resumable via `resume_interrupted_run` — it is not a closed state the
    /// way completed/failed/cancelled are), so `run.finished_at` stays
    /// `None` for this outcome. That is real, existing ledger behavior this
    /// transcript surfaces honestly rather than papering over.
    Error,
}

/// Everything the transcript produced, for callers to assert against.
pub(crate) struct GoldenTranscript {
    pub conversation: Conversation,
    pub run: Run,
    pub outcome: GoldenOutcome,
}

/// The `event_type` strings this transcript produces, in the exact order
/// they are emitted. Shared by every proof test that checks ordering so the
/// expected shape is written down in exactly one place per crate.
pub(crate) const EXPECTED_EVENT_TYPES: [&str; 7] = [
    "conversation.created",
    "run.created",
    "run.started",
    "user.message",
    "user.message",
    "system.note",
    // The 7th (terminal) event's type depends on `GoldenOutcome` — see
    // `terminal_event_type`.
    "run.completed",
];

/// The terminal event type for a given outcome — the one entry in
/// `EXPECTED_EVENT_TYPES` that varies.
#[must_use]
pub(crate) fn terminal_event_type(outcome: GoldenOutcome) -> &'static str {
    match outcome {
        GoldenOutcome::Pass => "run.completed",
        GoldenOutcome::Fail => "run.failed",
        GoldenOutcome::Error => "run.interrupted",
    }
}

/// Replay the canonical golden synthetic-run transcript through
/// `o7-ledger`'s own production write API. Produces exactly one conversation
/// and one run with a full, currently-supported observable lifecycle:
/// `conversation.created -> run.created -> run.started -> user.message x2 ->
/// system.note -> run.{completed,failed,interrupted}` — 7 events, sequence
/// 1..=7.
///
/// # Errors
/// Propagates any `LedgerError` from the underlying production calls.
pub(crate) async fn apply_golden_transcript(
    ledger: &SqliteLedger,
    outcome: GoldenOutcome,
) -> Result<GoldenTranscript, LedgerError> {
    let conversation = ledger.create_conversation(None).await?;

    let run = ledger
        .create_run(
            NewRun {
                conversation_id: conversation.conversation_id.clone(),
                parent_run_id: None,
                agent: "claude".to_owned(),
                role: "implementer".to_owned(),
            },
            None,
        )
        .await?;

    ledger.start_run(run.run_id.clone()).await?;

    ledger
        .append_user_message(
            conversation.conversation_id.clone(),
            serde_json::json!({
                "text": "synthetic: implement the requested change",
                "synthetic": true,
                "source": "q-deck-r05-golden-transcript",
            }),
            Some(run.run_id.clone()),
            None,
        )
        .await?;

    ledger
        .append_user_message(
            conversation.conversation_id.clone(),
            serde_json::json!({
                "text": "synthetic: follow-up clarification",
                "synthetic": true,
                "source": "q-deck-r05-golden-transcript",
            }),
            Some(run.run_id.clone()),
            None,
        )
        .await?;

    ledger
        .append_event(NewEvent {
            event_id: EventId::generate(),
            conversation_id: conversation.conversation_id.clone(),
            run_id: Some(run.run_id.clone()),
            attempt_id: None,
            event_type: EventType::SystemNote,
            schema_version: o7_ledger::EVENT_SCHEMA_VERSION,
            payload: serde_json::json!({
                "note": "synthetic: progress checkpoint",
                "synthetic": true,
                "source": "q-deck-r05-golden-transcript",
            }),
        })
        .await?;

    let run = match outcome {
        GoldenOutcome::Pass => ledger.complete_run(run.run_id.clone()).await?,
        GoldenOutcome::Fail => ledger.fail_run(run.run_id.clone()).await?,
        GoldenOutcome::Error => ledger.interrupt_run(run.run_id.clone()).await?,
    };

    Ok(GoldenTranscript {
        conversation,
        run,
        outcome,
    })
}
