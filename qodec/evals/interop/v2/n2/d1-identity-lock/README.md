# Scope N2-D1 — arm identity, tokenizer lock, execution policy

Identity-lock phase of Scope N2-D (arm identity, tokenizer lock, determinism
canary, and token benchmark). Locks raw-arm, QODEC, RTK, and tokenizer
identity plus the execution policy — **before** any determinism canary
(N2-D2) or benchmark execution (N2-D3) runs. Per the governing N2-D
directive, this phase stops and reports rather than resolving every
ambiguity by running something.

See [`n2d1-contract.json`](n2d1-contract.json) for the full, machine-readable
contract. Highlights:

- **Raw-arm identity**: locked for 8 of 17 N2-C primary cases + the N2-A
  canary (durable, hash-locked inputs already exist). **Not yet resolved**
  for the 9 `repository-miner` primary cases — their durable asset only
  contains a source tree + a frozen, unexecuted build/test plan; producing
  their raw input requires an actual build/test capture step. See
  `identified_ambiguities.repository_miner_raw_input`.
- **QODEC identity**: locked. Canonical build = `flake.nix`'s
  `packages.qodec` (Nix/Crane, not the ad hoc `cargo build --release` lane in
  `tools.lock.toml`). Canonical invocation =
  `qodec encode --codec fold-grep-guarded --meter o200k --passthrough-on-no-gain --json`,
  matching the N1 pilot's own frozen choice and the pre-existing
  `rtk-comparison-contract.json`'s "frozen VG" definition.
- **RTK identity**: the `tools.lock.toml` v0.42.4 pin (ad hoc
  `cargo install --git --tag`) and the N1 pilot's real build (Nix
  `packages.rtk-pinned`, exact commit `5d32d073...`) are reconciled in favor
  of the Nix commit-pinned, vendored-lockfile build — chosen for
  reproducibility (immutable commit vs. mutable tag), not performance.
  **Also found**: under the strict "never use an unverified-for-determinism
  filter" policy, all 18 N2-D primary cases currently resolve to
  `rtk pipe --passthrough`, since `--filter log` (the natural fit for CI-log
  content) is proven non-deterministic by the N1 pilot's own evidence, and
  `--filter git-diff`/`--filter cargo-test` have never been tested. See
  `identified_ambiguities.rtk_filter_determinism`.
- **Tokenizer identity**: locked and confirmed by source inspection —
  `qodec/src/meter.rs`'s `Bpe::o200k()` really does call
  `tiktoken_rs::o200k_base()` (tiktoken-rs 0.7.0). No separate tokenizer
  script exists or is needed — N2-D reuses qodec's own
  `encode --meter o200k --json` envelope as the uniform counter for all
  three arms. Conformance fixtures are committed in
  [`tests/test_tokenizer_conformance.py`](tests/test_tokenizer_conformance.py),
  4/4 passing against a real qodec binary.
- **Execution policy**: reused wholesale from the pre-existing
  `qodec/evals/interop/v2/execution-environment.md` contract; N2-D adds no
  new numeric policy of its own.

## Two identified, load-bearing ambiguities — stopping here

1. **`repository_miner_raw_input`** — 9 of 17 N2-C primary cases (53%) have
   no raw benchmark input yet; producing one requires reusing the frozen
   N2-B miner framework to actually execute each case's frozen build/test
   plan. This is real execution, not identity-locking, so it is out of
   scope for N2-D1 itself.
2. **`rtk_filter_determinism`** — RTK's only two filters with any
   determinism evidence are `--filter grep` (deterministic, doesn't apply
   to any N2-D case) and `--filter log` (proven non-deterministic).
   `--filter git-diff` and `--filter cargo-test` are untested. Until a
   symmetric determinism sub-probe is run and passes, RTK is, in effect, a
   passthrough baseline for the full N2-D corpus under current evidence.

Both are reported in `n2d1-contract.json`'s `identified_ambiguities` block
with a proposed resolution path, and both require a user decision before
N2-D2 can proceed.

## Layout

- `n2d1-contract.json` — the full identity-lock contract
- `tests/test_tokenizer_conformance.py` — committed conformance fixtures for
  the o200k meter (real qodec binary, no estimates)

## Scope N2-D1b status (repository-miner raw-input capture)

**Stage 1 (five-ecosystem pilot) and Stage 2 (full 9-case matrix) acceptance
claims are REVOKED.** CI runs #1 through #6 of
`qodec-n2d1b-miner-pilot.yml` all reported job-level "success", and were
initially reported as accepted evidence -- but real inspection of the
actual captured bytes (not just receipt schema and CI conclusion) found
every one of the 18 captures in the "accepted" run was an infrastructure/
sandbox failure, not genuine workload output: rustup couldn't resolve its
default toolchain inside Sandboy confinement (rust), `/dev/null` was never
in the sandbox policy (jvm-maven, jvm-gradle), the per-job Python venv
directory was never exposed to the sandbox (python), and there was no
dotnet-specific trusted-restore step so the confined run attempted its own
NuGet restore under network denial and failed (dotnet).

See:
- [`capture-content-audit-run6.json`](capture-content-audit-run6.json) —
  per-capture real hashes, byte sizes, and root-cause classification for
  all 18 captures.
- [`stage1-and-stage2-acceptance-revocation.json`](stage1-and-stage2-acceptance-revocation.json) —
  the formal revocation record: what remains valid (CI-plumbing evidence
  only), what does not (any raw input from runs #1-#6), and what must
  happen before re-acceptance (a fail-closed content-acceptance gate, then
  a from-scratch re-run, inspected at the byte level).

A fail-closed content-acceptance gate (`tools/content_acceptance.py`) has
since been added to the capture engine, plus fixes for all four root
causes above. The five-ecosystem pilot and full matrix must both pass
content-level acceptance on fresh captures before any new Stage 1/Stage 2
acceptance record may be built.

## What N2-D1 does not do

Execute the determinism canary (N2-D2) or the primary token benchmark
(N2-D3), execute any repository-miner build/test plan, run an RTK
filter-determinism probe, modify N2-C or N2-D0's frozen outputs, or declare
a winner. See `n2d1-contract.json`'s `stop_and_report` block.
