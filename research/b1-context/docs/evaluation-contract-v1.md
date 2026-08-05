# Evaluation contract v1 — design (R4A, design-only)

**Status: DESIGN FREEZE. No evaluator code was written or executed in R4A.** This
document defines what evaluation *means* for a task-conditioned projection under a
capability-based, deterministic, structural contract. It does **not** modify
state-observables/v0 or selector/v0, and it does **not** redefine `o7.b1.report/v1`.

Bound to the source-complete R3.1 baseline (artifact `3bf00a67`, receipt
`0f78e1cd`, report `sha256:7f72133a…`). `case_role: development`,
`designed_after_observing_case_0002: true`, `holdout_evidence: false`,
`generalization_claim_allowed: false`.

Artifacts: `schema/evaluation-contract-v1.schema.json` (schematizes both the
contract instance `o7.b1.evaluation-contract/v1` and the output `o7.b1.evaluation/v1`),
and `fixtures/case-0002/evaluation-contract-v1.yaml` (the case-0002 instance).

## 1. Capability-based arms

```
required_arms:  full_derived, projection
optional_arms:  negative_control
```

- A **required** arm that cannot be constructed → `UNAVAILABLE`.
- A **missing optional** arm (case-0002 has `negative_control: []`) → `NOT_APPLICABLE`
  — never zero, never `PARTIAL`, never a ceremonial `PASS`.
- Non-applicable metrics serialize as `null`, never `0`.
- No synthetic negative-control arm is ever fabricated.
- The gold state is an **oracle/reference**, not an evaluated arm.

This replaces v0's structural coupling to three fixed arms and its unconditional
`negative_control[0]` dereference (the R3 defect's cousin). Wrapping that access in
an `if` would make it *run*; this contract defines what it *means*.

## 2. Metric families (kept separate)

**Source compilation** — requested source availability; derived-session
completeness; extractor identity; source and derived digests.

**Structural observation support** — per task and per question: required
observation IDs, selected required IDs, missing required IDs,
required-observation coverage, fully-supported question count. These are
**structural** metrics. They are **not** semantic recall and **not**
answerability; no LLM judge is used.

**Projection validity** — reuses the existing `validity.py` meanings (required
topics/kinds, in-force status, authority eligibility, contradiction checks,
provenance completeness, record/byte budget compliance). Selector semantics are
unchanged.

## 3. Relation requirements (depth-1, explicit)

Each requirement is `{from, kind, direction: outgoing, depth: 1, match: all,
endpoint_policy, gold_derived_targets}`. **Targets are derived from the frozen
gold-state relation graph, never from any observed projection.** Two endpoint
policies:

- **`edge_witness`** — every matching exact gold edge must appear in the projected
  relation set. A stale/ineligible target may remain unselected, but it must exist
  in the gold state and the emitted edge must be exact. Used where the target is
  legitimately superseded.
- **`all_current_targets_materialized`** — every matching **current/pending**
  target must both appear as an exact edge **and** be selected as an observation.

Rules: an edge absent from the gold state is **fabricated** → fail closed. A
relation-only reference to a stale observation does **not** count as presenting it
as current. **No transitive/multi-hop closure — depth is exactly 1** unless a
future contract explicitly defines traversal.

case-0002 (derived from the frozen gold graph):

| task/question | from | kind | endpoint policy | gold target(s) | target status |
|---|---|---|---|---|---|
| audit/qa3 | obs-round0-closed | supersedes | edge_witness | obs-plane-record-not-frozen-v6 | superseded |
| audit/qa3 | obs-reviewer-durability-resolved | supersedes | edge_witness | obs-reviewer-durability-unresolved-v6 | superseded |
| audit/qa3 | obs-profiler-fail-closed | supersedes | edge_witness | obs-profiler-prior-defect | superseded |
| impact/qi1 | obs-oracle-topology-constraint | depends_on | all_current_targets_materialized | obs-repo-authority-04108e7, obs-window-invariant, obs-output-path-dependence | current |

The audit supersession targets are all stale, so `edge_witness` is correct (the
projected context legitimately carries supersession edges to omitted stale
observations). The impact dependency targets are all current, so they must be
materialized.

## 4. Forbidden stale-as-current

A listed forbidden observation presented in the **selected** set as `current` or
`pending` is a **stale-state-safety FAILURE**. Reported per forbidden id with one
classification: `absent`, `stale_relation_endpoint_only`,
`selected_with_stale_status`, `selected_as_current_or_pending`. Only the last is a
failure; a relation-only stale reference is allowed. Projection validity may
independently reject a selected superseded/rejected record.

case-0002 forbidden lists (frozen questions): audit/qa4 →
`obs-reviewer-acceptance-claim`, `obs-plane-record-not-frozen-v6`,
`obs-reviewer-durability-unresolved-v6`; resume/qr2 →
`obs-plane-record-not-frozen-v6`.

## 5. All-task dependence + external budget

**Task dependence** is computed over **every unordered task pair**, not only the
first two (v0's latent limitation). Per pair: set equality, subset/superset,
symmetric-difference count, context-digest equality, and whether the difference is
selector relevance or budget-only. Intent-level shapes are frozen (not exact IDs
or counts):

```
resume__audit:  materially_different
audit__impact:  left_strict_superset   (audit ⊃ impact)
resume__impact: materially_different
```

Shape semantics: `materially_different` = sets differ and are not equal;
`left_strict_superset` = left contains every right item plus ≥1 more; unknown
shape names fail closed.

**Budget** is bound to the committed `fixtures/case-0002/budget-v0.yaml`
(digest-bound) with unchanged values `byte_budget: 20000`, `record_budget: 32`,
`unit: utf8_bytes+records`. No tuning in R4A.

## Outcome lattice

Independent axes: `source_compilation`, `projection_validity`,
`question_observation_support`, `relation_support`, `stale_state_safety`,
`task_dependence`, `negative_control_diagnostics` — each `PASS | FAIL |
UNAVAILABLE | NOT_APPLICABLE`.

Overall `PASS | FAIL | UNAVAILABLE`:
- `PASS`: every required family is applicable and passes.
- `FAIL`: at least one applicable required gate fails.
- `UNAVAILABLE`: a required input or arm cannot be constructed.
- An absent optional negative-control arm does **not** prevent `PASS`.
- Optional diagnostic families **cannot** upgrade a failing required gate.
- Generation success is separate from evaluation `PASS`.
- **`PARTIAL` is not used in evaluation v1** — it is preserved only as a legacy
  `report-v1` field until a later integration decision.

## Output contract — `evaluation-v1.json`

Standalone, computable from frozen inputs **without** rerunning extraction or
projection. Contains: contract id + digest; evaluator impl digest (placeholder in
R4A); exact input artifact digests; arm availability; per-task results; per-question
observation support; per-question relation support; stale-state checks; the
all-pairs task-dependence matrix; budget identity; the outcome axes; the overall
outcome; and explicit metric-semantics prose.

## Migration and compatibility

- Evaluation v0 remains available for historical case-0001 reproduction; no
  existing case-0001 report bytes change.
- R3 and R3.1 remain immutable.
- Evaluation v1 initially produces a **separate** artifact (`evaluation-v1.json`).
- Integration into `report.json` requires a later **explicit report-contract
  revision**; `o7.b1.report/v1` is NOT redefined here.

## Unresolved design issues (recorded, not invented)

1. **Byte-budget counted representation.** v0 selection counts per-record canonical
   bytes + 1 (newline) for the byte budget and record count = number of selected
   records. Whether the byte budget should instead count the rendered `context.json`
   total is unresolved; carried to R4B rather than decided here.
2. **`full_derived` arm structural support metric.** Under v0 the full-derived arm
   carried no compiled observations; how (or whether) v1 credits raw-source presence
   distinctly from compiled support is deferred to R4B, kept as a structural, not
   semantic, measure.
3. **Multi-hop relations.** Explicitly out of scope; depth-1 only until a future
   contract defines traversal.

## Sequencing after R4A

R4B implements a standalone evaluator against **synthetic** fixtures first; only
R4C applies it to the already-frozen R3.1 bytes; R4D is optional report
integration. Designing the ruler, building the ruler, and measuring are kept
distinct — so the result cannot come out exactly the size anyone hoped for.
