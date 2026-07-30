//! Shared golden synthetic-run transcript builder for Q-Deck R0.5's
//! live-readiness proof. This is o7d's own copy of the SAME canonical
//! transcript shape documented in `docs/q-deck/r05-live-readiness.md` and
//! implemented for o7-ledger's own tests in
//! `crates/o7-ledger/tests/support/mod.rs` — mirrored rather than shared via
//! production code (a test-only helper is not a real downstream seam, so it
//! is not worth adding to `o7-ledger`'s public API just to share it across
//! crates). Keep both copies in sync if this transcript's shape ever
//! changes.
//!
//! Every payload is marked `"synthetic": true` with a `"source"` tag so
//! nothing written by this fixture can ever be mistaken for real provider
//! output — this transcript proves the ledger/o7d/Q-Deck downstream
//! contract, it does not stand in for a production provider integration.

use o7_ledger::{
    Conversation, EventId, EventType, Ledger as _, LedgerError, NewEvent, NewRun, Run, SqliteLedger,
};

/// Which existing, already-canonical terminal state the transcript ends in.
/// See `o7-ledger`'s copy of this type for the full rationale: nothing new
/// is invented, ERROR (`Interrupted`) is never collapsed into FAIL
/// (`Failed`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GoldenOutcome {
    Pass,
    Fail,
    Error,
}

/// Everything the transcript produced, for callers to assert against.
pub(crate) struct GoldenTranscript {
    pub conversation: Conversation,
    pub run: Run,
}

/// The `event_type` strings this transcript produces, in the exact order
/// they are emitted (the 7th/terminal entry varies — see
/// `terminal_event_type`).
pub(crate) const EXPECTED_EVENT_TYPES: [&str; 7] = [
    "conversation.created",
    "run.created",
    "run.started",
    "user.message",
    "user.message",
    "system.note",
    "run.completed",
];

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

    Ok(GoldenTranscript { conversation, run })
}
