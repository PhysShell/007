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

The deliverable is therefore not a score. It is an **observer admission table**:
a mapping from Gortex response metadata to the role that observer is permitted
to play when a claim is being decided. It does *not* assign epistemic statuses
to claims — `docs/decision-and-admission-protocol.md` owns that, and S3 below
explains why the distinction has to be enforced. A tool that fails most of this
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

The shape matters more than the magnitude — but read the shape precisely,
because it is easy to overclaim in Gortex's favour here.

All four figures above measure **one operation**: `search_symbols` retrieval,
i.e. locating an anchor. They stratify by how the query is phrased:

- given the identifier `FooService.RefreshToken`, locate that symbol — this is
  the 96.8 % exact tier;
- given *"where is it decided whether a payment may be re-submitted?"*, locate
  the anchor — this is the 25–30 % concept/multi-hop tier.

**Exact-identifier localization has R@5 96.8 %. The fidelity of the subsequent
graph expansion — callers, implementations, contracts, blast radius — is not
established by that benchmark at all.** It measures finding the anchor, not
what is returned once the anchor is held. Establishing the second leg
independently is precisely the job of S2 below; until S2 runs, we have a
published number for one leg and nothing for the other.

So the two conditional probabilities to measure separately are:

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

**Seed injection rule.** The seed must be supplied as a resolved symbol ID,
never as a name string — a name string re-enters Gortex's own exact-tier
resolver and leaks S1 into S2 at precisely the point the two surfaces are meant
to be separated.

That is necessary but not sufficient. The ID must additionally be **bound to a
gold identity** held by the fixture:

```text
(repo commit, path, byte/line range, expected symbol)
```

and the binding checked before the expansion query is scored. An ID taken on
faith because Gortex returned it lets a mis-identified seed back in through the
side door: the expansion would then be scored against the wrong anchor, and a
localization error would be recorded as a fidelity result.

**Oracle availability is a per-(language, relation) fact, not a tier fact.**
Gortex's tier describes *its own extractor*, not what independent analysis
exists in the world. A lower-tier language may well have a compiler, a language
server, or another static-analysis oracle that Gortex simply does not use.
Deriving "no oracle" from "Tier 3" would let us declare part of the world
unmeasurable by our own bookkeeping.

The unit is therefore a matrix, filled in explicitly per fixture:

```text
(language, relation) -> independent | manual | constructed | unavailable

independent    a compiler / LSP / analyser outside Gortex answers it
manual         ground truth established by hand
constructed    the fixture is authored so ground truth holds by construction
unavailable    none of the above is obtainable at acceptable cost
```

Tier remains a *stratification of results* — we report per tier because we
expect the answer to vary with it — but it does not determine whether ground
truth is obtainable.

**Route decision (settled).** Two measurement routes were considered: a
hand-labelled sample, and honesty-of-caveats in place of correctness. The
adopted plan takes the second first, backed by constructed oracles, and defers
the first to escalation:

```text
Tier 1        independent compiler/LSP oracle
              + adversarial constructed fixtures

Tier 2/3      honesty-of-caveats broad survey
              + adversarial constructed fixtures
                        |
                        v
              if material ambiguity remains
                        |
                        v
              hand-labelled sample (30-50 symbols, one language)
```

The rationale for the pairing: honesty-of-caveats answers the harness's central
question — does the system report the boundary of its own knowledge truthfully
— and needs no oracle. Constructed adversarial fixtures stop it from passing on
good manners alone, since ground truth there holds by construction even where
no compiler exists. Neither layer substitutes for the other; the hand-labelled
sample is an escalation, not a prerequisite.

### S3 — Confidence calibration and the observer admission table

The centre of the harness.

For every S1/S2 result, record the response metadata alongside the outcome:

```text
tier              1 | 2 | 3
origin            lsp_resolved | ast_resolved | inferred | text_matched
confidence        scalar as reported
caveats           coverage/incompleteness markers present or absent
identity_binding  does the result name a commit/path/range we can pin
outcome           correct | incomplete | wrong
```

**Pre-registration is mandatory.** "Reliable enough to act on" is *our*
threshold, not Gortex's. Chosen after inspecting results, it can be moved to
produce any false-safe rate we like — measuring our own leniency rather than the
tool. The admission policy is therefore fixed and committed **before** the
measurement run, and the run is scored against the committed version.

**Do not reduce this to a scalar.** A single false-safe rate compresses away the
information the observer admission table needs. Produce a reliability breakdown: bucket
by `(tier, origin, claimed confidence)` and report observed accuracy per bucket.
What `o7` consumes is the inverse mapping — given a bucket, what is the
empirical P(correct).

#### The harness calibrates an observer; it does not establish facts

This separation is load-bearing, and an earlier draft of this document got it
wrong. `docs/decision-and-admission-protocol.md` defines `ESTABLISHED` as a
*specific claim supported by current, identity-bound evidence*, and separately
forbids turning a probability into a green check:

> A scalar probability may later help scheduling or prioritization. It must not
> silently turn an unproved merge precondition into a green check.

A bucket with 99.9 % historical accuracy does not make today's `Foo → Bar`
an established fact. It only says what role this observer is permitted to play
when that fact is being decided. Emitting `ESTABLISHED` directly from a
calibration bucket would be exactly the prohibited move, one layer lower down
where it is harder to see.

The harness output is therefore an **observer admission policy**, in its own
vocabulary:

```text
(tier, origin, confidence, caveats, identity_binding)
    ->
NOT_ADMISSIBLE            not usable as evidence for anything
NAVIGATION_HINT           may steer search; may not appear in a rationale
SUPPORTING_OBSERVER       may corroborate, never sufficient alone
CORROBORATION_REQUIRED    admissible once a second observer agrees
SOLE_OBSERVER_ALLOWED     admissible on its own
```

The canonical decision layer, unchanged and still the only thing that speaks
the protocol's vocabulary, then decides per claim:

```text
current evidence
  + observer admission policy
  + exact identity binding
  + required corroboration
    ->
ESTABLISHED | UNKNOWN | STALE | CONFLICTING | ...
```

Two standing constraints on the table this harness produces:

- **`SOLE_OBSERVER_ALLOWED` is forbidden for irreversible and admission
  decisions**, at any measured accuracy. For navigation and reversible planning
  it is potentially available. For merge or admission, Gortex may be supporting
  evidence but never the sole cause of a green state. This follows from the
  existing reversible-first policy rather than adding to it.
- **Unobserved buckets are `NOT_ADMISSIBLE`, not unclassified.** A bucket that
  never appeared in the measurement run has no calibration behind it; absence of
  evidence is not a pass.

The frightening metric, reported per bucket rather than overall:

```text
false-safe rate = share of wrong or incomplete results that Gortex presented
                  as sufficient to act on, under the pre-registered policy
```

"Sufficient to act on" is read against the pre-registered observer admission
policy, not against Gortex's scalar. A wrong result that arrived marked
`inferred` with a coverage caveat is a miss; the same wrong result arriving in
a bucket we had pre-registered as `SOLE_OBSERVER_ALLOWED` is a false-safe.

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

Each case is a minimal triple:

```text
<case>/
  source/       the code under test
  oracle.yaml   ground truth AND admissibility expectations
  README.md     what the case is adversarial about
```

`oracle.yaml` carries two blocks, and the second is the reason this corpus
serves S2 and S3 at once:

```yaml
facts:
  callers:
    expected: [...]
    forbidden: [...]      # phantom edges this case is built to provoke

admissibility:
  may_claim_complete: false
  may_claim_safe_to_rename: false
  required_caveats:
    - coverage_incomplete
```

`facts` scores correctness. `admissibility` scores honesty: a system that
returns an incomplete caller set is merely wrong; one that returns it while
claiming completeness is unsafe, and only the second block distinguishes them.
A case can therefore fail on fidelity, on honesty, or on both — and the third
outcome, wrong-but-correctly-caveated, stops the corpus from being scored as a
simple pass/fail.

**Ordering.** The first control vertical is *same-named methods on sibling
classes* — a minimal reproduction of the #461 defect class that does not depend
on #461, GDScript, or any upstream fix. Then alias/re-export, interface
dispatch, and cross-repo same-name.

The stale-file and deleted-symbol cases ship as a **separate second package**:
they exercise index lifecycle and invalidation rather than resolution, and need
a mutation step the resolution cases do not. Mixing them into the first package
would conflate two different failure modes under one score.

Package 2 takes four states of one entity rather than two isolated cases —

```text
1  fresh indexed symbol
2  file modified after observation
3  symbol deleted after observation
4  file or repository disappears after indexing
```

— and scores three properties that must not be collapsed:

```text
graph freshness          does the index catch up
staleness signaling      is evidence already handed out marked STALE
action admissibility     what may still be done with that evidence
```

**Freshness does not substitute for signaling.** If an agent obtained a fact at
state A and a watcher rebuilt the graph to state B fifty milliseconds later, the
fact the agent is still holding must lose admissibility. A system that reindexes
quickly but never says "what you were told is no longer true" is not fresh; it
is racing. This maps directly onto the `STALE` status in
`docs/decision-and-admission-protocol.md`, which already refuses to count
`STALE` as success for safety-sensitive guards — the measurement is whether an
external observer supplies the signal that status depends on.

Package 1 is frozen by design once its three known defects are fixed. Seven
cases cover the resolution failure modes worth distinguishing; further variants
would grow a collection rather than the instrument.

## Outcome of the first round (2026-08-07)

```text
resolution qualification   CLOSED    verdict: KEEP NARROW
lifecycle qualification    UNKNOWN   deferred, not passed
```

Measured against gortex `v0.63.1` at corpus identity
`5f68dd6e785b93d573325235cbe3f50ec30ea0a2`. Record, raw sweep and the
materialised admission table:
[`research/gortex-admissibility/results/resolution-v1.1/`](../research/gortex-admissibility/results/resolution-v1.1/).

The finding that decided it was not the phantom edge or the missed caller —
those landed in the tier that announces itself as unreliable, which is where
they belong. What broke is a stronger claim:

```text
lsp_resolved  ≠  complete
```

Provenance describes how an observed edge was resolved. It is not a certificate
that the fact space is closed. An `lsp_resolved` answer carrying no caveat is
indistinguishable, from outside, between "this is everything" and "this is
everything I model" — dynamic dispatch and non-code sources of truth are two
different ways for the second to be true.

`SOLE_OBSERVER_ALLOWED` therefore stays barred for irreversible and admission
decisions. That was written into this document as a precaution before any
measurement; it is now carried by one.

**Lifecycle is UNKNOWN, not passed.** Until a lifecycle round runs, `o7` must
not treat this observer as authority for invalidating evidence it has already
consumed. Exact-head and current-identity checks remain `o7`'s own
responsibility. Package 2 becomes a small targeted qualification round when a
real consumer of staleness notification exists — not before, and not to earn
the word COMPLETE.

## The decision this produces

The harness exists to reach one of four verdicts, not to accumulate findings.

| Measured | Verdict |
|---|---|
| Graph fidelity good, confidence and caveats calibrated, staleness signalled | **KEEP** — supporting code-graph observer |
| Tier 1 / exact-anchor good, lower-tier or concept weak | **KEEP NARROW** — navigation plus expansion from a verified seed |
| Graph sometimes wrong but marks its uncertainty honestly | **KEEP AS HINT** — `NAVIGATION_HINT` only, never evidence |
| Wrong or incomplete output regularly presented as safe, or stale evidence stays admissible | **DROP** from the `o7` decision path |

`DROP` is not a claim the tool is worthless — it may remain a perfectly good
grep-on-steroids. It means `o7` does not build proofs on it.

## Stop rules

The measurement is bounded, and these end it rather than extend it:

- **No new fixtures before the first run.** Corpus v1 is frozen at eight cases.
  A fixture is added when a run surfaces a defect that needs one to
  characterise — on evidence, not in anticipation.
- **No patching the tool under test.** If producing a first result requires
  patching Gortex, writing a substantial compatibility adapter, or growing the
  corpus around each of its quirks, stop and decide on what has already been
  seen. We are evaluating an external observer, not quietly signing on as its
  second maintainer.
- **No universal runner.** The first run reads eight cases. A runner
  generalised over 257 languages is not a prerequisite for it and must not
  become one.

This track is bounded in the schedule too: one short research round (package 2,
freeze) and one measurement round, then a verdict. The A-series does not wait on
it. GCX1 stays recorded and untouched under `PhysShell/qodec` until this
concludes — the failure mode being avoided is the familiar one where evaluating
a library turns into building a benchmark framework, then a wire-format
laboratory, while the original product quietly ages in the corner.

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
