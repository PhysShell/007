import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/svelte";
import EventTimeline from "./EventTimeline.svelte";
import type { EventDto } from "../lib/types";

function eventDto(overrides: Partial<EventDto> = {}): EventDto {
  return {
    schema_version: 1,
    event_id: "evt-1",
    conversation_id: "conv-1",
    run_id: null,
    attempt_id: null,
    sequence: 1,
    event_type: "system.note",
    created_at: Date.now(),
    payload: { a: 1 },
    ...overrides,
  };
}

describe("EventTimeline", () => {
  it("renders an empty state with no events", () => {
    render(EventTimeline, { props: { events: [] } });
    expect(screen.getByText("No events yet.")).toBeInTheDocument();
  });

  it("renders an event type this build has never seen without crashing", () => {
    const events = [eventDto({ event_type: "some.future.event.kind.v7" })];
    render(EventTimeline, { props: { events } });
    expect(screen.getByText("some.future.event.kind.v7")).toBeInTheDocument();
  });

  it("renders a large payload inside a bounded, scrollable container rather than crashing", async () => {
    const bigPayload = { blob: "x".repeat(50_000) };
    const events = [eventDto({ payload: bigPayload })];
    render(EventTimeline, { props: { events } });
    const toggle = screen.getByRole("button");
    await toggle.click();
    const pre = document.querySelector(".payload");
    expect(pre).not.toBeNull();
    expect(pre?.textContent?.length).toBeGreaterThan(40_000);
  });

  it("keeps events in the order given (already-sorted upstream), never re-sorting by wall clock", () => {
    const events = [
      eventDto({ event_id: "a", sequence: 5, created_at: 1 }),
      eventDto({ event_id: "b", sequence: 6, created_at: 0 }), // "older" timestamp, later sequence
    ];
    render(EventTimeline, { props: { events } });
    const rows = screen.getAllByText(/^#\d+$/);
    expect(rows.map((r) => r.textContent)).toEqual(["#5", "#6"]);
  });
});
