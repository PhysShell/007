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

/** Q-Deck A0.5 corrective (fresh exact-head Codex P1): `get_run`'s
 * candidate-state projection is itself best-effort — `crates/o7d/src/routes.rs`'s
 * own doc comment says so explicitly. Under replay-limiter saturation, or if
 * the blocking projection task panics, ONE response omits all three
 * candidate fields entirely, exactly as if `exec` were unconfigured — even
 * for a run whose candidate state genuinely IS captured or materialized.
 *
 * Checking KEY PRESENCE, never value truthiness, is what makes this safe:
 * an ABSENT `materialization_status` (this poll got no projection at all —
 * transient, say nothing), an explicit `null` (the real server never sends
 * this today, only omits the key, but this is treated the same
 * defensively), and the literal string `"not_applicable"` (a genuine,
 * complete, trustworthy answer — no candidate contract for this run,
 * settled, never retried) are three different meanings a plain truthiness
 * check would wrongly collapse into one. */
export function hasCandidateProjection(run: RunDto): boolean {
  return (
    "materialization_status" in run &&
    run.materialization_status !== null &&
    run.materialization_status !== undefined
  );
}

/** Merges a freshly fetched `RunDto` over the previously displayed one:
 * every field always comes from the fresh response EXCEPT the three
 * candidate-state fields, which are preserved from `previous` when the
 * fresh response carries no projection at all
 * (`!hasCandidateProjection(next)`) — otherwise a single
 * transiently-saturated poll would silently erase a candidate projection
 * this page already correctly displayed from an earlier poll. */
export function mergeRunProjection(previous: RunDto | null, next: RunDto): RunDto {
  if (hasCandidateProjection(next) || !previous) return next;
  return {
    ...next,
    candidate_source_run_id: previous.candidate_source_run_id,
    candidate_tree_oid: previous.candidate_tree_oid,
    materialization_status: previous.materialization_status,
  };
}

/** Q-Deck A0.5 corrective round 6 (fresh exact-head Codex P1, PR #110):
 * a PRESENT `materialization_status` is not automatically a FINAL one.
 * `crates/o7d/src/canonical.rs`'s own `candidate_projection` maps every
 * I/O error resolving the record directory or reading `events.jsonl`
 * (permission, descriptor exhaustion, a transient device hiccup — never
 * distinguished from a permanent condition) to `"failed"`, and every
 * parse/replay error — including an `ArtifactResolver` I/O failure
 * inside `verify_prefix`, not just genuine tamper/corruption — to
 * `"verification_failed"`. Treating either as a trustworthy final answer
 * (the previous round's `hasCandidateProjection` check alone) meant a
 * purely transient filesystem hiccup could permanently strand a sealed,
 * genuinely-materializable run on "failed" forever, with no further poll
 * ever attempted once the underlying condition cleared.
 *
 * Deliberately an ALLOWLIST (`materialized`/`not_applicable`), not a
 * denylist (`!== "failed"`) — a denylist only ever closes the ONE
 * specific status a given review pass happened to name, and silently
 * keeps trusting every OTHER status (including a value this build has
 * never seen) as final. An allowlist is closed by construction: only
 * these two literal, server-documented TERMINAL outcomes ever stop
 * polling; `"failed"`, `"verification_failed"`, and any future
 * unrecognized value all keep retrying (on the same backed-off, bounded-
 * only-by-page-lifetime schedule `hasCandidateProjection`'s own retry
 * loop already uses) until either a final answer arrives or the page is
 * left. */
export function isFinalCandidateProjection(run: RunDto): boolean {
  return run.materialization_status === "materialized" || run.materialization_status === "not_applicable";
}
