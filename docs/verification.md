# Verification harnesses

The verification ROI from `security-layers.md`, wired up. Three layers over the
pure functions and the untrusted-input parsers, in ascending effort.

To reach the pure functions from out-of-tree harnesses, the crate now exposes a
library target (`src/lib.rs`); `src/main.rs` is a thin CLI over it. The fuzzed
entry points are `o7::judge::extract_json_array`, `o7::judge::parse_findings_json`,
and `o7::gate::GateManifest::parse`.

## proptest — property tests (stable)

Runs as part of the normal test suite, no extra tooling:

    cargo test

Covers `finding_id` (16-hex shape, determinism, the dedup-critical
same-`(path,rule)`/different-`message` ⇒ different-id invariant),
`extract_json_array` (panic-free; `Some` ⇒ bracket-delimited), and `sanitize`
(path-safe, length-preserving). See the `proptest!` block in `src/judge.rs`.

## cargo-fuzz — the untrusted-input parsers (nightly)

Three targets under `fuzz/fuzz_targets/`, each asserting "never panics" (plus the
bracket invariant for the model-output parser):

- `extract_json_array` — the model's raw stdout (least trusted).
- `findings_json` — `serde_json` over own-check `findings.json`.
- `gate_toml` — `toml` over a target repo's `.007/gate.toml`.

Run:

    cargo +nightly fuzz run extract_json_array -- -max_total_time=60
    cargo +nightly fuzz run findings_json      -- -max_total_time=60
    cargo +nightly fuzz run gate_toml          -- -max_total_time=60

Status: all three ran clean on this box (18.4M / 1.5M / 1.0M executions, 0 crashes).
Corpora and artifacts are git-ignored (`fuzz/.gitignore`).

## Kani — bounded no-panic proofs (nightly + CBMC)

Two `#[kani::proof]` harnesses in `src/judge.rs` (behind `#[cfg(kani)]`, invisible
to normal builds) prove, symbolically over bounded inputs, that
`extract_json_array` and `sanitize` never panic and hold their output invariants —
slice-boundary safety being exactly Kani's sweet spot.

    cargo kani --harness extract_json_array_never_panics
    cargo kani --harness sanitize_is_panic_free_and_path_safe

> **Note:** Kani could not be exercised in this sandbox — `cargo kani setup`
> downloads its release bundle from `github.com/.../releases/download/...`, a host
> the session's egress policy blocks (HTTP 403). The proofs are authored and ready;
> run them where the Kani bundle is reachable. `cfg(kani)` is registered in
> `Cargo.toml` (`[lints.rust] unexpected_cfgs`) so stable builds stay warning-free.

## Static analysis & supply chain

### Lints (`[lints]` in `Cargo.toml`)

Enforced by `cargo clippy --all-targets` (the nix `flake check` runs it with
`-D warnings`). Curated to pass **clean today** — a ratchet, not a blast, per the
project's own "false positive is worse than a miss" directive:

- `unsafe_code = "forbid"` — there is no `unsafe` in the tree; locked in.
- `unreachable_pub = "deny"`, `rust_2018_idioms = "deny"`.
- clippy restriction set that is already clean: `unwrap_used`, `expect_used`,
  `panic`, `dbg_macro`, `todo`, `unimplemented`, `indexing_slicing`.
- The only two index sites (`reps[i]` over internally-stored indices) carry a
  justified `#[allow(clippy::indexing_slicing)]` documenting the in-bounds invariant.

### Supply chain (`deny.toml`)

    cargo deny check

Gates advisories (RUSTSEC, yanked crates), a permissive-only license allow-list,
`wildcards = "deny"`, and crates.io-only sources.

### Deliberately deferred (the ratchet)

Left off on purpose — turning these on wholesale today produces fatigue, not signal:

- clippy `pedantic` + `nursery` — ~59 warnings on the current tree (mostly
  `missing_errors_doc`, name-repetition, `must_use`). Adopt at `warn` with a
  baseline, or fix in slices.
- `missing_debug_implementations`, `arithmetic_side_effects` — noisy on the
  internal structs / counters; enable per-slice.
- `print_stdout` — **N/A**: `o7` is a CLI, stdout is the product.
- Next tools (highest ROI first): `cargo-mutants` (do the tests actually assert?),
  `cargo-semver-checks` (once the lib API is consumed), `cargo-udeps`,
  `cargo-hack --feature-powerset`, and CodeQL/Semgrep in Actions across the
  (public) sibling repos.
