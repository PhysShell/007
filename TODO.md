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
- **A1-F contract freeze**: **ACCEPTED / CLOSED / FROZEN**. Accepted at
  `b61540a` after six corrective rounds (R1–R5.1), then **amended pre-merge by
  R5.2** (`a4a9f97`), which closed four P1s from external review on PR #123 —
  including one that let an ambiguous provider execution free the dispatch slot,
  and one that charged only half an artifact's bytes against the closure budget.
  **The frozen baseline is the merged head of PR #123, not `b61540a`**: an
  implementation built against `b61540a` would be building the pre-R5.2 contract.
  Normative source: `docs/q-deck/a1-authority-contracts.md` (§9 carries all seven
  rounds). Fifteen frozen decisions — artifact model / digests / per-object and closure bounds /
  versions / the complete `ArtifactKindV1` set (FD-1), acyclic evidence-graph
  rank rule with imported authority roots and resolver duties (FD-2),
  raw-vs-normalized evidence (FD-3), untrusted/accepted split (FD-4), authority
  direction (FD-5), duplicate-id replay-vs-conflict (FD-6), no model-supplied
  executable authority (FD-7), no-provider-call-on-replay (FD-8), fail-closed
  post-dispatch ambiguity (FD-9), the `provider_execution_id`/`dispatch_id`
  grain split on an execution-level receipt (FD-10), twelve receipt/artifact
  congruence predicates (FD-11), transition authority (FD-12), head-bound
  evidence (FD-13), `CampaignStateV1` + `verify_wire`/`resolve_genesis`/
  `resolve_event`/`fold` + the `state_version` rule + the `HUMAN_REQUIRED` exit
  rule (FD-14), and human command binding with honest actor attestation (FD-15).
  Complete wire schemas for all eleven message kinds, the provider execution
  receipt, interaction manifest, `ScopeContractV1`, `CampaignStateV1`, and
  `CampaignEventV1` with its eleven payload schemas. Design input: issue #95.
- **A1-F S1** (first §7 supersede, PR #126, merged `288cf84`): FD-1.4 had
  classified `InteractionManifestV1` under both the 1 MiB typed-object bound and
  the 64 MiB "manifest" bound. Decision: **64 MiB**; it stays a typed A1 object
  for FD-1.7 media types and its FD-2 rank. No version gate fired — the blob and
  therefore `contract_digest` changed, and nothing else did.
- **A1-F S2** (second §7 supersede, PR #135, merged `ae720c1`): FD-1.3 said
  nothing about a JSON object carrying the same member name twice, and RFC 8259
  leaves the behaviour undefined, so first-wins and last-wins were both
  conforming readings of identical bytes — reopening the "semantically
  identical, digest-unequal" class FD-1.2 exists to close. Decision: **member
  names are unique within every A1 JSON object**, a duplicate is rejected at
  parse time, no first-wins or last-wins reading is permitted. Two §5.4 rows
  added, the nested case required explicitly. No version gate fired again, so
  **the blob is the only thing distinguishing `B1` from `B2`** — an
  implementation bound to a version number would not see S2 at all, while S2
  changes the set of documents the contract admits.

  ```text
  B2  e22539ddf4f7c9ab260e16835eef8ef18abbe726   current authority
      sha256:2de682894cd2084444b5d7d6c5db8807a80f08a3d41541e9c00caf92462d1e1a
  B1  3b26849cc39a3391aaed46cca56be3b6715afabb   superseded by S2
  B0  7db92f1b3dc9d7040da074956a0b3f2f200174c8   superseded by S1
  ```
- **A1-F v2 convergence, Phase G — `FD-v2-GRAPH`: APPROVED / CLOSED** at
  `b853a2e`, ceremony commit `1b82a88`, after twelve corrective rounds.
  Normative source: `docs/tasks/a1-f-v2-phase-g.md`; ledger
  `docs/tasks/a1-f-v2-convergence.md`. Adjudicated contract-first against blob
  `3b26849`, the authority at that time (post-S1; the S1 graph delta was
  *proved* NONE, not taken from S1's own summary). S2 has since superseded that
  blob and Phase G has **not** been re-adjudicated against `B2`: S2 changes an
  admission rule and touches no kind, rank, edge or payload variant, so a graph
  delta is not expected — but "not expected" is not "proved NONE", which is the
  standard Phase G held itself to for S1. Recorded as owed, not as done. The
  frozen result: a **semantic edge
  registry of 69 exact edges** — 56 `Intra` / 13 `Causal`, exact source and
  target kinds, discriminated by event kind and payload variant — over a
  **47-node typed universe**, acyclicity machine-checked (Kahn 47/47 over 26
  `Intra` typed→typed edges). Baseline is a **41-slot** frozen `ArtifactRef`
  inventory by recursive extraction (a flat pass missed a slot reachable only
  through `as §3.5`), split **32 exact / 9 open**, under the rule that a generic
  `ArtifactRef` field creates no graph authority. Eleven envelope-bearing
  message kinds, unchanged (`KEEP_V1_MODEL`); `CampaignRunBinding` added as
  typed support authority with pre-dispatch binding admission; `ArtifactImported`
  out of V0. Terminals are 20 kinds plus **one open meta-target**
  (`AnyCommittedEnvelope` = the eleven FD-1.9 message kinds), which resolves
  through and is charged against the FD-1.5 budget instead of terminating
  traversal. Uniqueness is **per-field occurrence**; global `(source, target)`
  uniqueness was **rejected** as an invariant, since the design input already
  carries one pair under two classes. The one `Causal` edge with no event-log
  source is witnessed by **`COMMITTED`** over the accepted canonical log prefix —
  replay-checkable, no clock, no new reducer policy. Owed to the v2 draft:
  field-path spelling, plus **one machine-checked wire realization ledger**
  (event-kind universe equality → the 11/10 payload presence map →
  forward/reverse carrier coverage) as the registry↔wire joint.
- **Next in the v2 convergence: Envelope v2.** The ledger's disposition columns
  (47 frozen decisions + 141 §5.4 rows) remain open — Phase G closed the graph,
  not the migration.
- **A1-V0 — gated behind the v2 freeze, not the immediate next step.** The
  sanctioned line is recorded in the convergence ledger §7 and is
  `INVENTORY ADMITTED → Phase G → v2 convergence → freeze → A1-V0`; PR #124 was
  reclassified an implementation probe for exactly this reason (it implements
  v1 while the sanctioned line runs through v2). So A1-V0 is **not** parallel
  work and **not** started against v1 — it is the acceptance milestone *after*
  the v2 convergence freezes. Task: `docs/tasks/a1-v0.md` (§5 of the contract),
  bound to contract blob `e22539dd` (post-S2, PR #135), not to a branch head;
  that binding moves when v2 freezes. Steps 1 and 2 have merged (PR #124,
  PR #132) and both crates carry that blob in their headers; the rebind from
  `B1` happened in `85ac63c`, after S2 merged and not before, because a blob
  does not exist to bind to until it does. One
  real coder/reviewer/human corrective loop — coder on the claude CLI, reviewer
  on `--engine arliai` (read-only, no tool surface), controller sealing and
  folding, merge manual. Acceptance = one live corrective cycle + a full campaign
  replay with zero provider invocations + the negative matrix of §5.4.
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
