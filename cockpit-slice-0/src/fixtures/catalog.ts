/*
 * ============================================================================
 *  UI-ONLY · PROVISIONAL · DETERMINISTIC FIXTURE CATALOG
 * ============================================================================
 *
 * Hand-authored, fully deterministic scenarios that drive the spike. There is NO
 * `Date.now()`, NO `Math.random()`, NO clock, NO I/O anywhere in this file — the
 * same catalog folds to the same view model every time, so tests and screenshots
 * are reproducible.
 *
 * Every event here is a UI-only fixture event (see ../types/ui-fixture-events).
 * None of these strings are canonical protocol names, model ids, or wire payloads;
 * they are placeholders chosen to exercise each presentation state. The real
 * events/identities arrive from the daemon/ledger after roadmap PR 4.
 *
 * `uiSeq` is the per-conversation ordering key. The REPLAY scenario deliberately
 * delivers duplicate and out-of-order events to prove the fold reconciles them.
 * ============================================================================
 */
import type {
  UiAgentFamily,
  UiFixtureEvent,
  UiPermissionMode,
  UiRunStatus,
} from "../types/ui-fixture-events";

export interface UiFixtureScenario {
  /** conversationId */
  readonly id: string;
  /** short human name of the scenario */
  readonly name: string;
  readonly title: string;
  /** what presentation states this scenario demonstrates */
  readonly demonstrates: string;
  /** events delivered when the conversation first loads (may be partial history) */
  readonly initialDelivery: readonly UiFixtureEvent[];
  /** optional scripted reconnect replay (duplicate + out-of-order + tail) */
  readonly replay?: { readonly batch: readonly UiFixtureEvent[] };
  /** uiSeq where dispatch-appended (mock action) events begin */
  readonly nextUiSeqStart: number;
}

// --- tiny typed factories (keep scenarios readable; all ids stable) ---------

const id = (conv: string, seq: number) => `${conv}#${seq}`;

const meta = (
  conv: string,
  seq: number,
  title: string,
  opts: { loadState?: "loading" | "error"; errorMessage?: string; unread?: number } = {}
): UiFixtureEvent => ({
  kind: "conversationMeta",
  id: id(conv, seq),
  conversationId: conv,
  uiSeq: seq,
  title,
  ...opts,
});

const conn = (
  conv: string,
  seq: number,
  status: "connected" | "disconnected" | "reconnecting" | "replaying",
  detail?: string,
  replayCursor?: number
): UiFixtureEvent => ({
  kind: "connection",
  id: id(conv, seq),
  conversationId: conv,
  uiSeq: seq,
  status,
  detail,
  replayCursor,
});

const ctrl = (
  conv: string,
  seq: number,
  fields: {
    requestedPermission?: UiPermissionMode;
    effectivePermission?: UiPermissionMode;
    requestedModel?: string;
    effectiveModel?: string;
    modelLocked?: boolean;
  }
): UiFixtureEvent => ({
  kind: "controlState",
  id: id(conv, seq),
  conversationId: conv,
  uiSeq: seq,
  ...fields,
});

const user = (
  conv: string,
  seq: number,
  text: string,
  recovered?: boolean
): UiFixtureEvent => ({
  kind: "userMessage",
  id: id(conv, seq),
  conversationId: conv,
  uiSeq: seq,
  text,
  ...(recovered ? { recovered } : {}),
});

const agent = (
  conv: string,
  seq: number,
  source: UiAgentFamily,
  text: string,
  runId?: string,
  recovered?: boolean
): UiFixtureEvent => ({
  kind: "agentMessage",
  id: id(conv, seq),
  conversationId: conv,
  uiSeq: seq,
  source,
  text,
  runId,
  ...(recovered ? { recovered } : {}),
});

const delta = (
  conv: string,
  seq: number,
  source: UiAgentFamily,
  messageId: string,
  chunk: string,
  runId?: string,
  done?: boolean
): UiFixtureEvent => ({
  kind: "agentMessageDelta",
  id: id(conv, seq),
  conversationId: conv,
  uiSeq: seq,
  source,
  messageId,
  chunk,
  runId,
  ...(done ? { done } : {}),
});

const tool = (
  conv: string,
  seq: number,
  source: UiAgentFamily,
  toolCallId: string,
  toolName: string,
  phase: "requested" | "started" | "completed" | "failed",
  title: string,
  detail?: string,
  runId?: string,
  recovered?: boolean
): UiFixtureEvent => ({
  kind: "toolActivity",
  id: id(conv, seq),
  conversationId: conv,
  uiSeq: seq,
  source,
  toolCallId,
  tool: toolName,
  phase,
  title,
  detail,
  runId,
  ...(recovered ? { recovered } : {}),
});

const perm = (
  conv: string,
  seq: number,
  source: UiAgentFamily,
  requestId: string,
  toolName: string,
  requestedMode: UiPermissionMode,
  status: "pending" | "allowed" | "denied",
  rationale?: string,
  runId?: string
): UiFixtureEvent => ({
  kind: "permissionRequest",
  id: id(conv, seq),
  conversationId: conv,
  uiSeq: seq,
  source,
  requestId,
  tool: toolName,
  requestedMode,
  status,
  rationale,
  runId,
});

const artifact = (
  conv: string,
  seq: number,
  source: UiAgentFamily,
  artifactId: string,
  name: string,
  artifactKind: string,
  summary: string,
  runId?: string
): UiFixtureEvent => ({
  kind: "artifactCard",
  id: id(conv, seq),
  conversationId: conv,
  uiSeq: seq,
  source,
  artifactId,
  name,
  artifactKind,
  summary,
  runId,
});

const gate = (
  conv: string,
  seq: number,
  source: UiAgentFamily,
  gateId: string,
  name: string,
  status: "running" | "passed" | "failed",
  detail?: string,
  runId?: string
): UiFixtureEvent => ({
  kind: "gateCard",
  id: id(conv, seq),
  conversationId: conv,
  uiSeq: seq,
  source,
  gateId,
  name,
  status,
  detail,
  runId,
});

const result = (
  conv: string,
  seq: number,
  source: UiAgentFamily,
  status: "accepted" | "rejected",
  verdict: string,
  summary: string,
  runId?: string
): UiFixtureEvent => ({
  kind: "resultCard",
  id: id(conv, seq),
  conversationId: conv,
  uiSeq: seq,
  source,
  status,
  verdict,
  summary,
  runId,
});

const failure = (
  conv: string,
  seq: number,
  source: UiAgentFamily,
  reason: string,
  detail?: string,
  runId?: string,
  recovered?: boolean
): UiFixtureEvent => ({
  kind: "terminalFailure",
  id: id(conv, seq),
  conversationId: conv,
  uiSeq: seq,
  source,
  reason,
  detail,
  runId,
  ...(recovered ? { recovered } : {}),
});

const run = (
  conv: string,
  seq: number,
  runId: string,
  agentFamily: UiAgentFamily,
  label: string,
  status: UiRunStatus,
  parentRunId?: string,
  role?: string
): UiFixtureEvent => ({
  kind: "runStatus",
  id: id(conv, seq),
  conversationId: conv,
  uiSeq: seq,
  runId,
  agent: agentFamily,
  label,
  status,
  parentRunId,
  role,
});

const deleg = (
  conv: string,
  seq: number,
  parentRunId: string,
  childRunId: string,
  targetAgent: UiAgentFamily,
  targetRole: string
): UiFixtureEvent => ({
  kind: "delegation",
  id: id(conv, seq),
  conversationId: conv,
  uiSeq: seq,
  parentRunId,
  childRunId,
  targetAgent,
  targetRole,
});

// --- scenarios --------------------------------------------------------------

// 1. New empty conversation.
const emptyConv: UiFixtureScenario = (() => {
  const c = "conv-empty";
  return {
    id: c,
    name: "New empty conversation",
    title: "New conversation",
    demonstrates: "empty state; enabled composer with nothing sent yet",
    initialDelivery: [
      meta(c, 1, "New conversation", { unread: 0 }),
      ctrl(c, 2, {
        requestedPermission: "ask",
        effectivePermission: "ask",
        requestedModel: "claude-opus-4-8",
        effectiveModel: "claude-opus-4-8",
        modelLocked: true,
      }),
      conn(c, 3, "connected"),
    ],
    nextUiSeqStart: 100,
  };
})();

// 2. Active Claude run with streaming output + tool activity.
const claudeActive: UiFixtureScenario = (() => {
  const c = "conv-claude-active";
  return {
    id: c,
    name: "Active Claude run (streaming)",
    title: "Refactor the auth module",
    demonstrates:
      "active run; streaming message; tool activity; stop control enabled",
    initialDelivery: [
      meta(c, 1, "Refactor the auth module", { unread: 1 }),
      ctrl(c, 2, {
        requestedPermission: "acceptEdits",
        effectivePermission: "acceptEdits",
        requestedModel: "claude-opus-4-8",
        effectiveModel: "claude-opus-4-8",
        modelLocked: true,
      }),
      conn(c, 3, "connected"),
      user(c, 4, "Refactor the auth module to extract the token-refresh path."),
      run(c, 5, "run-ca-1", "claude", "Claude · refactor auth", "running"),
      tool(
        c,
        6,
        "claude",
        "tc-1",
        "read_files",
        "completed",
        "Read auth/token.rs, auth/mod.rs",
        "2 files, 418 lines"
      ),
      tool(
        c,
        7,
        "claude",
        "tc-2",
        "edit",
        "started",
        "Editing auth/token.rs",
        "extracting refresh_token()"
      ),
      delta(c, 8, "claude", "m1", "I'm extracting the refresh path into its own ", "run-ca-1"),
      delta(c, 9, "claude", "m1", "function so the retry policy is testable in isolation…", "run-ca-1"),
    ],
    nextUiSeqStart: 100,
  };
})();

// 3. Client disconnects mid-run, then reconnect replays (duplicate + out-of-order).
const replayConv: UiFixtureScenario = (() => {
  const c = "conv-replay";
  // Pre-disconnect canonical events.
  const eUser = user(c, 4, "Add a bounded worker pool to the judge batch path.");
  const eRun = run(c, 5, "run-rp-1", "claude", "Claude · worker pool", "running");
  const eTool = tool(
    c,
    6,
    "claude",
    "tc-1",
    "grep",
    "started",
    "Searching for the judge batch loop",
    "ripgrep: 'for .* in findings'"
  );
  const eDelta1 = delta(c, 7, "claude", "m1", "Found the sequential loop; wiring a ", "run-rp-1");
  return {
    id: c,
    name: "Disconnect during run + replay",
    title: "Add a bounded worker pool",
    demonstrates:
      "disconnected transport; reconnect replay with DUPLICATE and OUT-OF-ORDER delivery; fold dedups + reorders",
    initialDelivery: [
      meta(c, 1, "Add a bounded worker pool", { unread: 1 }),
      ctrl(c, 2, {
        requestedPermission: "acceptEdits",
        effectivePermission: "acceptEdits",
        requestedModel: "claude-opus-4-8",
        effectiveModel: "claude-opus-4-8",
        modelLocked: true,
      }),
      conn(c, 3, "connected"),
      eUser,
      eRun,
      eTool,
      eDelta1,
      conn(c, 8, "disconnected", "Client lost the daemon — the run keeps going without us"),
    ],
    replay: {
      // Deliberately scrambled: replaying(9), then duplicates of 6 & 4 & 5,
      // then tail events out of order (11 before 10, 12 before 13), then connected(14).
      batch: [
        conn(c, 9, "replaying", "Replaying missed events from cursor 7", 7),
        eTool, // DUPLICATE (uiSeq 6)
        eUser, // DUPLICATE (uiSeq 4)
        delta(c, 11, "claude", "m1", "bounded semaphore around the per-file calls.", "run-rp-1", true),
        tool(c, 10, "claude", "tc-1", "grep", "completed", "Located the loop", "judge.rs:812", "run-rp-1"),
        eRun, // DUPLICATE (uiSeq 5)
        run(c, 13, "run-rp-1", "claude", "Claude · worker pool", "completed"),
        result(c, 12, "claude", "accepted", "ACCEPTED", "Bounded --jobs pool added; ordering preserved.", "run-rp-1"),
        conn(c, 14, "connected", "Caught up — nothing was lost"),
      ],
    },
    nextUiSeqStart: 100,
  };
})();

// 4. Claude parent delegates to a Codex child (run graph with a branch).
const delegationConv: UiFixtureScenario = (() => {
  const c = "conv-delegation";
  return {
    id: c,
    name: "Claude parent + Codex child",
    title: "Ship the retry-policy change",
    demonstrates:
      "parent/child runs; delegation branch; Claude vs Codex distinction; jump-to-run",
    initialDelivery: [
      meta(c, 1, "Ship the retry-policy change", { unread: 2 }),
      ctrl(c, 2, {
        requestedPermission: "auto",
        effectivePermission: "auto",
        requestedModel: "claude-opus-4-8",
        effectiveModel: "claude-opus-4-8",
        modelLocked: true,
      }),
      conn(c, 3, "connected"),
      user(c, 4, "Implement the retry policy and have Codex write the tests."),
      run(c, 5, "run-parent", "claude", "Claude · coordinator", "running"),
      agent(c, 6, "claude", "I'll implement the policy, then delegate the test suite to Codex.", "run-parent"),
      deleg(c, 7, "run-parent", "run-child", "codex", "implementer"),
      run(c, 8, "run-child", "codex", "Codex · write tests", "running", "run-parent", "implementer"),
      tool(c, 9, "codex", "tc-x", "write_file", "started", "Creating retry_policy_test.rs", undefined, "run-child"),
      agent(c, 10, "codex", "Writing property tests for the backoff bounds.", "run-child"),
      run(c, 11, "run-parent", "claude", "Claude · coordinator", "waiting"),
    ],
    nextUiSeqStart: 100,
  };
})();

// 5. Permission request (one pending, one already resolved).
const permissionConv: UiFixtureScenario = (() => {
  const c = "conv-permission";
  return {
    id: c,
    name: "Permission request",
    title: "Clean up stale migrations",
    demonstrates: "pending permission request card; resolved (allowed) request; deterministic veto framing",
    initialDelivery: [
      meta(c, 1, "Clean up stale migrations", { unread: 1 }),
      ctrl(c, 2, {
        requestedPermission: "ask",
        effectivePermission: "ask",
        requestedModel: "claude-opus-4-8",
        effectiveModel: "claude-opus-4-8",
        modelLocked: true,
      }),
      conn(c, 3, "connected"),
      user(c, 4, "Delete the stale migration files under db/migrations/legacy."),
      run(c, 5, "run-pm-1", "claude", "Claude · cleanup", "waiting"),
      perm(c, 6, "claude", "perm-read", "read_dir: db/migrations", "ask", "allowed", "Read-only listing — auto-allowed by policy.", "run-pm-1"),
      perm(c, 7, "claude", "perm-rm", "bash: rm -rf db/migrations/legacy", "acceptEdits", "pending", "Destructive delete outside acceptEdits scope — needs an explicit decision.", "run-pm-1"),
    ],
    nextUiSeqStart: 100,
  };
})();

// 6. Model mismatch (requested ≠ effective) with the lock lit.
const modelMismatchConv: UiFixtureScenario = (() => {
  const c = "conv-model-mismatch";
  return {
    id: c,
    name: "Model mismatch",
    title: "Investigate the flaky test",
    demonstrates: "requested vs effective model drift; model-lock indicator; drift surfaced not hidden",
    initialDelivery: [
      meta(c, 1, "Investigate the flaky test", { unread: 1 }),
      ctrl(c, 2, {
        requestedPermission: "ask",
        effectivePermission: "ask",
        requestedModel: "claude-opus-4-8",
        effectiveModel: "claude-sonnet-4",
        modelLocked: true,
      }),
      conn(c, 3, "connected"),
      user(c, 4, "Why does auth::tests::refresh_race fail 1 in 20?"),
      run(c, 5, "run-mm-1", "claude", "Claude · investigate", "running"),
      agent(c, 6, "system", "⚠ Model drift: requested claude-opus-4-8, observed claude-sonnet-4. The exact-model policy tripped the kill switch.", "run-mm-1"),
      run(c, 7, "run-mm-1", "claude", "Claude · investigate", "failed"),
    ],
    nextUiSeqStart: 100,
  };
})();

// 7. Verifier failure (gate failed → terminal failure → rejected result).
const verifierFailureConv: UiFixtureScenario = (() => {
  const c = "conv-verifier-failure";
  return {
    id: c,
    name: "Verifier failure",
    title: "Fix the token expiry bug",
    demonstrates: "gate failed card; terminal failure card; rejected result; failed run",
    initialDelivery: [
      meta(c, 1, "Fix the token expiry bug", { unread: 1 }),
      ctrl(c, 2, {
        requestedPermission: "acceptEdits",
        effectivePermission: "acceptEdits",
        requestedModel: "claude-opus-4-8",
        effectiveModel: "claude-opus-4-8",
        modelLocked: true,
      }),
      conn(c, 3, "connected"),
      user(c, 4, "Fix the off-by-one in token expiry and prove it with a test."),
      run(c, 5, "run-vf-1", "claude", "Claude · expiry fix", "running"),
      tool(c, 6, "claude", "tc-1", "edit", "completed", "Patched auth/token.rs", "expiry: >= → >"),
      gate(c, 7, "claude", "gate-verify", "verify (cargo test)", "failed", "2 failed: refresh_race, expiry_boundary", "run-vf-1"),
      failure(c, 8, "system", "Verifier gate failed", "The trusted gate ran and did NOT pass. not-run is not a pass; failed is not accepted.", "run-vf-1"),
      run(c, 9, "run-vf-1", "claude", "Claude · expiry fix", "failed"),
      result(c, 10, "system", "rejected", "REJECTED", "o7d verdict: the verify gate failed; no artifact is trusted.", "run-vf-1"),
    ],
    nextUiSeqStart: 100,
  };
})();

// 8. Successful artifact + gate → accepted result → completed run.
const artifactGateConv: UiFixtureScenario = (() => {
  const c = "conv-artifact-gate";
  return {
    id: c,
    name: "Successful artifact and gate",
    title: "Add --jobs to the judge",
    demonstrates: "artifact card; passing gate card; accepted result; completed run",
    initialDelivery: [
      meta(c, 1, "Add --jobs to the judge", { unread: 0 }),
      ctrl(c, 2, {
        requestedPermission: "acceptEdits",
        effectivePermission: "acceptEdits",
        requestedModel: "claude-opus-4-8",
        effectiveModel: "claude-opus-4-8",
        modelLocked: true,
      }),
      conn(c, 3, "connected"),
      user(c, 4, "Add a bounded --jobs worker pool to o7 judge; keep ordering."),
      run(c, 5, "run-ag-1", "claude", "Claude · judge --jobs", "running"),
      tool(c, 6, "claude", "tc-1", "edit", "completed", "Edited judge.rs, main.rs", "+142 −18"),
      artifact(c, 7, "claude", "art-1", "feat/judge-jobs @ a1b2c3d", "branch+commit", "Bounded semaphore worker pool; per-file pairing preserved.", "run-ag-1"),
      gate(c, 8, "claude", "gate-verify", "verify (cargo test)", "passed", "121 passed / 0 failed", "run-ag-1"),
      result(c, 9, "system", "accepted", "ACCEPTED", "o7d verdict: gate passed on a real independent run; artifact is trusted.", "run-ag-1"),
      run(c, 10, "run-ag-1", "claude", "Claude · judge --jobs", "completed"),
    ],
    nextUiSeqStart: 100,
  };
})();

// 9. Interrupted then recovered conversation (honest post-restart status).
const interruptedConv: UiFixtureScenario = (() => {
  const c = "conv-interrupted";
  return {
    id: c,
    name: "Interrupted / recovered conversation",
    title: "Migrate the config loader",
    demonstrates:
      "recovered historical items; INTERRUPTED_BY_HOST_RESTART; interrupted run; fresh queued attempt",
    initialDelivery: [
      meta(c, 1, "Migrate the config loader", { unread: 1 }),
      ctrl(c, 2, {
        requestedPermission: "acceptEdits",
        effectivePermission: "acceptEdits",
        requestedModel: "claude-opus-4-8",
        effectiveModel: "claude-opus-4-8",
        modelLocked: true,
      }),
      conn(c, 3, "connected", "Restored from workspace snapshot"),
      // Items recovered from history after a host restart:
      user(c, 4, "Move config loading from env vars to the layered config crate.", true),
      agent(c, 5, "claude", "Started the migration; converting the env reads first.", "run-in-1", true),
      tool(c, 6, "claude", "tc-1", "edit", "completed", "Edited config/mod.rs", "env → layered", "run-in-1", true),
      failure(c, 7, "system", "INTERRUPTED_BY_HOST_RESTART", "The VPS rebooted mid-run. Saved: session id, branch, worktree, last tool call. This is NOT a completed run.", "run-in-1", true),
      run(c, 8, "run-in-1", "claude", "Claude · config migration", "interrupted"),
      run(c, 9, "run-in-2", "claude", "Claude · config migration (attempt 2)", "queued"),
    ],
    nextUiSeqStart: 100,
  };
})();

// 10a / 10b. Two concurrent conversations (both active, both unread) — the list
// itself demonstrates concurrency.
const concurrentA: UiFixtureScenario = (() => {
  const c = "conv-concurrent-a";
  return {
    id: c,
    name: "Concurrent conversation A",
    title: "Bump the CI runner image",
    demonstrates: "one of two simultaneously-active conversations; unread + activity indicators in the list",
    initialDelivery: [
      meta(c, 1, "Bump the CI runner image", { unread: 2 }),
      ctrl(c, 2, {
        requestedPermission: "ask",
        effectivePermission: "ask",
        requestedModel: "claude-opus-4-8",
        effectiveModel: "claude-opus-4-8",
        modelLocked: true,
      }),
      conn(c, 3, "connected"),
      user(c, 4, "Bump ubuntu-22.04 → ubuntu-24.04 in the worker gate."),
      run(c, 5, "run-ca", "claude", "Claude · CI bump", "running"),
      delta(c, 6, "claude", "m1", "Updating the runner label and re-pinning the action SHAs…", "run-ca"),
    ],
    nextUiSeqStart: 100,
  };
})();

const concurrentB: UiFixtureScenario = (() => {
  const c = "conv-concurrent-b";
  return {
    id: c,
    name: "Concurrent conversation B",
    title: "Draft the changelog",
    demonstrates: "the second simultaneously-active conversation",
    initialDelivery: [
      meta(c, 1, "Draft the changelog", { unread: 1 }),
      ctrl(c, 2, {
        requestedPermission: "plan",
        effectivePermission: "plan",
        requestedModel: "claude-opus-4-8",
        effectiveModel: "claude-opus-4-8",
        modelLocked: true,
      }),
      conn(c, 3, "connected"),
      user(c, 4, "Draft a changelog entry for the ledger + worker PRs."),
      run(c, 5, "run-cb", "codex", "Codex · changelog", "running"),
      agent(c, 6, "codex", "Drafting entries for PR 1 (ledger) and PR 2 (worker).", "run-cb"),
    ],
    nextUiSeqStart: 100,
  };
})();

export const SCENARIOS: readonly UiFixtureScenario[] = [
  claudeActive,
  delegationConv,
  permissionConv,
  replayConv,
  verifierFailureConv,
  artifactGateConv,
  modelMismatchConv,
  interruptedConv,
  concurrentA,
  concurrentB,
  emptyConv,
];

export const SCENARIOS_BY_ID: Readonly<Record<string, UiFixtureScenario>> =
  Object.fromEntries(SCENARIOS.map((s) => [s.id, s]));
