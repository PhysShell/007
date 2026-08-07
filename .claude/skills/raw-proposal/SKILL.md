---
name: raw-proposal
description: Turn a link, paper, repo, or rough idea into a raw proposal file under proposals/. Use when the user pastes a URL and asks for a proposal, says "сделай пропозал" / "запиши идею" / "накидай пропозал", or asks to survey an external system as prior art for 007. Not for promoting a proposal into docs/ — that needs a human.
---

# Raw proposal

Turns "here is a link / here is a thought" into a committed file under
`proposals/`, in the shape the directory already expects.

## Read these first — do not restate them here

- `proposals/README.md` — the one rule (nothing there is normative or citable),
  the statuses, the promotion path.
- `proposals/TEMPLATE.md` — the section shape you are filling in.

They are the source of truth. If they disagree with this skill, they win, and
this skill is what needs fixing.

## Procedure

1. **Read** `proposals/README.md` and `proposals/TEMPLATE.md`.
2. **Take the input.** A URL, a paper, a repo, a paragraph of thinking, or a
   chat we just had. If it is a link, fetch it. If fetching fails, say so and
   write the note from what the user gave you — do not invent the contents.
3. **Pick the number**: `ls proposals/` → next free four-digit sequence. Never
   reuse or renumber.
4. **Write** `proposals/NNNN-slug.md` from the template. Slug is short and
   lowercase-hyphenated.
5. **Add the index row** in `proposals/README.md`.
6. **Commit** (see below). Do not push and do not open a PR unless asked.

## What the note has to do

The point is not to summarise the link. It is to answer: **what, if anything,
does this change for 007** — and to be honest when the answer is "probably
nothing, but here is the shape worth remembering".

- **Itch** — what is annoying or missing *here*, in this repo, today. If you
  cannot name one, the note is a bookmark, and it should say that about itself.
- **Idea** — the transferable part. A mechanism, a split, a signature, a
  diagram. Not a feature list copied from a landing page.
- **Why it might be wrong** — mandatory and non-decorative. What it costs, what
  it collides with, which existing decision in `docs/` it would re-open, and the
  reason a reasonable person says no. A note without this section is advocacy.
- **What would make it real** — the cheapest experiment or measurement that
  turns opinion into evidence.

Match the repo's register: dense, concrete, dry. No enthusiasm about
"platforms", "solutions", or "ecosystems".

## Links and facts

Raw notes are exempt from rule 4 grounding (`docs/evidence-and-decision-discipline.md`)
— that is what makes them cheap, and it is also exactly why they cannot be
cited. So:

- Record the **URL and the access date** in the note. That is what makes later
  verification possible.
- Do **not** launch a verification campaign. Checking a headline number or two
  is fine; a full artifact-bound survey means the thing is not raw any more.
- Never launder a source's marketing into a stated fact. "The page claims X" and
  "X" are different sentences, and the difference is the whole discipline.
- If something is genuinely load-bearing and you *did* verify it, say how, in
  one clause. If you did not, say that instead.

## Don'ts

- Do not write into `docs/`, and do not touch a frozen record, a gate, or an
  unfreeze trigger. Promotion is a human decision.
- Do not present the proposal as accepted, or write acceptance criteria. If the
  note is turning into a design document, stop and tell the user it wants
  promotion instead.
- Do not create implementation tasks or start implementing.

## Commit

Commit only `proposals/`. If the current branch is `main`, switch to
`raw-proposals` (create from `origin/main` if missing) first.

```text
docs(proposals): NNNN -- <title>

<what the idea is, in two or three lines>
<where it came from — link and access date>
<the strongest objection to it>
```

Then tell the user the file, the number, and the objection you found hardest to
answer — that last part is the useful half of the reply.
