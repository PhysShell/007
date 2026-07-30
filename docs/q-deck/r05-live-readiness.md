# Q-Deck R0.5 — synthetic live-run readiness

## What this slice proves, and what it deliberately does not

R0 (`docs/q-deck/architecture.md`) shipped the downstream read path —
`o7-ledger` → `o7d` REST/SSE → Q-Deck — but flagged a known gap: **nothing
writes real production run data into `o7-ledger`.** The `o7-run` → `o7-ledger`
append path is a separate, still-deferred piece of work.

R0.5 does not close that gap. It proves the downstream contract is solid
*independent of* who eventually writes into it, using a deterministic
synthetic transcript applied through `o7-ledger`'s own production write API —
so a future live-provider vertical only has to get events INTO the ledger
correctly; the ledger → o7d → Q-Deck path is already proven end to end,
including restart and reconnect.

**This is not a production provider integration.** Nothing here changes
`crates/o7-run`, `crates/o7-worker`, Sandboy/ProcessBoundary, or the canonical
`RunEvent`/`RunEventKind` reducer protocol those crates own — that is a
completely separate event vocabulary or a completely different concern (see
below), untouched.

## Canonical event vocabulary used

Q-Deck's entire downstream contract runs on `o7-ledger::EventType` — the
closed PR-1 event set: `conversation.created`, `run.created`, `run.started`,
`run.completed`, `run.failed`, `run.cancelled`, `run.interrupted`,
`user.message`, `system.note` — plus the `Run`/`Conversation`/`PersistedEvent`
models and the lifecycle-transition methods (`start_run`, `complete_run`,
`fail_run`, `cancel_run`, `interrupt_run`) that are the SOLE authority for
`Run.status`, validated centrally by `o7-ledger::transitions`. `o7d` does not
depend on `o7-run` at all (`crates/o7d/Cargo.toml` has no such dependency) —
`o7-run`'s own `RunEvent`/`RunEventKind`/`reduce_all`/`Verdict` types are a
separate reducer for the gate/policy/sandbox-evidence pipeline, unrelated to
what Q-Deck ever sees. Nothing new was invented here; R0.5 uses exactly the
event vocabulary and write API R0 already established.

## The golden transcript

One synthetic run, applied through `o7-ledger`'s production write API
(`create_conversation` → `create_run` → `start_run` →
`append_user_message` ×2 → `append_event(SystemNote)` → one terminal
transition), producing exactly 7 events, sequence 1–7:

| seq | event_type | note |
|-----|------------|------|
| 1 | `conversation.created` | `run_id: null` |
| 2 | `run.created` | |
| 3 | `run.started` | |
| 4 | `user.message` | `payload.synthetic: true` |
| 5 | `user.message` | `payload.synthetic: true` |
| 6 | `system.note` | `payload.synthetic: true` |
| 7 | `run.{completed,failed,interrupted}` | outcome-dependent |

Every payload carries `"synthetic": true` and `"source":
"q-deck-r05-golden-transcript"` so nothing this fixture writes can ever be
mistaken for real provider output. No secrets, no unbounded model output —
every payload is a short, fixed, human-written string.

### PASS / FAIL / ERROR mapping

The task asked for PASS/FAIL/ERROR terminal variants **only if the current
code already canonically distinguishes them** — it does, so no new status was
invented:

- **PASS** → `RunStatus::Completed`
- **FAIL** → `RunStatus::Failed`
- **ERROR** → `RunStatus::Interrupted`

`Interrupted` is the existing status for an involuntary abort, genuinely
distinct from an assessed `Failed` verdict — using it as "ERROR" doesn't
collapse ERROR into FAIL or invent a fourth state. One real, load-bearing
consequence of this choice, surfaced honestly rather than papered over:
`RunStatus::is_terminal()` deliberately excludes `Interrupted` (it is
resumable via `resume_interrupted_run`), so **`finished_at` stays `None` for
the ERROR outcome** — proven at every layer (ledger, REST, SSE, frontend)
rather than backfilled to look like a closed run.

## Where the transcript lives

Three copies of the same replay logic, deliberately NOT unified into a shared
production dependency (a test-only helper is not a real downstream seam, so
adding one to `o7-ledger`'s public API just to share it across crates/tests
would be scope creep):

- `crates/o7-ledger/tests/support/mod.rs` — used by `tests/golden_transcript.rs`
- `crates/o7d/tests/support/mod.rs` — used by `tests/golden_transcript_rest.rs` and `tests/golden_transcript_sse.rs`
- `crates/o7-ledger/examples/seed_r05_fixture.rs` — used by the production packaging smoke (a real on-disk SQLite file, seeded via `cargo run --example seed_r05_fixture -- <path> [pass|fail|error]`)
- `apps/q-deck/src/test-support/goldenTranscript.ts` — typed DTO fixtures mirroring the same shape for Q-Deck's own component tests

All four encode the exact same 7-event shape above. If this transcript's
shape ever changes, all four must change together.

## Two real bugs this slice found (in already-frozen R0 code)

Neither `RunPage.svelte` nor `ConversationPage.svelte` had ever been rendered
by any automated test before R0.5 — no test file for either existed. Writing
the first ones surfaced a real, previously-invisible bug in both: each
component's `$effect` synchronously **read** its own `stream` (and, for
`RunPage`, `timer`) state purely to tear it down at the top of the effect
(`stream?.close(); stream = ...`). Reading a piece of `$state` inside an
effect's own body makes it a tracked dependency of that SAME effect; a later
write to that same state (`RunPage`: asynchronously, inside
`getRun(...).then(...)`; `ConversationPage`: synchronously, the very next
statement) then retriggers the same effect — which tears down and reopens the
stream/timer, calls `getRun`/lists runs again, and writes the same tracked
state again, forever. Confirmed by CPU time and RSS climbing without bound in
an isolated repro, and by the fix eliminating both immediately.

**Fix**: both effects already had a correct `return () => {...}` cleanup
closure that closes the previous stream/timer at exactly the right time
(before the effect reruns for a new id, and on unmount) — the redundant
top-of-effect read-then-teardown was deleted, leaving only plain (unread)
writes in the synchronous body. Zero behavior change other than removing the
infinite loop, verified by the full frontend suite passing (32/32) and a
targeted before/after repro. Fixed forward on this new branch, not amended
into PR #83's frozen history.

## What R0.5 proves, layer by layer

- **Ledger** (`crates/o7-ledger/tests/golden_transcript.rs`): events read back
  in original order and byte-identically on repeated reads; run metadata
  (agent/role/status/finished_at) matches exactly what the transition methods
  wrote (never recomputed); a close-then-reopen of the same on-disk file
  preserves everything.
- **o7d REST** (`crates/o7d/tests/golden_transcript_rest.rs`): the run is
  listed and reachable by id, the conversation is reachable by id, events
  page correctly under a small `limit` with no loss via `next_after`,
  terminal status/timestamps match the ledger exactly (including ERROR's
  absent `finished_at`), and an id adjacent to a real one still 404s.
- **o7d SSE** (`crates/o7d/tests/golden_transcript_sse.rs`): connect → receive
  part of the transcript → real TCP disconnect → reconnect with
  `Last-Event-ID` → receive exactly the missed tail, no gap, no duplicate;
  AND a real restart of the daemon PROCESS (not just the client) against the
  same on-disk file resumes correctly.
- **Q-Deck** (`RunPage.svelte.test.ts`, `ConversationPage.svelte.test.ts`, plus
  additions to `Dashboard.svelte.test.ts` and `eventStream.svelte.test.ts`):
  active/terminal rendering (including the ERROR/no-duration case),
  this-run-only vs. whole-conversation timeline filtering, full run-history
  pagination (previously untested), resilience to a transient refresh
  failure, an `unsupported`-schema SSE frame closing the stream without
  being treated as a normal event, and the absence of any mutation control.
- **Production packaging smoke**: a real on-disk SQLite file seeded via
  `cargo run --example seed_r05_fixture`, served by the real,
  unmodified-except-for-this-slice `o7d serve --static-dir dist` binary —
  `GET /` and an unmatched client route both return the shell;
  `GET /api/v1/health`, `GET /api/v1/runs/{id}` return real data;
  `GET /api/v1/does-not-exist` returns the JSON 404, never the shell; and an
  SSE reconnect with `Last-Event-ID` resumes correctly against the real
  running process.

## Still blocked on the live-provider vertical

R0.5 changes nothing about the actual gap: something still has to call
`o7-ledger`'s write API from a real running agent (`o7-run`/`o7-worker`). That
integration — translating whatever `o7-run`'s own `RunEvent`/`RunEventKind`
stream produces into `o7-ledger`'s `EventType` vocabulary, if that's even the
right shape, and wiring a real process's lifecycle into
`start_run`/`complete_run`/etc. — is out of scope here and not attempted. If
that integration turns out to need changes to the shared event contract, the
production worker, or `o7-run` itself, that is a new, separate seam to
describe explicitly before touching any of it — not something to infer from
this slice.
