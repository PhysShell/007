# Holdout run verdict — frozen `qodec project` on H1/H2/H3

Run **after** the freeze (`holdout-freeze-manifest.json`), with the CI-qualified
producer **frozen and untouched**: Qodec `project` QA `6a5d4030` (binary `0a0996f3`)
+ adapter `o7.b1.qodec-project-arm/v0` (`f8fa87e3`). C1 was never used as input;
selector/projector v0, the evaluator, schema and contract semantics are unchanged.

This is a **synthetic producer-holdout**, not a real-source generalization holdout
(see `README.md`); it cannot support a generalization claim. Its job is to find where
the frozen producer breaks on shapes it was not built against. It did.

| case | shape | baseline | Q1R | relation_support | new failure |
|---|---|---|---|---|---|
| H1 | relation-light | PASS | **PASS** | (baseline passed) | — |
| H2 | relation-heavy | FAIL (relation_support) | FAIL | **REPAIRED** | task_dependence |
| H3 | budget-pressure | FAIL (relation_support) | FAIL | **REPAIRED** | projection_validity |

**What generalized.** On both failing cases the frozen producer **repaired the
relation-evidence loss** — H2 by materializing both supersession sources
(`obs-h2-super-src-1/2`), H3 by materializing `obs-h3-super-src` — every emitted
relation gold-grounded, no fabricated edge. The case-0002 capability reproduces on
new material.

**What it found (real edges, not a clean pass).**
- **H3 → budget-aware v1 is required.** The repaired `load` context is 9 records
  against the task's tight `record_budget: 8`, so the frozen evaluator rejects it on
  `projection_validity` (budget overflow). `qodec project` v0 is closure without
  eviction, fed the contract's loose budget; under real budget pressure the mandatory
  closure does not fit. This directly specifies the next feature: **`qodec project`
  v1 — constraint-aware selection under budget.** (Predicted.)
- **H2 → a methodology limit.** The repair changes the audit task's selection, which
  shifts the audit↔history task-dependence pair off the baseline-pinned `incomparable`
  expectation, so `task_dependence` fails. A valid *alternative* projection is
  penalized by a pair expectation pinned to one projection. Either the
  `task_dependence` pair_expectations must be projection-invariant, or holdout
  fixtures must not freeze baseline-specific pair shapes. (Surfaced.)

**Verdict.** `generalization: NOT_ESTABLISHED` — mixed results on 3 synthetic cases.
The frozen producer repairs relation-evidence loss on new material, and its two
concrete limits (budget, task-dependence interaction) are now specified rather than
speculative. A real-source holdout remains the only path to a generalization claim;
the next Qodec feature (`project` v1, budget-aware) is now justified by evidence, not
guessed.
