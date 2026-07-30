//! Q-Deck R0.5 production packaging smoke: seed a real on-disk SQLite
//! database with the canonical golden synthetic-run transcript, through
//! `o7-ledger`'s own production write API — never a hand-inserted row. This
//! is the exact same transcript shape documented in
//! `docs/q-deck/r05-live-readiness.md` and implemented for this crate's own
//! tests in `tests/support/mod.rs` (mirrored here, not shared via production
//! code — this example is not part of the library's public API surface, and
//! the transcript itself is not a real downstream seam, just a fixture).
//!
//! Usage: `cargo run --example seed_r05_fixture -- <db-path> [pass|fail|interrupted]`
//! (defaults to `pass` if the outcome argument is omitted.)

use o7_ledger::{EventId, EventType, Ledger as _, NewEvent, NewRun, SqliteLedger};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let db_path = args
        .next()
        .ok_or("usage: seed_r05_fixture <db-path> [pass|fail|interrupted]")?;
    let outcome = args.next().unwrap_or_else(|| "pass".to_owned());

    let ledger = SqliteLedger::open(&db_path)?;

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

    let run = match outcome.as_str() {
        "pass" => ledger.complete_run(run.run_id.clone()).await?,
        "fail" => ledger.fail_run(run.run_id.clone()).await?,
        "interrupted" => ledger.interrupt_run(run.run_id.clone()).await?,
        other => {
            return Err(format!("unknown outcome {other:?}, expected pass|fail|interrupted").into())
        }
    };

    println!("conversation_id={}", conversation.conversation_id.as_str());
    println!("run_id={}", run.run_id.as_str());
    Ok(())
}
