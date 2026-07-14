# Execution environment & reproducibility contract — Interop Benchmark v2

This document is part of the `interop-benchmark-v2` design scope but changes
**none** of its frozen numeric gates. It defines the *canonical execution
substrate* every future capture, smoke run and (eventually) reader run must use,
and the identity fields each run must record. It is a contract about
*reproducibility*, not about scoring.

Nothing here runs a model. External model CLIs and their authentication live
**outside** the Nix derivation; only their *identity* is recorded into the
future manifest.

## Reproducibility identity

Every future capture / run MUST record all of the following:

| Field | Notes |
|---|---|
| `flake_lock_sha256` | SHA256 of the committed `flake.lock` |
| `nix_system` | e.g. `x86_64-linux` |
| `nix_version` | `nix --version` |
| `nixpkgs_revision` | locked `nixpkgs` rev from `flake.lock` |
| `rust_toolchain_identity` | channel + components from `rust-toolchain.toml` |
| `qodec_source_sha` | git SHA of the `qodec/` crate source |
| `qodec_binary_sha256` | SHA256 of the built `qodec` binary |
| `rtk_source_sha` | pinned commit of `rtk-src` |
| `rtk_binary_sha256` | SHA256 of the built `rtk-pinned` binary |
| `tool_versions` | resolved from Nix, never from the runner `PATH` |
| `tokenizer_identity` + `tokenizer_sha256` | the real target tokenizer |
| `locale` | fixed |
| `timezone` | fixed |
| `env_var_allowlist` | explicit allowlist actually exported |
| `command` | argv actually executed |
| `cwd` | working directory |
| `stdin_sha256` / `stdout_sha256` / `stderr_sha256` | stream digests |
| `exit_code` | process exit code |
| `wall_time` | wall-clock duration |

The smoke runner in `smoke/` already emits the execution-identity subset
(command, cwd, stream SHAs, exit code, qodec/rtk source+binary identity) so the
schema is exercised before any real corpus exists.

## Environment policy

- **Nix is the canonical Linux execution environment.** Tool versions come from
  the flake, never from an ambient `PATH`. In particular, RTK is the
  `rtk-pinned` package built from the pinned `rtk-src` commit — never an
  unpinned RTK installed from `PATH`.
- **Minimum supported benchmark platform right now: `x86_64-linux`.** Other
  systems may *build*, but must not be declared *validated* without actual CI on
  that system.
- **Locale is fixed**, e.g. `LC_ALL=C.UTF-8`.
- **Timezone is fixed** as `UTC`.
- **No tool version is taken at random from the runner `PATH`.**
- **Network-dependent captures are forbidden in ordinary PR CI.** Large corpus
  or network jobs run via `workflow_dispatch` / schedule and publish their
  outputs as checksummed Actions artifacts.
- **External model CLIs and authentication stay outside the Nix derivation.**
  Model identity and runtime are still recorded into the future manifest.

## Nix substrate summary

The flake (`/flake.nix`) provides, in addition to the pre-existing `o7`
outputs:

- `packages.qodec` — the `qodec/` crate, built with its own Cargo
  manifest/lock identity.
- `packages.rtk-pinned` — RTK built from the pinned `rtk-src` commit via
  `rustPlatform.buildRustPackage` with RTK's own vendored `Cargo.lock` (no
  mutable prebuilt release binary is ever downloaded).
- `devShells.qodec-bench` — qodec, rtk-pinned, python3 (Nix-pinned deps), git,
  ripgrep, gnugrep, jq, hyperfine, actionlint.
- `apps.qodec-v2-contract-test`, `apps.qodec-rtk-smoke`.
- `checks.qodec-build`, `checks.rtk-pinned-build`, `checks.qodec-v2-contract`,
  `checks.qodec-rtk-smoke`, `checks.github-actions-lint`.

`nix flake check` runs these without any model or tokenizer download from the
network during the execution phase. If a full `nix flake check` unintentionally
pulls heavy, irrelevant outputs, that is documented rather than hidden, and
targeted checks are separated from the full check while the existing `o7`
checks (`o7`, `clippy`, `fmt`) are preserved.

## flake.lock is committed and mandatory

`flake.lock` **is committed** and pins `rtk-src` to commit
`5d32d0736f686b69d1e8b9dc45c007d4eb77a0a2` (`flake = false`) with its
`narHash`, `lastModified` and `rev`. CI evaluates against the committed lock
with `--no-update-lock-file`, so a run **fails** rather than silently relocking
if evaluation ever tries to change it. Do not rely on CI to create unreviewed
lock state.

The `rtk-src` `narHash` was produced by NAR-serialising the pinned tree
(`git archive` of the exact commit, which honours `.gitattributes export-ignore`
exactly as the GitHub tarball nix fetches does) and sha256-hashing it. The
serialiser was validated by reproducing two already-locked `github:` narHashes
in this same lock (`numtide/flake-utils` and `nix-systems/default`) byte-for-byte
before it was trusted for `rtk-src`.

## Smoke RTK invocation model

The non-scoring smoke suite runs **real, pinned RTK**, one `rtk pipe` subcommand
per fixture, driven by `smoke/fixtures/manifest.json`:

| Fixture | `rtk_mode` | `rtk_filter` | Real argv | Observed |
|---|---|---|---|---|
| `build-log.txt` | `pipe-filter` | `log` | `rtk pipe --filter log` | reduced |
| `search-listing.txt` | `pipe-filter` | `grep` | `rtk pipe --filter grep` | passthrough (RTK `never_worse` returned raw) |
| `test-runner.txt` | `pipe-filter` | `cargo-test` | `rtk pipe --filter cargo-test` | reduced |
| `structured.json` | `passthrough` | — | `rtk pipe --passthrough` | passthrough (no supported filter; explicit) |

Rules enforced by the runner: a real `rtk` subcommand is invoked; RTK exit code
must be `0` (nonzero is a smoke failure); stderr is recorded; required RTK stdout
must be non-empty; the exact argv, whether RTK changed the payload, and whether
`never_worse` returned raw are all recorded; each result is classified as
`reduced` or `passthrough` (with `unsupported-explicit-passthrough` support
marking for JSON). RTK output is **not** required to be smaller than raw, because
`never_worse` may legitimately return the raw input.

`packages.rtk-pinned` is built with `pkgs.rustPlatform.buildRustPackage` (using
RTK's own vendored `Cargo.lock`), **not** crane. `packages.qodec` uses crane.

## Orchestration tests vs real RTK integration

Two distinct test surfaces, never conflated:

- **Orchestration unit tests** (`TestSmokeSuite`) prove qodec losslessness over
  arbitrary and hand-authored "RTK-shaped" input. They validate *plumbing only*
  and are explicitly **not** a substitute for RTK integration.
- **Real RTK integration** (`TestRealRtkIntegration` + the
  `checks.qodec-rtk-smoke` derivation) executes the pinned RTK binary: real
  `rtk pipe` exit codes, non-empty output, `argv` containing `pipe`, qodec
  roundtrip over *actual* RTK stdout, hybrid tokens ≤ actual RTK-stdout tokens,
  and proof that an RTK failure or empty output cannot yield a passing report.

## Authoring-environment note

Nix is not installed in the environment that authored this scope, and the raw
GitHub tarball/codeload path is blocked by org egress policy — but `git` egress
is available, which is how the pinned RTK tree was cloned, built (with `cargo`,
producing a binary identical to what `buildRustPackage` compiles) and exercised,
and how the `flake.lock` `narHash` above was computed and validated. The Nix
`nix flake check` / `nix build` commands run in GitHub Actions, whose runners
have full internet and a Nix install.
