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

## What is raw here is the proposal, not the facts

There is **no evidentiary carve-out for this directory.** Rule 4 of the same
document — and the "Grounding factual claims" rule in
[`AGENTS.md`](../AGENTS.md), which names a *doc* as one of the places it binds —
applies to every factual claim in this repository, including these files. A
claim about what the code or an existing mechanism does names the artifact, the
exact property, and a revision, and keeps the artifact-says / inference /
decision split, here exactly as in `docs/`.

What makes a note raw is that its **proposal** is unratified, not that its
**facts** are cheaper. Non-citability comes from rule 3, not from a lower
standard of evidence — and the two must not be confused, because a directory
that relaxed grounding "since nobody may cite it anyway" would be a supply of
unbound claims sitting one promotion away from becoming citable.

The practical form:

- Bind a claim about this repo to a commit (`git rev-parse HEAD`), not to a date.
  Dates are for when *you looked*; commits are what a reader can recover. Write
  the **full object ID**, not a seven-character prefix — a prefix stops resolving
  the moment another object shares it, which is precisely when someone is trying
  to check an old claim.
- Bind an external source to whatever immutable anchor it has — a tag, a commit,
  a content digest, a dated API version — plus the URL and the access date. Where
  the source is a mutable page with no such anchor, the verbatim quote is what is
  bound; say so, and treat the claim as stale on any later reading.
- A note is allowed to say "I did not verify this" — that is a bound statement.
  What it may not do is state an unverified thing in the voice of a fact.

Keeping the bar high is also what makes the notes cheap to promote: a promoted
document inherits claims that are already grounded, instead of a pile that has
to be re-derived.

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

By hand:

```bash
cp proposals/TEMPLATE.md proposals/0007-my-idea.md   # next free number
```

Fill in the header, write the idea, commit. Long enough to be understood next
month, short enough that abandoning it costs nothing. If you find yourself
writing acceptance criteria, it is not raw any more — promote it.

Or hand it to an agent: **`/raw-proposal <link or idea>`**, or just paste a link
and ask for a proposal. The procedure lives in
[`.claude/skills/raw-proposal/SKILL.md`](../.claude/skills/raw-proposal/SKILL.md),
which reads this file rather than restating it — so the conventions are edited
here, in one place, and the skill follows.

## Promotion

An idea leaves this directory when a human ratifies it, in the sense rule 3
already defines. Promotion means: a document lands in `docs/` (or a task in
`docs/tasks/`) that owns the idea, and this file's header flips to `promoted`
with a pointer to it. The raw note is not deleted — it is the record of where
the thing came from and what it looked like before it was tidied.

Promotion does **not** introduce grounding — the claims were bound when they were
written (see above). What promotion adds is ratification and the obligation to
re-check: a bound claim is stale once its artifact moves, so a note promoted
months later re-verifies against current `HEAD` before its facts carry any
weight.

## Index

| # | Idea | Status |
| --- | --- | --- |
| [0001](0001-secret-non-disclosure-tests.md) | Secret non-disclosure tests as an executable gate | raw |
