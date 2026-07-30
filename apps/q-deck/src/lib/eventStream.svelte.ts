// Live conversation event tail. Native `EventSource` already implements the
// reconnect-with-Last-Event-ID contract o7d's stream honors (the browser
// remembers the last received `id:` and resends it automatically on
// reconnect) — this module's job is just: seed the initial cursor from
// history, dedupe defensively by sequence, and expose connection state a
// mobile UI can render honestly (never silently show stale data as fresh).

import { conversationEventsStreamUrl, getConversationEvents } from "./api";
import type { EventDto } from "./types";

export type StreamStatus = "connecting" | "open" | "reconnecting" | "closed";

export class ConversationEventStream {
  events: EventDto[] = $state([]);
  status: StreamStatus = $state("connecting");

  #conversationId: string;
  #seen = new Set<number>();
  #source: EventSource | null = null;
  #hasOpenedOnce = false;

  constructor(conversationId: string) {
    this.#conversationId = conversationId;
    void this.#start();
  }

  async #start(): Promise<void> {
    // Seed history via REST first: EventSource has no way to set a custom
    // Last-Event-ID on its very FIRST connection (only the browser's own
    // automatic reconnects do that), so the initial cursor for a fresh
    // stream has to travel as `?after=` instead.
    let after: number | undefined;
    try {
      const page = await getConversationEvents(this.#conversationId, undefined, 500);
      for (const e of page.items) {
        this.#push(e);
      }
      after = page.items.at(-1)?.sequence;
    } catch {
      // History fetch failing doesn't block the live stream — it'll replay
      // from the beginning instead (o7d replays all events with no `after`).
    }

    const suffix = after !== undefined ? `?after=${after}` : "";
    const source = new EventSource(conversationEventsStreamUrl(this.#conversationId) + suffix);
    this.#source = source;

    source.onopen = () => {
      this.status = "open";
      this.#hasOpenedOnce = true;
    };
    source.onerror = () => {
      // EventSource retries on its own; distinguish "never connected" from
      // "was open, now retrying" so the UI can say the right thing.
      this.status = this.#hasOpenedOnce ? "reconnecting" : "connecting";
    };
    source.onmessage = (ev: MessageEvent<string>) => {
      let parsed: EventDto;
      try {
        parsed = JSON.parse(ev.data) as EventDto;
      } catch {
        return; // a malformed frame is dropped, not crashed on
      }
      this.#push(parsed);
    };
  }

  #push(event: EventDto): void {
    if (this.#seen.has(event.sequence)) return; // defensive: server already guarantees no dup
    this.#seen.add(event.sequence);
    // Insert in sequence order rather than assuming arrival order — history
    // and live frames both flow through here, and history could in principle
    // race a fast-arriving live event during the seed fetch.
    const idx = this.events.findIndex((e) => e.sequence > event.sequence);
    if (idx === -1) {
      this.events.push(event);
    } else {
      this.events.splice(idx, 0, event);
    }
  }

  close(): void {
    this.#source?.close();
    this.#source = null;
    this.status = "closed";
  }
}
