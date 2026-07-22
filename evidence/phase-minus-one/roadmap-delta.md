# Roadmap delta (proposed) — effect of the Consilium acquisition

This is a **proposed** delta, not an edit to the real 007 roadmap. It assumes the recommended strategy: **vendor selected modules (B) + reference/reject the rest (D)**.

> **Operator decision (post Phase -1, `operator-decision.md`):** the temporary MCP bridge is **rejected permanently**; delegation is 007's own typed protocol with o7d as the sole source of truth; MCP/council/conduct are **reference only**. The authoritative 9-PR execution sequence lives in `operator-decision.md`; the PR notes below are aligned to it.

## Phases 007 can SHORTEN (borrow a proven, tested implementation)
- **Worker process lifecycle** — `sessions.rs`+`runner.rs` give kill-on-drop, bounded backpressure, timeout-kills-child, real-process tests. Saves the from-scratch build; shortens to a wiring + Sandboy-pairing task.
- **Grounding oracle / independent gates** — `verify.rs` is a deterministic, timeout-bounded, tamper-resistant build/test runner with "not-run is not a pass." Shortens the gates phase to "relocate the decision to o7d + wire trust + Sandboy."
- **Worktree isolation** — `safety/git.rs`+`fs.rs` are TOCTOU-hardened and test-proven; extracting them saves the hardest, easiest-to-get-wrong security code. Shortens the worktree-lifecycle phase to extraction + o7d-single-owner wiring.
- **Cockpit UI** — the React layer is wire-agnostic (pure reducer + presentational components, one impure hook). Shortens the cockpit-UI phase to "swap the transport hook."
- **Ledger storage shape** — `quota.rs` is a working append-only SQLite pattern to extend into the event ledger + BudgetPolicy.
- **Event/protocol typing** — `event.rs`+`protocol.rs`+ts-rs codegen give a typed wire foundation to extend.

## Phases 007 must NOT shorten (own them, do not import)
- **o7d verdict authority** — must stay deterministic and o7d-owned. Consilium's verdict is an LLM conductor; import the grounding-rule *idea*, not the decider.
- **Exact-model kill-switch** — must be built new; Consilium has no ModelPolicy and its failover is the opposite.
- **Sandboy (process boundary)** — absent in Consilium; nothing to borrow. The worktree is NOT a substitute.
- **Append-only ledger with crash/reconnect recovery** — Consilium's transcript is post-hoc, no recovery; build the real thing.
- **Live permission-mode switching for the SDK worker** — Consilium's operator control is boundary-only (between subtasks), not mid-call; build new.
- **Run-lifetime decoupled from the UI socket** — Consilium ties them; this is a from-scratch o7d ownership model.

## Consilium can REPLACE these planned 007 PRs (as vendored adaptations)
- "Build a worker process runner" → vendor `sessions.rs`/`runner.rs`.
- "Build a build/test verification gate" → vendor `verify.rs`.
- "Build detached-worktree isolation" → extract `safety/git.rs`+`fs.rs`.
- "Build trust-on-first-use for verify commands" → wire `safety/trust.rs`+`commands.rs`.
- "Scaffold the cockpit UI + wire protocol" → vendor `ui/src` + `protocol.rs`.
- "Design the quota/budget store" → extend `quota.rs`.

## NEW acquisition / adaptation PRs this creates
1. `worktree-isolation` crate extraction (sever ts-rs UI export; add run-id + RAII/typed cleanup; async wrappers). **Stop-gate:** o7d must be the *sole* worktree owner (repo-global prune races otherwise).
2. Vendor `verify.rs` + relocate the accept/fail decision to o7d + wire trust store. **Stop-gate:** no verify command runs from untrusted repo config without trust-on-first-use; verify runs only inside Sandboy.
3. Vendor `sessions.rs`/`runner.rs` + process-group reap + Sandboy pairing. **Stop-gate:** cancel kills the whole process group.
4. Extend `event.rs`/`protocol.rs` with permission.requested / rate_limit / model-info+drift / tool.started+completed + run_id/seq/cursor.
5. Extend `quota.rs` into the event ledger + BudgetPolicy (fail-closed reads, spend gate, run-event rows).
6. `ModelPolicy{Exact,ExplicitLadder}` enforced at the (rejected) failover choke point + a model-drift kill-switch fed by the new model-info event. **Stop-gate:** under Exact, any observed model change halts the run.
7. **Native 007 delegation** (operator decision — NO MCP bridge): a typed `delegation.requested` event on the ledger; o7d checks role/budget/depth/exact-model/permission/paths/gates and itself creates the child run + worktree. Consilium MCP schemas are reference input only. **Stop-gate:** delegation is a typed event bound to a run_id, never an MCP tool call; o7d is the sole source of truth.
8. Cockpit: vendor `ui/src`, swap `useSession` for a cursor/ledger client, remove UI-initiated run creation. **Stop-gate:** closing the browser leaves the run alive.

## Upstream dependencies this introduces
- Runtime crates transitively via vendored modules: `rusqlite(bundled)`, `rustix`, `axum`/`tokio-tungstenite` (cockpit), `ts-rs` (or drop it and hand-write types). **No `rmcp`** — the MCP bridge is rejected. The `worktree-isolation` tier needs only `anyhow/serde/rand/rustix`.
- A one-time NOTICE/attribution for MIT-vendored code.

## Impact per named subsystem
| Subsystem | Impact |
|---|---|
| **AgentDriver** | Borrow the `Adapter` trait shape + `sessions/runner` lifecycle; replace two-bool permission with live-switchable PermissionMode. Effort shortened. |
| **claude -p adapter** | Vendor+adapt `adapters/claude.rs`; must add rate_limit + model-drift + tool start/complete. |
| **Codex adapter** | Vendor+adapt `adapters/codex.rs`; must source an observed model (else Exact fails closed for Codex). Codex probe still owed (CLI not installed). |
| **normalized events** | Adopt `event.rs` shape, extend for the 5 missing/incompatible event types (see event-mapping.json), and persist them. |
| **worktree lifecycle** | Big win: extract the hardened machinery; o7d becomes sole owner; wire it (Consilium never did). |
| **MCP / delegation** | **MCP bridge REJECTED (operator decision).** Build 007's own typed delegation (`delegation.requested`); MCP schemas reference-only, no runtime dependency. |
| **council skill** | **Reference only (operator decision)** — not vendored; a design reference for a future advisory Skill; o7d keeps the verdict. |
| **conduct / RunGraph** | Adopt the grounding-rule idea; reject shared-cwd sequential model + LLM verdict; give each agent its own worktree; relocate the verdict. Largest effort. |
| **web cockpit** | Vendor UI + protocol scaffolding; rewrite `handle_session` for a run-registry + ledger cursor; decouple run from socket. |
| **quota budgets** | Extend the append-only store into a fail-closed BudgetPolicy + ledger. |
| **Sandboy** | Unaffected by acquisition — Consilium provides nothing here; it is the missing boundary the vendored pieces must run inside. |
| **exact model lock** | New build; Consilium is the anti-pattern reference. Add ModelPolicy + drift kill-switch. |
| **live permission mode** | New build; Consilium's operator control is boundary-only. |
| **crash recovery** | New build; Consilium's transcript is post-hoc. Extend the quota-store pattern into a recoverable ledger. |
| **WIT plugins** | Untouched — Consilium has no WIT/Wasm; out of scope for this acquisition. |
