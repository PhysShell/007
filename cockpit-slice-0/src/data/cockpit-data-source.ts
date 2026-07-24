/*
 * ============================================================================
 *  THE SEAM · presentation layer ⇆ data source
 * ============================================================================
 *
 * The seam is split into TWO interfaces so the read path and the command path can
 * be implemented by DIFFERENT things at integration:
 *
 *   • `CockpitReadSource`  — conversation discovery, snapshot, and subscription.
 *                            After PR 4, a `LedgerCockpitAdapter` implements this
 *                            by folding canonical ledger events into UiFixtureEvents.
 *   • `CockpitCommandPort` — send / stop / permission / model-lock / reconnect.
 *                            In production these are daemon/o7d-owned actions that
 *                            travel over the real transport — NOT in-process calls.
 *
 * The two are deliberately separate because they graduate separately: reads become
 * a ledger projection; commands become authenticated RPCs into o7d. A fixture
 * implementation is free to back BOTH with one in-memory store (as
 * `FixtureCockpitDataSource` does here), but nothing in the presentation layer may
 * assume they are the same object.
 *
 * Both remain NARROW: the UI can observe and can request mock commands, but it
 * cannot start, own, or end a run through either interface.
 *
 * INVARIANT (architectural): the mock run/conversation state lives in the store,
 * which is a module-level singleton independent of any component. Subscribing and
 * unsubscribing (mount/unmount) NEVER mutate that state. Closing the client does
 * not stop a mock run. Covered by ./cockpit-data-source.test.ts.
 * ============================================================================
 */
import type {
  UiFixtureEvent,
  UiPermissionMode,
} from "../types/ui-fixture-events";
import { SCENARIOS, type UiFixtureScenario } from "../fixtures/catalog";

export type UiEventListener = (events: readonly UiFixtureEvent[]) => void;
export type UiConversationsListener = (ids: readonly string[]) => void;

/** READ side: conversation discovery + per-conversation snapshot/subscription. */
export interface CockpitReadSource {
  /** Current known conversation ids, in stable order. */
  listConversationIds(): readonly string[];
  /** Subscribe to the SET of conversations (dynamic discovery). Fires immediately
   *  with the current list, then again whenever the set changes. Returns an
   *  unsubscribe fn. */
  subscribeConversations(listener: UiConversationsListener): () => void;
  /** Everything delivered for a conversation so far (may be a partial history). */
  snapshot(conversationId: string): readonly UiFixtureEvent[];
  /** Subscribe to subsequently-delivered event batches for one conversation.
   *  Unsubscribing removes ONLY the listener; it never touches store state. */
  subscribe(conversationId: string, listener: UiEventListener): () => void;
}

/** COMMAND side: fixture-driven mock commands. None launches a real process; each
 *  appends mock events to the backing store. In production these are o7d-owned. */
export interface CockpitCommandPort {
  send(conversationId: string, text: string): void;
  stop(conversationId: string): void;
  setPermission(conversationId: string, mode: UiPermissionMode): void;
  setModelLock(conversationId: string, locked: boolean): void;
  reconnect(conversationId: string): void;
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
 * Deterministic, fixture-backed data source implementing BOTH seam interfaces
 * over one in-memory store. All mock state lives here, decoupled from React.
 */
export class FixtureCockpitDataSource
  implements CockpitReadSource, CockpitCommandPort
{
  private readonly cells = new Map<string, ConversationCell>();
  private readonly order: string[] = [];
  private readonly conversationListeners = new Set<UiConversationsListener>();

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

  // --- CockpitReadSource ---------------------------------------------------

  listConversationIds(): readonly string[] {
    return this.order.slice();
  }

  subscribeConversations(listener: UiConversationsListener): () => void {
    this.conversationListeners.add(listener);
    // Fire immediately with the current set (dynamic-discovery contract).
    listener(this.order.slice());
    return () => {
      this.conversationListeners.delete(listener);
    };
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

  // --- CockpitCommandPort --------------------------------------------------

  send(conversationId: string, text: string): void {
    const cell = this.cell(conversationId);
    const trimmed = text.trim();
    if (!trimmed) return;
    this.deliver(cell, [
      {
        kind: "userMessage",
        id: `${cell.scenario.id}:sent:${cell.nextUiSeq}`,
        conversationId: cell.scenario.id,
        uiSeq: cell.nextUiSeq++,
        text: trimmed,
      },
    ]);
  }

  stop(conversationId: string): void {
    const cell = this.cell(conversationId);
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
  }

  setPermission(conversationId: string, mode: UiPermissionMode): void {
    const cell = this.cell(conversationId);
    // Mock policy: requested applies immediately; `bypass` cannot take effect
    // unless sandbox attestation is enforced (unknown in this spike) — so it
    // resolves to `auto` as the effective mode, surfacing the requested≠effective
    // gap the real controls must show honestly.
    const effective = mode === "bypass" ? "auto" : mode;
    this.deliver(cell, [
      {
        kind: "controlState",
        id: `${cell.scenario.id}:perm:${cell.nextUiSeq}`,
        conversationId: cell.scenario.id,
        uiSeq: cell.nextUiSeq++,
        requestedPermission: mode,
        effectivePermission: effective,
      },
    ]);
  }

  setModelLock(conversationId: string, locked: boolean): void {
    const cell = this.cell(conversationId);
    this.deliver(cell, [
      {
        kind: "controlState",
        id: `${cell.scenario.id}:lock:${cell.nextUiSeq}`,
        conversationId: cell.scenario.id,
        uiSeq: cell.nextUiSeq++,
        modelLocked: locked,
      },
    ]);
  }

  reconnect(conversationId: string): void {
    const cell = this.cell(conversationId);
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
  ):
    | { runId: string; agent: "claude" | "codex" | "system"; role?: string; label: string }
    | undefined {
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
