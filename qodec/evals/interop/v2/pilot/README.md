# Interop Benchmark v2 — Scope N1 public-log pilot

**Status:** `pilot-development-non-gating` · **Split:** public-development only ·
**Base:** accepted N0 head `529ca24557af9a45833025bf06e36e76800eb610`

The first non-smoke pilot leaderboard over **real** tool logs. Ten first-party
public-development cases are captured from pinned tools and evaluated under four
logical arms:

```
RAW          the real tool output
QODEC        qodec fold-grep-guarded (VG), a lossless notation layer
RTK          the pinned RTK reducer (lossy; may return raw via never_worse)
RTK + QODEC  qodec over the RTK-reduced output
```

This is a **development pilot**: public-development cases only, **no** validation
split, **no** sealed-heldout material, **no** reader/model calls, **no**
production-promotion verdict, and **no** change to any frozen numeric gate. The
four-arm run is explicitly **non-gating**.

## What is real here

Every payload is emitted by a real, pinned tool over a fixed in-repo fixture —
never hand-written. Capture uses the frozen N0 corpus engine (no shell, explicit
env allowlist, no network, canonical `LC_ALL=C.UTF-8` / `TZ=UTC`), reused as a
library; only the N0 "demonstration-only" CLI rules are not reused. Canonical
`raw.*` / `rtk.*` bytes are produced by the **Nix-pinned** toolchain in CI (the
authoring host cannot run the pinned tools), captured twice in independent temp
dirs and compared byte-for-byte before they are trusted.

## Cases (10)

| Case | Family | Ecosystem | Tool |
|---|---|---|---|
| rust-compile-type-mismatch | compiler-build | rust | rustc E0308 |
| rust-compile-borrow | compiler-build | rust | rustc E0499 |
| rust-compile-multi | compiler-build | rust | rustc (E0308 + 16×E0425) |
| python-syntaxerror | lint-static-analysis | python | py_compile SyntaxError |
| python-doctest-pass | test-runner | python | doctest -v (pass) |
| python-json-decode-error | structured-data-query | python | json.tool (malformed) |
| ripgrep-error-search | search-listing | language-neutral | rg --sort path |
| jq-select-names | structured-data-query | language-neutral | jq |
| git-diff-noindex | git-diff-history | language-neutral | git diff --no-index |
| ripgrep-def-exploration | code-exploration-callgraph | language-neutral | rg (fn/def) |

**Diversity:** 7 families, 3 ecosystems (mandatory floor: 10 cases, ≥4 families,
≥3 ecosystems). The suggested container/orchestrator and application/CI-log
families and a runtime exception traceback are omitted with cause — see
`deviation_justification` in [`pilot-manifest.json`](pilot-manifest.json).

## Layout

```
pilot/
  pilot-manifest.json        case list, distribution, source & deviation policy
  schemas/                   pilot case / anchors / snapshot-manifest / manifest / report
  cases/<id>/
    case.json provenance.json capture-recipe.json anchors.json
    fixture/…                 real inputs
    snapshots/{raw,rtk}.{stdout,stderr}   canonical evidence (CI-captured)
    receipts/{native,rtk}.json  snapshot-manifest.json
  tools/  pilot_lib pilot_capture pilot_reproduce pilot_validate pilot_run
  tests/  test_pilot_corpus test_pilot_security test_pilot_runner test_frozen
```

`raw.*` and `rtk.*` are canonical evidence. QODEC and RTK+QODEC outputs are
**derived** run outputs and never become canonical corpus snapshots.

## Running (Nix)

```bash
nix build .#checks.x86_64-linux.qodec-v2-pilot-validate    # corpus + tests
nix build .#checks.x86_64-linux.qodec-v2-pilot-reproduce   # capture twice, compare
nix build .#checks.x86_64-linux.qodec-v2-pilot-run         # four-arm report -> $out/pilot
nix build .#checks.x86_64-linux.qodec-v2-pilot-capture     # canonical snapshots -> $out/cases
```

CI (`qodec-v2-public-log-pilot.yml`): `corpus-validate`, `capture-reproducibility`,
`pilot-four-arm-run` (uploads `pilot-report.json`, `pilot-summary.md`,
`case-manifest.json`, `provenance-report.json`), `flake`. The canonical snapshots
are bootstrapped by the manual `capture-bootstrap` job.

## Correctness invariants (per case, enforced by the runner)

`decode(qodec(raw)) == raw` · `rtk exit == 0` · `rtk output non-empty` ·
`decode(qodec(rtk)) == rtk` · `qodec tokens ≤ raw tokens` ·
`hybrid tokens ≤ rtk tokens` · committed raw/rtk hashes match. RTK is **not**
required to be smaller than RAW (never_worse may return raw). Anchor survival
through RTK is reported but non-gating.
