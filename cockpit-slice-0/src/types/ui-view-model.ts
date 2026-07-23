/*
 * ============================================================================
 *  UI-ONLY · PROVISIONAL · VIEW MODEL (fold output)
 * ============================================================================
 *
 * These are the shapes the presentation layer renders. They are produced by the
 * pure fold in ../data/fold.ts from UI-only fixture events. They are NOT a wire
 * contract. When the canonical protocol lands (roadmap PR 4), the adapter behind
 * the CockpitEventSource seam re-produces THESE shapes from canonical ledger
 * events; the presentation layer should not need to change. If a field here has
 * no canonical source yet, it is provisional and flagged in the docs.
 * ============================================================================
 */
import type {
  UiAgentFamily,
  UiConnectionStatus,
  UiPermissionMode,
  UiRunStatus,
} from "./ui-fixture-events";

export type UiConversationLoadState = "loading" | "error" | "empty" | "ready";

export type UiTimelineItemKind =
  | "message"
  | "tool"
  | "permission"
  | "artifact"
  | "gate"
  | "result"
  | "failure";

export interface UiTimelineItemBase {
  readonly key: string;
  readonly itemKind: UiTimelineItemKind;
  readonly source: UiAgentFamily | "user";
  readonly runId?: string;
  readonly uiSeq: number;
  /** True when replayed from history after a reconnect/recovery. */
  readonly recovered: boolean;
}

export interface UiMessageItem extends UiTimelineItemBase {
  readonly itemKind: "message";
  readonly role: "user" | "agent";
  readonly text: string;
  readonly streaming: boolean;
}

export interface UiToolItem extends UiTimelineItemBase {
  readonly itemKind: "tool";
  readonly tool: string;
  readonly phase: "requested" | "started" | "completed" | "failed";
  readonly title: string;
  readonly detail?: string;
}

export interface UiPermissionItem extends UiTimelineItemBase {
  readonly itemKind: "permission";
  readonly requestId: string;
  readonly tool: string;
  readonly requestedMode: UiPermissionMode;
  readonly status: "pending" | "allowed" | "denied";
  readonly rationale?: string;
}

export interface UiArtifactItem extends UiTimelineItemBase {
  readonly itemKind: "artifact";
  readonly name: string;
  readonly artifactKind: string;
  readonly summary: string;
}

export interface UiGateItem extends UiTimelineItemBase {
  readonly itemKind: "gate";
  readonly name: string;
  readonly status: "running" | "passed" | "failed";
  readonly detail?: string;
}

export interface UiResultItem extends UiTimelineItemBase {
  readonly itemKind: "result";
  readonly status: "accepted" | "rejected";
  readonly verdict: string;
  readonly summary: string;
}

export interface UiFailureItem extends UiTimelineItemBase {
  readonly itemKind: "failure";
  readonly reason: string;
  readonly detail?: string;
}

export type UiTimelineItem =
  | UiMessageItem
  | UiToolItem
  | UiPermissionItem
  | UiArtifactItem
  | UiGateItem
  | UiResultItem
  | UiFailureItem;

export interface UiRunNode {
  readonly runId: string;
  readonly parentRunId?: string;
  readonly agent: UiAgentFamily;
  readonly role?: string;
  readonly label: string;
  readonly status: UiRunStatus;
  readonly children: UiRunNode[];
}

export interface UiRunGraph {
  readonly roots: UiRunNode[];
  /** Flat index for "jump to run" from a timeline item. */
  readonly byId: Record<string, UiRunNode>;
}

export interface UiControlsState {
  readonly permission: {
    readonly requested: UiPermissionMode;
    readonly effective: UiPermissionMode;
    readonly options: readonly UiPermissionMode[];
    /** requested ≠ effective — the UI must never show a lit button that lies. */
    readonly mismatch: boolean;
  };
  readonly model: {
    readonly requested: string;
    readonly effective: string;
    readonly locked: boolean;
    /** requested ≠ effective — a model-drift condition, surfaced not hidden. */
    readonly mismatch: boolean;
  };
}

export interface UiConnectionState {
  readonly status: UiConnectionStatus;
  readonly detail?: string;
  readonly replayCursor?: number;
  readonly offline: boolean;
}

export interface UiComposerState {
  readonly enabled: boolean;
  readonly offline: boolean;
  readonly canSend: boolean;
  readonly canStop: boolean;
  readonly placeholder: string;
}

export interface UiConversationViewModel {
  readonly id: string;
  readonly title: string;
  readonly loadState: UiConversationLoadState;
  readonly errorMessage?: string;
  readonly agents: readonly UiAgentFamily[];
  readonly unread: number;
  readonly activity: "idle" | "active";
  readonly timeline: readonly UiTimelineItem[];
  readonly runGraph: UiRunGraph;
  readonly controls: UiControlsState;
  readonly connection: UiConnectionState;
  readonly composer: UiComposerState;
  /** UI fixture-sequence cursor of the last folded event (replay resume point). */
  readonly cursor: number;
}

/** Lightweight per-conversation summary for the navigation list. */
export interface UiConversationSummary {
  readonly id: string;
  readonly title: string;
  readonly loadState: UiConversationLoadState;
  readonly agents: readonly UiAgentFamily[];
  readonly unread: number;
  readonly activity: "idle" | "active";
  readonly lastPreview: string;
}
