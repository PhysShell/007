# B2-T run 01 — calibration results

```text
Corpus      A1-F §9, N=41, unit = reviewer-recorded finding
Contract    preregistration.md §3, frozen at C = 153684f (F = 68c3f1d)
Output      research/b2-t/run-01/classifier-output.json, committed at 3d5acf1
            BEFORE packet-key.json was opened
Computed    from the per-item records, not from the classifier's prose summary
```

## The frozen §4.3 outputs, and nothing else

```text
classified_n / 41     41 / 41
compound_n            0
unclear_n             0

R_broad   point estimate        20 / 41  =  48.8%
          sensitivity interval  48.8% – 48.8%

R_obs     point estimate         5 / 41  =  12.2%
          sensitivity interval  12.2% – 12.2%
```

The sensitivity interval collapses to the point estimate. That is arithmetic,
not a strong result: with `compound_n = unclear_n = 0` the lower and upper
bounds are the same expression. The interval existed to expose unresolved rows
being dropped; there were none to drop.

Constraint `r_obs = yes ⇒ r_broad = yes` — **holds** on all five.

`r_obs` items: `item_015`, `item_028`, `item_036`, `item_037`, `item_041`.

## Reviewer provenance — descriptive stratification only

```text
                external (n=4)      internal (n=37)
R_broad             2 / 4              18 / 37
R_obs               0 / 4               5 / 37
```

n=4 cannot support a detection-rate claim and is not reported as one (§4.2).

## Pre-registered controls, as they came back

**Selection negatives (§8)** — real defects from the same period, registered
before classification as *not* identity conflations:

```text
R5.1#2  CANCEL could not pass its own guard   -> item_004   r_broad = no    held
R5.1#4  genesis signature vs genesis prose    -> item_014   r_broad = YES   did not hold
```

One of the two negatives came back positive. The blind classifier read R5.1#4 as
"a single resolution function covered both genesis and non-genesis events
although the two differ in what inputs are available". That is a fact about this
run and is recorded here without adjudication: whether the definition is too
broad, or the negative was mis-chosen when it was registered, is not something
this document decides.

**Contaminated items (§2.1)** — named before the run as already discussed at
length, and therefore not independent:

```text
R5.2#2  producer_execution_id grain   -> item_026   r_broad = yes
R5.2#3  ArtifactRef byte string       -> item_007   r_broad = yes
```

Both landed positive. They remain in the denominator per §2.1. **No
contamination-excluded rate is computed here**: that statistic is not in the
frozen §4.3 list, and adding one after seeing the numbers is precisely the move
the preregistration exists to prevent. Whether to compute it is a decision for
the maintainer, to be recorded before it is computed.

## Not in this document

No H0/H1/H2 verdict. §4.3 forbids calibration issuing one, and §2.1 bars this
corpus from confirming anything — the categories were formed while reading it.
Any reading of the numbers above against the hypotheses is post-calibration
interpretation and must be labelled as such wherever it appears.

## One recorded defect in the output itself

The classifier's prose summary states "r_broad = yes on 18", then lists twenty
ids, then self-corrects to 20 inside the same sentence. Recorded verbatim in
`classifier-summary-verbatim.md`. Every number above is counted from the
per-item records, which §3 makes the data.
