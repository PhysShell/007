/*
 * ============================================================================
 *  UI-ONLY · PROVISIONAL · THE FOLD (pure reducer)
 * ============================================================================
 *
 * Folds a stream of UI-only fixture events into a UiConversationViewModel.
 *
 * Two properties this fold guarantees, and which the fixtures deliberately stress
 * with duplicate + out-of-order delivery:
 *   1. DEDUPLICATION  — an event whose `id` was already folded is ignored.
 *   2. STABLE ORDERING — events are ordered strictly by `uiSeq`, regardless of
 *      the order they were delivered in.
 *
 * These mirror (in UI-only terms) the ledger's idempotency-key + per-conversation
 * monotonic-sequence guarantees. They are re-implemented here ONLY so the spike is
 * self-contained; the real guarantees live in the daemon/ledger. When PR 4 lands,
 * the adapter feeds canonical events (already ordered/deduped by the ledger) into
 * `projectConversation` and this UI-side reconciliation becomes a thin fallback.
 *
 * `applyEvent`/`applyEvents` maintain the ordered, deduplicated log.
 * `projectConversation` is a pure projection of that log into the view model.
 * ============================================================================
 */
import type {
  UiFixtureEvent,
  UiAgentFamily,
  UiPermissionMode,
} from "../types/ui-fixture-events";
import type {
  UiConversationViewModel,
  UiConversationSummary,
  UiControlsState,
  UiConnectionState,
  UiRunGraph,
  UiRunNode,
  UiTimelineItem,
} from "../types/ui-view-model";

export interface UiFoldState {
  readonly seenIds: ReadonlySet<string>;
  /** Deduplicated, ascending-by-uiSeq log. */
  readonly orderedEvents: readonly UiFixtureEvent[];
}

export function emptyFoldState(): UiFoldState {
  return { seenIds: new Set(), orderedEvents: [] };
}

const PERMISSION_OPTIONS: readonly UiPermissionMode[] = [
  "plan",
  "ask",
  "acceptEdits",
  "auto",
  "bypass",
];

const AGENT_ORDER: readonly UiAgentFamily[] = ["claude", "codex", "system"];
const ACTIVE_RUN_STATUSES = new Set(["queued", "running", "waiting", "cancelling"]);

/**
 * Fold one event into the log. Duplicate `id` → returned unchanged (dedup).
 * Otherwise inserted at the position that keeps the log ascending by `uiSeq`
 * (stable for equal `uiSeq`: later delivery sorts after).
 */
export function applyEvent(
  state: UiFoldState,
  event: UiFixtureEvent
): UiFoldState {
  if (state.seenIds.has(event.id)) return state;

  const seenIds = new Set(state.seenIds);
  seenIds.add(event.id);

  const events = state.orderedEvents.slice();
  // Find the first index whose uiSeq is strictly greater — insert before it.
  let lo = 0;
  let hi = events.length;
  while (lo < hi) {
    const mid = (lo + hi) >> 1;
    if (events[mid].uiSeq <= event.uiSeq) lo = mid + 1;
    else hi = mid;
  }
  events.splice(lo, 0, event);

  return { seenIds, orderedEvents: events };
}

export function applyEvents(
  state: UiFoldState,
  incoming: readonly UiFixtureEvent[]
): UiFoldState {
  let next = state;
  for (const ev of incoming) next = applyEvent(next, ev);
  return next;
}

// --- projection helpers -----------------------------------------------------

interface MutableItem {
  item: UiTimelineItem;
  order: number;
}

function orderedAgents(present: Set<UiAgentFamily>): UiAgentFamily[] {
  return AGENT_ORDER.filter((a) => present.has(a));
}

/**
 * Pure projection of the deduplicated/ordered log into a view model.
 * Grouped items (streaming messages, tool calls, permission requests, gates) are
 * keyed by their stable group id and updated in place; the first appearance fixes
 * their timeline position (= uiSeq order).
 */
export function projectConversation(
  conversationId: string,
  state: UiFoldState
): UiConversationViewModel {
  const events = state.orderedEvents;

  let title = conversationId;
  let loadOverride: "loading" | "error" | undefined;
  let errorMessage: string | undefined;
  let unread = 0;

  const items = new Map<string, MutableItem>();
  const agents = new Set<UiAgentFamily>();
  const streamDone = new Map<string, boolean>();

  const runNodes = new Map<string, UiRunNode>();
  const runInsertOrder: string[] = [];

  let controls: UiControlsState = {
    permission: {
      requested: "ask",
      effective: "ask",
      options: PERMISSION_OPTIONS,
      mismatch: false,
    },
    model: { requested: "", effective: "", locked: false, mismatch: false },
  };

  let connection: UiConnectionState = {
    status: "connected",
    offline: false,
  };

  let order = 0;
  const put = (key: string, item: UiTimelineItem) => {
    const existing = items.get(key);
    if (existing) existing.item = item;
    else items.set(key, { item, order: order++ });
  };
  const prev = (key: string) => items.get(key)?.item;

  const ensureRun = (runId: string): UiRunNode => {
    let node = runNodes.get(runId);
    if (!node) {
      node = {
        runId,
        agent: "system",
        label: runId,
        status: "queued",
        children: [],
      };
      runNodes.set(runId, node);
      runInsertOrder.push(runId);
    }
    return node;
  };

  for (const ev of events) {
    switch (ev.kind) {
      case "conversationMeta": {
        title = ev.title;
        if (ev.loadState) loadOverride = ev.loadState;
        errorMessage = ev.errorMessage;
        if (typeof ev.unread === "number") unread = ev.unread;
        break;
      }
      case "userMessage": {
        put(ev.id, {
          key: ev.id,
          itemKind: "message",
          source: "user",
          role: "user",
          text: ev.text,
          streaming: false,
          uiSeq: ev.uiSeq,
          recovered: !!ev.recovered,
        });
        break;
      }
      case "agentMessage": {
        agents.add(ev.source);
        put(ev.id, {
          key: ev.id,
          itemKind: "message",
          source: ev.source,
          role: "agent",
          text: ev.text,
          streaming: false,
          runId: ev.runId,
          uiSeq: ev.uiSeq,
          recovered: !!ev.recovered,
        });
        break;
      }
      case "agentMessageDelta": {
        agents.add(ev.source);
        const key = `msg:${ev.messageId}`;
        const priorText =
          prev(key)?.itemKind === "message"
            ? (prev(key) as { text: string }).text
            : "";
        if (ev.done) streamDone.set(ev.messageId, true);
        put(key, {
          key,
          itemKind: "message",
          source: ev.source,
          role: "agent",
          text: priorText + ev.chunk,
          streaming: !streamDone.get(ev.messageId),
          runId: ev.runId,
          uiSeq: prev(key)?.uiSeq ?? ev.uiSeq,
          recovered: (prev(key)?.recovered ?? false) || !!ev.recovered,
        });
        break;
      }
      case "toolActivity": {
        agents.add(ev.source);
        const key = `tool:${ev.toolCallId}`;
        put(key, {
          key,
          itemKind: "tool",
          source: ev.source,
          tool: ev.tool,
          phase: ev.phase,
          title: ev.title,
          detail: ev.detail,
          runId: ev.runId,
          uiSeq: prev(key)?.uiSeq ?? ev.uiSeq,
          recovered: (prev(key)?.recovered ?? false) || !!ev.recovered,
        });
        break;
      }
      case "permissionRequest": {
        agents.add(ev.source);
        const key = `perm:${ev.requestId}`;
        put(key, {
          key,
          itemKind: "permission",
          source: ev.source,
          requestId: ev.requestId,
          tool: ev.tool,
          requestedMode: ev.requestedMode,
          status: ev.status,
          rationale: ev.rationale,
          runId: ev.runId,
          uiSeq: prev(key)?.uiSeq ?? ev.uiSeq,
          recovered: (prev(key)?.recovered ?? false) || !!ev.recovered,
        });
        break;
      }
      case "artifactCard": {
        agents.add(ev.source);
        const key = `art:${ev.artifactId}`;
        put(key, {
          key,
          itemKind: "artifact",
          source: ev.source,
          name: ev.name,
          artifactKind: ev.artifactKind,
          summary: ev.summary,
          runId: ev.runId,
          uiSeq: prev(key)?.uiSeq ?? ev.uiSeq,
          recovered: (prev(key)?.recovered ?? false) || !!ev.recovered,
        });
        break;
      }
      case "gateCard": {
        agents.add(ev.source);
        const key = `gate:${ev.gateId}`;
        put(key, {
          key,
          itemKind: "gate",
          source: ev.source,
          name: ev.name,
          status: ev.status,
          detail: ev.detail,
          runId: ev.runId,
          uiSeq: prev(key)?.uiSeq ?? ev.uiSeq,
          recovered: (prev(key)?.recovered ?? false) || !!ev.recovered,
        });
        break;
      }
      case "resultCard": {
        agents.add(ev.source);
        put(ev.id, {
          key: ev.id,
          itemKind: "result",
          source: ev.source,
          status: ev.status,
          verdict: ev.verdict,
          summary: ev.summary,
          runId: ev.runId,
          uiSeq: ev.uiSeq,
          recovered: !!ev.recovered,
        });
        break;
      }
      case "terminalFailure": {
        agents.add(ev.source);
        put(ev.id, {
          key: ev.id,
          itemKind: "failure",
          source: ev.source,
          reason: ev.reason,
          detail: ev.detail,
          runId: ev.runId,
          uiSeq: ev.uiSeq,
          recovered: !!ev.recovered,
        });
        break;
      }
      case "runStatus": {
        agents.add(ev.agent);
        const node = ensureRun(ev.runId);
        runNodes.set(ev.runId, {
          ...node,
          parentRunId: ev.parentRunId ?? node.parentRunId,
          agent: ev.agent,
          role: ev.role ?? node.role,
          label: ev.label,
          status: ev.status,
        });
        break;
      }
      case "delegation": {
        agents.add(ev.targetAgent);
        ensureRun(ev.parentRunId);
        const child = ensureRun(ev.childRunId);
        runNodes.set(ev.childRunId, {
          ...child,
          parentRunId: child.parentRunId ?? ev.parentRunId,
          agent: child.agent === "system" ? ev.targetAgent : child.agent,
          role: child.role ?? ev.targetRole,
        });
        break;
      }
      case "controlState": {
        const requested = ev.requestedPermission ?? controls.permission.requested;
        const effective = ev.effectivePermission ?? controls.permission.effective;
        const reqModel = ev.requestedModel ?? controls.model.requested;
        const effModel = ev.effectiveModel ?? controls.model.effective;
        const locked = ev.modelLocked ?? controls.model.locked;
        controls = {
          permission: {
            requested,
            effective,
            options: PERMISSION_OPTIONS,
            mismatch: requested !== effective,
          },
          model: {
            requested: reqModel,
            effective: effModel,
            locked,
            mismatch: reqModel !== "" && effModel !== "" && reqModel !== effModel,
          },
        };
        break;
      }
      case "connection": {
        const offline =
          ev.status === "disconnected" || ev.status === "reconnecting";
        connection = {
          status: ev.status,
          detail: ev.detail,
          replayCursor: ev.replayCursor,
          offline,
        };
        break;
      }
    }
  }

  // Build the run tree.
  const byId: Record<string, UiRunNode> = {};
  for (const id of runInsertOrder) {
    const n = runNodes.get(id)!;
    byId[id] = { ...n, children: [] };
  }
  const roots: UiRunNode[] = [];
  for (const id of runInsertOrder) {
    const node = byId[id];
    const parentId = node.parentRunId;
    if (parentId && byId[parentId]) byId[parentId].children.push(node);
    else roots.push(node);
  }
  const runGraph: UiRunGraph = { roots, byId };

  const timeline = Array.from(items.values())
    .sort((a, b) => a.order - b.order)
    .map((m) => m.item);

  const activity: "idle" | "active" = runInsertOrder.some((id) =>
    ACTIVE_RUN_STATUSES.has(byId[id].status)
  )
    ? "active"
    : "idle";

  const loadState = loadOverride
    ? loadOverride
    : timeline.length === 0
      ? "empty"
      : "ready";

  const offline = connection.offline;
  const canStop = activity === "active" && !offline;
  const canSend = !offline && loadState !== "loading" && loadState !== "error";

  const composer = {
    enabled: !offline,
    offline,
    canSend,
    canStop,
    placeholder: offline
      ? "Offline — reconnecting to the daemon…"
      : canStop
        ? "A run is active — send to steer, or stop it"
        : "Message the agent…",
  };

  const cursor = events.length ? events[events.length - 1].uiSeq : 0;

  return {
    id: conversationId,
    title,
    loadState,
    errorMessage,
    agents: orderedAgents(agents),
    unread,
    activity,
    timeline,
    runGraph,
    controls,
    connection,
    composer,
    cursor,
  };
}

/** Convenience: fold a full event list from scratch. */
export function foldConversation(
  conversationId: string,
  events: readonly UiFixtureEvent[]
): UiConversationViewModel {
  return projectConversation(conversationId, applyEvents(emptyFoldState(), events));
}

export function summarize(vm: UiConversationViewModel): UiConversationSummary {
  const lastMessage = [...vm.timeline]
    .reverse()
    .find((i) => i.itemKind === "message") as
    | { text: string }
    | undefined;
  const lastAny = vm.timeline[vm.timeline.length - 1];
  const lastPreview = lastMessage
    ? lastMessage.text
    : lastAny
      ? `[${lastAny.itemKind}]`
      : vm.loadState === "empty"
        ? "No messages yet"
        : "";
  return {
    id: vm.id,
    title: vm.title,
    loadState: vm.loadState,
    agents: vm.agents,
    unread: vm.unread,
    activity: vm.activity,
    lastPreview: lastPreview.length > 80 ? lastPreview.slice(0, 79) + "…" : lastPreview,
  };
}
