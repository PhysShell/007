# B1 development result — case-0001

> Golden **development** fixture. One fixture can show the pipeline runs and detects known losses; it cannot prove the pipeline works on new cases.

## Verdict

| field | value |
|---|---|
| deterministic_compilation_pipeline | **PASS** |
| task_conditioned_projection | **IMPLEMENTED** |
| development_result | **PASS** |
| generalization | **NOT_EVALUATED** |
| source_set_complete | false |
| holdout_evaluated | false |
| authoritative_for_a_series | false |

## Task-dependent selection

Selection is a pure deterministic function of `gold state + task + selector contract/version + budget`, under `o7.b1.selector/v0` (impl `sha256:8e2dd816e6fdd82f6af4ec559e012e5f82484231ba36cc5f38f7e7759dacb029`).

### `case-0001-continue-b1-v0`

- required topics: `b1-scope`
- preferred topics: `authority-boundary`, `holdout-readiness`
- required kinds: `constraint`, `goal`, `next_action`
- selected **9** observation(s), omitted 9 with explicit reasons, 5999 bytes
- projection validity: **valid**
- compiled-observation coverage 100.0%, structural question support 100.0%, presented-provenance ratio 100.0%, at reduction ratio 0.0064 vs full derived

### `case-0001-audit-source-capture-v0`

- required topics: `source-capture`
- preferred topics: `capture-topology`, `negative-control`
- required kinds: `constraint`, `evidence`, `status`
- selected **11** observation(s), omitted 7 with explicit reasons, 8151 bytes
- projection validity: **valid**
- compiled-observation coverage 100.0%, structural question support 100.0%, presented-provenance ratio 100.0%, at reduction ratio 0.0078 vs full derived

**Same gold state, different tasks, different projections:** `case-0001-continue-b1-v0` selected 5 observation(s) the other did not; `case-0001-audit-source-capture-v0` selected 7 the other did not; symmetric difference **12**; neither is a superset of the other (true).

## What worked

- All 6 fixture input blobs verified by digest and size (fail-closed check).
- Both extractors ran deterministically over the local CAS and produced 195 derived transcript records (1561141 bytes), user-visible only, no chain-of-thought.
- Every task's projection passed the validity checks in `validity.py`, which are computed from the presented records themselves and are proven able to fail by the adversarial tests in `tests/test_validity.py`.

## How to read the coverage numbers

`compiled_observation_coverage` and `structural_question_support` are **structural**: they ask whether an arm carries the compiled observation together with its complete provenance graph. They are **not** semantic recall and **not** answerability.

A raw transcript may well contain the information behind an observation while carrying neither the compiled observation nor its provenance — it still scores zero here. So a low `full_derived` figure must **not** be read as "only that fraction of the project state is present in the conversations". Measuring that requires a separate extraction/readout experiment, which does not exist yet.

## Where the negative control lost state

Each proposition is reported separately, because they are established by different evidence:

| superseded belief | fixture-expected divergence | corrective evidence absent from NC | contrary claim detected |
|---|---|---|---|
| `obs-nc-claim-bd-separate` | true | true | false |
| `obs-nc-claim-no-session-e` | true | true | false |

Absence of a corrective conversation id proves the reconstruction lacked that corrective reference. It does **not** by itself prove the reconstruction asserted the opposite proposition — that is the separate `contrary_claim_detected` column.

## Context reduction

| task | arm | input bytes | records | compiled-obs coverage | reduction vs full |
|---|---|---|---|---|---|
| case-0001-continue-b1-v0 | full_derived | 1561141 | 195 | 0.0% | 1.0000 |
| case-0001-continue-b1-v0 | negative_control | 39496 | 48 | 0.0% | 0.0253 |
| case-0001-continue-b1-v0 | projection | 9919 | 9 | 100.0% | 0.0064 |
| case-0001-audit-source-capture-v0 | full_derived | 1561141 | 195 | 9.1% | 1.0000 |
| case-0001-audit-source-capture-v0 | negative_control | 39496 | 48 | 0.0% | 0.0253 |
| case-0001-audit-source-capture-v0 | projection | 12183 | 11 | 100.0% | 0.0078 |

## What still cannot be claimed

- B1 is **not** proven; context preservation is **not** solved.
- The schema is **not** universal and this is **not** production ready.
- Nothing here **generalizes**; the source set is **not** SOURCE_SET_COMPLETE.
- Two tasks over one fixture show selection responds to the task. They do **not** show the selector generalizes to unseen tasks or unseen gold states.
- B1 remains read-only and non-authoritative over the A-series.

## Reproduce

```
python3 research/b1-context/tools/run_case.py --fixture case-0001 --data-root "$HOME/.local/share/o7-research" --out /tmp/o7-b1-case-0001
```

The task-dependent projection half needs no CAS and no secrets:

```
python3 research/b1-context/tools/project_case.py --fixture case-0001 --out research/b1-context/results/case-0001-v0
```
