# Ledger schema v1

Schema version is tracked with SQLite's `user_version` pragma; `CURRENT_SCHEMA_VERSION
= 1`. Migrations are ordered `(version, sql)` and applied in a single transaction on
every open — already-applied versions are skipped, an empty DB gets the full set, and
re-running is a no-op. Never edit a shipped migration in place; add a new one.

## Entities

### `conversation`
| column | type | notes |
|---|---|---|
| `conversation_id` | TEXT PK | UUID string; the unit the event `sequence` is scoped to |
| `created_at` | INTEGER | unix millis (metadata only) |
| `status` | TEXT | `open` \| `closed` |

### `run`
| column | type | notes |
|---|---|---|
| `run_id` | TEXT PK | |
| `conversation_id` | TEXT FK → conversation | |
| `parent_run_id` | TEXT FK → run, nullable | supports delegation trees (populated in later PRs) |
| `agent` | TEXT | opaque label; the ledger does not interpret it |
| `role` | TEXT | opaque label |
| `status` | TEXT | `queued`/`running`/`completed`/`failed`/`cancelled`/`interrupted` |
| `created_at` | INTEGER | |
| `finished_at` | INTEGER, nullable | set only on a terminal status |

Index: `idx_run_conversation(conversation_id)`.

### `run_attempt`
| column | type | notes |
|---|---|---|
| `attempt_id` | TEXT PK | |
| `run_id` | TEXT FK → run | |
| `attempt_number` | INTEGER | `UNIQUE(run_id, attempt_number)` |
| `status` | TEXT | `running`/`completed`/`failed`/`cancelled`/`interrupted` |
| `started_at` | INTEGER | |
| `finished_at` | INTEGER, nullable | |

### `event`
| column | type | notes |
|---|---|---|
| `event_id` | TEXT PK | |
| `conversation_id` | TEXT FK → conversation | |
| `run_id` | TEXT FK → run, nullable | |
| `attempt_id` | TEXT FK → run_attempt, nullable | |
| `sequence` | INTEGER | **`UNIQUE(conversation_id, sequence)`** |
| `event_type` | TEXT | one of the closed PR-1 set below |
| `schema_version` | INTEGER | per-event payload schema version |
| `created_at` | INTEGER | |
| `payload_json` | TEXT | JSON; round-trips without meaning change |

Index: `idx_event_conversation_sequence(conversation_id, sequence)`.

### `idempotency_record`
| column | type | notes |
|---|---|---|
| `scope` | TEXT | part of PK |
| `key` | TEXT | part of PK; `PRIMARY KEY(scope, key)` |
| `request_digest` | TEXT | SHA-256 of the canonical request |
| `result_reference` | TEXT | id of the produced entity/event |
| `created_at` | INTEGER | |

## Cross-entity integrity (the ledger enforces these itself)
Independent foreign keys are not enough — these composite constraints stop the
inconsistent references a less-careful caller could otherwise create:
- `run` has `UNIQUE(conversation_id, run_id)`; `event` has
  `FOREIGN KEY (conversation_id, run_id) REFERENCES run(conversation_id, run_id)`, so an
  event can only reference a run **in its own conversation**.
- `run_attempt` has `UNIQUE(run_id, attempt_id)`; `event` has
  `FOREIGN KEY (run_id, attempt_id) REFERENCES run_attempt(run_id, attempt_id)` plus
  `CHECK (attempt_id IS NULL OR run_id IS NOT NULL)`, so an event's attempt must belong
  to its run.
- `run` has `FOREIGN KEY (conversation_id, parent_run_id) REFERENCES run(conversation_id,
  run_id)`, so a **parent run must live in the same conversation**.
- `CREATE UNIQUE INDEX idx_one_running_attempt ON run_attempt(run_id) WHERE status =
  'running'` — **at most one running attempt per run**.

(Composite FKs use MATCH SIMPLE: when a nullable column such as `run_id`/`parent_run_id`
is NULL, the FK is not enforced, which is exactly what conversation- or run-less events
and root runs need.)

## Schema version guards (on open)
- A `user_version` **newer** than this build supports → `SCHEMA_TOO_NEW` (an old binary
  must not write a new DB).
- After migrating, the live schema is validated against the expected tables/columns; a
  DB that only *claims* the current version but is incomplete → `INTEGRITY` failure.

## The sequence invariant
- `sequence` is **per conversation**, not global — there is no single global cursor.
- Uniqueness is enforced by `UNIQUE(conversation_id, sequence)`.
- The next value is computed (`MAX(sequence)+1`) **and** the row inserted in **one**
  `BEGIN IMMEDIATE` transaction. `IMMEDIATE` + `busy_timeout` serialize writers across
  connections, so parallel appends to one conversation get `1, 2, 3, …` with no dups
  and no race gaps. A rolled-back transaction consumes no sequence.

## Cursor contract (`read_events`)
`read_events(conversation, after_sequence, limit)` returns:
```
WHERE conversation_id = ? AND sequence > ?   -- exclusive cursor
ORDER BY sequence ASC
LIMIT min(limit, MAX_READ_LIMIT)             -- hard cap = 1000
```
- The cursor is **exclusive** (`> after`), so paginating with `after = last.sequence`
  loses and duplicates nothing.
- `after_sequence = None` starts at the beginning.
- Repeated queries are deterministic; ordering is stable.
- An **unknown conversation returns an empty list**, never an error masquerading as
  corruption.
- Timestamps are never the cursor — ordering is always by `sequence`.

## Idempotency
Scopes: `create-conversation`, `create-run`, `append-user-message`. For a given
`(scope, key)`:
- **Same digest** → the prior `result_reference` is returned; nothing new is created.
- **Different digest** → `IDEMPOTENCY_CONFLICT`; nothing changes.
It is never `INSERT OR IGNORE` without a digest check — two different requests must
never be collapsed into one identity. The check + the guarded operation + the record
insert all happen in one transaction.

## Event set (closed for PR 1)
`conversation.created`, `run.created`, `run.started`, `run.completed`, `run.failed`,
`run.cancelled`, `run.interrupted`, `user.message`, `system.note`.

Deliberately absent until PR 4: Claude/Codex-specific events, tool calls, permission
modes, model drift, delegation, artifacts, gates.

## State transitions (enforced centrally in `transitions.rs`, default-deny)
**Run:** `queued → running`; `running → {completed, failed, cancelled, interrupted}`;
`interrupted → running` (via a new attempt). Forbidden (examples): `completed →
running`, `failed → completed`, `cancelled → completed`, and anything else.

**Attempt:** `running → {completed, failed, cancelled, interrupted}`. Attempts never
restart — a new attempt row is created instead. `create_attempt` requires the run to be
`running` and refuses a second running attempt.

**Atomic couplings:** a run leaving `running` (complete/fail/cancel/interrupt) finishes
its running attempt in the same transaction; `resume_interrupted_run` transitions
`interrupted → running` **and** creates a fresh running attempt in one transaction (first
closing any lingering running attempt). So run and attempt state are never left
inconsistent.
