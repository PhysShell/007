# Memory Plane for 007 — evidence record, requirements, candidate architecture

- **Status:** design input · **agent-authored and `pending`**
  (`docs/evidence-and-decision-discipline.md` rule 3). This document is **not an
  approved architecture**. Nothing here is normative authority, and nothing here
  may be cited to justify a further autonomous decision.
- **Date:** 2026-08-09. External claims are bound to the versions named in §1 and
  are `STALE` the moment those move (rule 4).
- **Scope:** the normative half of agent memory — what an agent is *obliged* to
  know before an action is admissible, and how that obligation survives a
  session boundary. The advisory half (embeddings, similarity recall, drift) is
  in scope only as a thing to be fenced off.
- **No code follows from this document.** It fixes vocabulary, requirements, and
  failure classes so that later work does not re-derive them or land the weaker
  forms by accident.
- **Review round 1 (2026-08-09):** seven findings against the first revision,
  all closed here — §4.0 (input closure), §4.1.1 (scope-key canonicalization),
  §4.2.1 (witness determinism), §4.3 (`ScopeExpansionFailure` structure),
  §4.5.1 (waiver expiry as an event), §4.8 (manifest completeness, v0 handoff
  scope), §5.6 (named consistency conflicts), §6 (version binding). Two were
  defects of this document against its **own** requirements: REQ-9 did not close
  over `resolver_version`, and `WAIVED` had no return transition — which would
  have made the attractor projection a clock read, the exact class REQ-11
  forbids.
- **Review round 2 (2026-08-09):** four findings, all closed here. Two were
  again input-closure defects, one level below the round-1 fix: `R` did not
  close over the witness rule, the closure rule set, or the closure bounds
  (§4.0, `ResolverPolicy`), and `C` did not close over its advisory half at all,
  since semantic recall is by construction outside authoritative state (§4.0.1,
  `AdvisoryInputSnapshot`). The other two removed hidden inputs of the same
  kind: `schema_version` now sits inside state identity rather than only inside
  the handoff predicate, and the attractor rebuild property names the embedding
  model and indexer it actually depends on (§4.5.2). The recurrence of one
  failure class across two rounds is itself the finding — every fix that
  introduces a versioned component must be checked against the input tuples in
  §4.0 before it is called done.

> **Red line, stated once and binding on everything below.**
> **No implementation material in this document is derived from
> `MichaelNeuberger/neo4j-agent-integrations`.** That repository is marked
> `Copyright (c) 2025 Michael Neuberger — All Rights Reserved` (§1.5). It was
> read as a reference artifact to establish *what a third party claims*, and for
> nothing else. No Cypher schema, no drift logic, no probe implementation, and
> no data model from it is adapted here. Where a requirement below happens to
> address a failure class that third-party artifacts also exhibit, the
> requirement is derived from 007's own constraints and is marked as such.

## 0. What this adds, and what it deliberately does not re-tread

This document sits between four existing ones and repeats none of them.

| Existing document | What it already owns | What this document adds |
|---|---|---|
| `docs/agent-memory-layer.md` (draft) | Memory item types, trust levels (`agent-claimed` … `human-confirmed`), the `007 writes memory, not the agent` boundary, ingest/recall commands | A **lifecycle** for failure knowledge (a trust level says who vouched for an entry; it does not say whether the entry is still true), and the split between the planner projection and the regression projection |
| `docs/task-aware-context-generator.md` (draft) | Deterministic Context IR, cache key, `context.meta.json`, omitted-candidate reasons, the proposed freshness stage | The **normative-obligation** half: which entries are *required* rather than *ranked*, a typed outcome when the required set does not fit, and the rule that a compiler may never silently satisfy a budget by narrowing the obligation |
| `docs/invariant-registry.md` (ratified) | Named invariants and their executable witnesses, enforcement-site join | The **scoping** question — which registered invariants are active for a given unit of work — which the registry deliberately does not answer |
| `docs/evidence-and-decision-discipline.md` (ratified) | Rules 1–4; `raw → classifier → typed → policy` | An application of rule 4 to *memory reads*: an advisory retrieval signal is a lower-layer signal and may never enter the upper layer as a fact |

The one-line statement of the gap all four leave open:

```text
recall answers   "what is probably useful to remember?"
007 needs first  "what must be true in my context for this action to be admissible?"
```

Those are different questions with different failure modes. A missed recall is a
worse answer. A missed obligation is an inadmissible action that looks fine.

## 1. Source / factual record

Everything in this section is *artifact says*, bound to a version. Inferences
are marked and kept out of the tables.

### 1.0 Method and its limits

Sources were read on **2026-08-09** via an HTTP fetcher. One limitation must be
recorded, because it changes the weight of §2:

**`neo4j.com` returned HTTP 403 to the fetcher on every attempt.** The article
body was never read directly. Every article-level claim below (author, date,
tested version, test count, scenario list) is **second-hand**, taken from the
Neo4j Community announcement page and search-index summaries, and is therefore
weaker evidence than the vendor-documentation claims, which were fetched
directly. Where an article-level claim is load-bearing, it is marked
`[second-hand]`.

### 1.1 Artifact identity

| Artifact | Identity | Verified |
|---|---|---|
| Article | *Constant-cost semantic memory for multi-agent systems*, `neo4j.com/blog/developer/…`, author Alexander Erdl, published **2026-08-04** `[second-hand]` | 2026-08-09 |
| Version the article runs on | **Semvec 0.7.0**, exercised by a 274-test pytest suite `[second-hand]` | 2026-08-09 |
| Engine | `semvec` on PyPI, current **0.8.7** | 2026-08-09 |
| Vendor documentation | `semvec-docs.pages.dev` (`architecture/`, `api-reference/{core,coding,cortex}/`, `user-guide/{in-process-library,production-hardening,coding-agents/claude-code}/`, `benchmarks/`, `changelog/`, `getting-started/licensing/`) | 2026-08-09 |
| Article's code repository | `MichaelNeuberger/neo4j-agent-integrations` — **All Rights Reserved** | 2026-08-09 |

Recorded discrepancy, immaterial but not swept: the vendor changelog dates
0.8.7 **2026-08-03**; the PyPI project page shows a release date of
**2026-08-04**. PyPI's release history also lists yanked releases in the ranges
**0.4.0–0.5.2** and **0.8.1–0.8.2** (read as a project-page summary; per-release
yank reasons were not opened).

### 1.2 Three layers, three different APIs

The single most confusable fact about this source, and the one an earlier round
of this analysis got wrong in both directions:

```text
SemvecState.update()            (core)
    similarity · beta · pattern_strength · fsm · phase · norm ·
    topic_switch · novelty_score · dedup_signal
    → no drift_score field

SemvecSession.run()/run_sync() → TurnResult      (library facade, since 0.7.0)
    context · top_similarity · short_circuit ·
    drift_score (0.0–1.0) · drift_detected (drift_score >= 0.5) ·
    drift_phase ("stable" < 0.3 · "shifting" 0.3–0.5 · "drifted" >= 0.5) ·
    dedup_signal · retrieval_error

demo integration                                  (third-party, ARR)
    the same signals, re-thresholded at 0.55 / 0.55, persisted to a graph
```

*Artifact says:* the numeric drift score and the `stable/shifting/drifted`
vocabulary are the **vendor's**, documented at the session layer, not the demo's
invention. The demo's own contribution at this point is operational thresholds
and persistence. *Inference:* the signal is a two-line computation over two
embeddings; nothing in 007 would need the vendor to obtain it.

Other API facts, directly fetched:

- Coding surface: `register_code_change(file_path, intent, signature)`,
  `record_error(error_text, source)`,
  `check_anti_resonance(proposal, threshold=0.7)`,
  `get_compacted_context(task, *, invariants, test_summary, git_diff)`,
  `build_handoff_context(next_checkpoint)`, `save_state()/load_state()`;
  `NegativeAttractorSet.check(proposal_vector, threshold)`.
- `CodePointer` fields: `intent_vector`, `file_path`, `signature`, `importance`,
  `access_count`, `timestamp`, `semantic_hash` (auto-computed).
- Budget-constrained assembly order: **invariants (always) → anti-resonance →
  code pointers (by task similarity) → test status → git diff → semantic
  memories → phase indicator**.
- MCP surface: six tools — `pss_get_context`, `pss_update`,
  `pss_check_anti_resonance`, `pss_register_code`, `pss_record_error`,
  `pss_save`; driven by `SessionStart` / `PreCompact` hooks.
- Cortex state transfer: `StateVectorPacket.serialize()/deserialize()` plus
  **`verify_integrity()`**, documented as a `serialize → deserialize`
  round-trip check preserving IEEE-754 bit patterns. **There is no documented
  `verify_consistency()`, and no documented behavioural-equivalence check
  anywhere in the vendor API.**
- Exact-text layer: `LiteralCache`, described as a verbatim structured-memory
  layer; numeric values held as `Decimal`; snapshot redaction is controlled by
  `include_literal_cache_text`.
- Default state dimension 384; default tier capacities 15 / 50 / 200.

### 1.3 Defect record from the vendor's own changelog

Quoted because the vendor states these more damningly than any critic would.

**0.8.2 (2026-08-01)**

- Tier capacities were not persisted; with non-default tiers,
  `to_bytes() → from_bytes()` raised `checksum mismatch` — "a freshly written
  snapshot then failed its **own** checksum".
- "Six pieces of live state were missing from the snapshot, so the next
  `update()` after a restore returned different metrics than the state it was
  copied from."
- "Identical conversations now produce identical state, including above memory
  capacity where eviction and consolidation are active."
- "Evicted memories no longer stay alive internally. They were retained by the
  clustering layer, so memory use grew with the conversation instead of staying
  bounded."

**0.8.5 (2026-08-03)**

- "the heaviest was `build_handoff_context()`: it named different error patterns
  after every restore — four restores of the same unchanged state produced four
  different sets".
- Snapshot format 9/10: "States written by 0.8.5 **cannot** be read by older
  versions; those reject them explicitly rather than misreading them."; downgrade
  after a new-format write is not possible.

**Restore fidelity is licence-gated (documented in production hardening).**
Reproducing the absolute values of `calculate_fsm()`, `calculate_metrics()` and
`calculate_advanced_metrics()` after a restore requires **all three** of: an
official wheel, a Pro or Enterprise licence, and *the same licence subject that
wrote the snapshot*. Otherwise the restore is "still complete and valid" but
those methods "resume from a **fresh** salt". The same page states that from
0.8.2 "the integrity checksum covers all of it, so a blob that verifies is a
blob that continues" — a claim scoped to one version and one licence context.

### 1.4 Benchmark record

- LOCOMO, LLM-as-judge `J`: **Semvec 0.605** over 1540 non-adversarial QAs
  across all ten conversations; reader/judge `openai/gpt-4o-mini` at
  temperature 0, binary `CORRECT`/`WRONG` per QA.
- Comparators as published by the same vendor page: **mem0 paper 0.669**, and a
  **head-to-head reproduction 0.675** — the reproduction run on **one
  conversation, `conv-44`, n = 123**.
- Cost claims: ~2 000 context tokens per reader call; "~87 % smaller prompts" /
  "~8×" versus full replay; "17× faster" wall-clock; **zero generative LLM calls
  at ingest** (an embedder is still required — the claim is about generative
  calls, not about zero compute).
- Internal inconsistency in the vendor's own material: the product site states
  "4–5k tokens. At turn 10 and 10 000", the benchmark page ~2 000 per reader
  call.

*Inference:* the comparative headline rests on unequal samples (1540 QAs versus
123), so it establishes an interesting cost/quality trade-off and does not
establish a quality ranking in either direction.

### 1.5 Entitlement, licensing, patents

- Engine: proprietary, closed Rust source, wheels only. "Commercial use requires
  a Pro or Enterprise license."
- Tiers: Community 5 QPS sustained / 50 burst (documented use cases:
  "evaluation, prototyping, open-source side projects"); Pro 200 / 2000;
  Enterprise unthrottled. JWT TTL 30 days; expiry is a hard fail
  (`LicenseExpiredError`); rate-limit exhaustion raises `RateLimitError`.
- Patents: four applications **pending** — US non-provisional 19/269,195 and
  19/550,466; EP 25 188 105 and EP 26 160 795. The vendor's own wording: claims
  of pending applications, not enforceable exclusive rights.
- Declared patent boundary (explicitly out of scope for public documentation):
  state-update mathematics and adaptive policies; phase-detection rules,
  thresholds and window sizes; retention scoring and eviction logic;
  topic-switch internals; multi-agent consensus and attention mechanisms; binary
  layout. Technical review only under NDA.
- Article's code repository: **All Rights Reserved** (see the red line above).

### 1.6 Claims examined and *not* established

Recorded so that a later reader does not promote them by repetition:

- **"The integration repository is MIT."** **Refuted.** It is
  `All Rights Reserved`. The plausible origin of the error is the unrelated
  `neo4j-labs/neo4j-agent-integrations`, a different repository that does not
  mention semvec.
- **`verify_consistency()` in the vendor API.** Not found. Only
  `verify_integrity()` exists, and it checks serialization fidelity, not
  behaviour.
- **A `mutations` field returned by `update()`.** Not found on any fetched page.
- **A vendor warning phrased as "a snapshot checksum is not a cross-version
  state identity".** Not found in that form. What *is* documented: format-9/10
  rejection by older versions, and the licence-gated diagnostic salt. Use those,
  not the paraphrase.
- **The `INVESTIGATED` edge carrying timing, similarity, agent role or an
  LLM-call flag.** Not established; the repository's own README documents three
  properties — `step`, `drift_score`, `phase`.
- **A demo-side warning that changing the embedding model requires retuning the
  numeric thresholds.** Raised in review; not independently verified here.

## 2. Evidence assessment of the demonstration

Stated in the three-level form rule 4 requires.

```text
Artifact says:  the demonstration exports agent state, checksums it with
                SHA-256, imports it into another agent, and reports increased
                similarity plus a passing behavioural probe after import.
                [second-hand for the article; README-level for the repository]

Inference:      that is an observed outcome of one scripted healthcare
                scenario on one version. It is not a demonstration that
                handoff is deterministic, restorable, or correctness-
                preserving, because none of those properties is what the
                scenario measures.

Decision:       the demonstration is admissible in 007 as evidence that the
                shape of the problem is real, and is NOT admissible as
                evidence for any property of handoff correctness.
```

There is a second, harder reason, and it comes from stitching two vendor
documents together rather than from opinion:

```text
article runs on 0.7.0                       (released 2026-06-04)
        │
        │  vendor changelog, 0.8.2 (2026-08-01) and 0.8.5 (2026-08-03):
        │    restore diverged from the source state on the next update()
        │    tier capacities did not survive a snapshot
        │    build_handoff_context() named different error patterns per restore
        │    evicted memories were retained internally — growth was unbounded
        ▼
the article's cross-session handover scenario ran on a version in which the
vendor has since documented defects in exactly the mechanism the scenario
demonstrates
```

This is not a criticism of the vendor — disclosing that class of defect in
public release notes is better practice than most. It is a statement about
evidence weight: a passing demonstration on a version whose restore path was
later found non-deterministic cannot be promoted to a property proof. It is the
context-layer instance of a rule this repository already enforces on itself:
*an absent signal is not a negative result*, and its mirror — **a passing
demonstration is not an established property** (`AGENTS.md`, "diagnosing is not
repairing" block).

## 3. Requirements

Derived from 007's own constraints. REQ-1 … REQ-8 restate obligations the repo
already implies; REQ-9 … REQ-11 are the ones this round added, and they are
**not** an appendix — 9 and 11 are what make 3 and 7 checkable rather than
aspirational.

| # | Requirement | Checkable by |
|---|---|---|
| REQ-1 | Normative facts (goals, decisions, invariants, evidence references) survive a session boundary **exactly**, not approximately | byte-level manifest equality across export/import, within one schema and canonicalization version (§4.8) |
| REQ-2 | Context construction is **bounded** by a declared budget, and the bound is a property of the compiler, not of the caller's restraint | compiler refuses rather than exceeds |
| REQ-3 | Every included entry carries a machine-readable reason for its inclusion | presence of an `InclusionProof` per entry |
| REQ-4 | Evidence is **revision-bound**: it names the revision and digest it was established against | schema-level; no unbound evidence admitted |
| REQ-5 | Stale knowledge is **detectable** and distinguishable by cause | typed `stale_reason` on every non-`VERIFIED` binding |
| REQ-6 | Failure knowledge has a **lifecycle**, and state transitions require evidence | transition guards (§4.5) |
| REQ-7 | Handoff acceptance is **deterministic**; stochastic checks may not gate it | acceptance predicate contains no model call |
| REQ-8 | Semantic retrieval may **advise** but never **establish**: it cannot create a fact, satisfy an invariant, preserve a decision, authorize an action, or complete a handoff | trust-boundary test per consumer |
| REQ-9 | Scope resolution and context compilation are **each** reproducible and **diffable** from their own complete, declared input tuple (§4.0) | re-run each stage → byte-identical output + comparable metadata |
| REQ-10 | No component on the read path of normative state may depend on an external entitlement — licence, quota, remote availability, or token TTL | dependency audit of the read path |
| REQ-11 | Every derived normative projection is **disposable**: deletable and rebuildable from authoritative state, digest-identical | `rm -rf` the derived tree, rebuild, compare digests |

Three notes that are load-bearing:

- **REQ-9 is stronger than "deterministic".** Reproducibility alone lets you
  rebuild a black box. The compilation result must therefore carry
  `context_bytes`, `context_digest`, `included_entry_ids`, `inclusion_reason[]`,
  `omitted_candidate_ids`, and `token_count` — so that two compilations can be
  *compared*, not merely both regenerated. This extends, and must stay
  compatible with, the `context.meta.json` field list in
  `docs/task-aware-context-generator.md`.
- **REQ-9 is stated over two stages on purpose.** An earlier revision of this
  document named a single input tuple for compilation and omitted
  `resolver_version` — which the architecture itself introduces as a versioned
  component — so two runs agreeing on all four named inputs could still produce
  different required sets. That is the document's own REQ-9 failing on the
  document's own architecture. §4.0 fixes the input tuples; the requirement now
  refers to them rather than restating one of them incompletely.
- **REQ-10 generalizes a lesson, it is not a swipe at a vendor.** The pattern it
  forbids is any arrangement where reading the project's own normative state can
  fail on a calendar, a quota, or someone else's key. §1.5's licence-gated
  diagnostic salt is the cleanest illustration available: a value whose
  reproducibility depends on *which subject signed the snapshot* is not a fact,
  whatever else it is.
- **REQ-11 is the general form of the "evicted memories stayed alive" defect.**
  Once a derived structure is allowed to hold state that authoritative state
  cannot regenerate, it has quietly become a second source of truth, and both
  boundedness and reproducibility start rotting without a visible symptom. If a
  derived index is frightening to delete, it is no longer an index.

## 4. Candidate architecture

Candidate, not adopted. The point of writing it down is to make the failure
classes discussable before anything is built.

```text
                     AUTHORITATIVE STATE
        goals · decisions · invariants · evidence · failures
                              │
                    Normative Scope Resolver
                    deterministic · versioned · auditable
                              │
              ┌───────────────┴───────────────┐
              ▼                               ▼
      REQUIRED FOR SCOPE                OPTIONAL FOR SCOPE
      goals · invariants ·              semantic memories ·
      decisions · evidence              historical failures ·
                                        neighbouring code
                                              │
                                    freeze → AdvisoryInputSnapshot
                                              │  (§4.0.1)
              └───────────────┬───────────────┘
                              ▼
                      Context Compiler
                 hard budget · deterministic · diffable
                              │
                              ▼
                            Agent
```

### 4.0 Two functions, two input tuples

The plane has **two** deterministic stages, and each must close over its own
inputs. Stating one tuple for both is how a versioned component drops out of a
reproducibility claim without anyone noticing.

```text
ResolvedScope | ScopeExpansionFailure
                = R( canonical_state_digest,
                     canonical_scope_key,
                     resolver_policy_digest )

CompiledContext = C( canonical_state_digest,
                     resolved_scope_digest,
                     advisory_input_snapshot_digest,
                     model_budget_profile_digest,
                     compiler_version )
```

Every behavioural parameter of a stage is folded into exactly one digested
policy object rather than added to the signature one at a time — a signature
that grows a parameter per feature is a signature people stop reading.

```text
CanonicalStateEnvelope {           ResolverPolicy {
    schema_version                     resolver_version
    canonicalization_version           witness_rule_version
                                       closure_rule_set_version
    goal_state_digest                  max_derivation_depth
    decision_state_digest              max_required_entries
    invariant_state_digest         }
    evidence_state_digest
    failure_registry_digest        resolver_policy_digest =
}                                      digest(canonical(ResolverPolicy))

canonical_state_digest =
    sha256(canonical(CanonicalStateEnvelope))
```

Naming and closure, fixed here so that two names never drift into two concepts:

- **`canonical_state_digest`** is the one name for the identity of authoritative
  normative state. It covers the five partition digests **and**
  `schema_version` alongside `canonicalization_version`: the same canonical
  bytes may mean different things under two schema versions, so schema identity
  belongs inside state identity rather than only inside the handoff predicate.
  `exact_state_digest` was an alias in an earlier revision and is not used.
- **`canonical_scope_key`** is a `ScopeKey` (§4.1) after the canonicalization
  rule in §4.1.1. An un-canonicalized key is not an input to anything.
- **`resolver_policy_digest`** carries everything that changes what `R` returns:
  the witness rule (§4.2.1), the closure rule set, and both closure bounds
  (§4.3). The bounds are inputs, not configuration — they decide whether `R`
  returns a scope or a `ScopeExpansionFailure`, which makes them as
  result-determining as the graph itself. `closure_rule_set_version` is separate
  from `resolver_version` on purpose: changing the transitive-closure rules
  changes the function even when the executing code reports the same version.
- **`advisory_input_snapshot_digest`** is §4.0.1. Without it, `C` is not closed:
  the optional half of its input (semantic memories, historical failures,
  neighbouring code) is by construction *not* part of authoritative state, so
  two compilations can agree on every other argument and still differ.
- **`model_budget_profile_digest`** covers the budget profile *and* the
  `tokenizer_id` / `output_reserve` it implies (§4.4).

The rule that generalizes all of them: **every input of `R` or `C` appears in
the `HandoffManifest`, as a digest and — where useful for diffing — expanded.**
If a value is an argument, it is recorded; if it is not recorded, it is not
permitted to be an argument.

#### 4.0.1 Advisory inputs are frozen before they are compiled

Making semantic retrieval part of normative state would buy determinism by
destroying the trust boundary §4.9 exists to hold. The alternative is to leave
retrieval advisory *and* non-deterministic, and to materialize its result before
the deterministic stage consumes it:

```text
AdvisoryInputSnapshot {
    items[]                canonically ordered
    provenance[]           which channel proposed each item
    retrieval_identity     retriever + version + embedding model identity
    snapshot_digest
}
```

```text
retrieve                 may vary between runs
        ▼
freeze advisory snapshot recorded, digested, provenance-carrying
        ▼
compile                  MUST NOT vary
```

The snapshot is still advisory — nothing in it can establish a fact, satisfy an
invariant, or complete a handoff (REQ-8). It is merely *fixed*, which is what
REQ-9 needs. **Trust and determinism are independent axes**, and conflating them
is how a system ends up either trusting its retrieval or being unable to
reproduce anything that touched it.

### 4.1 Normative Scope Resolver

Named *normative* so that no later reader mistakes it for retrieval. Its input
is structural, never free text:

```text
ScopeKey {
    goal_node_id
    artifact_ids[]
    contract_ids[]
}
```

A natural-language task description may *select* a `ScopeKey`, and that
selection is itself an event, not a hidden step:

```text
ScopeCandidateSelected {
    candidates[]
    selected
    rationale
}
```

*Why this matters more than it looks:* a deterministic function of a
non-deterministic input is not deterministic. Admitting NL inside the normative
API would reproduce, one layer up, exactly the defect class §1.3 records.

#### 4.1.1 Canonicalization of a ScopeKey

`artifact_ids[]` and `contract_ids[]` are **sets, not sequences**. Before a
`ScopeKey` is used as an input or digested it is canonicalized: deduplicated,
sorted under a declared total order over identifiers, and stamped with
`scope_key_canonicalization_version`. Two callers naming the same artifacts in a
different order must produce the same `canonical_scope_key` and therefore the
same `resolved_scope_digest`. A list whose order is an accident of iteration is
an unversioned input in disguise.

### 4.2 Closure semantics

`Required(scope)` is a transitive closure, and every element carries its
derivation:

```text
InclusionProof {
    entry_id
    rule_id
    derivation_path[]
    depth
}

goal G17
  -> touches artifact A4
  -> governed_by contract C2
  -> requires invariant I8
  -> derived_from decision D3
  -> justified_by evidence E19
```

REQ-3 is then satisfied by construction rather than by a later "explain why you
recalled this" pass, which would be an advisory signal impersonating an audit
trail.

#### 4.2.1 The witness must be as deterministic as the result

In any real graph an invariant or an evidence node is reachable by **more than
one** derivation path. If the resolver records whichever path its traversal
reached first, then a change in iteration order — a different map
implementation, an edge inserted elsewhere, a parallel expansion — leaves
`Required(scope)` identical while `InclusionProof`, `resolved_scope_digest` and
every diff built on them change. That is a determinism claim that covers the
answer and not the explanation, which under REQ-9 is not a determinism claim at
all.

The resolver therefore declares a **witness rule**, versioned as
`witness_rule_version`, and it is one of:

```text
ALL_MINIMAL     record every minimal inclusion reason, in canonical order
SINGLE_TIEBREAK record one witness, selected by a declared total order over
                (depth, rule_id, derivation_path) — never by traversal order
```

`ALL_MINIMAL` is the safer default: it is the only one under which "why is this
here" survives the removal of a single edge without silently changing shape.
`SINGLE_TIEBREAK` is admissible where proof size is a real cost, provided the
tie-break is a declared function of the data. Traversal order is never a
tie-break, and `witness_rule_version` is part of `ResolverPolicy` (§4.0) — it
changes what `R` returns, so it is one of `R`'s inputs and not a setting applied
somewhere alongside it.

### 4.3 Closure bounds, and two failures that must not be merged

Depth alone does not bound anything — depth 3 with wide fan-out is still ten
thousand nodes. Two limits, and two distinct typed outcomes:

```text
max_derivation_depth
max_required_entries
        │
        ▼
SCOPE_EXPANSION_UNSATISFIABLE     the required set could not be computed
                                  within declared closure bounds
                                  (before the compiler runs)

CONTEXT_BUDGET_UNSATISFIABLE      the required set is correct and does not
                                  fit the budget
                                  (after the compiler runs)
```

Collapsing these into one error makes the resulting diagnosis unactionable: the
first says the graph or the bounds are wrong, the second says the work item is
too large. This is the same three-state discipline `AGENTS.md` already enforces
for `PASS`/`FAIL`/`ERROR` — a state that means "the machine could not obtain an
answer" must not be filed under a state that means "the answer is no".

Since §5 rates closure blow-up as likely rather than hypothetical, the
scope-side failure carries as much diagnostic structure as the budget-side one.
A typed dead end is still a dead end:

```text
ScopeExpansionFailure {
    canonical_state_digest     ─┐ the full input tuple of R (§4.0):
    canonical_scope_key         │ a failure that cannot be replayed
    resolver_policy_digest     ─┘ is an anecdote

    reached_depth              ─┐ expanded from the policy for diffing,
    max_derivation_depth        │ not a second source of the same values
    required_entries_seen       │
    max_required_entries       ─┘

    frontier[]                 unexpanded nodes at the cut, canonically ordered
    triggering_rule_ids[]      the closure rules that produced the fan-out
}
```

A failure is an output of `R`, so it records the same inputs a success would —
otherwise the one outcome most worth reproducing is the one that cannot be.
`frontier[]` and `triggering_rule_ids[]` are what make it actionable: they say
*where* the closure exploded and *which rule* did it, which is the difference
between "tighten this contract-to-artifact binding" and "raise the limit until
it stops complaining".

### 4.4 Budget failure is a typed outcome with a proposal, never a silent trim

```text
ContextBudgetFailure {
    required_tokens
    available_tokens
    output_reserve
    tokenizer_id

    decomposition_candidates[] {
        sub_scope_key
        fits
        retained_requirements[]
        deferred_requirements[]
    }
}
```

Two constraints:

- **A decomposition candidate is a proposal and is never auto-applied.** A
  compiler that picks a smaller scope to make the numbers work has silently
  changed the task, which is the same failure as a gate that lowers its
  threshold to go green.
- **The budget is per-model.** `tokenizer_id` and `output_reserve` are part of
  the failure record because a token bound measured with the wrong tokenizer, or
  without reserving room for the model's own output, is not a bound. A "hard"
  budget that softens on model change was never hard.

### 4.5 Failure Registry lifecycle

```text
ACTIVE  ──▶ REMEDIATED    requires verification_record_id
        ──▶ SUPERSEDED    requires successor_failure_id
        ──▶ WAIVED        requires decision_id + expires_at + scope_binding
   *    ──▶ OBSOLETE      requires invalidation_reason

WAIVED  ──▶ ACTIVE        via a recorded WaiverExpired event
```

Transitions carry evidence or they are opinions with better typography. Two
consequences need their own headings, because both are places where a plausible
shortcut would silently break a requirement.

#### 4.5.1 Expiry is an event, never a clock read

A waiver with an `expires_at` and no return transition leaves exactly two
possibilities, and both are defects. Either the waiver is effectively permanent,
which is not what an expiry date means; or the projection compares `expires_at`
against wall-clock time at read, in which case **one unchanged
`failure_registry` yields different projections at different times** — REQ-9 and
REQ-11 both fail, and they fail invisibly, because every individual read looks
correct.

So expiry is materialized:

```text
WAIVED --WaiverExpired--> ACTIVE      authoritative state changes
                                      canonical_state_digest changes
                                      the projection is reproducible again
```

The general rule this instance serves, and the one worth carrying to every other
projection in the plane: **no derived normative projection may depend on a clock
read.** Time may *cause* a state transition; it may never be an input to a
projection.

A waiver also carries `scope_binding`. A decision to tolerate one failure in one
place is not a decision to switch that failure pattern off everywhere, and an
unscoped waiver is the cheapest available way to silently disable a control.

#### 4.5.2 Two projections, not one

```text
planner_projection(F)  ≠  regression_projection(F)
```

A `REMEDIATED` failure leaves the planner's advisory context — it must stop
forbidding an approach that is legal again — and **stays** in the regression
projection forever, because it is precisely the thing whose recurrence must be
detected. Deleting it from both is how a fixed bug becomes a rediscovered bug.

The semantic attractor set is a **projection of `ACTIVE` entries only**, and
under REQ-11:

```text
attractor_index == build_attractor_index(
                       failure_registry,
                       embedding_model_identity,
                       attractor_indexer_version )
```

is an acceptance property, not an implementation note.

The index is semantic, so its bytes depend on the embedding model and the
indexer as much as on the registry — the same hidden-input problem §4.9 already
records for drift observations. Naming those inputs keeps the rebuild property
true instead of aspirational. This does **not** promote the index: it stays
advisory under REQ-8. Trust and determinism are independent axes (§4.0.1), and
an advisory artifact that is reproducible from recorded inputs is strictly
better than one that is merely advisory.

*Relationship to `docs/agent-memory-layer.md`:* `FailurePatternMemory` and the
trust levels answer *who vouched for this entry*. This answers *is it still in
force*. Both are needed; neither substitutes — but note that the existing trust
enum already mixes the two, which is conflict **C-1** in §5, and closing it is a
change to that document rather than an addition to it.

### 4.6 Evidence binding, without the word "stable"

The word is dropped from the model deliberately: a promise of stable symbol
identity across refactors is not one we can keep, and naming a field for a
guarantee it cannot provide invites callers to trust it.

```text
SymbolLocator {
    qualified_name
    syntax_fingerprint
    span_hint
}

ResolutionResult {
    matched_by            which rung matched
    confidence_class      degraded on lower rungs
    resolved_subject
}
```

Resolution is a ladder — qualified name, then syntax fingerprint, then span hint
— and *which rung matched* is recorded, because a match on the bottom rung is
a weaker claim than a match on the top one.

```text
Rename MUST NOT cause automatic semantic rebinding.
```

An unresolved binding that demands a human or an explicit re-binding decision is
strictly better than a clever match that silently proves a claim against the
wrong code. This is the binding-layer form of the constraint
`docs/task-aware-context-generator.md` already proposes for spans: a candidate
that fails re-resolution is dropped with a recorded reason, never silently
rendered.

### 4.7 Verification provenance, and staleness as a cause

```text
VerificationStamp {
    subject_revision
    recipe_id
    recipe_version
    environment_digest
    result
    cost_class
}

stale_reason:
    SUBJECT_CHANGED
    RECIPE_CHANGED
    ENVIRONMENT_CHANGED
    DEPENDENCY_CHANGED
```

`recipe_id` + `recipe_version` is the load-bearing pair: it makes a claim
**re-derivable** instead of remembered, and it makes a changed recipe invalidate
its own past results. Making staleness a *cause* rather than a state is what
lets the re-verification queue be scheduled at all — `cost_class` then decides
what is re-checked eagerly and what is batched.

Delineation, so the layering stays honest:

```text
staleness detection   cheap, digest-driven, answers "might this be wrong now?"
truth verification    priced, recipe-driven, answers "is this still proved?"
```

### 4.8 Handoff: two planes, one of which gates

```text
HandoffManifest {
    // state identity — the envelope of §4.0
    schema_version
    canonicalization_version
    canonical_state_digest
    goal_state_digest
    decision_state_digest
    invariant_state_digest
    evidence_state_digest
    failure_registry_digest

    // inputs of R
    canonical_scope_key
    scope_key_canonicalization_version
    resolver_policy_digest
    resolver_version              ─┐ expansion of the policy,
    witness_rule_version           │ for diffing and debugging;
    closure_rule_set_version       │ the digest above is the
    max_derivation_depth           │ authoritative input
    max_required_entries          ─┘

    // inputs of C
    resolved_scope_digest
    advisory_input_snapshot_digest
    model_budget_profile_digest
    compiler_version

    // outputs
    compiled_context_digest
}
```

The grouping is the point: every argument of `R` and `C` in §4.0 appears here,
and the expanded policy fields are a *view* of `resolver_policy_digest`, never a
second place to set the same value. A manifest that omits an argument cannot
re-derive its own result; a manifest that carries two independently-settable
copies of one argument is worse.

```text
HANDOFF_ACCEPTED  iff  schema_version_equal
                   AND canonicalization_version_equal
                   AND exact_manifest_valid
                   AND required_goals_equal
                   AND active_invariants_equal
                   AND decisions_preserved
                   AND evidence_refs_resolvable
                   AND artifact_digests_valid          ← all deterministic

SemanticContinuityAssessment → PASS / WARN / INCONCLUSIVE
                                                       ← never gates
```

**v0 is same-schema, same-canonicalization only, and says so.** REQ-1 is checked
by byte-level manifest equality; the word *compatible* would quietly widen that
into an undefined compatibility relation, which in a document this strict is a
promise with no semantics behind it. A version mismatch in v0 is not a failed
handoff — it is `HANDOFF_MIGRATION_REQUIRED`, a distinct outcome with no
migration path implemented yet.

When cross-version handoff is actually needed, it gets its own object rather
than a loosening of the predicate:

```text
HandoffMigration {
    from_schema_version / from_canonicalization_version
    to_schema_version   / to_canonicalization_version
    migration_recipe_id + version
    pre_migration_digests[]
    post_migration_digests[]
}
```

with acceptance defined as equality of the **canonical normative records after
migration**, not of the bytes before it. Until that exists, the honest surface
is a refusal, not a comparison nobody has defined.

- `decisions_preserved` is equality over a **canonical** serialization, with
  `canonicalization_version` in the manifest. Undefined ordering is the classic
  source of both false diffs and false passes.
- A probe may be promoted to blocking **only if it is itself deterministic** — a
  structural API call with an exact expected output qualifies; anything with a
  model in the loop does not.
- Ten passing probes never substitute for one missing normative decision, and a
  failing probe never invalidates a valid exact handoff. State equivalence is
  correctness; behavioural similarity is diagnostics.

**Acceptance property (provenance noted, derivation independent):**

```text
restore(snapshot) × N  →  compile(same inputs)  →  byte-identical output × N
```

*Provenance:* this test is motivated by a documented third-party failure class —
§1.3, `build_handoff_context()` producing four different error-pattern sets from
four restores of one unchanged state. *Derivation:* the requirement itself
follows from REQ-9 alone and would exist without that citation. The citation is
evidence that the failure class is real and cheap to hit, not authority for the
requirement.

### 4.9 The advisory plane, fenced

```text
advisory signal (embeddings · similarity recall · drift · attractors)
    cannot:
      - establish a fact
      - satisfy an invariant
      - preserve a decision
      - authorize an action
      - complete a handoff
```

This is `docs/evidence-and-decision-discipline.md`'s cross-cutting constraint
applied to memory reads: a retrieval score is a raw transport signal, and it
needs a typing step before any policy may read it.

Goal drift specifically **leaves the memory subsystem**. It is a monitor over
the goal graph, not a memory feature:

```text
cosine(current_work, active_goal)  →  possible drift        (advisory alarm)
                                          │
                                          ▼
        is the current task required by / reachable from the active goal?
                                          │
                              exact GoalGraph answer        (the verdict)
```

Every emitted observation is versioned, because a threshold means nothing
without the model it was tuned against:

```text
DriftObservation {
    embedding_model_id
    embedding_model_version
    detector_version
    threshold_profile
    score
}
```

## 5. Open risks

Named because a requirements document that lists only solved problems is a
brochure.

1. **Symbol identity is the expensive part.** §4.6 degrades gracefully instead of
   solving it. On a large refactor the honest outcome is a burst of `UNRESOLVED`
   bindings requiring human or explicit re-binding decisions. If that burst is
   large enough to be routinely dismissed in bulk, the mechanism has failed
   regardless of its correctness.
2. **Closure blow-up is likely, not hypothetical.** The bounds in §4.3 will fire.
   Whether `SCOPE_EXPANSION_UNSATISFIABLE` is a rare signal or a daily
   annoyance depends entirely on how narrowly contracts bind to artifacts, which
   is not yet designed.
3. **Re-verification has a real budget.** REQ-5 and §4.7 create a queue of work
   proportional to change rate × binding count. `cost_class` is a plan for
   scheduling it, not a plan for paying for it.
4. **Canonicalization is a versioned contract with a migration story.** Changing
   `canonicalization_version` invalidates comparability of every stored digest.
5. **The registry can become bureaucracy.** Rule 3 already warns against a
   register with "priestly ambitions". A failure lifecycle with four transition
   guards is exactly the kind of thing that acquires ceremonial states nobody
   uses. Guard: if a state has no consumer that behaves differently because of
   it, delete the state.
6. **This document overlaps two drafts that are themselves pending, and three of
   the conflicts are already visible.** Nothing here reconciles them; ratifying
   any of the three documents makes the following a required consistency pass,
   named now so it is not rediscovered later:

   | # | Conflict | Where | Proposed resolution |
   |---|---|---|---|
   | C-1 | `superseded` and `rejected` sit in the **trust levels** list, but they are dispositions, not statements about who vouched for an entry | `docs/agent-memory-layer.md` → "Trust levels" | Split the enum: trust (`agent-claimed` … `human-confirmed`) stays; `superseded` / `rejected` move to a status/lifecycle field aligned with §4.5. This is a **change** to the existing model, not an addition to it |
   | C-2 | IR requirements demand "a stable identity" per selected item; §4.6 here refuses to promise stable symbol identity and replaces it with a resolution ladder | `docs/task-aware-context-generator.md` → "IR requirements" | Replace the requirement with the `SymbolLocator` + `ResolutionResult` contract, so a degraded match is visible rather than assumed |
   | C-3 | The existing cache key (commit, task hash, profile, extractor versions, ranking version, budget config) and REQ-9's two input tuples (§4.0) are different closures over overlapping inputs | `docs/task-aware-context-generator.md` → "Determinism and reproducibility" | Reconcile into one declared closure per stage; whichever survives must contain **every** versioned component it invokes |

## 6. Non-normative evaluation of Semvec

Fenced deliberately, so that no later reader can convert "we evaluated it" into
"the architecture depends on it".

- **Out of the Memory Plane implementation plan.** If run at all, this is a
  black-box comparison producing a written verdict, not a dependency.
- **Two version facts, not one.** `>= 0.8.5` is a **minimum admissible
  version**, not a pin — every earlier version, including the 0.7.0 the article
  used, carries the documented restore and boundedness defects in §1.3. A range
  is not a revision binding: an evaluation whose dependency can float to a
  release nobody assessed would break rule 4 inside an exercise about rule 4.
  So any run also records an **exact run identity**:

  ```text
  minimum_admissible_version   >= 0.8.5          (admission policy)
  run_identity                 exact version
                             + wheel sha256
                             + platform tag
                             + python identity   (what was actually executed)
  ```

  Findings are bound to `run_identity`, never to the range.
- **Community tier only**, whose documented use cases explicitly include
  evaluation and prototyping — and note the 5 QPS ceiling shapes what can be
  measured.
- **Do not report `calculate_*` diagnostics as findings.** Their absolute values
  are licence- and subject-dependent by the vendor's own documentation, so on
  Community they are not measurements of anything transferable.
- **No material from the ARR integration repository.** See the red line.
- **Nothing measured this way may enter 007 as evidence** — it is a comparison
  of an external product, and per REQ-8 an external advisory system is not an
  authority.

Independently of the outcome: the vendor's retention scoring, phase detection
and eviction sit inside a declared patent boundary and are therefore
undocumented. A component whose selection logic cannot be inspected fails REQ-3
before any licensing or patent question is reached. The commercial and legal
arguments are real, but they are the second reason, not the first. For any
actual product decision, freedom-to-operate analysis belongs with a specialist,
not with an architecture note.

## 7. Disposition

```text
Source / factual record (§1)             RECORDED, revision-bound (rule 4)
Demo evidence assessment (§2)            RECORDED — not qualifying evidence
REQ-1 … REQ-11 (§3)                      PROPOSED — pending ratification
Candidate architecture (§4)              CANDIDATE — not adopted, not built
Open risks (§5)                          RECORDED
Semvec evaluation (§6)                   NON-NORMATIVE, optional, fenced
```

Per rule 3, everything marked `PROPOSED` or `CANDIDATE` above is agent-authored
and stays `pending` until a human ratifies or rejects it, and may not be cited
as authority for a further autonomous decision in the meantime. The suggested
sequence is to ratify or reject §3 first — requirements are cheap to argue about
and expensive to retrofit — and only then to decompose §4 into implementable
slices.
