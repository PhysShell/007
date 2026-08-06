# API migration assurance direction (AMA-0)

Status: **accepted direction, exploration gated at the AMA-0.5 protocol-formation
packet, under a hard 48-maintainer-hour cap** · Track: **AMA** (this note is
AMA-0) · Revision: **4**.

Revision history, kept because the errors are load-bearing:

- **rev 1** (`3ec1343`) — scoped the track as coverage assurance for API
  migrations generally. That scope contained a false product: locating
  type-*visible* breaks is packaging `tsc` and invoicing for it.
- **rev 2** (`288e5e7`) — narrowed the subject to type-invisible semantic
  change; introduced the mutation oracle, blind normalization, construct
  families, and confidence-bound thresholds. Two defects survived: §7.2
  over-claimed what the mutation oracle can witness, and a single-axis
  construct family with an undefined trial unit could not support the bounds
  it was being asked to carry.
- **rev 3** (`5c275d1`) — factorized claim cells, oracle fidelity, delta
  provenance, the independent-cluster definition, the `N_qualify` / `N_publish`
  split, adjudicator-independence limits, and the AMA-0.5 packet. One defect
  survived: the protocol had learned to kill the product but had granted itself
  diplomatic immunity — the only cost measurement sat after the codebook freeze,
  and the packet carried no resource ceiling of its own.
- **rev 4** (`d557578`) — the pre-freeze cost-floor probe, the numeric caps, the
  banned `EXTEND` outcome, and external-only reopening after `STOP`.
- **rev 4 erratum** (this) — execution fix, not a methodological revision: two
  branches of the early stopping rule were unreachable because a three-hour
  censor caps `B_floor` at 174h, and the aggregator over the four timings was
  never named. §10.3 now decides on censored-case count, the 200/400h thresholds
  move to the post-freeze gate in §8.4, and exclusion handling in §10.2 stops the
  denominator from shrinking silently. Taxonomy, corpus model, claim cells, and
  scope are untouched.

Scope: **subject definition, evaluation protocol, and pre-registered decision
thresholds — not an implementation contract.**

Implementation gate: **the AMA-0.5 protocol-formation packet only** (§10). No
locator, no analyzer product, no delta extraction from release notes, no PR
authoring, no service. PR generation is a *forbidden* downstream artifact until
AMA-1 passes — otherwise the patch generator gets polished while the central
question, "did we find the right facts at all", quietly decomposes in the corner.

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
baseline** marking where no product exists, and a **fidelity-bounded oracle for
the locator** (§6). The product surface is what it cannot diagnose.

The four layers that rev 1's appraisal was summing, kept because the confusion
recurs:

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
provenance · `delta_provenance` (§6.3) · empirical class per §3 · claim cell per
§7.1 · cluster id per §7.2 · repository commit · toolchain and `tsconfig` ·
adjudicated fact occurrences · expected opaque regions · negative examples ·
adjudication provenance and adjudicator independence status (§8.3) · known
limits.

## 6. Oracles, their fidelity, and delta provenance

### 6.1 The mutation oracle

At a pinned commit and configuration, mutate the SDK declaration surface —
delete a field, rename a method, narrow a literal union, flip optionality,
change a generic constraint — and take the resulting `tsc` diagnostics as
reproducible ground truth for that configuration. Measures resolution recall,
symbol-resolution precision, analyzability lost after a PR, wrapper and
dynamic-dispatch effects, and covered-surface change — without human beings
reading commits until they start seeing ASTs in their sleep.

> **Limit, binding:** the mutation oracle qualifies the ability to *locate
> occurrences of a known contract fact*. It proves nothing about the ability to
> *detect the semantic change itself*, which is problem A and out of scope.

### 6.2 Oracle fidelity

A mutation is a valid witness only when it is an adequate surrogate for the same
contract-fact occurrence. Direct field access surveys cleanly; a changed default
survives a conservative surrogate (temporarily make the optional field required
— `tsc` then shows every site that omitted it); retry semantics do not survive
at all — the compiler locating a call is not an oracle for *dependence on error
behaviour*. The same holds for pagination termination logic, ordering and
idempotency assumptions, downstream interpretation of a response, dependence on
a pinned API version, and a field whose meaning changed without its type moving.

```text
oracle_fidelity:
  exact                mutation witnesses the same occurrence set
  conservative_proxy   mutation witnesses a superset or a related set
  unavailable          no mutation surrogate exists; adjudication only
```

Fidelity is a property of the pair *(semantic-fact kind, mutation strategy)* —
**not** of a case, and not of an adjudicator's discretion at labelling time. One
case routinely carries facts of differing fidelity. It is therefore determined in
advance and recorded in the frozen codebook (§8), like every other threshold.

Consequences, binding:

- **`exact`** may enter the machine-computed `false-opaque rate` and the strong
  form of field self-calibration.
- **`conservative_proxy`** supports a diagnostic report, never a guarantee.
- **`unavailable`** stays with adjudication.

Without this ladder, §7.3 performs exactly the substitution the whole document
was built to prevent: "the compiler found something" quietly becomes "the
compiler proved the semantic fact".

### 6.3 Delta provenance

`N_publish` (§7.4) is bounded not only by adjudication budget but by the
**supply of semantic deltas**: independent clusters are limited by *available
repositories × deltas of that kind that actually happened*. For rare cells —
pagination termination, ordering, idempotency — the historical supply is
single-digit. No provider has shipped fifty-nine pagination-semantics changes.

Synthetic deltas relieve the shortage and break something else: authored by us,
they measure the analyzer against *our model of what changes look like*. That is
the same hazard as oracle fidelity on a different axis, and it takes the same
ladder:

```text
delta_provenance:
  historical   a change that actually shipped        → may support an assurance claim
  synthetic    constructed by us                     → diagnostic only, never assurance
  hybrid       historical delta transplanted onto a different repository
                                                     → recorded separately; claim strength
                                                       decided at freeze, not afterwards
```

Cells are reported with their provenance mix. Otherwise a cell built from six
synthetic deltas prints identically to one built from six historical ones, which
is precisely how a benchmark becomes a mirror.

## 7. Claim cells, independence, and the statistical form of the thresholds

### 7.1 The claim cell is factorized on two axes

Misses cluster by *how the client is obtained* and by *what kind of semantic
dependence the code carries*. A single direct typed call may carry an explicit
argument value, a dependence on an omitted field's default, a string key, a
downstream response-field check, retry control flow, and a pagination
termination condition. A locator can be flawless on the first three and useless
on the last three; averaged into one "direct typed call" family, the number is
again brilliant and meaningless.

Candidate axes — **candidate**, to be replaced by the empirically discovered
codebook of §8, not to be treated as the taxonomy:

| Resolution construct | Semantic dependence |
|---|---|
| Direct symbol access | Explicit argument / value |
| Local alias | Omitted field / default |
| Thin wrapper | String key / path |
| Interface or DI | Response-field interpretation |
| Factory or registry | Error / retry control flow |
| Generic abstraction | Pagination / iteration |
| Dynamic / config-driven | Ordering / idempotency / version semantics |

The published unit of claim is the cell:

```text
thin local wrapper  ×  omitted-field/default dependence  ×  exact mutation oracle
```

Cells may be merged into a coarser family **only** under a rule fixed before
measurement: same failure mechanism, same evidence procedure, and empirically
compatible results. Never because merging is the cheapest way to reach `N`.

### 7.2 What counts as one independent trial

Stratification alone does not buy independence. Within one cell the following
are correlated: many occurrences of one delta in one repository; several
repositories sharing an internal wrapper library; many migrations of one SDK;
several versions of one consumer; constructs emitted by one code generator.

**Cluster unit:** `consumer repository × provider semantic delta × claim cell`.

Inside a cluster, individual fact occurrences contribute to fact-level recall,
but the whole cluster yields **one** outcome for the false-assurance bound:

```text
27 occurrences in one repo/delta/cell
26 found, 1 missed

fact recall:      26/27
cluster outcome:  false assurance = 1
effective n:      1, not 27
```

**Shared-ancestor collapsing (required).** The cluster unit above does not by
itself handle the very correlation that motivated it: five repositories using
the same wrapper library would count as five clusters. Clusters sharing a common
code ancestor — the same wrapper package, the same generator, the same
documentation template, or AST similarity above a pre-registered threshold —
collapse to **one** outcome. Without this, the cheapest way to reach `N` is to
count forks of the same code, which is also the most worthless.

Otherwise the rule of three receives hundreds of pseudo-independent observations
and returns a laboratory hallucination, this time wearing a confidence interval.

### 7.3 Metrics

```text
fact-level recall        ground-truth fact occurrences found, per cell
false assurance rate     runs claiming coverage while an occurrence inside the
                         declared universe was missed        (governing metric)
assurance coverage       share of the real surface on which a strong claim was made
opaque recall            known-opaque regions correctly declared opaque
false-opaque rate        regions declared opaque that an `exact`-fidelity mutation
                         resolves in the same configuration  (machine-measurable)
risk–coverage curve      assurance coverage as a function of accepted risk
analysis cost            per report, at realistic repository size, and reproducibility
```

`false-opaque rate` is defined against the oracle rather than an adjudicator, so
analyzer cowardice is caught automatically — but only at `exact` fidelity (§6.2);
a `conservative_proxy` witness cannot convict a region of being falsely opaque.

`false assurance rate` governs — a spurious warning annoys, a false assurance
report causes an incident. `assurance coverage` is its mandatory pair: a perfect
false-assurance rate is trivially bought by declaring everything opaque, which is
flawless and worthless.

**Field self-calibration (the lab→field bridge).** `false assurance rate` is
observable in the corpus and **not observable in production**, where a miss
surfaces only through an incident: a biased, delayed, lossy channel. The mutation
oracle closes part of that gap, because it runs **on the customer's own
repository at analysis time** — inject synthetic contract-fact mutations of
`exact` fidelity, measure the locator against the customer's actual code and
configuration, and report that measurement instead of inheriting a benchmark
number.

```text
this repository, this tsconfig, this SDK version:
  120 exact-fidelity contract-fact mutations injected
  118 located
    2 missed — both in cell "generic abstraction × response-field", declared opaque
```

This is the strongest artifact the product can ship, and it is machine-produced.
Everything outside `exact` fidelity remains an explicitly inherited lab claim
with a stated similarity argument — never an implied one.

### 7.4 Thresholds are bounds, and there are two of them

A raw percentage without confidence bounds deceives cheaply: zero misses in 20
clusters is not a 0% miss rate — its one-sided 95% upper bound is about 15%. At
zero observed failures, the exact binomial requirement `n ≥ ln(0.05)/ln(1−X)` is:

```text
X = 10%  →  n = 29
X =  5%  →  n = 59
X =  2%  →  n = 149
X =  1%  →  n = 299
```

Per claim cell, in independent clusters after §7.2 collapsing. Not per corpus.

**Two different `N`, because one number is being asked to serve two standards of
proof.** Deciding whether research should continue and publishing an external
assurance claim are not the same evidentiary bar:

```text
N_qualify   minimum clusters to justify CONTINUE / NARROW / STOP
N_publish   minimum clusters before an external claim is made for that cell
```

Collapsing them would require AMA-1 to assemble a nearly commercial corpus
before earning the right to decide whether assembling a corpus is worthwhile — a
gate that demands a finished factory as a permit to build an experimental shed.

Pre-commitment therefore takes this form, per cell:

```text
one-sided 95% upper bound on false-assurance rate   ≤ X_cell
one-sided 95% lower bound on assurance coverage     ≥ Y_cell
minimum independent clusters                        ≥ N_qualify_cell / N_publish_cell
```

And the differentiation threshold against the agent baseline is stated on the
*difference*, with its own uncertainty — not on two point estimates:

```text
at an accepted false-assurance upper bound ≤ X_cell:

  lower confidence bound( assurance_coverage_specialised
                        − assurance_coverage_agent )  ≥ Δ_cell
```

`X`, `Y`, both `N`s, and `Δ` are fixed **when the AMA-1 protocol is frozen,
before any locator result is seen**. A threshold chosen after seeing the score is
not a threshold; it is retrospective self-approval with a table.

## 8. Taxonomy discovery, adjudication, and their independence limits

### 8.1 Discovery protocol

The codebook is built empirically on a **discovery cohort that never enters the
qualification corpus**:

```text
1. fix the sampling frame before looking at code — language, SDK, repository
   sizes, inclusion and exclusion criteria
2. the blind normalizer works from provider-side artifacts only (§4)
3. two coders independently label consumer code by observed resolution
   construct and semantic dependence
4. disagreements produce a codebook with machine- or procedure-checkable
   membership rules
5. freeze the codebook
6. validate classification reproducibility on a separate holdout cohort
7. an unforeseen pattern is `unclassified` / opaque — never retrofitted into a
   convenient cell
```

The locator is excluded from this process entirely — neither its outputs nor its
internal capabilities. Otherwise the result is not a taxonomy of real code but an
inventory of what the current implementation happens to digest.

### 8.2 Frozen artifacts required before AMA-1

```text
sampling-frame hash          claim-cell construction rule
discovery-cohort manifest    cluster independence + collapsing rule
codebook revision            holdout validation result
oracle-fidelity table        unclassified-pattern policy
```

### 8.3 Adjudicator independence (recorded limit, not a solved problem)

The protocol above assumes several human coders. The actual team is one
maintainer plus agents, and the tempting substitution — an agent as the second
independent coder — destroys independence quietly and irreversibly: that coder
correlates with the LLM baseline under evaluation and with the locator itself if
it carries LLM components. Agreement between two instances of one model measures
that model's consistency, not truth, while presenting as inter-rater agreement.

```text
agent as first-pass candidate enumerator   permitted (cheap, recall-oriented)
agent as independent second adjudicator    prohibited while the locator or the
                                           baseline shares its model family
```

Where a second human is unavailable, this is **recorded as a validity limit**,
not dressed as double-blind adjudication.

Ordering matters too: a time-and-motion pilot run before the codebook freeze
conflates codebook immaturity with intrinsic case difficulty. Disagreement rate
before and after the freeze are two different quantities and are reported
separately.

### 8.4 Adjudication cost is decomposed, not averaged

A "case" has wildly variable mass: three direct occurrences in one, a monorepo
with a wrapper layer, a historical refactor, and a CI that died during a previous
administration in another.

```text
T_case = T_provider_artifact_review
       + T_blind_normalization
       + T_repository_reproduction
       + T_primary_adjudication
       + T_independent_secondary_pass
       + T_disagreement_resolution
       + T_evidence_packaging
```

Recorded per case: candidate occurrences · confirmed occurrences · claim cell ·
cluster id · repository size · files touched · toolchain reproducibility ·
presence of a historical migration commit · presence of tests or traces ·
disagreement rate.

Published as distribution, not mean — one light Stripe example would otherwise
turn the mean into a sales flyer:

```text
p50 and p90 adjudication time per claim cell
disagreement rate (pre- and post-freeze, separately)
reproduction-failure rate
cost per accepted independent cluster
```

Budget projection:

```text
Total = Σ over claim cells ( N_cell × p90 accepted-cluster cost )
      + failed-reproduction cost
      + taxonomy and normalization overhead
```

Not `number of families × mean time of a pretty case`. Six commercially
meaningful cells at a modest four hours per independent cluster, for a 5% ceiling
at zero observed failures:

```text
6 × 59 × 4 = 1,416 hours
```

— for `N_publish` alone, before failed setups, disagreements, and infrastructure.
This is the most likely place for the project to take a shovel to the head, and
learning it before the locator exists is the good outcome.

**Post-freeze economic gate (the only place the 200/400h thresholds mean
anything).** The projection above uses `p90` over a cost distribution that is not
truncated from above, so it is the gate that can legitimately return a large
number. Evaluated after step 8 of §10.5, on the pre-locator corpus:

```text
projected pre-locator corpus cost > 400h   → STOP
                            200h .. 400h   → NARROW to the cheapest cells with
                                             historical delta supply
                                  ≤ 200h   → FREEZE admissible on this criterion
```

The aggregator is fixed here and now — `p90` of accepted-cluster cost per claim
cell, as written above — and not chosen once the numbers are visible. The early
probe of §10.2 cannot feed this gate: its costs are right-censored at three
hours, so no aggregate over them can exceed 174h by construction.

## 9. Asset accounting, and 007's narrowed role

| Asset | Real status | Role in AMA |
|---|---|---|
| Worktree isolation | Built | Reproducible pinned-input runs |
| Gate reduction, fail-closed verdicts | Built | Evaluation gating |
| Digest-chained event ledger | Built | Evidence for each measurement |
| `o7 replay` | Built | Independent re-verification of a run |
| Live ingress / REST + SSE (`o7d`) | Partly built | Run observation |
| Fact-occurrence locator | **Absent** | The product risk |
| Codebook, corpus, oracles | **Absent** | The first required artifacts |
| Capability / action broker | Direction only (ABR-0) | Not counted as implementation |
| Agentopedia / documentation drift | Idea | Not counted as an asset |
| Multi-tenant SaaS runtime | Absent | A separate future project |
| Automatic PR generation | Commodity | Forbidden before AMA-1 |

007 is neither the analyzer nor the assurance product. Its available role is an
**evaluation and evidence harness** around a future locator: pinned inputs →
gates → digest-chained evidence → replay → decision record. Even that is
secondary at this stage.

## 10. AMA-0.5 — the protocol-formation packet (the real first experiment)

The only work permitted after this note. Not a locator.

This section applies the note's own kill discipline to the note. A protocol that
can only kill the product, never itself, becomes an indefinite refinement loop in
which every iteration is honest and justified — a very rigorous way never to
start.

### 10.1 Two cost measurements, deliberately different

The post-freeze time-and-motion pilot is **not** moved earlier: disagreement rate
measured before the codebook freeze conflates codebook immaturity with intrinsic
case difficulty (§8.3). A separate, cruder probe runs *first*, because cost and
agreement are different quantities and the second need not wait for the first.

| Probe | Measures | May **not** claim |
|---|---|---|
| Pre-freeze cost-floor probe | a lower bound on real per-case cost | p90, disagreement rate, codebook maturity |
| Post-freeze time-and-motion | the full adjudication cost distribution and disagreement | that building the taxonomy was worthwhile |

The early probe is a **one-sided kill instrument**:

> High cost may kill the direction. Low cost may not qualify it.

Otherwise four convenient cases become a startup's financial model — accounting
by the "these four Stripe examples were quick" method, which is already
over-represented in the world.

### 10.2 Pre-freeze cost-floor probe

```text
cases:                    4
total maintainer budget:  12 hours
per-case censor:          3 hours
```

The four cases are chosen **before** work starts and must differ deliberately in
expected cost driver. These are **not** claim cells — no codebook exists yet —
they are cost-driver strata used only to force heterogeneity:

```text
1. provider artifacts clear + repository reproducible
2. wrapper / alias indirection
3. semantic control-flow dependence
4. historically messy or reproduction-hostile
```

A **reserve case per stratum** is designated at the same time as the four, before
any work starts — otherwise a replacement becomes a second case selection made
with knowledge of the first one's outcome.

Measured: `T_provider_artifact_review`, `T_blind_normalization`,
`T_repository_reproduction`, `T_primary_adjudication`, `T_evidence_packaging`.

Excluded: `T_independent_secondary_pass`, `T_disagreement_resolution`. Where no
second human exists, the result is declared a **lower bound** on the full
`T_case`, not an approximation of it — which is more useful anyway: if the
project dies at the lower bound, the rest of the reasoning is unnecessary.

At three hours each case takes exactly one outcome — `accepted`, `excluded with
reason`, or `right-censored at >3h`. Not "one more hour, it's nearly done". That
is how methodology packets acquire permanent residency.

**Exclusions do not silently shrink the denominator.** The decision rule of
§10.3 counts censored cases out of four, so an exclusion must be classified:

```text
excluded for cost-adjacent reasons     counts as CENSORED
  (repository will not reproduce, toolchain unobtainable, provider artifacts
   exist but are unusable without disproportionate effort)
  — the exclusion is itself cost evidence

excluded for validity reasons          replaced from the stratum's reserve
  (blinding cannot be attested, no provider-side artifact exists at all)
  — a sampling defect, not a cost signal; if no reserve can be run inside the
    12-hour budget, the probe is incomplete and outcomes are restricted to
    NARROW or STOP
```

### 10.3 The early stopping rule

**Erratum against the first form of this section** (rev 4, `d557578`; history not
rewritten): it projected `B_floor = 2 × 29 × cost floor` against 200h/400h
branches while censoring every case at three hours. Any completed case therefore
satisfies `cost ≤ 3h`, so `B_floor ≤ 174h` **by construction** and both branches
were unreachable; a single censored case left the true value somewhere in
`(174, ∞)` with no way to place it. The aggregator over four timings was also
never named, and choosing one after seeing the numbers would have been the exact
retrospective self-approval this note was written against. The mechanical
consequence: the early probe cannot evaluate corpus economics at all, and the
200h/400h thresholds belong to the post-freeze gate in §8.4, where the cost
distribution is not truncated from above.

What twelve hours can honestly establish is only whether deliberately
heterogeneous cases fit under a three-hour floor. So the rule counts outcomes,
not money:

```text
0 censored cases
  → AMA-0.5 may continue
  → no monetary projection is recorded at all  (see the note below)

1 censored case
  → NARROW
  → the narrowing must exclude, or separately isolate, that cost-driver stratum
  → if the stratum is commercially mandatory: STOP

≥2 censored cases
  → STOP
```

The timings are recorded as inputs to the post-freeze gate. They are **not**
aggregated into any early decision — there is no aggregator, because the decision
no longer depends on one.

> **Withdrawn by execution, not by revision.** An earlier form of this section
> had the 0-censored branch record `B_floor ≤ 174h`. That figure was derived from
> four cases under a soft two-cell hypothesis, not from a universal constant, and
> it does not survive the probe's narrowing to three strata
> (`ama/ama-0.5/scope-decision-01.json`, taken under this section's own `NARROW`
> outcome after delta D2 yielded zero admissible cases in a frozen
> five-repository supply frame). The early probe publishes no monetary
> projection; economics stay a post-freeze question decided on `p90` over an
> untruncated distribution (§8.4).

Two properties of this design are deliberate and must not be "fixed" by a later
reader:

- **The probe is one-sided by construction.** It can only observe that a stratum
  refuses to fit in three hours. Four cheap cases establish nothing about
  publishable-corpus cost, and the `0 censored` branch says so in its own text.
- **A `STOP` here is a verdict on the product, not on the research question.** A
  cheap one-off comparison ("does specialised localization beat a strong agent at
  all?") remains meaningful afterwards, but as separate one-time work, never as
  AMA continuing under another name.

### 10.4 Package cap

```text
AMA-0.5 total budget: 48 maintainer-hours
  12h  pre-freeze cost-floor probe
  36h  everything else

per-case ceiling inside AMA-0.5: 6 maintainer-hours
  → then: accepted / excluded / unresolved-cost failure
```

Agent runtime, compiler runtime, and CI waiting are recorded separately and do
**not** substitute for maintainer-hours. Active reading, framing, checking,
correcting, labelling, and deciding all count. Otherwise human cost is declared
zero while an agent spends three weeks producing material that someone then
spends three weeks verifying.

The per-case ceiling does not assert that a case objectively needs no more than
six hours. It asserts that no single case may eat the feasibility packet.

### 10.5 Ordered packet

```text
0. pre-freeze cost-floor probe — 4 heterogeneous cases, 12h, 3h censor
                                 (STOP / NARROW trigger only)
1. empirical taxonomy discovery (§8.1) on the discovery cohort
2. factorized claim-cell schema (§7.1)
3. oracle-fidelity table, per (fact kind, mutation strategy) (§6.2)
4. delta-provenance classification and per-cell supply survey (§6.3)
5. independent-cluster and shared-ancestor collapsing rules (§7.2)
6. freeze the codebook
7. holdout classification validation (§8.1)
8. post-freeze adjudication time-and-motion pilot (§8.3, §8.4)
9. N_qualify / N_publish and total budget projection (§7.4, §8.4)
10. freeze X, Y, N, Δ
11. decision under the 48-hour cap
```

### 10.6 Outcomes — and the one that does not exist

```text
FREEZE AMA-1  /  NARROW  /  STOP
```

**`EXTEND AMA-0.5` is prohibited.** Otherwise the timebox is decorative ribbon
around an unbounded protocol.

**FREEZE** requires every mandatory artifact of §8.2 complete within the cap —
sampling frame, discovery manifest, frozen codebook, claim-cell construction
rules, oracle-fidelity table, delta-provenance supply survey, cluster and
collapsing rules, holdout result, post-freeze cost pilot, `N_qualify` /
`N_publish`, `X` / `Y` / `Δ`, budget projection — and **no open question capable
of changing** claim-cell membership, oracle fidelity, cluster identity, or
threshold interpretation.

**NARROW** applies when the full scope cannot be frozen but a smaller protocol
can be formed **from data already collected**, without a further discovery round:

```text
was:  7 resolution constructs × 7 semantic dependences
now:  direct access / local alias  ×  explicit value / omitted default / string key
```

**STOP** applies when the mandatory packet is incomplete; no admissible narrowing
follows from the data already held; the cost probe or the supply survey kills the
economics; the taxonomy remains unstable; or adjudication validity cannot be
supported even in a reduced scope.

"Insufficient data" at budget exhaustion means `STOP` or `NARROW` — never "a
little more data". That sentence is the whole self-application.

### 10.7 Reopening after STOP

After `STOP`, AMA-0.5 does not reopen because a better methodological idea
arrived. Those can be produced indefinitely, and models are trained to excel at
precisely that.

```text
admissible:
  - a design partner with access to real consumer repositories
  - an independent second human adjudicator becomes available
  - a substantial new source of historical deltas is found
  - reproduction/adjudication cost drops materially due to an existing tool
  - an external corpus already contains the required independent clusters

inadmissible:
  - "we thought of a more accurate classification"
```

The latter is simply revisions 5, 6, and 19, each impeccably justifying the next.

These figures — 4 cases, 12h, 3h censor, 48h cap, 6h per case, and the 400h
post-freeze pre-locator ceiling of §8.4 — are not scientifically ideal constants. They are management boundaries,
and their purpose is not to establish a law of nature but to stop an inquiry into
provability from quietly becoming a multi-year inquiry into itself.

## 11. AMA-1 — Semantic Impact Localization Qualification

Runs only if AMA-0.5 returns FREEZE.

**Inputs:** a manually normalized semantic delta (blinded per §4) · a pinned
TypeScript repository commit · pinned SDK, compiler, and `tsconfig` · the frozen
codebook and thresholds.

**Outputs:** fact-level inventory · evidence per finding · declared covered
universe as a set of claim cells · opaque set · mutation-oracle score with
fidelity annotations · adjudicated semantic-corpus score · strongest-agent
baseline · per-cell bounds per §7.4 · decision.

**Decision: CONTINUE / NARROW / STOP.**

### Baseline protocol (pre-registered)

Baselines: the specialised locator · a strongest-form coding-agent baseline ·
grep/AST · compiler-only (§1).

The agent baseline is held to the **same output contract** as the product —
findings, coverage claim, evidence, known unknowns, potentially opaque regions,
confidence and abstentions — and receives the same normalized delta, the same
claim-cell codebook, the same repository state, and the right to invoke `tsc`,
grep, and AST tooling. It is denied sight of the qualification labels. Comparing
a system required to publish its blind spots against an agent merely told "find
everything" is a staged fight in which the opponent's shoelaces were tied
beforehand.

Frozen before the run: model and version · prompt · available tools · token
budget · number of attempts · result-aggregation rules. Otherwise the baseline
retroactively becomes an idiot or a genius depending on which conclusion the
authors needed.

The decisive comparison, per §7.4: at an identical accepted false-assurance
ceiling, does the specialised system's coverage advantage survive its own
confidence interval? If a well-prompted agent matches on both fact-level recall
and calibrated abstention, there is no specialised company here — there is a good
prompt pack and possibly a workflow around it, a finding worth reaching in one
experiment rather than in one funding round.

### Kill criteria

Spent effort is not an argument against these; citing it is precisely the failure
mode they exist to stop.

```text
- the false-assurance upper bound cannot be driven under X_cell on any cell
  covering a commercially meaningful share of real integrations
- adequate completeness holds only on an artificially narrow universe
  (assurance coverage below its pre-registered floor)
- the opaque set swallows the majority of real integrations
- a strongest-form agent baseline matches on both recall and calibrated
  abstention, with Δ_cell not surviving its confidence bound
- projected adjudication + analysis cost exceeds the value of the misses prevented
- historical delta supply cannot reach N_publish on more than one cell, and the
  shortfall can only be covered by synthetic deltas (diagnostic, not assurance)
```

## 12. Deferred, and non-goals

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
  API linting, and it is meaningless before the locator has per-cell numbers.
- **Compliance.** Not a vertical; the *packaging* of the assurance report for
  change management — initiator, analyzed universe, versions compared, findings,
  changes made, checks run, what stayed opaque, who accepted the residual risk,
  and the evidence digest bound to that acceptance. 007's ledger and `o7 replay`
  already produce the substrate.

Non-goals: no claim of completeness over arbitrary code; no "provably
unaffected" verdict (the phrase is retired from this track); no universal
API-change platform; no product runtime built on subscription-auth CLIs.

## 13. Liability boundary (recorded, not solved)

A provider-authored bot opening PRs in customer repositories creates a
responsibility chain — recommendation, authored code, approved merge, production
failure — that an evidence pack can document but cannot allocate. Internal
platform teams are the right first buyer for three reasons that are not about
distribution: one owner for both the API and its consumers, access to every
repository and CI, and liability contained inside one organisation. They also
make adjudication tractable, since they can confirm which services should have
migrated.

## 14. Unfreeze triggers

```text
AMA-0.5 after a STOP         only on an external trigger from §10.7
AMA-1 qualification run      after AMA-0.5 returns FREEZE
locator implementation       after AMA-1 returns CONTINUE on pre-registered bounds
wrapper / DI analysis        after the corpus shows resolved-only coverage is too small
delta extraction (problem A) after localization qualifies on a fixed normalized input
PR-time regression signal    after the locator has published per-cell numbers
delivery runtime             after a paying internal-platform design partner exists
provider-side distribution   after §13 is contractually settled
```

Nothing is built now beyond the AMA-0.5 packet, and that packet is capped at 48
maintainer-hours with no `EXTEND` outcome. This note exists so the narrow wedge
does not mutate, three commits later, into a SaaS, a GitHub App, a cloud runtime,
and an insurance policy — nor, three revisions later, into a permanent seminar
about how one would rigorously determine whether to begin.
