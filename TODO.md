# 007 — resume here

Where we stopped + the exact next step. Updated 2026-07-02 (leaving for the day);
2026-07-26: verdict soundness + the o7-run stitch landed (see below).

## A-series status (reconciled 2026-08-04)

The sections below are preserved as history; they predate the A-series.
Current authoritative state:

- **R1** (durable follow-up Command): accepted/merged, PR #90.
- **A0.0 contract**: completed contract-first — commit `71800fc`, the first
  commit of PR #92, frozen before any implementation.
- **A0 candidate-state continuity**: accepted at `52627c3`, merged as
  `f1ac458` (PR #92, eight forward-only corrective rounds). Normative
  source: `docs/q-deck/a0-candidate-state.md`.
- **A1-F contract freeze**: `PROPOSED FREEZE / REVIEW REQUIRED /
  NON-AUTHORITATIVE` — `docs/q-deck/a1-authority-contracts.md` (branch
  `claude/a1-contract-freeze-dkrnnq`, corrective round R1 applied). Fifteen
  frozen decisions: artifact model / digests / per-object and closure bounds
  / versions (FD-1), acyclic evidence-graph rank rule with imported
  authority roots and resolver duties (FD-2), raw-vs-normalized evidence
  (FD-3), untrusted/accepted split (FD-4), authority direction incl. the
  `created_at`/`first_observed_at` rule (FD-5), duplicate-id
  replay-vs-conflict (FD-6), no model-supplied executable authority (FD-7),
  no-provider-call-on-replay (FD-8), fail-closed post-dispatch ambiguity
  (FD-9), the `provider_execution_id`/`dispatch_id` grain split carried by
  an execution-level receipt (FD-10), receipt/artifact congruence (FD-11),
  transition authority (FD-12), head-bound evidence (FD-13), `CampaignStateV1`
  + the pure V0 fold + the `state_version` rule (FD-14), and human command
  binding with honest actor attestation (FD-15). Complete wire schemas for
  all eleven message kinds plus receipt, manifest, `ScopeContractV1`, and
  `CampaignStateV1`. Design input: issue #95. **A1 implementation begins only
  after this freeze is accepted** — then A1-V0 (§5): one real
  coder/reviewer/human corrective loop, coder on the claude CLI, reviewer on
  `--engine arliai` (read-only, no tool surface), merge manual.
- **Post-A0 hardening**: separate follow-up issue; does not reopen A0.
- **B1** (`research/b1-context/`): parallel, read-only, non-authoritative.

## Built & working
- **`o7 run`** — one isolated gated agent run (WSL worktree → full-auto claude →
  gate manifest → harvest). Scaffolded; not yet exercised on a real coding task.
- **Verdict soundness (2026-07-26):** the false green is dead — a skipped
  required gate (`env = "windows"`) scores `BLOCKED`, never `PASS`; the only
  legitimate skip is a pre-declared `waive_reason` on the manifest step (the
  o7-run `GateApplicability::Waived` doctrine, blank reason rejected). The
  step's declared `required` is preserved in the record.
- **The MVP ↔ o7-run stitch (2026-07-26, `src/events.rs`):** `o7 run` now
  declares its obligation contract up front, records a digest-chained
  `events.jsonl` in every run record, and takes its verdict from the
  canonical `o7-run` reducer (so a non-clean agent exit is `ERROR` even with
  green gates). New **`o7 replay <run-dir>`** independently re-verifies a
  stored record: chain continuity, per-event digests, artifact content
  digests (task/diff/gate logs), and a verdict recomputation against
  `meta.json`. Root-package tests now run in the hosted gate.
  Deferred to the next slice, on purpose: the `o7-ledger` append path and
  the `o7-ledger`/`o7-run` id unification.
  **Q-Deck R0.6 (`docs/q-deck/r06-verdict-fidelity.md`)** closed one
  concrete piece of that gap: `o7-ledger`'s `RunStatus` now has sealed
  `Blocked`/`Error` matching `o7-run::Verdict`'s own `Blocked`/`Error`
  (previously there was no ledger status for either, so a live producer
  couldn't have projected every sealed verdict without collapsing
  meaning). Vocabulary alignment only — the actual append-path wiring and
  id unification above remain the next slice, not R1 mutations.
  **Q-Deck R0.7 (`docs/q-deck/r07-live-ingress.md`)** did that next
  slice: `o7 run --ledger <path>` now projects its canonical events into
  `o7-ledger` live, per event, as the run executes (new
  `LiveLedgerProjector`); the id unification above is done too
  (`create_run_with_id`, sharing the canonical `RunId` verbatim). No
  ledger without the flag remains byte/semantics-unchanged. Real
  process-level acceptance (`tests/live_ingress_e2e.rs`): actual `o7 run`
  + `o7d` processes, REST/SSE visible before completion, daemon restart,
  replay agreement, idempotent recovery retry, and a real SIGKILL
  interruption proving `Interrupted`, never `Error`.
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
11. Own.NET evidence coverage for flow diagnostics (parallel effort, tracked
    in Own.NET's `docs/tasks/evidence-coverage.md` — mirrored to keep §16
    numbering aligned).
12. Firecracker/gVisor (Sandboy Layer 1) — only once an actually-untrusted
    target repo enters scope.

## Build (nix devShell)
`cargo build` (regenerates `Cargo.lock` — judge added `sha1`/`sha2`) →
`cargo fmt` → `nix flake check` → commit `Cargo.lock`.
