# case-0002 R3 — authoritative untuned actual baseline (frozen observations)

> Observations frozen only. No remediation. The v0 design ran UNCHANGED (schema v0,
> selector v0, evaluation v0) via the behavior-preserving R2/R2.1 admission adapter,
> against fixture v1 and the frozen Round 0 sources.

## Multi-axis observation (recorded, not rewritten)

```
generation:              PASS            # exit 0 = generation completed (not determinism)
projection:              SURVIVED_UNTUNED
task_dependence:         v0 pairwise (resume vs audit) — differ, accepted; THIRD task excluded
evaluation:              UNAVAILABLE_UNDER_V0   # negative_control []; evaluation null
determinism:             DETERMINISTIC (two independent runs, byte-identical)
overall_observation:     UNTUNED_PROJECTION_SURVIVED__EVALUATION_UNAVAILABLE
report_development_result: PARTIAL       # = evaluation has no negative-control arm, NOT projection failure
```

## Projection survived untuned

All three tasks generated; every projection validity block valid; no missing required
kind or topic; no selector or schema modification.

| task | selected | valid | evaluation |
|---|---|---|---|
| resume-product-integration | 6 | valid | null |
| audit-r21-evidence-provenance | 10 | valid | null |
| assess-change-impact-on-oracle-topology | 9 | valid | null |

## Task dependence — with its v0 limitation stated

v0 compared **only the first two** task summaries (resume vs audit): projections differ
(symmetric difference 14), acceptance accepted. The third task
(assess-change-impact) is projected and valid but is **not** part of this pairwise
comparison; its task-dependence is **not** measured by v0 and is **not** generalized here.

## Evaluation — unavailable under v0

case-0002 has no negative-control reconstruction (`negative_control: []`), so the v0
three-arm evaluation has no negative-control arm and every task evaluation is `null`.
This is `UNAVAILABLE_UNDER_V0` — **not** an evaluation PASS, and **not** a projection
failure. `report_development_result: PARTIAL` means exactly this.

## Determinism

Two independent executions of the exact authoritative command, same environment, output
directory removed between them. Complete output trees (14 files incl `derived/*.jsonl`)
compared byte-for-byte: `same_relative_file_set: True`,
`all_corresponding_bytes_identical: True`, stdout
and stderr identical. Verdict **DETERMINISTIC**. (The two tree-manifest digests differ
only by the recorded `run` label; every per-file digest is identical.)

report.json = `cas:sha256:9b4cd434d1dac50a0789650d8a55e919e570a7840cb4e2f327804f9f948ebfa2`.

## Static v0 limitations (recorded, not fixed)

task-dependence considers only the first two summaries; `budget-v0.yaml` is not consumed;
`required_relation_paths` is not evaluated; `forbidden_stale_as_current` is not evaluated;
no negative-control arm exists for case-0002. **None fixed in R3.**

## Environment

Python 3.14.6, PyYAML 6.0.3, Linux 6.18.39-1-lts x86_64, locale C.UTF-8, TZ UTC.

The next decision is R4 evaluation-contract design — not selector or schema remediation.
The projection has now stated, in bytes, that it survived unchanged.
