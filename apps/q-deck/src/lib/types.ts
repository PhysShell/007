// Mirrors crates/o7d/src/dto.rs exactly. Q-Deck derives every displayed value
// from these — no field is invented client-side, per the R0 source-of-truth
// rule (see docs/q-deck/architecture.md).

export interface HealthDto {
  schema_version: number;
  status: string;
}

export type ConversationStatus = "open" | "closed";

export interface ConversationDto {
  schema_version: number;
  conversation_id: string;
  created_at: number;
  status: ConversationStatus;
}

export type RunStatus =
  | "queued"
  | "running"
  | "completed"
  | "failed"
  | "cancelled"
  | "interrupted"
  | "blocked"
  | "error";

/** Q-Deck A0.5 (`docs/q-deck/a0-candidate-state.md` §9). The four
 * `materialization_status` values `crates/o7d/src/canonical.rs`'s
 * `candidate_projection` currently produces. Kept as an OPEN union
 * (`| (string & {})`) rather than a closed one deliberately: this is a
 * server-computed value, not a client invariant, and a future server build
 * adding a fifth value must render safely (see `materializationTone`/
 * `materializationLabel` below) rather than the type system silently lying
 * about exhaustiveness. */
export type MaterializationStatus =
  | "materialized"
  | "not_applicable"
  | "failed"
  | "verification_failed"
  | (string & {});

export interface RunDto {
  schema_version: number;
  run_id: string;
  conversation_id: string;
  parent_run_id: string | null;
  agent: string;
  role: string;
  status: RunStatus;
  created_at: number;
  finished_at: number | null;
  /** Q-Deck A0.5: candidate-state provenance — the run this one's cumulative
   * candidate state continues from. Deliberately a SEPARATE concept from
   * `parent_run_id` above (run lineage): they commonly coincide but the
   * backend makes no promise they always do, and neither field is derived
   * from the other here.
   *
   * All three candidate-state fields below are
   * `#[serde(skip_serializing_if = "Option::is_none")]` on the wire
   * (`crates/o7d/src/dto.rs`) — ABSENT from the JSON object entirely when
   * unset, never present as an explicit `null` the way `parent_run_id`/
   * `finished_at` above are. Optional (`?`) models that absence correctly. */
  candidate_source_run_id?: string | null;
  candidate_tree_oid?: string | null;
  materialization_status?: MaterializationStatus | null;
}

export interface EventDto {
  schema_version: number;
  event_id: string;
  conversation_id: string;
  run_id: string | null;
  attempt_id: string | null;
  sequence: number;
  event_type: string;
  created_at: number;
  payload: unknown;
}

export interface PageDto<T> {
  schema_version: number;
  items: T[];
  next_before: string | null;
}

export interface EventPageDto {
  schema_version: number;
  items: EventDto[];
  next_after: number | null;
}

export interface ErrorDto {
  schema_version: number;
  error: string;
  code: string;
}

/** Q-Deck R1 (`docs/q-deck/r1-command.md` §8). A command's own bookkeeping
 * status — NOT the child run's verdict; `"completed"` here means only "the
 * child run reached a sealed terminal status," nothing about which one. */
export type CommandStatus = "accepted" | "started" | "completed" | "rejected";

/** Response from `POST /api/v1/conversations/{id}/commands`, sent only
 * after durable acceptance. Versioned independently of `API_SCHEMA_VERSION`
 * — this is its own wire surface, starting at 1 (§8). */
export interface CommandAcceptedDto {
  schema_version: number;
  command_id: string;
  conversation_id: string;
  parent_run_id: string;
  run_id: string;
  status: CommandStatus;
}

/** A run is currently doing something (queued to start, or running). Does
 * NOT mean "anything else is terminal" — `interrupted` is neither active nor
 * sealed, see `isSealedRun`. */
export function isActiveRun(status: RunStatus): boolean {
  return status === "queued" || status === "running";
}

/** A run whose verdict is fixed and will never change again. `interrupted`
 * is deliberately NOT included: in `o7-ledger`, an interrupted run is
 * resumable (`resume_interrupted_run`) back to `running` — it is a pause,
 * not a sealed outcome. A client must keep watching an interrupted run (e.g.
 * keep polling it) in case it resumes; only completed/failed/cancelled/
 * blocked/error are safe to stop watching. `blocked`/`error` (Q-Deck R0.6,
 * `docs/q-deck/r06-verdict-fidelity.md`) are the ledger's projection of
 * `o7-run::Verdict`'s own sealed `Blocked`/`Error` — both sealed here exactly
 * as they are sealed there, and never the same thing as `interrupted`. */
export function isSealedRun(status: RunStatus): boolean {
  return (
    status === "completed" ||
    status === "failed" ||
    status === "cancelled" ||
    status === "blocked" ||
    status === "error"
  );
}
