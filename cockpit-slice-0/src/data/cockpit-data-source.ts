/*
 * ============================================================================
 *  THE SEAM · presentation layer ⇆ data source
 * ============================================================================
 *
 * `CockpitEventSource` is the ONLY thing the presentation layer knows about where
 * its data comes from. It deals exclusively in UI-only fixture events; the fold
 * (./fold.ts) turns those into the view model the components render.
 *
 * This is the integration seam. Today the only implementation is
 * `FixtureCockpitDataSource` (in-memory, deterministic, no I/O). After the
 * canonical event protocol is frozen (roadmap PR 4), a SECOND implementation —
 * a `LedgerCockpitAdapter` — will subscribe to canonical ledger events and MAP
 * each one into a `UiFixtureEvent` (or this event shape is replaced and the fold
 * updated). The presentation layer and the fold should not need to change: they
 * only ever see `UiFixtureEvent`s arriving through this interface.
 *
 * Deliberately NARROW: no run control, no worker handles, no transport. The UI
 * cannot start, own, or end a run through this interface — it can only observe,
 * and dispatch fixture-driven mock actions.
 *
 * INVARIANT (architectural): the mock run/conversation state lives in the store,
 * which is a module-level singleton independent of any component. Subscribing and
 * unsubscribing (mount/unmount) NEVER mutate that state. Closing the client does
 * not stop a mock run. See fold.test-style coverage in
 * ./cockpit-data-source.test.ts.
 * ============================================================================
 */
import type {
  UiFixtureEvent,
  UiPermissionMode,
} from "../types/ui-fixture-events";
import { SCENARIOS, type UiFixtureScenario } from "../fixtures/catalog";

/** Fixture-driven UI actions. None of these start/stop a real process; each just
 *  appends mock events to the in-memory store. Named UI-only + provisional. */
export type UiCockpitAction =
  | { readonly type: "composer.send"; readonly conversationId: string; readonly text: string }
  | { readonly type: "composer.stop"; readonly conversationId: string }
  | {
      readonly type: "controls.setPermission";
      readonly conversationId: string;
      readonly mode: UiPermissionMode;
    }
  | {
      readonly type: "controls.setModelLock";
      readonly conversationId: string;
      readonly locked: boolean;
    }
  | { readonly type: "connection.reconnect"; readonly conversationId: string };

export type UiEventListener = (events: readonly UiFixtureEvent[]) => void;

export interface CockpitEventSource {
  /** Stable, ordered list of conversation ids the source knows about. */
  listConversationIds(): readonly string[];
  /** Everything delivered for a conversation so far (may be a partial history). */
  snapshot(conversationId: string): readonly UiFixtureEvent[];
  /** Subscribe to subsequently-delivered event batches. Returns an unsubscribe fn.
   *  Unsubscribing removes ONLY the listener; it never touches store state. */
  subscribe(conversationId: string, listener: UiEventListener): () => void;
  /** Apply a fixture-driven mock action. No real process is launched. */
  dispatch(action: UiCockpitAction): void;
}

interface ConversationCell {
  readonly scenario: UiFixtureScenario;
  events: UiFixtureEvent[];
  seenIds: Set<string>;
  nextUiSeq: number;
  listeners: Set<UiEventListener>;
  replayed: boolean;
}

function orderedInsert(events: UiFixtureEvent[], ev: UiFixtureEvent): void {
  let lo = 0;
  let hi = events.length;
  while (lo < hi) {
    const mid = (lo + hi) >> 1;
    if (events[mid].uiSeq <= ev.uiSeq) lo = mid + 1;
    else hi = mid;
  }
  events.splice(lo, 0, ev);
}

/**
 * Deterministic, fixture-backed data source. Holds every scenario's delivered
 * event log in memory. All mock state lives here, decoupled from React.
 */
export class FixtureCockpitDataSource implements CockpitEventSource {
  private readonly cells = new Map<string, ConversationCell>();
  private readonly order: string[] = [];

  constructor(scenarios: readonly UiFixtureScenario[] = SCENARIOS) {
    for (const scenario of scenarios) {
      const events: UiFixtureEvent[] = [];
      const seenIds = new Set<string>();
      for (const ev of scenario.initialDelivery) {
        if (seenIds.has(ev.id)) continue;
        seenIds.add(ev.id);
        orderedInsert(events, ev);
      }
      this.cells.set(scenario.id, {
        scenario,
        events,
        seenIds,
        nextUiSeq: scenario.nextUiSeqStart,
        listeners: new Set(),
        replayed: false,
      });
      this.order.push(scenario.id);
    }
  }

  listConversationIds(): readonly string[] {
    return this.order.slice();
  }

  snapshot(conversationId: string): readonly UiFixtureEvent[] {
    return this.cell(conversationId).events.slice();
  }

  subscribe(conversationId: string, listener: UiEventListener): () => void {
    const cell = this.cell(conversationId);
    cell.listeners.add(listener);
    return () => {
      // Unsubscribe removes ONLY the listener — store/run state is untouched.
      cell.listeners.delete(listener);
    };
  }

  dispatch(action: UiCockpitAction): void {
    const cell = this.cell(action.conversationId);
    switch (action.type) {
      case "composer.send": {
        const text = action.text.trim();
        if (!text) return;
        this.deliver(cell, [
          {
            kind: "userMessage",
            id: `${cell.scenario.id}:sent:${cell.nextUiSeq}`,
            conversationId: cell.scenario.id,
            uiSeq: cell.nextUiSeq++,
            text,
          },
        ]);
        break;
      }
      case "composer.stop": {
        const activeRun = this.findActiveRun(cell);
        if (!activeRun) return;
        this.deliver(cell, [
          {
            kind: "runStatus",
            id: `${cell.scenario.id}:stop:${cell.nextUiSeq}`,
            conversationId: cell.scenario.id,
            uiSeq: cell.nextUiSeq++,
            runId: activeRun.runId,
            agent: activeRun.agent,
            role: activeRun.role,
            label: activeRun.label,
            status: "cancelling",
          },
        ]);
        break;
      }
      case "controls.setPermission": {
        // Mock policy: requested applies immediately; `bypass` cannot take effect
        // unless sandbox attestation is enforced (unknown in this spike) — so it
        // resolves to `auto` as the effective mode, surfacing the requested≠effective
        // gap the real controls must show honestly.
        const effective =
          action.mode === "bypass" ? "auto" : action.mode;
        this.deliver(cell, [
          {
            kind: "controlState",
            id: `${cell.scenario.id}:perm:${cell.nextUiSeq}`,
            conversationId: cell.scenario.id,
            uiSeq: cell.nextUiSeq++,
            requestedPermission: action.mode,
            effectivePermission: effective,
          },
        ]);
        break;
      }
      case "controls.setModelLock": {
        this.deliver(cell, [
          {
            kind: "controlState",
            id: `${cell.scenario.id}:lock:${cell.nextUiSeq}`,
            conversationId: cell.scenario.id,
            uiSeq: cell.nextUiSeq++,
            modelLocked: action.locked,
          },
        ]);
        break;
      }
      case "connection.reconnect": {
        const replay = cell.scenario.replay;
        if (!replay) {
          this.deliver(cell, [
            {
              kind: "connection",
              id: `${cell.scenario.id}:reconn:${cell.nextUiSeq}`,
              conversationId: cell.scenario.id,
              uiSeq: cell.nextUiSeq++,
              status: "connected",
            },
          ]);
          return;
        }
        // Deliver the scripted replay batch — deliberately containing duplicate and
        // out-of-order events plus the post-disconnect tail. The fold dedups/orders.
        this.deliver(cell, replay.batch);
        cell.replayed = true;
        break;
      }
    }
  }

  private deliver(cell: ConversationCell, batch: readonly UiFixtureEvent[]): void {
    for (const ev of batch) {
      if (cell.seenIds.has(ev.id)) continue;
      cell.seenIds.add(ev.id);
      orderedInsert(cell.events, ev);
      if (ev.uiSeq >= cell.nextUiSeq) cell.nextUiSeq = ev.uiSeq + 1;
    }
    // Notify listeners with the RAW batch (dups/out-of-order included) so the
    // client-side fold is what actually reconciles — exactly like a real replay.
    for (const listener of cell.listeners) listener(batch);
  }

  private findActiveRun(
    cell: ConversationCell
  ): { runId: string; agent: "claude" | "codex" | "system"; role?: string; label: string } | undefined {
    const latest = new Map<
      string,
      { agent: "claude" | "codex" | "system"; role?: string; label: string; status: string }
    >();
    for (const ev of cell.events) {
      if (ev.kind === "runStatus") {
        latest.set(ev.runId, {
          agent: ev.agent,
          role: ev.role,
          label: ev.label,
          status: ev.status,
        });
      }
    }
    for (const [runId, r] of latest) {
      if (["queued", "running", "waiting"].includes(r.status)) {
        return { runId, agent: r.agent, role: r.role, label: r.label };
      }
    }
    return undefined;
  }

  private cell(conversationId: string): ConversationCell {
    const cell = this.cells.get(conversationId);
    if (!cell) throw new Error(`unknown conversation: ${conversationId}`);
    return cell;
  }
}

/** Module-level singleton: the mock world outlives every component. */
export const fixtureDataSource = new FixtureCockpitDataSource();
