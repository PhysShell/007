// Proves the client half of the reconnect contract: dedup by sequence, honest
// connection-state transitions, and history-then-live ordering — using a
// mock EventSource (jsdom doesn't implement one), not a real server.

import { describe, expect, it, vi, beforeEach, afterEach } from "vitest";
import { ConversationEventStream } from "./eventStream.svelte";
import type { EventDto } from "./types";

function eventDto(sequence: number, runId: string | null = null): EventDto {
  return {
    schema_version: 1,
    event_id: `evt-${sequence}`,
    conversation_id: "conv-1",
    run_id: runId,
    attempt_id: null,
    sequence,
    event_type: "system.note",
    created_at: 1_000 + sequence,
    payload: { n: sequence },
  };
}

class MockEventSource {
  static instances: MockEventSource[] = [];
  url: string;
  onopen: (() => void) | null = null;
  onerror: (() => void) | null = null;
  onmessage: ((ev: MessageEvent<string>) => void) | null = null;
  closed = false;

  constructor(url: string) {
    this.url = url;
    MockEventSource.instances.push(this);
  }

  emit(sequence: number, runId: string | null = null): void {
    this.onmessage?.(new MessageEvent("message", { data: JSON.stringify(eventDto(sequence, runId)) }));
  }

  open(): void {
    this.onopen?.();
  }

  error(): void {
    this.onerror?.();
  }

  close(): void {
    this.closed = true;
  }
}

beforeEach(() => {
  MockEventSource.instances = [];
  vi.stubGlobal("EventSource", MockEventSource);
  vi.spyOn(globalThis, "fetch").mockResolvedValue(
    new Response(
      JSON.stringify({ schema_version: 1, items: [], next_after: null }),
      { status: 200 },
    ),
  );
});

afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

async function flush(): Promise<void> {
  await Promise.resolve();
  await Promise.resolve();
}

describe("ConversationEventStream", () => {
  it("starts connecting, then reports open once EventSource opens", async () => {
    const stream = new ConversationEventStream("conv-1");
    expect(stream.status).toBe("connecting");
    await flush();
    const source = MockEventSource.instances[0];
    source.open();
    expect(stream.status).toBe("open");
    stream.close();
  });

  it("reports reconnecting (not connecting) after having been open once", async () => {
    const stream = new ConversationEventStream("conv-1");
    await flush();
    const source = MockEventSource.instances[0];
    source.open();
    source.error();
    expect(stream.status).toBe("reconnecting");
    stream.close();
    expect(stream.status).toBe("closed");
  });

  it("dedupes by sequence even if the same event arrives twice", async () => {
    const stream = new ConversationEventStream("conv-1");
    await flush();
    const source = MockEventSource.instances[0];
    source.emit(1);
    source.emit(2);
    source.emit(1); // duplicate — the server shouldn't do this, but be defensive
    expect(stream.events.map((e) => e.sequence)).toEqual([1, 2]);
    stream.close();
  });

  it("keeps events in sequence order regardless of arrival order", async () => {
    const stream = new ConversationEventStream("conv-1");
    await flush();
    const source = MockEventSource.instances[0];
    source.emit(3);
    source.emit(1);
    source.emit(2);
    expect(stream.events.map((e) => e.sequence)).toEqual([1, 2, 3]);
    stream.close();
  });

  it("seeds history via REST before the live stream starts", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValue(
      new Response(
        JSON.stringify({
          schema_version: 1,
          items: [eventDto(1), eventDto(2)],
          next_after: 2,
        }),
        { status: 200 },
      ),
    );
    const stream = new ConversationEventStream("conv-1");
    await flush();
    expect(stream.events.map((e) => e.sequence)).toEqual([1, 2]);
    const source = MockEventSource.instances[0];
    expect(source.url).toContain("after=2");
    source.emit(3);
    expect(stream.events.map((e) => e.sequence)).toEqual([1, 2, 3]);
    stream.close();
  });

  it("drops a malformed SSE frame instead of throwing", async () => {
    const stream = new ConversationEventStream("conv-1");
    await flush();
    const source = MockEventSource.instances[0];
    source.onmessage?.(new MessageEvent("message", { data: "not json" }));
    expect(stream.events).toEqual([]);
    stream.close();
  });
});
