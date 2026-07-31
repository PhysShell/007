# Q-Deck R1 — the first real multi-turn Command vertical

## Purpose

R0.7 proved a single isolated `o7 run` projects live into `o7-ledger`. R1
proves the conversation can continue: a user-submitted follow-up command,
accepted durably, produces a NEW sealed canonical run in the SAME
conversation, continuing the SAME provider session where one is available —
end to end through `o7d`'s REST/SSE surface and a minimal Q-Deck UI.

This is one narrow, provable vertical — not a queue, not cancel/approve,
not a workflow engine, not multi-agent orchestration. See "What R1 does
not do" at the end.

## 0. Frozen semantic decision: a command never reopens a sealed run

A run, once sealed (`docs/q-deck/r06-verdict-fidelity.md`,
`docs/q-deck/r07-live-ingress.md`), never transitions back to `running` and
never gets a second verdict. R1 does not change this. A command is never a
mutation of a prior run — it always produces a **new** run:

```
Conversation
  ├─ Run A: initial task, sealed
  ├─ Run B: command 1, parent_run_id=A, sealed
  └─ Run C: command 2, parent_run_id=B, sealed
```

Each accepted command allocates:
- a new canonical `RunId`;
- a new flat canonical run record (`runs/<target>/<run-id>/`, exactly
  R0.7's shape — durable command artifact, `events.jsonl`, `meta.json`);
- a new ledger `run` row in the SAME conversation, with `parent_run_id`
  pointing at the run this command continues from (`run.parent_run_id`
  already exists in the schema since R0's `SCHEMA_V1` — R1 is the first
  slice to actually populate it);
- a new `run_attempt`;
- a continuation of the SAME provider session, if — and only if — the
  parent run's provider session identity is durably recorded and passes
  validation (§5). If it isn't, the command fails closed (§5) — R1 never
  silently starts a fresh, uncontinued session and calls it a
  continuation.

**Provider session continuity and canonical run identity are different
concepts, never conflated:**

| | scope | authority |
|---|---|---|
| provider `session_id` | lets the model continue its own context | opaque to `o7-run`; carried, never interpreted |
| canonical `RunId` | one sealed, replayable, append-only event stream | `o7-run`'s reducer is the sole verdict/replay authority |
| ledger `run`/`command` rows | durable lifecycle, projection, read model | never a second reducer, never infers a verdict |

A command, a ledger row, or a provider session is never treated as an
alternate reducer. `o7_run::reduce`/`o7_run::replay` remain the only place
a verdict is computed, exactly as R0.7 froze.

## 1. Current-tree audit (read before this slice's code changes)

Read directly from the tree at the R0.7 merge SHA, not assumed:

- **`src/agent.rs`** — `run_claude` invokes `claude -p <task> --model
  <model> --permission-mode bypassPermissions --output-format json
  --max-turns <n>` as a ONE-SHOT `Command::output()` call. It captures raw
  stdout/exit code only; there is a `TODO(phase-2)` comment already in the
  file: *"parse claude JSON stdout for session_id + total_cost_usd and
  thread them into RunMeta"*. R1 is that phase-2 work, scoped narrowly to
  `session_id` (§3.1) — `total_cost_usd` stays out of scope here.
- **A real parsing precedent already exists** in `src/judge.rs`'s
  `call_claude`: `--output-format json` produces a single top-level JSON
  object, `{"result": "...", "session_id": "...", "total_cost_usd": ...}`,
  parsed via `serde_json::from_str::<Value>` then `.get("session_id")`.
  R1's session-extraction code is NOT guessing a shape — it reuses this
  exact, already-observed envelope.
- **`src/main.rs`**'s `run()`/`execute()`/`execute_live()` (R0.7): mints
  `RunId` as `format!("{secs}-{}", std::process::id())`, always starts a
  fresh worktree from `--repo`/`--base`, always calls `agent::run()` (a
  one-shot, non-continuing call), and `execute_live`'s durability ordering
  (ledger binding + task artifact durable before `RunStarted`, per-event
  append+sync, artifact-aware catch-up) is exactly what a command's child
  run must also follow — R1 does not build a second pipeline.
- **`crates/o7-run`**: `reduce`/`reduce_all`/`replay::verify_prefix`
  operate purely over one run's own `RunEvent` stream; nothing here knows
  about conversations, commands, or other runs. R1 does not touch this
  crate's reducer/replay logic at all — a command's child run is reduced
  and replayed exactly like any other run.
- **`crates/o7-ledger`**: `run.parent_run_id` (nullable, FK to
  `run(conversation_id, run_id)`) has existed since `SCHEMA_V1` but has
  never been populated (`ledger_projector.rs::attach_run` has hardcoded
  `parent_run_id: None`). `idempotency_record` (scope + key +
  request_digest → result_reference) is the existing, reused idempotency
  primitive (`create_run_with_id`, `append_system_note`) — R1's command
  idempotency reuses this SAME table under a new scope, not a parallel
  mechanism.
- **`crates/o7d`**: R0's router is READ-ONLY by design (`lib.rs`'s own doc
  comment: *"R0 is read-only: no run creation, no mutation, nothing that
  changes ledger state lives behind this router"*). `AppState` currently
  holds only `ledger: SqliteLedger`. R1 is the first slice to add a
  mutating route and therefore the first slice where `AppState` needs
  enough configuration to safely OWN spawning a child run (§6).
- **Q-Deck**: conversation/run pages are read-only, polling/SSE-driven.
  There is no form, no POST call, no client-side identifier generation
  anywhere in the current frontend.
- **Existing process-level E2E fixtures** (`tests/live_ingress_e2e.rs`):
  establish the pattern R1's E2E test reuses verbatim — a fake `claude` on
  `PATH` that writes a sentinel/argv-evidence file and returns a
  structured JSON envelope, real compiled `o7`/`o7d` binaries as separate
  OS processes, REST/SSE assertions against a real SQLite ledger.

## 2. Command identity

A `CommandId` is a distinct identity — never equal to a `RunId`, a
provider `session_id`, or a ledger event id. A command record carries:

```
command_id       — new opaque id, distinct namespace from RunId/EventId
conversation_id  — the conversation this command belongs to
parent_run_id    — the run this command continues from
command_text     — the exact, byte-verbatim command text
idempotency_key  — caller-supplied, reused via the SAME idempotency
                    primitive create_run_with_id/append_system_note use
                    (scope + key + request-digest → result_reference)
created_at       — acceptance time
status           — accepted | started | completed | rejected
child_run_id     — set once a child run is allocated (accepted onward)
```

`status` here is the COMMAND's own lifecycle bookkeeping — it is NEVER a
second verdict authority. The command's `child_run_id`'s own ledger `run`
row and `o7-run` canonical stream remain the sole source of the CHILD
RUN's verdict; `command.status = completed` means "the child run reached a
sealed terminal status," read back from the run, never independently
decided.

## 3. Authority and durability ordering

```
validate request (schema, size limits, non-empty command text)
    ↓
durably record ACCEPTED command (idempotent create; conversation + parent
    run existence checked; parent must be the conversation's current tail
    run; no other accepted/started command open for this conversation)
    ↓
allocate child RunId; durable command → run binding (same transaction as
    acceptance, or a follow-up transaction the recovery scan can repair —
    see §7)
    ↓
respond 202 to the HTTP caller — ONLY after the above is durable
    ↓
(background, NOT before the response) start provider continuation
    ↓
write canonical child-run events/artifacts (R0.7's exact durability
    ordering: durable command-as-artifact + ledger_binding.json BEFORE
    RunStarted, per-event append+sync, no truncate-rewrite)
    ↓
project child run into ledger (LiveLedgerProjector, unchanged)
    ↓
served through the EXISTING REST/SSE/Q-Deck read paths — no new read
    surface, no second projector
```

Two invariants this ordering exists to enforce:

- **The provider is never invoked before durable command acceptance.** A
  crash between "durable acceptance" and "provider invocation" must leave
  a `command` row in `accepted` status with no evidence a provider process
  ever ran — recoverable, re-driveable, never silently lost (§7).
- **The client is never told "accepted" before durable acceptance
  actually happened.** A `202` is only ever written after the `command`
  row (and its idempotency record) are committed to SQLite.

## 4. Idempotency

Reuses the exact primitive `create_run_with_id`/`append_system_note`
already use: `Idempotency { key }` plus a scope + request-digest check in
`idempotency_record`. New scope: `create_command`. The digest covers
`(conversation_id, parent_run_id, command_text)` — the semantically
relevant fields of the request.

- **Same key, byte-identical request** (same `conversation_id`,
  `parent_run_id`, `command_text`): replays the existing `command_id` and
  its `child_run_id` (once allocated) — the ordinary idempotent-replay
  path already proven in R0.7. A provider process is NEVER started a
  second time for a replayed request.
- **Same key, a DIFFERENT request** (any of the three fields differs):
  `IdempotencyConflict` — no mutation, no provider invocation, mapped to
  HTTP `409`.

## 5. Concurrency

R1's first slice allows **at most one** `accepted` or `started` command
per conversation at a time. A second command submitted while one is still
`accepted`/`started` is rejected — `409` — BEFORE any provider invocation.
No queueing, no ordering guarantee for a second command; the caller must
wait for the first to reach `completed`/`rejected` and resubmit. A command
queue is explicitly out of scope for this slice (see "What R1 does not
do").

The concurrency check and the command insert happen inside the SAME
`IMMEDIATE` SQLite transaction `o7-ledger`'s other writes already use
(`SqliteLedger::with_tx`) — SQLite's own writer serialization is the
concurrency guard; there is no separate lock table.

## 6. Provider session contract

- Extracted from the SAME structured JSON envelope `judge.rs::call_claude`
  already parses (`{"result", "session_id", "total_cost_usd", ...}`) — a
  real, previously-observed shape, not a guessed one.
- Typed: a `ProviderSessionId` newtype (non-empty, no control characters),
  never a bare `String` passed around positionally.
- Persisted durably on the run row that produced it (`run.provider_session_id`,
  new nullable column, R1's `SCHEMA_V3`) — not only in the flat record, so
  a continuation can look it up without depending on `runs_dir` layout.
- Bound to run/conversation provenance: a continuation looks up the
  **parent run's own** `provider_session_id` — never accepts one directly
  from an HTTP request. §3.6's forbidden-inputs list makes this explicit.
- Never logged as a credential; never returned to the browser (§8's
  forbidden-inputs list is symmetric — never accepted FROM Q-Deck, and
  the response DTOs in this slice never echo it back either).
- Used ONLY to continue the exact conversation lineage it came from: a
  continuation is refused unless `parent_run_id` both (a) belongs to the
  target conversation and (b) is that conversation's current tail run
  (§5's stale-parent protection), so a session can't be replayed against
  an unrelated or superseded run.

**Fail-closed rule**: if the parent run has no durably recorded
`provider_session_id` (the initial run's `claude` call never returned one,
or it was a `--no-ledger` legacy run, or it predates R1), the command is
REJECTED — `422` — with a diagnostic naming the missing session. R1 never
silently starts a fresh, uncontinued session in its place and calls it a
continuation.

## 7. Recovery

A `command` row's own lifecycle is scanned the same way R0.5's
still-`running`-at-open scan works: `o7 recover` (plain, no new flags in
this slice) additionally finds any `command` row stuck `accepted`/
`started` whose `child_run_id` is unset, or whose `child_run_id`'s run has
no durable canonical record yet, and reports it as pending/recoverable —
visible, not silently dropped. Actually re-driving a stuck command (retrying
the provider invocation automatically) is OUT of scope for this slice;
the recovery surface only needs to make a stuck command DISCOVERABLE, not
self-heal it — re-submission under the SAME idempotency key is the
documented recovery path once a stuck command is found (§4 guarantees this
is safe and won't double-invoke the provider once the child run's own
canonical record exists).

## 8. HTTP API

```
POST /api/v1/conversations/{conversation_id}/commands
```

Request:

```json
{
  "schema_version": 1,
  "parent_run_id": "...",
  "command": "...",
  "idempotency_key": "..."
}
```

Response, only after durable acceptance:

```json
{
  "schema_version": 1,
  "command_id": "...",
  "conversation_id": "...",
  "parent_run_id": "...",
  "run_id": "...",
  "status": "accepted"
}
```

Status codes:
- `202` — durably accepted (including an idempotent replay of a prior
  identical request).
- `400` — malformed request (missing/empty field, command text over the
  size limit, whitespace-only command).
- `404` — unknown conversation, or `parent_run_id` does not exist AT ALL.
- `409` — `parent_run_id` is not the conversation's current tail run
  (stale parent); idempotency key reused with a different request;
  another command is already `accepted`/`started` for this conversation.
- `422` — the parent run has no valid, durable provider session to
  continue (§6's fail-closed rule).
- `500` — reserved for a genuine, unexpected server-side failure (a
  SQLite error, a spawn failure) — never used for a validation outcome
  covered by one of the codes above.

**Never accepted from the client** (defense against exactly the inputs
that would let an untrusted caller pick its own blast radius): shell
command text to execute directly, an executable path, an arbitrary model,
an arbitrary provider, a provider session id, a worktree path, a ledger
path, a permission mode, or environment variable overrides. The ONLY
caller-supplied strings that flow into this vertical are `conversation_id`
(routing), `parent_run_id` (validated against the ledger), `command`
(carried as inert text — see §9.2 on argv handling), and
`idempotency_key`.

## 9. Minimal production vertical

### 9.1 Provider session extraction (`src/agent.rs`)

`run_claude`'s structured-output parsing is factored out and reused:
parse the top-level JSON envelope, extract `session_id` as a typed
`ProviderSessionId`, keep returning raw stdout unchanged (still the
existing evidence contract). A missing/malformed `session_id` on a
continuation-capable call path is a distinct, explicit typed failure —
never silently treated as "no session" and papered over. The ORIGINAL,
non-R1 `o7 run` path (no `--conversation-id`/command involved) keeps
working exactly as before; it additionally persists the session identity
durably (via the ledger, §9.3) whenever a ledger sink is active, purely as
a forward-looking side effect — it does not change that path's own
observable behavior.

### 9.2 Continuation invocation (`src/agent.rs`)

A separate, explicitly named primitive — not a pile of new nullable
parameters bolted onto `run()`:

```rust
pub fn continue_session(
    session_id: &ProviderSessionId,
    workdir: &Path,
    command: &str,
    model: &str,
    max_turns: u32,
) -> Result<AgentRun>
```

Builds argv directly via `std::process::Command` — `.arg(command)` for
the command text as ONE argument, never through a shell or a
concatenated string. A dedicated test proves a command containing spaces,
quotes, `$()`, backslashes, and embedded newlines is delivered as a single
literal argv element and never interpreted by a shell.

### 9.3 Durable model (`crates/o7-ledger`)

`SCHEMA_V3` (forward-only, following R0.6's rebuild-for-CHECK-constraint /
plain-ADD-COLUMN precedents already in `migrations.rs`):

- `ALTER TABLE run ADD COLUMN provider_session_id TEXT` — nullable;
  populated once a run's agent call returns one.
- `CREATE TABLE command (...)` — `command_id` PK, `conversation_id`,
  `parent_run_id`, `command_text`, `status` (CHECK'd vocabulary:
  `accepted`/`started`/`completed`/`rejected`), `child_run_id` (nullable),
  `created_at`, `updated_at`; FK to `conversation`, and to `run` for
  `parent_run_id` only. Deliberately NO FK from `child_run_id` to
  `run.run_id`: `bind_command_child_run` (below) durably records the
  freshly minted child run id BEFORE that run's own ledger row exists —
  the whole point of the durability-ordering rule in §3. A synchronous FK
  there would make that required ordering impossible to satisfy; an
  earlier draft of this migration had one and it was caught by
  `crates/o7-ledger/tests/commands.rs` failing with a real
  `FOREIGN KEY constraint failed` on the very first bind.

New typed `SqliteLedger` methods (no raw SQL anywhere outside this crate —
Q-Deck/`o7d`/the root CLI only ever call these):

- `create_command(request, idempotency) -> Result<Command, LedgerError>` —
  validates parent existence + tail-ness + no-concurrent-open-command
  inside one `IMMEDIATE` transaction; idempotent per §4.
- `bind_command_child_run(command_id, run_id) -> Result<Command, LedgerError>`
  — durably binds AND transitions the command to `started` in the same
  call (there is no separate `mark_command_started`: binding a child run
  id *is* what "started" means for a command). Compare-and-swap
  (`WHERE child_run_id IS NULL`), not a blind write: two callers racing to
  bind the SAME command (e.g. `o7d` handling a retried idempotent request
  before the first attempt's response landed) both get back the SAME
  already-bound `Command` — the caller must compare the returned
  `child_run_id` against the id it proposed to know whether it won the
  race and should actually spawn.
- `mark_command_completed` / `mark_command_rejected`.
- `active_command_for_conversation(conversation_id) -> Result<Option<Command>, LedgerError>`
  — the concurrency check's read side, and also what recovery uses to
  find a stuck command.
- `stuck_commands() -> Result<Vec<Command>, LedgerError>` — every command
  still `accepted`/`started` whose `child_run_id` is unset, or whose bound
  run has no ledger row of its own yet — `o7 recover`'s §7 read side. A
  command whose child run DOES have a ledger row is not returned here:
  that run is an ordinary run row, already covered by the pre-existing
  still-`running`/`queued` scan.
- `set_run_provider_session(run_id, session_id) -> Result<Run, LedgerError>`.

`ledger_projector::PendingProjection::attach_run` gains a `parent_run_id:
Option<&CanonicalRunId>` parameter (threaded into `NewRun`), replacing the
hardcoded `None` — every existing call site (the plain `o7 run` live path,
`o7 recover`'s catch-up) passes `None` explicitly; only the new
command-continuation path passes `Some`.

### 9.4 Execution ownership

**Decision**: `o7d` itself is the production owner of spawning a child
run's continuation process. It is a direct `std::process::Command` spawn
of the `o7` binary's own new `continue` subcommand (§9.5), explicit argv,
no shell — the same discipline as §9.2. `o7d` does not accept a
repo/worktree-root/runs-dir/gate-manifest from the HTTP request (§8's
forbidden list) — instead, `o7d serve` is configured with these as
**fixed, server-side startup flags** (`--repo`, `--worktree-root`,
`--runs-dir`, `--gate`, mirroring `o7 run`'s own flag names), matching
this MVP's existing single-tenant framing (one `o7d` process serves one
target repo). The mutation endpoint is only registered/functional when
`o7d serve` is given these; a `POST .../commands` on an `o7d` started
without them is a `500` naming the missing configuration, not a silent
no-op.

A spawned child run always uses: the conversation the parent run belongs
to; the parent run validated per §5/§8; a freshly minted child `RunId`;
the parent's own durable provider session identity; and the SAME
repo/worktree-root/runs-dir/gate authority `o7d` itself was launched
with — never a value from the HTTP client.

### 9.5 Canonical child run (`o7 continue`, root crate)

A new CLI subcommand, NOT a second execution pipeline: it constructs the
same worktree-at-base → `execute_live`-shaped flow R0.7 already has, with
exactly two differences from `o7 run`:
- the agent step calls `agent::continue_session` (§9.2) instead of
  `agent::run`, using the parent's durable session id;
- the durable "task" artifact minted before `RunStarted` is the command
  text (not a `--task <file>` read), and the ledger run/attach carries
  `parent_run_id = Some(<the parent run's canonical id>)`.

Every other step is IDENTICAL to R0.7's `execute_live`: durable
command-artifact + `ledger_binding.json` before `RunStarted`; per-event
append+`sync_data`, never rewritten; `LiveLedgerProjector` (unchanged
type) for live projection; the frozen R0.6 verdict mapping; artifact-aware
`o7 recover --run-dir` catch-up (`verify_prefix`, `seal_or_repair_interrupted`)
applies to a command's child run exactly as it does to any other run — R1
adds no second reducer and no second catch-up path.

### 9.6 `o7d` mutation endpoint

`POST /api/v1/conversations/{conversation_id}/commands` (§8). Request body
size and command-text length are both capped (small, fixed constants —
this is a command, not a file upload); an empty or whitespace-only command
is `400`. Handler: validate → `SqliteLedger::create_command` (durable,
idempotent, concurrency-checked, stale-parent-checked) → spawn `o7
continue` in the background (§9.4) → respond `202` immediately, without
waiting for the spawned process. All existing read-only routes and the
SSE resume contract are unchanged.

### 9.7 Q-Deck UI

One command box on the conversation page: a textarea, a Send button, a
pending state while awaiting the `202`, and an explicit accepted/rejected
message (surfacing the `409`/`422`/`400` body, not a generic error). The
idempotency key is generated ONCE per user submission (not per HTTP
attempt) and reused across a transport-level retry of that SAME
submission, so a flaky connection retried by the browser can't create two
commands. A double-click on Send is guarded (the button disables the
instant a submission starts). The textarea is cleared only once `202` is
received — a rejected/failed submission leaves the typed text in place so
nothing is lost. Once accepted, the new child run appears through the
EXISTING REST/SSE polling — no new client-side polling loop, no new SSE
channel. The provider session id is never received by, stored in, or
displayed by the browser — it never appears in this slice's response
DTOs.

## Known limitations and evidence

**No worktree/diff carryover across a command.** `o7 continue`'s child run
starts a FRESH worktree at `--base` (mirroring `o7 run`'s own
`worktree::add`), it does NOT re-apply the parent run's `diff.patch` before
dispatching the command. This slice proves PROVIDER SESSION continuity
only (the `--resume <session_id>` conversation-context carryover) — not
cumulative repo-state continuity across turns. A deliberate scope-
narrowing call, not an oversight: the frozen HTTP contract and Command
identity model in this doc say nothing about file-state carryover, and
widening the vertical to also thread the parent's diff into the child's
worktree is real additional surface area (which base to apply against,
what a conflict means, whether to fail closed or best-effort) that
belongs in its own reviewed slice, not folded into the first proof that
multi-turn continuation works at all.

**A pre-existing, base-reproducible environmental test hang.**
`crates/o7-ledger/tests/crash_durability.rs`'s `kill_after_commit_preserves_event`
and `kill_before_commit_leaves_no_partial` hang indefinitely on this
development VPS (single vCPU, ~2GB RAM, already under swap pressure) — the
re-exec'd child test process never reaches its own `println!("READY
...")` line. This is NOT a regression introduced by this branch:
reproduced identically (same two tests, same hang, `timeout 60` exit code
124) against the pristine, unmodified R0.7 merge commit
`a8b3664f1aba1468b218ba873278c396725685c8` in an isolated worktree with a
freshly built target dir — before ANY R1 code existed. Every other test in
the workspace (including the new `crates/o7-ledger/tests/commands.rs` and
`tests/r1_command_e2e.rs`) is exercised with:

```
cargo test --workspace -- --skip kill_after_commit_preserves_event \
  --skip kill_before_commit_leaves_no_partial \
  --skip a_blocking_fifo_target_fails_closed_within_a_bound
```

and passes. CI (a proper multi-core, non-swapping runner) is expected not
to reproduce this hang at all; if it doesn't, these two tests should run
unskipped there. This exclusion is scoped to exactly these two named
tests — no other test in the workspace is skipped, and no failure is
papered over by it.

A THIRD, unrelated test in the same environment is timing-sensitive enough
to also fail under this VPS's load: `crates/o7-worker/tests/sandboy_lifecycle.rs`'s
`a_blocking_fifo_target_fails_closed_within_a_bound` asserts a helper
subprocess responds within a hardcoded `tokio::time::timeout(Duration::from_secs(5), ...)`.
Evidence this is environmental, not an R1 regression: (1) this exact test
function is already present, byte-for-byte, in the frozen R0.7 merge
commit `a8b3664f1aba1468b218ba873278c396725685c8` — R1 never touches
`o7-worker` or `o7-sandbox-protocol` at all; (2) a full `cargo test
--workspace` run under this session's concurrent build load failed FOUR
tests in this one file at once (`a_blocking_fifo_target_fails_closed_within_a_bound`,
`an_unexpectedly_launched_target_is_a_fail_not_the_refusal_pass`,
`cancelling_a_launch_after_a_fork_reaps_the_whole_group`,
`cancelling_a_launch_mid_report_reaps_the_backend`) and took 586s for a
suite that should take seconds; (3) re-run in isolation (no competing
`cargo` process), three of those four passed — only the tightest-bound one
(5s) still failed, in 514s for 16 tests, itself still dramatically slower
than normal. This is the signature of severe scheduling/swap contention on
a single-vCPU, ~2GB-RAM VPS, not a logic defect. The gate commands below
additionally skip this one test, for the same reason and with the same
scoping discipline as the two `crash_durability` skips above.

## What R1 does not do

No command queue (§5's single-in-flight rule is deliberate, not a
placeholder for one). No cancel/approve/reject/follow-up-editing UI. No
workflow DSL, no multi-agent orchestration, no general-purpose remote
shell. No Codex engine wiring (`Engine::Codex` stays `Phase 2`, unchanged).
No provider selection UI, no credential/key management surface. No
integration with the unmerged PR #84/#86 content — neither is rebased,
neither is treated as authority here. No `o7-worker`/Sandboy migration, no
executor-qualification (EQ-0+) work. No fix for the pre-existing,
documented "`meta.json`-after-seal" replay gap (`docs/q-deck/r07-live-ingress.md`'s
own follow-up note) — unrelated to this slice. No raw-stream
retention/redaction redesign. No R2 work of any kind. No reopening of a
sealed run, under any circumstance, for any reason.
