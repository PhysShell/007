# API migration assurance direction (AMA-0)

Status: **accepted direction, exploration gated at the corpus gate** · Track:
**AMA** (this note is AMA-0) · Scope: **product model, asset accounting, and a
pre-registered evaluation protocol — not an implementation contract**.

Implementation gate: **nothing beyond a corpus and an evaluation protocol.** The
next AMA step is permitted to build an adjudicated historical migration corpus,
a differential ground-truth harness, and the baseline comparison. It is **not**
permitted to build the analyzer as a product, a GitHub App, or a service. That
decision belongs to the corpus gate in §6, which can also end the track.

Ratification: adjudicated in an interactive session with the maintainer, so per
rule 3's carve-out in `docs/evidence-and-decision-discipline.md` this is
**ratified, not pending**. Like ABR-0, it is a frozen direction record: it exists
so later work does not re-derive the forks, and does not silently restore the
weaker forms.

## 1. The four layers, and why conflating them was the original error

The direction started from an appraisal that treated 007's existing evidence
machinery as "half the solution" to verified API migrations. That arithmetic was
wrong, because four separable layers were being added together:

```text
1. execution provability   — the run happened, gates really ran, the record is
                             tamper-evident and independently replayable
2. coverage provability    — the set of affected call sites is complete within a
                             declared region, and the region's outside is named
3. patch generation        — the code change itself
4. delivery infrastructure — multi-tenant runtime, App install, billing, custody
```

007 has layer 1 (§5). Layer 3 is commoditising and is not a differentiator.
Layer 4 is a separate product runtime, not an adapter over 007. **Layer 2 does
not exist in this repository in any form, and it is the entire research risk.**

Possessing layer 1 does not make layer 2 half-built. It makes layer 2's
*eventual output* recordable. Those are different sentences.

The same care applies to methodology: QODEC's development rigour is not an
importable dependency. There is no `qodec-rigor = "1.0"`, and adding one would
not yield soundness.

## 2. Accepted product model

- **The product is bounded coverage assurance for external API migrations.** The
  deliverable sold is an *assurance report*. PR authorship is a replaceable
  downstream mechanism and may be performed by any coding agent or by a human.
- **Completeness is claimed only inside an explicitly declared analyzable
  universe.** "We find everything" is not a claim this track is permitted to
  make, in a report or in marketing. The claim shape is fixed:

  ```text
  Universe U:  declared, machine-checkable membership conditions
  Claim:       within U, the affected-site inventory is complete
  Outside U:   the opaque set, enumerated, never summarised as "clean"
  ```

- **Three assertion levels, not three risk buckets.** `Resolved` (statically
  resolved inside U) · `Evidence-bounded` (supported by named evidence — types,
  imports, traces — falling short of resolution) · `Opaque` (completeness not
  establishable by this analyzer). The opaque set is a first-class part of the
  contract, printed at the top, not an appendix.
- **"No trace found" is never "proven unaffected."** The phrase "provably
  unaffected" is retired from this track.

This is rule 4 applied to a product surface: what the analysis *says*, what is
*inferred* from it, and what is *decided* on top must stay separated in the
customer-facing artifact too.

## 3. The value surface is type-invisible change

**Artifact says** (TypeScript compiler behaviour, a stable documented property
of `tsc`; re-verify empirically against the pinned toolchain at corpus
construction): removing or renaming a member of a typed declaration surface
makes every statically resolvable use of that member a type error.

**Inference:** for the resolvable part of any declared universe, type-*visible*
breaking changes are already enumerated completely, soundly, and at zero
marginal cost by the consumer's own compiler. A product that locates them sells
what `tsc` gives away.

**Decision:** the AMA value surface is **breaking changes that do not produce a
type error**. Representative classes, to be sampled deliberately in the corpus:

```text
semantics of a field changed behind an unchanged signature
enum / literal value added, removed, or re-meaninged
default value or implicit behaviour changed
pagination, ordering, or idempotency semantics changed
deprecation without removal from the type surface
required → optional (and the reverse, where inference hides it)
error codes, retry, and rate-limit semantics
behaviour bound to a pinned API version rather than to a symbol
string-keyed payload internals: expand lists, filter and field names,
  resource/operation strings — invisible to the type system by construction
```

Two consequences that bind the work downstream:

- **Corpus filtering.** A historical case where the compiler alone catches every
  affected site is a *negative* case: evidence that no product is needed there.
  Such cases are collected and counted separately, never used to report recall.
  Reporting them as successes would produce excellent, meaningless numbers.
- **Granularity.** The inventory unit is not the call site but *(symbol, path to
  the affected fact)* — field- and argument-level. A field-level change measured
  at method granularity simultaneously over- and under-counts, so recall would
  be measured against the wrong denominator.

## 4. Evaluation protocol (pre-registered)

Per rule 4's discipline, thresholds are fixed **before** measurement. A
threshold chosen after seeing the analyzer's score is not a threshold; it is
retrospective self-approval with a table.

### 4.1 Two ground-truth sources, deliberately different

**Differential ground truth (machine-checked, cheap).** At a pinned repository
commit and pinned toolchain, mutate the SDK type surface (delete or rename a
member); the resulting compiler diagnostic set *is* the ground truth for the
resolvable region. No human adjudication. It also empirically traces the
boundary of U and measures covered-surface change per commit.

Validity limit, stated up front: this oracle is only valid for type-visible
mutations, so it measures the **locator**, not the semantic-change detector. It
is exactly and only an oracle for `resolution recall`.

**Adjudicated historical corpus (expensive, irreplaceable).** Needed for what no
compiler yields: global recall and opaque discovery on real, type-invisible
changes. A historical migration commit is **not** ground truth by itself — the
developer may have missed a site, fixed it later, deleted the feature, refactored
in the same commit, migrated a wrapper instead of the call sites, or covered
paths no test exercises. Adjudication combines historical evidence, before/after
static analysis, release notes, manual review, and where available test or trace
validation. Building an assurance product on an unadjudicated benchmark would
make the benchmark itself a false-confidence generator.

Per-case record: API/SDK identity · old and new version · breaking-change
declaration and its class per §3 · repository before/after · migration commits ·
adjudicated affected sites · expected opaque regions · negative examples ·
language and toolchain versions · resolution assumptions · known limitations ·
adjudication provenance.

### 4.2 Metrics

```text
resolution recall        ground-truth sites found, over ground-truth sites
                         inside the declared universe          (differential oracle)
global discovery recall  over all known affected sites, opaque patterns included
opaque detection recall  known-opaque regions correctly declared opaque rather
                         than silently reported clean
false assurance rate     runs claiming coverage while a site inside the declared
                         universe was missed
covered-surface ratio    size of the region carrying the strong guarantee
analysis cost            per report, at realistic repository size
```

`false assurance rate` is the governing metric: a spurious warning annoys, a
false assurance report causes an incident. `covered-surface ratio` is its
mandatory pair — a perfect false-assurance rate is trivially obtained by
declaring the whole repository opaque, which is flawless and worthless.

### 4.3 Baselines, in their strongest form

Compared on every corpus case: the specialised analyzer · a coding-agent
baseline (Claude Code / Codex) · a grep/AST baseline · the compiler-only
baseline of §3.

The agent baseline must be given its **best** form — explicitly prompted to
enumerate what it may have missed and why — because comparing recall alone is
comparing against a strawman: an LLM baseline with no opaque set has an
unbounded false assurance rate at *any* recall. The comparison is on the pair
(recall, calibration).

## 5. Asset accounting

Rule 4, applied to our own inventory. "Direction only" and "idea" are not
partial implementations.

| Asset | Real status | Role in AMA |
|---|---|---|
| Worktree isolation | Built | Executing migrations |
| Gate reduction, fail-closed verdicts | Built | Post-change verification |
| Digest-chained event ledger | Built | Audit evidence |
| `o7 replay` | Built | Independent re-verification of the record |
| Live ingress / REST + SSE (`o7d`) | Partly built | Runtime observation |
| Call-site coverage engine | **Absent** | The product risk |
| Historical migration corpus | **Absent** | The first required artifact |
| Capability / action broker | Direction only (ABR-0) | Not counted as implementation |
| Agentopedia / documentation drift | Idea | Not counted as an asset |
| Multi-tenant SaaS runtime | Absent | A separate future project |
| Automatic PR generation | Commodity | Not a differentiator |

007's built assets are, in AMA terms, a shell for *executing and recording* a
migration — reusable later, and irrelevant to whether the coverage engine works.

## 6. First decision gate (the corpus gate)

Permitted before the gate: the adjudicated corpus, the differential harness, the
baseline runs, the metric report. Nothing else.

The gate decides whether the product exists, on the pre-registered metrics of
§4.2 evaluated against the thresholds fixed when the corpus protocol is frozen —
before any analyzer result is seen.

### Kill criteria

The track ends if any of these holds; existing effort is not an argument
against them, and citing it is the failure mode these criteria exist to stop:

```text
- false assurance rate cannot be driven to the pre-registered threshold
- high completeness holds only on an artificially narrow universe
  (covered-surface ratio below its pre-registered floor)
- the opaque set swallows the majority of real integrations
- a strongest-form coding-agent baseline matches both the recall and the
  calibration of the specialised analyzer  (then this is a prompt, not a product)
- adjudication + analysis cost exceeds the value of the misses prevented
- not enough high-quality historical cases are obtainable
```

## 7. Explicitly deferred

```text
GitHub App                        multi-language support
multi-tenant SaaS runtime         raw fetch / dynamic URL analysis
automatic PR authoring            wrapper and DI-indirection analysis
provider→customer distribution    compliance productisation
capability-broker integration     daily PR-time coverage-regression signal
```

Two of these carry their own recorded reasoning, so a later reader does not
mistake deferral for absence of thought:

- **Daily signal.** A blast-radius bot that fires once a quarter has no
  retention. The durable form is a PR-time *coverage regression* signal —
  reporting that a change converted resolved sites into opaque ones (dynamic
  dispatch introduced, a covered wrapper bypassed, the SDK version moved without
  an assurance run). That measures the degradation of a codebase's own
  analyzability, which is a stronger idea than API linting, and it is still
  deferred: it is meaningless before the analyzer's numbers exist.
- **Compliance.** Not a vertical. It is the *packaging* of the assurance report
  for change management: initiator, analyzed region, versions compared, findings,
  changes made, checks run, what stayed opaque, who accepted the residual risk,
  and the evidence digest bound to that acceptance. 007's ledger and `o7 replay`
  already produce the substrate; no separate compliance platform is implied.

## 8. Non-goals

No claim of completeness over arbitrary code. No "provably unaffected" verdict.
No universal API-change platform. No analyzer implementation before the corpus
gate. No product runtime built on subscription-auth CLIs (AMA's delivery layer,
if it ever exists, has a different trust model than 007's local operator).

## 9. Liability boundary (recorded, not solved)

A provider-authored bot opening PRs in customer repositories creates a
responsibility chain — recommendation, authored code, approved merge, production
failure — that an evidence pack can document but cannot allocate. Internal
platform teams are the correct first buyer for three reasons that are not about
distribution: one owner for both the API and its consumers, access to every
repository and CI, and liability contained inside one organisation. They also
make adjudication tractable, since they can confirm which services should have
migrated.

## 10. Unfreeze triggers

```text
analyzer implementation      after the corpus gate passes on pre-registered metrics
wrapper / DI analysis        after the corpus shows resolved-only coverage is too small
PR-time regression signal    after the analyzer has published numbers
delivery runtime             after a paying internal-platform design partner exists
provider-side distribution   after the liability boundary of §9 is contractually settled
```

Nothing is built now beyond the corpus and the evaluation protocol. This note
exists so the narrow wedge does not mutate, three commits later, into a SaaS, a
GitHub App, a cloud runtime, and an insurance policy.
