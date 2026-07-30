import { describe, expect, it, vi, afterEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/svelte";
import Dashboard from "./Dashboard.svelte";
import * as api from "../lib/api";
import type { PageDto, RunDto } from "../lib/types";
import { goldenRun, goldenRunActive } from "../test-support/goldenTranscript";

function runDto(overrides: Partial<RunDto> = {}): RunDto {
  return {
    schema_version: 1,
    run_id: "run-1",
    conversation_id: "conv-1",
    parent_run_id: null,
    agent: "claude",
    role: "implementer",
    status: "running",
    created_at: Date.now() - 60_000,
    finished_at: null,
    ...overrides,
  };
}

function page(items: RunDto[]): PageDto<RunDto> {
  return { schema_version: 1, items, next_before: null };
}

/** Dashboard now fetches "active" and "recent" as two independent listRuns
 * calls (a real server-side status filter, not a client-side split of one
 * page — see Dashboard.svelte). Route each call by whether it carries a
 * `status` filter, so a mock reflects that contract instead of returning the
 * same page to both. */
function mockActiveAndRecent(active: RunDto[], recent: RunDto[]) {
  return vi.spyOn(api, "listRuns").mockImplementation(async (params = {}) =>
    params.status ? page(active) : page(recent),
  );
}

afterEach(() => {
  vi.restoreAllMocks();
});

describe("Dashboard", () => {
  it("shows a loading state before the first fetch resolves", () => {
    vi.spyOn(api, "listRuns").mockReturnValue(new Promise(() => {})); // never resolves
    render(Dashboard);
    expect(screen.getByText("Loading runs…")).toBeInTheDocument();
  });

  it("shows an error state when o7d is unreachable and nothing has loaded yet", async () => {
    vi.spyOn(api, "listRuns").mockRejectedValue(new Error("network down"));
    render(Dashboard);
    await waitFor(() => expect(screen.getByText(/Couldn't reach o7d/)).toBeInTheDocument());
  });

  it("splits runs into Running and Recent, and shows an empty message for each when appropriate", async () => {
    mockActiveAndRecent(
      [],
      [runDto({ run_id: "r1", status: "completed", created_at: Date.now() - 5000 })],
    );
    render(Dashboard);
    await waitFor(() => expect(screen.getByText("Nothing running right now.")).toBeInTheDocument());
    expect(screen.getByText("claude · implementer")).toBeInTheDocument();
  });

  it("lists an active run under Running with its status and age", async () => {
    mockActiveAndRecent([runDto({ run_id: "r-active", status: "running" })], []);
    render(Dashboard);
    await waitFor(() => expect(screen.getByText("running")).toBeInTheDocument());
    expect(screen.queryByText("Nothing running right now.")).not.toBeInTheDocument();
  });

  it("pages through every active run instead of truncating at one page", async () => {
    // The status filter alone only bounds WHICH runs come back, not HOW MANY
    // — 201+ genuinely active runs still spans more than one page. Mock two
    // pages or `active` would silently truncate to whatever the first page
    // held, exactly the bug this test guards against.
    vi.spyOn(api, "listRuns").mockImplementation(async (params = {}) => {
      if (!params.status) return page([]); // the unfiltered "recent" call
      if (params.before === undefined) {
        return {
          schema_version: 1,
          items: [runDto({ run_id: "a1" }), runDto({ run_id: "a2" })],
          next_before: "cursor1",
        };
      }
      expect(params.before).toBe("cursor1");
      return page([runDto({ run_id: "a3" })]);
    });
    const { container } = render(Dashboard);
    await waitFor(() => expect(container.querySelectorAll(".run-card")).toHaveLength(3));
  });

  it("shows a stale/offline banner but keeps the last-known list on a later failed refresh", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    try {
      // Each refresh() makes 2 calls (active + recent) via Promise.all. Let
      // the first refresh's pair succeed and every call from the second
      // refresh onward reject, so the poll-interval tick is the one that
      // goes offline.
      let calls = 0;
      vi.spyOn(api, "listRuns").mockImplementation(async (params = {}) => {
        calls += 1;
        if (calls > 2) throw new Error("blip");
        return params.status ? page([runDto({ run_id: "r1" })]) : page([]);
      });
      render(Dashboard);
      await waitFor(() => expect(screen.getByText("claude · implementer")).toBeInTheDocument());

      // Advance past the poll interval to trigger the second (failing) fetch.
      await vi.advanceTimersByTimeAsync(5100);

      expect(screen.getByText("Offline — showing last known state")).toBeInTheDocument();
      // The stale row must still be visible — going blank would be worse
      // than showing slightly-old data with an honest offline banner.
      expect(screen.getByText("claude · implementer")).toBeInTheDocument();
    } finally {
      vi.useRealTimers();
    }
  });

  it("shows the golden synthetic-run transcript's active run under Running, tied to the canonical R0.5 contract", async () => {
    // Q-Deck R0.5 live-readiness proof: the same DTO shape
    // `crates/o7-ledger`'s and `crates/o7d`'s golden-transcript tests
    // produce via the real production write API and REST/SSE surface, not
    // ad hoc test data — see docs/q-deck/r05-live-readiness.md.
    mockActiveAndRecent([goldenRunActive()], []);
    render(Dashboard);
    await waitFor(() => expect(screen.getByText("running")).toBeInTheDocument());
    expect(screen.getByText("claude · implementer")).toBeInTheDocument();
    expect(screen.queryByText("Nothing running right now.")).not.toBeInTheDocument();
  });

  it("shows the golden synthetic-run transcript's completed/failed/interrupted/blocked/error runs under Recent (not currently active, regardless of whether each is sealed)", async () => {
    mockActiveAndRecent(
      [],
      [
        goldenRun("pass", { run_id: "r05-golden-run-pass" }),
        goldenRun("fail", { run_id: "r05-golden-run-fail" }),
        goldenRun("interrupted", { run_id: "r05-golden-run-interrupted" }),
        goldenRun("blocked", { run_id: "r06-golden-run-blocked" }),
        goldenRun("error", { run_id: "r06-golden-run-error" }),
      ],
    );
    render(Dashboard);
    await waitFor(() => expect(screen.getAllByText("claude · implementer")).toHaveLength(5));
    expect(screen.getByText("completed")).toBeInTheDocument();
    expect(screen.getByText("failed")).toBeInTheDocument();
    expect(screen.getByText("interrupted")).toBeInTheDocument();
    expect(screen.getByText("blocked")).toBeInTheDocument();
    expect(screen.getByText("error")).toBeInTheDocument();
  });

  it("never renders a mutation control (start/stop/cancel/approve/reject) in R0", async () => {
    mockActiveAndRecent(
      [runDto({ status: "running" })],
      [runDto({ run_id: "r2", status: "failed" })],
    );
    render(Dashboard);
    await waitFor(() => expect(screen.getByText("running")).toBeInTheDocument());
    for (const forbidden of [/stop/i, /cancel/i, /approve/i, /reject/i, /start/i]) {
      expect(screen.queryByRole("button", { name: forbidden })).not.toBeInTheDocument();
    }
  });
});
