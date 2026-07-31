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

**Ordering, exactly** (tightened after an independent re-gate found the
first implementation of this section got it backwards): for every
canonical event, (1) mint it, (2) serialize it and durably append it —
flushed (`sync_data`) — to `events.jsonl` BEFORE anything else happens, (3)
only then project it to the ledger. A ledger row must never exist for an
event that isn't yet part of the on-disk canonical record — the ledger run
itself is not created (and `running` is not reported) until the canonical
`RunStarted` line has already been flushed to disk. A canonical-journal
write failure is fatal (propagates, aborting the run) — a *ledger
projection* failure is not (see §2.5); these are deliberately different
failure modes.

**A third independent re-gate found two more crash-consistency gaps in
this same durability chain, both fixed**:

- `RunStarted` durably references `task.md` by digest (`ArtifactRef`) — but
  the live path used to write `task.md` itself only after the agent
  finished (the no-ledger path's original timing). A SIGKILL during the
  agent run left a durable `RunStarted` whose referenced artifact didn't
  exist on disk: structurally valid for `reduce_all` (which never resolves
  artifact content), but not for a real replay that does. Fixed: on the
  live path only, `RunRecord::write_task_durable` (`write_all` +
  `sync_data`, unlike `write_task`'s plain `std::fs::write`) runs BEFORE
  `RunStarted` is even minted — proved by
  `sigkill_during_the_agent_leaves_a_durable_task_md_matching_run_started`
  (`tests/live_ingress_e2e.rs`), which kills mid-agent and asserts
  `task.md`'s content hashes to exactly the digest `RunStarted` committed
  to.
- After every event was already durably appended one at a time, `execute()`
  still finished with a single `rec.write_text(EVENTS_FILE, ...)` over the
  WHOLE stream — same bytes, but `write_text` is a plain `std::fs::write`
  (open, truncate, rewrite, no `sync_data`), reopening a window where the
  file could be empty or partial between the truncate and the rewrite
  landing, for a journal that was otherwise already fully durable. Fixed:
  the live path never rewrites its own journal — it only reads it back and
  verifies the bytes match the in-memory stream. The no-ledger path (which
  never did the live per-event append) keeps the original one-shot write.

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

The **minimal** ledger-side fix, as actually shipped: a new
`create_run_with_id(request, run_id, idempotency)` sibling to `create_run`
(not a field added to `NewRun` — `create_run` itself stays untouched, byte-
for-byte, for every existing caller). Idempotency is **mandatory** here,
keyed by `run_id` itself — a retry (a resumed live process, or `o7
recover`'s catch-up) replays the existing row via the ordinary idempotent-
replay path instead of hitting a raw `UNIQUE` constraint error on the
primary key. The same key reused with a genuinely different `run_id` is a
hard `IdempotencyConflict`.

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
- `--conversation-id` without `--ledger` is rejected at CLI **parse** time
  (`clap`'s `requires = "ledger"`), not silently ignored — an independent
  re-gate correctly found the first implementation only checked this
  inside the `Some(ledger)` branch, so a bare `--conversation-id` with no
  `--ledger` was silently accepted and did nothing.

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
  record is never held hostage to sink health), and the canonical verdict
  printed and stored in `meta.json` is never altered by a sink failure — a
  sink is infrastructure, not verdict. But the **process exit code** is:
  a `Pass` whose explicitly requested ledger projection is incomplete is
  never reported as a successful (`0`) exit — `o7 run` exits non-zero
  (the same path as a non-`Pass` verdict) and prints a warning naming the
  exact recovery command
  (`o7 recover --ledger <path> --run-dir <run-dir>`, §2.7) to run before
  trusting Q-Deck's view of that run.

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

**Restart safety is not only per-event.** An independent re-gate correctly
found that the first implementation of this section made the *events*
idempotent but not the surrounding *lifecycle* — retrying
`ConversationSelector::New` minted a second conversation with no
idempotency key; a re-`start_run` on an already-`Running`/terminal run hit
`ForbiddenTransition`; a re-`create_attempt` created a second attempt;
re-sealing was not a no-op. Fixed:

- Conversation creation under `New` is keyed
  `conversation-for-run:{run_id}` — idempotent, resolves to the same
  conversation on retry.
- `attach_run` (§3) checks the run's CURRENT status before acting:
  `start_run` only from `Queued`; the existing `running` attempt is looked
  up (`SqliteLedger::running_attempt`, new — a run has at most one by
  construction) and reused instead of creating a second one; a run already
  terminal/interrupted is attached to its **most recent attempt regardless
  of status** (`SqliteLedger::latest_attempt`, new), not `attempt_id: None`
  — see the idempotency-across-terminal-attach fix below for why `None`
  was wrong.
- `seal(verdict)` reads the run's current status first: already sealed
  with EXACTLY this verdict → no-op; already sealed/interrupted with a
  **different** status → hard conflict (`LedgerError`-wrapping error), a
  terminal outcome is never silently overwritten.

**A second independent re-gate found the above still incomplete** for two
distinct reasons, both now fixed:

1. **Conversation resolution for an already-existing run.** `attach_run`'s
   own `create_run_with_id` call is idempotent on `run_id`, but its
   idempotency digest also includes `conversation_id` (§2.3/§3) — so a
   caller that guesses the WRONG `ConversationSelector` for an
   already-existing run (e.g. catch-up always assuming `New` for a run that
   was actually created under an explicit `--conversation-id`) hits an
   `IdempotencyConflict` *before* any "the existing row's real conversation
   wins" logic is ever reached — that logic runs strictly after the digest
   check passes, not instead of it. Fixed at the call site, not inside
   `attach_run`: `main.rs::catch_up` now looks the run up by
   `canonical_run_id` FIRST, and if it already exists, passes
   `ConversationSelector::Existing(run.conversation_id)` — never guesses.
   `ConversationSelector::New` is now only the genuinely-first-attach
   fallback, when no row (and so no conversation to disagree with) exists
   yet.
2. **`attempt_id` is part of `append_system_note`'s idempotency digest**
   (§3's `project`), so it must be *stable* across a live run and a later
   catch-up of the SAME event, not merely present-vs-absent. Attaching to a
   terminal run with `attempt_id: None` (the original design) changed the
   digest for every already-projected event's replay, since those events
   were recorded with `Some(original_attempt)` — turning a should-be no-op
   catch-up into a guaranteed `IdempotencyConflict` for anything already
   correctly projected. `SqliteLedger::latest_attempt` (unlike
   `running_attempt`, which only finds a *currently running* one) finds a
   run's attempt by `attempt_number` regardless of status, so a terminal
   attach now reuses the exact same `attempt_id` its events were originally
   recorded under.

**A third independent re-gate found one more window in this same
lifecycle, fixed**: `PendingProjection::open` (phase 1 — resolves, and for
a fresh conversation actually CREATES, the ledger conversation) runs
*before the worktree even exists*; `attach_run` (which creates the ledger
RUN row) doesn't run until well after, once canonical `RunStarted` is
durable. If the process crashes in that window, catch-up finds no run row
and — even with fix #1 above — has nothing to look an existing row's
conversation up FROM. Guessing `ConversationSelector::New` here silently
discards an explicit `--conversation-id` the original invocation was
actually given, creating a phantom second conversation instead of
resolving the real one. Fixed with a small durable
`ledger_binding.json` (`run_id`, `conversation_id`, `agent`, `role`,
`record::LedgerBinding`) written — durably, `write_all` + `sync_data` —
in the run's record directory BEFORE canonical `RunStarted` is even
minted, from the SAME `conversation_id` phase 1 already resolved. Catch-up,
when no run row exists, reads this file instead of guessing; `New` is now
only the residual fallback for a run whose record predates this fix (or
crashed before this file's own durable write — impossible from this point
on, since nothing durable exists yet at that point for there to be
anything to catch up).

The first implementation's own recovery test only proved the degenerate
case (every `system.note` AND its idempotency record deleted, i.e. full
reconstruction from scratch) — which never exercises either fix above,
since a from-scratch catch-up never re-projects an already-correct event
under a real `attempt_id`, and never resolves a conversation for a run
that already has one. `tests/live_ingress_e2e.rs` now separately proves:
an intact stream (nothing deleted) catches up as an exact no-op; an
existing prefix plus a missing suffix restores only the tail without
disturbing the prefix's original event ids; a single missing event
among an otherwise fully-projected stream is restored in place; and
catch-up under an explicit `--conversation-id` resolves that SAME
conversation, not a phantom new one.

**Recovery/catch-up entry point**: `o7 recover --ledger <path> --run-dir
<run-dir>` (extends the existing R0.5 `o7 recover`, which still always
runs its own still-running → `Interrupted` scan regardless). Reads
`run-dir/events.jsonl`, looks the run up first to resolve its real
conversation (above), then reuses `PendingProjection::open` + `attach_run`
(idempotently attaching to whatever the ledger already has) and
re-projects the **entire** stream through the ordinary `project`/`seal`
calls — every already-applied event is a safe no-op via its own
idempotency key AND its original `attempt_id`, so this is correct whether
the sink missed nothing, a tail, one event in the middle, or (the
degenerate case) everything. No second reducer, no new import format —
the exact same projector a live run uses.

**A fourth independent re-gate found three more gaps in this same
recovery path, all fixed**:

1. **Artifact-blind verification.** Catch-up used only
   `o7_run::reduce::reduce_all` — chain continuity, digests, reducer
   structure — never resolving or verifying referenced artifacts
   (`task.md`, `diff.patch`, gate logs). A record whose `task.md` was
   altered or deleted after the fact would still catch up cleanly and have
   the ledger report it `Completed` — a record `o7 replay` itself would
   reject once sealed. Fixed by extracting `o7_run::replay::verify_prefix`:
   everything `replay` checks (chain, digests, reducer, AND full artifact
   content-digest verification) EXCEPT the sealed requirement, so an
   unsealed or crashed-mid-run prefix is verified exactly as strictly as a
   sealed one, without demanding a verdict it doesn't have yet. `replay`
   itself is now built on top of this same primitive — one code path, not
   two divergent ones.
2. **An untrusted `ledger_binding.json`.** `LedgerBinding::read` only
   deserialized the file — never checked its `schema`, never checked its
   `run_id` matched the canonical stream it was found alongside, and (when
   an existing ledger run row was found) still let the sidecar file's
   `agent`/`role` override the row's own persisted values instead of using
   them directly. A stale or tampered sidecar could misdirect a no-row
   recovery into the wrong conversation, corrupt `create_run_with_id`'s
   idempotency digest for an already-existing run, or substitute a wrong
   agent/role on first attach. Fixed: `schema`/`run_id` are validated
   before the binding is trusted at all; when a ledger run row already
   exists, its own `conversation_id`/`agent`/`role` are used VERBATIM (a
   disagreeing binding is refused, not silently preferred either way);
   when no row exists, the binding is REQUIRED (not merely consulted) —
   its absence is now a hard error, not a fallback to
   `ConversationSelector::New`. Every `--ledger` run since the third
   corrective round writes its binding durably before `RunStarted`, so a
   missing binding at this point means a corrupt or pre-fix record, never
   a legitimate case to paper over with a guessed identity.
3. **`Interrupted` could permanently outrank a proven sealed verdict.** A
   plain `o7 recover` (no `--run-dir`) classifies any run stuck `running`
   as `interrupted` — correct as far as it knows, since it never consults
   `events.jsonl`. But if the canonical stream was ALREADY genuinely
   sealed (a real `RunSealed`, a concrete reducer verdict) and only the
   ledger's own `seal()` call never landed, ordinary `seal()` then refuses
   to overwrite `interrupted` — it is, correctly, an otherwise-settled
   dead-end for everyone else. Without an exception, that TEMPORARY
   recovery classification would permanently block the TRUE canonical
   verdict from ever reaching the ledger. Fixed with one narrow, explicit
   bypass — `SqliteLedger::repair_interrupted_run_to_terminal` (mirroring
   `resume_interrupted_run`'s existing precedent: a precondition-gated
   exception to the general transition table, not a loosening of it) and
   `LiveLedgerProjector::seal_or_repair_interrupted`, reachable ONLY from
   `catch_up` after it has independently verified a genuinely sealed
   stream via `verify_prefix` above. The live path's own `seal()` call is
   completely unchanged — `interrupted` stays an immovable dead-end for
   it, exactly as before.

**A fifth independent re-gate found two more gaps, both fixed**:

1. **Repair left the run's own attempt internally inconsistent.** An
   ordinary terminal transition (`set_run_status`) atomically closes BOTH
   the run and its own running attempt to matching statuses.
   `repair_interrupted_run_to_terminal` only updated `run` — leaving
   `run = completed` next to `attempt = interrupted`, an internally
   contradictory ledger. The round-4 E2E proof for this repair also used an
   impossible fixture (it reverted only `run.status`, when a real crash
   before any seal would never have gotten the attempt — or the terminal
   ledger event — to `completed` in the first place, so it could never
   have caught this). Fixed: the repair now atomically closes the run's
   most-recent `interrupted` attempt to the matching terminal attempt
   status in the SAME transaction; the E2E fixture now reverts the run,
   its attempt, AND removes the terminal event, and asserts the attempt's
   post-repair status and that exactly one terminal ledger event exists.
2. **An unsealed canonical prefix could attach to an already-terminal
   ledger run.** `verify_prefix` legitimately tolerates an unsealed
   stream — but catch-up did not check the EXISTING ledger run's status
   before re-projecting: an unsealed prefix over an already-`completed`/
   `failed`/`blocked`/`errored` ledger run silently succeeded, reporting
   "still running" while the ledger disagreed. Fixed: catch-up now fails
   closed, BEFORE touching the ledger, if the canonical stream is unsealed
   and an existing ledger run is already one of the sealed terminal
   statuses — only `queued`, `running`, or `interrupted` are acceptable
   ledger statuses for an unsealed prefix to attach to.

Also found: the existing-run branch of `ledger_binding.json` validation
only compared `conversation_id`, silently accepting a binding whose
`agent`/`role` disagreed with the persisted row — despite this section's
own claim that "a disagreeing binding is refused." Both fields are now
checked the same way `conversation_id` is.

Six new regression tests now cover the binding-validation and artifact
paths directly (a prior gap the re-gate also flagged): an altered artifact
is refused BEFORE the ledger is touched at all (proved by snapshotting the
full ledger state and asserting it is byte-identical after the refused
attempt); an unsupported `ledger_binding.json` schema; a binding whose
`run_id` doesn't match; a missing binding when no run row exists (a
genuinely `--ledger`-less run recovered against a freshly-opened ledger); a
binding copied verbatim from a different run's record directory; and a
binding disagreeing with an existing run row across all three fields
(`conversation_id`/`agent`/`role`, looped in one test).

## 3. Minimal production projector

```
canonical o7-run event (RunEvent)
  -> LiveLedgerProjector    (new, root crate — depends on o7-run + o7-ledger,
                              nothing downstream depends on it)
       -> o7-ledger's existing public async API only — no raw SQL outside o7-ledger
```

Split into two phases (`PendingProjection` → `LiveLedgerProjector`), matching
§2.2's ordering fix — `main.rs::run()` calls phase 1 before the worktree
exists; `main.rs::execute_live` calls phase 2 only after canonical
`RunStarted` is durably on disk:

1. **Phase 1** (`PendingProjection::open`): open the ledger, resolve/create
   the conversation (§2.4) — idempotently, safe before the worktree exists
   since it reports no run status.
2. **Phase 2** (`PendingProjection::attach_run`, called only after durable
   `RunStarted`): create the ledger run with the SAME `RunId`
   (`create_run_with_id`, §2.3) — idempotently attaching if it already
   exists; `start_run` only if still `Queued`; attach to the existing
   `running` attempt or create one, only while the run is actually live.
3. Project every canonical event, **including `RunStarted` and
   `RunSealed`** — no exceptions, no events skipped by kind. Persisting
   source-event provenance (source `run_id`, canonical `sequence`, `event_digest`,
   `schema_version`, `kind`) for every one of them, first and last included,
   is what makes it possible to durably correlate exactly which canonical
   event produced any given ledger transition — an earlier version of this
   projector treated `RunStarted`/`RunSealed` as no-ops here, which lost
   that correlation for precisely the two most load-bearing events.
   **Vehicle, not a new taxonomy**: `o7-ledger::EventType` is a closed,
   documented enum (`crates/o7-ledger/src/models.rs`: "Claude/Codex-specific
   events, tool calls, ... artifacts and gates are intentionally NOT here —
   they arrive in PR 4") — this slice does not widen it. Every canonical
   kind projects as `EventType::SystemNote` with a structured JSON payload
   carrying the provenance fields above plus kind-specific data — reusing
   the ledger's existing generic event/payload/SSE path, not a new wire
   concept. The DEDICATED ledger lifecycle events
   (`run.created`/`run.started`/`run.completed`/etc, from
   `create_run_with_id`/`start_run`/`complete_run`/etc, per R0.5/R0.6) are
   unaffected and remain the authoritative status transitions — the
   `system.note` provenance record is additional correlation, not a
   replacement for them.
4. Apply terminal status **only** from the sealed canonical `Verdict` the
   caller hands it at `RunSealed` — never recomputed, never inferred.
   Idempotent: already sealed with this exact verdict → no-op; sealed with
   a different one → hard conflict, never silently overwritten (§2.7).
5. Finishing the attempt happens as a side effect of the SAME transaction
   that seals the run (`o7-ledger`'s own `set_run_status`, unchanged since
   R0.6) — no separate attempt-finishing call is needed.
6. No raw SQL outside `o7-ledger`'s own crate boundary.
7. `o7-run`/the root CLI depend on `o7-ledger` (a plain Rust dependency);
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

**Known follow-up, not a blocker for this slice** (fourth re-gate): canonical
`RunSealed`/the ledger's own seal both land before `meta.json` is written
(`execute`/`execute_live` in `main.rs`). A SIGKILL in that specific tail
window leaves a fully sealed, artifact-verifiable `events.jsonl` but no
`meta.json` — and `o7 replay` (`events::replay_record`) requires `meta.json`
unconditionally, with no fallback, to read the *stored* verdict it compares
the recomputed one against. `o7 recover --run-dir`'s catch-up is unaffected
(it never reads `meta.json`), but a bare `o7 replay` against such a record
fails. This gap predates R0.7's live-ingress work — `meta.json`'s own
write timing hasn't changed here — so it is out of scope for this slice; a
follow-up should either give `replay_record` a path that tolerates a missing
`meta.json` (comparing only against the reducer's own recomputation, with no
stored verdict to cross-check) or make `meta.json` durable at the same point
`RunSealed` is.
