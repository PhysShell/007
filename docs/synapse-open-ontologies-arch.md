# Synapse Engine and Open Ontologies — architecture decision for 007

Status: **accepted architecture decision** · Scope: the memory/context layer
described in [`agent-memory-layer.md`](agent-memory-layer.md) · Companion to
[`evidence-and-decision-discipline.md`](evidence-and-decision-discipline.md)
(rule 4 governs every factual claim below).

Two external projects were evaluated as candidate foundations for 007's memory
layer: `synapse-engine` (Markdown → RDF agent memory) and `open-ontologies`
(ontology engineering toolkit). This document records what they actually
provide, what 007 takes from them, what it refuses, and the conditions under
which the refusal must be revisited.

Per rule 4, "the artifact says X" is kept separate from "we infer Y" and "we
decide Z" throughout. Every external claim binds the commit in
[§10](#10-revision-bound-evidence).

## 1. Question being decided

Not "which of these two repositories is better". The load-bearing question is:

> Does 007's memory layer need an RDF/OWL representation and a description-logic
> reasoner, or is a typed relational model with recursive queries sufficient?

Everything else — Markdown parsing strategy, MCP surface, drift detection — is
downstream of that answer, and answering it first prevents adopting a
representation because it looks rigorous rather than because a query needs it.

## 2. Existing 007 invariants

These are already fixed by [`agent-memory-layer.md`](agent-memory-layer.md) and
are not reopened here. Any candidate architecture that contradicts them is
rejected on that ground alone.

```text
I1  Memory is derived from artifacts. Artifacts are not derived from memory.
I2  007 writes memory. The agent does not write trusted memory.
I3  Every memory entry carries provenance.
I4  Canonical truth lives in runs/<target>/<run-id>/ —
    task.md, meta.json, diff.patch, gate/<name>.log, gate/verdict.json.
I5  Rendered Markdown (context.md, task.md, digests) is a projection of memory,
    never a source for it.
```

`I1` and `I5` are the ones an RDF-memory design tends to violate quietly, which
is the subject of [§4](#4-facts-versus-claims).

## 3. What Synapse and Open Ontologies actually provide

### 3.1 Synapse Engine

*Artifact says.* At the pinned commit, `crates/semantic-engine` indexes
Markdown into an RDF graph: YAML frontmatter becomes properties, `[[WikiLinks]]`
become relations, documents and blocks get PROV-O provenance, and eight MCP
tools are registered in `crates/semantic-engine/src/mcp/tools.rs`
(`get_entity_narrative`, `get_domain_logic`, `get_dependency_impact`,
`sync_specification_to_graph`, `get_entity_neighborhood`,
`index_markdown_directory`, `sparql_query`, `get_provenance`). The README at the
same commit documents three of them (lines 44-46).

*Artifact says.* `crates/semantic-engine/src/md_sync/parser.rs:64-68` derives
each block identifier by hashing three inputs together:

```rust
block_hasher.update(p_trim.as_bytes());       // block content
block_hasher.update(file_hash.as_bytes());    // hash of the WHOLE file
block_hasher.update(i.to_string().as_bytes()); // block ordinal
```

*Inference.* Because `file_hash` covers the whole file, editing any paragraph
changes the identifier of **every** block in that document, including untouched
ones; inserting a paragraph additionally shifts every subsequent ordinal. The
provenance is granular in shape but not in behaviour — a one-line edit
invalidates the document's entire anchor set, so "which block changed" is not
answerable from identifiers alone.

*Artifact says.* `src/mcp/tools.rs:49` describes `sync_specification_to_graph`
as granting a file "elevated truth status", and `src/ingest/mod.rs:56` implements
this by injecting the role string `CoreSpecification` at ingest.

*Inference.* The elevation is a label applied at ingest time with no admission
policy, no ratification record, and no invalidation rule. It changes how the
graph presents a statement without changing what evidence backs it.

*Artifact says.* README line 5 claims the logical core "guarantees absolute
certainty in data retrieval, provenance tracking, and deductive reasoning
capabilities", and line 65 claims "zero hallucinations in memory recall".
`AUDIT_REPORT.md:5` records the reviewer as "Synapse Automation Engine (Product
Owner Agent)" and line 58 as "VERDICT: CERTIFIED FOR RELEASE".

*Inference.* Determinism of retrieval over recorded triples is a real property
and is not the same property as truth. The pipeline that produced the triples —
Markdown parsing, link extraction, and whatever asserted the statement upstream —
is outside the guarantee. A self-issued certificate from the system's own agent
is a test run, not an independent audit; it is exactly the shape rule 3
(non-self-ratifying decisions) names.

### 3.2 Open Ontologies

*Artifact says.* At the pinned commit the repository provides a single Rust
binary over Oxigraph with SHACL validation, SPARQL, OWL-RL/OWL-EL and an OWL2-DL
tableaux reasoner, ontology alignment, versioning, drift detection
(`src/drift.rs`), a KGCL change-report emitter (`src/kgcl.rs`, reached via
`Drift::detect_kgcl`), a competency-question runner (`src/cq.rs`), an MCP
server, and a Tauri studio. `src/drift.rs:18` canonicalises each snapshot via
RDFC 1.0 before diffing.

*Artifact says.* `docs/determinism.md` documents two nondeterminism defects and
their fixes. Tableaux classification failed on 2 runs in 6 on a fixed commit and
machine because `named_classes` is a `HashSet` and five expansion sites in
`src/tableaux.rs` collected `self.nodes.keys()` in hash order; the fix is
sorting both traversals by identifier, moving the conformance binary from 4/6 to
12/12 passing and from 1.17-2.57 s to 0.01 s. Alignment produced five different
correspondence sets (1151-1154, five distinct hashes) across five runs of the
same binary on the same input.

*Inference.* The tableaux failure mode is the instructive one: nothing false was
ever asserted; a true entailment was silently omitted when an unlucky ordering
exhausted the budget first. Omission is indistinguishable from a correct
negative without a complete oracle — which makes hash-order nondeterminism a
correctness hazard in any evidence system, including a purely relational one.

*Artifact says.* `benchmark/reasoner/README.md:43-84` withdraws the repository's
own published 1,633x speed claim against HermiT: three stacked faults meant the
benchmark measured process start-up over an empty store. Measured correctly on
`pizza.owl`, HermiT completes in 170 ms with 311 inferred subsumptions; Open
Ontologies spends 10.9 s, expires its budget, and infers 0, leaving 143
undetermined. The document states "No speed claim against any Java reasoner
should be made from this repository."

*Inference.* Two conclusions, and they point in opposite directions. The
project's engineering culture is above average — it publishes its own inverted
result with reproduction commands. Its OWL2-DL reasoner is nonetheless not a
substitute for HermiT, Pellet, or ELK on classification workloads today.

## 4. Facts versus claims

The trust boundary that a Markdown-first memory design inverts. `I1` and `I5`
require that authority flow from artifacts outward; a graph built by parsing
prose flows the other way, and the inversion is invisible in a box diagram
because both shapes contain a node labelled "Markdown".

```text
runs/... / diff.patch / gate/verdict.json / analyzer output
                         │
             deterministic extractors
                         │
                         ▼
                  typed facts
             with artifact provenance
                         │
                         ├──────────────┐
                         │              │
human-authored Markdown  │              │
agent-produced text      │              │
          │              │              │
          ▼              │              │
        claims           │              │
          │              │              │
 artifact binding / deterministic evaluation
          │                             │
          ▼                             │
SUPPORTED / REFUTED / UNDETERMINED      │
          │                             │
          └──────────────┬──────────────┘
                         ▼
                 007 memory index
                 written only by 007
                         │
                         ▼
           rendered context.md / task.md /
                  digest / recall view
```

The distinction is operational, not vocabulary:

- a **fact** exists because it was deterministically extracted from an
  authoritative artifact;
- a **claim** exists because a human or an agent asserts it;
- a claim acquires a status only after binding to artifacts and evaluation;
- the Markdown rendering of memory is a projection, never a back-channel source
  of truth.

A human-confirmed document can be an authoritative *decision*. That does not
make its statements about current code permanent facts — those stay
revision-bound, and rule 4 requires keeping "the artifact says", "we infer", and
"we decided" apart even inside a ratified document.

### 4.1 Two verdict axes, not one

Open Ontologies' `consistent / rejected / undetermined` is not transplanted
wholesale onto memory claims. `consistent` means only "no contradiction found",
which is weaker than "supported" — an empty knowledge base is consistent with
nearly everything, absence of knowledge being famously agreeable.

007 keeps two axes:

```text
claim ↔ evidence      SUPPORTED / REFUTED / UNDETERMINED
model / constraints   CONSISTENT / INCONSISTENT / UNKNOWN
```

`UNDETERMINED` must be self-explaining, so that "there is no implementation" is
never conflated with "we could not establish one":

```json
{
  "claim_verdict": "UNDETERMINED",
  "reason_code": "MISSING_IMPLEMENTATION_COVERAGE",
  "evidence": [],
  "missing_evidence": [
    "call-site inventory",
    "required gate result"
  ]
}
```

## 5. Why 007 does not need RDF/OWL today

**Decision.** 007 does not adopt RDF/OWL or Open Ontologies as an architectural
dependency. The current domain model consists predominantly of operational
facts, provenance, changes, and reachability. Typed relations with SQL /
recursive CTEs, or a small Datalog-like layer, are sufficient for it.

The supporting observation is about query shape. 007's real questions — impact
of a change, which requirement lost its implementation, transitive closure over
call edges, what differs between two runs — are reachability and aggregation
over a fact table. They are recursive-query problems. Description logic earns
its cost when class-level axiomatics and subsumption are themselves the product;
007's graph is nearly free of class axioms.

The cost side is measured, not assumed: at the pinned revision the OWL2-DL path
carries documented hash-order nondeterminism (since fixed) and, on a 1,944-triple
ontology, spends 10.9 s to infer nothing where HermiT takes 170 ms
([§3.2](#32-open-ontologies)). Paying that for what a recursive CTE answers is a
bad trade. People have bought themselves a logic reactor to boil a kettle often
enough that the failure mode deserves a name.

## 6. Ideas adopted without dependencies

Four things are worth taking as discipline. None requires a triple store, and
all four are implementable against the existing `runs/` layout.

1. **Competency-question corpus** — see [§7](#7-competency-questions-as-the-admission-test).
2. **Evidence-aware tri-state claim verdicts** — the two axes and the
   `reason_code` / `missing_evidence` shape from
   [§4.1](#41-two-verdict-axes-not-one).
3. **Deterministic traversal rule** — every hash-derived traversal is fully
   sorted, with a total order on ties, and the ordering is asserted in tests
   rather than assumed. This is not borrowed elegance; it is the defect class
   from `docs/determinism.md`, whose signature is a silently omitted true result.
4. **Stable documentation anchors** — separate `snapshot_hash`,
   `block_content_hash`, and a `block_id` that survives edits elsewhere in the
   file.

On (4), an AST path made only of sibling indices reproduces the Synapse failure
one level up: inserting a paragraph still shifts half the document. The
identifier is composed instead from

```text
heading ancestry
+ node kind
+ local semantic key or normalized-content fingerprint
```

with `snapshot_hash`, source span, and `block_content_hash` stored separately, so
that "moved", "edited", "deleted", and "file changed elsewhere" stay
distinguishable.

## 7. Competency questions as the admission test

A competency question is admitted as a test case, not as prose. A question
without positive, negative, and vacuity cases will return an empty list against
an empty store and be scored as a pass — the same defect class as the withdrawn
benchmark in [§3.2](#32-open-ontologies), wearing a memory layer's clothes.

```text
CQ-001
Question:
  Which requirement has no confirmed implementation?

Required inputs:
  requirement claims
  implementation bindings
  artifact revisions

Expected semantics:
  return only requirements with no SUPPORTED implementation binding

Required witness:
  requirement identity
  inspected revision
  attempted bindings
  reason for absence

Vacuity guard:
  test corpus must contain at least one requirement and one implementation
```

Opening set:

1. Which artifact supports this specific statement?
2. Which claims remain UNDETERMINED?
3. Which requirements have no confirmed implementation?
4. Which implementations lost their requirement between two revisions?
5. What changed between run-42 and run-58?
6. Which past failures are relevant to the current task?
7. Which facts became stale after their source artifact changed?
8. What is the transitive impact of changing entity X?

The corpus is the admission test for the storage decision in
[§5](#5-why-007-does-not-need-rdfowl-today): it defines the minimum relation
model, and it makes the "is a graph engine needed" question empirical rather
than aesthetic.

## 8. Escalation criteria for a graph or ontology layer

The decision in [§5](#5-why-007-does-not-need-rdfowl-today) is reopened when a
genuine need appears for at least one of:

- class axiomatics and subsumption as a **product** function;
- open-world semantics;
- interchange with external RDF/OWL ontologies;
- formal classification not replaceable by ordinary rules over facts.

Explicitly **not** escalation criteria: the graph looks impressive, SPARQL
exists, or a README depicts a brain.

## 9. Rejected integrations

| Rejected | Ground |
| --- | --- |
| Synapse's fact-from-Markdown trust model | Inverts `I1`/`I5` ([§4](#4-facts-versus-claims)) |
| Open Ontologies as a Cargo dependency | Single-author project, department-sized scope, recent inverted-benchmark history; the useful parts are specifications (RDFC 1.0, KGCL) implementable directly, and Oxigraph is reachable without the wrapper |
| OWL2-DL reasoner | [§5](#5-why-007-does-not-need-rdfowl-today) — measured cost, no matching query need |
| RDF store as default memory representation | Same |
| Label-based elevated truth | See below |
| Regex-first Markdown parsing as the extraction path | Claims must bind to structure, not to a pattern match |
| Writing inferences back into human documents without a review gate | Violates `I2` |

On elevated truth, the corrected form:

> A source may acquire limited authority only through an explicit policy: source
> type, admissible claim kinds, ratification receipt, revision binding, and an
> invalidation rule.

There is no universal `role: CoreSpecification` that converts text into truth.
At the pinned commit that role is literally a string added at ingest
(`src/ingest/mod.rs:56`) — an epaulette stitched onto a Markdown file.

## 10. Revision-bound evidence

```text
Internal artifacts inspected:
- docs/agent-memory-layer.md
  blob bbc8d3499aa7c7ee0016309e894365b929a141b4
- docs/evidence-and-decision-discipline.md
  blob 39aa9f3948d4c95c99a11da687e06d450884b6c2

External repositories inspected:
- pmaojo/synapse-engine
  commit 7a10e81efe988d55369bf748d434558fb56b5b21
- fabio-rovai/open-ontologies
  commit 63a4d14111f993873e5e65de7667655fdfa9e52b

Inspection date:
- 2026-08-06
```

Verification status: both internal blob hashes were confirmed against this
repository's HEAD at authoring time (`git rev-parse HEAD:<path>`). Both external
commits were confirmed to exist and were checked out by SHA; every quoted file,
line number, and figure above was read at those commits.

The Open Ontologies revision is load-bearing rather than a decorative link to a
moving `main`: it is the revision at which `docs/determinism.md` and
`benchmark/reasoner/README.md` document the two corrected nondeterminism sources
and the withdrawn benchmark claim. An earlier revision would support neither the
adoption in [§6](#6-ideas-adopted-without-dependencies) nor the rejection in
[§9](#9-rejected-integrations).

**Non-load-bearing observational metadata.** Star counts, open-issue counts, and
last-push dates were considered and are deliberately excluded. They carry no
information about a layer's fitness, and being unanchored they would be the
first thing in this document to rot — which would be a poor look for a document
about documentation rot.

## 11. Decision

```text
ADOPT:
- competency questions
- evidence-aware tri-state claim verdicts
- deterministic traversal checklist rule
- stable documentation anchors

DO NOT ADOPT:
- Synapse fact-from-Markdown trust model
- Open Ontologies dependency
- OWL2-DL reasoner
- RDF store as the default memory representation
- label-based elevated truth

DEFER:
- Datalog/recursive-query implementation until competency questions
  expose the minimum required relation model
- Oxigraph or RDF only until explicit escalation criteria are met
```

No new code follows from this document. It fixes a representation decision, a
trust boundary, and an admission test; the first implementable step is the
competency-question corpus of [§7](#7-competency-questions-as-the-admission-test),
which determines the relation model that the deferred items depend on.
