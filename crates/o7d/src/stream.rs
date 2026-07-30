//! `GET /api/v1/conversations/:id/events/stream` — cursor-resumable SSE.
//!
//! `o7-ledger` has no pub/sub (see the Q-Deck R0 implementation map): the only
//! source of truth is `read_events(after_sequence)`. So this stream is a
//! bounded poll loop over that same call, not an in-memory broadcast — a
//! broadcast channel would lose events emitted before a client subscribed, or
//! between one client's disconnect and its reconnect, and RE-DERIVING what was
//! missed from the ledger anyway. Polling the ledger directly means there is
//! only ever one source of truth to get right.

use std::collections::VecDeque;
use std::convert::Infallible;
use std::time::Duration;

use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::response::sse::{Event, KeepAlive, Sse};
use futures_util::stream::{self, Stream};
use o7_ledger::{ConversationId, Ledger as _, PersistedEvent, SqliteLedger};

use crate::dto::{EventDto, EventsParams};
use crate::error::ApiError;
use crate::state::AppState;

/// How long an idle poll waits before asking the ledger again. Bounded and
/// short enough that a mobile client sees a new event promptly; long enough
/// that an idle conversation doesn't spin the ledger's connection mutex.
const POLL_INTERVAL: Duration = Duration::from_millis(750);

/// Rows fetched per poll. Same order of magnitude as the REST events page —
/// this is a live tail, not a bulk-history read.
const STREAM_BATCH: usize = 200;

struct PollState {
    ledger: SqliteLedger,
    conversation_id: ConversationId,
    cursor: Option<u64>,
    buffered: VecDeque<PersistedEvent>,
    batch: usize,
}

pub(crate) async fn conversation_events_stream(
    State(state): State<AppState>,
    Path(conversation_id): Path<String>,
    Query(params): Query<EventsParams>,
    headers: HeaderMap,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, ApiError> {
    let id = ConversationId::from_raw(conversation_id);
    // Same existence contract as the REST events endpoint: an unknown
    // conversation is 404, not a 200 connection that heartbeats forever with
    // nothing behind it.
    state
        .ledger
        .conversation(id.clone())
        .await?
        .ok_or(ApiError::NotFound)?;

    // `Last-Event-ID` wins over `?after=` when both are present: it is the
    // browser EventSource's OWN reconnect signal, sent automatically on every
    // reconnect attempt and always describing what THIS client last actually
    // received. `?after=` is only a first-connect convenience a caller sets by
    // hand; on a reconnect it would be stale unless the caller updated it in
    // lockstep with every event received, which is exactly the bookkeeping
    // `Last-Event-ID` exists so a client does not have to do.
    let cursor = headers
        .get("last-event-id")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
        .or(params.after);

    // `?limit=` sets the per-poll batch size, clamped to STREAM_BATCH: it is
    // a caller-tunable knob (a slow client may want smaller catch-up bursts
    // after a long reconnect gap), never accepted-but-silently-ignored, and
    // never large enough to turn one poll into a bulk-history read.
    let batch = params
        .limit
        .map_or(STREAM_BATCH, |l| l.clamp(1, STREAM_BATCH));

    let initial = PollState {
        ledger: state.ledger,
        conversation_id: id,
        cursor,
        buffered: VecDeque::new(),
        batch,
    };

    let events = stream::unfold(initial, |mut st| async move {
        loop {
            if let Some(persisted) = st.buffered.pop_front() {
                st.cursor = Some(persisted.sequence);
                let dto = EventDto::from(persisted);
                let payload = match serde_json::to_string(&dto) {
                    Ok(json) => json,
                    // A DTO of plain strings/numbers/an already-valid JSON
                    // Value essentially cannot fail to serialize; if it ever
                    // does, end the stream rather than emit a corrupt frame
                    // under a sequence number nothing else will ever repeat.
                    Err(_) => return None,
                };
                let event = Event::default().id(dto.sequence.to_string()).data(payload);
                return Some((Ok(event), st));
            }

            match st
                .ledger
                .read_events(&st.conversation_id, st.cursor, st.batch)
                .await
            {
                Ok(fresh) if fresh.is_empty() => {
                    tokio::time::sleep(POLL_INTERVAL).await;
                    // A comment line, not a `data:` event: heartbeats keep a
                    // mobile proxy from treating the connection as idle-dead
                    // without ever surfacing as a phantom event to the client.
                    return Some((Ok(Event::default().comment("heartbeat")), st));
                }
                Ok(fresh) => {
                    st.buffered.extend(fresh);
                    // loop: pop the first of what was just buffered
                }
                // The ledger itself failed. Ending the stream here (rather
                // than looping forever against a broken connection, or
                // silently pretending nothing happened) matches "storage
                // errors do not masquerade as empty data" for the poll path
                // too — the client's `EventSource` will retry the whole
                // connection, which re-opens the ledger call fresh.
                Err(_) => return None,
            }
        }
    });

    Ok(Sse::new(events).keep_alive(KeepAlive::default()))
}
