# Consilium — Phase -1 acquisition & divergence study

**Status: Phase -1 evidence is complete and ready for independent review.** (Not ACCEPTED — acceptance is an independent gate's call.)

Research-only study. No 007 production code was changed. All findings cite evidence under [`evidence/phase-minus-one/`](../../evidence/phase-minus-one/); every claim there is anchored to `path:line` in one of the two repositories.

---

## 1. Executive verdict

Consilium (`TemurTurayev/consilium`, MIT) is a **more-built system than 007 is today**, and it independently solves several problems 007 has only planned — worker process lifecycle, a deterministic build/test grounding oracle, TOCTOU-hardened git-worktree isolation, a wire-agnostic React cockpit, and an append-only SQLite store. But its **center of gravity is incompatible with 007's core invariants**: the accept/fail verdict is owned by an **LLM conductor** (not a trusted deterministic plane), **automatic silent model failover is always-on** (the exact opposite of `ModelPolicy::Exact`), the **run lifetime is tied to the UI socket** (closing the browser SIGKILLs the agent), and the **"transcript" is a post-hoc whole-file JSON** with no crash/reconnect recovery. Its strong safety machinery (worktree isolation, trust store) is **built, tested, and completely unwired** — no production run routes through it.

**Recommendation:** do **not** depend on Consilium as a library and do **not** fork it wholesale. **Vendor a chosen set of low-coupling, test-proven modules** (verify oracle, process lifecycle, worktree isolation + trust store, quota/ledger storage shape, event/protocol + React presentation) and **adapt them so o7d owns the verdict, the ledger, and model policy**; build 007's **own typed delegation protocol** (o7d as sole source of truth) with Consilium's MCP schemas as **reference only**; and **reject** the silent-failover core, the post-hoc transcript, the conductor's verdict ownership, and the in-process Tauri shell (reference + fixtures only).

> **Operator decision (post Phase -1 acceptance — see [`operator-decision.md`](../../evidence/phase-minus-one/operator-decision.md)):** the "temporary sandboxed MCP bridge" this report originally recommended is **rejected permanently** — no MCP bridge, no MCP runtime dependency, MCP schemas reference-only; delegation is 007's own typed protocol; and **council + conduct are reference-only**. Phase -1 remains ACCEPTED/CLOSED; this is a deliberate strategy choice between the two options the report described, not a research change. Sections 10/13/14 and the reuse-matrix reflect this.

### Definitive answers (task §7)
1. **Should 007 depend on Consilium?** No — not as a library/runtime dependency.
2. **Should 007 fork Consilium?** No wholesale fork; yes to **vendoring specific modules** (MIT).
3. **Should individual modules be ported?** Yes — the peripheral, test-proven ones (see reuse-matrix.json).
4. **Highest-value modules:** `verify.rs` (grounding oracle), `safety/git.rs`+`fs.rs`+`trust.rs` (worktree isolation + trust), `sessions.rs`+`runner.rs` (process lifecycle), the React UI + `protocol.rs`, `quota.rs` (append-only storage shape), `eval.rs` (external-oracle pattern).
5. **Dangerous to port:** `resilience.rs` (silent failover), `models.rs` auto-upgrade, `transcript.rs` (post-hoc), `conduct.rs`'s verdict ownership, the Tauri in-process shell.
6. **Can Consilium replace the planned AgentDriver?** Partly — the `Adapter` trait + `sessions/runner` lifecycle are a strong base, but the permission model and event set must be extended; it is an adapt, not a drop-in.
7. **Can it replace o7d?** **No.** Consilium has no trusted control plane; its decider is an LLM.
8. **Can it replace the Cockpit?** The **React UI yes** (wire-agnostic); the **server no** (run-tied-to-socket must be rewritten).
9. **Can it provide the exact-model kill switch?** **No** — there is no `ModelPolicy`/drift check anywhere; failover is mandatory and ungated.
10. **Can it provide live permission switching?** **No** — operator control parks only *between* subtasks, never mid-call.
11. **Can it replace Sandboy?** **No** — it provides zero process containment; the worktree is not a sandbox (and isn't even wired).
12. **Can its `conduct` be used as RunGraph?** Only as a **worker-driver after heavy adaptation** — reject its shared-cwd sequential model + LLM verdict; adopt the grounding-rule idea.
13. **Can its MCP be the delegation protocol?** **No — reference only (operator decision).** No MCP bridge, no MCP runtime dependency; 007 uses its **own typed delegation protocol** (`delegation.requested`, o7d as sole source of truth). MCP schemas inform that design only as reference.
14. **Recommended integration sequence:** append-only ledger → worker lifecycle (Sandboy) → worktree+verifier+trust (o7d-owned) → event protocol → Claude vertical slice (Exact kill-switch) → Claude SDK worker → Codex worker → native 007 delegation → cockpit. Authoritative 9-PR sequence in [`operator-decision.md`](../../evidence/phase-minus-one/operator-decision.md); **no MCP bridge**.
15. **First vertical slice:** one Claude worker, run in a **Sandboy + detached worktree**, driven by o7d, emitting to an **append-only ledger**, with the **verify oracle as an o7d-owned gate** and **ModelPolicy::Exact halting on drift** — a single delegated subtask end-to-end with o7d owning the verdict.

---

## 2. Exact repository identities
See [`evidence/phase-minus-one/identities.json`](../../evidence/phase-minus-one/identities.json).
- **007:** `/home/physshell/Documents/repos/007`, origin `github.com/PhysShell/007`, HEAD `bf68e91b102859d74ffb9cd4acc12e4b90ad9958`, base branch `main` (clean, in sync with origin), research branch `research/consilium-phase-minus-one`.
- **Consilium:** origin `github.com/TemurTurayev/consilium`, resolved `origin/main` = `191f2d5f81458e041f60a734c60095e251e521b4` (commit 2026-07-16), core crate `consilium` v0.2.0, **MIT**. Cloned shallow (`--depth=1`). SHA resolved independently, not taken from the task prompt.

## 3. Environment and validation status
See [`environment.json`](../../evidence/phase-minus-one/environment.json) + [`test-results.json`](../../evidence/phase-minus-one/test-results.json). Host: Linux x86_64, 12 cores / 31 GB; toolchain via `nix` (rustc 1.96.1, cargo 1.96.2, clippy 0.1.96, node 24.18.0, npm 11.16.0). Ran on the local host (the tandem VPS is 1 core/2 GB with no Rust and would BLOCK the builds).

| Target | Result |
|---|---|
| `cargo build/test/clippy -p consilium` (core) | **PASS** — 480 tests pass, 0 fail, 0 ignored, 0 clippy warnings |
| `npm ci / typecheck / test / build` (ui) | **PASS** — 68 tests pass, clean build |
| `cargo build/test --workspace` (desktop/Tauri) | **BLOCKED** — missing system `dbus-1`/webkit2gtk/gtk3/libsoup; environment, not a code defect. Upstream failures not patched. |
| Claude minimal probe | **PASS** (1/2 calls) — see `raw/claude-probe.redacted.json` |
| Codex minimal probe | **BLOCKED** — `codex` CLI not installed; install/login disallowed by scope |
| Gemini / Grok probes | **NOT RUN** — out of scope |

Zero-quota coverage exists across **all** pipeline layers (the 480 core tests, default run skips `#[ignore]`, use scripted adapters + recorded fixtures — no network/quota). The E2E acceptance gate is met specifically by **three named, isolated, reproducible zero-quota flows** (see [`zero-quota-e2e.md`](../../evidence/phase-minus-one/zero-quota-e2e.md), captured in `raw/zero-quota-e2e.log`): `conduct_test::happy_path_single_subtask` (adapter→session→conduct→terminal Accept), `server_test::ws_streams_conduct_events_then_terminal_frame` (WS front door → … → terminal frame), and `conduct_test::failing_tests_force_rework_even_if_conductor_would_accept` (the flow that traverses **verify** → grounding veto → terminal). The "480 tests" figure is coverage, not a single scenario.

## 4. What Consilium actually implements
A multi-agent orchestrator that **forks vendor CLIs** (`claude`, `codex`, `agy`, `grok`) as child processes and normalizes their stdout into one `AgentEvent` stream, driven by a conductor/worker deliberation loop, exposed via a localhost WebSocket server (desktop UI) and a stdio MCP server ("attached-conductor"). CLI subcommands: `conduct/council/review/auto/serve/mcp/doctor/init/models/eval`. Full map in [`consilium-module-map.md`](../../evidence/phase-minus-one/consilium-module-map.md).

## 5. What is production-quality
- **`verify.rs`** — deterministic, timeout-bounded (600s SIGKILL), "not-run is not a pass," lint advisory. A real grounding oracle.
- **`sessions.rs`/`runner.rs`** — kill-on-drop, concurrent stderr drain, bounded backpressure, timeout-kills-child, `advisory&&write` hard-bail; real-process tested.
- **`safety/git.rs`+`fs.rs`+`trust.rs`** — TOCTOU-hardened worktree isolation (fd/device/inode-bound capability, `--no-checkout` raw materialization, fail-closed cleanup) + digest-scoped trust store; adversarial hostile-repo tests (hooks/smudge/fsmonitor never execute; substitution preserves both trees). See [`worktree-safety.md`](../../evidence/phase-minus-one/worktree-safety.md).
- **`quota.rs`** — genuinely append-only INSERT-only SQLite with a windowed aggregate.
- **`eval.rs`** — external-oracle scoring with protected-path restore (an approach can't pass by rewriting its own test).
- **React UI** — wire-agnostic pure reducer + presentational components, single impure hook.
- **`event.rs`/`protocol.rs`** — clean ts-rs single-source-of-truth typed wire.

## 6. What is beta/prototype quality
- **`resilience.rs` failover** — works, but silent-ish model switching with no Exact mode (design-incompatible, not "buggy").
- **`conduct.rs` verdict** — LLM-owned; shared-cwd sequential; disjoint-files unenforced; no rollback of failed writes; no-verifier accepts on the agent's word.
- **`transcript.rs`** — post-hoc whole-file; a mid-run crash loses everything; no recovery.
- **Grok adapter** — beta; token accounting "unverified against real output."
- **`prompts.rs`** — bare interpolation of feedback/findings with an explicit `TODO(M3)` prompt-injection hardening note.
- **The entire `safety/` worktree+trust+preflight subsystem is UNWIRED** — production-quality code with zero production callers. This is the single most important quality caveat: excellent machinery that protects nothing today.

## 7. Architectural overlap with 007
007 today is a **~3.2k-LoC single CLI crate** (`o7`, subcommands run/judge/invoke) that shells out to `claude`/`codex` one-shot and writes flat-file records; none of the intended control plane (o7d, ledger, cockpit, Sandboy wiring, worktree wiring, RunGraph, WIT) exists in code (see [`007-current-state.md`](../../evidence/phase-minus-one/007-current-state.md)). Genuine overlaps: (a) **"DONE ≠ acceptance"** — both separate the agent's self-report from acceptance (007 via gate exit codes; Consilium via the grounding rule); (b) **worktree-as-not-a-sandbox** — both state it explicitly; (c) **provider routing/adapters** over subscription CLIs; (d) **verification gates**. Consilium has built out (a)/(c)/(d) far past 007, and has the worktree isolation 007 only planned.

## 8. Fundamental divergences
See [`rungraph-divergence.md`](../../evidence/phase-minus-one/rungraph-divergence.md) + [`security-boundaries.md`](../../evidence/phase-minus-one/security-boundaries.md).
1. **Verdict authority:** LLM conductor vs o7d (trusted, deterministic). Consilium cannot be the source of truth.
2. **Model policy:** always-on silent failover (`resilience.rs:190-317`, cross-family reorder `conduct.rs:1237`) vs `ModelPolicy::Exact` (halt on any switch). No lock/kill-switch exists.
3. **Run vs UI lifetime:** run tied to the socket (`server.rs:382-407` → `kill_on_drop`) vs "closing the browser must not stop the agent."
4. **Persistence:** post-hoc whole-file transcript vs append-only ledger with cursor resume + crash recovery.
5. **Isolation model:** shared in-place cwd, sequential, worktree isolation unwired; and no process sandbox at all — vs separate worktrees per run + Sandboy as the process boundary.
6. **Live control:** operator parks between subtasks vs live permission-mode switching of a running worker.

## 9. Reuse matrix summary
Full machine-readable matrix (25 components): [`reuse-matrix.json`](../../evidence/phase-minus-one/reuse-matrix.json).
- **adopt:** React presentational components + reducer (the only genuinely largely-as-is piece).
- **adapt:** Adapter trait, Claude & Codex adapters, AgentEvent, failure classification, **session runner** (needs process-group reap + Sandboy + async cancel), **verifier** (needs decision relocated to o7d + trusted command source + Sandboy + fail-closed), review parser, quota store, config, auth/doctor, worktree safety (extract), trusted-verify commands (wire), Axum server (scaffolding), wire protocol, **React transport** (useSession / protocol client / run creation).
- **reference:** topology, **council**, **conduct** (grounding-rule idea only, o7d-owned verdict), **MCP schemas**.
- **reject:** **MCP runtime bridge**, resilience/failover, model-health auto-upgrade, transcript store, Tauri shell.

> Reclassification notes: (a) per independent-gate review, `session runner` and `verifier` moved **adopt → adapt** (reuse is not "largely as-is"; both need Sandboy + o7d ownership changes), and **React UI was split** into an *adopt* presentational/reducer half and an *adapt* transport half. (b) Per **operator decision** (post Phase -1), **MCP moved to reference** (bridge rejected, schemas reference-only) and **council + conduct moved to reference** (not vendored).

## 10. Recommended strategy
**Hybrid: vendor selected modules (B) + reference/reject the rest (D). No MCP bridge (operator decision).** Exact module boundaries in [`strategy-comparison.md`](../../evidence/phase-minus-one/strategy-comparison.md); operator ruling in [`operator-decision.md`](../../evidence/phase-minus-one/operator-decision.md). Vendor `verify.rs`, `sessions/runner`, `quota.rs`, `safety/git+fs+trust`, `event/protocol` + React presentation, and the `eval.rs` oracle pattern — adapting each so o7d owns verdict/ledger/policy. **Delegation is 007's own typed protocol** (`delegation.requested`, o7d as the sole source of truth); MCP schemas + council + conduct are **reference-only**. Reject the MCP runtime bridge, `resilience.rs`/`models.rs`/`transcript.rs`/conduct's verdict/Tauri.

## 11. Security implications
See [`security-boundaries.md`](../../evidence/phase-minus-one/security-boundaries.md). The four gaps 007 must own: (1) **no exact-lock / ungated automatic model switching**; (2) **Sandboy is the missing process boundary — and the substitute worktree isn't even wired**, so today there is neither a worktree nor a sandbox around agent writes (no env scrub; gemini runs `--dangerously-skip-permissions`; secret reads / egress / arbitrary spawn / Docker socket / npm lifecycle scripts all fall through to the host); (3) **untrusted repo config supplies both verify commands and model config with the trust store disconnected** = RCE-on-verify; (4) **the verdict is LLM-owned and several gates fail open** (no-verifier accept, reviewer-unavailable accept, fail-open quota reads). The 5-category ownership split (git/worktree lifecycle · process sandbox · network · agent policy · evidence verification) is in the same file.

## 12. Roadmap changes
Full proposed delta (do not edit the real roadmap): [`roadmap-delta.md`](../../evidence/phase-minus-one/roadmap-delta.md). Shortens: worker lifecycle, grounding gate, worktree isolation, cockpit UI, ledger storage shape, event/protocol typing. Cannot shorten (must be owned): o7d verdict, Exact kill-switch, Sandboy, append-only recoverable ledger, live permission switching, run-decoupled-from-socket. Each new acquisition PR carries a stop-gate (e.g. o7d sole worktree owner; verify only inside Sandboy; under Exact any drift halts; browser-close leaves run alive).

## 13. Exact next PR sequence
Authoritative operator sequence ([`operator-decision.md`](../../evidence/phase-minus-one/operator-decision.md)) — **no MCP bridge**:
1. **Append-only ledger** — conversations, runs, events, monotonic sequence, cursor replay, crash recovery, idempotency keys.
2. **Worker lifecycle** — adapt `sessions`/`runner`: process-group ownership, cancellation, heartbeat, orphan detection, environment isolation, Sandboy boundary.
3. **Worktree & verifier** — extract worktree safety (o7d = sole owner); adapt `verify.rs`; wire the trusted command store; verdict stays with o7d.
4. **Event protocol** — `agent.init`, `agent.message.delta`, `tool.requested/started/completed`, `permission.requested/changed`, `rate_limit`, `model.observed`, `model.drift`, `delegation.requested/accepted`, `artifact.published`, `gate.completed`, `run.completed`.
5. **Claude vertical slice (answer #15)** — o7d → `claude -p` adapter → Sandboy → detached worktree → event ledger → verify → o7d verdict, with the exact-model kill switch.
6. **Claude Agent SDK worker** — persistent session, live permission switching, interrupt, resume, requested/effective mode, worker recovery.
7. **Codex worker** — real structured events, worktree, Sandboy, artifacts, model-identity policy. Until an observed model is proven for Codex: **Codex + ModelPolicy::Exact = fail-closed**.
8. **Native 007 delegation** — `delegation.requested`, child run, typed TaskSpec, completion proposal, gates, verified result, return to coordinator.
9. **Cockpit** — conversation timeline, Claude & Codex in one chat, cursor replay, permission selector, model lock, restore workspace, Android PWA via Tailscale.

## 14. Rejected alternatives
- **Strategy A (upstream library dep):** rejected — `consilium` is one tightly-coupled crate with a binary; depending on it imports the silent-failover core and the LLM verdict, making o7d non-authoritative. High source-of-truth + exit risk.
- **Strategy D wholesale (reimplement everything):** rejected as the *primary* approach — discards months of tested, hard-to-re-derive security code (worktree TOCTOU hardening, grounding oracle, lifecycle). Retained only for the incompatible core + as the source of test fixtures.
- **Adopting `conduct.rs` as RunGraph as-is / `resilience.rs` failover / `transcript.rs` as the ledger / the Tauri shell:** rejected on invariant grounds (verdict ownership, Exact, recovery, android-first). See §8.
- **Strategy C (temporary MCP bridge):** rejected by **operator decision** (post Phase -1) — it would add a second worker-launch path, a second delegation protocol, a separate cancellation model, and non-durable run identity, then require a mandatory later removal. 007 builds its own typed delegation protocol; MCP schemas are reference-only. See [`operator-decision.md`](../../evidence/phase-minus-one/operator-decision.md).

## 15. Open blockers
See [`open-questions.md`](../../evidence/phase-minus-one/open-questions.md). Highest: (1) **Codex real probe BLOCKED** (CLI not installed) — and Stage D found Codex exposes **no observed model**, so Exact may be unenforceable for Codex; decide the policy. (2) **Tauri workspace never built** (missing GUI system libs) — low impact given the reject, but its Rust code is unvalidated. (3) **Single-owner worktree contract** (repo-global prune + no `Drop` cleanup) must be guaranteed before wiring. Plus design questions: trusted-config location, ledger schema, model-id normalization for drift, live permission-switch contract, MCP-bridge retirement trigger.

## 16. Evidence index
[`evidence/phase-minus-one/README.md`](../../evidence/phase-minus-one/README.md) lists every artifact. Machine-readable: `identities.json`, `environment.json`, `test-results.json`, `event-mapping.json` (15 entries), `reuse-matrix.json` (24 components), `commands.ndjson`. Narrative: `007-current-state.md`, `consilium-module-map.md`, `rungraph-divergence.md`, `worktree-safety.md`, `mcp-assessment.md`, `web-ui-assessment.md`, `security-boundaries.md`, `strategy-comparison.md`, `roadmap-delta.md`, `open-questions.md`. Raw redacted logs/traces under `raw/`.
