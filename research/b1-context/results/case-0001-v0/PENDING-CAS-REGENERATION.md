# Pending: 3-arm report must be regenerated on a CAS-equipped machine

The corrective round (`#issuecomment-5182632187`) changed the observation
schema (`topics`), introduced the versioned selector contract
`o7.b1.selector/v0`, made selection task-dependent, renamed the structural
metrics and split the negative-control claims.

Every one of those changes alters `report.json`. The previous report and the
frozen expectation were produced by the pre-corrective pipeline and are now
**wrong** — not merely out of date — so they were removed rather than left
committed where they would read as current results.

## What is here, and reproducible by anyone

The task-dependent **projection** needs no CAS, no secrets and no network. It is
committed under `tasks/` and regenerated with:

```sh
python3 research/b1-context/tools/project_case.py \
  --fixture case-0001 --out research/b1-context/results/case-0001-v0
```

Two runs produce byte-identical output. `projection-comparison.json` records the
acceptance invariant as data: same gold state, two tasks, different selections,
neither a superset of the other.

## What is missing, and why

The 3-arm evaluation needs the derived transcripts and the sealed negative
control out of the owner's local CAS. It cannot run on a machine without those
private blobs, so `report.json`, `report.md` and
`fixtures/case-0001/expected-report-v0.json` are absent until the owner runs, once:

```sh
python3 research/b1-context/tools/run_case.py \
  --fixture case-0001 --data-root "$HOME/.local/share/o7-research" \
  --out research/b1-context/results/case-0001-v0 --update-expectations
```

`--update-expectations` refuses to freeze anything unless every CAS input blob
was available, so an incomplete run can never mint an expectation.

Until then `run_case.py` **fails closed** with an explicit message: a missing
expectation is treated as a hard error, never as "nothing to check against".

## Retired artifact digests (for the record)

| artifact | digest at head `4805502` |
|---|---|
| `results/case-0001-v0/report.json` | `sha256:93a12b4e769e63116b8c9d31583fb9bcf22da6d2e24678a7218c81c6c8a2d6c1` |
| `fixtures/case-0001/expected-report-v0.json` | `sha256:93a12b4e769e63116b8c9d31583fb9bcf22da6d2e24678a7218c81c6c8a2d6c1` |

Those bytes remain in git history at that commit; they are simply no longer
presented as the current result.
