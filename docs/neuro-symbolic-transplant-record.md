# Transplant record — Vargas, *Semantic Cognition Matrix* (2026-07-24)

Status: transplant record (analysis, no decision) · Scope: 007 · Companion to
[`docs/paper-transplant-map.md`](paper-transplant-map.md)

One external architecture was read against 007's landed doctrine. This file is the
verdict table, not an essay: **almost nothing transplants, one thing does, and the
paper's real value to us is as a failure case.** The one net-new item has its own
design record — [`docs/invariant-registry.md`](invariant-registry.md).

## Source

Federico Vargas, *"The Semantic Cognition Matrix: A Metabolic Neuro-Symbolic
Architecture for Safe, Autonomous, and Synthetic Intelligence"*, independent
preprint, 2026-07-24, 8 pp. Architecture: a typed knowledge graph (NetworkX
in-memory, optional Neo4j) + an "Ontological Nucleus" of immutable axioms + a
"Critical Cortex" validator that refuses graph writes violating them, with R-GCN/GAT
embeddings as a supplementary layer and a desktop-agent shell on top (NOEMA OS →
SAMANTHA-OS).

Extraction caveat, per rule 4: the PDF uses per-page subset fonts with private
glyph orderings, so quoted figures below were re-derived from the document's own
generation-summary table and cross-checked against the abstract's prose. Anything
not independently re-extracted is **not** cited here.

## Verdict

### ALREADY STRONGER IN 007 — do not transplant

| Paper's idea | What 007 already has, and why it is stronger |
|---|---|
| Immutable invariant kernel (axioms the agent may not edit) | The paper's axioms are runtime data in the same process that mutates the graph. 007 puts a share of its invariants where the agent cannot reach them at all: the compile-enforced lint set in `Cargo.toml` (`unsafe_code = "forbid"`, `unwrap_used`, `panic`, `indexing_slicing`), and `crates/o7-harness-policy`, whose feature boundary is proved by a **negative compilation** (`--features probe-leak` must fail to build; CI asserts the failure *and* that it names the absent symbols). |
| Critical Cortex (validator gates writes) | `crates/o7-verifier` separates the two things the Cortex conflates: `evidence.rs` records what a run observed and **is never a verdict**, with every abnormal outcome a distinct non-completion that can never be a pass; `trust.rs` binds trust to a content hash of the executable plus a digest over the whole command specification, and is never sourced from the repository. |
| Provenance in the data model | `docs/agent-memory-layer.md` already requires provenance on every entry and grades it (`agent-claimed` / `artifact-derived` / `gate-derived` / `analyzer-derived` / `human-confirmed` / `superseded` / `rejected`), with `agent-claimed` barred from authoritative context. `docs/decision-and-admission-protocol.md` adds a closed epistemic algebra (`ESTABLISHED` / `REFUTED` / `UNKNOWN` / `CONFLICTING` / `STALE` / `UNAVAILABLE` / `OUT_OF_SCOPE`). Mechanically: the digest-chained `events.jsonl` and `o7 replay`. |
| "Functional self-awareness" | The paper's own example is telemetry — the graph gained a node, so the system reports N+1. 007's equivalent is the obligation contract `o7 run` declares up front (`src/events.rs`) plus ledger/Q-Deck projections, which are projections of canonical state and explicitly *not* alternate authorities (`docs/autonomy-controller.md`). |

### REGRESSION — actively rejected, already adjudicated on `main`

- **`validate-before-commit` as `proposal → checks → authorization → commit`.**
  That is steps 1–4 of `docs/evidence-and-decision-discipline.md` rule 2 with the
  load-bearing step missing. Without step 5 — an **atomic conditional mutation that
  consumes the precondition identity the decision was made against**
  (`merge(sha = accepted_head)`) — it is disciplined check-then-act, and TOCTOU
  eats disciplined checks for breakfast.
- **Runtime axioms as the sole safety boundary.** `docs/security-layers.md`:
  a decision point is not an enforcement point, and *"a deny is decoration if we
  call the tool before asking."* `docs/paper-transplant-map.md` §2.1 already
  forbids the adjacent confusion — trace/eval is **verify-before-harvest**,
  forensics and a gate, never the sandbox.
- **Safety as pattern matching.** The Cortex blocks writes by matching lowercased
  triples against a set of 12 malicious patterns. A blacklist is a heuristic, not
  a boundary.
- **The replacement thesis** (knowledge graph + Horn clauses supplant LLM
  reasoning). The paper's own neural layer reports an MRR too low to carry weight,
  and its future-work section concedes "integrate with LLMs as QA modules". If
  graph-shaped memory is ever wanted here, the already-analysed candidate is
  `docs/omnigraph.md` (server-side Cedar on every mutation, server-determined
  actor, branch/review/merge, graph+vector+FTS with RRF) — not NetworkX.

### NET-NEW — the one real delta

- **Stable invariant IDs + executable coverage vectors.** The paper's one genuine
  hygiene win is that `AX-CORE-02` is an *addressable object*, so coverage can be
  counted per invariant rather than per line. 007 has proptest/fuzz/Kani
  (`docs/verification.md`) and a "shared positive/negative semantic conformance
  corpus" (rule 1), but that corpus is scoped to projection-bound contracts and
  deferred to MG-C+; today nothing can answer *"which normative invariant has no
  executable negative witness?"* Design record: [`docs/invariant-registry.md`](invariant-registry.md).

### CODIFICATION OPPORTUNITY — ours, not the paper's

A `GroundedClaim` **envelope** over the types that already exist — not one type
that absorbs them. The distinctions 007 paid for must survive: evidence is not a
verdict, a raw signal is not a semantic fact, trust basis is not epistemic status.

```rust
struct GroundedClaim {
    id: ClaimId,
    proposition: Proposition,

    grounding: GroundingRef,        // -> o7_verifier::evidence
    trust: TrustBasis,              // -> o7_verifier::trust  (how it was obtained)
    epistemic_status: EpistemicStatus, // ESTABLISHED / REFUTED / ... (what it means)

    authority: AuthorityRef,
    revision: RevisionBinding,      // rule 4: the claim is STALE when this moves

    derived_by: Option<DerivationRef>,
}
```

One canonical envelope, not one god-object: the failure mode being designed
against is a `TrustBasis` quietly promoting itself into an `EpistemicStatus`.
Recorded as an opportunity — **no code, and not scheduled**; the first real
consumer picks it up.

### EXTERNAL FAILURE EVIDENCE — the paper's actual contribution to us

Cited, with its framing, in
[`docs/evidence-and-decision-discipline.md`](evidence-and-decision-discipline.md).
Summary of what the paper reports about its own Generation 1: axioms existed in
natural language only, were *not computationally enforced*, and safety-violation
coverage was **0 %** — while relation-type validation was never implemented, invalid
edges could persist rather than be rejected, and one axiom's safety patterns were
not checked at all. Its own summary table records axiom coverage climbing 0 % →
28 % → 100 % across generations.

```text
declared invariant      ≠  executed invariant
executed check          ≠  enforcement boundary
passing regression set  ≠  safety proof
```

That is an independent instance of the failure class 007's rules were derived
against — evidence of recurrence, never authority for the rules.

## Disposition

```text
Immutable invariant kernel        ALREADY STRONGER  — no action
Critical Cortex / validator       ALREADY STRONGER  — no action
Provenance in the data model      ALREADY STRONGER  — no action
Functional self-awareness         ALREADY STRONGER  — no action
validate-before-commit (4-step)   REGRESSION        — rejected (rule 2, step 5)
Runtime axioms as the boundary    REGRESSION        — rejected (security-layers.md)
Graph/Horn replacement thesis     REGRESSION        — rejected (omnigraph.md if ever)
Stable invariant IDs + vectors    NET-NEW           — docs/invariant-registry.md
GroundedClaim envelope            CODIFICATION      — opportunity, unscheduled
Gen-1 enforcement gap             FAILURE EVIDENCE  — cited, non-normative
```

No code follows from this document.
