# 007 — Current State Audit

Evidence-grade snapshot of the `007` repo at branch `research/consilium-phase-minus-one` (HEAD `bf68e91`, identical to `main` for reads). Every claim below is derived from repository content, not prose. Citations are `path:line`.

**Headline:** 007 is **not** the multi-component control-plane architecture the owner intends. On disk it is a **single Rust binary crate** named `o7` (`Cargo.toml:2`) with three subcommands — `run`, `judge`, `invoke` (`src/main.rs:27-34`) — that shell out to the `claude`/`codex` CLIs as one-shot subprocesses and write flat-file run records. There is **no** `o7d` daemon, **no** cockpit/PWA UI, **no** Claude SDK worker, **no** Sandboy wiring, **no** WIT/Wasm, **no** RunGraph, **no** event ledger, **no** Tailscale, and **no** recovery/reconnect. Those appear only as roadmap prose in `docs/`. The whole tree is ~3,200 LOC of source (`src/*.rs`, 9 files), of which `judge.rs` (1,409) and `invoke.rs` (1,177) dominate; `run` itself is ~180 lines.

A recurring pattern: the code is small and careful (forbids `unsafe`, denies `unwrap`/`panic`/`indexing` via `Cargo.toml:44-60`, has proptest + fuzz + Kani proofs), while the ambitious architecture lives entirely in `docs/` as explicitly-deferred design.

---

## crates / modules layout
**Classification: IMPLEMENTED (but nothing like the intended component split).**

- Single binary crate `o7`, not a workspace: `Cargo.toml:1-10` (`[[bin]] name = "o7"`). Only one other Cargo.toml exists, the fuzz harness `fuzz/Cargo.toml:1-5` (`o7-fuzz`). No `package.json` anywhere (repo has zero JS/TS).
- Library modules (`src/lib.rs:7-13`): `agent`, `gate`, `invoke`, `judge`, `record`, `verdict`, `worktree`. Responsibilities:
  - `agent.rs` — `Engine{Claude,Codex}` enum + `run_claude` subprocess launcher + `DENY` list (`src/agent.rs:5-94`).
  - `gate.rs` — `.007/gate.toml` manifest parse + `bash -lc` step runner (`src/gate.rs:13-115`).
  - `verdict.rs` — `Verdict` enum + `reduce` (`src/verdict.rs:11-38`).
  - `record.rs` — `RunMeta` + `RunRecord` filesystem writer (`src/record.rs:13-89`).
  - `worktree.rs` — git worktree add/remove/diff/rev-parse (`src/worktree.rs:6-46`).
  - `judge.rs` — read-only FP-triage subcommand (whole file).
  - `invoke.rs` — schema-bound one-shot call subcommand (whole file).
- No `o7d/`, `cockpit/`, `worker/`, `server/`, `ui/`, or adapter crates exist. Top-level source dirs are only `src/ docs/ examples/ judge/ fuzz/ tools/ evidence/`.

## run lifecycle (states, who owns them)
**Classification: PARTIALLY-IMPLEMENTED — a linear synchronous pipeline; no state machine, no state owner, no daemon.**

- `o7 run` is one blocking function: canonicalize repo → add worktree → `execute()` → tear worktree down (`src/main.rs:82-135`). `execute()` runs the agent, harvests a record, runs gates, reduces to a verdict (`src/main.rs:137-183`).
- The **CLI process itself owns the run start-to-finish**; there is no `o7d` control plane (`grep o7d` matches only `O7DIFF*` rule codes in a doc, not a daemon). Invariant "only o7d decides run state" is therefore **not applicable — no o7d exists.**
- Terminal state is `Verdict` (`src/verdict.rs:11-18`), but only `Pass/Fail/Error` are ever produced (`reduce`, `src/verdict.rs:26-37`); `Warn/Blocked/NotApplicable` are reserved-for-later. Exit code 0/1 mirrors the verdict (`src/main.rs:131-133`).
- The project's own doc concedes the record store is "an **archive, not a state machine**… no cross-run index for dedup/retry" (`docs/loop-canvas.md:46`). No persisted `RunState`, no resumable/queued runs, no "closing the browser doesn't stop the agent" (runs die with the terminal — invariant 8 unmet).

## evidence / artifact format
**Classification: IMPLEMENTED (flat-file records); tamper-evident hash chain PLANNED.**

- `run` record layout `runs/<target>/<run-id>/`: `task.md`, `agent.stdout`, `diff.patch`, `gate/<name>.log`, `gate/verdict.json`, `meta.json` (`src/record.rs:43-88`, `src/main.rs:155-181`; matches `README.md:28-37`).
- `meta.json` = `RunMeta` (schema-versioned, forward-compat optionals): `schema, kind, run_id, target, repo, base_commit, engine, model, verdict, steps, agent_exit_code`, plus skip-if-none `session_id/cost_usd/started_at/finished_at` (`src/record.rs:13-35`). Note the timing/cost/session fields are **declared but never populated by `run`** — set to `None` at `src/main.rs:176-178`; a `TODO(phase-2)` to parse them is at `src/agent.rs:92-94`.
- `judge` overlay `fp-verdicts.json` + `meta.json` + `raw.<file>.txt`, schema at `judge/fp-verdicts.schema.json`; staleness guard = sha256 of the findings file (`generated_from`, `src/judge.rs:390,595-611`).
- `invoke` artifacts: `prompt.txt, stdout.raw, stderr.log, result.json, meta.json`; sha256 of prompt + declared inputs for provenance (`src/invoke.rs:178-180,415-429`).
- **No chained `prev_record_hash`/`record_hash`** across records — this is a P0 TODO, not code (`TODO.md:66-67`).

## current AgentDriver (how agents are actually launched)
**Classification: IMPLEMENTED as direct one-shot CLI subprocess; Claude SDK worker ABSENT; long-lived sessions ABSENT.**

- `run` launches Claude via `Command::new("claude") -p <task> --model … --permission-mode bypassPermissions --output-format json --max-turns … --disallowedTools <DENY>` (`src/agent.rs:65-91`). This is `claude -p` headless full-auto — the "adapter", not the SDK worker.
- Codex is **not wired** in `run`: `Engine::Codex => bail!("codex engine is Phase 2 — not wired yet")` (`src/agent.rs:61`).
- `judge` and `invoke` each spawn their own read-only subprocesses: `call_claude` (`src/judge.rs:962-1021`, `src/invoke.rs:628-661`) and `call_codex` (`src/judge.rs:880-952`, `src/invoke.rs:692-766`). These are the only two backends; both are subscription-auth CLIs, prompt over stdin.
- No `@anthropic-ai/claude-agent-sdk`, no persistent session, no streaming worker (`grep`: `claude sdk`, `claude-agent-sdk`, `session worker`, `websocket` = 0 hits). `invoke` deliberately passes `--no-session-persistence` (`src/invoke.rs:652`). Invariant 12 (no auto model failover): there is no failover path at all — one call, one model.

## worktree handling
**Classification: IMPLEMENTED.**

- `git worktree add -b <branch> <path> <base>` on a throwaway branch `o7/<run-id>`; force-remove on teardown; `diff_vs_base` stages `add -A` then `diff --cached <base>` (`src/worktree.rs:6-25`, `src/main.rs:105-127`).
- The code is explicit that a worktree is **not** a security boundary (aligns with intended invariant 10, but the boundary that would replace it is absent): "`current_dir(worktree)` sets **cwd, not a boundary**" (`docs/loop-canvas.md:45`) and `src/agent.rs:33-44` ("not a sandbox — the real guardrail is the throwaway worktree" / deny-list is best-effort).
- Not enforced: Claude and Codex in *separate* worktrees/runs (invariant 5). `run` only ever runs one engine (Claude); Codex bails.

## verification / gates
**Classification: IMPLEMENTED (gate runner + independent read-only judge + schema re-validation); artifact/acceptance checks beyond exit codes PARTIAL.**

- Gate = ordered `bash -lc` steps from `.007/gate.toml`, each producing a `StepVerdict{name,required,verdict,exit_code,log}`; success⇒PASS else FAIL, spawn error⇒ERROR (`src/gate.rs:57-114`). Reduction: any ERROR wins, then any required FAIL, else PASS (`src/verdict.rs:26-37`). Example manifest `examples/gate.own.net.toml` (ruff/mypy/pytest).
- "Agent DONE ≠ acceptance" (invariant 3) is *honored in `run`*: the verdict is computed purely from gate steps (`src/main.rs:160-161`), independent of the agent's own exit (recorded separately at `src/record.rs:24`).
- `invoke` independently re-validates backend output against a caller-supplied JSON Schema — it never trusts the backend's own conformance claim (`src/invoke.rs:159,246,805-818`).
- Acceptance is **gate-exit-code based only**: no artifact-content checks, no diff-policy enforcement (O7DIFF path/budget rules exist only as a design doc, `docs/actions-plans-evidence-abridge.md:417-426`). No two-of-two independent gate requirement in code.
- Verification harnesses that *do* exist: proptest + unit tests inline in `judge.rs`/`invoke.rs`; three fuzz targets (`fuzz/fuzz_targets/`); Kani proofs for the string slicers (`src/judge.rs:1053-1086`).

## Sandboy integration status
**Classification: ABSENT in this repo (PLANNED in docs; spiked in a sibling repo).**

- Gate steps run through **bare `bash -lc`**, not Sandboy (`src/gate.rs:80-84`). `GateStep` has only `name/cmd/required/env` — there is **no `sandbox_policy` field** to even carry a policy (`src/gate.rs:26-36`).
- Sandboy is a *separate* project in Own.NET, "spiked, not wired": "gate.rs still calls `Command::new("bash")` directly" (`docs/zero-trust-framework.md:74`; also `docs/security-layers.md:21` "❌ absent from 007 today", `README.md:104-108` "Not yet wired into `o7`"). Wiring it is P0 in `TODO.md:63-65`.

## WIT / Wasm component plans
**Classification: PLANNED (docs only) — ABSENT from 007's code.**

- Zero Wasm/WIT code in the tree (`grep wasm|WIT` hits only docs). The design position: WIT is "a typed plugin **ABI**… not the policy format" for untrusted-input parsers, cage them with Wasmtime (`docs/zero-trust-framework.md:462-490`) — matching intended invariant 11, but as prose. WASM SARIF adapters are marked "**Spiked** — `Own.NET/audit/adapters/`", i.e. in a different repo (`docs/paper-transplant-map.md:43`). Nothing in 007.

## cockpit (UI) roadmap and real UI code
**Classification: ABSENT.**

- Zero hits for `cockpit`, `PWA`, `mobile`(1 incidental), `web ui`, `http server`, `axum`/`actix`/`warp`, `websocket`, `SSE`(as a real term). No `package.json`, no HTML/TS/frontend. There is no UI code and no UI roadmap doc. The intended invariants about the UI (1: UI never launches agents; 8: closing browser doesn't stop the agent) are moot — no UI and no daemon exist to satisfy them.
- Closest external consumer surface: `o7 invoke` is explicitly shaped to match a **Python** consumer named "Demand Radar" (`src/invoke.rs:78-88,104-107`), a cross-repo conformance target — not a 007 cockpit.

## model policy & permission mode handling
**Classification: model policy ABSENT (only provider routing exists); permission mode = STATIC per-subcommand, no live switching.**

- No `ModelPolicy`, no `Exact`, no model-switch detection/halt (`grep ModelPolicy` = 0; `Exact` hits are the word "exactly" in prose). `--model` is a free string forwarded to the CLI (`src/main.rs:58-59`, `src/agent.rs:74`). Invariants 6 and 12 (halt on observed model switch; no auto-failover under Exact) are **unimplemented** — there is no model-identity check on the response at all.
- What *does* exist is provider **routing** by model-id prefix in `judge`: `model_family` + `resolve_provider` map `opus/sonnet/claude→Claude`, `gpt/o3/codex→Codex`, with a footgun guard that refuses a Claude model under `--provider codex` (`src/judge.rs:259-305,403-408`). This is routing/validation, not policy enforcement.
- Permission mode is **hardcoded per subcommand, not switchable**: `run` uses `--permission-mode bypassPermissions` (`src/agent.rs:74-76`); `judge` and `invoke` pin `--permission-mode default` plus `--tools ""`/`--strict-mcp-config` to force a closed, read-only world (`src/judge.rs:979-985`, `src/invoke.rs:645-651`). No mechanism for live permission-mode switching mid-session (invariant 7 unmet — there is no session).

## existing event schemas (append-only ledger? recovery?)
**Classification: PARTIALLY-IMPLEMENTED as an immutable per-run archive; a real event ledger is ABSENT; recovery ABSENT.**

- The only persisted "schema" is `RunMeta`/`JudgeMeta`/`InvokeMeta` (`src/record.rs:13-35`, `src/judge.rs:223-244`, `src/invoke.rs:107-126`) plus `StepVerdict` (`src/verdict.rs:41-49`). These are per-run summary docs, not an append-only event stream. Each `schema` is a `u32` version field for forward-compat, not a chained ledger.
- No event log, no `runs/index.*`, no chained hashes — the doc calls the store an "archive, not a state machine" and lists the missing ledger as new infra (`docs/loop-canvas.md:46,49`). No `reconnect` anywhere (`grep` = 0); the 6 `recover` hits are `judge`'s verdict *key-recovery* logic, unrelated to crash recovery.

## persistence / recovery design
**Classification: persistence = flat files IMPLEMENTED; crash/reconnect recovery ABSENT.**

- Persistence is directory writes under `runs/` (`RunRecord`, `src/record.rs:42-88`) and `invoke`'s `--out` dir with an empty-dir guard that refuses to overwrite a prior run (`ensure_empty_out`, `src/invoke.rs:315-331`). No database, no WAL, no resumable state.
- No recovery-after-crash/reconnect design in code; `invoke` deliberately disables session persistence (`src/invoke.rs:652`). Intended invariant 9 (conversations & events recover after reconnect/crash) is **unaddressed** — there are no long-lived conversations to recover.

---

## Summary table

| Dimension | Status | Key evidence |
|---|---|---|
| Crates/modules layout | **IMPLEMENTED** (single binary crate, not the intended split) | `Cargo.toml:1-10`; `src/lib.rs:7-13`; only 2 Cargo.toml, 0 package.json |
| Run lifecycle / state ownership | **PARTIALLY-IMPLEMENTED** (linear CLI pipeline; no o7d, no state machine) | `src/main.rs:82-183`; `docs/loop-canvas.md:46` |
| Evidence/artifact format | **IMPLEMENTED**; hash-chain **PLANNED** | `src/record.rs:43-88`; `README.md:28-37`; `TODO.md:66-67` |
| AgentDriver (launch today) | **IMPLEMENTED** as one-shot `claude -p`/`codex exec`; **SDK worker ABSENT** | `src/agent.rs:65-91`; `src/agent.rs:61` (codex bails); `src/judge.rs:880-1021` |
| Worktree handling | **IMPLEMENTED** (explicitly "not a boundary") | `src/worktree.rs:6-25`; `src/agent.rs:33-44`; `docs/loop-canvas.md:45` |
| Verification / gates | **IMPLEMENTED** (gate + independent judge + schema re-validation); acceptance-beyond-exit-code **PARTIAL** | `src/gate.rs:57-114`; `src/verdict.rs:26-37`; `src/invoke.rs:805-818` |
| Sandboy integration | **ABSENT in code / PLANNED in docs** (spiked in sibling repo) | `src/gate.rs:80-84` (bare bash); `src/gate.rs:26-36` (no policy field); `docs/zero-trust-framework.md:74` |
| WIT / Wasm | **PLANNED (docs only) / ABSENT** | `docs/zero-trust-framework.md:462-490`; `docs/paper-transplant-map.md:43`; 0 code hits |
| Cockpit / UI | **ABSENT** | no UI files; `grep cockpit/PWA/axum/websocket` = 0; `src/invoke.rs:78-88` (consumer is a Python service, not a 007 UI) |
| Model policy & permission mode | **model policy ABSENT** (routing only); **permission mode STATIC, no live switch** | `src/agent.rs:74-76`; `src/judge.rs:259-305`; `src/invoke.rs:645-652`; 0 `ModelPolicy` hits |
| Event schemas / append-only ledger | **PARTIAL** (immutable per-run archive); event ledger **ABSENT** | `src/record.rs:13-35`; `src/verdict.rs:41-49`; `docs/loop-canvas.md:46,49` |
| Persistence / recovery | persistence **IMPLEMENTED** (flat files); recovery/reconnect **ABSENT** | `src/record.rs:42-88`; `src/invoke.rs:315-331,652`; 0 `reconnect` hits |

**Intended-invariant reality check:** (1) moot — no UI. (2) moot — no o7d; the CLI decides. (3) honored in `run` (verdict from gates, not agent — `src/main.rs:160-161`). (4) gates are independent but acceptance is exit-code-only, no artifact checks. (5) unmet — `run` runs Claude only; Codex bails. (6),(12) unmet — no model-identity check or failover exists. (7) unmet — permission mode is static, no session to switch. (8),(9) unmet — synchronous CLI, no daemon/recovery. (10) acknowledged in code but the replacement boundary (Sandboy) is not wired. (11) docs-only.
