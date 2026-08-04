# State-observables schema v0

`state-observables-v0.schema.json` (`o7.b1.state-observables/v0`) is the minimal
typed-state schema for **one** B1 experiment. It is deliberately not a universal
ontology of project knowledge — it is only large enough to represent, project,
and evaluate the `case-0001` golden fixture.

## What it encodes

**Observation kinds:** `goal`, `decision`, `constraint`, `status`, `work_item`,
`unresolved_question`, `evidence`, `risk`, `next_action`.

**Relation kinds:** `supports`, `derived_from`, `supersedes`, `contradicts`,
`blocks`, `depends_on`, `part_of`.

Every observation carries a stable `observation_id`, a `kind`, `topics`, a
`statement` (and optional `structured_value`), an `authority`, a `status`, at
least one `provenance` ref, and — where the source provides them — `observed_at`
/ `valid_from` / `valid_to` / `superseded_by`.

## Topics, and what they are deliberately not

`topics` is **task-independent** classification: it says what a statement is
*about*, never how much it matters. Topic ids are drawn from the gold state's own
closed `topic_vocabulary`, and the selector contract fails closed on any id
outside it, so a task can never silently match nothing.

An observation may **not** carry a global `importance`/priority field — the
validator rejects it. Priority is meaningless without a task, and putting it on
the observation would let one task's judgement leak into every other task's
projection. Task-relative priority lives in `selector-v0.schema.json`
(`o7.b1.selector/v0`) and nowhere else.

## Companion contract

`selector-v0.schema.json` defines how a task's `selectors` block turns a gold
state into a projection: eligibility, exclusion, relevance, scoring, the anchor
escape hatch and its ratio ceiling, deterministic ordering, budget behaviour and
the full fail-closed list. Selection is a pure deterministic function of
`gold state + task + selector contract/version + budget`.

## Not frozen

Neither schema is frozen. The corrective round
(`#issuecomment-5182632187` §7) blocked freezing precisely because the
task-applicability representation had not been chosen yet; `topics` plus an
external selector contract is the *first* working answer, not a settled one. A
freeze needs at least a second fixture and a holdout, neither of which exists.

## Authority vocabulary (closed)

```
platform_capture          bytes captured from a platform surface
deterministic_derivative  produced by a deterministic extractor over a RAW digest
human_confirmed           the owner/controller stated it
controller_or_repo_record a repo/issue/PR/commit record
agent_claim               an agent asserted it (advisory)
```

The order above is **not** a trust ranking that timestamps or agreement can
override. Rules the tooling enforces (not all expressible in JSON Schema alone):

- `agent_claim` never becomes authoritative on its own.
- Two agreeing `agent_claim`s do not manufacture evidence.
- A `superseded` observation must name `superseded_by` and must not appear in a
  current projection as if in force.
- A `contradicts` relation is never resolved silently; it surfaces.
- Absence of a fact stays absence — it is not backfilled.
- Timestamps are never the sole ordering authority.
- The schema does not depend on Qodec or any codec backend.

## Freeze semantics

Schema v0 is considered **frozen** only for the purpose of selecting holdout
cases and running versioned experiments against a stable representation. Any
backward-incompatible change after that freeze requires a new schema version
(`.../state-observables-v1...`); v0 is never edited in place to mean something
new. `case-0001` shaped this schema, so — per `../holdout/README.md` — it can
debug the representation but can never substantiate a generalization claim.
