# Paper-transplant map — what `awesome-ai-agent-papers` is worth pulling into 007 / Own.NET

Status: design note · Scope: cross-repo (007 · Own.NET · Sandboy · OwnAudit) ·
Source: [`VoltAgent/awesome-ai-agent-papers`](https://github.com/VoltAgent/awesome-ai-agent-papers)

> **Read this next to the in-flight PRs, not instead of them.** The security /
> sandbox / policy slice of this analysis is *already* being written:
>
> - 007 **#9** `docs/zero-trust-framework.md` — the run/gate sandbox roadmap,
>   CUE-not-TOML policy authoring, WIT/WASM scoping, the tamper-evident
>   run-record **hash chain**, egress, supply-chain `.007/`, cross-repo
>   responsibility table.
> - Own.NET **#182** — the CUE `owen.policy` authoring decision (§8 addendum to
>   `agent-capability-layer.md`) + Sandboy pointer.
> - Own.NET **#181** — the `.agents/` memory & policy layer + `.007/gate.toml`
>   manifest contract.
>
> This note deliberately **does not re-tread** any of that. It covers the one
> slice none of those touch: the **eval / verification / orchestration**
> transplant, sourced explicitly from the paper list — which is also exactly
> this branch's scope.

## 0. Method (so the claims are checkable)

Grounded against the repos' own source of truth — READMEs, the two accepted
ADRs (`007/docs/security-layers.md`, `Own.NET/docs/notes/sandboy-isolation-adr.md`),
`AGENTS.execution-surfaces.md`, `agent-capability-layer.md`, the `audit/adapters/`
and `sandboy/` spikes — not the 363 paper PDFs. The awesome list is bucketed into
five categories (Multi-Agent · Memory & RAG · Eval & Observability · Agent
Tooling · AI Agent Security); the transplant candidates below cite papers by
their verbatim titles from it.

## 1. The correction that sets the priorities

The instinct "pull in more papers" is mostly wrong here, because **half the
obvious targets are already built as spikes or already in flight**. The transplant
that *remains* is narrow and it is not about the sandbox — it is about turning the
existing run record into something a verifier can read.

| Candidate | Papers | Real status |
|---|---|---|
| Sandboy in gate | Sandlock `arXiv:2605.26298` (already cited in the ADR) | **Spiked** — `Own.NET/sandboy/` (Landlock+seccomp wrap-the-child). Wiring roadmap in **#9**. |
| WASM SARIF adapters | STELP, MCP-SandboxScan (conceptual) | **Spiked** — `Own.NET/audit/adapters/` (zero-import WIT, fuel/epoch/mem caps, sha256 provenance). |
| `owen.policy` authoring | — (engineering) | **In flight** — CUE decision in Own.NET **#182**; not-built flag in `agent-capability-layer.md`. |
| Run-record integrity | — | **In flight** — hash chain in 007 **#9**. |
| **trace-driven `o7 eval`** | **TrajAD**, **TriCEGAR**, **Automated Structural Testing of LLM-Based Agents** | **Net-new.** ← the real transplant (§2). |
| **consensus judge** | **PerspectiveGap** | Deferred in `TODO.md`; **net-new** design (§2). |
| **evidence graph** | **Reliable Graph-RAG for Codebases: AST-Derived Graphs vs LLM-Extracted Knowledge Graphs** | **Net-new**, Own.NET (§3). |
| **claim-cards** | **JADE** | **Net-new**, Own.NET (§3). |

## 2. 007 — the transplant worth doing

### 2.1 `trace.jsonl` + `o7 eval` (the one large paper-transplant)

The run record already harvests `meta.json` / `diff.patch` / `agent.stdout` /
`gate/*.log`. What it does **not** have is a *normalized* action trace a verifier
can reason over, plus an explicit claim-vs-evidence ledger:

```text
runs/<target>/<run-id>/
  trace.jsonl          # normalized events: tool call, file touch, test step, net attempt
  claims.json          # what the agent asserts it did
  verification.json    # what diff/gate/logs actually confirm
  risk.json            # anomalies: unexplained action, missing evidence, flaky gate
```

- **TrajAD** — runtime trajectory anomaly detection with error *localization* and
  rollback/retry: the shape for `risk.json` + a future `--retry-from step:<id>`.
- **TriCEGAR** — trace-driven abstraction (predicate trees over the trace) for
  probabilistic runtime verification: the shape for turning `trace.jsonl` into
  checkable predicates rather than eyeballed stdout.
- **Automated Structural Testing of LLM-Based Agents** — OpenTelemetry-style trace
  events + automated assertions: the concrete event schema and assertion form.

**Delineation from #9 (important, not a duplicate):** #9's `record_hash` chain
answers *"was the record tampered with"* (integrity). This answers *"does the
record's story hold up against the diff and gates"* (semantic verification). They
compose — integrity under it, verification over it — and are different layers.

**Framing correction:** this is **verify-before-harvest**, not "-before-commit",
and it is **forensics / a gate, not a defense**. Per `security-layers.md`, *"a
deny is decoration if we call the tool before asking"* — the trace catches after
the fact; the boundary is Sandboy. Do not let the eval layer masquerade as the
sandbox.

### 2.2 Consensus judge v0 (PerspectiveGap)

`TODO.md` already defers "claude+codex race + cross-family judge". **PerspectiveGap**
(distributing context to sub-agents without information leakage) validates the
*independent-runs → deterministic merge* shape and warns off the failure mode:

```
o7 run --engine claude … ; o7 run --engine codex …
o7 judge --mode independent-merge --inputs run-a run-b \
         --criteria correctness,small-diff,test-pass,security
```

No "agents converse for 14 rounds." Independent runs, deterministic merge, and the
merge reads §2.1's `verification.json` as input — so this lands *after* the trace
layer, not before. Builds on the existing read-only `judge.rs`.

## 3. Own.NET — candidates, not commits (policy side already covered by #182/#181)

These are real and **not** in #182/#181 (which are policy/CUE and `.agents/`). But
per the "maybe nothing needs to go to Own.NET while the policy PRs are open" call,
they are recorded here as **candidate P-0NN proposals**, not opened yet:

- **Evidence graph** — *Reliable Graph-RAG for Codebases* (AST-derived beats
  LLM-extracted for code RAG): project the *existing* `diagnostics.Evidence` /
  OwnIR / CFG / lifetime facts into a deterministic symbol/ownership/lifetime
  graph the agent **queries** rather than one an LLM **extracts**. Hard constraint
  from `AGENTS.execution-surfaces.md`: **no second provenance type** ("Evidence2"
  is explicitly forbidden), and no premature Datalog — this is a projection of the
  canonical `Evidence`, nothing more.
- **Claim-cards** — **JADE** (decompose a response into individual claims, check
  each against expert knowledge): give every diagnostic a deterministic evidence
  card (claim → facts from the graph), not an LLM eval. Slots directly into the
  open evidence-coverage acceptance criteria in `execution-surfaces.md` §5.

## 4. What NOT to pull (confirmed against the repos' pain)

RL training of agents · long-running self-evolving multi-agent colonies · GUI/mobile
agent training · social-simulation · medical/hospital-workflow agents · "agents
open PRs autonomously." None closes the actual gap. The present pain is *"an agent
with a shell can do anything and then we trust stdout"* — a **trust-boundary** gap
(owned by Sandboy #9) and a **verification** gap (§2), not a creativity gap.

## 5. Priority, reconciled with what's already open

1. **`owen.policy` (CUE)** — highest daily ROI, but **already #182**. Nothing to do here.
2. **007 `trace.jsonl` + `o7 eval`** — the top *net-new* item. TrajAD / TriCEGAR / Structural-Testing.
3. **Wire Sandboy into gate** — spike done; roadmap **#9**. Finish, don't transplant.
4. **Consensus judge v0** — PerspectiveGap; after the trace layer.
5. **Own.NET evidence graph + JADE claim-cards** — candidate P-0NN (§3).
6. **WASM adapters harden** — adversarial corpus + build; spike done.

Net: after #9 / #182 / #181, the single paper-driven thing genuinely left to design
is **§2.1 — the trace-driven verifier over the run record.** Everything else is
already spiked, already in flight, or a deliberate no.

## 6. Placement

Canonical file: `007/docs/paper-transplant-map.md`. Lives in 007 because the
analysis is cross-repo and 007 is the private orchestration hub that drives the
others (per `README.md` and the #9 responsibility table). Own.NET-side items (§3)
stay here as candidates rather than seeding thin docs into Own.NET while its policy
PRs (#181/#182) are open.
