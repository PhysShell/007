# Evokoa transplant map — what the Evokoa stack is worth pulling into 007 / qodec

Status: design note · Scope: cross-repo (007 · qodec) ·
Source: [`Evokoa`](https://github.com/Evokoa) — `pgGraph`, `pgContext`,
`polygres-skills`, `polygres-sdk`, `polygres-cli`, `homebrew-tap`

> **Read this next to the landed design docs, not instead of them.** Three of
> the four patterns an initial reading of Evokoa suggests are *already* written
> down here, and more strictly than the source states them:
>
> - `docs/autonomy-controller.md` — transition authority, evidence guards,
>   `HUMAN_REQUIRED`, escalation triggers, and the explicit rule that
>   `READY_TO_MERGE` does not authorize an unconditional merge.
> - `docs/evidence-and-decision-discipline.md` — rule 4 (revision-bound
>   artifact grounding) and the `raw → classifier → typed → policy` constraint.
> - `docs/task-aware-context-generator.md` §Context IR / §Determinism —
>   provenance-backed context packs with `source_hash`, ranking version, and
>   recorded selection reasons.
>
> This note covers the narrow remainder: **one genuine gap in the context
> generator (§3.1)** and **one missing named contract (§3.2)**. Everything else
> is either already landed or a deliberate no.

## 0. Method (so the claims are checkable)

Per `docs/evidence-and-decision-discipline.md` rule 4, external artifacts are
not captured by this repository's commit, so each carries its own version
anchor. All Evokoa claims below were read **2026-08-06** from the repositories'
own `README.md` at their then-current default branch, with release anchors:

```text
pgGraph          v1.0.0   released 2026-07-25
pgContext        v0.2.0   released 2026-07-23
polygres-skills  no release; README read 2026-08-06
```

These claims are **stale once those artifacts change** and must be re-checked
before being cited as evidence. What follows separates *artifact says* from
*inference* wherever the two diverge.

## 1. The correction that sets the priorities

The instinct "Evokoa has three patterns worth transplanting" is mostly wrong
here, because **007 already states the two governance patterns, and states them
harder than the source does**. What remains is one narrow *freshness* gap in the
context generator, plus a naming gap.

| Candidate | Evokoa source | Real status here |
|---|---|---|
| Capability-separated agents (plan / operate / diagnose) | `polygres-skills` | **Landed, stronger.** `docs/autonomy-controller.md` binds transitions to durable evidence and escalates to `HUMAN_REQUIRED`; polygres-skills states approval as prose guidance to a model. |
| Approximate accelerator, authoritative truth | `pgContext` | **Landed in qodec** (§4); **partially landed in 007** — the gap is §3.1. |
| Authoritative state + derived rebuildable index | `pgGraph` | **Landed as doctrine** — `docs/agent-memory-layer.md` ("index and recall layer over trusted run artifacts, not a replacement"). The *storage* technique is a deliberate no (§5). |
| **Freshness ≠ reproducibility in the context pack** | `pgContext` | **Net-new.** ← the real transplant (§3.1). |
| **`diagnose` ≠ `repair` as a named contract** | `polygres-skills` | **Net-new**, cheap, no code (§3.2). |

## 2. What verified, and what an initial reading overstated

### 2.1 Verified against the artifacts

- **pgGraph** — README says: "Your tables stay the source of truth. pgGraph
  builds a derived graph index and lets you query it from SQL", and "pgGraph is
  strictly derived state". Forward *and* reverse CSR edge stores, atomically
  written `.pggraph` artifacts, and read-only mapping so "the operating system
  page cache can then share those physical pages across isolated PostgreSQL
  backends" are all stated. Traversal bounds are stated as explicit circuit
  breakers: "depth limits, visited-node tracking, frontier limits, pagination,
  and strict OOM/memory safeguards".
- **pgContext** — README says, verbatim: "Exact search is the correctness
  oracle." Also: "Every candidate is resolved back to the live source row,
  checked against PostgreSQL visibility and filters, and scored exactly before
  it is returned", under "PostgreSQL MVCC visibility, ACL/RLS, and SQL
  predicates". Dense + PostgreSQL full-text hybrid with reciprocal-rank fusion
  is stated as stable.
- **polygres-skills** — four skills; the design skill produces "plans without
  mutating a project"; "Replacement imports, migrations, revocations, deletes,
  and schema mutations require explicit approval"; the troubleshooting skill
  "does not use private observability or perform repair mutations".

### 2.2 Three claims that do not survive contact with the artifacts

These matter because each would otherwise enter this repo as an unbound
citation — precisely the failure `docs/evidence-and-decision-discipline.md`
rule 4 exists to stop.

1. **"pgGraph pairs an immutable CSR base with small mutable overlays for
   fresh changes."** *Artifact says:* the README exposes a `mutable` mode in a
   quickstart invocation. *It does not describe* an overlay, delta layer, or
   incremental-update mechanism over the immutable base. The overlay
   architecture is **inference, not artifact** — do not cite it as pgGraph's
   design.

2. **"pgContext is 3.8–5.3× faster than pgvector."** *Artifact says* exactly
   that, for GloVe-100-angular (1.18M vectors, cosine), e.g. "0.910 recall@10 at
   2.4 ms versus 13.0 ms". *The same README also says* measured **x86**
   performance claims remain roadmap items. So the figure is self-published
   **and architecture-scoped**, not a general result. Treat as a result to
   reproduce, not a property of the design.

3. **"pgGraph's benchmark numbers…"** — the pgGraph README carries **no**
   performance numbers at all. A separate benchmark-data release (ICIJ Offshore
   Leaks snapshot, 2026-08-02) exists; the README does not summarize it.

Additional scope limits worth recording before anyone proposes a dependency:
pgContext is PostgreSQL **17 and 18** only, lists "Drop-in pgvector
compatibility" as **No**, IVFFlat as **not implemented**, and `halfvec` /
`sparsevec` / `bitvec` as **"Partial, experimental"** — at v0.2.0. pgGraph
v1.0.0 states PostgreSQL 14 through 18.

## 3. 007 — the transplant worth doing

### 3.1 Freshness is not reproducibility (`docs/task-aware-context-generator.md`)

This is the one substantive gap, and pgContext's ordering is what makes it
visible.

*Artifact says* (`docs/task-aware-context-generator.md`, this repo, §Selection
pipeline / §Determinism and reproducibility): the pipeline is seeds → "Retrieve
exact matches" → structural expansion over typed edges → historical evidence →
budget → render. The cache key binds `repository commit` alongside task hash,
profile/extractor/ranking versions and budget configuration; `context.meta.json`
records `dirty-worktree status` and `omitted candidates and omission reasons`.

*Inference:* the commit in the cache key makes a pack reproducible **and**
commit-keyed — so the naive form of this criticism ("the pack isn't tied to a
revision") is wrong and should not be repeated. The gap is narrower and
survives anyway, in three places:

- **historical evidence is cross-revision by construction** — step 4 pulls from
  `o7 memory` / Omnigraph, whose entities are bound to *their* revisions, not
  this one;
- **`dirty-worktree status` is recorded, not gated** — a dirty tree means the
  keyed commit does not describe the bytes on disk;
- **extractor output can outlive its input** within such a tree.

Reproducibility binds *pack → inputs*; freshness binds *pack → the tree the
agent will edit*. The doc names the first and has the ingredients for the
second, but no step that spends them.

*Decision (proposed, not ratified — rule 3):* the context generator should
adopt pgContext's ordering explicitly — **an accelerator may only propose; the
authoritative revision decides.**

```text
recall stage  (index, typed edges, o7 memory, ranking)
        │      may be stale, approximate, or heuristic
        ▼
candidate set
        │      re-resolve each span against the working-tree revision
        │      the run will actually execute against
        ▼
admitted context entry
        │  carries: source revision, content digest at that revision,
        │           resolution outcome, which recall channel proposed it
        ▼
budget + render
```

Two constraints that follow, both cheap:

- A candidate that fails re-resolution (file gone, span moved, digest
  mismatch) is **dropped with a recorded reason**, not silently rendered. A
  context pack that quietly ships a stale span is the context-layer form of the
  false green that `TODO.md` records as already killed for gate verdicts.
- Re-resolution must be **batched over the revision**, not one authority call
  per candidate. pgContext's per-candidate resolution is cheap because index and
  resolver share a page cache; a per-candidate filesystem read plus rehash here
  would make verification cost more than the recall it verifies.

Delineation from existing docs, so this is not a duplicate: `§Determinism`
answers *"can this pack be rebuilt identically"*; this answers *"does this pack
still describe the tree the agent will edit"*. They compose — determinism under
it, freshness over it — and are different layers.

### 3.2 Name the `diagnose` / `repair` boundary

`polygres-skills` separates read-only diagnosis from repair as a first-class
contract: the troubleshooting skill "does not use private observability or
perform repair mutations", and repair requires separate approval.

007 has the *machinery* for this (`docs/autonomy-controller.md`'s
`HUMAN_REQUIRED`, `docs/security-layers.md`, the sandbox boundary) but does not
name the discipline anywhere an agent will read it: `AGENTS.md` does not state
it, and `diagnose` appears in only two documents, neither normatively.

The transplant is the contract, not the Polygres content — four lines, no code:

```text
diagnosing is not repairing            a diagnostic run may not mutate
a likely boundary is not a root cause  name the evidence or say "unknown"
missing evidence is not a passed check absent signal ≠ negative result
a first page is not the whole result   partial success is not success
```

Line 3 is the same failure class as the verdict-soundness rule already enforced
in code (a skipped required gate scores `BLOCKED`, never `PASS` — `TODO.md`,
2026-07-26). Line 4 is the pagination case of it. Stating them once as agent-
facing prose costs nothing and closes the gap between what the harness enforces
and what the driven agent is told.

## 4. qodec — already landed, nothing net-new

pgContext's "correctness oracle" is **already qodec's constitution**, in a
different domain and arguably in a stricter form.

*Artifact says* (`PhysShell/qodec` `README.md`, read 2026-08-06):

- "Rules the lab lives by" #2: "**Measured, not modeled.** A dictionary entry is
  committed only if re-tokenizing the actual replacement beats the legend line
  it adds."
- On the rules verifier: "proposals are never trusted, only measured (byte-exact
  inversion on every file touched, strict token win overall)" — "in the first
  live run it kept 1 of 3 rules this session's own proposer drafted."
- On the learned ranker: "The ranker never decides — acceptance stays measured —
  so a wrong model wastes probes, never bytes."
- On staleness of the cached accelerator: the artifact pins the legend file's
  checksum (`%q1 ext sum=…`) and "decode without the exact file fails closed
  instead of reconstructing wrong bytes."
- "Fallback is a feature" — a codec that cannot win returns `raw`.
- `qodec risk` is "Diagnostic, not a gate — it flags, the measured A/B stands
  decide" — §3.2's boundary, already correctly drawn.

*Inference:* map that onto pgContext and the correspondence is exact —
profile / ranker / proposer are the approximate recall stage; byte-exact
roundtrip plus strict measured token win is the correctness oracle; checksum
pinning with fail-closed decode is the staleness check on the cached
accelerator.

*Decision:* **no transplant, and no new document in qodec.** The pattern is
implemented, measured, and documented where it is enforced. Seeding a thin
design note restating it would add a second, weaker statement of a rule that
already lives next to its tests — the same reason §6 of
`docs/paper-transplant-map.md` declined to seed thin docs into Own.NET.

One naming note, recorded and nothing more: qodec independently arrived at
pgContext's discipline. If a shared vocabulary is ever wanted across repos,
qodec's "measured, not modeled" is the better name — it says what the rule
requires, where "correctness oracle" only says what it is.

## 5. What NOT to pull

- **CSR / mmap graph storage (pgGraph).** The technique is sound and the
  bounds discipline is admirable, but nothing here has demonstrated a traversal
  bottleneck to justify it. 007's present retrieval is typed-edge expansion over
  a repository-scale corpus; `git log` plus deterministic extractors beat a
  derived artifact that must then be invalidated, versioned, and proven fresh.
  Revisit only with a measured traversal cost, per `docs/performance.md`'s rule
  that 007 is subprocess-bound.
- **pgContext as a dependency.** PostgreSQL 17/18 only, no pgvector drop-in,
  IVFFlat absent, three vector types experimental, at v0.2.0. Adopt the
  *ordering* (§3.1), not the extension.
- **polygres-sdk.** A narrow versioned Runtime API that withholds the database
  password is a good shape, but it is not a capability system: the API key is
  still a broad client-side credential. `docs/zero-trust-framework.md` and
  `docs/architecture/capability-fd-transport.md` already aim past it. The
  commercial coupling to Polygres makes direct use unattractive regardless.
- **polygres-cli, homebrew-tap.** Packaging and distribution. Nothing to learn.
- **Skill *content*.** Everything Polygres-specific. What transfers is the
  authority split, not the operational knowledge.

## 6. Priority, reconciled with what is already landed

1. **§3.2 — name the `diagnose`/`repair` boundary.** Cheapest, no code, and it
   closes a stated gap between harness enforcement and agent-facing prose.
2. **§3.1 — freshness in the context generator.** The one net-new design item.
   Lands as an edit to `docs/task-aware-context-generator.md`, and only reaches
   code when `o7 context` does.
3. **Everything else — no.** Recorded above with the reason.

Net: after `docs/autonomy-controller.md`,
`docs/evidence-and-decision-discipline.md`, and
`docs/task-aware-context-generator.md`, the single Evokoa-driven thing genuinely
left to design is **§3.1 — re-resolution against the authoritative revision
before a candidate enters the context pack.** The governance patterns are
already here and already stricter.

## 7. Placement

Canonical file: `007/docs/evokoa-transplant-map.md`, alongside
`docs/paper-transplant-map.md`, whose structure and cross-repo rationale this
note follows. It lives in 007 because the analysis spans 007 and qodec and 007
is the orchestration hub. Per §4, qodec receives no file from this analysis.
