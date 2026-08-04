# B1: Intent-Compiled Context Lab

```text
STATUS:  READ-ONLY R&D
         NON-AUTHORITATIVE
         MUST NOT DRIVE A-SERIES TRANSITIONS
```

This directory is the home of program **B1** from the A / B1 / B2 / S split:
an experimental track testing one narrow hypothesis —

> Can selected project-state observables be reliably projected into the
> context of a concrete task (typed state + task-conditioned projection),
> preserving working state, reducing context cost, and improving
> next-step execution?

B1 failure does not kill B2. B2 failure does not kill B1. Neither blocks
the A-series (A0 candidate-state continuity onward). Nothing here gains
authority over campaign or admission state without an explicit promotion
decision.

## Context

- Program record: the A/B1/B2/S convergence discussion (issues #91, #94,
  #95 and the PR #93 review thread).
- Related documents: `docs/agent-memory-layer.md` (draft),
  `docs/task-aware-context-generator.md` (draft),
  `docs/decision-and-admission-protocol.md` (accepted design decision,
  merged to `main` via PR #93).
- Qodec (`PhysShell/qodec`) is an experimental encoder/projection backend
  for representation experiments. It does not own the definitions of
  Decision, Assessment, Evidence, or ContextCompleteness — a codec must
  not define the meaning it is tasked to preserve.

## Structure

```text
schema/      state-observables schema v0 and event/assessment rules
             (built only AFTER source capture; the interim source set is
             captured — see fixtures/case-0001/manifest.yaml, promotion
             to SOURCE_SET_COMPLETE still pending)
tools/       the deterministic development vertical (extractors, projector,
             evaluator, run_case.py) — not part of the cargo workspace
fixtures/    golden development fixtures; case-0001 is the first
results/     actual measured results per fixture (report.json/report.md)
holdout/     evaluation cases NOT used while designing the schema
```

## Reproducible development vertical (case-0001 v0)

One command runs the whole vertical (verified RAW → derived transcripts →
schema v0 → gold state → task-conditioned projection → deterministic 3-arm
evaluation → measured report):

```sh
python3 research/b1-context/tools/run_case.py \
  --fixture case-0001 \
  --data-root "$HOME/.local/share/o7-research" \
  --out /tmp/o7-b1-case-0001
```

Offline, no secrets, read-only over inputs, fail-closed, byte-identical across
runs. See `tools/README.md` for details and `results/case-0001-v0/report.md` for
the current result.

**Honest status:** the latest run is `development_result: PASS` on this single
golden fixture — the pipeline runs end-to-end and detects the negative control's
known state loss. That is **not** proof of generalization. `generalization:
NOT_EVALUATED`, `source_set_complete: false`, `holdout_evaluated: false`,
`authoritative_for_a_series: false`. A golden fixture whose questions were shaped
on its own material can debug the representation; it cannot show the pipeline
works on new cases.

## Storage policy

- Source artifacts and experimental blobs live in the owner's external
  local CAS and are **never committed** to this repository. They are
  three distinct classes with different authority, never lumped together:
  - **platform captures/exports** — RAW;
  - **derived transcripts** — deterministic derivatives with provenance
    to a RAW digest;
  - **agent reconstructions** — advisory `agent_claim` negative controls,
    not RAW under any circumstances.
- The repository holds digests, sizes, selectors, manifests, and expected
  outputs only.
- Digest mismatch on any referenced blob invalidates the fixture
  (fail closed). A missing blob makes the fixture unavailable, which is
  a different state.

## Rules

1. This directory is not part of the cargo workspace; production crates
   must not (and cannot) import from it.
2. Fixture case-0001 is a **golden development fixture**: the schema and
   its questions were shaped on this material, so it can debug the
   representation but cannot prove generalization. Generalization claims
   require `holdout/` cases with questions fixed before compaction.
3. Agent reconstructions are `authority: advisory` negative controls.
   They never contribute to expected state; they exist to be measured
   against RAW.
