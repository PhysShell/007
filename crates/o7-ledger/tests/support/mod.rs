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
//!
//! ## The seam this transcript closed, and the one it still does NOT
//!
//! `crates/o7-run` has its own `Verdict` enum (`Pass`, `Fail`, `Blocked`,
//! `Error` — see `crates/o7-run/src/state.rs`), fixed only once a run is
//! `Sealed`. As of Q-Deck R0.6 (`docs/q-deck/r06-verdict-fidelity.md`),
//! `o7_ledger::RunStatus` has its own sealed `Blocked`/`Error`, matching
//! o7-run's own vocabulary by name — closing the gap that previously made it
//! impossible to project every sealed `Verdict` into the ledger. What R0.6
//! does NOT do: wire a real `o7-run`/`o7-worker` producer to actually call
//! `block_run`/`error_run` (o7d still doesn't depend on o7-run at all — see
//! `docs/q-deck/architecture.md`). `RunStatus::Interrupted` remains a
//! genuinely different, UNSEALED, resumable concept (`resume_interrupted_run`
//! can bring it back to `Running`) and must never be described as
//! terminal/sealed/"error" anywhere.

use o7_ledger::{
    Conversation, EventId, EventType, Ledger as _, LedgerError, NewEvent, NewRun, Run, SqliteLedger,
};

/// Which existing, already-canonical status the transcript ends in.
/// `Pass`/`Fail`/`Blocked`/`Error` are all sealed/terminal outcomes
/// (`RunStatus::Completed`/`Failed`/`Blocked`/`Error`). `Interrupted` is NOT
/// a co-equal "outcome" of the same kind — it is a resumable pause, not a
/// verdict; see the module doc comment above for why it is kept in this enum
/// anyway (for the convenience of one `apply_golden_transcript` entry point)
/// despite being a different kind of thing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GoldenOutcome {
    /// Sealed. Ends in `RunStatus::Completed`.
    Pass,
    /// Sealed. Ends in `RunStatus::Failed`.
    Fail,
    /// NOT sealed. Ends in `RunStatus::Interrupted`, which
    /// `RunStatus::is_terminal()` deliberately excludes — an interrupted run
    /// is resumable via `resume_interrupted_run` back to `Running`, so
    /// `run.finished_at` stays `None`. Real, existing ledger behavior this
    /// transcript surfaces honestly rather than treating as a fourth
    /// terminal outcome.
    Interrupted,
    /// Sealed. Ends in `RunStatus::Blocked` (R0.6).
    Blocked,
    /// Sealed. Ends in `RunStatus::Error` (R0.6) — NOT the same concept as
    /// `Interrupted`.
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
    // The 7th event's type depends on `GoldenOutcome` — see
    // `outcome_event_type`. Not necessarily "terminal": for `Interrupted` it
    // is a resumable pause, not a sealed outcome.
    "run.completed",
];

/// The 7th event's type for a given outcome — the one entry in
/// `EXPECTED_EVENT_TYPES` that varies.
#[must_use]
pub(crate) fn outcome_event_type(outcome: GoldenOutcome) -> &'static str {
    match outcome {
        GoldenOutcome::Pass => "run.completed",
        GoldenOutcome::Fail => "run.failed",
        GoldenOutcome::Interrupted => "run.interrupted",
        GoldenOutcome::Blocked => "run.blocked",
        GoldenOutcome::Error => "run.errored",
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
        GoldenOutcome::Interrupted => ledger.interrupt_run(run.run_id.clone()).await?,
        GoldenOutcome::Blocked => ledger.block_run(run.run_id.clone()).await?,
        GoldenOutcome::Error => ledger.error_run(run.run_id.clone()).await?,
    };

    Ok(GoldenTranscript {
        conversation,
        run,
        outcome,
    })
}
