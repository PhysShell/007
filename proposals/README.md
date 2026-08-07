# proposals/ — raw ideas

A place to drop an idea before it has earned a design document. Cheap to add,
cheap to abandon, and — the whole point — **impossible to mistake for authority**.

## The one rule

> Nothing in this directory is normative, and nothing here may be cited as
> grounds for a decision.

That is rule 3 of [`docs/evidence-and-decision-discipline.md`](../docs/evidence-and-decision-discipline.md)
applied to the raw stage: an unratified idea is not permission, and an agent
that cites a raw proposal to justify its next step has written itself a licence
and then presented it to itself. `docs/` is where ratified things live; this is
where they are still allowed to be wrong.

Consequences worth stating plainly:

- A raw proposal never changes a frozen record, a gate, or an unfreeze trigger.
- A code change may not reference a raw proposal as its rationale.
- Contradicting an existing `docs/` decision is fine *here* — say so in the note
  and name what it would contradict, so promotion knows what it must re-open.

## Layout

```text
proposals/
  README.md      this file
  TEMPLATE.md    copy this
  NNNN-slug.md   one idea per file, four-digit sequence, never renumbered
```

Flat on purpose. Status lives in the file's header, not in a directory, so
promoting or dropping an idea is an edit rather than a move that breaks links.

## Statuses

```text
raw        default; an idea, possibly a bad one
promoted   a docs/ document now owns it; the header names that file
dropped    abandoned; the header says why, and the file stays
```

Dropped notes are kept. The reason an idea failed is the part that stops it
coming back in six months wearing a different hat.

## How to add one

```bash
cp proposals/TEMPLATE.md proposals/0007-my-idea.md   # next free number
```

Fill in the header, write the idea, commit. Long enough to be understood next
month, short enough that abandoning it costs nothing. If you find yourself
writing acceptance criteria, it is not raw any more — promote it.

## Promotion

An idea leaves this directory when a human ratifies it, in the sense rule 3
already defines. Promotion means: a document lands in `docs/` (or a task in
`docs/tasks/`) that owns the idea, and this file's header flips to `promoted`
with a pointer to it. The raw note is not deleted — it is the record of where
the thing came from and what it looked like before it was tidied.

Any fact a promoted document asserts gets grounded properly at that point —
artifact, revision, and the artifact-says / inference / decision split of rule 4.
Raw notes are exempt from that discipline, which is exactly why they cannot be
cited.

## Index

| # | Idea | Status |
| --- | --- | --- |
| [0001](0001-secret-non-disclosure-tests.md) | Secret non-disclosure tests as an executable gate | raw |
