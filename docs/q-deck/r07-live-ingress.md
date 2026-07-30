# Q-Deck R0.7 — real "o7 run" → o7-ledger live ingress

## Purpose

R0.5 proved the read surface (o7d REST/SSE, Q-Deck) against a *synthetic*
run transcript written directly through o7-ledger's own write API. R0.6
closed the vocabulary gap (`Blocked`/`Error`) that synthetic transcript
would have needed. Neither slice ever ran a real `o7 run` process and had
its own canonical events reach the ledger. This slice closes exactly that
gap: a real `o7 run` invocation, run today exactly as it runs today, ends
with its canonical event stream having been projected into `o7-ledger` as
it happened — not imported afterward — so o7d/Q-Deck can watch it live.

## Exact production path today (before this slice)

Read directly from `src/main.rs` and `src/events.rs`, not summarized from
memory:

```
Cli::parse()
  -> run(RunArgs)
       -> worktree::add(repo, base, wt, branch)          // materialize worktree
       -> execute(...)
            -> agent::run(engine, wt, task, model, max_turns)   // BLOCKS until the
                                                                 // agent process exits
            -> RunRecord::create(runs_dir, target, run_id)      // mkdir runs/<target>/<run-id>
            -> rec.write_task / write_agent_stdout / write_diff // flat files, in order
            -> manifest.run(wt, rec.gate_dir())                 // BLOCKS: runs every gate
                                                                 // step to completion, one
                                                                 // Command::output() at a time,
                                                                 // returns Vec<StepVerdict>
            -> events::build_events(run_id, contract, task_ref, diff_ref, ar, &steps, dir)
                 -> mints the ENTIRE canonical RunEvent stream in one call, AFTER
                    the agent and every gate have already finished:
                    RunStarted, AgentStarted, AgentExited, PatchCaptured,
                    (GateStarted, GateFinished) x N, RunSealed
            -> rec.write_text(EVENTS_FILE, to_jsonl(&stream))     // events.jsonl, once, whole
            -> events::canonical_verdict(&stream)                // reduce_all() over the
                                                                   // already-complete stream
            -> rec.write_meta(&meta)                              // meta.json, once
       -> worktree::remove(repo, wt)                     // teardown
```

**The load-bearing fact this slice is built around**: today, no canonical
event exists in memory or on disk *during* agent/gate execution. The whole
stream is synthesized post-hoc, in one non-interruptible call, from
already-completed step results (`Vec<StepVerdict>`) and an already-exited
agent (`AgentRun`). There is no point in the current code where a "this run
is now `running`" signal could be emitted, because `RunStarted` itself does
not exist as an event until after everything is already done.

This means **live ingress requires restructuring `execute()`/`build_events`
so each canonical event is minted and appended (to `events.jsonl` and to
the ledger projector) at the moment the real thing it describes actually
happens**, not reconstructed afterward. This is the one, specific,
unavoidable change to the root `o7 run` path this slice makes — narrowly
scoped to *when* events are minted, not *what* the canonical reducer or
verdict semantics are.

## 2.1 Source of truth

Unchanged from what `src/events.rs`'s own module doc already states, and now
made an explicit cross-crate contract:

- `o7-run`'s canonical `RunEvent` stream and its reducer
  (`o7_run::reduce::reduce`/`reduce_all`, `crates/o7-run/src/state.rs`) are
  the sole authority for event ordering, obligation state, gate state, run
  phase, sealed verdict, digest chain, and replay.
- `o7-ledger` is a durable **projection** (read model) for `o7d` and
  Q-Deck — never a second authority.

**Forbidden, unconditionally:**
- recomputing a verdict inside the ledger projector;
- deriving verdict from process exit code, `meta.json`, or the gate-file
  set, inside the projector (the projector receives the verdict the
  canonical reducer already produced — it does not look anywhere else);
- a second reducer anywhere in `o7-ledger`, `o7d`, or Q-Deck;
- treating flat files and the ledger as two equally-authoritative sources —
  the flat record + its own replay (`o7 replay`) is the ONLY verdict
  ground truth; the ledger's job is to durably reflect it, not to
  co-decide it.

**Concretely, this reducer already supports incremental, one-event-at-a-time
folding** — `pub fn reduce(state: RunState, event: &RunEvent) -> Result<RunState, ReduceError>`
in `crates/o7-run/src/reduce.rs` is a pure per-event fold; `reduce_all` is
just this folded over a slice. The projector does not need — and must not
build — any new reduction logic: the same `reduce`/`reduce_all` a batch
caller uses is available for a live caller to track phase/verdict locally
if it ever needs to (in practice it does not: the terminal ledger call only
needs the *final* `Verdict` the CLI already computes via
`canonical_verdict`, handed down once, at `RunSealed`).

## 2.2 Live path, not post-run import

Production path this slice establishes:

```
canonical event minted (o7 run, incrementally, as each real step happens)
  -> existing canonical flat record append (events.jsonl, same as today, per-event now)
  -> typed/idempotent ledger projection (new: LiveLedgerProjector, via o7-ledger's public API)
  -> o7d REST/SSE (unchanged — already reads whatever is in the ledger)
  -> Q-Deck (unchanged — already polls/streams via o7d)
```

`run finished -> read runs/... -> import` is **not** an acceptable
implementation of this path — it is explicitly reserved below as a
recovery-only tool for catching a sink back up after a crash, never the
primary way live data reaches the ledger.

## 2.3 Identity

One physical run has exactly one identity across every layer:
`o7-run::RunId` (already minted in `main.rs::run()` as
`format!("{secs}-{}", std::process::id())`, flat run directory name, the
`run_id` field inside every line of `events.jsonl`) **must equal**
`o7-ledger::RunId` for the same run, byte-for-byte.

**Blocker found, and the chosen fix**: `o7-ledger::SqliteLedger::create_run`
(`crates/o7-ledger/src/sqlite.rs`) currently *mints its own* `RunId` via
`crate::RunId::generate()` — `NewRun` has no `run_id` field at all. Calling
it as-is would silently create a second, random ledger identity for every
run, exactly the failure mode this section forbids. Both `o7_run::RunId`
and `o7_ledger::RunId` are opaque non-empty-string newtypes with the same
underlying representation (`crates/o7-run/src/ids.rs`,
`crates/o7-ledger/src/ids.rs`) — no shared primitive crate is needed, only
an explicit, lossless string conversion at the seam
(`o7_ledger::RunId::new(o7_run_id.as_str().to_owned())`).

The **minimal** ledger-side fix: add an optional `run_id: Option<crate::RunId>`
field to `NewRun` (`None` preserves every existing caller's behavior byte-
for-byte — the ledger still generates one, exactly as R0/R0.5/R0.6's tests
expect; `Some(id)` is the new live-ingress path, which uses exactly that
id and must fail with a distinct, documented conflict rather than silently
minting a second run if that id is already taken by an unrelated row).

Other identities in this slice:
- **Conversation ID**: `o7_ledger::ConversationId`, resolved per §2.4 below
  — never independently reinvented by the projector.
- **Attempt ID**: `o7_ledger::AttemptId`, minted by
  `SqliteLedger::create_attempt` exactly as today (R0.5/R0.6 never needed
  to unify this with anything on the `o7-run` side — there is no
  `o7-run`-side attempt concept to unify with; one live run projects to
  exactly one ledger attempt).
- **Source event identity**: `run_id + canonical event index (RunEvent.sequence)
  + canonical event digest (RunEvent.event_digest)`. Never timestamp alone
  (the envelope's own rule: timestamps are metadata, never the ordering
  key). This triple is exactly what the projector's idempotency key is
  derived from (§2.7).
- **`ledger.sequence`** (the per-conversation append-order integer o7-ledger
  already assigns inside its own append transaction) remains a
  ledger-local durable cursor for o7d's REST/SSE pagination — it does
  **not** replace or alias the canonical `RunEvent.sequence` above; they
  are different numbers with different scopes and must never be conflated
  in code or in documentation.

## 2.4 Conversation semantics

R0.7 does not pretend multi-turn Command exists yet. Explicit, minimal CLI
contract (exact flag name chosen after auditing `RunArgs` in `src/main.rs` —
none of the existing flags overlap):

- `--conversation-id <id>` (new, optional): if given, the run projects into
  that existing ledger conversation; if that id does not resolve to a real
  conversation, the CLI fails loudly before any worktree/agent work starts
  — never silently creates one under a caller-supplied id, and never
  silently falls back to "create a new one instead."
- Omitted: a **new** ledger conversation is created for this run (matching
  today's implicit assumption that every `o7 run` invocation is its own
  isolated unit — R1's multi-turn Command is explicitly out of scope, per
  `docs/q-deck/architecture.md`).
- No "pick the most recent conversation" behavior anywhere, ever — that
  would make conversation identity a race with whatever else is running
  concurrently.
- No browser-side / Q-Deck-side conversation creation — Q-Deck remains
  read-only; this flag only exists on the `o7 run` CLI.

## 2.5 Opt-in ledger sink

`--ledger <sqlite-path>` (new, optional, `RunArgs`):

- **Absent**: `o7 run` behaves exactly as it does today — flat record only,
  no ledger touched, no new dependency exercised. Proved by a regression
  test that runs the exact same fixture with and without the flag and
  diffs the flat record byte-for-byte (`meta.json`/`events.jsonl` identical
  modulo the run id/timestamps every run already varies by).
- **Present**: live projection is mandatory for the whole run — the
  projector is constructed (and the ledger file opened / schema-attested)
  **before** the worktree is even created, exactly like the existing
  `build_contract` fail-fast discipline in `main.rs::run()`. An unopenable
  ledger path (bad file, failed schema attestation) fails the CLI loudly
  before anything is spent, the same way an invalid gate manifest already
  does.
- A projection **write** failure *during* the run is never silently
  swallowed and never silently downgrades to "flat-file-only for the rest
  of this run" — the canonical run continues to completion (the canonical
  record is never held hostage to sink health), but the CLI's own exit
  path reports, separately and explicitly, that durable projection is
  incomplete and this run needs the recovery path (§2.7) before Q-Deck's
  view of it can be trusted. The canonical verdict printed and stored in
  `meta.json` is never altered by a sink failure — a sink is
  infrastructure, not verdict.

## 2.6 Verdict mapping

Exactly R0.6's frozen mapping, reused verbatim — this slice does not touch
it:

| `o7-run::Verdict` | `o7-ledger::RunStatus` |
|---|---|
| `Pass` | `Completed` |
| `Fail` | `Failed` |
| `Blocked` | `Blocked` |
| `Error` | `Error` |

Distinct lifecycle concepts, unaffected:
- `Cancelled` — explicit cancellation, sealed. Not produced by this slice's
  projector (there is no cancel command yet — R1).
- `Interrupted` — the process/execution stopped before a sealed verdict was
  reached; unsealed, resumable. This is what the projector applies when a
  live-projected run's process disappears without ever emitting
  `RunSealed` (§2.7/§5's interruption test) — never `Error`.

Never, anywhere in this slice: `Error -> Interrupted`, `Interrupted ->
Error`, `Blocked -> Failed`, `Error -> Failed`.

## 2.7 Recovery and idempotency

One canonical source event (identified by `run_id + sequence + event_digest`,
§2.3) is applied to the ledger at most once, ever. o7-ledger already ships
the exact primitive this needs: `Idempotency { key: String }` plus a
scope+request-digest check
(`crates/o7-ledger/src/sqlite.rs`'s `idempotency::check`/`record`, already
used by `create_conversation`/`create_run`/`append_user_message`, scopes
`SCOPE_CREATE_RUN`/`SCOPE_APPEND_USER_MESSAGE`). The projector reuses this
same primitive with a new scope for canonical-event projection, keyed by
`{run_id}:{sequence}` (the digest is carried as part of the recorded
request digest, so a *different* event content replayed under the same
`run_id`/`sequence` — which should never happen for an honest canonical
stream — is caught as an `IdempotencyConflict`, not silently accepted).

Replaying an already-applied event (or the whole prefix of a stream) must
not: create a second run or attempt, duplicate an event, change an
already-assigned `ledger.sequence`, re-finalize an already-terminal run,
change a terminal verdict, turn an interruption into a verdict, or create a
second conversation. Re-applying `N+1..end` after a sink crash at event `N`
must land exactly the missing suffix.

**Recovery entry point** (narrow, reusing the existing `o7 replay`
machinery rather than inventing an import framework): the projector itself
is the reusable function
(`project_event(&self, event: &RunEvent) -> Result<(), ProjectError>` or
similar, one call per canonical event, safe to call twice for the same
event). Recovery is: read `events.jsonl` for a run whose ledger state is
`running`/absent past where the sink is known to have stopped, and call
that same function again over the full (or tail) stream — no new
"migration/import" surface, no second code path with different semantics
than the live one.

## 3. Minimal production projector

```
canonical o7-run event (RunEvent)
  -> LiveLedgerProjector    (new, root crate — depends on o7-run + o7-ledger,
                              nothing downstream depends on it)
       -> o7-ledger's existing public async API only — no raw SQL outside o7-ledger
```

Responsibilities (owned entirely by this one type, not spread across
`main.rs`):
1. Resolve/create the conversation (§2.4) — once, before the run's first
   event.
2. Create the ledger run with the SAME `RunId` (§2.3's `NewRun.run_id`
   extension) at `RunStarted`.
3. `start_run` (→ `Running`) at the same moment.
4. Create the ledger attempt via existing lifecycle APIs, once.
5. Project every subsequent canonical event, in stream order.
6. Persist source-event provenance for every projected event: source
   `run_id`, canonical `sequence`, canonical `event_digest`, canonical
   `kind`/`RUN_EVENT_SCHEMA_VERSION`. **Vehicle, not a new taxonomy**:
   `o7-ledger::EventType` is a closed, documented enum
   (`crates/o7-ledger/src/models.rs`: "Claude/Codex-specific events, tool
   calls, ... artifacts and gates are intentionally NOT here — they arrive
   in PR 4") — this slice does not widen it. `RunStarted`/`RunSealed`'s
   terminal outcome map onto the EXISTING dedicated ledger calls
   (`start_run`, and `complete_run`/`fail_run`/`block_run`/`error_run`/
   `interrupt_run`, each of which already emits its own correctly-typed
   ledger event, per R0.5/R0.6). Every OTHER canonical kind
   (`AgentStarted`, `AgentExited`, `PatchCaptured`, `GateStarted`,
   `GateFinished`, `WorktreeCreated`, `PolicyChecked`,
   `SandboxEvidenceCaptured`) is projected as `EventType::SystemNote` with a
   structured JSON payload carrying the provenance fields above plus the
   kind-specific data — reusing the ledger's existing generic
   event/payload/SSE path exactly as `system.note` already works, not a new
   wire concept.
7. Apply terminal status **only** from the sealed canonical `Verdict` the
   caller hands it at `RunSealed` — never recomputed, never inferred.
8. Finish the attempt with a status consistent with the run's terminal
   status, via existing lifecycle APIs.
9. No raw SQL outside `o7-ledger`'s own crate boundary.
10. `o7-run`/the root CLI depend on `o7-ledger` (a plain Rust dependency);
    neither depends on `o7d`, HTTP, or Q-Deck, directly or transitively.

## What R0.7 does not do

- No migration of the root `o7 run` execution model onto `o7-worker`/Sandboy
  — `crates/o7-worker`, `sandbox/Sandboy`, worktree lifecycle, verifier
  architecture, provider qualification, and R1 auth are untouched unless a
  concrete, documented blocker proves otherwise.
- No R1 mutation surface anywhere (no start/stop/cancel/approve/reject/
  follow-up/provider-selection/auth UI/shell/file-editing) — ledger, o7d,
  or Q-Deck.
- No change to `o7-run`'s reducer, verdict semantics, or digest chain.
- No change to Q-Deck components beyond what's proven strictly necessary
  for this slice's acceptance evidence.

**The next slice after R0.7** is real multi-turn Command (R1) — not
attempted here.
