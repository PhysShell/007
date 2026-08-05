# case-0002 baseline adjudication — erratum to the v0 baseline

> Correction round only. No implementation remediation. The original baseline
> report (`results/case-0002-baseline-v0/case-0002-baseline-report.md`) and all
> v0 fixture/baseline artifacts are preserved **unchanged**; this is an added
> erratum, not an edit.

Reviewed baseline head: `415bfafceca08740f150b5ecfd60260ce4f1d5e9`.

## What was wrong in the v0 baseline classification

The v0 baseline classified the `project_case` projection failure primarily as
**SELECTOR_GAP**. That was **overclaimed**. Selector v0 already provides
`anchor_observation_ids` — a justified, rationale-required, ratio-capped (≤34%)
escape hatch for an eligible observation needed despite no topic match. The
audit task left it empty. More decisively, the audit task's **frozen questions
require no status observation**: their required observation IDs are of kinds
`decision`, `constraint`, `evidence` only. So `required_kinds: [..., status]`
was an over-specification with no support in the frozen contract.

## Adjudication (Case A)

- **status is not actually required.** Removed it from the audit task's
  `required_kinds` in fixture v1. Nothing else changed: topics were **not**
  retagged, no observation was added or removed, gold-state is byte-identical to
  v0, and no actual baseline output was used as an answer key.

## Test — unchanged selector v0 against fixture v1

Implementation unchanged (007 `c8751223`; blobs identical — `project_case.py`
576eb320, `selector.py` 42412fc2, `pipeline.py` 92f2d409, `projector.py`
24c4ffc6). `project_case.py --fixture case-0002-v1`, run twice:

| task | selected | valid | missing required kinds | anchors used |
|---|---|---|---|---|
| resume-product-integration | 6 | ✅ | — | 0 |
| audit-r21-evidence-provenance | 10 | ✅ | — | 0 |
| assess-change-impact-on-oracle-topology | 9 | ✅ | — | 0 |

exit 0, all three tasks valid, projections differ (symmetric difference 14),
task-dependence accepted, **byte-identical across both runs**.
`projection-comparison.json` = `cas:sha256:b16477d0…`.

## Result

```
classification: FIXTURE_ORACLE_DEFECT_CONFIRMED
SELECTOR_GAP: RETRACTED as a v0 finding
selector_v1_needed: false
```

The unchanged selector v0 handles the corrected fixture perfectly, so the fixture
was the defect, not the selector — exactly the "widen the doorway because someone
carried the wardrobe in sideways" trap avoided.

## What still stands (unchanged, not this round's scope)

- **Top-level `BASELINE_ADMISSION_BLOCKED` remains valid.** `run_case.py` still
  fails on the empty `negative_control` — the **EVALUATION_CONTRACT_GAP** is a
  confirmed runtime defect and was not touched.
- The observed-vs-static separation from the corrected classification:

```
observed_at_runtime:
  - negative_control_IndexError   (EVALUATION_CONTRACT_GAP, confirmed)
  - missing_required_kinds        (RECLASSIFIED: FIXTURE_ORACLE_DEFECT, not SELECTOR_GAP)
static_code_findings_not_reached:
  - expected_report_required
  - only_first_two_tasks_compared
  - budget_file_ignored
  - relation_paths_ignored
  - stale_as_current_ignored
newly_noted_admission_plumbing (running v1):
  - registry fixture_id bound to the requested dir name
  - registry filename hard-coded (tasks-v0.yaml) + fixture-dir confinement
```

## Not done (still forbidden this round)

No change to negative-control behaviour, expected-report handling, task-dependence
evaluation, budget loading, relation closure, stale-as-current checks, schema or
selector semantics, Qodec arm, holdout. qodec PR #16 untouched. Stopped after this
fixture-v1 projection report.

## Suggested ordering (controller's decision, not started)

```
R2: generic admission plumbing (actual-only baseline mode; graceful absence of
    negative-control; case-0001 byte identity) — one focused change at a time.
R3: authoritative untuned full run.
R4: evaluation-v1 design (all-task comparison; relation-path coverage;
    stale-as-current; explicit budget contract) — separate commits, never "misc fixes".
```
