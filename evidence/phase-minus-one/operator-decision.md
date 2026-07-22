# Operator decision (post Phase -1 acceptance)

Phase -1 is **ACCEPTED / CLOSED**. The independent gate returned ACCEPTED after the REWORK fixes; this document records a subsequent **operator strategy decision** that overrides one recommendation in the Phase -1 report. It is a deliberate choice between the two strategies the report described, **not** a research error. The research findings (module maps, event mapping, divergence, security, worktree study) are unchanged; only the *what-to-do* on MCP changes.

## Decision: MCP is REJECTED from the execution roadmap — permanently
- **MCP bridge:** REJECTED (not temporary, not "might be useful later").
- **MCP runtime dependency:** none.
- **Consilium MCP server:** do not port / do not vendor.
- **Consilium MCP schemas:** **reference only**, no vendoring.
- **Delegation transport:** 007's own typed protocol.
- **Source of truth:** o7d only.

**Rationale (operator):** a temporary MCP bridge does not move us toward the target architecture enough to justify a second worker-launch path, a second delegation protocol, a separate cancellation model, the absence of durable run identity, a later migration, and the mandatory removal of an already-working mechanism. "Temporary" infrastructure tends to still be here in four years with nobody remembering why.

## Target chain (adopted immediately, no MCP)
```
Claude worker
   ↓ typed worker protocol
o7d
   ↓ RunGraph / delegation request
Codex worker
   ↓ append-only events
o7d ledger
   ↓
artifacts + trusted gates + Sandboy
   ↓
o7d verdict
   ↓
Claude worker & Cockpit
```
Claude never calls Consilium and never reaches Codex via MCP. It **publishes a structured event**:
```json
{
  "type": "delegation.requested",
  "conversation_id": "conv-17",
  "parent_run_id": "run-42",
  "target": { "agent": "codex", "role": "implementer" },
  "task_spec": "task-91",
  "idempotency_key": "..."
}
```
o7d checks: role's right to delegate; budget; depth; exact-model policy; permission policy; allowed paths; required gates — then o7d itself creates the child run and worktree.

## Corrected PR sequence (authoritative)
1. **Append-only ledger** — conversations, runs, events, monotonic sequence, cursor replay, crash recovery, idempotency keys.
2. **Worker lifecycle** — adapt Consilium `sessions`/`runner`: process-group ownership, cancellation, heartbeat, orphan detection, environment isolation, Sandboy boundary.
3. **Worktree & verifier** — extract worktree safety; o7d becomes the sole worktree owner; adapt `verify.rs`; wire the trusted command store; verdict stays with o7d.
4. **Event protocol** — add: `agent.init`, `agent.message.delta`, `tool.requested`, `tool.started`, `tool.completed`, `permission.requested`, `permission.changed`, `rate_limit`, `model.observed`, `model.drift`, `delegation.requested`, `delegation.accepted`, `artifact.published`, `gate.completed`, `run.completed`.
5. **Claude vertical slice** — o7d → `claude -p` adapter → Sandboy → detached worktree → event ledger → verify → o7d verdict, with the exact-model kill switch.
6. **Claude Agent SDK worker** — after the one-shot slice is proven: persistent session, live permission switching, interrupt, resume, requested/effective mode, worker recovery.
7. **Codex worker** — real structured events, worktree, Sandboy, artifacts, model-identity policy. **Until an observed model is proven for Codex: Codex + ModelPolicy::Exact = fail-closed.**
8. **Native 007 delegation** — `delegation.requested`, child run, typed TaskSpec, completion proposal, gates, verified result, return to the coordinator.
9. **Cockpit** — conversation timeline, Claude & Codex in one chat, cursor replay, permission selector, model lock, restore workspace, Android PWA via Tailscale.

## Updated acquisition strategy (supersedes the report's recommendation for the named items)
```
Vendor / adapt:
- process lifecycle (sessions/runner)
- worktree safety
- verifier (verify.rs)
- trust store
- selected event/protocol ideas
- React presentation layer

Reference only:
- council
- conduct
- MCP schemas
- topology

Reject:
- MCP runtime bridge
- silent failover
- LLM-owned verdict
- transcript store
- Tauri shell
```
Net change vs the Phase -1 reuse-matrix: **MCP, council, and conduct move to REFERENCE** (no vendoring), and the **temporary MCP bridge is REJECTED**. All other classifications stand.

## The first vertical slice (spine before limbs)
```
One Claude run
→ created via o7d
→ launched in Sandboy
→ runs in a detached worktree
→ writes append-only events
→ passes a trusted verify gate
→ gets its verdict ONLY from o7d
→ survives UI disconnect
```
**No MCP, no Codex, no council, no Cockpit, no live SDK in the first slice.** Prove the spine first; add limbs, buttons, and the choir of electronic sages afterward.
