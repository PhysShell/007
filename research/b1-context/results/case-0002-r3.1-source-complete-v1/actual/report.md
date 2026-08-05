# B1 development result — case-0002

> Golden **development** fixture. One fixture can show the pipeline runs and detects known losses; it cannot prove the pipeline works on new cases.

## Verdict

| field | value |
|---|---|
| deterministic_compilation_pipeline | **PASS** |
| task_conditioned_projection | **IMPLEMENTED** |
| development_result | **PARTIAL** |
| generalization | **NOT_EVALUATED** |
| source_set_complete | false |
| holdout_evaluated | false |
| authoritative_for_a_series | false |

## Task-dependent selection

Selection is a pure deterministic function of `gold state + task + selector contract/version + budget`, under `o7.b1.selector/v0` (impl `sha256:8e2dd816e6fdd82f6af4ec559e012e5f82484231ba36cc5f38f7e7759dacb029`).

### `case-0002-resume-product-integration`

- required topics: `product-integration`
- preferred topics: `round-0`
- required kinds: `decision`, `next_action`, `status`
- selected **6** observation(s), omitted 18 with explicit reasons, 3385 bytes
- projection validity: **valid**

### `case-0002-audit-r21-evidence-provenance`

- required topics: `r21-evidence`
- preferred topics: `measurement`, `reviewer-acceptance`, `supersession`
- required kinds: `decision`, `evidence`
- selected **10** observation(s), omitted 14 with explicit reasons, 6482 bytes
- projection validity: **valid**

### `case-0002-assess-change-impact-on-oracle-topology`

- required topics: `oracle-topology`
- preferred topics: `measurement`, `r21-evidence`
- required kinds: `constraint`, `evidence`, `risk`
- selected **9** observation(s), omitted 15 with explicit reasons, 5758 bytes
- projection validity: **valid**

**Same gold state, different tasks, different projections:** `case-0002-resume-product-integration` selected 5 observation(s) the other did not; `case-0002-audit-r21-evidence-provenance` selected 9 the other did not; symmetric difference **14**; neither is a superset of the other (true).

## What worked

- All 3 fixture input blobs verified by digest and size (fail-closed check).
- Both extractors ran deterministically over the local CAS and produced 2384 derived transcript records (5339109 bytes), user-visible only, no chain-of-thought.
- Every task's projection passed the validity checks in `validity.py`, which are computed from the presented records themselves and are proven able to fail by the adversarial tests in `tests/test_validity.py`.

## How to read the coverage numbers

`compiled_observation_coverage` and `structural_question_support` are **structural**: they ask whether an arm carries the compiled observation together with its complete provenance graph. They are **not** semantic recall and **not** answerability.

A raw transcript may well contain the information behind an observation while carrying neither the compiled observation nor its provenance — it still scores zero here. So a low `full_derived` figure must **not** be read as "only that fraction of the project state is present in the conversations". Measuring that requires a separate extraction/readout experiment, which does not exist yet.

## Where the negative control lost state

Each proposition is reported separately, because they are established by different evidence:

| superseded belief | fixture-expected divergence | corrective evidence absent from NC | contrary claim detected |
|---|---|---|---|

## Context reduction

| task | arm | input bytes | records | compiled-obs coverage | reduction vs full |
|---|---|---|---|---|---|

## What still cannot be claimed

- B1 is **not** proven; context preservation is **not** solved.
- The schema is **not** universal and this is **not** production ready.
- Nothing here **generalizes**; the source set is **not** SOURCE_SET_COMPLETE.
- Two tasks over one fixture show selection responds to the task. They do **not** show the selector generalizes to unseen tasks or unseen gold states.
- B1 remains read-only and non-authoritative over the A-series.

## Reproduce

```
python3 research/b1-context/tools/run_case.py --fixture case-0002 --registry tasks-v1.yaml --actual-only --data-root "$HOME/.local/share/o7-research" --out /tmp/o7-b1-case-0002
```

The task-dependent projection half needs no CAS and no secrets:

```
python3 research/b1-context/tools/project_case.py --fixture case-0002 --out research/b1-context/results/case-0002-v0
```
