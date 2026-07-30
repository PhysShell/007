// Mirrors crates/o7d/src/dto.rs exactly. Q-Deck derives every displayed value
// from these — no field is invented client-side, per the R0 source-of-truth
// rule (see docs/q-deck/architecture.md).

export const API_SCHEMA_VERSION = 1;

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
