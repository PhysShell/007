# Classification task

You are given a classification contract and 41 items. Classify every item
against the contract. Nothing else is asked of you.

Items are in `items.md`, one per `## item_NNN` heading. Their order carries no
information. Each item is verbatim source text from a software project's review
history.

## The contract

Reproduced verbatim. Where it says `finding_ref`, use the item's `item_id`.

### IDENTITY_DECOMPOSITION (`R_broad`)

> The defect's primary cause is that two independently variable objects, grains,
> scopes, roles, revisions, or meanings were represented or referred to as one —
> or that an identifier failed to define which of several such objects it
> identifies.

### OBSERVATION_BINDING (`R_obs`)

> Correcting the defect requires distinguishing the representation/carrier of
> evidence from the proposition actually observed, or distinguishing multiple
> observations from the semantic support relation among them.

`R_obs` is a subset test, not a synonym for `R_broad`. A grain/role/actor
identity defect can be `R_broad` without being `R_obs`.

### Exclusion (binding on both)

> Merely adding an id, digest, enum, unknown/unsupported state, or provenance
> field does not qualify unless the defect was **caused** by conflating the
> things that field separates.

This exclusion is what keeps a fix that introduces an `unresolved` state from
entering `R_broad` through the back door. Ambiguity/unknown-state handling is a
different axis and is not evidence for either category.

### Per-finding record

```
finding_ref            round + number, e.g. "R5.1 #2"
classification_status  classified | compound | unclear
primary_root_cause     free text, exactly one   — iff status = classified
r_broad                yes | no                 — iff status = classified
r_obs                  yes | no                 — iff status = classified
secondary_tags[]       zero or more, reported separately
confidence             low / medium / high
evidence               one sentence, quoting the finding
```

- `classified` — one primary root cause could be named. `primary_root_cause` is
  present; `r_broad` and `r_obs` are answered.
- `compound` — two genuinely distinct root causes, neither subordinate.
- `unclear` — the recorded text is insufficient to classify.

For `compound` and `unclear`, `primary_root_cause` is **absent** and `r_broad` /
`r_obs` are **n/a**. They are never coerced to `no`: "could not be classified" and
"classified as not this" are different facts, and collapsing them is the defect
this study is about.

**Constraint:** `r_obs = yes` ⇒ `r_broad = yes`. `R_obs` is a subset test; an
observation-binding defect is by construction an identity defect.

The headline rates use **primary cause only**. Secondary tags are reported
alongside and never folded into the headline.

## Output

One record per item, all 41, in item order, as JSON:

```json
[
  {
    "item_id": "item_001",
    "classification_status": "classified | compound | unclear",
    "primary_root_cause": "free text — present only if status is classified",
    "r_broad": "yes | no — present only if status is classified",
    "r_obs": "yes | no — present only if status is classified",
    "secondary_tags": [],
    "confidence": "low | medium | high",
    "evidence": "one sentence, quoting the item"
  }
]
```

Omit `primary_root_cause`, `r_broad` and `r_obs` entirely for `compound` and
`unclear`. Do not substitute `no` for a missing answer.

Classify each item independently. Do not ask for clarification mid-pass, do not
revise earlier items after seeing later ones, and do not seek any information
outside this packet.
