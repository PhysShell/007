# B2-T run 01 — interpretation

```text
STATUS:  POST-CALIBRATION INTERPRETATION — NOT PREREGISTERED CONFIRMATION
         §4.3 bars calibration from issuing an H0/H1/H2 verdict; §2.1 bars this
         corpus from confirming anything, because the categories were formed
         while reading it. Nothing here may be quoted as a confirmed prediction.
         Does not modify the preregistration; the freeze at C = 153684f holds.
```

Produced by an independent instance from the frozen material and the results,
deliberately without being told what either participant had predicted, then
reviewed and amended in discussion.

## 1. What the numbers constrain

Established, and only this: on the complete 41-finding corpus recorded by this
review process, under the frozen definitions, by one blind classifier — all 41
were classifiable, 20 took `R_broad` as primary cause, 5 took `R_obs`, and every
`R_obs` lies inside `R_broad`.

That is enough to retire one story: "a couple of amusing one-offs". Under the
current definition the phenomenon is common among this process's recorded
defects.

It establishes none of: that 48.8% of defects *in the codebase* are of this
nature; that a future review yields the same share; that the taxonomy predicted
the phenomenon independently; that the 20 are one engineering-homogeneous,
mechanically catchable cause; that a transferable record exists; or that moving
one would pay.

The collapsed sensitivity interval adds almost nothing to confidence. It says
only that this particular arithmetic source of uncertainty is absent because
there were no unresolved rows. Construct validity did not become 100%.

**Reading against the hypotheses, labelled as interpretation.** H0 sits poorly
with the result, though calling it refuted would require a threshold that was
deliberately never frozen. H1 gains the strongest practical argument. H2 is not
established: its structural conjunct — the *same* semantic record produced by
one authority boundary and consumed by another — was not measured at all, and
five similar defects in five places remain compatible with pure H1. H3 has
essentially no bearing evidence.

## 2. The three limits that reduce the weight of 48.8%

**Taxonomy shaped by this corpus (§2.1).** The categories were designed around
phenomena observed here. Blinding protects against one rater's fitting; it does
not protect against a feature space built around the material. The honest
statement of what ran is: after building a category inspired by this defect set,
a blind classifier placed about half that set in it.

**Control B did not hold.** The registered negative — a resolution function
whose signature required a value only obtainable after work it had not done —
came back `r_broad = yes`, on the reading that one function covered two cases
with different available inputs. From a single failed negative it is **not**
possible to distinguish "the control was mis-chosen" from "the definition is
wider than the intended construct". What is established is narrower and duller:
the frozen wording admits that rationale without violating the contract. No
confidence percentage is attached to this; there is no denominator for one.

**`compound_n = 0`.** An escape hatch for "two genuinely distinct root causes,
neither subordinate" was offered explicitly and used zero times. This licenses a
claim about the **instrument**, not about any item: several findings may
legitimately have one primary cause behind multiple symptoms, and no individual
item is second-guessed here. But combined with Control B it is a coherent
threat — a classifier that may prefer a sufficiently broad single causal story
over admitting causal plurality, which would inflate `R_broad` precisely through
breadth of explanation. By how much, these data cannot say.

## 3. The correct sceptical formulation, and its limit

The strongest attack is not "the category can explain any software defect". The
data refute that: **21 of 41 came back `no`**, and the one held negative control
held. A universal solvent does not leave a 51% negative rate.

The accurate and more dangerous version is:

> the category may be elastic enough to merge several engineering-distinct
> defect classes under one label, inflating apparent reuse value.

One does not need a universal solvent to end up with a useless bag of five
different chemicals sharing a label.

## 4. A provenance fact to carry with the table

The external stratum's only two `r_broad` positives are **exactly** the two
findings registered in advance as contaminated by prior discussion
(`item_007` = R5.2#3, `item_026` = R5.2#2).

So this corpus contains **no uncontaminated external-reviewer `R_broad`
positive**. This neither licenses a contamination-excluded rate nor says an
external reviewer does not find such defects. It forbids the attractive story —
"the pattern showed up independently under an external reviewer too". It did
not. Whenever the provenance table is repeated, this goes with it.

## 5. Procedural ruling: the boundary trace was not performed

The cheapest honest test of H2's structural conjunct is a no-code boundary
trace over existing contracts: for each `R_obs` case, can one name a single
semantic record S that authority A really produces, that crosses a real
boundary, and that authority B really consumes *as S* — as opposed to A and B
independently reconstructing a similar concept?

It was **not** done. §4.3's inconclusive branch says the only permitted next
action is corpus extension, and the clause is written without qualification.
Declaring a boundary trace "engineering reading, not research" after the result
is in would be a loophole taken after seeing the numbers, and the fact that it
could kill H2 in a day is exactly what makes it tempting.

Recorded instead: **the clause is over-binding** — it forecloses cheap
investigation that is not corpus work. Changing it is a post-calibration
protocol amendment for the next cycle, stated as such, never applied backwards
to this one.

## 6. Next step: a corpus-eligibility audit, and neither candidate chosen

Selecting the extension corpus is preparation for the permitted action, not a
new evidentiary test — §4.3 itself contemplates memory-plane findings "once
their atomic units are reconstructed and frozen".

**Own.NET no longer holds its earlier priority.** It does carry
reviewer-recorded findings — `docs/notes/architecture-review-2026-07.md`, R1–R6
as `### R<n>.` sections — but there are six of them in that file and the unit is
coarser: architectural themes ("the DI subsystem sits outside the general
discipline") rather than specific defects of the grain "`ArtifactRef` did not
say which of two byte strings it identified". Comparing `x/6` against `20/41`
would parody the reason the unit of observation was frozen at all, and it is the
same commit-unit-versus-finding-unit problem already refused for the §5 corpus.
Its other numbered lists are remediation plans and design rules, not findings.

So neither Own.NET nor memory-plane is chosen. The next step is an audit that
does **not** read the semantics of any candidate finding, establishing per
candidate:

- does a natural, independently recorded unit at the grain of "one specific
  reviewer finding" exist;
- how many such units exist;
- can they be frozen without retrospective splitting or merging;
- how comparable is the recording process's provenance to the first corpus.

If Own.NET needs six themes decomposed after the fact, it likely loses. If
memory-plane permits atomic records to be recovered along boundaries that
already existed, it may be cleaner despite the extra work.

**Recommended for the next contract, not applied to this result:** require a
concrete denotation collision — two distinguishable referents that the system
makes indistinguishable at a verdict-relevant point through one identifier,
field, slot, reference, grain designation or binding — and exclude the bare fact
that one function, type or algorithm serves cases with differing preconditions
or available inputs. Direction of effect on 48.8% is downward or unchanged;
magnitude is not derivable from the frozen outputs and is not guessed here.

## 7. Engineering state

**INCONCLUSIVE**, scoped: shared H1 discipline only, versus an H2 shared
observation-binding record. Grounds, in order:

1. H2's structural requirement has not been tested at all.
2. `R_broad` construct width is in serious question — Control B and
   `compound_n = 0` together.
3. This corpus yielded no independent external positive evidence.
4. No independent corpus exists yet.
5. The cost-derived decision threshold has not been frozen, and must be before
   the next corpus is opened.

The line is not dead. Twenty positives, five `R_obs`, twenty-one genuine
negatives and one held negative control are enough structure that continuing is
not hunting signal in noise. The next task is not to confirm an attractive half;
it is to see whether a useful class survives a stricter causal boundary and an
independent unit of observation.

## 8. What we predicted, and what happened

```text
Pre-run expectation (both participants):   H1
Calibration outcome:                       not adjudicated
```

Not "H1 looked right". Not "mostly H1". Not "H1, with H2 worth exploring". The
frozen design set no threshold for material versus low, and H2 carries an
unmeasured boundary condition besides. The bet was not tested.

This is recorded because "the result did not refute my preferred hypothesis"
converts into "so I was right after all" with almost no friction, and no gate in
this project catches that one.
