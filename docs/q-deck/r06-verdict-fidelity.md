# Q-Deck R0.6 — canonical verdict fidelity

## Purpose

Close the semantic mismatch that currently prevents an honest live `o7-run` →
`o7-ledger` producer integration:

- `o7-run` has sealed verdicts: `Pass`, `Fail`, `Blocked`, `Error`
  (`crates/o7-run/src/state.rs::Verdict`, fixed only once a run is `Sealed`).
- `o7-ledger` has `RunStatus`: `Completed`, `Failed`, `Cancelled`,
  `Interrupted` (plus the non-sealed `Queued`/`Running`).
- `Interrupted` is resumable and is **not** `Error` — a genuinely different
  concept (see `docs/q-deck/r05-live-readiness.md`'s "open seam" section,
  written when R0.5 first ran into this and correctly refused to conflate
  the two).
- A live producer therefore cannot currently project every `o7-run` verdict
  into the ledger without collapsing meaning: there is no ledger status for
  `Blocked` or `Error` at all.

**This vertical aligns the ledger/API/Q-Deck vocabulary only.** It does
**not** wire the live producer — no code here calls into `o7-run`,
`o7-worker`, or Sandboy, and no mutation surface is added anywhere.

## Frozen semantic mapping

The eventual live producer must be able to express:

| `o7-run::Verdict` | `o7_ledger::RunStatus` | sealed? |
|---|---|---|
| `Pass` | `Completed` | yes |
| `Fail` | `Failed` | yes |
| `Blocked` | `Blocked` | yes |
| `Error` | `Error` | yes |

Existing meanings are unchanged and restated here for the record:

- `Cancelled` — an explicit cancellation outcome. Sealed.
- `Interrupted` — an unsealed, resumable operational interruption
  (`resume_interrupted_run` can bring it back to `Running`). **Never**
  presented as `Error`, `Blocked`, or terminal/sealed, anywhere in the
  ledger, `o7d`, or Q-Deck.
- `Completed`, `Failed`, `Blocked`, `Error`, `Cancelled` are all sealed —
  `RunStatus::is_terminal()` returns `true` for exactly this set.

## Why `Interrupted` and `Error` are not the same thing

This is worth stating precisely, because R0.5's first draft got exactly this
wrong once already: `o7-run::Verdict::Error` is a SEALED, terminal judgment
— fixed only once a run reaches `RunPhase::Sealed`, and (per
`src/events.rs`, the root `o7 run` path) it is what a non-clean agent exit
already produces today, even with every gate green. `o7_ledger::Interrupted`
is UNSEALED and resumable — it means "the process running this stopped
before reaching any verdict at all," not "the verdict was an error." R0.6
gives the ledger a genuine `Error` status distinct from `Interrupted`
precisely so a future live producer never has to choose between two wrong
answers (calling a sealed error verdict "interrupted," or calling a
resumable interruption "error").

## Implementation map (written before any code change)

1. **SQLite constraints / schema attestation.** `run.status` and
   `run_attempt.status` are `TEXT NOT NULL` with no `CHECK` constraint in
   schema v1 — validity is enforced entirely by the Rust-level closed-enum
   boundary (`RunStatus::parse`, no catch-all). `event.event_type` is
   deliberately left unconstrained (its own doc comment: forward-compatible,
   a newer value reads back rather than failing — future event kinds are
   meant to land without a ledger migration). `run.status`/
   `run_attempt.status` carry no such promise: they are tightly governed by
   the central transition tables and parsed on every read. R0.6 adds real
   `CHECK` constraints to `run.status` and `run_attempt.status` only —
   never to `event.event_type`, which stays open-ended by design.
2. **Migration required.** `CURRENT_SCHEMA_VERSION` 1 → 2. SQLite has no
   `ALTER TABLE ADD CONSTRAINT` — a `CHECK` constraint requires a full
   table rebuild (create-new → copy → drop → rename). This surfaced two
   real, previously-latent gaps in the migration system itself (present
   since v1 shipped, never exercised because there had only ever been one
   migration):
   - `migrations::apply()` ran all pending migrations in ONE shared
     transaction, with `PRAGMA foreign_keys` already `ON` (set in `init()`
     before `apply()` runs). Toggling `foreign_keys` is a no-op inside a
     transaction, so a table rebuild couldn't safely disable FK enforcement
     around itself under the old structure. Fixed: each migration now runs
     in its own transaction, with `foreign_keys OFF` → rebuild → `PRAGMA
     foreign_key_check` → `foreign_keys ON` around it (SQLite's own
     documented-safe pattern for this exact operation).
   - `validate_schema()`'s reference database was built by executing the
     literal `SCHEMA_V1` string directly — never generalized to "run every
     migration in order." Harmless with exactly one migration; wrong the
     moment a second one exists. Fixed: the reference is now built by
     calling `apply()` itself on a fresh in-memory connection, so
     attestation always mirrors exactly what a real upgraded database looks
     like, for any number of migrations.
3. **Transition-table changes.** `run_transition_allowed`: add
   `(Running, Blocked)` and `(Running, Error)`. `attempt_transition_allowed`:
   same two, for the new `AttemptStatus::Blocked`/`Error` (added so a
   blocked/errored run's attempt is never silently mislabeled `Completed` by
   `terminal_attempt_status`'s catch-all — the exact kind of meaning-collapse
   this vertical exists to eliminate). `RunStatus::is_terminal()` extended
   to include `Blocked | Error`, still excluding `Interrupted`.
4. **`EventType` wire names.** `RunBlocked` → `"run.blocked"`, `RunErrored`
   → `"run.errored"`. New `SqliteLedger::block_run`/`error_run`, mirroring
   `fail_run`/`cancel_run` exactly.
5. **`o7d` DTO/schema compatibility.** `RunDto.status` and
   `EventDto.event_type` are already bare-string pass-throughs of the ledger
   value (`r.status.as_str().to_owned()`, `e.event_type` verbatim) — no DTO
   code change is needed for the new values to flow through. `?status=`
   filtering (`parse_statuses`) calls `RunStatus::parse` generically, so
   `blocked`/`error` become valid filter values automatically and anything
   else still 400s exactly as before.
6. **API schema version bumped: `API_SCHEMA_VERSION` 1 → 2.** Adding
   `blocked`/`error` to `status` is *not* the same kind of change as
   `event_type`'s own forward-compatible widening, despite both being "a new
   string value inside an existing field": Q-Deck never branches on
   `event_type`'s exact value, but `RunStatus` is a closed union its client
   logic (`isSealedRun`) uses for control flow. An old, not-yet-upgraded
   Q-Deck build talking to a new o7d would otherwise accept the response
   (same version number, so `checkSchema` lets it through) and then silently
   poll a blocked/errored run forever, having no way to recognize the new
   value as sealed — a real, previously-unrecognized incompatibility, not a
   safe additive change. The bump makes that combination fail closed instead:
   an old client rejects the mismatched version outright
   (`UnsupportedSchemaError`) rather than misreading it. `apps/q-deck/src/lib/api.ts`'s
   `EXPECTED_SCHEMA_VERSION` bumped to match; `apps/q-deck/src/lib/types.ts`'s
   own unused, drift-prone duplicate of this constant was deleted rather than
   also bumped. Proven by dedicated tests on both sides — the new value
   round-tripping under the bumped version, and (separately) the *old*
   version (1) now being rejected rather than silently accepted — not just
   asserted.
7. **Q-Deck.** `lib/types.ts`: add `"blocked" | "error"` to the `RunStatus`
   union; extend `isSealedRun` to include them (`isActiveRun` is unchanged —
   neither is active). `StatusBadge.svelte`: two new tone-map entries,
   reusing the existing `tone-warn`/`tone-bad` classes (no new CSS).
   `Dashboard.svelte`/`RunPage.svelte`/`ConversationPage.svelte` need **no
   further changes** — none of them enumerate statuses directly; they only
   ever call `isActiveRun`/`isSealedRun`, so the new values flow through
   Dashboard's "Recent" bucket and RunPage's stop-polling-once-sealed logic
   automatically.
8. **Golden transcript.** `GoldenOutcome` (`Pass | Fail | Interrupted` as of
   R0.5) gains `Blocked | Error` in all three mirrored copies (o7-ledger
   tests, o7d tests, Q-Deck test-support) plus the `seed_r05_fixture`
   example, proved at every existing layer.

## What R0.6 does not do

- No live producer wiring — `o7-run`/`o7-worker`/Sandboy are untouched.
- No mutation surface — no start/stop/approve/reject/follow-up/provider
  selection anywhere, ledger or API or Q-Deck.
- No change to `crates/o7-run`, `src/events.rs`, or the root `o7 run` path.

**The next slice after R0.6 is the real live-ingress vertical** — wiring an
actual `o7-run`/`o7-worker` execution into `o7-ledger`'s write API using the
mapping this document freezes — **not** R1 ("Command") mutations. R1 was
already deferred behind a proven read path in `docs/q-deck/architecture.md`;
that ordering is unchanged by this slice.
