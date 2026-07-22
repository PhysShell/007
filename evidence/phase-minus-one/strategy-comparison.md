# Strategy comparison — how 007 should relate to Consilium

Grounding facts (from evidence): Consilium is **MIT** (vendoring/forking is legally clean, keep the NOTICE). Consilium `core` is a **single binary crate** named `consilium` (with `main.rs`), ~24.5k LoC, **tightly coupled through `conduct.rs`** — it is *not* a library with a small public API. 007 today is a **~3.2k-LoC single CLI crate** (`o7`) with none of the intended control-plane built (no o7d, ledger, cockpit, worktree wiring, Sandboy). So Consilium is, in most dimensions, *more built* than 007 — but its center (LLM-owned verdict + always-on silent failover + run-tied-to-socket + post-hoc transcript) conflicts with 007's core invariants.

## The four strategies

### Strategy A — Consilium as an upstream library dependency (`cargo add consilium`)
- **Advantages:** least code to write; upstream bug-fixes flow in.
- **Coupling:** maximal — `consilium` is one crate with a binary; depending on it drags the whole orchestrator, the silent-failover core, and its config/opinions. There is no small public API to depend on.
- **Maintenance burden:** low day-to-day, but every 007 need becomes an upstream PR or a patch.
- **Security update path:** you inherit upstream's threat model, incl. the unwired safety machinery and untrusted-repo config loading; you cannot fix-forward without forking anyway.
- **Compatibility risk:** high — pulls `resilience.rs` (auto-failover) and `conduct.rs` (LLM verdict), both of which violate 007 invariants 2/6/12.
- **Source-of-truth risk:** severe — the dependency owns the verdict, the failover, and the run lifecycle; o7d cannot be the source of truth.
- **Effect on roadmap:** fast demo, but boxes 007 into Consilium's model.
- **Exit strategy:** poor — ripping out a deep dependency later is expensive.
- **Verdict: reject as the primary strategy.**

### Strategy B — Fork / vendor selected Consilium modules into 007
- **Advantages:** take exactly the high-value, low-coupling, test-proven pieces (`verify.rs`, `sessions.rs`+`runner.rs`, `quota.rs`, `safety/git.rs`+`fs.rs`+`trust.rs`, `event.rs`, `protocol.rs`+UI, MCP schemas) and adapt them so **o7d owns verdict/ledger/policy**. MIT permits it. Full control, no runtime coupling to an incompatible core.
- **Coupling:** you choose it per module. The universal leaves (`event`, `confine`, `topology`, `json_extract`) and the persistence/safety modules extract cleanly; the run-stack hub (`conduct`) does not and is deliberately left behind.
- **Maintenance burden:** medium — you own the vendored code and must occasionally re-sync fixes from upstream by hand.
- **Security update path:** you control it; you can wire the trust store, add Sandboy, make gates fail-closed — none of which upstream does.
- **Compatibility risk:** low, because you import the *compatible* pieces and reimplement the *incompatible* verdict/failover/ledger in 007.
- **Source-of-truth risk:** low — o7d stays authoritative; imported modules are subordinate primitives.
- **Effect on roadmap:** accelerates the hard parts (worktree isolation, grounding oracle, lifecycle, UI) that 007 has not built, without importing the parts it must own.
- **Exit strategy:** good — vendored modules are just 007 code; drop any one independently.
- **Verdict: recommended core of the strategy.**

### Strategy C — Consilium as an external worker/orchestration subprocess under 007
- **Advantages:** fastest path to real multi-agent delegation *today*: Consilium's **MCP attached-conductor** server is literally built for a Claude session to drive Codex/Gemini/Grok workers at zero programmatic Claude cost. 007 could shell out to it as a bridge.
- **Coupling:** process-level only, but Consilium-as-blackbox owns its own cwd/verdict/failover model.
- **Maintenance burden:** low (pin a binary), but you inherit its behavior wholesale.
- **Security update path:** you must run it *inside Sandboy* and treat its output as advisory — it provides neither process sandbox nor trusted-verify.
- **Compatibility risk:** medium — acceptable only if 007 treats it as an untrusted advisory worker, never as the verdict authority.
- **Source-of-truth risk:** high if trusted; acceptable if strictly a sandboxed, advisory bridge.
- **Effect on roadmap:** unblocks delegation now while the vendored worker layer is built.
- **Exit strategy:** excellent — it is an external process; delete it when 007's own workers exist.
- **Verdict: OPERATOR-REJECTED (post Phase -1, see `operator-decision.md`).** A temporary bridge is not adopted: it would add a second worker-launch path, a second delegation protocol, a separate cancellation model, and no durable run identity, then demand a mandatory later removal. 007 builds its own typed delegation protocol instead; Consilium MCP schemas are reference-only.

### Strategy D — Ideas + fixtures only, reimplement everything
- **Advantages:** maximum purity/control; zero coupling and zero upstream-sync risk; Consilium's excellent test fixtures (recorded Claude/Codex streams, hostile-repo worktree probes) seed 007's own tests.
- **Coupling:** none.
- **Maintenance burden:** highest up-front (rewrite ~everything).
- **Compatibility/source-of-truth risk:** none.
- **Effect on roadmap:** slowest; discards genuinely good, tested code (worktree TOCTOU hardening, grounding oracle, lifecycle) that would take 007 months to re-derive and re-harden.
- **Exit strategy:** n/a.
- **Verdict: correct for the incompatible core (failover, transcript, conduct's verdict, Tauri) and as the source of fixtures; wrong to apply wholesale.**

## Recommended strategy — Hybrid **B (vendor) + D (reference/reject)**

> **Operator decision (post Phase -1, `operator-decision.md`):** the temporary MCP bridge (Strategy C) is **rejected permanently**. The strategy is now B (vendor selected modules) + D (reference/reject the rest); MCP is reference-only. Delegation is 007's own typed protocol with o7d as the sole source of truth.

Not "partially reusable" — here are the exact boundaries.

**Vendor into 007 (Strategy B), adapting so o7d owns state (see reuse-matrix.json):**
- `orchestrator/verify.rs` → o7d independent gate (move the *decision* to o7d; run inside Sandboy; source commands from trusted config).
- `sessions.rs` + `orchestrator/runner.rs` → o7d worker process manager (add process-group reap; require Sandboy).
- `quota.rs` → the append-only SQLite substrate for the event ledger **and** BudgetPolicy (add spend gate; fail-closed reads; run-event rows).
- `safety/git.rs` + `fs.rs` + `trust.rs` + `commands.rs` → a standalone `worktree-isolation` crate **plus** wired trust-on-first-use (o7d as sole owner; never call it a sandbox).
- `event.rs` + `protocol.rs` + `ui/src` (React) → the cockpit event/protocol/UI layer (extend events for permission/rate-limit/model-drift + tool start/complete; add run_id/seq/cursor; swap the one impure `useSession` hook).
- `eval.rs`'s external-oracle + protected-path-restore pattern → 007's acceptance-testing harness.

**No MCP bridge (operator decision):** the temporary Strategy-C MCP bridge is **rejected**. Delegation is 007's own typed protocol — a `delegation.requested` event on the ledger, with o7d checking role/budget/depth/exact-model/permission/paths/gates and itself creating the child run + worktree. `mcp.rs` **schemas are reference-only** (design input for that typed protocol; not vendored, no runtime dependency).

**Reference only (not vendored):** `council` and `conduct` (adopt the grounding-rule *idea* with an o7d-owned verdict, not the engine), `mcp.rs` schemas, `topology`.

**Reject as code / fixtures-only:** the **MCP runtime bridge**, `resilience.rs` (always-on silent failover — incompatible with Exact), `models.rs` catalog auto-upgrade (keep-newest vs exact-pin), `transcript.rs` (post-hoc, no recovery), `conduct.rs`'s *verdict ownership* (relocate to o7d), and the Tauri in-process shell (wrong for android-first).

**Why this and not A:** 007's non-negotiable invariants (o7d owns the verdict; Exact forbids silent failover; browser-close must not kill the agent; append-only ledger with recovery; Sandboy is the process boundary) are exactly the things Consilium's *core* gets wrong, while its *peripheral* modules (isolation, oracle, lifecycle, UI, storage shape) are exactly what 007 lacks and are high quality. Depending on the whole (A) imports the conflicts; reimplementing the whole (D) discards the wins. B+C takes the wins, quarantines the conflicts, and keeps o7d authoritative — with a clean per-module exit.
