import { describe, expect, it, vi, afterEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/svelte";
import Dashboard from "./Dashboard.svelte";
import * as api from "../lib/api";
import type { PageDto, RunDto } from "../lib/types";

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
    vi.spyOn(api, "listRuns").mockResolvedValue(
      page([runDto({ run_id: "r1", status: "completed", created_at: Date.now() - 5000 })]),
    );
    render(Dashboard);
    await waitFor(() => expect(screen.getByText("Nothing running right now.")).toBeInTheDocument());
    expect(screen.getByText("claude · implementer")).toBeInTheDocument();
  });

  it("lists an active run under Running with its status and age", async () => {
    vi.spyOn(api, "listRuns").mockResolvedValue(
      page([runDto({ run_id: "r-active", status: "running" })]),
    );
    render(Dashboard);
    await waitFor(() => expect(screen.getByText("running")).toBeInTheDocument());
    expect(screen.queryByText("Nothing running right now.")).not.toBeInTheDocument();
  });

  it("shows a stale/offline banner but keeps the last-known list on a later failed refresh", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    try {
      vi.spyOn(api, "listRuns")
        .mockResolvedValueOnce(page([runDto({ run_id: "r1" })]))
        .mockRejectedValueOnce(new Error("blip"));
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

  it("never renders a mutation control (start/stop/cancel/approve/reject) in R0", async () => {
    vi.spyOn(api, "listRuns").mockResolvedValue(
      page([runDto({ status: "running" }), runDto({ run_id: "r2", status: "failed" })]),
    );
    render(Dashboard);
    await waitFor(() => expect(screen.getByText("running")).toBeInTheDocument());
    for (const forbidden of [/stop/i, /cancel/i, /approve/i, /reject/i, /start/i]) {
      expect(screen.queryByRole("button", { name: forbidden })).not.toBeInTheDocument();
    }
  });
});
