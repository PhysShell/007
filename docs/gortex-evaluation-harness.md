# Gortex evaluation harness — admissibility boundary, not a verdict

Status: design decision · Scope: external code-graph evidence consumed by `o7`
planning, review, and correction

## Purpose

[Gortex](https://github.com/zzet/gortex) is a local code-intelligence engine
that indexes repositories into a graph and exposes it over CLI, MCP, HTTP, and
a web UI. It is the closest external neighbour to several things `o7` wants:
provenance-tagged edges, artifact nodes, per-session working sets, staleness
notification, and token-bounded context assembly.

This document specifies how to evaluate it. **The question is not "is Gortex
good".** The question is:

> Which Gortex outputs may `o7` consume as evidence without independent
> verification, and which must be treated as `UNKNOWN` until a second observer
> agrees?

The deliverable is therefore not a score. It is an **admission table** mapping
Gortex response metadata onto the epistemic statuses already defined in
`docs/decision-and-admission-protocol.md`. A tool that fails most of this
harness is still useful inside a narrow admitted band; a tool that scores well
on average is still unsafe if its failures are indistinguishable from its
successes.

## Why the boundary is the interesting object

Gortex's own published benchmark (`BENCHMARK.md` in its repository) reports,
for `search_symbols` retrieval:

```text
exact tier      R@5  96.8 %
concept tier    R@5  25.4 %
multi-hop tier  R@5  30.0 %
overall         R@5  55.1 %   (ripgrep baseline 17.3 %)
```

Two caveats belong next to those numbers whenever they are cited: the corpus is
the Gortex repository itself, and the gold set is hand-authored by the project.
They are self-reported, not independently replicated. That is not an accusation
— it is the normal state of a four-month-old project — but it is why we run our
own.

The shape matters more than the magnitude. Gortex has good evidence for
**anchor → context** and weak evidence for **intent → anchor**. Concretely:

- given `FooService.RefreshToken`, find callers, implementations, contracts,
  blast radius — this is the 96.8 % path;
- given *"where is it decided whether a payment may be re-submitted?"*, find the
  anchor first — this is the 25–30 % path.

These are different conditional probabilities and must be measured separately:

```text
P(useful context | correct seed)     anchor → context
P(correct seed  | natural-language task)   intent → anchor
```

Collapsing them produces the failure mode this harness exists to catch: a
compact, fast, well-structured context window describing the wrong part of the
system. Efficient delivery of the wrong thing is still delivery of the wrong
thing.

## The capability/confidence contract

Gortex tiers its language support (`docs/languages.md` in its repository):

| Tier | Extractor | Count | Provides |
|------|-----------|-------|----------|
| 1 | Bespoke tree-sitter, hand-tuned queries | ~30 | symbols, resolved call edges, ORM/contract/dataflow, accurate ranges |
| 2 | Regex + structural heuristics | ~60 | top-level symbols and imports; call edges vary per language |
| 3 | Generic `go-sitter-forest` signature extractor | ~165 | definitions and basic edges; **no** scope-aware resolution, contracts, ORM, or dataflow |

The tier boundary is stated plainly by the project. The open question is
whether it is *propagated* — whether a result derived from a Tier 3 extractor
is reliably marked as heuristic and incomplete, or whether it can surface as a
high-confidence, low-impact, safe-to-rename verdict.

Gortex issue [#461](https://github.com/zzet/gortex/issues/461) is the reference
case: GDScript call edges resolved by method name only, ignoring the receiver,
producing phantom edges and missing real ones; impact analysis reported low
risk and a rename proposed two sites instead of twenty-plus. It was closed as
completed on 2026-08-05, with the fix retaining the receiver and its inferred
type so the edge can rise to `ast_resolved`.

**We do not claim this recurs across the other Tier 2/3 languages.** There is no
evidence for that, and not every lower-tier extractor necessarily resolves by
name alone. What the case establishes is narrower and sufficient:

> The binding between a language's extraction capability and the epistemic
> confidence of a result derived from it is a contract whose enforcement we have
> not measured.

That contract is what surface S3 below tests.

## Surfaces

Four surfaces, measured in this order. Correctness gates economy: a token
figure computed over wrong answers is not a saving.

### S1 — Localization (intent → anchor)

Natural-language task descriptions in, ranked candidate anchors out.

- Task classes: exact (identifier known), concept (behaviour described),
  multi-hop (requires traversal to state).
- Metrics: Recall@1/5/20, MRR.
- Stratified by language tier.

Downstream task success is the metric we actually care about, but it requires
an agent in the loop and is expensive and noisy. Decouple: run Recall@k against
gold anchors over the full set (deterministic, cheap), then run agent-in-loop on
a 10–15 task subsample **to check that Recall@k correlates with success at all**.
If it does not correlate, that is itself the result and S1's cheap metric is
retired.

### S2 — Graph fidelity (anchor → facts)

Correct symbol supplied up front; ask for callers, callees, implementations,
impact, contracts. Precision/recall against an independent oracle.

**Seed injection rule.** The seed must be supplied as a **resolved symbol ID
from the graph**, never as a name string. A name string re-enters Gortex's own
exact-tier resolver (96.8 %), which leaks S1 into S2 at precisely the point the
two surfaces are meant to be separated.

**The oracle constraint — read before stratifying.** An independent oracle for
callers and implementations means a compiler or an LSP. That exists for Tier 1.
It does not exist for Tier 2/3 — which is *why* those languages are Tier 2/3.
Correctness on the tiers we trust least is therefore unmeasurable by
construction. Two honest routes, and the choice must be recorded:

1. **Hand-labelled sample.** 30–50 symbols in one Tier 3 language, ground truth
   established manually. Expensive; the only route to real correctness numbers
   below Tier 1.
2. **Honesty-of-caveats instead of correctness.** Do not measure whether the
   answer is right; measure whether Gortex marked it `heuristic` /
   `coverage incomplete` in the cases where its own documentation says it cannot
   know. This needs no oracle, because the predicate is about the system's
   self-description rather than about the world.

Route 2 is cheaper and is arguably the better fit for this harness's stated
goal, since the admission table consumes exactly that self-description. Route 1
is the fallback if route 2 shows the self-description is unreliable.

### S3 — Confidence calibration and the admission table

The centre of the harness.

For every S1/S2 result, record the response metadata alongside the outcome:

```text
tier          1 | 2 | 3
origin        lsp_resolved | ast_resolved | inferred | text_matched
confidence    scalar as reported
caveats       coverage/incompleteness markers present or absent
outcome       correct | incomplete | wrong
```

**Pre-registration is mandatory.** "Reliable enough to act on" is *our*
threshold, not Gortex's. Chosen after inspecting results, it can be moved to
produce any false-safe rate we like — measuring our own leniency rather than the
tool. The admission policy is therefore fixed and committed **before** the
measurement run, and the run is scored against the committed version.

**Do not reduce this to a scalar.** A single false-safe rate compresses away the
information the admission table needs. Produce a reliability breakdown: bucket
by `(tier, origin, claimed confidence)` and report observed accuracy per bucket.
What `o7` consumes is the inverse mapping — given a bucket, what is the
empirical P(correct).

`docs/decision-and-admission-protocol.md` already fixes the target vocabulary,
including the constraint this harness must respect:

> A scalar probability may later help scheduling or prioritization. It must not
> silently turn an unproved merge precondition into a green check.

Gortex emits a scalar plus an origin. `o7` admits categorical statuses. The
harness output is the translation layer:

```text
(tier, origin, confidence, caveats)  ->  ESTABLISHED | UNKNOWN | ...
```

with `UNKNOWN` as the default for every bucket not explicitly admitted, and
`ESTABLISHED` granted only where the observed error rate justifies it. Buckets
that never appear in the measurement run are not admitted — absence of evidence
is `UNKNOWN`, not a pass.

The frightening metric, reported per bucket rather than overall:

```text
false-safe rate = share of wrong or incomplete results that Gortex presented
                  as sufficient to act on, under the pre-registered policy
```

### S4 — Token economy, conditional on correctness

Not tokens returned. Either of:

```text
tokens / correctly recovered gold fact
tokens / successfully completed task
```

A 150-token answer at 25 % recall is not six times better than a 900-token
answer at 100 % recall. It lost cheaply. Under the second metric, tokens spent
on failed tasks correctly diverge.

## Adversarial ambiguity fixtures

A separate fixture target with ground truth known by construction. These are the
cases that turn an approximately-correct graph into dangerous automation:

- two identically named methods on sibling classes;
- alias / re-export, including a barrel file re-exporting a renamed symbol;
- overload sets;
- generated members;
- dynamic dispatch through an interface with several implementors;
- an identically named symbol in a different repository of the same workspace;
- a stale file — indexed, then modified on disk;
- a deleted symbol — removed in the working tree, still live in the index.

The last two test the staleness model rather than the resolver, and are the ones
most likely to interact with `notifications/stale_refs` and per-session working
sets.

This fixture set is buildable without installing Gortex at all, and is the
cheapest available next step.

## Consumption architecture

The harness exists to justify one of these two shapes, not to pick a favourite:

```text
admitted                               not admitted
--------                               ------------
task                                   task
 -> o7 localization / evidence policy    -> Gortex smart_context
 -> candidate anchors                    -> trust
 -> Gortex graph expansion
 -> independent verification where
    the verdict is irreversible
```

The left shape spends Gortex on the leg it has evidence for. The right shape
stakes the system on its least-demonstrated leg. Nothing in this harness
presumes the left shape wins — but the burden of the measurement falls on the
right one.

## Out of scope

**GCX1** — Gortex's tab-delimited, round-trippable MCP wire format, reporting a
median −27.4 % tiktoken saving against JSON and 100 % round-trip integrity over
20 representative responses (`docs/wire-format.md` in its repository). This is a
separable and much cleaner research object: round-trip fidelity, byte and token
cost, parsing cost, pathological fixtures, schema evolution. It belongs to
[`PhysShell/qodec`](https://github.com/PhysShell/qodec) and should be pursued
independently of whether Gortex itself passes this harness.

**Adopting Gortex.** This document specifies a measurement, not an integration.
No dependency, MCP wiring, or sidecar is implied by running it.

## Status of the numbers cited here

Verified against the Gortex repository on 2026-08-07: the tier counts and their
stated capability boundary, the R@5 figures and their self-corpus, the GCX1
saving and round-trip claim, and the state of issues #461 and #465 (both closed
completed on 2026-08-05). Note a minor internal inconsistency in the upstream
project: the README says 257 languages, `docs/languages.md` says 256.

None of these are independently replicated. Replicating them is not this
harness's goal — bounding their consumption is.
