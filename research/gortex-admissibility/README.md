# Gortex admissibility — adversarial fixture corpus

```text
STATUS:  READ-ONLY R&D
         NON-AUTHORITATIVE
         MUST NOT DRIVE A-SERIES TRANSITIONS
```

Fixture corpus for the harness specified in
[`docs/gortex-evaluation-harness.md`](../../docs/gortex-evaluation-harness.md).

The harness measures **where an external code-graph observer's output may be
consumed**, not whether the tool is good. This corpus is its constructed-oracle
layer: cases authored so that ground truth holds by construction, which is what
makes them usable in languages and relations where no compiler or language
server can be pointed at the question.

Nothing here requires Gortex to be installed. The corpus is a measuring
instrument and is written against no particular tool.

## Structure

```text
schema/      oracle-v0 (resolution) and lifecycle-v0 (index lifecycle)
fixtures/    case-000x resolution cases; case-010x lifecycle cases
tools/       check.sh -- validates the instrument, invokes no tool under test
results/     measured results per case (git-ignored until a run is promoted)
```

**Corpus v1 is frozen.** Eight cases: seven resolution, one lifecycle. No
further fixtures before the first real measurement run. Growth from here is
driven by defects that run surfaces, not by anticipation.

Each case:

```text
<case>/
  source/         the code under test
  oracle.yaml     resolution cases: ground truth AND admissibility expectations
  lifecycle.yaml  lifecycle cases: one entity, its states, four questions each
  tools/          per-case executables where a claim must be constructed rather
                  than asserted (codegen, mutation steps)
  README.md       what the case is adversarial about
```

## The two blocks in `oracle.yaml`

This is the corpus's one real idea, so it is worth stating plainly.

```yaml
facts:            # scores CORRECTNESS   (harness surface S2)
  - relation: callers
    expected: [...]     # must be present; missing one is an incompleteness failure
    forbidden: [...]    # phantom edges the case is built to provoke

    admissibility:      # scores HONESTY  (harness surface S3)
      may_claim_complete: false
      may_claim_safe_to_rename: false
      required_caveats: [dynamic_dispatch]
```

A system that returns an incomplete caller set is merely **wrong**. One that
returns the same set while claiming completeness is **unsafe**. Only the second
block separates them, and the separation is the entire reason this corpus exists
rather than a plain recall benchmark.

Three outcomes per fact, not two:

```text
right                                    pass
wrong, correctly caveated                miss    -- honest, recorded, not alarming
wrong, presented as sufficient to act    FALSE-SAFE
```

`case-0003` and `case-0007` add a fourth: **right, but presented as exhaustive
when exhaustiveness is not available** — also a false-safe, on an edge set that
matches `expected` exactly.

## Package 1 — resolution (this directory)

| Case | Provokes | Primary axis |
|------|----------|--------------|
| `case-0001-sibling-same-name` | name-only resolution, receiver ignored | correctness (phantom) |
| `case-0002-alias-reexport` | renaming re-export through a barrel | correctness (silent miss) |
| `case-0003-interface-dispatch` | dynamic dispatch over two implementors | **honesty** |
| `case-0004-cross-repo-same-name` | workspace index merging repository roots | correctness (over-reach) |
| `case-0005-overload-set` | overload set collapsed to one symbol | correctness (both directions) |
| `case-0006-generated-members` | generated definition, contract is the source of truth | safety ≠ correctness |
| `case-0007-sibling-same-name-lua` | case-0001 where the language settles less | **calibration pair** |

`case-0001` is the control vertical: a minimal reproduction of the defect class
behind upstream issue [#461](https://github.com/zzet/gortex/issues/461) — call
edges resolved by method name with the receiver ignored — reproduced without
depending on GDScript, on that issue, or on whether its fix landed.

`case-0007` mirrors `case-0001` into Lua and is scored **as a pair with it**.
Same shape, same semantics, different admissible claim, because the language
settles less. A tool reporting equal confidence on both has not connected what
the source determines to what it is willing to assert.

Package 1 is **frozen by design**. The seven cases cover ambiguity, aliases,
dispatch, cross-repo, overloads, generated source-of-truth, and the calibration
pair. Adding a twentieth variant would grow a collection, not the instrument.

## Package 2 — index lifecycle

One case, `case-0101-index-lifecycle`: one entity followed through five states,
scored on four questions per state, driven by scripted mutation rather than a
snapshot pair.

```text
s0-fresh            observation taken here
s1-file-modified    old answer becomes short, not wrong
s2-symbol-deleted   entity gone, tree valid
s3a-file-removed    entity gone, import left dangling
s3b-repo-removed    repository gone
```

```text
Q1  graph_current_correct    does a fresh query answer correctly now
Q2  stale_signal_required    is the holder of the old answer told it expired
Q3  old_observation_valid    does the fact handed out still hold
Q4  action_admissible        may an action derived from it still proceed
```

Q1 does not substitute for Q2, and conflating them is the failure this package
exists to catch. An index that converges on state B fifty milliseconds after an
agent was handed a fact at state A passes Q1 at every state here and can still
fail Q2, Q3 and Q4 at all of them. Instantly correct about the present and
silent about what it said in the past is not freshness — it is a race, and the
consumer absorbs it.

`s1` is the sharpest: the old observation is not contradicted, only **short**.
Nothing about the cached answer looks wrong, and a rename driven by it edits one
site of two.

Package 2 is scoped to this one case on purpose. No symlink matrix, no
`git reset`, no concurrent-watcher permutations. A new fixture gets added when a
measurement run produces a defect that needs one — on evidence, not in
anticipation.

## Deliberate non-assertions

- **No case asserts an extractor tier.** Tier is a property of the tool and its
  configuration at measurement time and is recorded in the result. The oracle
  records `language`, never a tier.
- **No case asserts that a tool will fail it.** The corpus states what the
  source determines and what may be claimed about it. Which tools fall where is
  the measurement, not the premise.
- **Oracle availability is per `(language, relation)`.** `case-0007` records
  `independent_oracle.available: false` because Lua does not determine the
  answer for a parameter receiver — not because of any extractor's tier.
  Deriving "no oracle" from "low tier" would let our own bookkeeping declare
  part of the world unmeasurable.

## Scoring a run

1. Resolve each seed and check it against `seeds[].gold_identity` **before
   scoring any expansion**. A mis-resolved seed is a localization failure (S1)
   and must not be recorded as a fidelity result (S2).
2. Score `expected` / `forbidden` per fact → correctness.
3. Score claims made against `admissibility` → honesty, and the false-safe rate.
4. Record `(tier, origin, confidence, caveats, identity_binding)` per result for
   the S3 reliability breakdown.

Thresholds for what counts as "sufficient to act on" are pre-registered in the
harness document and committed before a measurement run. Chosen afterwards, they
measure our own leniency rather than the tool.

## Validating the instrument

```sh
tools/check.sh
```

Five checks, reported separately because they prove different things:

| | Check | Proves |
|---|---|---|
| **A** | oracle integrity | oracles parse, enums are valid, every path and cited line resolves |
| **B** | type validity (`tsc --strict`) | each fixture is a valid program |
| **C** | relation oracle (language service) | the claimed caller / reference / implementation sets are the ones TypeScript computes |
| **D** | codegen stability | `case-0006`'s generated file is what its contract generates |
| **E** | lifecycle package | lifecycle oracles are well-formed and every mutation state applies, asserts its postcondition, and resets |

No code-graph engine is invoked by any of them; the script validates the
measuring instrument only.

**B is not C, and the distinction was a real defect here.** An earlier revision
ran only `tsc --noEmit` while claiming the constructed oracles had been
"validated against tsc". Type-checking proves a fixture compiles and says
nothing about *whose* callers those are — the control vertical's whole claim.
Step C closes that: it drives `findReferences`,
`getImplementationAtPosition`, and — for overload attribution —
`getResolvedSignature`, then compares against each `oracle.yaml`.

A missing toolchain reports **UNAVAILABLE**, never PASS, and the summary says
explicitly that the relation claims stand unproved in that run.

A TypeScript fact that step C *could not evaluate* — a seed that does not
resolve to a declaration — is a **failure**, not a skip. Reported as `UNEVAL`.
An unresolved seed is a corpus defect, and letting it pass would be the
instrument committing the exact fault it exists to detect: reporting `PASS` for
a check that did not run. Only a whole case in a language out of the oracle's
reach (`case-0007`, Lua) skips benignly, and it skips at case level where it is
visible.

Step C earned its place on first execution by failing: it rejected
`case-0005` fact 0, and the disagreement was real. A call never *selects* an
implementation signature — TypeScript resolves it to one of the overload
signatures — so per-signature attribution must apply only when the seed is
itself an overload signature. Seeding the implementation asks which calls reach
that body, which is a different question with a different answer. The checker
knew; the corpus did not, until C was run.

Step C covers package 2 as well: `case-0101`'s observation is taken at the
committed fresh state, so it is computable by the same language service and is
checked there. Leaving it asserted would have reproduced in package 2 exactly
the defect package 1 was corrected for.

Status on this container: A/B/C/D/E all PASS — eight oracles clean, seven
TypeScript cases compiling under `--strict`, twelve relation facts agreeing with
the language service, five mutation states applying and resetting cleanly, one
non-`.ts` expectation correctly reported as out of that oracle's reach rather
than silently dropped. `case-0007` is skipped by C by
construction: no Lua toolchain is present, and none is required, since nothing
in that case depends on the file executing.
