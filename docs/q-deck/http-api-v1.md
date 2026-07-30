# o7d HTTP/SSE API — v1

Every non-SSE endpoint responds with JSON carrying a top-level
`schema_version` (currently `1`). Q-Deck's client
(`apps/q-deck/src/lib/api.ts`) rejects any response whose `schema_version` it
doesn't recognize rather than guessing at its shape. The one SSE endpoint
(`/events/stream`, below) is `text/event-stream`, not JSON — each of its
frames carries its own embedded JSON payload with the same `schema_version`
field, checked the same way (`checkSchema`, shared with the REST path) before
being accepted.

Every DTO here is defined once, in `crates/o7d/src/dto.rs`, and derived from
an `o7-ledger` domain type — never a raw pass-through of it.

## Static shell (production)

`o7d serve --static-dir <apps/q-deck/dist>` serves the built Q-Deck shell
same-origin with this API: `/api/v1/*` (below) always wins; anything else is
looked up as a file under `--static-dir`, and anything that isn't a real file
there falls back to `index.html` — a client-side route like `/runs/abc123`
has no file of its own, so the shell loads and Q-Deck's own router (not
o7d's) resolves the path. Omitting `--static-dir` serves the API alone — the
dev-mode setup, where Vite's own dev server serves the shell and proxies
`/api` to this process instead (see `apps/q-deck/vite.config.ts`).

## Errors

Any non-2xx response body:

```json
{ "schema_version": 1, "error": "human-readable message", "code": "STABLE_CODE" }
```

`code` is one of `NOT_FOUND` (404), `BAD_REQUEST` (400 — a malformed cursor
or limit), or an `o7_ledger::LedgerError::code()` value (500 — a storage
failure). A storage failure is always reported as an error, never silently
reshaped into an empty list — "unknown data" and "the ledger is broken" are
different claims and must not read the same on the wire.

## `GET /api/v1/health`

```json
{ "schema_version": 1, "status": "ok" }
```

## `GET /api/v1/conversations`

Query params: `limit` (default 50), `before` (opaque cursor from a previous
page's `next_before`).

```json
{
  "schema_version": 1,
  "items": [
    { "schema_version": 1, "conversation_id": "...", "created_at": 0, "status": "open" }
  ],
  "next_before": "1753900000123.42"
}
```

Newest first. `next_before` is a `created_at.rowid` pair (see
`architecture.md`'s cursor section) — opaque to the client; pass it back
verbatim as `before` to page further into history. **A populated page's
`next_before` is not itself proof there are more rows** — keep paging until a
page comes back with an empty `items` array, exactly as
`o7-ledger::SqliteLedger::list_conversations` documents.

## `GET /api/v1/conversations/{conversation_id}`

The single conversation, or 404.

## `GET /api/v1/conversations/{conversation_id}/events`

Query params: `after` (a `sequence` number; strictly-greater-than, `None`
starts at the beginning), `limit` (default 200).

```json
{
  "schema_version": 1,
  "items": [
    {
      "schema_version": 1,
      "event_id": "...",
      "conversation_id": "...",
      "run_id": "...|null",
      "attempt_id": "...|null",
      "sequence": 1,
      "event_type": "run.started",
      "created_at": 0,
      "payload": {}
    }
  ],
  "next_after": 7
}
```

Ascending, gap-free (this is `o7-ledger`'s own `sequence` guarantee, not
re-derived here). An unknown `conversation_id` is `404` — the parent
resource's existence is checked before reading its events, the same contract
`get_conversation`/`get_run` already have.

## `GET /api/v1/conversations/{conversation_id}/events/stream`

Server-Sent Events. Each event frame:

```
id: 7
data: {"schema_version":1,"event_id":"...","sequence":7,...}

```

Plus periodic `: heartbeat` comment lines (no `id`/`data`) so a mobile proxy
does not treat an idle connection as dead.

**Cursor**: `Last-Event-ID` header (sent automatically by the browser's
`EventSource` on every reconnect) wins over a `?after=` query param if both
are present. Use `?after=` only for a fresh connection's initial position —
seed it from the last `sequence` of a REST history fetch, since `EventSource`
has no API to set a custom `Last-Event-ID` on its very first connection.

**Correctness**: this is a bounded poll (~750ms) over `o7-ledger::read_events`
— not a broadcast — so reconnecting with `Last-Event-ID: N` always replays
everything after `N` from the ledger itself, with no reliance on the server
process having been continuously running or holding any per-client buffer.
A storage error ends the stream (the client's `EventSource` retries the
whole connection on its own); it is never silently swallowed as "no more
events right now." An unknown `conversation_id` is `404`, checked before the
poll loop starts — never a `200` connection that heartbeats forever with
nothing behind it.

## `GET /api/v1/runs`

Query params: `limit` (default 50), `before` (opaque cursor), `conversation_id`
(optional — omit for the global "all runs" dashboard view, set it to scope to
one conversation's runs, e.g. for the conversation page), `status` (optional,
comma-separated `RunStatus` values, e.g. `?status=queued,running` — a real
database-level filter via `o7-ledger::SqliteLedger::list_runs`'s `statuses`
parameter, not something the caller approximates by paging and hoping a
bounded page happened to contain everything with that status). An
unrecognized status value is `400`.

Same `PageDto` shape as `/conversations`, with `RunDto` items:

```json
{
  "schema_version": 1,
  "run_id": "...",
  "conversation_id": "...",
  "parent_run_id": "...|null",
  "agent": "claude",
  "role": "implementer",
  "status": "queued|running|completed|failed|cancelled|interrupted|blocked|error",
  "created_at": 0,
  "finished_at": 0
}
```

`blocked`/`error` (Q-Deck R0.6, `docs/q-deck/r06-verdict-fidelity.md`) are
sealed — `finished_at` is set, same as `completed`/`failed`/`cancelled`.
`interrupted` remains the one non-sealed value in this set (`finished_at` is
`null`) — it is a resumable pause, not a verdict, and must never be conflated
with `error`. Adding these two values was an ADDITIVE change to this field —
`schema_version` did not bump, following the same forward-compatibility
policy `event_type` already established (see the events endpoint above): an
unrecognized status string is a client's own problem to degrade gracefully
on, not a wire-shape break.

## `GET /api/v1/runs/{run_id}`

The single run, or 404.
