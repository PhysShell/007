# Performance notes (007-specific)

**Conclusion up front:** 007 is **subprocess / LLM-bound**, not compute-bound.
Don't micro-optimize the Rust — profiling would show ~100% of wall-clock in
`Command` waits. The one real lever is **parallelizing the judge's per-file
`claude` calls**.

This grounds the classic Rust perf advice ("Writing words and reading dwords")
against what 007 actually is.

## Where the time goes

Every module's real work is an external process:

| module | spawns | cost |
| --- | --- | --- |
| `judge` | `claude -p` per file | seconds (~8.8s/call measured on a 12-file sonnet run) |
| `agent` | `claude -p` (full-auto run) | seconds → minutes |
| `gate`  | `bash -lc` per step | seconds |
| `worktree` | `git` | milliseconds |

The Rust glue between these calls (dedup, JSON/TOML parse, overlay assembly) is
**microseconds** — six orders of magnitude below the LLM/subprocess latency that
dominates. Cache lines, register allocation, `Box`, loop unrolling, a faster
`from_str_radix` — all real techniques, all **irrelevant** here. Optimizing them
is exactly the "blind optimization" the source article opens by warning against.

## The micro-opt catalog vs 007

| Technique | 007 |
| --- | --- |
| profile first / don't optimize one-time costs / algorithms first | **the principle that applies** — see below |
| cache locality, flat `Vec`, avoid `Vec<Vec>` | N/A — no matrices/nested hot structures |
| keep data in registers, avoid `Box<dyn Trait>` | already so — `Engine` is an enum, no `dyn` in hot paths |
| `smallvec`/`smallstring`, unrolling, `#[inline]`, assert-before-index | N/A — no compute hot loop to feed |
| `TypedArena` for an AST | **N/A — 007 has no AST** (see below) |

## The one real lever — parallelize per-file judge calls

The article's **"Parallelize"** section is the hit. `judge::run` loops over files
**sequentially** (`for (fi, file) in files.iter().enumerate()` → blocking
`call_claude`). The calls are **independent across files**, so on a real run
(~156 findings over many files) sequential wall-clock = the *sum* of every call's
latency — easily 10–20+ minutes. A bounded worker pool cuts that to roughly the
*max* per batch: near-linear speedup in the number of workers.

### Design (build when the real STS run lands)

- **`--jobs N`** (default a small number, e.g. 4) — a bounded pool over the
  per-file work items. Bounded, not unbounded: respect the `claude`/Anthropic
  rate limits; a burst of 156 concurrent calls would throttle or fail.
- **Ordering-safe by construction.** Verdict↔finding pairing is *per file*
  (positional zip within a file's `raws`/`fif`, key fallback otherwise). The
  overlay is a `finding_id → verdict` map assembled after the fact, so files
  completing out of order changes nothing. No new correctness surface.
- **Error isolation.** One file's failure must not abort the run — collect
  per-file results, warn on failures (matches the existing skip-with-warning
  posture), and still emit the overlay for the files that succeeded.
- **Cost is unchanged** — same number of `claude` calls, just issued with
  bounded concurrency instead of one at a time. The win is wall-clock, not $.
- Consider light backoff/retry on transient `claude` failures under concurrency.

Gate: implement alongside the real STS run (design with real data, per `TODO.md`),
not speculatively.

## Out of scope for 007

- **`TypedArena` / AST arenas.** 007 consumes analyzer output as flat
  `Vec<Finding>` → deduped `Vec<Rep>` → a `BTreeMap` overlay. There is no tree to
  arena-allocate. This technique belongs to the repos that *parse* — **snipx**
  (Tree-sitter) and **ownlang** (lexer/parser/codegen) — not here.
- **Release-profile tweaks** (`lto = true`, `panic = "abort"`, `codegen-units = 1`).
  One-liners the article endorses, but they will not move 007's wall-clock (it is
  subprocess-bound). Adopt only if binary size ever matters, not for speed.

## The rule

> The fastest code is code that doesn't run at all; the second-fastest is code
> that never has to wait.

For 007 the waiting is on `claude`, `git`, and `bash` — so the only optimization
that pays is **not waiting on them serially**. Everything else is noise.
