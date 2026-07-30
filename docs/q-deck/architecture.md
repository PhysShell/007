# Q-Deck architecture (R0 — Observe)

## What this is

Q-Deck is a mobile-first, read-only web view of 007 agent runs. It exists so
the owner can check in on a run from a phone without SSH or tmux. R0 is
**observation only** — no run creation, no start/stop/approve/reject.

## Component boundaries

```
crates/o7-run       canonical run-event protocol + pure reducer (unchanged by Q-Deck)
crates/o7-ledger    SQLite + cursor replay, the durable source of truth
crates/o7-worker    generic process runtime (unchanged by Q-Deck)
crates/o7d          the daemon: composes o7-ledger's read APIs into HTTP/SSE
apps/q-deck         static Svelte+TS PWA; an UNTRUSTED CLIENT of o7d
```

`o7d` is not a new role invented for this milestone — it is already named as
the intended control-plane daemon in `o7-worktree`'s, `o7-verifier`'s, and
`o7-worker`'s own doc comments (trust store owner, worktree lifecycle owner,
verdict authority). Building it here is the first thing to actually
instantiate it, wiring already-built, previously-disconnected crates.

## Source-of-truth rule

Every value Q-Deck displays is derived from `o7-ledger` through `o7d`'s typed
DTOs (`crates/o7d/src/dto.rs`) — never invented client-side, never a second
event store, reducer, or cache that could disagree with the ledger. `o7d`
itself keeps the SQLite connection, subprocess handles, and any control
socket entirely on its own side of the HTTP boundary; the browser never
receives anything but JSON and SSE text frames.

## A known gap this milestone does NOT close

As of this work, **nothing writes real production run data into
`o7-ledger`.** The existing `o7 run` CLI (the repo's only currently-wired
binary) is synchronous and writes to flat files (`runs/<target>/<run-id>/...`
via `src/record.rs`) — it does not use `o7-ledger` or `o7-worker` at all. The
`o7-run` → `o7-ledger` append path, and the `RunId` type unification between
the two crates, are both explicitly deferred to a later slice (see
`TODO.md`). Q-Deck reads exactly what `o7-ledger` holds, which today means:
real data in tests (populated through the ledger's own public write APIs),
and an empty dashboard against a production ledger until that separate,
already-tracked gap is closed. This is a corpus/wiring question, not
something Q-Deck should paper over by inventing its own data path.

## Cursor / reconnect semantics

- `o7-ledger`'s only ordering primitive is a **per-conversation monotonic
  `sequence: u64`**, gap-free by construction (allocated inside one
  `BEGIN IMMEDIATE` transaction). There is no global cursor.
- REST pagination (`/conversations`, `/runs`) uses a separate keyset cursor —
  `(created_at, rowid)` — because listing rows have no natural sequence
  counter of their own. `rowid` (not the entity's UUID) is the tiebreak for
  same-millisecond rows, because `rowid` is genuinely monotonic by insertion
  order for these append-only, never-deleted tables; a UUID tiebreak would
  order same-millisecond ties in a lexicographic order unrelated to when the
  rows actually arrived.
- The SSE stream's cursor is the *same* per-conversation `sequence` the REST
  events endpoint uses — one cursor concept everywhere, not a REST-shaped one
  and a differently-shaped stream one.
- `Last-Event-ID` (the browser `EventSource`'s own automatic reconnect
  signal) always wins over a client-supplied `?after=` query param, because
  it is guaranteed fresh — a `?after=` value only travels correctly if the
  caller updates it in lockstep with every event received, which is exactly
  the bookkeeping `Last-Event-ID` exists so a client does not have to do.

## Why SSE, not WebSocket

R0's data flow is one-way: server → client. `EventSource`/SSE gives cursor
resume (`Last-Event-ID`) for free, degrades gracefully through HTTP proxies,
and needs no new dependency edge beyond what serving JSON already requires.
A duplex protocol would add complexity (framing, ping/pong, a second
connection-state machine) to solve a problem R0 does not have.

## Why polling the ledger, not a broadcast channel

`o7-ledger` has no pub/sub mechanism today — it is purely pull-based
(`read_events(after_sequence)`). The SSE handler is a bounded poll loop
(~750ms) over that same call, not an in-memory broadcast: a broadcast channel
would lose events emitted before a subscriber existed, or between one
client's disconnect and its reconnect, and re-deriving the loss from the
ledger anyway — so there would be two sources of truth to keep in sync for no
benefit. Polling the ledger directly means there is only ever one.

## Security / network assumptions (R0)

- `o7d` binds `127.0.0.1` by default; a non-loopback bind requires the
  explicit `--allow-non-loopback` flag.
- No public-internet authentication model exists in R0. Deployment is
  expected behind the owner's own private network (WireGuard/Tailscale).
- No wildcard CORS, no CDN assets, no externally-fetched JavaScript, no
  telemetry.
- Errors returned to the client carry a stable machine `code`
  (`o7_ledger::LedgerError::code()`) but never the underlying `Display` text,
  which can contain raw SQLite error strings — kept off the wire even on a
  private network, as a cheap defense-in-depth measure.

## R0 non-goals (explicit)

No run creation, stop/cancel, approvals, follow-up prompts,
provider/model selection, terminal emulation, in-browser shell, file
editing, diff/artifact rendering beyond simple metadata, public
authentication, multi-user authorization, push notifications, WebSocket, or
any VB-4 (sandboxing milestone) changes.

## Later: the mutation model (not implemented)

Q-Deck R1 ("Command") is expected to add start/stop/approve/reject/follow-up.
The natural shape, not yet built: `o7d` would gain write endpoints that call
into `o7-ledger`'s existing lifecycle methods (`start_run`, `cancel_run`,
etc. — already implemented and tested, just not exposed over HTTP), each
requiring an explicit mutation-scoped auth story R0 deliberately does not
attempt. R1 should not begin until R0's read path has been proven reliable
across a real mobile-network disconnect/reconnect, in production — a pretty
"stop" button wired to nothing trustworthy is worse than no button.

**R0.6 ("canonical verdict fidelity", `docs/q-deck/r06-verdict-fidelity.md`)
added `RunStatus::Blocked`/`Error` (sealed) alongside the existing
`Completed`/`Failed`/`Cancelled` (sealed) and `Interrupted` (unsealed,
resumable) — closing the semantic gap that previously made it impossible
for a live `o7-run` producer to project all four of its own sealed verdicts
(`Pass`/`Fail`/`Blocked`/`Error`) without collapsing meaning. The next slice
after R0.6 is wiring that real live producer — not R1.**
