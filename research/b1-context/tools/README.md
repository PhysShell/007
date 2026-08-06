# B1 context vertical — tools (v0)

A single deterministic, offline, read-only pipeline that takes the verified
`case-0001` sources and produces a measured development result:

```
verified RAW sources (local CAS)
  -> deterministic derived transcripts (extractors v0)
  -> state-observables schema v0 (observations carry task-independent `topics`)
  -> evidence-backed gold state
  -> task + versioned selector contract (o7.b1.selector/v0)
  -> task-dependent projection
  -> deterministic 3-arm evaluation (structural metrics, honestly named)
  -> report with real metrics
```

Selection is a pure deterministic function of
`gold state + task + selector contract/version + budget`. Two different tasks
over the same gold state produce different projections; `project_case.py` writes
the proof as data (`projection-comparison.json`).

The **projection** half needs no CAS, no secrets and no network — anyone can
reproduce it from a checkout. Only the 3-arm evaluation needs the owner's
private blobs.

```sh
python3 research/b1-context/tools/project_case.py \
  --fixture case-0001 --out research/b1-context/results/case-0001-v0
```

This directory is **not** part of the cargo workspace and no production crate
imports it.

## Admission modes (R2 plumbing — no change to selection/evaluation/schema/budget meaning)

- `--registry <bare-filename>` (default `tasks-v0.yaml`): run a versioned task
  registry inside the SAME fixture directory, so a fixture revision runs without
  cloning the fixture into a new directory. The name must be a bare filename
  confined to the fixture dir (absolute paths, separators, `..`, missing files
  and escapes fail closed). The selected registry filename (when non-default) and
  its digest are recorded in the generated metadata.
- `--actual-only`: generate the honest actual report and write every ordinary
  artifact under `--out`, but never read, write or verify a committed
  expectation. It performs exactly ONE generation and exits 0 when generation
  completes successfully, regardless of the report's `development_result`.
  **Exit 0 means generation succeeded — it does NOT claim the outputs are
  deterministic**; determinism is established only by an external two-run
  comparison (the R3 protocol). Operational success and evaluation status are
  printed separately. The report's `reproduce_command` records `--actual-only`
  (and a non-default `--registry`) so it actually regenerates the report.
  Mutually exclusive with `--update-expectations`.
- Negative-control cardinality is fail-closed: `negative_control: []` runs with
  no negative-control arm (evaluations stay `null`, `development_result` PARTIAL);
  exactly one entry preserves the existing behaviour; more than one, a missing
  field, or a non-list value fail closed (never silently pick the first).

Normal (verification) mode is unchanged: it still requires the committed
`expected-report-v1.json` and fails closed on mismatch or absence.

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
  with the committed `derived-manifest.yaml`, a report that disagrees with the
  committed `expected-report-v1.json`, a task-dependence acceptance failure, or a
  present legacy `expected-report-v0.json` exits non-zero and writes no fabricated
  report. A missing blob is UNAVAILABLE (→ PARTIAL), which is distinct from
  INVALID.
- **Exception-safe staged freeze.** `--update-expectations` completes generation
  and verification in temporary sibling directories before promotion: every gate
  and the repeated byte-identity check pass first, and a failed pre-promotion
  generation leaves the committed artifacts untouched. Each same-filesystem
  rename is atomic, and ordinary Python exceptions trigger rollback. The
  multi-step promotion of the results directory and `expected-report-v1.json` is
  **not** crash-atomic: an abrupt process or machine failure may require recovery
  from `.update-bak` before rerunning the update.
- **Deterministic.** `context.json`, `context.md`, `context.meta.json`,
  `report.json`, `report.md` are byte-identical across runs (canonical UTF-8,
  sorted keys, LF, timestamps kept out of canonical digests).
- **No chain-of-thought.** Extractors never emit hidden reasoning (`thinking` /
  `thoughts` / `reasoning_recap`) and never pass tool plumbing off as user text.
- **No LLM judge.** The evaluation is purely structural.

## Outputs

Canonical artifacts are written under `--out`; the committed copies live in
`../results/case-0001-v0/` (report + per-task context + projection-comparison) and
`../fixtures/case-0001/` (inputs + `expected-report-v1.json`). Derived transcripts
are written to `<out>/derived/*.jsonl`; they are **external CAS blobs** and are
never committed (also `.gitignore`d under `results/`).
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

## Standalone evaluation/v1 evaluator (`evaluate_v1.py`)

`evaluate_v1.py` (module: `o7b1/evaluate_v1.py`) is a **contract-driven,
deterministic, structural** evaluator for `o7.b1.evaluation/v1` (contract
revision 3). It consumes already-frozen projection and source-compilation
artifacts; it never reruns extraction or projection, never uses an LLM judge, and
never touches `report.json` or evaluator v0. Every identifier and gate target
comes from the supplied contract and inputs — the module contains no case literal.

It validates the exact input closure (dual-digest structured inputs, artifact-bytes
+ JSONL bodies), runs contract-input consistency gates 01–11 plus the declared
verifier-only constraints, computes the frozen structural gates (question support,
relation edge-witness / all-current-materialized, projection validity with byte
usage **recomputed** from the selected set, stale-state safety, all-pairs task
dependence), and emits a canonical `evaluation-v1.json` that it validates against
`evaluation-output-v1-r2.schema.json` before writing (atomic temp+rename).

Operational vs evaluation result are distinct: a schema-valid artifact is exit 0
regardless of whether `overall` is PASS / FAIL / UNAVAILABLE. Nonzero exit is
reserved for an inability to emit a trustworthy artifact (bad contract schema,
bad evaluator identity, malformed invocation, undeclared input access, output
schema failure, internal exception). `--mode qualification|authoritative` refuses
on a dirty worktree.

```sh
python3 evaluate_v1.py --contract CONTRACT.yaml \
  --input gold_state:gold-state.json=PATH ... --input derived_body:SESSION=PATH \
  --out evaluation-v1.json [--mode dev|qualification|authoritative]
```

Gates 06/07/08 are enforced **literally against the frozen contract**: the question
files' `required_relation_paths` and question-level `forbidden_stale_as_current` are
the source of truth, and the contract's explicit expansions must match them exactly
(gold-graph grounding is a separate, accurately-named consistency gate). Source
compilation reports the **raw** source bytes/digests from the fixture manifest and
availability from the report's `input_digests` — never conflating derived-body
evidence with raw-source bytes. The negative-control arm is `AVAILABLE` only with a
supplied, identity-valid body; a metadata-only declaration is `UNAVAILABLE`. Arm
availability requires the *whole* set (every session's body, every task's context),
not just one.

Its synthetic qualification suite (`tests/test_evaluate_v1.py`,
`tests/synth_eval_v1.py`, fixtures under
`tests/fixtures/evaluation-v1-synthetic/`) reports each case as fixture →
triggered oracle → observed output → pass/fail and never applies the evaluator to
case-0002.

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
