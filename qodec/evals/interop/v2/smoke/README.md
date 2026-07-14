# Smoke suite — NON-BENCHMARK

> **NON-BENCHMARK · NON-GATING · NOT PART OF THE 48 BASE CASES · NOT PART OF HELD-OUT**

This directory exists only to prove the *plumbing* works with **real, pinned
RTK**: that qodec and the pinned RTK binary can be invoked, that qodec is
lossless over the *actual* RTK output, that token accounting uses the real
target tokenizer, and that a full reproducibility-identity block is recorded. It
is **not** a benchmark and produces **no** scores.

- These fixtures are **not** real v2 corpus cases.
- There are **no** gold reader questions here.
- Nothing here may appear in a coverage manifest. `case_id`s starting with
  `smoke-` and any case/question tagged `non-benchmark` are rejected by
  `validate_contract.py`.
- The smoke **report** is written to an output directory and is **never
  committed**. In GitHub Actions it is uploaded as an artifact
  (`if-no-files-found: error`).

## Fixtures and real RTK invocation

Driven by `fixtures/manifest.json` (`fixture`, `rtk_mode`, `rtk_filter`). Each
fixture runs a **real `rtk pipe` subcommand** — never a bare no-op invocation:

| Fixture | Shape | `rtk_mode` | argv |
|---|---|---|---|
| `build-log.txt` | repeated build/diagnostic log | `pipe-filter` | `rtk pipe --filter log` |
| `search-listing.txt` | search result over a generated tree | `pipe-filter` | `rtk pipe --filter grep` |
| `test-runner.txt` | test-runner-like output | `pipe-filter` | `rtk pipe --filter cargo-test` |
| `structured.json` | structured JSON | `passthrough` | `rtk pipe --passthrough` |

RTK's `never_worse` guard may return the raw input (e.g. `grep` on the search
fixture), which is recorded as a `passthrough` — RTK output is **not** required
to be smaller than raw.

## Invariants (enforced; nonzero exit on any failure)

```text
all mandatory identity fields populated
decode(qodec(raw))        == raw
qodec_tokens  <= raw_tokens                       # target tokenizer, no chars/4
rtk execution succeeded   (exit code == 0)         # nonzero is a smoke failure
rtk stdout non-empty                               # per non-empty fixture
decode(qodec(rtk(raw)))   == rtk(raw)              # qodec lossless over ACTUAL rtk stdout
hybrid_tokens <= rtk_tokens                         # target tokenizer
```

Each RTK result records: exact `argv`, `exit_code`, stdout/stderr digests,
`changed`, `never_worse_returned_raw`, and a `classification` of `reduced` or
`passthrough`. A pinned RTK binary is **required** (`--rtk` / `$RTK_BIN`); the
runner fails without it. Semantic equivalence `rtk(raw) == raw` is **NOT** tested
— RTK is a lossy reducer.

## Reproducibility identity (mandatory)

The report includes `flake_lock_sha256`, `nix_system`, `nix_version`,
`nixpkgs_revision`, `rust_toolchain_identity`, `qodec_source_sha`
(`repository_commit_sha` + `qodec_tree_sha`), `qodec_binary_sha256`,
`rtk_source_sha`, `rtk_binary_sha256`, `locale`, `timezone`,
`environment_variable_allowlist`, `tokenizer_identity` and `tokenizer_sha256`.
**The runner fails if any mandatory identity field is absent.**

## Orchestration vs integration

- Orchestration unit tests (`TestSmokeSuite`) validate plumbing on arbitrary /
  hand-authored input — **not** a substitute for RTK integration.
- Real RTK integration (`TestRealRtkIntegration` + `checks.qodec-rtk-smoke`)
  executes the pinned RTK binary.

## Run

```bash
python run_smoke.py --qodec /path/to/qodec --rtk /path/to/rtk --out /tmp/smoke-out
# or via the flake:  nix build .#checks.x86_64-linux.qodec-rtk-smoke --no-update-lock-file
```
