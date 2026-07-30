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
  | "interrupted";

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

/** A run is still doing something; anything else is a terminal outcome. */
export function isActiveRun(status: RunStatus): boolean {
  return status === "queued" || status === "running";
}
