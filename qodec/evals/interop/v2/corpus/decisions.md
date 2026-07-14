# Corpus compiler — design decisions (Scope N0)

Binding decisions for `interop-corpus-compiler-v1`.

## Canonical vs derived

- **Raw and RTK snapshots are canonical corpus evidence.** They are captured
  first-party bytes: `raw.stdout/raw.stderr` from the native tool, and
  `rtk.stdout/rtk.stderr` from the pinned RTK reducer.
- **Qodec / VG / RTK+qodec outputs are derived**, produced from a *pinned policy
  during a benchmark run*. They are never stored in a case bundle; the validator
  rejects `qodec.stdout`, `qodec.stderr`, `vg.stdout`, `hybrid.stdout`,
  `qodec-envelope.json` inside a bundle.

## Reduction-isolation implemented; operational-proxy deferred

- **Reduction-isolation** is the N0 capture model: `capture-native` records the
  raw tool output once; `capture-rtk` feeds *those exact committed raw bytes*
  into `rtk pipe`. RTK never re-runs the native tool. This isolates RTK's
  reduction from tool nondeterminism and makes `raw → RTK` reproducible.
- **Operational-proxy mode** (RTK wrapping/rewriting the live command) is
  **deferred** — not implemented in N0.

## No real benchmark cases in N0

- `benchmark_case_count = 0`. The manifest's `benchmark_cases` is empty and the
  validator fails if it is not. The only case is a `demonstration` bundle,
  carrying the full NON-BENCHMARK marker set. A demonstration case can never
  enter `benchmark_cases` or take a public/validation/held-out status — that
  needs a separate scope, provenance review and manifest entry.

## No external datasets ingested

- N0 downloads nothing and adds no Terminal-Bench / SWE-bench / Loghub /
  BugSwarm payloads. The provenance schema *supports* external sources (with a
  pinned immutable revision, license, PII/secret review and sanitization
  hashes), but no external source is added here.

## Determinism choices

- Commands are argv arrays; shell execution is forbidden in this contract
  version (a future version may add an explicit shell policy).
- `capture_timestamp` is bound to `SOURCE_DATE_EPOCH`, not wall-clock, so
  committed receipts are reproducible. It is metadata and never enters snapshot
  bytes; the reproducibility comparison ignores it and `wall_time_s`.
- RTK's `log` filter groups by severity using a hashed set, so an input with two
  lines of the same severity yields nondeterministic ordering. The demonstration
  input is shaped to have exactly one error line and one warning line, which
  makes `rtk pipe --filter log` byte-deterministic. This is recorded so future
  cases pick filter/input combinations that are reproducible (or declare a
  normalization policy).

## Security posture

- Child processes are built from an explicit environment allowlist; the runner
  environment is not inherited. `GITHUB_TOKEN`, API keys, `SSH_AUTH_SOCK`, cloud
  and model credentials are stripped even if allowlisted, and proxy variables
  are dropped so capture cannot reach the network by accident.
- All bundle paths are relative and confined to the bundle: `..`, absolute
  paths and symlink escapes are rejected.
- CI never rewrites committed snapshots: `regenerate` is compare-only without
  `--write`, and no CI job passes `--write`.
