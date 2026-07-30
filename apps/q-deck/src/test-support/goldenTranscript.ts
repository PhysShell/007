// Mirrors the Rust golden synthetic-run transcript
// (crates/o7-ledger/tests/support/mod.rs, crates/o7d/tests/support/mod.rs) as
// typed DTOs for Q-Deck's own frontend proof tests — see
// docs/q-deck/r05-live-readiness.md for the single canonical definition all
// three copies implement. Test-only: nothing outside `*.test.ts` files
// imports this module, so it is not part of the built production bundle.

import type { EventDto, EventPageDto, RunDto, RunStatus } from "../lib/types";

// Only "pass"/"fail" are sealed/terminal outcomes (RunStatus completed/
// failed). "interrupted" is NOT a third co-equal outcome of the same kind —
// it is a resumable pause (o7-ledger's resume_interrupted_run can bring it
// back to "running"), kept in this union only for apply_golden_transcript's
// convenience. Never call it "terminal" or "ERROR": that name belongs to
// o7-run::Verdict::Error, a genuinely different SEALED concept this fixture
// does not represent — the projection from o7-run::Verdict onto
// o7-ledger's status vocabulary is an open seam, not decided here (see
// docs/q-deck/r05-live-readiness.md).
export type GoldenOutcome = "pass" | "fail" | "interrupted";

export const GOLDEN_CONVERSATION_ID = "r05-golden-conversation";
export const GOLDEN_RUN_ID = "r05-golden-run";
const BASE_CREATED_AT = 1_700_000_000_000;

function outcomeStatus(outcome: GoldenOutcome): RunStatus {
  switch (outcome) {
    case "pass":
      return "completed";
    case "fail":
      return "failed";
    case "interrupted":
      return "interrupted";
  }
}

function outcomeEventType(outcome: GoldenOutcome): string {
  switch (outcome) {
    case "pass":
      return "run.completed";
    case "fail":
      return "run.failed";
    case "interrupted":
      return "run.interrupted";
  }
}

/** The run mid-transcript, before any outcome transition — `run.started`
 * has happened, nothing else has. This is what a real client sees while a
 * run is still active, polled repeatedly until it reaches one of the
 * outcomes below (or, from "interrupted", potentially back to this same
 * "running" shape via a resume — see `goldenRunResumed`). */
export function goldenRunActive(overrides: Partial<RunDto> = {}): RunDto {
  return {
    schema_version: 1,
    run_id: GOLDEN_RUN_ID,
    conversation_id: GOLDEN_CONVERSATION_ID,
    parent_run_id: null,
    agent: "claude",
    role: "implementer",
    status: "running",
    created_at: BASE_CREATED_AT,
    finished_at: null,
    ...overrides,
  };
}

/** The run after its outcome transition. */
export function goldenRun(outcome: GoldenOutcome, overrides: Partial<RunDto> = {}): RunDto {
  const status = outcomeStatus(outcome);
  return {
    schema_version: 1,
    run_id: GOLDEN_RUN_ID,
    conversation_id: GOLDEN_CONVERSATION_ID,
    parent_run_id: null,
    agent: "claude",
    role: "implementer",
    status,
    created_at: BASE_CREATED_AT,
    // interrupted is resumable, not a sealed/closed state — finished_at
    // stays unset, exactly mirroring the real ledger (see the Rust golden
    // transcript tests for the full rationale). Never backfilled to look
    // like a closed run just because this is the "outcome" fixture.
    finished_at: status === "interrupted" ? null : BASE_CREATED_AT + 5_000,
    ...overrides,
  };
}

/** The regression this transcript must prove: an interrupted run is not a
 * dead end. What a client sees after `o7-ledger`'s `resume_interrupted_run`
 * — back to `running`, `finished_at` unset, same run identity. */
export function goldenRunResumed(overrides: Partial<RunDto> = {}): RunDto {
  return goldenRunActive(overrides);
}

/** All 7 events of the full transcript, in order — mirrors exactly what
 * `o7-ledger`'s production write API produces via `apply_golden_transcript`:
 * `conversation.created -> run.created -> run.started -> user.message x2 ->
 * system.note -> run.{completed,failed,interrupted}`. */
export function goldenEvents(outcome: GoldenOutcome): EventDto[] {
  const types = [
    "conversation.created",
    "run.created",
    "run.started",
    "user.message",
    "user.message",
    "system.note",
    outcomeEventType(outcome),
  ];
  return types.map((event_type, i) => ({
    schema_version: 1,
    event_id: `r05-golden-event-${i + 1}`,
    conversation_id: GOLDEN_CONVERSATION_ID,
    // conversation.created precedes the run's own existence — every other
    // event in this transcript belongs to the one golden run.
    run_id: i === 0 ? null : GOLDEN_RUN_ID,
    attempt_id: null,
    sequence: i + 1,
    event_type,
    created_at: BASE_CREATED_AT + i * 1000,
    payload: { synthetic: true, source: "q-deck-r05-golden-transcript" },
  }));
}

/** The transcript truncated to just before its terminal event — what a
 * client sees while the run is still active (pairs with `goldenRunActive`). */
export function goldenEventsActive(): EventDto[] {
  return goldenEvents("pass").slice(0, 3);
}

export function goldenEventPage(events: EventDto[]): EventPageDto {
  return {
    schema_version: 1,
    items: events,
    next_after: events.at(-1)?.sequence ?? null,
  };
}
