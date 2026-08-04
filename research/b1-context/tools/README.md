# B1 context vertical — tools (v0)

A single deterministic, offline, read-only pipeline that takes the verified
`case-0001` sources and produces a measured development result:

```
verified RAW sources (local CAS)
  -> deterministic derived transcripts (extractors v0)
  -> state-observables schema v0
  -> evidence-backed gold state
  -> task-conditioned projection
  -> deterministic 3-arm evaluation
  -> report with real metrics
```

This directory is **not** part of the cargo workspace and no production crate
imports it.

## Run it

```sh
python3 research/b1-context/tools/run_case.py \
  --fixture case-0001 \
  --data-root "$HOME/.local/share/o7-research" \
  --out /tmp/o7-b1-case-0001
```

Requirements: Python 3.10+ and PyYAML. No network, no secrets. The owner's local
CAS (`$data-root/cas`) must contain the six `case-0001` blobs; without them the
run is not possible and the tests that need them skip cleanly.

Properties enforced by the code:

- **Read-only inputs.** RAW blobs are only read (digest + size verified on every
  access). Nothing is written to the CAS or to any source blob. All outputs land
  under `--out`.
- **Fail closed.** A digest/size mismatch, a derived transcript that disagrees
  with the committed `derived-manifest.yaml`, or a report that disagrees with the
  committed `expected-report-v0.json` exits non-zero and writes no fabricated
  report. A missing blob is UNAVAILABLE (→ PARTIAL), which is distinct from
  INVALID.
- **Deterministic.** `context.json`, `context.md`, `context.meta.json`,
  `report.json`, `report.md` are byte-identical across runs (canonical UTF-8,
  sorted keys, LF, timestamps kept out of canonical digests).
- **No chain-of-thought.** Extractors never emit hidden reasoning (`thinking` /
  `thoughts` / `reasoning_recap`) and never pass tool plumbing off as user text.
- **No LLM judge.** The evaluation is purely structural.

## Outputs

Canonical artifacts are written under `--out`; the committed copies live in
`../results/case-0001-v0/` (report + context) and `../fixtures/case-0001/`
(inputs + `expected-report-v0.json`). Derived transcripts are written to
`<out>/derived/*.jsonl`; they are **external CAS blobs** and are never committed.
To store them in the owner's CAS after a run:

```sh
for f in /tmp/o7-b1-case-0001/derived/*.jsonl; do o7-cas put "$f"; done
```

## Tests

```sh
cd research/b1-context/tools && python3 -m unittest discover -s tests -p 'test_*.py'
```

Extractor, projection, schema and evaluation tests run on synthetic fixtures and
do **not** require the private RAW blobs. One integration test runs the full
pipeline only if the local CAS is present, and skips otherwise.

## Honest status

- `development_result: PASS` means the pipeline ran end-to-end on this one golden
  fixture and reproduced the expected measured numbers — including detecting the
  negative control's known state loss. It does **not** mean B1 is proven, that
  context preservation is solved, that the schema is universal, that anything is
  production ready, or that any of this **generalizes**.
- `generalization: NOT_EVALUATED`, `source_set_complete: false`,
  `holdout_evaluated: false`, `authoritative_for_a_series: false`.
- Generalization requires holdout cases whose questions are fixed and
  digest-bound before compaction; `case-0001` is permanently excluded from
  holdout because the schema and questions were shaped on it.
