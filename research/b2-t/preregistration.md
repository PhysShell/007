# B2-T — observation/identity decomposition: preregistration

```text
STATUS:  READ-ONLY R&D / NON-AUTHORITATIVE
         MUST NOT DRIVE A-SERIES TRANSITIONS
         DRAFT — NOT YET FROZEN (see §0)
```

## 0. What this document is, and when it becomes binding

This is the preregistration for B2-T. It fixes the question, the corpus, the
classification contract and the analysis protocol **before** the data is read,
so that the result cannot be reinterpreted after it is known.

It is a **draft until the maintainer records the freeze** in §9. Everything
below is written to be frozen; nothing below is authoritative while §9 is empty.
Once frozen, changes go through a new revision that says what changed and why —
never a silent edit.

**Why the hurry.** The option this document preserves decays. A taxonomy written
after reading the corpus it will be applied to is not a taxonomy, and a selection
rule chosen after seeing what it selects is not a selection rule. Everything in
§§1–5 costs nothing today and cannot be recovered once anyone reads the
material in §5.

## 1. The question

Repeatedly over the preceding weeks, a defect in this project turned out to be
one of two things called by one name. The hypothesis is that this is a *class*,
frequent enough to be worth catching structurally rather than by attention.
Deliberately no count here: measuring that share is what this document is for,
and a motivating number with no selector or provenance is the failure mode under
study.

The question is deliberately **not** "can we express our mechanisms in a common
schema" — that question invites a schema. It is:

> What share of independently discovered, verdict-relevant defects is best
> explained by identity conflation, and what share of *that* share could a
> concrete carrier / observation / support-relation decomposition actually have
> prevented?

### Hypotheses

| | claim | what would establish it |
|---|---|---|
| **H0** | No reusable decomposition. Keep bespoke models. | both rates low |
| **H1** | Shared conceptual / lint discipline. No transferred record. | `R_broad` material, `R_obs` low |
| **H2** | Shared observation-binding wire schema. | `R_obs` material **and** the same semantic record must cross a process/domain boundary |
| **H3** | Authority-level representation. | only if that record participates in A1 admission / replay / transition semantics |

H2 is not established by "several codebases apply a similar principle" — that is
H1 with three implementations. H2 requires that one record be *produced* by one
authority/domain boundary and *consumed* by another.

**The calibration in §2 does not adjudicate this table.** The words "low" and
"material" above have no frozen threshold, and inventing one today would be as
arbitrary as inventing one after the result. This table describes the outcome
space of the research line; §4.3 says what calibration is actually allowed to
produce, and it is not a verdict.

## 2. Calibration corpus

**`docs/q-deck/a1-authority-contracts.md` §9 — the A1-F corrective rounds.**

Unit of analysis: **one numbered finding, exactly as the reviewer recorded it.**
Not a commit, not a round, not a symptom. No retrospective splitting of a
multi-symptom finding and no merging of related ones.

Counted directly from §9:

```
R1    6      R5    6
R2    6      R5.1  5
R3    7      R5.2  4
R4    7           ──
                  41
```

**N = 41.**

### 2.1 This corpus is calibration, not confirmation

The categories in §3 were formulated *while reading this material*. Blinding the
classifier fixes rater bias; it does not fix the fact that the instrument was
shaped by the object it will measure. So this corpus can estimate how common the
pattern is among independently recorded findings. It **cannot** serve as
independent confirmation that the pattern exists, and no result from it may be
reported as a confirmed prediction.

Two findings are additionally contaminated by direct prior discussion —
**R5.2 #2** (`producer_execution_id` had no defined grain for human artifacts)
and **R5.2 #3** (`ArtifactRef` did not say which of two byte strings it named).
Both were examined at length before the taxonomy was written. They remain in the
denominator; the classifier must not be one of the people who discussed them.

### 2.2 Estimand

The measured quantity is:

> the share of **identity-decomposition / observation-binding root causes among
> defects discovered and recorded by this review process**.

It is not the share of such defects in the codebase. A conflation that never
produced a visible symptom was never recorded and is not in the denominator.
For the decision "is this class worth catching mechanically" the detected
distribution is the relevant one — but it must not later be read as a property
of the code.

## 3. Frozen classification contract

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

## 4. Analysis protocol

1. **Blinded classification.** The classifier receives the 41 findings *verbatim*
   — not paraphrased, since a paraphraser can help the hypothesis without
   noticing — in shuffled order, with no round labels, no reviewer provenance,
   no chronology, no hypothesis, and no access to the discussion that produced
   this document.
2. **Unblind metadata after classification.** Report the internal (R1–R5.1,
   n=37) vs external (R5.2, n=4) split as a **descriptive stratification only**
   — n=4 cannot support a detection-rate claim, and must not be reported as one.

   N=41 is the size of the corpus, **not automatically the denominator of a
   point estimate.** Both denominators are fixed here, before any of `yes`,
   `compound_n` or `unclear_n` exists:

   ```
   point estimate       R_broad = r_broad_yes / classified_n
                        R_obs   = r_obs_yes   / classified_n

   sensitivity interval over the full population, per rate:
     lower bound  =  yes                           / 41
     upper bound  = (yes + compound_n + unclear_n) / 41
   ```

   The point estimate is never published alone: every rate appears with its
   interval and with `classified_n / 41`, `compound_n`, `unclear_n`. Dropping
   the unresolved rows is the cheapest route to a convenient number, and the
   interval is what makes that visible.
### 4.3 What calibration is allowed to produce

**Calibration issues no H0/H1/H2 verdict.** It answers one question — *what is
the base rate under the frozen definitions?* — and asking it to also adjudicate
§1 would require a threshold that does not exist, which is exactly the gap
through which "material" gets defined after the number is known.

Frozen outputs, and nothing else:

```
classified_n / 41 , compound_n , unclear_n
R_broad  point estimate + full-population sensitivity interval
R_obs    point estimate + full-population sensitivity interval
reviewer-provenance split, descriptive only (internal n=37 / external n=4)
```

Any reading of those numbers against §1 is **post-calibration interpretation**
and must be labelled as such. It may never be reported as preregistered
confirmation, and §2.1 already bars this corpus from confirming anything.

### Inconclusive, defined before results

**INCONCLUSIVE is a state of the engineering decision, not a statistical
label.** It obtains when the observed result leaves a concrete choice — build
shared machinery, or don't — genuinely unresolved.

If it obtains, the only permitted next action is corpus extension: memory-plane
findings once their atomic units are reconstructed and frozen, or Own.NET review
records. And before that next corpus is opened, **the specific decision and its
criterion must be written down** — a threshold justified by the cost of the
machinery under consideration, not by the aesthetics of a percentage.

If the result is plainly uninteresting, the line dies here without ceremony.
That is a permitted and expected ending.

## 5. Preserved option: the A1-V0 corpus (PR #124)

PR #124 merged on 2026-08-10 (head `0c3e6c9`, now an ancestor of `main`). It is
**not** a prospective corpus: the taxonomy was not frozen when it was written,
and its material had already been in view. It is a **secondary implementation
corpus**, held closed.

**Eligibility rule, frozen here, before any classification of its contents:**

> Non-merge commits reachable from `0c3e6c9` but not from `9e303b9`
> (the branch merge-base) whose subject line begins with `A1-V0:`.

Mechanically evaluable from subject lines alone: **17** of the 26 non-merge
commits in the PR.

**What this freeze does and does not buy.** The rule is frozen before any
classification, but **not** before its authors saw the commit subjects — it was
written by reading them. Freezing prevents further selection drift; it does not
make #124 a blinded holdout, and a later blinded corpus-builder cannot undo it.
Therefore:

> **#124 is secondary retrospective robustness evidence only.** It may never be
> promoted to independent confirmation, in any later revision, on any result.

Constraints on any future use:

- Unit is **one corrective commit**, not a finding. This is a *different
  estimand* from §2 and its numerator and denominator may never be pooled with
  N=41.
- A commit closing two genuinely distinct root causes is recorded as `compound`
  with `contains_broad_identity` / `contains_observation_binding` flags, rather
  than being forced to one primary cause. Authors' habits in grouping fixes must
  not become a statistical property of defects.
- Corpus-builder and classifier are **different people/agents**. The builder
  applies the eligibility rule above without knowing the hypothesis; otherwise
  selection bias merely relocates into "is this commit corrective?".
- **Known contamination — both parties.** Both participants in the discussion
  that produced this document have already seen these commits' subject lines:
  the assistant that drafted it, and the maintainer who reviewed the draft.
  Neither may be the corpus-builder or the classifier for this corpus. (For §2
  the assistant is disqualified on the separate ground that it helped form the
  taxonomy.)

No commitment is made to analyse it at all. Freezing the rule is free today and
impossible tomorrow; running the analysis is a separate decision.

## 6. Stop rule for prospective instrumentation

> No prospective instrumentation is introduced unless calibration leaves a live
> decision that additional independent evidence can change.

A standing obligation to record every verdict-relevant defect, with roles,
metadata and drift diagnostics, is a permanent tax on a project with one
operator and several open tracks. It is the *conditional* consequence of an
INCONCLUSIVE or live result, not a component of T0.

If it is ever introduced, it carries a bias the retrospective corpora do not
have. A1-F §9 was written by people who did not know it would be counted; a
prospective corpus is generated by a process that knows. The expected direction
is that small defects go unrecorded ("faster to just fix it") and the sample
drifts toward large ones — which is exactly where identity conflations are
hypothesised to live, so `R_broad` would rise with nobody doing anything wrong.
Countermeasures, to be frozen at that point and not before:

- eligibility by verdict-relevance only — never by size, severity, or repair
  time;
- the recording obligation admits no "too small" exception;
- size/friction proxies recorded per finding (`fix_commit_count`,
  `files_changed`, `lines_added`/`lines_deleted`, `time_to_first_fix`) — used
  **only** post hoc to detect drift, never in eligibility, and never treated as
  a true magnitude;
- comparison with A1-F restricted to proxies reconstructible in both corpora
  the same way. Available metadata is not comparable measurement.
- `discovery_source` (`internal_review` / `external_review` /
  `implementation_feedback` / `ci_runtime` / `contract_change`) recorded
  separately from `work_surface`. An implementation PR does not make its
  findings implementation feedback.

## 7. What is deliberately not frozen

- Any percentage threshold.
- Any name for a candidate representation. `ObservationBinding`,
  `carrier` / `observation` / `support_unit` are working words in this document
  and claim no status.
- Any commitment to build anything. H2 and H3 are hypotheses about whether code
  would be justified, not plans.
- The independence model. Independence is a **relation** between observations
  (`duplicate_of`, `variant_of`, `derived_from`, `same_support_unit_as`), not a
  field on one; the coordinate-ribbon case (nine `timer-stop-*` corpus cases,
  distinct carriers, distinct observations, one semantic support unit) is why a
  binary id would already be wrong.

## 8. Domain negative control

**FD-1.2 framing** — length-prefixed field framing with domain separators — is
recorded here, before any schema draft, as the domain negative. It is
verdict-determining and is *not* an observation-binding problem. If a future
candidate representation "explains" it elegantly, the representation has grown
into a universal solvent and is refuted by that fact.

Selection negatives, also recorded before classification — real defects from the
same period whose primary cause is not identity conflation:

- **R5.1 #2** — `CANCEL` could not pass its own event guard (two rules that
  cannot both hold).
- **R5.1 #4** — the genesis signature contradicted the genesis prose
  (`resolve_event` required a policy obtainable only by work it had not done).

Deliberately **not** used as a selection negative: R5.2 #3, the closure-budget
undercount. On inspection its primary cause is that `ArtifactRef` did not say
which of two byte strings it identified — carrier identity — with the budget
undercount as the operational consequence. It belongs in the positives.

## 9. Freeze record

The freeze is a **two-commit ceremony**, because a commit cannot contain its own
hash:

```text
C   the last commit that touches §§0-8      — the frozen content
F   the next commit, touching §9 only       — the human freeze record
```

`F` does not name itself. Its existence in history, and the fact that it changes
nothing but this section relative to `C`, is the provenance of the freeze.

```text
FROZEN CONTENT REVISION:   153684fd27e7d55992015cc33cb03ec23df03c1d
FROZEN BY:                 PhysShell <mouse.kcsource@gmail.com>
DATE:                      2026-08-10
```

**How this record was made, stated plainly.** The maintainer approved the freeze
explicitly in session and instructed the assistant to write this block; the
assistant authored the commit. So the approving act is the maintainer's and the
transcription is not — an auditor should read `FROZEN BY` as the human who took
responsibility, and the commit's git author as who typed it. These are different
facts and this document is the wrong place to blur them.

**Status: FROZEN.** §§0–8 are binding as of `153684f`. Changes go through a new
revision that says what changed and why, never a silent edit — and any change
made after results are known must say so in the same breath.

Classification of §2 and inspection of §5 may now begin, under §4 and under the
role separation in §2.1 and §5.

### Verifying this freeze

The claim is not "F is a small diff" but "F left §§0–8 byte-identical". Those are
different assertions, and only the third check below establishes the second one:

```bash
set -euo pipefail

C=153684fd27e7d55992015cc33cb03ec23df03c1d
F=<this commit>

test "$(git rev-parse "${F}^")" = "$C"                       # F descends from C
test "$(git diff --name-only "$C" "$F")" = \
  "research/b2-t/preregistration.md"                          # one file only
diff \
  <(git show "$C":research/b2-t/preregistration.md | sed '/^## 9\./,$d') \
  <(git show "$F":research/b2-t/preregistration.md | sed '/^## 9\./,$d')
                                                              # §§0-8 identical
```

Exit 0 on all three, or the freeze does not hold.
