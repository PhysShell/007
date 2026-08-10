# H2 evaluator boundary — `task_dependence` retired as a non-normative shape metric

**Classification: `TASK_DEPENDENCE_RETIRED_AS_NON_NORMATIVE_SHAPE_METRIC`**
**`boundary_closed: true`**

Evaluator methodology only. No real-source holdout, no `qodec project v1`, no change to
Q1R/C1, gold or questions, and no historical verdict recomputed or rewritten.

## The defect

Rev3 gated `task_dependence` on whether two tasks' selected sets stood in a relationship
pinned into the contract (`expected_shape: incomparable`). H2 shows why that is a
measurement bug rather than a semantic one:

| | audit | history | shape |
|---|---|---|---|
| baseline projector v0 | 5 records | 2 records | `incomparable` ✓ pinned |
| **Q1R (qodec project)** | 7 records | 2 records | **`left_strict_superset`** ✗ FAIL |

Q1R's relation-aware closure legitimately admitted `obs-h2-super-src-{1,2}` into the audit
context. Both projections cover their own required observations; Q1R had *repaired*
`relation_support`. Nothing normative was violated — the axis was measuring which
optional-but-admissible records one projector happened to choose.

## Phase H2-A — is there an independent invariant? No.

Rather than reach for a replacement metric, each candidate was tested against a metamorphic
class of valid projections, built by varying only what the contract does not declare
mandatory (add optional admissible records, drop an optional record, reorder, saturate).

| candidate | verdict |
|---|---|
| selected-set **shape** (the rev3 rule) | **not invariant** — varies over `{incomparable, left_strict_superset, equal}` while every other required axis passes |
| "the two sets are **not identical**" | **not invariant** — saturating both sides makes them identical, all other axes still PASS |
| "each task's required observations present" | duplicate of `question_observation_support` |
| "required relation witnesses present" | duplicate of `relation_support` |
| "no stale material as current" | duplicate of `stale_state_safety` |
| "context within budget" | duplicate of `projection_validity` — this is exactly what H3 fails on |

Task obligations are normative and already owned. Which further admissible records a valid
projector includes is *realization*; the shape of the resulting sets is a property of that
realization. Cost is owned by the budget condition. **Nothing normative is left for a shape
gate to measure**, so rev4 keeps the measurement and drops its authority.

## Rev4 — forward-only

> A projection-validity verdict may not depend on incidental membership of optional,
> semantically admissible records unless the contract independently declares that membership
> normative.

`task_dependence` becomes `gating: false`, `status: diagnostic`, reported as descriptive
topology (`equal` / `left_strict_superset` / `right_strict_superset` / `incomparable`). The
rev3 pinned expectations remain in the file, explicitly marked historical.

The rev4 evaluator (`o7b1/evaluate_v2.py`) **imports the rev3 evaluator unmodified** and
re-decides only which measurements may gate — so every number rev4 reports is the number
rev3 computed.

## Qualification

| check | result |
|---|---|
| rev4 contract schema validation (case-0002, h1, h2, h3) | **PASS** |
| synthetic suite (existing o7b1 tests) | **PASS — 141 tests** |
| metamorphic projection invariance | **PASS** — rev4 overall constant across the class while topology varies |
| historical rev3 reproduction | **PASS** — rev3 evaluator run as-is |
| H2 false shape failure removed | **true** (rev3 FAIL[task_dependence] → rev4 PASS) |
| H3 budget failure preserved | **true** (rev3 FAIL[projection_validity] → rev4 FAIL[projection_validity]) |
| H1 pass preserved | **true** |
| negative controls (6) | **PASS — all fail through their owning axes** |
| C1 / Q1R semantic verdict | **PASS** (see scope note) |

Negative controls: required-observation removed → `question_observation_support`; relation
witnesses removed → `relation_support`; stale presented as current → `stale_state_safety`;
ineligible record → `projection_validity`; fabricated relation → `projection_validity`;
budget exceeded → `projection_validity`. No "different projection = valid" escape hatch.

**Scope note on C1/Q1R:** case-0002's derived body is private and not in-repo, so those two
arms were not re-run end to end. Their rev3 verdicts are the recorded PASS from Q1/Q2, and
the one gating property rev4 *adds* — relation groundedness — was verified directly against
gold for both (0 ungrounded edges each). The divergent-topology witness is carried by H2,
where two valid projections genuinely differ in shape.

## A gap found by the negative controls, and closed

`NC5` fabricates an edge to an observation that does not exist. **Under rev3 no axis
objected to it.** That run still came out FAIL — but only because `task_dependence` was
already failing for an unrelated reason. The fabricated edge was never the thing being
caught.

Removing the shape gate takes away that accidental cover, so rev4 gives the property an
explicit owner: `projection_validity` now requires every asserted relation to exist in gold.
This is projection-invariant (grounding does not depend on which optional records were
chosen) and it is a validity property, not a shape one. The gap was **pre-existing**; rev4
did not open it, it exposed it.

## Backward compatibility

Nothing was retracted or rewritten. Rev3 contracts, the rev3 evaluator (`58a7a111`), the
Q1/Q2 receipts, the H1/H2/H3 reports and all L1/L2/L3 evidence are byte-intact — `git status`
shows every artifact of this work as *new*, none as modified.

- Historical: *under rev3, H2 `task_dependence` = FAIL.* That stands.
- New: *under rev4, that rev3 failure is classified as a non-projection-invariant measurement
  artifact.*

Not contradictory: the semantics changed explicitly, forward-only, under a new identity.

## Next gate

The evaluator boundary is closed, so the real-source holdout **may now be considered** — as
a separate authorization, not opened by this receipt.

## Artifacts

`h2-boundary-qualification-receipt.json` (`d0afbfab`), `qualification-evidence.json`.
Schema `schema/evaluation-contract-v1-r4.schema.json`; rev4 contracts for case-0002 + h1/h2/h3.
Evaluator `tools/o7b1/evaluate_v2.py`. Harnesses `tools/{build_contract_r4,h2a_invariance_probe,h2_boundary_qualify,h2_boundary_receipt}.py`.
