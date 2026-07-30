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

### PASS / FAIL, and interrupted (NOT a third "ERROR" outcome)

Only two variants are genuine sealed/terminal outcomes, both already
canonical — no new status invented:

- **PASS** → `RunStatus::Completed`
- **FAIL** → `RunStatus::Failed`

The transcript also has an **`Interrupted`** variant, but — corrected after
an earlier draft of this slice called it "ERROR" and treated it as a third,
co-equal terminal outcome, which a review round correctly flagged as wrong —
**`Interrupted` is NOT a sealed verdict and must never be described as
"terminal" or "ERROR".** `RunStatus::is_terminal()` deliberately excludes it:
an interrupted run is *resumable* back to `running` via
`resume_interrupted_run`, so `finished_at` stays `None` — real ledger
behavior, proven honestly rather than backfilled to look like a closed run.
This transcript proves BOTH halves of that: the run reaching `interrupted`
with `finished_at: null` at every layer, AND (the regression the earlier
draft was missing) the SAME run later resuming back to `running` via
`resume_interrupted_run`, checked at the ledger and o7d-REST layers and, most
importantly, that **Q-Deck's `RunPage` keeps polling through an interrupted
run and picks up that resume** — before the fix, `RunPage` treated
`interrupted` the same as a sealed outcome and stopped polling on it, so it
would never have noticed a real resume happen.

**A genuinely open seam this transcript does NOT resolve**: `crates/o7-run`
has its own `Verdict` enum (`Pass`, `Fail`, `Blocked`, `Error` —
`crates/o7-run/src/state.rs`), fixed only once a run is `Sealed`. That
`Verdict::Error` is a SEALED, terminal judgment in o7-run's own model — a
different concept entirely from `o7-ledger`'s `Interrupted`, which is
UNSEALED and resumable. There is currently no defined projection from
o7-run's four sealed `Verdict` values onto `o7-ledger`'s status vocabulary at
all (recall `o7d` doesn't depend on `o7-run`) — whether a real
`Verdict::Error` should someday become `RunStatus::Failed`, a new status, or
something else is genuinely undecided. This transcript's `Interrupted`
variant demonstrates the existing ledger state on its own terms; it does not
claim to be — and must not be read as — an answer to that question.

## Where the transcript lives

Three copies of the same replay logic, deliberately NOT unified into a shared
production dependency (a test-only helper is not a real downstream seam, so
adding one to `o7-ledger`'s public API just to share it across crates/tests
would be scope creep):

- `crates/o7-ledger/tests/support/mod.rs` — used by `tests/golden_transcript.rs`
- `crates/o7d/tests/support/mod.rs` — used by `tests/golden_transcript_rest.rs` and `tests/golden_transcript_sse.rs`
- `crates/o7-ledger/examples/seed_r05_fixture.rs` — used by the production packaging smoke (a real on-disk SQLite file, seeded via `cargo run --example seed_r05_fixture -- <path> [pass|fail|interrupted]`)
- `apps/q-deck/src/test-support/goldenTranscript.ts` — typed DTO fixtures mirroring the same shape for Q-Deck's own component tests

All four encode the exact same 7-event shape above. If this transcript's
shape ever changes, all four must change together.

## Real bugs this slice found (in already-frozen R0 code, and in its own first draft)

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
infinite loop, verified by the full frontend suite passing and a targeted
before/after repro. Fixed forward on this new branch, not amended into PR
#83's frozen history.

A second review round found two more real bugs, both fixed forward here:

- **`RunPage`'s polling stopped on `interrupted`.** It used `isActiveRun`
  (queued/running) as its "still needs watching" condition — but the correct
  condition is "not yet SEALED", and `interrupted` is neither active nor
  sealed (see the outcome-mapping section above). Fixed with a new
  `isSealedRun` (`completed`/`failed`/`cancelled` only) in `lib/types.ts`,
  used in place of `isActiveRun` for both of `RunPage`'s polling
  decisions — proven by a new regression
  (`RunPage.svelte.test.ts`'s "keeps polling through an interrupted run and
  picks up a subsequent resume back to running").
- **The "real daemon restart" SSE test was not a process restart.** The
  first draft only `.abort()`-ed a `tokio::task::JoinHandle` and spawned a
  new task in the SAME test process — a real "reopen the same file" proof,
  but not a real "kill and restart the o7d PROCESS" proof. Rewritten to
  spawn the actual compiled `o7d` binary as its own OS process
  (`env!("CARGO_BIN_EXE_o7d")`), kill and reap it, then spawn a second real
  process against the same file. Building this surfaced two more small
  bugs, fixed alongside it: `main.rs` was logging the raw `--listen`
  argument instead of the actual bound address (useless for `--listen
  ...:0`, needed so the test can discover the real port), and the test
  harness itself was closing the child's stderr pipe after reading one
  line, which crashes the child the moment it tries to write its SECOND
  log line (`eprintln!` panics on any write failure, including the EPIPE
  that produces) — fixed by draining stderr for the process's whole
  lifetime in a background thread instead.

## What R0.5 proves, layer by layer

- **Ledger** (`crates/o7-ledger/tests/golden_transcript.rs`): events read back
  in original order and byte-identically on repeated reads; run metadata
  (agent/role/status/finished_at) matches exactly what the transition methods
  wrote (never recomputed); a close-then-reopen of the same on-disk file
  preserves everything; an interrupted run resumes to `running` via
  `resume_interrupted_run`.
- **o7d REST** (`crates/o7d/tests/golden_transcript_rest.rs`): the run is
  listed and reachable by id, the conversation is reachable by id, events
  page correctly under a small `limit` with no loss via `next_after`,
  status/timestamps match the ledger exactly (including interrupted's absent
  `finished_at`, and its resume to `running` over the wire), and an id
  adjacent to a real one still 404s.
- **o7d SSE** (`crates/o7d/tests/golden_transcript_sse.rs`): connect → receive
  part of the transcript → real TCP disconnect → reconnect with
  `Last-Event-ID` → receive exactly the missed tail, no gap, no duplicate;
  the interrupted outcome travels as `run.interrupted`, never collapsed into
  `run.failed`; AND a real restart of the daemon **PROCESS** (the actual
  compiled `o7d` binary, killed and reaped, then respawned — not a task in
  this test's own process) against the same on-disk file resumes correctly.
- **Q-Deck** (`RunPage.svelte.test.ts`, `ConversationPage.svelte.test.ts`, plus
  additions to `Dashboard.svelte.test.ts` and `eventStream.svelte.test.ts`):
  active-run and completed/failed/interrupted rendering (including
  interrupted's no-duration case), this-run-only vs. whole-conversation
  timeline filtering, full run-history pagination (previously untested),
  resilience to a transient refresh failure, an `unsupported`-schema SSE
  frame closing the stream without being treated as a normal event, the
  absence of any mutation control, and — the regression a review round
  required — `RunPage` keeping poll through an interrupted run and picking
  up a subsequent resume back to `running`.
- **Production packaging smoke**: a real on-disk SQLite file seeded via
  `cargo run --example seed_r05_fixture`, served by the real,
  unmodified-except-for-this-slice `o7d serve --static-dir dist` binary —
  `GET /` and an unmatched client route both return the shell;
  `GET /api/v1/health`, `GET /api/v1/runs/{id}` return real data;
  `GET /api/v1/does-not-exist` returns the JSON 404, never the shell; and an
  SSE reconnect with `Last-Event-ID` resumes correctly against the real
  running process. **Caveat, stated plainly**: this was run locally on this
  branch's own worktree, not through any PR-triggered CI — no GitHub Actions
  workflow exists for `o7d`/Q-Deck at all (`.github/workflows/` has none),
  and adding one is explicitly out of this slice's allowed scope (CI
  workflow changes are on the forbidden-scope list). A local gate, however
  genuine, is not the same claim as an independent server-side re-gate; that
  remains open until whoever owns CI configuration adds one.

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

**Update (R0.6, `docs/q-deck/r06-verdict-fidelity.md`)**: one concrete piece
of that gap — `o7-ledger` had no status corresponding to `o7-run`'s sealed
`Blocked`/`Error` verdicts, so a live producer could not have projected
every canonical verdict without collapsing meaning — is now closed. R0.6
does not wire the producer either; it only makes the ledger/API/Q-Deck
vocabulary capable of representing what a real producer would eventually
need to say.

**Update (R0.7, `docs/q-deck/r07-live-ingress.md`)**: this gap is now
closed. `o7 run --ledger <path>` is that real producer — its canonical
`RunEvent` stream is translated into `o7-ledger`'s `EventType` vocabulary
live, per event, by a new `LiveLedgerProjector`, and a real process's
lifecycle is wired into `start_run`/`create_attempt`/`complete_run`/etc.
exactly as anticipated above. No changes were needed to `o7-run` itself
beyond restructuring *when* its canonical events are minted (incrementally
instead of in one post-hoc batch) — the reducer/verdict semantics this
section worried about needing to change were untouched.
