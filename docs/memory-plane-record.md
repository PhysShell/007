# Memory Plane for 007 — evidence record, requirements, candidate architecture

- **Status:** split. **§3 (REQ-1 … REQ-11) is ratified**; everything else in this
  document remains agent-authored and **`pending`**
  (`docs/evidence-and-decision-discipline.md` rule 3). This document is **not an
  approved architecture**: §4 has no authority, and no part of it acquires any
  by satisfying a ratified requirement.
- **Ratification:** the maintainer ratified **§3, REQ-1 … REQ-11, and only
  those**, in an interactive session on **2026-08-10**, under rule 3's carve-out.
  - **Ratified design revision**, bound by three identities rather than one,
    following `docs/architecture/prior-art-the-grid.md` §1.3 — a commit ID alone
    depends on that commit staying reachable, and the thing actually ratified is
    the text, not the container:

    ```text
    H  ratified head   219953efac10bf689738a4749cbcc09e02df737c
                       the revision the maintainer ratified against

    B  file blob       e75d842bdf11d22bae5ff9f88858da51c1e97cc2
                       sha256:b8f40d033c5c6e0b41cec242b024e62dad58439bd3676cf99e39cbe698c39560
                       docs/memory-plane-record.md at H — 1119 lines, 57844 bytes

    S  ratified text   sha256:cb2c12a6c9a97061c762299d7358c2cb3d297eb266320fc019c95caeeb3ad089
                       §3 alone, from "## 3. Requirements" to the line before
                       "## 4. Candidate architecture" — 58 lines, 6317 bytes.
                       Byte-identical at H and at every later revision of this
                       branch, which is what makes the ratification checkable
                       without resolving H at all.

    M  incorporation   a76b83c9bac945ac32806573470704ead2ee47ff
                       "Merge pull request #125" — what proves `H` is reachable
                       from `main`. Second parent `71bae80`; `H` verified as an
                       ancestor of `main`, and `S` on `main` is unchanged. A
                       merge commit is not the merged head: this is the only one
                       of the four that establishes incorporation.
    ```

    Per rule 4 the ratification is bound to that text: if any requirement is
    reworded, `S` changes, the new wording is not covered, and that requirement
    returns to `pending`. Editorial changes elsewhere in this file leave `S`
    intact and do not disturb it. `H` must stay reachable from `main` — this
    branch is merged with a merge commit, as every prior PR in this repository
    was; a squash would collapse the history `H` names, and `S` is the anchor
    that survives it.
  - **Not ratified by that decision:** §4 candidate architecture; the proposed
    resolutions C-1 … C-4 in §5; §6, which is non-normative by construction.
  - **How the decision was reached, with the roles kept apart:** four review
    rounds, the last two requirements-only (see the round notes below). The
    final round produced a reviewer verdict of `PASS` with a **recommendation**
    to ratify — a recommendation is not authority under rule 3, and the
    `PROPOSED → RATIFIED` transition was made by the human maintainer, not by
    the agent that authored the document or by the one that reviewed it.
- **Date:** 2026-08-09; ratification record added 2026-08-10. External claims are
  bound to the versions named in §1 and are `STALE` the moment those move
  (rule 4).
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
- **Review round 3 (2026-08-09), requirements-only:** §3 was reviewed *as a
  ratification candidate* rather than as design. REQ-2, REQ-6 and REQ-10 stood
  unchanged; the other eight were rewritten so that each survives §4 being
  rejected — naming admission classes instead of `InclusionProof`, authority-
  appropriate revision identity instead of "revision and digest", deterministic
  stages instead of two named functions, advisory *signals* instead of
  *semantic* retrieval, and derivation inputs instead of authoritative state
  alone. REQ-1 now names failure-lifecycle state and is checked by recomputing
  identities from imported records, since two matching receipts over two missing
  stores also compare equal. No architecture changed in this round; §4 gained
  only cross-references and the C-4 conflict against the frozen A1 digest
  discipline.
- **Review round 4 (2026-08-09), two mechanical edits:** the ratification review
  found no remaining objection to the eleven requirements themselves, and two
  places where §3 still pointed *out* of itself — REQ-6's oracle cited §4.5, and
  REQ-9's note fixed a concrete output shape and a compatibility obligation
  toward a draft. Both now stay inside §3; the concrete forms moved to §4 and to
  C-3. Round 3 established that a requirement must survive §4 being rejected,
  and these were the last two places where its own oracle did not.

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

Each requirement is stated so that it survives the architecture in §4 being
rejected. Where a candidate mechanism satisfies one, the mechanism is named in
§4 and not in the requirement — a requirement that names a data type ratifies
that data type by the back door.

| # | Requirement | Checkable by |
|---|---|---|
| REQ-1 | **All** normative state required to continue a unit of work survives a session boundary **exactly**, not approximately — goals, decisions, invariants, evidence bindings, and failure-lifecycle state | export → import → recompute the canonical normative identities **from the imported records** → equality with the source identities. Manifest-to-manifest byte equality alone is not sufficient: two identical receipts over two missing stores also compare equal |
| REQ-2 | Context construction is **bounded** by a declared budget, and the bound is a property of the compiler, not of the caller's restraint | compiler refuses rather than exceeds |
| REQ-3 | Every item admitted to agent-visible context carries a machine-readable inclusion reason identifying its **admission class** and the rule or selection channel responsible for it | every admitted item has a reason record; admission class is one of the declared classes; no item is admitted without one |
| REQ-4 | Evidence is bound to the authoritative artifact and to the exact **revision or content identity appropriate to that authority**, sufficient to invalidate the claim when the relevant artifact changes | schema-level; no unbound evidence admitted |
| REQ-5 | A previously verified binding that ceases to be current carries a **typed invalidation cause**, and `STALE` / `UNRESOLVED` / `INVALID` remain distinguishable outcomes | mutate subject, recipe, environment and dependency independently; each produces its own typed cause, and an unresolvable subject does not masquerade as a stale one |
| REQ-6 | Failure knowledge has a **lifecycle**, and state transitions require evidence | negative fixtures: a transition lacking the evidence its declared lifecycle policy requires is rejected; positive fixtures: evidence-backed transitions are accepted |
| REQ-7 | Handoff acceptance is a **deterministic function of a complete set of recorded inputs**: re-evaluating the same recorded input identities under the same acceptance-policy identity produces the same result. Stochastic or ambient observations may not gate acceptance | re-evaluation equality, plus negative fixtures — no model call, no clock read, no ambient network or filesystem fact. An artefact that cannot be read yields `UNAVAILABLE`, never a rejection |
| REQ-8 | **Advisory retrieval or selection signals** may advise but never establish: they cannot create a fact, satisfy an invariant, preserve a decision, authorize an action, or complete a handoff | trust-boundary test per consumer, applied to every retrieval channel — lexical, structural and graph-based alike, not only embedding-based |
| REQ-9 | Every deterministic derived stage on the context path declares a **complete, versioned input closure**. Given identical recorded input identities and the same stage/policy identity, its output is reproducible and diffable | re-run each stage → byte-identical output + comparable metadata; and a meta-test: **declared stage inputs == fields committed by the invocation manifest** |
| REQ-10 | No component on the read path of normative state may depend on an external entitlement — licence, quota, remote availability, or token TTL | dependency audit of the read path |
| REQ-11 | Every derived normative projection is **disposable**: deletable and rebuildable digest-identically from authoritative state **plus its complete recorded derivation inputs**, including generator/policy identities and parameters | delete the derived tree, rebuild from the recorded inputs, compare digests |

Three notes that are load-bearing:

- **REQ-9 is stronger than "deterministic", and it is stated over *stages*, not
  over two named functions.** Reproducibility alone lets you rebuild a black
  box; a stage's output must also be *comparable*. For a context-producing
  stage that means a machine-readable comparison surface sufficient to identify
  the exact output and to explain both admissions and omissions. Which
  artifacts carry it, and under what field names, are implementation choices —
  see §4 for the candidate shape and C-3 in §5 for reconciliation with the
  existing `context.meta.json` field list. The requirement also does not say how
  many stages exist: §4's `R` and `C` are one way to satisfy it, and ratifying
  REQ-9 must not ratify that decomposition.
- **REQ-9 is the expensive one, and two review rounds argued for it rather than
  against it.** Both rounds found the same defect class in this document's own
  architecture — a versioned component that changed a result while sitting
  outside the declared input closure. That is the normal state of an
  engineering design unless something hunts for it, which is why the meta-test
  in the `Checkable by` column matters more than the prose: declared inputs
  compared against the fields the invocation actually commits, so the discipline
  does not depend on the next reviewer's attention span.
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

*This section is how the **candidate** architecture satisfies REQ-9. REQ-9
itself requires a complete versioned input closure per deterministic stage; it
does not require that there be exactly two stages, nor that they be called `R`
and `C`. Ratifying the requirement does not ratify this decomposition.*

The candidate plane has **two** deterministic stages, and each must close over
its own inputs. Stating one tuple for both is how a versioned component drops
out of a reproducibility claim without anyone noticing.

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
    failure_registry_digest
}
```

Their identities are computed by explicit field framing — **§4.0.2**, not by
serializing either structure.

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

The candidate comparison surface a compilation emits, which is what makes REQ-9's
*diffable* half concrete here rather than in the requirement:

```text
context_bytes · context_digest · included_entry_ids · inclusion_reason[] ·
omitted_candidate_ids · token_count
```

Reconciling these with the `context.meta.json` field list in
`docs/task-aware-context-generator.md` is C-3 in §5, and belongs to the
consistency pass after ratification — not to §3.

#### 4.0.1 Advisory inputs are frozen before they are compiled

Making semantic retrieval part of normative state would buy determinism by
destroying the trust boundary §4.9 exists to hold. The alternative is to leave
retrieval advisory *and* non-deterministic, and to materialize its result before
the deterministic stage consumes it:

```text
AdvisoryInputSnapshot {
    items[]                a sequence — the order the retrieval emitted them
                           is kept, and is framed in that order (§4.0.2)
    provenance[]           which channel proposed each item
    retrieval_identity     retriever + version + embedding model identity
    snapshot_digest        framed per §4.0.2, never a serialization
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

#### 4.0.2 Identity framing

Every identity this section owns is computed by **explicit length-prefixed field
framing**, following the discipline frozen in
`docs/q-deck/a1-authority-contracts.md` **FD-1.2**. An earlier revision of this
document wrote `digest(canonical(X))`, which is precisely the phrasing FD-1.2
refuses; that was conflict **C-4**, and this subsection closes it. The frozen
contract wins — the candidate moved.

```text
h = SHA-256
h.update(b"o7-memory-<family>\0v1\0")     domain separator, one per family
frame(field), … in a fixed order that never changes

frame(x)         = u64-le length prefix || bytes        (identical to FD-1.2)
strings          UTF-8 bytes
enums            framed by their stable snake_case name, never by tag byte
collections      frame(count as u64-le) then each element in the order
                 defined for that field
child digest     framed as its raw digest bytes
```

**Every integer names its width.** "Little-endian fixed width" is not a width,
and `x.to_le_bytes()` takes its width from a Rust type the document does not
fix — so an implementation with a `u32` count and one with a `u64` count could
both claim conformance and compute different digests. That is precisely the
class of ambiguity FD-1.2 exists to remove, so the widths are frozen per field
class and no bare `.to_le_bytes()` appears in any framing below:

```text
u32-le   schema_version · canonicalization_version ·
         scope_key_canonicalization_version · depth · rank ·
         max_derivation_depth · max_required_entries
u64-le   every collection count · token_count · total_budget · output_reserve
```

**Optionals.** An absent optional framed as the empty string hashes *identically*
to a present empty value — the two are one preimage, and no framing may pretend
otherwise. An earlier revision of this subsection claimed all three of "absent",
"empty" and "some value" hash distinctly; only the third is separated by this
rule. No framing in this section currently carries an optional field. Any future
one that must distinguish absent from present-empty carries an explicit presence
discriminator framed ahead of the value; it does not get the distinction for
free.

**No canonical-JSON scheme is introduced, here or anywhere below.** Nothing in
this section requires two serializers to agree on key order or whitespace.

**Owned versus foreign.** This section defines a preimage only for the
identities it owns. A digest that belongs to another object is framed as bytes
and its discipline is left where it lives — capturing someone else's identity
rules is how two incompatible definitions of the same digest appear.

```text
owned here      state envelope · resolver policy · scope key · resolved scope ·
                advisory input snapshot · budget profile · compiled context

foreign         the five partition digests (goals, decisions, invariants,
                evidence, failure registry) · artifact and evidence digests ·
                environment_digest · tokenizer and embedding-model identities
```

The five partition digests are an **open dependency**: their framing belongs to
the partition stores, which do not exist yet. Until they do, `canonical_state_
digest` is well-defined *given* them and no further — stated rather than papered
over.

```text
canonical_state_digest              domain b"o7-memory-state\0v1\0"
    frame(schema_version as u32-le)
    frame(canonicalization_version as u32-le)
    frame(goal_state_digest) frame(decision_state_digest)
    frame(invariant_state_digest) frame(evidence_state_digest)
    frame(failure_registry_digest)

resolver_policy_digest              domain b"o7-memory-resolver-policy\0v1\0"
    frame(resolver_version) frame(witness_rule_version name)
    frame(closure_rule_set_version)
    frame(max_derivation_depth as u32-le)
    frame(max_required_entries as u32-le)

canonical_scope_key digest          domain b"o7-memory-scope-key\0v1\0"
    frame(scope_key_canonicalization_version as u32-le)
    frame(goal_node_id)
    frame(artifact_ids count as u64-le), each in §4.1.1 order
    frame(contract_ids count as u64-le), each in §4.1.1 order

resolved_scope_digest               domain b"o7-memory-resolved-scope\0v1\0"
    frame(canonical_scope_key digest) frame(resolver_policy_digest)
    frame(required entries count as u64-le), each entry in ascending bytewise
        order of entry_id, and for each:
        frame(entry_id)
        frame(proofs count as u64-le), each proof in witness order (§4.2.1),
            and for each proof:
            frame(rule_id) frame(depth as u32-le)
            frame(derivation_path hop count as u64-le), each hop in path
                order — a derivation path is a sequence and keeps its order

advisory_input_snapshot_digest      domain b"o7-memory-advisory-snapshot\0v1\0"
    frame(retriever_id) frame(retriever_version)
    frame(embedding_model_identity)      — foreign bytes, framed as they arrive
    frame(items count as u64-le), each in the order the retrieval emitted
        them, and for each:
        frame(item_id) frame(provenance channel name) frame(rank as u32-le)

model_budget_profile_digest         domain b"o7-memory-budget-profile\0v1\0"
    frame(tokenizer_id) frame(total_budget as u64-le)
    frame(output_reserve as u64-le)

compiled_context_digest             domain b"o7-memory-compiled-context\0v1\0"
    — the `context_digest` of the comparison surface above is this value; one
      identity, not two names
    frame(context_bytes)
    frame(included entries count as u64-le), each in the order the entry
        appears in the context — presentation order is semantic here — and
        for each, id and reason framed together as ONE record, with
        admission_class serving as the union discriminator:
        frame(entry_id)
        frame(admission_class name)
        if admission_class = normative:            — InclusionProof (§4.2)
            frame(proofs count as u64-le), each in witness order (§4.2.1):
                frame(rule_id) frame(depth as u32-le)
                frame(derivation_path hop count as u64-le), each hop in
                    path order
        if admission_class = advisory:             — AdvisoryInclusionReason
            frame(selection_channel name)
            frame(rank as u32-le)
            frame(advisory_input_snapshot_digest)
    frame(omitted_candidate_ids count as u64-le), each in ascending
        bytewise order — an omission set has no natural order
    frame(token_count as u64-le)
```

**Why the reason is a tagged union.** §4.2 declares two admission classes with
two different reason records — `normative` carries an `InclusionProof`
(`rule_id`, `derivation_path[]`, `depth`), `advisory` carries an
`AdvisoryInclusionReason` (selection channel, rank, snapshot identity). One
flat shape cannot hold both, and C-4.1's attempt to do so failed in both
directions at once: it dropped `derivation_path` from the normative case — so
two proofs differing only in the path they took collided, even though
`resolved_scope_digest` frames that path correctly — and it left the advisory
case unrepresentable, since after `admission_class = advisory` the format still
demanded proofs.

`admission_class` is therefore the discriminator as well as a recorded fact.
Framing it before the variant is what keeps the preimage unambiguous: a reader
knows which arm follows before it has to parse the arm, and no byte sequence is
valid under both.

**Why the inclusion reason is inside the entry record.** The comparison surface
above lists `inclusion_reason[]` as part of what a compilation emits, so REQ-3
and C-3 make the reason part of the machine-comparable result — and an identity
that omitted it would give one digest to two compilations that admitted the same
text for different reasons, which is the failure the requirement names. It is
framed *within* each entry's record rather than as a parallel array, because two
parallel arrays are a pair of things that can fall out of step, and a digest that
commits both independently would not notice.

**Ordering is declared per field, never assumed.** Where a collection is a
sequence — a derivation path, the items of a snapshot, the entries as they
appear in a rendered context — its order is semantic and is preserved. Where it
is a set — artifact ids, contract ids, required entries, omitted candidates —
the order is *defined*: ascending bytewise comparison over the UTF-8 encoding of
the identifier, after deduplication. "Sorted" without a key and an encoding is
canonicalization wearing a false moustache, and it is not admitted here.

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
ordered by **ascending bytewise comparison over the UTF-8 encoding of the
identifier** — the set rule of §4.0.2, named here rather than left as "sorted" —
and stamped with `scope_key_canonicalization_version`. Two callers naming the
same artifacts in a different order must produce the same `canonical_scope_key`
and therefore the same `resolved_scope_digest`. A list whose order is an
accident of iteration is an unversioned input in disguise.

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

REQ-3 covers **everything** admitted to agent-visible context, so the advisory
half needs its own reason record rather than borrowing this one:

```text
admission class     reason record
normative           InclusionProof          rule + derivation path (§4.2)
advisory            AdvisoryInclusionReason selection channel + rank + snapshot
                                            identity (§4.0.1)
```

`provenance` answers *where an item came from*; an inclusion reason answers *why
it is in this context*. Those are different questions, and only the second one
makes a context auditable.

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
ALL_MINIMAL     record every minimal inclusion reason, in witness order
SINGLE_TIEBREAK record one witness, selected by a declared total order over
                (depth, rule_id, derivation_path) — never by traversal order
```

**Witness order** is that same total order, and it is stated once here because
both rules need it: ascending by `depth` (u32), then by `rule_id` (ascending
bytewise over its UTF-8 encoding), then by `derivation_path` (element-wise
ascending bytewise, shorter path first on a common prefix). `SINGLE_TIEBREAK`
takes the first element under it; `ALL_MINIMAL` frames all of them in it
(§4.0.2). An earlier revision said "canonical order" for `ALL_MINIMAL` and left
it at that, which names an intention rather than an order — and a witness rule
whose own ordering is undeclared reintroduces exactly the non-determinism this
subsection exists to remove.

An entry with more than one minimal proof is therefore representable: under
`ALL_MINIMAL` `resolved_scope_digest` frames a proof *count* per entry and then
each proof, so two entries differing only in how many ways they were reachable
have different identities. The earlier framing carried one `rule_id` and one
path per `entry_id`, under which the second proof either could not be
represented or was silently dropped at deduplication.

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
recomputed = recompute_partition_digests(imported records)
                                       ↑ from the records themselves,
                                         never read back from the manifest

HANDOFF_ACCEPTED  iff  schema_version_equal
                   AND canonicalization_version_equal
                   AND recomputed.goal_state_digest        == manifest.goal_…
                   AND recomputed.decision_state_digest    == manifest.decision_…
                   AND recomputed.invariant_state_digest   == manifest.invariant_…
                   AND recomputed.evidence_state_digest    == manifest.evidence_…
                   AND recomputed.failure_registry_digest  == manifest.failure_…
                   AND recomputed.canonical_state_digest   == manifest.canonical_…
                   AND evidence_refs_resolvable
                   AND artifact_digests_valid          ← all deterministic

SemanticContinuityAssessment → PASS / WARN / INCONCLUSIVE
                                                       ← never gates
```

**The comparison is manifest-against-recomputation, never manifest-against-
manifest.** A handoff that carries the manifest and loses the goal, decision,
invariant, evidence or failure-registry records behind it produces two identical
receipts over two different states, and a byte comparison of the receipts
accepts it. REQ-1 rules that out by name — its oracle recomputes the canonical
identities *from the imported records* — so the predicate does the same, and
covers the failure registry alongside the other four partitions because REQ-1
counts failure-lifecycle state as normative state.

**v0 is same-schema, same-canonicalization only, and says so.** Within one
schema and canonicalization version, the recomputed identities are compared
byte-for-byte; the word *compatible* would quietly widen that into an undefined
compatibility relation, which in a document this strict is a promise with no
semantics behind it. A version mismatch in v0 is not a failed handoff — it is
`HANDOFF_MIGRATION_REQUIRED`, a distinct outcome with no migration path
implemented yet.

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

- There is no `decisions_preserved` predicate. An earlier revision defined one as
  "equality over a canonical serialization", which C-3 removed from
  `HANDOFF_ACCEPTED` and C-4 forbids outright (§4.0.2 admits no canonical-JSON
  scheme). What gates instead is above and is already exact: each recomputed
  partition digest equals its manifest value, alongside
  `canonicalization_version_equal`. Decisions are covered by
  `recomputed.decision_state_digest == manifest.decision_state_digest`, not by a
  serialization anybody has to agree on twice.
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
advisory signal — any retrieval or selection channel, not only the vector ones
    (embeddings · lexical/BM25 · graph traversal · drift · attractors)
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
   the conflicts are already visible.** They were named here before ratification
   and resolved after it, in that order — the pass could not run earlier without
   importing a pending proposal into ratified surroundings.

   **Status: all four are closed.** C-1, C-2 and C-3 by `a92a707`, `1257669` and
   `4f746b6`, each a single-purpose commit, each carrying the ratified
   requirement that compels it and each explicitly declining to import the
   candidate types from §4. C-4 came later and on its own branch, because it
   changes §4 itself rather than the neighbouring drafts — a notarial record, a
   consistency fix and an architectural edit have different reasons to be
   reverted and do not belong in one commit.

   | # | Conflict | Where | Proposed resolution |
   |---|---|---|---|
   | C-1 | `superseded` and `rejected` sit in the **trust levels** list, but they are dispositions, not statements about who vouched for an entry | `docs/agent-memory-layer.md` → "Trust levels" | Split the enum: trust (`agent-claimed` … `human-confirmed`) stays; `superseded` / `rejected` move to a status/lifecycle field aligned with §4.5. This is a **change** to the existing model, not an addition to it |
   | C-2 | IR requirements demand "a stable identity" per selected item; §4.6 here refuses to promise stable symbol identity and replaces it with a resolution ladder | `docs/task-aware-context-generator.md` → "IR requirements" | Replace the requirement with the `SymbolLocator` + `ResolutionResult` contract, so a degraded match is visible rather than assumed |
   | C-3 | The existing cache key (commit, task hash, profile, extractor versions, ranking version, budget config) and the candidate input closures (§4.0) are different closures over overlapping inputs; separately, the existing `context.meta.json` field list and the candidate comparison surface (§4.0) describe the same output twice | `docs/task-aware-context-generator.md` → "Determinism and reproducibility" | Reconcile into one declared closure per stage — whichever survives must contain **every** versioned component it invokes — and into one comparison surface, rather than two field lists that drift |
   | C-4 | **Closed.** §4 wrote identities as `digest(canonical(X))`; 007 had **frozen** the opposite discipline — identities by explicit length-prefixed field framing, with "no canonical-JSON scheme is introduced" | `docs/q-deck/a1-authority-contracts.md` → FD-1.2 (frozen) | Done in §4.0.2: a domain separator and a fixed-order framing for each of the seven identities §4 owns, foreign digests framed as bytes with their discipline left where it lives, and set-versus-sequence ordering declared per field. The frozen document won; the candidate moved. §4 remains **candidate** — closing C-4 fixes a defect, it does not adopt the architecture |

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
REQ-1 … REQ-11 (§3)                      RATIFIED — maintainer, 2026-08-10,
                                           bound to 219953e (see the header)
Candidate architecture (§4)              CANDIDATE — not adopted, not built
Open risks (§5)                          RECORDED
Semvec evaluation (§6)                   NON-NORMATIVE, optional, fenced
```

REQ-1 … REQ-11 are now normative and may be cited as authority. Everything still
marked `CANDIDATE` or `RECORDED` above is agent-authored, stays `pending` under
rule 3, and may not be cited to justify a further autonomous decision — **and a
§4 construct does not inherit authority from satisfying a ratified requirement.**
Requirements constrain architectures; they do not bless the first architecture
that happens to meet them.

The order that keeps governance clean, and the reason for each step:

```text
1. ratify or reject REQ-1..11              requirements are cheap to argue
   DONE — maintainer, 2026-08-10            about and expensive to retrofit
2. record the exact ratified revision      rule 4: a ratification binds a
   DONE — 219953e, this commit              revision, not a title
3. C-1 / C-2 / C-3 consistency pass        only now may the two neighbouring
   DONE — a92a707, 1257669, 4f746b6         drafts be changed — before this,
                                           editing them would import a pending
                                           proposal into ratified surroundings
4. C-4: re-express §4 digests as explicit  the frozen A1 discipline wins;
   FD-1.2 framings — DONE, §4.0.2           §4 is what has to move
5. only then consider adopting or          §4 stays CANDIDATE until 1–4 are
   decomposing §4                          done
```

Steps 1 and 5 are the ones worth not blurring: ratifying requirements is not
adopting an architecture, and §4 remains a candidate however many review rounds
it survives.
