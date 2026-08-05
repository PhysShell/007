# AMA-0.5 — pre-freeze cost-floor probe

Machine encodings for step 0 of the AMA-0.5 packet. The normative source is
[`docs/architecture/api-migration-direction.md`](../../docs/architecture/api-migration-direction.md)
§10.2–§10.3; this directory adds no rules, it makes two of them checkable.

```text
cost-probe-manifest.schema.json   the immutable selection record
measurement-record.schema.json    one record per case run
```

No manifest is committed here yet. Selecting four repositories, four commits,
four reserves, and their digest-pinned provider artifacts is the selection act
itself — it cannot be synthesised, drafted with placeholders, or filled in later
without destroying the pre-registration it exists to provide.

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

**The decision counts outcomes, not money.**

```text
0 censored → AMA-0.5 may continue; record only B_floor ≤ 174h, which is
             explicitly NOT a qualification of the economics
1 censored → NARROW, excluding or isolating that stratum;
             STOP if the stratum is commercially mandatory
≥2 censored → STOP
```

The four timings are inputs to the post-freeze economic gate (§8.4), where a
`p90` over an untruncated distribution can legitimately cross the 200h/400h
thresholds. No early aggregate exists, and none may be invented after the numbers
are visible.

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
