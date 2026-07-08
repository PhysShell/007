# 007 — resume here

Where we stopped + the exact next step. Updated 2026-07-02 (leaving for the day).

## Built & working
- **`o7 run`** — one isolated gated agent run (WSL worktree → full-auto claude →
  gate manifest → harvest). Scaffolded; not yet exercised on a real coding task.
- **`o7 judge`** — read-only FP-triage. **Verified working**: produced a
  contract-conforming `fp-verdicts.json` on the oracle with grounded reasoning.
- Contract reconciled to the domain's source of truth:
  `OwnAudit/docs/fp-judge/verdict-contract.md` (+ `rubric.md`). 007's
  `judge/fp-verdicts.schema.json` is its machine encoding.
- Design record: `.claude` memory `007-harness-design`. Judge details: `judge/README.md`.

## ▶ RESUME HERE — FP-direction gate (the real Phase-1 gate)
The oracle leaks-only proof passed (both `real`) — but that doesn't test the
discriminator; a judge that always says `real` would pass it too. The FP direction
is what matters for the 156 FP-suspects. Domain built the control. Run:

```bash
o7 judge --repo ../OwnAudit \
         --findings ../OwnAudit/oracle/fixtures/findings-fp-control.json \
         --rubric   ../OwnAudit/docs/fp-judge/rubric.md \
         --out      ../OwnAudit/artifacts/fp-verdicts-fpcontrol.json
```
**PASS = both come back `false_positive`**, reasons citing teardown (`-=` in
`Dispose` / `_timer.Dispose()`).
- PASS → judge discriminates both directions → Phase-1 done → go to the STS run.
- Says `real` on the fixed code → tune the loop:
  1. rubric first → domain (`OwnAudit/docs/fp-judge/rubric.md`)
  2. prompt template second → me (`judge/prompt.template.md`)

## Then — the real STS run (the 156)
Domain hands: `--repo <STS source root>` + `--findings <STS-210 findings.json>` +
`--out ../OwnAudit/artifacts/fp-verdicts.json`.
- **`--dry-run` FIRST** — prints files + call count (cost estimate for ~198 ids).
  `--max-files N` to batch.
- STS **source must be local** on this box (whole-file context).
- Overwrites the oracle overlay at that `--out`. Domain's report merges only the
  overlay whose `generated_from` == current `findings.json` (staleness guard).
- **Perf:** the per-file `claude` calls are independent — add a bounded `--jobs N`
  worker pool here (sequential today = sum of ~198 call latencies; parallel ≈ max
  per batch, near-linear speedup). Ordering-safe (pairing is per-file). Design:
  `docs/performance.md`.

## Domain (OwnAudit agent) — parallel, its lane
1. Consumer: report/dashboard loads `fp-verdicts.json`, verifies `generated_from`,
   merges (confirmed FP → "judged-FP" section, counted not hidden; real first;
   uncertain visible).
2. Hands the STS-run invocation (paths above).

## Backlog (deferred — design with real data)
- `o7 run` first real exercise on an Own.NET coding task.
- consensus (claude+codex race + cross-family judge), memory layer.
- OwnAudit Windows gates (`env: windows`), container egress hardening —
  assessed in `docs/microvm-isolation.md` (Phase 1: policy/diff-contract, no
  VM, blocks on nothing; Phase 3: `o7 run --isolation microvm` once an
  untrusted target repo's `gate.toml` is actually in scope).

## Zero Trust backlog (`docs/zero-trust-framework.md` §16 — full rationale there)

P0:
1. Compile Sandboy, pass `./tests/demo.sh`.
2. Wrap every `.007/gate.toml` step through `sandboy run` instead of bare `bash -lc`.
3. Make `sandbox_policy` mandatory per step — fail closed on a missing one.
4. Hash every gate/policy/task/diff/log artifact into `meta.json`, chained
   (`prev_record_hash`/`record_hash`).

P1:
5. Layer 3 egress: blanket UDP block + TCP host/CIDR allowlist, ordered per step.
6. Spotlighting wrapper around untrusted source/diff/stdout in `judge/prompt.template.md`.
7. Hash-lock (`.007/gate.lock`) for the gate manifest + policies; signing later.
8. `cargo-udeps`, OpenSSF Scorecard (public siblings), CodeQL/Semgrep over Own.NET/OwnAudit.

P2:
9. Behavioral-baseline counters + red-flag rules in `meta.json`.
10. CUE authoring pipeline (`cue export … --out toml`, `o7 policy compile`).
11. Firecracker/gVisor (Sandboy Layer 1) — only once an actually-untrusted
    target repo enters scope.

## Build (nix devShell)
`cargo build` (regenerates `Cargo.lock` — judge added `sha1`/`sha2`) →
`cargo fmt` → `nix flake check` → commit `Cargo.lock`.
