# AMA-0.5 — pre-freeze cost-floor probe

Machine encodings for step 0 of the AMA-0.5 packet. The normative source is
[`docs/architecture/api-migration-direction.md`](../../docs/architecture/api-migration-direction.md)
§10.2–§10.3; this directory adds no rules, it makes two of them checkable.

```text
cost-probe-manifest.schema.json   the immutable selection record
measurement-record.schema.json    one record per case run
normalized-delta.schema.json      the problem-A input AMA takes as given (§4)
deltas/                           normalized deltas, authored blind
```

No manifest is committed here yet. Selecting four repositories, four commits,
four reserves, and their digest-pinned provider artifacts is the selection act
itself — it cannot be synthesised, drafted with placeholders, or filled in later
without destroying the pre-registration it exists to provide.

## Deltas are authored before consumers are searched

§4 requires the normalized delta to be written blind to the consumer repository.
Blinding is only attestable if it is established *first*: once anyone has looked
at consumer code, no later attestation can undo the knowledge, and the case is
excluded rather than discounted. So the order is fixed — pin the provider
artifacts, write the delta, commit it, and only then go looking for repositories
that use the affected surface.

Committed so far, both `delta_provenance: historical`:

```text
D1  Stripe    billing_mode.type default classic → flexible   (2025-09-30.clover)
D2  AWS JS    maxRetries → maxAttempts, equivalent = N + 1    (v2 → v3)
```

D2's historical supply was surveyed under a predeclared five-repository frame and
found insufficient for this probe — see `d2-supply-survey.json` for every
candidate, admissible or not, and the hypothesis the result generated. D2 itself
is retained and unchanged: it was not refuted, it ran out of consumers.

Each records a **visibility hypothesis** — expected class and expected oracle
fidelity — before any mutation is run, so the hypothesis cannot be retrofitted to
the result. §3 still decides the class empirically.

## Temporal alignment: a migration-shaped delta needs a pre-migration consumer

A delta that describes a transition — one major version of an SDK to the next —
is not localizable at a consumer's HEAD, because an actively maintained consumer
has already performed the transition. Pinning HEAD searches a repository that
already did the work, and finds either nothing or a value chosen natively under
the new semantics.

Discovered the hard way while selecting for D2: two candidates whose indexes
matched `maxAttempts` in real production TypeScript turned out to carry no
`aws-sdk` v2 at all, and the one genuine retry configuration among them
(`maxAttempts: 8, retryMode: 'adaptive'`) was authored under v3 from the start.
Neither refuted D2. Both showed that HEAD is the wrong time coordinate for it.

So a migration-shaped case is built from three parts, kept separate:

```text
provider artifact           defines the semantic delta, authored blind
pre-migration consumer      the object of localization: the last relevant
                            commit BEFORE the consumer's own migration
historical migration commit consumer-side evidence for adjudication only
```

The migration commit never feeds the normalized delta and is never a source of
normalization — it establishes ground truth for one case. And this rule is not
written into `deltas/D2-*.json`: the requirement follows from the delta's own
shape and is provider-derivable, but the amendment itself was prompted by
consumer contact, and a blinding attestation cannot be re-issued after the fact.
It lives here, in the selection procedure, for the same reason the Stripe
`apiVersion` observation lives in case evidence.

Selection filter for a migration-shaped case:

```text
1. a real migration PR or commit exists
2. the pin is its parent / base
3. the occurrence is in .ts/.tsx, or is PROVEN to be in the pinned TypeScript
   program (allowJs/checkJs) — otherwise this is a silent widening of the
   language scope at exactly the moment a convenient case was needed
4. the old value is connected to a control-flow budget
5. the migration witness shows that value carried over or dropped
6. the pinned state reproduces at least well enough to adjudicate
```

If a bounded search yields no primary and no reserve, the honest outcome is not
a more convenient delta. It is: **D2 has insufficient historical supply for this
probe**, and the scope narrows.

## Pinning a provider artifact

Fetch it **twice** and pin only a representation whose bytes reproduce. This is
not a formality: the HTML representation of the Stripe changelog page produced
two different digests on two consecutive fetches (build nonces in the page
shell), so pinning it would have reported STALE on every re-check and the binding
would have meant nothing. The same page's markdown variant reproduced byte for
byte, as did the AWS migration page's HTML.

`digest_stability_verified` is `const: true` in both schemas: an artifact that
cannot be pinned stably cannot be used, and the fix is to find a stable
representation, not to relax the field.

Retrieved copies are retained out-of-tree; the repository carries the digest, the
URL, and the exact normative quotation the delta rests on, rather than a copy of
third-party documentation.

## Execution rules that the schemas cannot express

**The selection commit is the commit.** The manifest carries no field for its own
commit hash: filling one changes the file and therefore the hash. Per rule 4 of
[`docs/evidence-and-decision-discipline.md`](../../docs/evidence-and-decision-discipline.md),
git records the commit and *that* is the binding — a SHA copied into the object
it identifies would be a fresh unbound claim. The measurement records reference
it as `manifest_commit`, where it is an external fact.

**The manifest is never edited after its commit.** A legitimate reserve
activation is recorded in the measurement records. If the selection turns out to
be wrong, that is a finding about the selection, not a licence to reselect.

**The censor is enforced on the timer, not on judgement.** At 180 accumulated
active minutes the outcome is written before any further work. "One more hour,
it's nearly done" is how methodology packets acquire permanent residency.

**Active minutes, derived not asserted.** `total_maintainer_minutes` must follow
from `timer_log`. Waiting on an agent, a compiler, or CI belongs between a
`pause` and a `resume`: those costs are real, are recorded elsewhere, and do not
consume the twelve-hour budget — otherwise human cost is declared zero while an
agent spends three weeks producing material someone then spends three weeks
checking.

**Exclusions are classified, not averaged.** `excluded_cost_adjacent` counts as
censored in the §10.3 tally, because failing to reproduce a repository is cost
evidence. Only `excluded_validity` — blinding unattestable, no provider artifact
— permits the stratum's reserve. A cost-adjacent exclusion confers no right to
reach for an easier case.

**Zero is a result, not an absence of one.** `accepted` includes a completed
adjudication with zero confirmed occurrences; zero is evidence, not an exclusion
and not a censor, and all the time it consumed is recorded. Three situations must
not be confused:

```text
temporal ineligibility known BEFORE the manifest
  → the case is simply not selected; it never becomes a case at all
search completed after the manifest, occurrences = 0
  → accepted, confirmed_occurrences: 0, full time counted
work not completed because of cost
  → censored / excluded_cost_adjacent under the existing rules
```

**The decision counts outcomes, not money.**

```text
0 censored → AMA-0.5 may continue
1 censored → NARROW, excluding or isolating that stratum;
             STOP if the stratum is commercially mandatory
≥2 censored → STOP
```

**The early probe publishes no monetary projection at all.** The `B_floor ≤ 174h`
figure previously recorded on a 0-censored outcome was derived from four cases
under a soft two-cell hypothesis, not a universal constant, and does not survive
the narrowing to three strata (`scope-decision-01.json`). The timings are inputs
to the post-freeze economic gate (§8.4), where a `p90` over an untruncated
distribution can legitimately cross the 200h/400h thresholds. No early aggregate
exists, and none may be invented after the numbers are visible.

## Narrowed scope

```text
retained   1. provider-clear / reproducible
           2. wrapper / alias indirection
           4. historically messy / reproduction-hostile
excluded   3. semantic control-flow dependence
```

D2 produced zero admissible cases in its frozen five-repository supply frame, so
stratum 3 cannot be filled. Numeric consequences: three primaries, 540 minutes of
primary budget, the 180-minute censor and the reserve parameters unchanged, and
the package cap still 48 hours counted as actually consumed. Nine hours is what
three primaries can physically spend; the freed three do not become a longer
adjudication or a new permitted search.

**Validity limit:** this probe makes no claim about the cost of
semantic-control-flow-dependence cases. Control-flow dependence is excluded from
*this probe*, not from AMA. Later taxonomy discovery may find dense control-flow
cells; they may not retroactively become a fourth early case.

A replacement delta chosen for high occurrence density was considered and
rejected: it would be selected after observing supply, on a criterion the failed
search itself generated — a new discovery round aimed at preserving a four-row
table, which is `EXTEND` in a respectable shirt. Full reasoning, including the
four-step shape of the ascertainment bias, is in `scope-decision-01.json`.

## Reserve budget (ratified)

```text
reserve_activation_budget_minutes: 360   (ceiling, drawn from the package remainder)
max_reserve_activations:           2
```

Both are `const` in the schema, so a manifest carrying different values does not
validate. The reasoning, recorded so it is not re-litigated mid-probe:

- Each reserve obeys the same 180-minute censor, so 360 fully funds two
  activations with no mid-experiment "give this one a bit longer".
- After the maximum permitted reserves, 30 of the package's remaining 36 hours
  survive for taxonomy discovery, codebook, holdout, and the post-freeze pilot.
- One validity defect is ordinary sampling noise. A second is still recoverable —
  that is precisely what a pre-designated reserve is for. A third means the
  selection failed systemically, and continuing to swap cases would be resampling
  against observed results.
- `4 / 720` would permit replacing the entire original sample and eat a third of
  the package. `1 / 180` is too brittle: a second independent validity defect
  would kill a probe that the reserve construction already exists to rescue.

Derived consequences:

- 360 is a ceiling, not six hours pre-spent. Only minutes actually used count
  against the 48-hour package cap; the remainder stays with the package.
- A third `excluded_validity` outcome leaves the probe incomplete, restricting
  outcomes to `NARROW` or `STOP`.
- Each stratum has exactly one reserve, so two activations mean two different
  strata. There is no reserve of a reserve.

No protocol questions remain open. The next artifact is the committed manifest.
