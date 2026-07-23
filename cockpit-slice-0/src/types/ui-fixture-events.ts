/*
 * ============================================================================
 *  UI-ONLY · PROVISIONAL · NOT A CANONICAL PROTOCOL
 * ============================================================================
 *
 * Every type in this file is a PRESENTATION-LAYER fixture shape invented for the
 * Cockpit Slice 0 spike. These are NOT wire events, NOT ledger events, and NOT
 * an authoritative protocol. The canonical event protocol (names, tags, payload
 * schemas, ordering guarantees) is frozen elsewhere in roadmap PR 4 and does not
 * exist yet.
 *
 * The discriminants below (`userMessage`, `toolActivity`, ...) are UI *rendering
 * intents*, deliberately NOT the roadmap's provisional wire names
 * (`agent.message.delta`, `tool.requested`, ...). See docs/ui/cockpit-slice-0.md
 * for the mapping and the list of what is unknown until PR 4.
 *
 * At integration time, a single adapter (the seam in ./cockpit-data-source.ts)
 * folds canonical ledger events into this same shape — OR this shape is deleted
 * and replaced. Nothing downstream of the fold should assume these names survive.
 * ============================================================================
 */

/** Marker so every consumer/import is self-documenting that this is throwaway. */
export const UI_ONLY_PROVISIONAL = true as const;

/** UI-only label for who authored a timeline item. Provisional — the real agent
 *  identity/attribution comes from PR 4 wire events, not from the UI. */
export type UiAgentFamily = "claude" | "codex" | "system";

/** UI-only permission modes shown by the mock selector. Provisional. */
export type UiPermissionMode =
  | "plan"
  | "ask"
  | "acceptEdits"
  | "auto"
  | "bypass";

/** UI-only run lifecycle presentation states. Provisional — NOT the authoritative
 *  o7d state machine, which is defined by the daemon, not the UI. */
export type UiRunStatus =
  | "queued"
  | "running"
  | "waiting"
  | "cancelling"
  | "completed"
  | "failed"
  | "interrupted";

/** UI-only connection presentation states for the mock transport indicator. */
export type UiConnectionStatus =
  | "connected"
  | "disconnected"
  | "reconnecting"
  | "replaying";

/**
 * Fields shared by every fixture event.
 *
 * - `id` — stable idempotency-ish key used by the fold to DEDUPLICATE. Duplicate
 *   deliveries carry the same `id`. (UI-only analogue of a ledger idempotency key.)
 * - `uiSeq` — per-conversation monotonic ordering key. The fold orders strictly by
 *   this, so out-of-order delivery still renders deterministically. (UI-only
 *   analogue of the ledger's per-conversation monotonic sequence — NOT the same
 *   number space.)
 */
export interface UiFixtureEventBase {
  readonly id: string;
  readonly conversationId: string;
  readonly uiSeq: number;
  /** Marks an item that was replayed from history after a reconnect/recovery. */
  readonly recovered?: boolean;
}

/** Conversation bootstrap / metadata (UI-only; not a wire event). */
export interface UiConversationMetaEvent extends UiFixtureEventBase {
  readonly kind: "conversationMeta";
  readonly title: string;
  /** Forces a load state; omit for the normal ready/empty derivation. */
  readonly loadState?: "loading" | "error";
  readonly errorMessage?: string;
  readonly unread?: number;
}

export interface UiUserMessageEvent extends UiFixtureEventBase {
  readonly kind: "userMessage";
  readonly text: string;
}

export interface UiAgentMessageEvent extends UiFixtureEventBase {
  readonly kind: "agentMessage";
  readonly source: UiAgentFamily;
  readonly runId?: string;
  readonly text: string;
}

/** One chunk of a streaming agent message. The fold concatenates chunks that
 *  share `messageId`, in `uiSeq` order; `done: true` ends the stream. */
export interface UiAgentMessageDeltaEvent extends UiFixtureEventBase {
  readonly kind: "agentMessageDelta";
  readonly source: UiAgentFamily;
  readonly runId?: string;
  readonly messageId: string;
  readonly chunk: string;
  readonly done?: boolean;
}

export interface UiToolActivityEvent extends UiFixtureEventBase {
  readonly kind: "toolActivity";
  readonly source: UiAgentFamily;
  readonly runId?: string;
  readonly toolCallId: string;
  readonly tool: string;
  readonly phase: "requested" | "started" | "completed" | "failed";
  readonly title: string;
  readonly detail?: string;
}

export interface UiPermissionRequestEvent extends UiFixtureEventBase {
  readonly kind: "permissionRequest";
  readonly source: UiAgentFamily;
  readonly runId?: string;
  readonly requestId: string;
  readonly tool: string;
  readonly requestedMode: UiPermissionMode;
  readonly status: "pending" | "allowed" | "denied";
  readonly rationale?: string;
}

export interface UiArtifactCardEvent extends UiFixtureEventBase {
  readonly kind: "artifactCard";
  readonly source: UiAgentFamily;
  readonly runId?: string;
  readonly artifactId: string;
  readonly name: string;
  readonly artifactKind: string;
  readonly summary: string;
}

export interface UiGateCardEvent extends UiFixtureEventBase {
  readonly kind: "gateCard";
  readonly source: UiAgentFamily;
  readonly runId?: string;
  readonly gateId: string;
  readonly name: string;
  readonly status: "running" | "passed" | "failed";
  readonly detail?: string;
}

export interface UiResultCardEvent extends UiFixtureEventBase {
  readonly kind: "resultCard";
  readonly source: UiAgentFamily;
  readonly runId?: string;
  readonly status: "accepted" | "rejected";
  readonly verdict: string;
  readonly summary: string;
}

export interface UiTerminalFailureEvent extends UiFixtureEventBase {
  readonly kind: "terminalFailure";
  readonly source: UiAgentFamily;
  readonly runId?: string;
  readonly reason: string;
  readonly detail?: string;
}

/** Establishes/updates a node in the run graph. */
export interface UiRunStatusEvent extends UiFixtureEventBase {
  readonly kind: "runStatus";
  readonly runId: string;
  readonly parentRunId?: string;
  readonly agent: UiAgentFamily;
  readonly role?: string;
  readonly label: string;
  readonly status: UiRunStatus;
}

/** Declares a parent→child delegation edge (redundant-safe with runStatus). */
export interface UiDelegationEvent extends UiFixtureEventBase {
  readonly kind: "delegation";
  readonly parentRunId: string;
  readonly childRunId: string;
  readonly targetAgent: UiAgentFamily;
  readonly targetRole: string;
}

/** Updates the controls panel (permission + model display). Partial merge. */
export interface UiControlStateEvent extends UiFixtureEventBase {
  readonly kind: "controlState";
  readonly requestedPermission?: UiPermissionMode;
  readonly effectivePermission?: UiPermissionMode;
  readonly requestedModel?: string;
  readonly effectiveModel?: string;
  readonly modelLocked?: boolean;
}

/** Updates the mock transport/reconnect/replay indicator. */
export interface UiConnectionEvent extends UiFixtureEventBase {
  readonly kind: "connection";
  readonly status: UiConnectionStatus;
  readonly detail?: string;
  /** UI fixture-sequence cursor the mock is replaying from, when replaying. */
  readonly replayCursor?: number;
}

/** The discriminated union of every UI-only fixture event. */
export type UiFixtureEvent =
  | UiConversationMetaEvent
  | UiUserMessageEvent
  | UiAgentMessageEvent
  | UiAgentMessageDeltaEvent
  | UiToolActivityEvent
  | UiPermissionRequestEvent
  | UiArtifactCardEvent
  | UiGateCardEvent
  | UiResultCardEvent
  | UiTerminalFailureEvent
  | UiRunStatusEvent
  | UiDelegationEvent
  | UiControlStateEvent
  | UiConnectionEvent;

export type UiFixtureEventKind = UiFixtureEvent["kind"];
