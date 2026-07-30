// Q-Deck R0.5 "live-readiness" proof, frontend half for ConversationPage:
// drives the real component against the golden synthetic-run transcript's
// DTO shape (test-support/goldenTranscript.ts) plus a dedicated pagination
// check, proving ConversationPage's own responsibilities — full run-history
// paging (never previously under direct test), the WHOLE conversation's
// timeline in order (unfiltered by run, unlike RunPage), and the read-only
// contract. ConversationEventStream's own reconnect/dedup/schema-validation
// behavior is already covered directly in eventStream.svelte.test.ts.

import { describe, expect, it, vi, beforeEach, afterEach } from "vitest";
import { render, waitFor } from "@testing-library/svelte";
import ConversationPage from "./ConversationPage.svelte";
import * as api from "../lib/api";
import type { PageDto, RunDto } from "../lib/types";
import { goldenEventPage, goldenEvents, GOLDEN_CONVERSATION_ID } from "../test-support/goldenTranscript";

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

function runDto(overrides: Partial<RunDto> = {}): RunDto {
  return {
    schema_version: 1,
    run_id: "run-1",
    conversation_id: GOLDEN_CONVERSATION_ID,
    parent_run_id: null,
    agent: "claude",
    role: "implementer",
    status: "completed",
    created_at: 1_700_000_000_000,
    finished_at: 1_700_000_005_000,
    ...overrides,
  };
}

function page(items: RunDto[], nextBefore: string | null = null): PageDto<RunDto> {
  return { schema_version: 1, items, next_before: nextBefore };
}

beforeEach(() => {
  MockEventSource.instances = [];
  vi.stubGlobal("EventSource", MockEventSource);
  vi.spyOn(api, "getConversationEvents").mockResolvedValue(goldenEventPage([]));
});

afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe("ConversationPage", () => {
  it("pages through every run in the conversation instead of truncating at one page", async () => {
    vi.spyOn(api, "listRuns").mockImplementation(async (params = {}) => {
      if (params.before === undefined) {
        return page([runDto({ run_id: "r1" }), runDto({ run_id: "r2" })], "cursor1");
      }
      expect(params.before).toBe("cursor1");
      return page([runDto({ run_id: "r3" })]);
    });

    const { container } = render(ConversationPage, {
      props: { conversationId: GOLDEN_CONVERSATION_ID },
    });
    await waitFor(() => expect(container.querySelectorAll(".run-row")).toHaveLength(3));
  });

  it("shows the WHOLE conversation's timeline in order, unfiltered by run (unlike RunPage)", async () => {
    vi.spyOn(api, "listRuns").mockResolvedValue(page([]));
    vi.spyOn(api, "getConversationEvents").mockResolvedValue(goldenEventPage(goldenEvents("pass")));

    const { container } = render(ConversationPage, {
      props: { conversationId: GOLDEN_CONVERSATION_ID },
    });
    // All 7, INCLUDING `conversation.created` (sequence 1, run_id null) —
    // ConversationPage has no run filter, unlike RunPage.
    await waitFor(() => expect(container.querySelectorAll(".seq")).toHaveLength(7));
    const seqs = Array.from(container.querySelectorAll(".seq")).map((el) => el.textContent);
    expect(seqs).toEqual(["#1", "#2", "#3", "#4", "#5", "#6", "#7"]);
  });

  it("never renders a mutation control (start/stop/cancel/approve/reject) in R0.5", async () => {
    vi.spyOn(api, "listRuns").mockResolvedValue(page([runDto()]));

    const { getByText, queryByRole } = render(ConversationPage, {
      props: { conversationId: GOLDEN_CONVERSATION_ID },
    });
    await waitFor(() => expect(getByText("claude · implementer")).toBeInTheDocument());
    // Anchored to the button's WHOLE accessible name: an unanchored /start/i
    // would false-positive on the timeline's own row buttons, whose text
    // includes the event_type "run.started" — a status label, not a mutation
    // control.
    for (const forbidden of [/^stop$/i, /^cancel$/i, /^approve$/i, /^reject$/i, /^start$/i, /^retry$/i]) {
      expect(queryByRole("button", { name: forbidden })).not.toBeInTheDocument();
    }
  });
});
