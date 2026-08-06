// Q-Deck R0.5 "live-readiness" proof, frontend half for RunPage: drives the
// real RunPage.svelte + real ConversationEventStream against the golden
// synthetic-run transcript's DTO shape (test-support/goldenTranscript.ts),
// proving RunPage's own responsibilities — lifecycle metadata updates,
// this-run-only timeline filtering/ordering, resilience to a transient
// refresh failure, and the read-only contract — without re-proving
// ConversationEventStream's own reconnect/dedup/schema-validation behavior,
// which is already covered directly in eventStream.svelte.test.ts.

import { describe, expect, it, vi, beforeEach, afterEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/svelte";
import RunPage from "./RunPage.svelte";
import * as api from "../lib/api";
import {
  goldenEventPage,
  goldenEvents,
  goldenEventsActive,
  goldenRun,
  goldenRunActive,
  goldenRunResumed,
} from "../test-support/goldenTranscript";

class MockEventSource {
  static instances: MockEventSource[] = [];
  onopen: (() => void) | null = null;
  onerror: (() => void) | null = null;
  onmessage: ((ev: MessageEvent<string>) => void) | null = null;
  closed = false;

  constructor() {
    MockEventSource.instances.push(this);
  }

  close(): void {
    this.closed = true;
  }
}

beforeEach(() => {
  vi.useFakeTimers({ shouldAdvanceTime: true });
  MockEventSource.instances = [];
  vi.stubGlobal("EventSource", MockEventSource);
});

afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe("RunPage", () => {
  it("shows an active run's lifecycle metadata, then updates to terminal once the poll sees completion", async () => {
    const active = goldenRunActive();
    const completed = goldenRun("pass");
    vi.spyOn(api, "getRun").mockResolvedValueOnce(active).mockResolvedValueOnce(completed);
    vi.spyOn(api, "getConversationEvents").mockResolvedValue(goldenEventPage(goldenEventsActive()));

    render(RunPage, { props: { runId: active.run_id } });
    await waitFor(() => expect(screen.getByText("running")).toBeInTheDocument());
    expect(screen.queryByText(/duration/)).not.toBeInTheDocument();

    // The poll interval (5s, matching Dashboard's) is what carries the page
    // from "running" to "completed" — RunPage never computes this itself.
    await vi.advanceTimersByTimeAsync(5100);
    await waitFor(() => expect(screen.getByText("completed")).toBeInTheDocument());
    expect(screen.getByText(/duration/)).toBeInTheDocument();
  });

  it("shows interrupted without a duration row — finished_at is honestly absent, not backfilled", async () => {
    // interrupted is a resumable pause, not a sealed/terminal outcome — see
    // goldenRun's own doc comment and docs/q-deck/r05-live-readiness.md.
    const interrupted = goldenRun("interrupted");
    vi.spyOn(api, "getRun").mockResolvedValue(interrupted);
    vi.spyOn(api, "getConversationEvents").mockResolvedValue(
      goldenEventPage(goldenEvents("interrupted")),
    );

    render(RunPage, { props: { runId: interrupted.run_id } });
    await waitFor(() => expect(screen.getByText("interrupted")).toBeInTheDocument());
    expect(screen.queryByText(/duration/)).not.toBeInTheDocument();
  });

  it("keeps polling through an interrupted run and picks up a subsequent resume back to running", async () => {
    // The regression this transcript must prove: RunPage must NOT stop
    // polling on `interrupted` (it is resumable, not sealed) — before the
    // fix, isActiveRun-based polling treated `interrupted` the same as a
    // sealed outcome and this page would never notice a later resume.
    const interrupted = goldenRun("interrupted");
    const resumed = goldenRunResumed();
    vi.spyOn(api, "getRun").mockResolvedValueOnce(interrupted).mockResolvedValueOnce(resumed);
    vi.spyOn(api, "getConversationEvents").mockResolvedValue(
      goldenEventPage(goldenEvents("interrupted")),
    );

    render(RunPage, { props: { runId: interrupted.run_id } });
    await waitFor(() => expect(screen.getByText("interrupted")).toBeInTheDocument());

    await vi.advanceTimersByTimeAsync(5100);
    await waitFor(() => expect(screen.getByText("running")).toBeInTheDocument());
  });

  it("shows blocked/error WITH a duration row — both are sealed (R0.6), unlike interrupted", async () => {
    for (const outcome of ["blocked", "error"] as const) {
      const run = goldenRun(outcome);
      vi.spyOn(api, "getRun").mockResolvedValue(run);
      vi.spyOn(api, "getConversationEvents").mockResolvedValue(goldenEventPage(goldenEvents(outcome)));

      const { unmount } = render(RunPage, { props: { runId: run.run_id } });
      await waitFor(() => expect(screen.getByText(outcome)).toBeInTheDocument());
      expect(screen.getByText(/duration/)).toBeInTheDocument();
      unmount();
      vi.restoreAllMocks();
    }
  });

  it("stops polling once a run reaches blocked or error — both are sealed, not resumable like interrupted", async () => {
    for (const outcome of ["blocked", "error"] as const) {
      const active = goldenRunActive();
      const sealed = goldenRun(outcome);
      // If polling incorrectly continued past a sealed run, a THIRD getRun
      // call (there isn't one queued) would be needed and this mock would
      // reject, failing the test loudly instead of silently passing.
      vi.spyOn(api, "getRun").mockResolvedValueOnce(active).mockResolvedValueOnce(sealed);
      vi.spyOn(api, "getConversationEvents").mockResolvedValue(goldenEventPage(goldenEventsActive()));

      const { unmount } = render(RunPage, { props: { runId: active.run_id } });
      await waitFor(() => expect(screen.getByText("running")).toBeInTheDocument());

      await vi.advanceTimersByTimeAsync(5100);
      await waitFor(() => expect(screen.getByText(outcome)).toBeInTheDocument());

      // One more interval tick: if polling didn't stop, getRun would be
      // called a third time against a mock with nothing left queued for it
      // (vitest's mockResolvedValueOnce chain falls through to undefined),
      // which would surface as a broken page instead of staying on outcome.
      await vi.advanceTimersByTimeAsync(5100);
      expect(screen.getByText(outcome)).toBeInTheDocument();

      unmount();
      vi.restoreAllMocks();
    }
  });

  it("renders this run's own timeline events in sequence order, from REST history alone (the reload path)", async () => {
    const completed = goldenRun("pass");
    const events = goldenEvents("pass");
    vi.spyOn(api, "getRun").mockResolvedValue(completed);
    // A freshly-mounted page (a reload, not a live update) restores entirely
    // from this REST history call — no live SSE frame is ever emitted in
    // this test.
    vi.spyOn(api, "getConversationEvents").mockResolvedValue(goldenEventPage(events));

    const { container } = render(RunPage, { props: { runId: completed.run_id } });
    await waitFor(() => expect(container.querySelectorAll(".seq")).toHaveLength(6));

    // `conversation.created` (sequence 1) has no run_id and belongs to no
    // run's own page — RunPage must filter it out, leaving sequences 2..7.
    const seqs = Array.from(container.querySelectorAll(".seq")).map((el) => el.textContent);
    expect(seqs).toEqual(["#2", "#3", "#4", "#5", "#6", "#7"]);
  });

  it("keeps showing the last-loaded run on a transient poll failure instead of blanking or erroring", async () => {
    const active = goldenRunActive();
    vi.spyOn(api, "getRun").mockResolvedValueOnce(active).mockRejectedValueOnce(new Error("blip"));
    vi.spyOn(api, "getConversationEvents").mockResolvedValue(goldenEventPage(goldenEventsActive()));

    render(RunPage, { props: { runId: active.run_id } });
    await waitFor(() => expect(screen.getByText("running")).toBeInTheDocument());

    await vi.advanceTimersByTimeAsync(5100);
    // Still showing the run — a transient refresh failure must not blank an
    // already-loaded page or replace it with an error state.
    expect(screen.getByText("running")).toBeInTheDocument();
    expect(screen.queryByText(/Run not found/)).not.toBeInTheDocument();
  });

  it("never renders a mutation control (start/stop/cancel/approve/reject) in R0.5", async () => {
    const completed = goldenRun("pass");
    vi.spyOn(api, "getRun").mockResolvedValue(completed);
    vi.spyOn(api, "getConversationEvents").mockResolvedValue(goldenEventPage(goldenEvents("pass")));

    render(RunPage, { props: { runId: completed.run_id } });
    await waitFor(() => expect(screen.getByText("completed")).toBeInTheDocument());
    // Anchored to the button's WHOLE accessible name: an unanchored /start/i
    // would false-positive on the timeline's own row buttons, whose text
    // includes the event_type "run.started" — a status label, not a mutation
    // control.
    for (const forbidden of [/^stop$/i, /^cancel$/i, /^approve$/i, /^reject$/i, /^start$/i, /^retry$/i]) {
      expect(screen.queryByRole("button", { name: forbidden })).not.toBeInTheDocument();
    }
  });

  // Q-Deck A0.5 (docs/q-deck/a0-candidate-state.md §9): a run fetched
  // before the candidate-state fields existed on the wire — `goldenRun`'s
  // own fixture never sets them — must still render cleanly. This is the
  // "existing RunDto consumers remain compatible" proof: nothing about
  // A0.5 requires every caller of `getRun` to already know about the three
  // new optional fields.
  it("renders the candidate-state section without error for a run predating A0.5's own fields", async () => {
    const completed = goldenRun("pass");
    vi.spyOn(api, "getRun").mockResolvedValue(completed);
    vi.spyOn(api, "getConversationEvents").mockResolvedValue(goldenEventPage(goldenEvents("pass")));

    render(RunPage, { props: { runId: completed.run_id } });
    await waitFor(() => expect(screen.getByText("completed")).toBeInTheDocument());
    expect(screen.getByText("Candidate state")).toBeInTheDocument();
    expect(screen.getByText("unavailable")).toBeInTheDocument();
    expect(screen.getByText("No candidate-state data for this run.")).toBeInTheDocument();
  });

  // Q-Deck A0.5: run lineage (parent_run_id) is RunPage's own metadata row,
  // deliberately separate from candidate-state provenance
  // (candidate_source_run_id), which CandidateStateCard.svelte.test.ts
  // covers on its own. This proves the two are independently displayed
  // even when they point at DIFFERENT runs — never conflated or derived
  // from one another.
  it("shows a parent-run link, independent of and possibly different from the candidate source run", async () => {
    const child = goldenRun("pass", {
      run_id: "run-child",
      parent_run_id: "run-parent",
      candidate_source_run_id: "run-candidate-source",
      candidate_tree_oid: "deadbeef",
      materialization_status: "materialized",
    });
    vi.spyOn(api, "getRun").mockResolvedValue(child);
    vi.spyOn(api, "getConversationEvents").mockResolvedValue(
      goldenEventPage(goldenEvents("pass").map((e) => ({ ...e, run_id: "run-child" }))),
    );

    render(RunPage, { props: { runId: "run-child" } });
    await waitFor(() => expect(screen.getByText("completed")).toBeInTheDocument());

    const parentLink = screen.getByRole("link", { name: "run-parent" });
    expect(parentLink).toHaveAttribute("href", "/runs/run-parent");
    const sourceLink = screen.getByRole("link", { name: "run-candidate-source" });
    expect(sourceLink).toHaveAttribute("href", "/runs/run-candidate-source");
    // Both are present, both distinct, neither hidden by the other.
    expect(parentLink).not.toBe(sourceLink);
  });

  it("hides the parent-run row entirely for a top-level run, without hiding the candidate-state section", async () => {
    const topLevel = goldenRun("pass", { parent_run_id: null });
    vi.spyOn(api, "getRun").mockResolvedValue(topLevel);
    vi.spyOn(api, "getConversationEvents").mockResolvedValue(goldenEventPage(goldenEvents("pass")));

    render(RunPage, { props: { runId: topLevel.run_id } });
    await waitFor(() => expect(screen.getByText("completed")).toBeInTheDocument());
    expect(screen.queryByText("parent run")).not.toBeInTheDocument();
    expect(screen.getByText("Candidate state")).toBeInTheDocument();
  });
});
