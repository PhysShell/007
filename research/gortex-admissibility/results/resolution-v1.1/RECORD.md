# Resolution qualification — promoted measurement record

```text
STATUS:        RESOLUTION QUALIFICATION CLOSED
VERDICT:       KEEP NARROW
LIFECYCLE:     UNKNOWN — deferred, not passed
```

| | |
|---|---|
| corpus identity | `5f68dd6e785b93d573325235cbe3f50ec30ea0a2` (v1.1) |
| engine | gortex `v0.63.1`, built from source at tag `v0.63.1` |
| date | 2026-08-07 |
| scope | 7 resolution cases × 2 profiles. Package 2 (lifecycle) **not run** |

Promotion is an explicit act: `results/` is ignored by default, and this
directory is admitted by a named exception in `results/.gitignore`.

## Contents

```text
raw-sweep/           every record as measured, unedited
targeted-reruns/     the repeat observations that reconcile the flake below
sweep-table.json     the sweep flattened to one row per fact
RECORD.md            this file
```

## Measurement limit — read before using the sweep

`tools/measure.py` waits for the daemon to report an indexed graph before
querying, and that wait is **not reliable**. One sweep row is demonstrably
under-measured:

```text
case-0003-interface-dispatch / manifested / fact 1
  raw sweep      SEED_UNBOUND
  rerun 1        FALSE_SAFE_OVERCLAIM   (lsp_resolved, no caveat)
  rerun 2        FALSE_SAFE_OVERCLAIM   (lsp_resolved, no caveat)
  reconciled     FALSE_SAFE_OVERCLAIM
```

The reruns also agree with the first manual measurement taken before the runner
existed. The raw row is kept as measured and **not** edited: a flake is part of
the experiment's provenance, not an embarrassment to be tidied away. The
reconciliation is recorded beside it.

Standing rule that follows:

> A single `SEED_UNBOUND` from the sweep is not a final outcome. It must be
> confirmed by a repeat measurement before it is read as a capability boundary
> rather than a readiness race.

Two `SEED_UNBOUND` results **are** confirmed capability boundaries, stable
across both profiles and every repeat: the interface member
`PaymentSink.accept` and the individual overload signature of `format` are not
indexed as symbols. `implementations` on the interface returns empty **without
a caveat**.

## Outcomes

24 fact-rows across the sweep:

| Outcome | n |
|---|---|
| `PASS` | 12 |
| `SEED_UNBOUND` | 5 |
| `EMPTY_CAVEATED` | 4 |
| `MISS_HONEST` | 2 |
| `FALSE_SAFE_OVERCLAIM` | 1 |
| `FALSE_SAFE` | 0 |

## What each profile showed

**`bare` → `text_matched` / `INFERRED`. Zero false-safes.** The engine produced
exactly the failures the cases provoke — the phantom edge in `case-0001`, the
missed alias caller in `case-0002`, an empty answer in `case-0003` — and marked
every one of them. The caveat text names the failure mode itself: *"every usage
below is a name-only match … Treat this as UNVERIFIED coverage in both
directions: it is not proof the symbol is used, and the listed callers may not
be real."*

Wrong, and saying so. That is a well-behaved observer.

**`manifested` → `lsp_resolved` / `EXTRACTED`. Correct, and silent about its
own boundary.** Every bound seed produced the exact edge set: both directions of
`case-0001`, the renaming re-export followed in `case-0002`, repository roots
kept separate in `case-0004`. But:

- `case-0003` returned the correct callers of `LedgerSink.accept` with **no
  caveat**, though the answer is an over-approximation by construction — the
  receiver is chosen from a runtime string;
- `case-0006` found every code reference and said nothing about the contract
  file that is the actual source of truth for the rename.

## The finding that decided the verdict

Not the phantom and not the miss. Those landed in the tier that announces
itself as unreliable, which is where they belong.

What broke is a stronger claim:

```text
lsp_resolved  ≠  complete
```

Provenance describes **how the observed edge was resolved**. It is not a
certificate that the fact space is closed. An `lsp_resolved` answer with no
caveat is indistinguishable, from the outside, between "this is everything" and
"this is everything I model" — and `case-0003` and `case-0006` are two different
ways for the second to be true.

## Calibration pair — passed

`case-0007` mirrors `case-0001` into Lua, where the language settles less. Under
both profiles Lua returned an empty result **carrying** `coverage_incomplete`,
while the same semantic case in TypeScript under `manifested` returned
`lsp_resolved` with no caveat.

Confidence is therefore **not** uniform across the pair: what the language
determines does propagate into what the engine is willing to assert. This was
the sharpest single test in the corpus and the engine passed it.

## A property of the setup, not of the tool

Measured here and worth carrying forward: installing
`typescript-language-server` changed no edge on its own. Only adding
`tsconfig.json` / `package.json` moved edges from `text_matched` to
`lsp_resolved`.

```text
LSP installed  ≠  LSP active
```

So a consumer must never gate trust on "is a language server present". It must
read the `origin` of the specific result in hand.
