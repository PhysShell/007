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

## Open before the timer starts

`reserve_activation_budget_minutes` and `max_reserve_activations` have no
protocol-assigned values. Four primaries censoring consume the 720-minute budget
exactly, so reserves are unfunded unless drawn from the AMA-0.5 package remainder
(§10.4). Both fields are required by the schema so the question cannot be
deferred past selection — a probe that discovers its reserve is unaffordable
halfway through has already spent the budget that would have answered it.
