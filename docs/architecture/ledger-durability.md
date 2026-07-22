# Ledger durability

The `o7-ledger` crate is the durable source of truth. This document states exactly
what "durable" means, which failures are covered, and where SQLite's guarantee ends
and the filesystem's begins — so nothing here promises more than the configuration
delivers.

## SQLite configuration (set on every open, `sqlite.rs::init`)
| Pragma | Value | Why |
|---|---|---|
| `journal_mode` | `WAL` | Write-ahead log: readers never block the single writer; a committed transaction is durable in the WAL before checkpoint. |
| `synchronous` | `FULL` | fsync the WAL at every commit (and on checkpoint). Required for the power-loss guarantee below; `NORMAL` would risk the last commit on power loss. |
| `foreign_keys` | `ON` | Referential integrity is enforced, not decorative (an event for a missing conversation is rejected). |
| `busy_timeout` | `5000 ms` | Concurrent writers wait-and-retry instead of erroring immediately; combined with `BEGIN IMMEDIATE` this serializes appends across connections. |

These pragmas are **verified effective at open** (`sqlite.rs::verify_effective_pragmas`):
foreign keys must read back on for every database, and WAL + `synchronous = FULL` are
asserted for file-backed databases (they are inert, hence not asserted, for
`:memory:`). If a filesystem silently refused WAL, open fails closed rather than
letting the durability claims above become a lie.

Every mutating operation runs inside a `BEGIN IMMEDIATE` transaction and is observable
only after `commit()` returns. `append_event` (and the typed `create_*`/`*_run`
methods) return their `PersistedEvent`/entity **only after a successful commit** —
never before.

## What "committed" means
An event/entity is *committed* once the transaction that inserted it has returned
from `commit()`. Under WAL + `synchronous = FULL`, that commit has fsynced the WAL
frames (including the commit frame) to stable storage before returning. The value is
then visible to every subsequent connection.

## Failures covered
| Failure | Guarantee | Evidence |
|---|---|---|
| **Process killed (SIGKILL) AFTER commit** | The event survives. The WAL commit frame was fsynced before `commit()` returned; a new connection replays it. | `tests/crash_durability.rs::kill_after_commit_preserves_event` — a real subprocess is SIGKILLed after printing `READY`, and the conversation is present on reopen. |
| **Process killed BEFORE commit** | No partial state. The open transaction has no commit frame, so WAL recovery discards it. | `tests/crash_durability.rs::kill_before_commit_leaves_no_partial` — a real subprocess holds an uncommitted `INSERT`, is SIGKILLed, and the row is absent on reopen. |
| **Rolled-back transaction** | No half-event, and no sequence is consumed (the next append has no gap). | `tests/append_replay.rs::rollback_leaves_no_half_event`. |
| **Host power loss** | A committed transaction survives, **provided** the OS and storage honor `fsync`. `synchronous = FULL` issues the durability barrier at commit; this is the strongest SQLite offers short of the OS lying. | Config (`synchronous = FULL`); not machine-tested here (would need real power-cut hardware). |
| **Corrupt / unreadable database** | Fails closed at `open()` — never silently accepted. | `open()` runs `PRAGMA quick_check` and returns `LedgerError::Integrity` (or a SQLite error) on anything but `ok`; `tests/migrations.rs::corrupt_database_fails_closed`. |

## Where the SQLite guarantee ends and the filesystem's begins
- SQLite's durability holds **only to the extent the OS/filesystem honor `fsync`**. If
  the storage layer acknowledges an `fsync` it has not actually flushed (some consumer
  SSDs with volatile write caches, some network filesystems), a committed transaction
  can still be lost on power loss. That is outside SQLite's — and this crate's —
  control.
- The `-wal` and `-shm` sidecar files are part of the durable state. Deleting a `-wal`
  that still holds un-checkpointed commits loses those commits. Do not move/delete the
  database's sidecars while a connection is (or was) open with uncheckpointed data.
- Metadata ordering (that the file's directory entry is durable) is the filesystem's
  job; SQLite assumes a POSIX-ish `fsync` contract.

## What is deliberately NOT guaranteed
- Durability of an **uncommitted** transaction (by design — that is the atomicity we
  rely on for "no partial event").
- Anything after a torn write on hardware that lies about `fsync`.
- Cross-machine replication or backup — out of scope for PR 1.

## Open sequence (exact order in `sqlite.rs::init`)
1. Set `busy_timeout`, `foreign_keys`, `synchronous`, `journal_mode = WAL`, and
   **verify** they took effect (fail closed otherwise).
2. **Refuse a too-new database:** if `user_version` > the version this build supports,
   return `SCHEMA_TOO_NEW` — an older binary must never write a newer schema.
3. Apply versioned migrations (idempotent; no-op on an up-to-date DB).
4. Run `PRAGMA quick_check` (the cheaper, justified variant of `integrity_check`); a
   non-`ok` result fails closed.
5. **Validate the live schema** against the expected tables/columns, so a database that
   merely *claims* the current `user_version` but is structurally incomplete fails
   closed rather than being written to.

## Recovery (separate, caller-driven)
- `recover_scan()` reports runs/attempts still in `running` state — i.e. interrupted by
  whatever stopped the previous process. It is **read-only**.
- **Opening never changes a status.** Marking those as `interrupted` is an explicit,
  caller-driven `mark_interrupted()` in its own transaction (which emits
  `run.interrupted`). The control plane decides; the ledger does not.
- A run leaving `running` (complete/fail/cancel/interrupt) atomically finishes its one
  running attempt, and `resume_interrupted_run()` atomically transitions
  `interrupted → running` plus a fresh attempt — so a run/attempt pair is never left
  inconsistent.
