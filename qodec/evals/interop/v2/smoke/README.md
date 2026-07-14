# Smoke suite — NON-BENCHMARK

> **NON-BENCHMARK · NON-GATING · NOT PART OF THE 48 BASE CASES · NOT PART OF HELD-OUT**

This directory exists only to prove the *plumbing* works: that the built qodec
binary and (when available) the pinned RTK binary can be invoked, that qodec is
lossless over arbitrary and RTK-shaped input, and that token accounting uses the
real target tokenizer. It is **not** a benchmark and produces **no** scores.

- These fixtures are **not** real v2 corpus cases.
- There are **no** gold reader questions here.
- Nothing here may appear in a coverage manifest. `case_id`s starting with
  `smoke-` and any case/question tagged `non-benchmark` are rejected by
  `validate_contract.py`.
- The smoke **report** is written to a temporary/output directory and is
  **never committed**. In GitHub Actions it may be uploaded as an artifact.

## Fixtures

| File | Shape |
|---|---|
| `fixtures/build-log.txt` | repeated build/diagnostic log |
| `fixtures/search-listing.txt` | search result over a generated fixture tree |
| `fixtures/test-runner.txt` | test-runner-like output |
| `fixtures/structured.json` | structured JSON |

## Arms & invariants

Arms: `raw`, `qodec`, `rtk`, `rtk+qodec` (the RTK arms run only when a pinned
RTK binary is supplied via `--rtk` or `$RTK_BIN`; otherwise they are recorded as
`skipped: rtk-binary-unavailable`).

Enforced invariants:

```
decode(qodec(raw))        == raw
decode(qodec(rtk(raw)))   == rtk(raw)          # rtk arm only
qodec_tokens  <= raw_tokens                     # target tokenizer, qodec no-gain passthrough
hybrid_tokens <= rtk_tokens                      # rtk arm only
all executions recorded (command, cwd, stream SHAs, exit code, wall time)
RTK source/binary identity recorded
qodec source/binary identity recorded
```

The last two token inequalities use qodec's no-gain **passthrough** and the
**target tokenizer**, never bytes/chars. Semantic equivalence `rtk(raw) == raw`
is **NOT** required or tested — RTK is a lossy reducer, so that would be a
meaningless test.

## Run

```bash
python run_smoke.py --qodec /path/to/qodec [--rtk /path/to/rtk] --out /tmp/smoke-out
# or via the flake:  nix run .#qodec-rtk-smoke
```
