# Phase -1 — Consilium acquisition & divergence study — evidence index

Research-only. No 007 production code changed. Branch `research/consilium-phase-minus-one`, base `main` (`bf68e91`). Consilium studied at `191f2d5f81458e041f60a734c60095e251e521b4` (MIT).

**Main report:** [`docs/research/consilium-phase-minus-one.md`](../../docs/research/consilium-phase-minus-one.md)

## Files
| File | What it is | Stage |
|---|---|---|
| `identities.json` | Exact repo identities (007 + Consilium SHAs, license, clone method) | 3.2 |
| `environment.json` | Host, toolchain (via nix), provider CLI availability | B |
| `commands.ndjson` | **Significant** commands executed (build/test/clippy/probe/setup), with exit codes. A full shell transcript was not kept; this is the significant-command record. Final git state is in `final-state.txt`. | all |
| `zero-quota-e2e.md` | Named, isolated, reproducible zero-quota E2E flows (incl. one through the verify stage) | B |
| `final-state.txt` | Final `git status --short` / `git diff --stat main...HEAD` / `git log -1` | 10 |
| `test-results.json` | Build/test/clippy/UI + provider-probe results (PASS/FAIL/BLOCKED/NOT_RUN) | B, E |
| `007-current-state.md` | Audit of 007's ACTUAL state (implemented/partial/planned/absent/frozen) | A |
| `consilium-module-map.md` | Per-module map of Consilium `core` + real control/data flow | C |
| `event-mapping.json` | Consilium AgentEvent → proposed 007 event (lossless/lossy/missing/incompatible) | D |
| `rungraph-divergence.md` | Consilium `conduct` vs 007 RunGraph, verified point-by-point | G |
| `worktree-safety.md` | Load-bearing worktree-safety audit (proven-by-test vs comment-only) | H |
| `mcp-assessment.md` | MCP tool inventory + invariant checks + suitability | I |
| `web-ui-assessment.md` | Axum/WS/React/Tauri assessment + reuse verdict per layer | J |
| `security-boundaries.md` | Threat table + 5-category ownership split | L |
| `reuse-matrix.json` | Machine-readable per-component decision (adopt/adapt/reference/reject) — 24 components | M |
| `strategy-comparison.md` | 4 strategies compared + recommended hybrid with exact boundaries | N |
| `roadmap-delta.md` | Proposed roadmap delta (shorten/keep, replaced PRs, new PRs, stop-gates) | 6 |
| `open-questions.md` | Blockers and open design questions | — |
| `raw/` | Redacted probe stream + build/test/clippy logs | B, E |

## Validation status (see test-results.json)
- Consilium `core`: build **PASS**, test **480 passed / 0 failed / 0 ignored**, clippy **0 warnings**.
- Consilium `ui`: npm ci / typecheck / test (**68**) / build all **PASS**.
- Consilium workspace (`desktop/src-tauri`): **BLOCKED** — missing system GUI libs (`dbus-1`/webkit2gtk); environment, not a code defect. Upstream failures not patched.
- Zero-quota E2E: **PASS** — 3 named flows (`zero-quota-e2e.md`), one traversing the verify stage to a terminal outcome.
- Claude probe: **PASS** (1/2 calls). Codex probe: **BLOCKED** (CLI not installed; install/login disallowed).

## raw/
- `claude-probe.redacted.json` — curated probe fields ONLY (model as base+context_profile+service_tier, permissionMode, apiKeySource, usage, rate-limit type/status, result). All identifiers replaced by stable placeholders. The full stream-json raw trace was **removed** — it leaked session/uuid/request ids, cwd, and the local mcp_servers/skills/slash_commands; no API/OAuth token was present.
- `zero-quota-e2e.log` — captured output of the three named zero-quota E2E tests.
- `versions.txt`, `stageB-summary.txt`, `cargo-*.log`, `npm-*.log` — build/test/clippy/UI logs.
