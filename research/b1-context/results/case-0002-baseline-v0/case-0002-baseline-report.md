# case-0002 untuned baseline — v0 design applied to new material

> Development/challenge case. This step observes the **unmodified** B1 v0 design on
> case-0002 before any improvement. A red result is expected and useful. **No
> remediation was performed.**

## Result

```
result: BASELINE_ADMISSION_BLOCKED
```

The unmodified v0 cannot admit case-0002 through **either** entry point. Both
failures are deterministic (byte-identical stderr across re-runs). No
authoritative baseline run was reached, so there are no canonical baseline
outputs to byte-compare; the *failure* is what reproduced.

## Implementation identity (exact, not floating main)

- 007 commit `c8751223a3ccf970c8e3c3f3089be9748ae3d8e6`, tree `f074f5e8…`
- Python 3.14.6, PyYAML 6.0.3
- named module git blobs: `run_case.py` 86a5b82c, `project_case.py` 576eb320,
  `pipeline.py` 92f2d409, `projector.py` 24c4ffc6, `evaluate.py` 37795659,
  `schema.py` 516c9b50, `registry.py` 73e7dae3, extractors 351e7410 / 31ce9180
- fixture-lock `fixtures/case-0002/fixture-lock-v0.yaml`, gold-state digest
  `sha256:2589319a…` (24 observations, 15 relations, frozen before projection)

## Phase 0 — qualification (PASS)

Round 0 manifest CLOSED + CAS 102/102 round-trip OK; all six case-0001 blobs
present; 101 unit tests PASS; case-0001 projection reproduced; full case-0001
CAS-backed vertical reproduced **byte-for-byte** (`report.json` == committed
`expected-report-v1.json`, `sha256:52f9d2be…`). Environment qualified — no
PREREQUISITE_FAILURE.

## Phase 2 — preflight (the breaks)

| entry point | exit | symptom | primary class |
|---|---|---|---|
| `project_case.py` | 2 | FAIL CLOSED: audit task INVALID projection — `missing_required_kinds ['status']` | **SELECTOR_GAP** |
| `run_case.py` | 1 | uncaught `IndexError` at `manifest['negative_control'][0]` (line 259) | **EVALUATION_CONTRACT_GAP** |

**SELECTOR_GAP (project_case).** v0 selection is a pure function of topic + kind
weight + budget, but *eligibility/selection is topic-only*. The status-kind
observations (`obs-round0-closed`, `obs-plane-record-frozen`) carry topics
(`round-0`, `product-integration`) that the audit task does not request, so its
`required_kinds: [status]` validity gate cannot be satisfied without pulling in
an unrelated topic wholesale. Kind and topic are orthogonal in the fixture but
the selector only filters by topic; the validity gate then has no way to be met.
(Readable also as FIXTURE_ORACLE_DEFECT — the audit task over-specified
`required_kinds` — but the orthogonality is the v0 characteristic the new case
surfaced.)

**EVALUATION_CONTRACT_GAP (run_case).** The 3-arm evaluation
(full_derived / negative_control / projection) **unconditionally** dereferences a
negative-control source. case-0002 has no negative-control reconstruction (its
planes are RAW captures and a repository plane-record, not an advisory agent
reconstruction), so `negative_control[0]` raises `IndexError` — not even a clean
fail-closed (also EXECUTION_ADMISSION_GAP).

## Latent blockers (read from the code; not reached at runtime)

- **EXECUTION_ADMISSION_GAP** — `run_case` non-update mode requires a committed
  `expected-report-v1.json`; case-0002 has none, and one must not be minted from
  the actual result. Not reached because the `IndexError` precedes it.
- **EVALUATION_CONTRACT_GAP** — `pipeline.build_comparison` / `assess_task_dependence`
  compare only `summaries[0]` and `summaries[1]`; the third task (`impact`) is
  projected but excluded from the task-dependence comparison. Not reached because
  projection fails first.
- **BUDGET_POLICY_GAP** — `BYTE_BUDGET`/`RECORD_BUDGET` are hard-coded in the CLIs;
  `budget-v0.yaml` is never read.
- **EVALUATION_CONTRACT_GAP** — the frozen questions carry `required_relation_paths`
  and `forbidden_stale_as_current` (relation-closure + stale-as-current checks);
  these are v0-unknown fields, silently ignored — v0 has no relation-path or
  stale-as-current evaluation.
- No hard-coded `case-0001` was found; the registry and gold-state paths are
  fixture-parameterised.

## Adapter decision — none built

The protocol permits a minimal **behavior-preserving admission adapter** only if
it reaches a baseline without changing schema/selector/evaluation meaning and
keeps case-0001 byte-identical.

- The negative-control blocker *is* admission plumbing (skip the nc arm when
  `negative_control` is empty; case-0001 stays byte-identical) — an adapter is
  possible.
- But a full baseline still cannot be reached, because the **SELECTOR_GAP**
  requires kind-aware selection = a **selector-semantics change**, which is
  forbidden. Building only the nc adapter would move `run_case`'s failure to the
  same SELECTOR_GAP without reaching a baseline.

Per the protocol ("if behavior preservation cannot be proven, stop; do not mask a
design change as plumbing"), **no adapter was built** and the step ends here.

## What was deliberately NOT done

No schema v1, selector v1, relation-closure implementation, evaluation v1, budget
tuning, expectation update, or case-specific hack. No adapter. No LLM judge. qodec
PR #16 and its branches untouched. The frozen case-0002 v0 fixture was not edited
(corrections would create v1).

## Status (unchanged)

```
schema_frozen: false
selector_frozen: false
evaluation_frozen: false
qodec_arm_started: false
holdout_started: false
```

## What the baseline tells us (for the next decision, not this step)

The new case broke v0 in two independent places before it could even run: the
selector cannot honour a kind requirement across topics, and the evaluation
hard-wires a negative-control arm that a non-reconstruction case does not have.
Neither is tuning-around-able; both are genuine design surfaces. That is exactly
what a challenge case is for — the system showed where it breaks before anyone
was allowed to quietly fix it.
