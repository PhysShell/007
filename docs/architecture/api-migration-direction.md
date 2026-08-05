# API migration assurance direction (AMA-0)

Status: **accepted direction, exploration gated at AMA-1** · Track: **AMA** (this
note is AMA-0) · Revision: **2** — supersedes revision 1 (`3ec1343`), which
scoped the track as coverage assurance for API migrations generally. That scope
contained a false product: locating type-*visible* breaks is packaging `tsc` and
invoicing for it. §1 records the correction.

Scope: **subject definition, evaluation protocol, and pre-registered decision
thresholds — not an implementation contract.**

Implementation gate: **nothing beyond the corpus, the oracles, and the AMA-1
qualification experiment** (§10). No analyzer product, no delta extraction from
release notes, no PR authoring, no service. PR generation is a *forbidden*
downstream artifact until the gate passes — otherwise the patch generator gets
polished while the central question, "did we find the right facts at all",
quietly decomposes in the corner.

Ratification: adjudicated in an interactive session with the maintainer, so per
rule 3's carve-out in `docs/evidence-and-decision-discipline.md` this is
**ratified, not pending**. Like ABR-0, it is a frozen direction record.

## 1. Subject

```text
NOT:  bounded coverage assurance for API migrations
BUT:  bounded assurance for the impact of type-invisible semantic API changes
```

**Artifact says** (TypeScript compiler behaviour, a documented and stable
property of `tsc`; re-verified empirically per configuration, see §3): removing
or renaming a member of a typed declaration surface makes every statically
resolvable use of that member a type error.

**Inference:** for the resolvable region, type-visible breaks are already
enumerated completely and at zero marginal cost by the consumer's own compiler.

**Decision:** the compiler is adopted in two roles — a **free negative
baseline** marking where no product exists, and a **machine oracle for the
locator** (§6). The product surface is what it cannot diagnose.

The four layers that revision 1's appraisal was summing, kept here because the
confusion recurs:

```text
1. execution provability   — built in 007 (§9)
2. coverage provability    — absent; the entire research risk
3. patch generation        — commodity; not a differentiator
4. delivery infrastructure — a separate runtime, not an adapter over 007
```

Possessing layer 1 makes layer 2's eventual output *recordable*. It does not
make it half-built. Likewise, development rigour is not an importable
dependency: there is no `qodec-rigor = "1.0"`, and adding one would not yield
soundness.

## 2. Unit of analysis: the contract fact occurrence

Not the call site. The unit is a changed **contract fact** and its concrete
occurrences:

```text
SDK symbol
 └── semantic fact path
      └── change kind
           └── concrete occurrence in consumer code

stripe.paymentIntents.create
 └── automatic_payment_methods.allow_redirects
      └── changed default semantics
           └── src/billing/intent.ts:88  (field omitted; relies on old default)
```

One call may carry several independently affected facts; a hundred calls to the
same method may touch none. Recall measured over call sites therefore has the
wrong denominator, and would be well-formatted nonsense.

## 3. Classification is empirical, never an a-priori list

Whether a change is type-visible is a property of *(SDK declarations, toolchain,
`tsconfig`, consumer code)*, not of the change's description. A new enum member
is invisible to ordinary use but caught by an exhaustive `switch` with a `never`
guard. A string key is opaque as `string` and fully resolvable as a literal
union. Fixing the classes in advance would be a guess wearing a taxonomy.

Membership is decided by experiment, per configuration:

```text
pinned repository commit + pinned SDK/toolchain + pinned tsconfig
  → mutate the specific contract fact
    → did relevant diagnostics appear?

T-visible    compiler is already a sufficient locator  → negative product result
T-invisible  no adequate diagnostic                    → the product surface
Opaque       even the region of potential impact cannot be bounded
```

T-visible outcomes are **published, not hidden**: "on this class of change no
specialised product is needed" is a finding. Suppressing it would turn the
benchmark into a measurement of how well we reimplemented a free compiler
feature.

## 4. Scope split, and the leak it must not spring

Two separable research problems:

- **A — semantic-delta extraction.** From release notes, docs, spec diff, or SDK
  diff, produce a normalized fact: *what changed, from what, to what.*
- **B — impact localization.** Find the code whose logic depends on that fact.

**AMA covers B only.** The semantic delta enters already normalized and
confirmed. Running both at once merges two failure modes into one number: an
extraction failure becomes indistinguishable from a localization failure, and
the metrics turn to porridge.

**Blind-normalization rule (experimental validity, mandatory).** A normalized
delta written by someone who has seen the consumer repository has already
performed part of the localization — the experiment would then measure a task
that does not exist in production. Therefore: the normalized delta is authored
**blind to the consumer repository**, from provider-side artifacts only, and the
case provenance records who authored it, from which artifacts, and that the
blinding held. A case whose blinding cannot be attested is excluded, not
discounted.

## 5. Corpus: three parts with three different jobs

**5.1 Type-visible control corpus.** Mutations for which `tsc` yields ground
truth. Job: qualify the locator, trace the boundary of the declared universe,
establish where no product exists, and detect coverage regression. Machine-
checked; no adjudication.

**5.2 Semantic-impact corpus.** Real or synthetic changes producing no type
error: changed default; re-meaninged field; retry/error semantics; new enum
cases without exhaustiveness; deprecated-but-valid paths; API-version-dependent
behaviour; string-keyed operation/expand/filter names; pagination semantics;
`required → optional` in response semantics. Adjudicated at **fact-occurrence**
granularity — not file, not method.

A historical migration commit is **not** ground truth by itself: the developer
may have missed a site, fixed it later, deleted the feature, refactored
concurrently, migrated a wrapper instead of the call sites, or covered paths no
test exercises. Adjudication combines historical evidence, before/after static
analysis, release notes, manual review, and where available test or trace
validation. Building an assurance product on an unadjudicated benchmark would
make the benchmark a false-confidence generator — an unusually elegant way to
fail.

**5.3 Opaque stress corpus.** Reflective dispatch; `client[resource][operation]`;
DI through opaque registration; generic wrappers; runtime code generation; field
paths built from data; configuration-selected operations. This corpus does not
measure finding things. It measures whether the system refrains from claiming a
completed analysis.

Per-case record: SDK identity and versions · normalized delta + blinding
provenance · empirical class per §3 · construct family per §7.1 · repository
commit · toolchain and `tsconfig` · adjudicated fact occurrences · expected
opaque regions · negative examples · adjudication provenance · known limits.

## 6. Oracles, and their stated limits

**Mutation oracle (lab).** At a pinned commit and configuration, mutate the SDK
declaration surface — delete a field, rename a method, narrow a literal union,
flip optionality, change a generic constraint — and take the resulting `tsc`
diagnostics as reproducible ground truth for that configuration. Measures
resolution recall, symbol-resolution precision, analyzability lost after a PR,
wrapper and dynamic-dispatch effects, and covered-surface change — without human
beings reading commits until they start seeing ASTs in their sleep.

> **Limit, binding:** the mutation oracle qualifies the ability to *locate
> occurrences of a known contract fact*. It proves nothing about the ability to
> *detect the semantic change itself*, which is problem A and out of scope.

**Field self-calibration (the lab→field bridge).** `false assurance rate` is
observable in the corpus and **not observable in production**, where a miss
surfaces only through an incident: a biased, delayed, lossy channel. A
pre-registered threshold therefore governs a lab quantity while the customer
buys a field one. The mutation oracle closes part of that gap, because it can be
run **on the customer's own repository at analysis time**: inject synthetic
contract-fact mutations, measure the locator against the customer's actual code
and configuration, and report that measurement instead of inheriting a benchmark
number.

```text
this repository, this tsconfig, this SDK version:
  120 synthetic contract-fact mutations injected
  118 located
    2 missed — both in construct family "generic wrapper", declared opaque
```

This is the strongest artifact the product can ship, and it is machine-produced.
Whatever it cannot cover (problem-A detection, genuinely type-invisible
semantics) stays an explicitly inherited lab claim with a stated similarity
argument — never an implied one.

## 7. Metrics and the statistical form of the thresholds

### 7.1 Claims are stratified by construct family

The rule of three (`3/n` as the one-sided 95% upper bound at zero observed
failures) requires **independent** trials. Corpus cases are not independent:
failures cluster by construct — every DI case fails together, every generic
wrapper fails together. The effective sample size is nearer the number of
distinct construct families than the number of cases; 300 cases of which 240 are
direct typed calls bound roughly like 60.

Therefore the claim shape changes, and so does the report:

```text
direct typed call        n=40, 0 misses  → upper bound  7.5%
thin local wrapper       n=18, 1 miss    → bound per exact method
DI-registered client     declared opaque → no claim made
```

No repository-wide single number. The universe is declared as a **set of
construct families**, each carrying its own evidence and its own bound. This
also makes the corpus tractable — n per family, not n across everything — and it
tells a buyer whether *their* pattern is covered rather than the benchmark's
average temperature.

### 7.2 Metrics

```text
fact-level recall        ground-truth fact occurrences found, per family
false assurance rate     runs claiming coverage while an occurrence inside the
                         declared universe was missed        (governing metric)
assurance coverage       share of the real surface on which a strong claim was made
opaque recall            known-opaque regions correctly declared opaque
false-opaque rate        regions declared opaque that the mutation oracle resolves
                         in the same configuration           (machine-measurable)
risk–coverage curve      assurance coverage as a function of accepted risk
analysis cost            per report, at realistic repository size, and reproducibility
```

`false-opaque rate` is deliberately defined against the oracle rather than
against an adjudicator: it catches analyzer cowardice automatically, and leaves
adjudication needed only for the opposite direction.

`false assurance rate` governs — a spurious warning annoys, a false assurance
report causes an incident. `assurance coverage` is its mandatory pair: a perfect
false-assurance rate is trivially bought by declaring everything opaque, which is
flawless and worthless.

### 7.3 Thresholds are bounds, not point estimates

A raw percentage without confidence bounds deceives cheaply: zero misses in 20
cases is not a 0% miss rate — its one-sided 95% upper bound is about 15%.
Pre-commitment therefore takes this form, per construct family:

```text
one-sided 95% upper bound on false-assurance rate   ≤ X_family
one-sided 95% lower bound on assurance coverage     ≥ Y_family
minimum n per family before any claim is published  ≥ N_family
```

And the differentiation threshold against the agent baseline is likewise paired:

```text
at an identical accepted false-assurance ceiling, the specialised system must
deliver a pre-registered minimum gain in fact-level recall or assurance coverage
```

The numeric values of `X`, `Y`, `N`, and the required gain are fixed **when the
AMA-1 protocol is frozen, before any analyzer result is seen**. A threshold
chosen after seeing the score is not a threshold; it is retrospective
self-approval with a table.

## 8. Baseline protocol (pre-registered)

Baselines: the specialised locator · a strongest-form coding-agent baseline ·
grep/AST · compiler-only (§1).

The agent baseline is held to the **same output contract** as the product —
findings, coverage claim, evidence, known unknowns, potentially opaque regions,
confidence and abstentions. Comparing a system required to publish its blind
spots against an agent merely told "find everything" is a staged fight in which
the opponent's shoelaces were tied beforehand.

Frozen before the run: model and version · prompt · available tools · whether
compiler/grep/AST invocations are permitted · token budget · number of attempts
· result-aggregation rules. Otherwise the baseline retroactively becomes an idiot
or a genius depending on which conclusion the authors needed.

The decisive comparison:

> At an identical accepted false-assurance rate, which system covers more of the
> real surface?

If a well-prompted agent matches on both fact-level recall and calibrated
abstention, there is no specialised company here. There is a good prompt pack and
possibly a workflow around it — a finding worth reaching in one experiment rather
than in one funding round.

## 9. Asset accounting, and 007's narrowed role

| Asset | Real status | Role in AMA |
|---|---|---|
| Worktree isolation | Built | Reproducible pinned-input runs |
| Gate reduction, fail-closed verdicts | Built | Evaluation gating |
| Digest-chained event ledger | Built | Evidence for each measurement |
| `o7 replay` | Built | Independent re-verification of a run |
| Live ingress / REST + SSE (`o7d`) | Partly built | Run observation |
| Fact-occurrence locator | **Absent** | The product risk |
| Three-part corpus + oracles | **Absent** | The first required artifact |
| Capability / action broker | Direction only (ABR-0) | Not counted as implementation |
| Agentopedia / documentation drift | Idea | Not counted as an asset |
| Multi-tenant SaaS runtime | Absent | A separate future project |
| Automatic PR generation | Commodity | Forbidden before the gate |

007 is neither the analyzer nor the assurance product. Its available role is an
**evaluation and evidence harness** around a future locator: pinned inputs →
gates → digest-chained evidence → replay → decision record. Even that is
secondary at this stage.

The primary AMA-0 artifact is now:

```text
normalized semantic-delta schema  +  fact-level ground-truth format
+ mutation oracle                 +  three-part corpus
+ baseline protocol               +  pre-committed decision thresholds
```

Not a GitHub App. Not PR generation. Not a SaaS. Not necessarily even a complete
analyzer.

## 10. AMA-1 — Semantic Impact Localization Qualification

The only experiment permitted after this note.

**Inputs:** a manually normalized semantic delta (blinded per §4) · a pinned
TypeScript repository commit · pinned SDK, compiler, and `tsconfig`.

**Outputs:** fact-level inventory · evidence per finding · declared covered
universe as a set of construct families · opaque set · machine-verifiable
mutation-oracle score · adjudicated semantic-corpus score · strongest-agent
baseline · per-family bounds per §7.3 · decision.

**Decision: CONTINUE / NARROW / STOP.**

### Kill criteria

Spent effort is not an argument against these; citing it is precisely the
failure mode they exist to stop.

```text
- the false-assurance upper bound cannot be driven under X on any family that
  covers a commercially meaningful share of real integrations
- adequate completeness holds only on an artificially narrow universe
  (assurance coverage below its pre-registered floor)
- the opaque set swallows the majority of real integrations
- a strongest-form agent baseline matches both recall and calibrated abstention
- adjudication + analysis cost exceeds the value of the misses prevented
- not enough blind-normalizable, high-quality cases are obtainable to reach
  N_family on more than one family
```

## 11. Deferred, and non-goals

```text
semantic-delta extraction (problem A)   multi-language support
GitHub App                              raw fetch / dynamic URL analysis
multi-tenant SaaS runtime               wrapper and DI-indirection analysis
automatic PR authoring                  compliance productisation
provider→customer distribution          PR-time coverage-regression signal
capability-broker integration
```

Two carry recorded reasoning so deferral is not mistaken for absence of thought:

- **PR-time signal.** A blast-radius bot firing once a quarter has no retention.
  The durable form is a *coverage regression* signal: a change converted resolved
  occurrences into opaque ones — dynamic dispatch introduced, a covered wrapper
  bypassed, the SDK version moved without an assurance run. That measures the
  degradation of a codebase's own analyzability, which is a stronger idea than
  API linting, and it is meaningless before the locator has numbers.
- **Compliance.** Not a vertical; the *packaging* of the assurance report for
  change management — initiator, analyzed universe, versions compared, findings,
  changes made, checks run, what stayed opaque, who accepted the residual risk,
  and the evidence digest bound to that acceptance. 007's ledger and `o7 replay`
  already produce the substrate.

Non-goals: no claim of completeness over arbitrary code; no "provably
unaffected" verdict (the phrase is retired from this track); no universal
API-change platform; no product runtime built on subscription-auth CLIs.

## 12. Liability boundary (recorded, not solved)

A provider-authored bot opening PRs in customer repositories creates a
responsibility chain — recommendation, authored code, approved merge, production
failure — that an evidence pack can document but cannot allocate. Internal
platform teams are the right first buyer for three reasons that are not about
distribution: one owner for both the API and its consumers, access to every
repository and CI, and liability contained inside one organisation. They also
make adjudication tractable, since they can confirm which services should have
migrated.

## 13. Unfreeze triggers

```text
locator implementation       after AMA-1 returns CONTINUE on pre-registered bounds
wrapper / DI analysis        after the corpus shows resolved-only coverage is too small
delta extraction (problem A) after localization qualifies on a fixed normalized input
PR-time regression signal    after the locator has published per-family numbers
delivery runtime             after a paying internal-platform design partner exists
provider-side distribution   after §12 is contractually settled
```

Nothing is built now beyond the corpus, the oracles, and AMA-1. This note exists
so the narrow wedge does not mutate, three commits later, into a SaaS, a GitHub
App, a cloud runtime, and an insurance policy.
