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

  it("shows ERROR (interrupted) without a duration row — finished_at is honestly absent, not backfilled", async () => {
    const errored = goldenRun("error");
    vi.spyOn(api, "getRun").mockResolvedValue(errored);
    vi.spyOn(api, "getConversationEvents").mockResolvedValue(goldenEventPage(goldenEvents("error")));

    render(RunPage, { props: { runId: errored.run_id } });
    await waitFor(() => expect(screen.getByText("interrupted")).toBeInTheDocument());
    expect(screen.queryByText(/duration/)).not.toBeInTheDocument();
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
});
