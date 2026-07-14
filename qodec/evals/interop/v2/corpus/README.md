# Qodec Interop Benchmark v2 — reproducible corpus compiler (Scope N0)

**Contract:** `interop-corpus-compiler-v1` · **Status:**
`compiler-only-before-benchmark-data` · **Base commit:** `d0de6b2`

This directory is the **corpus compiler**: a reproducible system to *define,
capture, verify and regenerate* benchmark case bundles. Scope N0 builds the
compiler and ships **exactly one NON-BENCHMARK demonstration case**. It creates
**zero** real Benchmark v2 cases, no public-development / public-validation /
sealed material, and no reader questions. No model calls; no external datasets.

```
NON-BENCHMARK · NON-GATING · NOT PART OF THE 48 BASE CASES
NOT PUBLIC-DEVELOPMENT · NOT PUBLIC-VALIDATION · NOT HELD-OUT
```

## Layout

```
corpus/
  corpus-contract.json     — compiler contract (benchmark_case_count = 0)
  manifest.json            — membership (benchmark_cases: [], demonstration_cases: [...])
  README.md · decisions.md
  schemas/                 — 7 JSON schemas (case/recipe/provenance/receipt/snapshot/evidence/manifest)
  tools/                   — corpus_tool.py (CLI) + capture/receipts/snapshots/hashing/jsonschema_mini
  examples/
    deterministic-log-demo/  — the single demonstration bundle
  tests/                   — model-free unit + real-RTK integration tests
```

## Canonical case-bundle layout

```
<case-id>/
  case.json  provenance.json  capture-recipe.json  evidence-map.json
  fixture/
  snapshots/  raw.stdout  raw.stderr  rtk.stdout  rtk.stderr
  receipts/   native.json  rtk.json
  snapshot-manifest.json
```

**Raw and RTK snapshots are the canonical corpus evidence. Qodec / VG / hybrid
outputs are derived run artifacts and are rejected inside a bundle.**

## CLI

```bash
python tools/corpus_tool.py <command>
```

| Command | What it does |
|---|---|
| `validate [--case ID]` | schemas, bundle integrity, paths, hashes, receipts, evidence spans, manifest membership, demonstration leakage, forbidden shell |
| `capture-native --case ID` | runs setup + native argv, writes `raw.*` + native receipt (no RTK, no qodec) |
| `capture-rtk --case ID` | reads committed `raw.stdout`, runs pinned `rtk pipe`, writes `rtk.*` + RTK receipt (reduction-isolation) |
| `regenerate --case ID [--write]` | compare-only by default (nonzero on drift, no working-tree change); `--write` rewrites snapshots + manifest |
| `verify [--case ID]` | committed-bundle integrity + schemas, no tool execution |
| `diff --case ID` | committed vs a fresh capture: changed files, SHA/size/exit/classification deltas |
| `list` | cases, statuses, completeness |
| `changed --files ... \| --base X --head Y` | maps changed files to affected case IDs (docs-only change → none) |

## Determinism

Canonical environment: `LC_ALL=C.UTF-8`, `LANG=C.UTF-8`, `TZ=UTC`,
`SOURCE_DATE_EPOCH` fixed. Network disabled during capture. No shell, ever —
commands are argv arrays. Child processes get an explicit environment allowlist,
never the inherited runner environment, and credential-bearing variables are
stripped even if allowlisted. Post-capture normalization is disabled by default;
a future case that needs it must declare a `normalization_policy` with before/
after hashes (the N0 demo needs none).

## Demonstration case

`deterministic-log-demo` generates a fixed build-log-like stream locally (no
network, no Docker), processed by the real `rtk pipe --filter log`. It carries
five evidence facts (exact, count, relation, ordering, absence). It is plumbing,
not benchmark evidence, and **cannot become a benchmark case by renaming or
retagging** — the validator enforces that.

See `decisions.md` for the design rationale.
