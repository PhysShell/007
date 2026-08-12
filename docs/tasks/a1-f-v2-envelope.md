# A1-F v2 — Envelope v2

**Status: DRAFTING (revision E-R15) — DRAFT PR OPEN FOR MECHANISM REVIEW.**

E-R0 established the ledger and its gate and recorded **E-1**. E-R1 repaired the
five P1s that followed — including E-1's false derivability claim. E-R2 closed
the trust joint itself: the middle term is now extracted rather than asserted,
per-field capability replaces a bare path list, the meta-target obeys frozen
clause 5, and a `--graph` override can no longer bypass its own preflight. E-R3
makes step 2 count rather than deduplicate, and rebases onto current `main`.
E-R4 answers §8.5: moving `PINNED_BLOB` is now a ceremony that consumes an
already-authorized Phase G supersede, and a pin failure never authorizes its own
repair. E-R5 scopes two of E-R4's claims to what their witness actually
observes. **E-R6 repairs the first defect found by a genuinely independent
reviewer** — a full green over a realization carrying none of the eleven
structural digests, plus three Majors from CodeRabbit including `assert`-guarded
checks that vanish under `PYTHONOPTIMIZE`. **E-R7 closes the same wound one
level deeper**: E-R6 bound the requirement's sources to the frozen map and left
its targets free. **E-R8 binds the last unchecked column of a structural row,
its path.** E-R9 names the family all three belong to and pins one latent
instance of it; E-R10 turns that into a design criterion and moves the pin to
the layer it belongs in. **E-R11 closes the family's other two species**, both
found from outside after the taxonomy predicted them. **E-R12 audits the
untouched steps against the criterion and finds the same species on the schema
term.** E-R13 names the mechanism that produces all three species and turns the
audit into a refutation surface. **E-R14 separates `ERROR` from `FAIL`**, the
same demotion surviving in the verdict vocabulary; **E-R15 finds the criterion
holds end to end, on both sides of the checking logic**. Sixteen revisions, no
graph change in any of them: the frozen registry is a hard input here, not a
subject.

**The gate is intentionally RED**, because Envelope v2 has drafted no schema.
What is up for review is the proof machinery, not the completion of the phase.

**E-1 is settled as of E-R1** and is not reopened below. Everything since has
been about the machine that decides when the gate may go green.

## 0. Inputs and authority

```yaml
base:
  commit: 6301881                      # merge of PR #128 into main
  role:   the branch point for this phase

hard_input:
  document: docs/tasks/a1-f-v2-phase-g.md   # as merged on main
  decision: FD-v2-GRAPH — APPROVED / CLOSED
  role:     EXTERNAL IMMUTABLE INPUT. Not a neighbouring file to be adjusted.
  extract:  docs/tasks/a1-f-v2-graph.json   # generated from it; do not hand-edit

current_v1_authority:
  document: docs/q-deck/a1-authority-contracts.md
  blob:     3b26849cc39a3391aaed46cca56be3b6715afabb
  rounds:   R1..R5.2, then S1 (FD-1.4 only)

design_input:
  ref:    refs/tags/a1-f-v2-phase-g-design-input-v1
  target: 37502e3ce5c397a7437445aafb88c13d84ba4ac0
  role:   EVIDENCE. Never authority.
```

The design input is now reachable through a preservation ref rather than a
deletable branch, so every citation in this phase is reproducible from a clone
of `main` plus that tag. That was not true while Phase G was being written, and
it is the one provenance defect the post-closure review caught.

## 1. Mandate, and what is not local drafting

```text
SCOPE
  Envelope v2 wire / schema / digest / version decisions
  + the realization ledger, from commit 1

FORBIDDEN as a local drafting choice — each REOPENS or SUPERSEDES Phase G:
  a new semantic node
  a new semantic edge
  removal of a frozen edge
  a twenty-second event kind

DISPOSITIONS
  close only rows a concrete Envelope-v2 decision consumes.
  never bulk-close the 47 + 141.
```

The forbidden list is not advice. Each of its four items is a check the gate of
§2 performs mechanically, so a drafting session cannot reach any of them by
accident — only by deciding to, on the record.

## 2. The realization ledger, and the gate that makes it binding

Phase G §4.2.6 made one machine-checked wire realization ledger a **required
acceptance artefact** of this phase, because the joint between graph authority
and wire schema was the last place a relation could still be invented by a human
reading a document.

### 2.1 Three independent terms, each derived rather than asserted

E-R0 read every left-hand side out of the **ledger**, so a ledger could declare a
world and be congratulated for matching it. E-R1 split the terms apart. E-R2
finishes the job, because a split into three files is worth nothing if two of
them are still hand-written:

```text
FROZEN GRAPH    docs/tasks/a1-f-v2-graph.json
  authority     tools/a1_v2_extract_graph.py --check, source pinned at blob
                450380ff. Message kinds, typed supports and the meta-target
                members are now PARSED from the document, not transcribed.
        |
        v
SCHEMA FACTS    docs/tasks/a1-f-v2-schema-facts.json
  what v2 IS    tools/a1_v2_extract_schema.py --check. It exists now, while
                there is no v2 schema, precisely so the trust model is fixed
                before there is anything to be tempted about.
        |
        v
REALIZATION     docs/tasks/a1-f-v2-realization-ledger.json
  what v2 CLAIMS  one row per carrier, and nothing else
```

E-R1 decided a schema was "drafted" from `bool(extracted_from)` and never looked
at `extractor` — so tomorrow's drafter could write `"extracted_from": "whatever"`
and hand-fill three arrays. That is the same guarantee-by-comment the ledger's
`_note` fields were in E-R0, moved one floor up. Both preflights now run
`--check` against a real extractor, and the corpus proves a hand-filled facts
file is rejected.

### 2.2 What each step actually compares

```text
0. observations well-formed  (BEFORE any comparison; see 2.6)
1. schema.event_kinds        == graph.event_kinds
2. schema.payload_presence   == graph.expected_payload
   AND schema.structural_commitments == the 11 payload-bearing kinds OF THAT MAP
   AND declared structural carriers, keyed by (source, TARGET) == exactly 1 for
       each frozen (CampaignEvent(k), P) pair, and exactly 0 everywhere else
   AND each structural row's PATH == the path the schema commits for that kind
3. per FIELD: source kind and COMPLETE target domain == its carriers'
4. every (source, target, class) carrier is a frozen edge, exactly
5. every frozen (source, target, class) has >= 1 carrier, exactly
```

**Step 3 is per-field capability, not a path list.** Phase G §4.2.6 clause 1
requires a field to declare the complete set of edges it may realize, and a set
of paths cannot express that. The attack it lets through is not exotic:

```text
WorkOrder.input.candidate_ref        actual target  CandidateStateReceiptRef
WorkOrder.input.materialization_ref  actual target  CandidateMaterializationRef

swap which path carries which in the ledger:
  both frozen triples still exist somewhere      -> steps 4 and 5 PASS
  each field realizes the wrong relation         -> nothing notices
```

Schema facts therefore carry `{path, source_semantic_kind,
allowed_concrete_target_kinds}`, and step 3 compares domains per path. The
corpus case is *two fields swap their targets*.

**Steps 4 and 5 are exact, because clause 5 says so.** Frozen: *"AnyCommittedEnvelope
is declared ONCE as a meta-target expansion, never as eleven separate edges."*
E-R1 violated this in its own corpus — it expanded row 69 into eleven carriers
and asserted that keeping only `CampaignFeedItem → WorkOrder` was a faithful
realization, which silently narrows a union of eleven kinds to one. Now the
carrier's target *is* `AnyCommittedEnvelope`, matched exactly; the expansion
appears only in step 3, where the field's permitted domain must equal all eleven
members. Two corpus cases pin it: *meta-target narrowed to one member* and
*meta-target realized as eleven separate carriers*.

### 2.3 The preflight binds to the path it will read

E-R1's preflight ran `extract --check` against the **default** artifact and the
gate then read `args.graph`, so `--graph /tmp/forged.json` produced a PASS
followed by five steps measured against the forgery. That is a literal bypass of
the protection E-R1 had just added. Each preflight now passes `--out` the same
path the gate reads.

`--skip-preflight` is gone as a documented flag, on the reasoning that a
documented skip is an invitation: within a year somebody adds it to CI to make
the checks faster, which is the customary way of optimising a check by deleting
it. E-R3 moved it into an environment variable, which is the same invitation
with worse ergonomics — **E-R6 removed it entirely.** Steps 1–5 are now an
importable `run_steps()`, the corpus calls that, and the shipped executable
contains no bypass of any kind.

### 2.4 It fails closed, and the corpus is executable

With nothing drafted, both preflights pass and all five steps are OWED:

```text
  preflight: graph extract              PASS   committed graph == derived from Phase G
  preflight: schema extract             PASS   committed facts == derived from the v2 schema source
  1. event-kind universe equality       OWED   no v2 schema extracted; frozen set has 21
  2. payload presence map               OWED   no v2 schema extracted; map requires 11 one / 10 zero
  3. per-field ArtifactRef capability   OWED   no v2 schema extracted; nothing to extract fields from
  4. forward carrier coverage           OWED   no carriers declared
  5. reverse carrier coverage           OWED   69/69 admitted edges have no wire carrier
RESULT: FAIL — Envelope v2 is not realized
```

`tools/tests/test_a1_v2_ledger_gate.py` — **31 cases, 31 as specified.** Twenty-two
mutate one thing and assert which step catches it; six exercise the preflight
itself, which E-R1 added and left undefended; two exercise the pin evidence of
§2.5. The corpus **imports `run_steps`**; it no longer asks the executable to
disarm.

| Mutation | Caught by |
|---|---|
| faithful realization of all 69 edges | nothing — exit 0 |
| nothing declared at all | all five OWED |
| schema grows a 22nd event kind / drops one | step 1 |
| **schema grows a payload on a payload-free kind, no ledger row** | **step 2** |
| **two fields swap their targets, globals still balance** | **step 3** |
| schema field with no carrier / carrier the extractor never saw | step 3 |
| **meta-target narrowed to one member** | **step 3** |
| **meta-target realized as eleven separate carriers** | **steps 4 and 5** |
| carrier for a relation the registry does not hold | step 4 |
| frozen `Intra` edge realized as `Causal` | steps 4 and 5 |
| an admitted edge left with no carrier | step 5 |
| **forged graph passed via `--graph`** | **preflight, no steps run** |
| **hand-filled schema facts** | **preflight** |
| **duplicate structural carrier for one event kind** | **step 2** |
| **Phase G source blob mismatch** | **extractor `--check`** |
| **schema commits no structural digests at all** | **step 2** |
| **payload edges relabelled as `artifact_ref` carriers** | **step 2** |
| **payload digest moved onto a sibling edge of the same source** | **step 2** |
| **structural carrier path the schema never declared** | **step 2** |
| **carrier declaring an invented third role** | **well-formedness** |
| **schema declaring one event kind twice** | **well-formedness** |
| **two contradictory schema facts for one field path** | **well-formedness** |
| **the same carrier occurrence declared twice** | **well-formedness** |
| **`PINNED_BLOB` edited with no evidence** | **extractor, at extraction** |
| **frozen checks under `PYTHONOPTIMIZE=1`** | **extractor, explicit raise** |
| **the retired harness env var** | **inert — preflight runs anyway** |
| **pin evidence that binds nothing** | **extractor, at extraction** |
| real artifacts | both preflights pass |

Every emphasised row is a defect a previous revision of this gate could not
catch while claiming to.

**What this number is, precisely.** The corpus is a **contract-relative
regression witness**. It establishes stability against known cases *under the
currently accepted oracle*. It neither establishes the oracle's adequacy nor
supplies independent evidence for the semantic correctness of its expected
verdicts. A shared authoring model can cause the corpus to **ratify the same
defect it is intended to guard against** — which has now happened three times
here (§7, E-R15). Read `31/31` as *every registered witness produces the result
currently specified for it*, and nothing more.

### 2.5 Re-pinning the authority blob is a ceremony, not an edit

`PINNED_BLOB` fails the run whenever the Phase G document changes at all —
including a change Phase G's own rules permit. So the pin needs a defined way to
move, because **legitimate supersession is an expected lifecycle event**, not an
anomaly. Left undefined, the first supersede meets a red check and a one-token
fix, and the property collapses into *the hash changed, therefore update the
hash*.

The layering that keeps it from collapsing:

> **Phase G supersede decides legitimacy → the re-pin binds the implementation
> to the new legitimate authority → the unchanged adversarial corpus verifies
> the binding did not alter behaviour.**

`PINNED_BLOB` sits in the middle floor only. It is an **evidence-binding
update, not a new authority decision** — so the procedure must *consume* an
already-authorized Phase G supersede rather than duplicate the supersede
ceremony. A re-pin that argues its own legitimacy is a second authority, and two
authorities over one frozen graph is the defect the pin exists to prevent.

**A re-pin is permitted only when all seven hold:**

1. the currently pinned Phase G artifact has been **legitimately superseded**;
2. the supersede relation is **recorded by Phase G's own normative mechanism** —
   not by this document, and not by the pin;
3. the new authoritative blob identity is **derived from that superseding
   artifact**, not from whatever the working tree happens to contain;
4. the pin change records **old blob id / new blob id / superseding authority
   reference**;
5. `tools/a1_v2_extract_graph.py --check` passes against the new authority;
6. the frozen mutation corpus is **rerun unchanged** — unchanged is the point:
   a corpus edited in the same commit proves the new authority against a ruler
   cut to fit it;
7. the re-pin and its evidence **land atomically**, in one commit.

**And the constraint that carries the whole thing: a pin failure must never
itself authorize the re-pin.** `AUTHORITY MOVED` is evidence of drift. It is
never a work item whose resolution is to make it stop printing. The extractor
now says so in the failure text, because the failure text is the only thing a
future reader is guaranteed to encounter at the moment of temptation.

Points 4 and 7 have a mechanical witness, stated at exactly its own reach.
`ORIGINAL_BLOB` is never edited, and `pin_chain_defect` refuses extraction when
a pin arrived without its paperwork, when paperwork was filed that moved no pin,
when an entry omits any of the three required fields, or when the recorded chain
does not actually run from the original blob to the pinned one. Both artifacts
carry it: `graph.json` now emits `original_blob` and `pin_history`, so a re-pin
is a visible structural change to the derived artifact rather than one token in
a source file.

Two words in that sentence would grow stronger semantics than the witness has,
so neither is used:

- **`PIN_HISTORY` is append-only *by contract*.** The extractor enforces chain
  integrity of the *observed* history, not historical immutability of prior
  entries. An editor can rewrite an old entry, repair the following links, and
  present a perfectly valid chain. Proving otherwise needs an external frozen
  witness — git history, or a countersigned record — which this floor does not
  have and is not given here.
- **The joint-validity property is over states, not over authoring.** What is
  actually enforced is that *the pin and its transition record must be jointly
  valid in every accepted checked state* — tree-state joint validity. It is not
  git-commit atomicity: a pin moved in commit A and its paperwork filed in
  commit B leaves a HEAD that passes, because the extractor observes a tree and
  never observes how that tree was assembled. Point 7 remains the requirement on
  whoever performs the ceremony; the machine holds only the weaker half.

Points 1, 2, 3 and 5 are not machine-checked here and are not claimed to be.
Legitimacy is a contract act; this floor cannot decide it and should not
pretend to. Nor does any of this defend against an editor who rewrites the
extractor's own constants — nothing self-hosted can. What the mechanism buys is
that the attempt must be **legible in a diff**: a re-pin can no longer look like
a typo fix, and a reviewer is handed the three facts they would otherwise have
to ask for.

## 3. E-1 — the envelope size bound

**Decision: `ref.size` keeps summing both halves, and Envelope v2 MUST define a
finite protocol-hard `max_envelope_document_bytes` — derived where the encoding
profile makes it derivable, explicitly chosen and justified where it does not.**

### 3.1 The seam

`E-V0-4` reached this phase as the one *contract*-level finding from the PR #124
probe, and Phase G §7 explicitly left "the envelope-size bound" undecided. The
open half:

```text
FD-1.8   ref to an envelope-bearing artifact:
             size = stored envelope bytes + stored payload bytes, together
FD-1.4   bounds typed A1 JSON objects, InteractionManifestV1, and opaque
             evidence blobs. It bounds no ENVELOPE.
```

So `max(ref.size)` is not derivable for eleven of the kinds. That is not
cosmetic. FD-1.5 charges `ref.size` against the direct and closure budgets
**before reading**, and a resolver must therefore reject an over-large declared
size — but for envelope-bearing artifacts there is no size to reject against.
Two conforming implementations can accept different bytes:

```text
implementation A   accepts any declared size up to the closure budget,
                   so an envelope-bearing ref may claim 128 MiB
implementation B   invents a ceiling — the probe used
                   1 MiB + min(64 MiB, 1 MiB) — and rejects above it
```

Same wire bytes, different verdicts. FD-1.2 froze digest framing precisely so
that two implementations cannot disagree about identity; leaving admissibility
undetermined re-opens the same class of divergence one layer up.

### 3.2 The candidates, and why two of them are already spent

The convergence ledger recorded three candidate shapes without choosing.

**Split the ref size — REJECTED.** Defining `size` over the payload alone and
carrying the envelope's size separately reintroduces exactly the defect R5.2
repaired: FD-1.8 exists because a ref that sizes only one half "charges half the
cost of what the resolver reads". Splitting it back out restores correct
accounting only if both numbers are always carried and summed — which is this
decision with extra steps, plus a new failure mode in which the two numbers
disagree.

**Reclassify typed non-envelope objects — ALREADY DONE, and it addressed the
other half.** See §4: S1 has already applied it to `InteractionManifestV1`. It
does not bound envelopes and never could.

**Bound the envelope explicitly — ACCEPTED, in a corrected form.** E-R0 wrote
that the maximum is *derived from the field set and never chosen*, and that
claim is false. Frozen FD-1.3 fixes only that stored bytes are UTF-8 JSON with
an object at top level, rejects a BOM and non-object roots, and states that
**content is never rewritten**. It says nothing about insignificant whitespace,
key order, or the lexical form of numbers — and FD-1.2 deliberately made
identity independent of serialization, so a conforming peer may encode the same
envelope differently and the stored document is kept verbatim.

Bounded field *values* therefore do not bound stored document *bytes*, and it is
document bytes that FD-1.8 sums into `ref.size`. Arithmetic over the field set
yields a bound on content, not on the artifact. E-R0 asserted the stronger thing.

```text
FROZEN for v2

ref.size = stored envelope bytes + stored payload bytes        KEEP (FD-1.8)

Envelope v2 MUST define a finite protocol-hard max_envelope_document_bytes.

  If the chosen v2 encoding profile makes that maximum mechanically derivable
  from schema + encoding grammar, publish the derivation.

  Otherwise the protocol chooses an explicit hard ceiling and records the
  justification for the number.

max_ref_size(kind) = max_envelope_document_bytes + max_payload_bytes(kind)

A resolver rejects ref.size above that maximum BEFORE reading either half, so
FD-1.5's declared-size-before-read discipline extends to envelope-bearing
artifacts as it already does to every other kind.
```

Derivability is available, but only at a price this decision does not pay on the
draft's behalf: it requires constraining the lexical representation — whitespace,
escape expansion, container syntax, number form. That is a wire-encoding
decision for Envelope v2 to take deliberately, not a consequence of bounding
fields. What E-1 fixes is that the maximum must be **finite, protocol-hard, and
published with its justification** whichever branch v2 takes; an implementation
inventing `1 MiB + min(64 MiB, 1 MiB)` remains forbidden either way.

### 3.3 What E-1 does not settle

The **number** is not decided here, and cannot be: it depends on the v2 envelope
field set *and* on the encoding profile, neither of which this phase has
drafted. E-1 decides the obligation and its two admissible discharges. The value
lands with the field set and the encoding decision.

E-1 also does not decide the **digest** half of FD-1.8. That FD fixes two things
about a ref — which digest identifies an envelope-bearing artifact, and what
`size` covers — and E-1 touches only the second.

E-1 also touches no node, no edge and no event kind, so the gate of §2 is
unaffected by it.

## 4. A correction to a convergence input: S1 already closed half of `E-V0-4`

The convergence ledger describes `E-V0-4` as two seams, both open. One of them
is no longer open against current authority, and this phase would otherwise
inherit a repair order for work already done.

`E-V0-4`'s second half was that FD-1.4 listed `manifest` among the 64 MiB
evidence blobs while also bounding "any typed A1 payload" at 1 MiB, leaving
`InteractionManifestV1` bounded twice. **S1 repaired it in v1**, by the third
candidate shape: current FD-1.4 reads

```text
typed A1 JSON object, except the one below       <=      1 MiB
InteractionManifestV1                            <=     64 MiB
opaque evidence blob (diff, raw provider bytes,
  gate log, patch)                               <=     64 MiB
```

`manifest` is gone from the evidence-blob list and the manifest has its own
line, on the grain argument that there is one manifest per *execution* rather
than per dispatch. Phase G §6 already records the 64 MiB figure as the inherited
v2 drafting baseline.

So `E-V0-4` enters this phase as **one** open seam, not two, and the remaining
half is §3's. Recorded here rather than silently narrowing the finding: a
convergence input that quietly shrinks is indistinguishable from one nobody
checked.

## 5. Dispositions consumed by this revision

Under the standing rule — a row closes only alongside the concrete decision that
consumes it, never in bulk because an adjacent phase finished. E-R0 reached past
that rule and E-R1 pulls it back.

**Consumed by E-1, in the size coordinate only:**

```text
FD-1.4  per-object bounds     PARTIAL. E-1 obliges a finite protocol-hard
                              envelope maximum; the VALUE is still owed, so the
                              block is not discharged. S1 separately resolved
                              this block's manifest double-bound.

FD-1.8  artifact refs         PARTIAL. The total-size semantics are consumed:
                              the summing rule survives verbatim and E-1
                              supplies the maximum it lacked. The digest /
                              identity half stays OPEN for the Envelope-v2
                              digest decision.

V1-N116 ref sizing only the envelope            semantic KEEP; proof coordinate
V1-N118 closure sized by envelopes alone        empty, pending v2 wire types
```

**Reverted to OPEN by E-R1:**

```text
V1-N117 ref digesting the payload, not the envelope
        E-R0 closed this under E-1. It is a DIGEST-DOMAIN row, and E-1 is a size
        decision — while §6 of the same revision said this phase decides no
        digest domain. A row closed by a decision that does not reach it is
        worse than an open row, because it looks discharged.
```

Everything else in the 47 + 141 remains OPEN. Two rows half-dispositioned and
two FDs partial is the honest state of a first substantive decision.

## 6. What this revision does not decide

- the v2 envelope field set, framing order, or any digest domain;
- the *value* of `max_envelope_document_bytes`, and which of E-1's two branches
  discharges it (§3.3);
- `campaign_protocol_version`, or `message_kind_version` for any kind;
- `E-V0-1`, `E-V0-2`, `E-V0-3` — implementation-level findings, still owed;
- any disposition beyond the five of §5.

## 7. Revision record

### E-R15 — the criterion holds on both sides of the checking logic

Not a finding. What E-R14 turns out to have been an instance of.

**The judgment state space was already ternary** — *judgment valid and
satisfied*, *judgment valid and violated*, *judgment not obtainable* — and the
caller-facing vocabulary projected it onto two. That is the E-R9 criterion one
floor **above** the checks rather than below them:

> **the caller-facing verdict was less discriminating than the judgment state it
> claimed to represent.**

So `FAIL — Envelope v2 is not realized` was not merely misleading wording. It
was a **false machine-readable fact**: the detail line knew the ruler was
invalid while the API result asserted the target had been measured and had
failed. Callers parse `RESULT`; repairing only the prose would have been
lipstick on a protocol bug.

The dominance rule, stated hard because a weaker form makes `ERROR` decorative:

> **A failed sub-check under an invalidated ruler is evidence about execution,
> not evidence that the target failed the gate.**

The 70th-edge case is the clean witness precisely because both facts coexist —
the premise is invalidated **and** step 5 observes an uncarried edge. The error
would be to treat them as competing findings and ask which wins. They live at
different levels: step 5 truthfully reports what it observed, and the terminal
evaluator must still say `ERROR`, because those observations are no longer
licensed to decide realization. My first corpus assertion said `errored and not
failed`, which would have discarded a real downstream observation merely because
it cannot support the final verdict — making the data fit the state machine.

#### The second chain

The instrument is not specific to translating authority into verification. The
same question applies at every arrow of the **consumption** chain:

```text
step observations -> judgment validity -> terminal semantic state
                                       -> exit code / RESULT -> caller
```

*Can two states that matter to the consumer collapse into one representation?*
They did:

```text
realization failed a valid ruler   ─┐
invalid ruler, realization unjudged ┴─> FAIL
```

Pure projection loss. So the full chain the criterion governs runs end to end:

```text
authority -> admitted evidence -> verifier representation -> judgment
          -> emitted verdict -> consumer
```

The same analytical instrument found information loss on **both** sides of the
actual checking logic. That is a stronger result than another P1.

#### `ERROR` is an epistemic state, not a process outcome

Recorded here and in the `Report` docstring, because it is the sort of rule a
future refactor tidies away: **do not merge `ERROR` into a generic
infrastructure-error bucket.** A crash, malformed input, an invalidated premise
and an unverifiable preflight may deserve different diagnostics, but if none of
them permits a valid judgement they share the terminal semantics that matter —
*no authoritative claim about realization was produced; do not repair the target
on the strength of this result.*

What E-R14 actually closed, then, is more fundamental than a diagnostics hole:
the gate can now say **"I do not know"** without lying and calling that
knowledge a failure.

#### The corpus statement, final form

> The corpus is a **contract-relative regression witness**. It establishes
> stability against known cases under the currently accepted oracle. It neither
> establishes the oracle's adequacy nor supplies independent evidence for the
> semantic correctness of its expected verdicts. **A shared authoring model can
> cause the corpus to ratify the same defect it is intended to guard against.**

Three failure modes of one epistemic dependency, all witnessed here:

| | what the shared model did |
|---|---|
| **E-R1** | encoded a clause-5 violation as an expected **PASS** |
| **E-R6 – E-R11** | omitted the observation distinctions the author could not see, so no mutation existed to catch them |
| **E-R14** | encoded the wrong verdict semantics as four expected **FAIL**s |

Before E-R14: implementation A, oracle A, 31/31. After: implementation B,
oracle B, 31/31. **A and B disagree on the externally visible contract.** The
green number was preserved across a change in what it means, which is the
cleanest available demonstration that a regression suite can be entirely green
while guarding yesterday's misconception with admirable discipline.

### E-R14 — `ERROR` is not `FAIL`

**P1 (Codex, against `442eedb`).** `AGENTS.md` rule 2 is binding and explicit:
`PASS` / `FAIL` / `ERROR` are three distinct states — *`FAIL` means the gate ran
and the target failed it; `ERROR` means the harness could not obtain a
trustworthy answer.* This gate had two.

So when `run_steps` detected that **its own** step-3 premise was invalidated, it
reported `FAIL` and `emit` concluded:

```text
RESULT: FAIL — Envelope v2 is not realized
```

The realization had not been judged at all. The same for a failed preflight: if
the committed graph is not what Phase G derives to, the ruler is not the
authority and no answer obtained downstream means anything.

**This is the E-R10 defect surviving one floor up.** E-R10 fixed the premise's
*diagnostic text* — it says `Do NOT alter the authority` — and left the *verdict
vocabulary* still blaming the target for a defect of the machinery. A reader who
trusts the RESULT line over the detail line gets exactly the wrong instruction,
and the RESULT line is the one a caller parses.

Now three verdicts and three exit codes:

| condition | verdict | exit |
|---|---|---|
| every step satisfied | `PASS` | 0 |
| the gate ran, the realization failed it | `FAIL — Envelope v2 is not realized` | 1 |
| premise invalidated, or preflight unverifiable | `ERROR — no trustworthy answer; the realization was NOT judged. Repair the verifier, not the target.` | 2 |

**`ERROR` dominates.** The corpus case makes the point: a widened graph that
invalidates the premise *also* leaves a 70th edge uncarried, so step 5 fails
too — and the verdict is still `ERROR`, because once the premise is void no step
verdict is trustworthy enough to report. A gate that announced `FAIL` there
would be reporting a measurement it had just declared itself unable to make.

Corpus 31, with four cases re-specified from `FAIL` to `ERROR`: those
assertions had been encoding the defect as expected behaviour, which is the
third time in this branch a corpus case has certified the very conflation it
was written to guard.

`graph.json` unchanged; zero rows, zero nodes, zero edges.

### E-R13 — normalization-before-validation, and a refutation surface

**A boundary earlier than the one E-R10 named:**

```text
raw observations -> well-formed evidence -> normative representation
                                                 -> acceptance predicate
```

The preservation criterion applies only *after* the first arrow. Before it,
`set()` and `{path: fact}` look innocent — one of them is mathematically the
norm's own shape — because they normalise the input before anything has
established that the input may be normalised.

> **A representation may be coarser than the raw evidence only after the
> distinctions it discards have been proven semantically irrelevant, or invalid,
> at the well-formedness boundary.**

**`normalization-before-validation`** is recorded as the *mechanism*, not a
fourth species. The taxonomy says **what information was lost**; this says
**where it was destroyed**, and it can produce all three:

| construct | produces |
|---|---|
| `set(...)` | multiplicity collapse |
| dict overwrite `{k: v}` | multiplicity collapse **+ contradictory-state erasure** |
| a projection key | projection loss |
| filter/dispatch on selected spellings | role substitution |

**Step 1 was not wrong, and sets are not the villain.** `required_kind_set ==
observed_kind_set` answers exactly the normative question *which kinds exist*.
The defect was upstream: `["A", "A"] → {"A"}` silently rewrote *the schema
declares A twice* into *the schema declares A*. The projection was right; the
**admission** to it was not. Blaming the mathematics would have been the wrong
lesson and the more comfortable one.

**The contradictory-path witness is the stronger of the two**, because
`{f["path"]: f}` did more than erase multiplicity: it introduced **order-dependent
conflict resolution where no resolution policy exists**. `P → X` and `P → Y` did
not become an error; they became *P → whichever came last*. Worth recording that
the first probe appeared to show step 3 catching it — reordering the rows showed
the verdict tracked list order, not the contradiction. An accidental downstream
collision, wearing the costume of an enforced invariant, which survives review
under the industry's proudest sentence: *the test passes*.

#### The admissibility rule

A coarser representation is admissible **iff**:

1. the input is already **well-formed**; **and**
2. every distinction discarded is **irrelevant to this step's normative
   quantifier**; **or**
3. an **explicit checked premise** makes that distinction impossible in the
   admitted authority.

Step 5 passes by (2). Step 3 passes by (3). E-R3, E-R7, E-R8 and E-R11 each
failed one of them.

#### The refutation surface

`step × dimension → verdict`, over the four dimensions this branch has supplied
witnesses for. Every cell is a falsifiable claim:

| | identity coordinates | path / provenance | semantic role | multiplicity |
|---|---|---|---|---|
| **well-formedness** | — | PRESERVED | PRESERVED (closed universe) | PRESERVED |
| **1** universe equality | PRESERVED | IRRELEVANT-BY-NORM | IRRELEVANT-BY-NORM | IRRELEVANT-BY-NORM¹ |
| **2** presence + carriers | PRESERVED `(src,tgt)` | PRESERVED (E-R8) | PRESERVED | PRESERVED (exact 1/0) |
| **3** field capability | PRESERVED; class **LOSSLESS-BY-PREMISE** (E-R10) | PRESERVED¹ | PRESERVED (filtered, universe closed) | IRRELEVANT-BY-NORM¹ |
| **4** forward coverage | PRESERVED (triple) | IRRELEVANT-BY-NORM² | **AUTHORITY-UNDECIDED**³ | IRRELEVANT-BY-NORM¹ |
| **5** reverse coverage | PRESERVED | IRRELEVANT-BY-NORM | IRRELEVANT-BY-NORM | IRRELEVANT-BY-NORM⁴ |

1. Admissible **only** because well-formedness rejects the duplicate first. This
   is clause 1 doing the work, not clause 2 alone.
2. Path validity is established for every row before this step — structural rows
   by step 2, `artifact_ref` rows by step 3.
3. Whether an **additional** `artifact_ref` carrier on a payload-bearing pair is
   admissible is not answered by the frozen text.
4. The norm quantifies **existence**, not count. Membership loses nothing
   *relative to the norm* — the positive case the criterion exists to permit.

**`AUTHORITY-UNDECIDED` is a completed analysis, not a gap in it.** The frozen
text does not answer, so the verifier does not answer either. A preservation
table that resolved its own empty cell would be an analysis tool voting itself
legislative power on the grounds that it noticed something — which is the E-1
error with better manners. It is closed by Phase G or not at all.

No mechanism changed in this revision. Corpus 31; `graph.json` unchanged.

### E-R12 — the audit, and what twelve revisions were actually about

**The dominant failure mode of this branch, stated as a conclusion:**

> **It was not incorrect authority, and it was not incorrect artifact
> generation. It was loss of normative distinctions during verification.**

`graph.json` is byte-identical across all thirteen revisions. Six P1s and three
Majors changed only *which invalid states the verifier could tell apart from
valid ones*. That is unusually direct evidence for the claim: had the problem
been the authority or the derivation, the derived artifact would have moved.

**The layering rule E-R11 earned:**

> **Establish observation well-formedness before asking whether the observations
> satisfy the authority.**

An unknown `carrier_kind` is not *coverage = false*; it is not a valid
observation at all. A duplicate row is not something later set membership should
quietly normalise. Without the rule, the five frozen steps reason over malformed
evidence and every `set()` and `{k: v}` becomes an accidental sanitiser.

#### The audit: preservation tables for the steps nobody had examined

For each step: the normative distinction, what encodes it, the representation
used, and whether the distinction survives — with *why*, or with the pinned
premise that makes a coarser view lossless. The four dimensions are the ones
this branch has now supplied witnesses for: **identity coordinates, path /
provenance, semantic role, multiplicity.**

| step | norm | representation | preserved? |
|---|---|---|---|
| **1** | exact set equality of event kinds | `set(schema.event_kinds)` | **Subject genuinely is a set** — set equality *is* the norm. But the observation was a list, and `set()` silently normalised a schema defining one kind twice. **Fixed:** well-formedness first. |
| **3** | a field declares the complete set of edges it may realize | `{f["path"]: f}`, source + target domain | **Was lossy twice.** Class omitted — coarser, now covered by a pinned premise (E-R10). And indexing by path let two contradictory facts resolve by **list order**: correct fact last, gate green. **Fixed:** well-formedness first. |
| **4** | every carrier maps only to §4.2.4 edges | `(source, target, class) ∈ admitted` | **Identity preserved exactly.** Role not observed here — legitimate only because well-formedness closes the role universe and step 2 binds the digest pairs. **One row owed:** whether an *additional* `artifact_ref` carrier on a payload-bearing pair is admissible is not decided by the frozen text, so this gate neither permits nor forbids it deliberately. |
| **5** | every edge has **>= 1** carrier | set membership over triples | **Lossless because the subject really is coarser.** The norm quantifies existence, not count; multiplicity and path are outside what it distinguishes. This is the positive case the criterion is meant to permit. |

The audit found the same species again, on the term that had no well-formedness
check at all — the **schema facts**. A kind declared twice collapsed under
`set()`; two contradictory facts for one path resolved by list order, and with
the correct fact last the gate passed while the middle term contained a
contradiction. Both now fail *schema facts well-formedness*. Corpus 29 → 31.

#### What E-R11 did and did not establish

It did **not** show the taxonomy is complete. It showed something narrower and
better: all three named species have explanatory power, and the two that lacked
current-code witnesses **produced them prospectively** on the next targeted
review. Prediction, then payout — not three labels fitted to old bugs. Nobody has
discovered the periodic table of verifier defects here.

#### What the corpus can and cannot establish

**Zero of the nine external findings came from the 31 author-written cases.**
That does not make the corpus useless; it fixes its meaning precisely:

> The corpus establishes **regression against known adversarial shapes**. It
> cannot establish **adequacy of the author's own observation model**, because
> the same model chooses the representation and the mutation.

That boundary is now demonstrated rather than argued. §8.7 is the sharpest
instance: the correct question sat written down, in this document, for two
revisions, beside the code where the answer was yes.

### E-R11 — the other two species, both predicted and both present

Two P1s from Codex against `ef0200a`, arriving one round after §7 named three
information-losing operations and claimed each had been witnessed at least once.
Both were the two whose only witnesses were historical. Reproduced before
acceptance:

```text
invented carrier_kind      -> (0, all five PASS)
duplicate artifact_ref row -> (0, all five PASS)   ledger rows: 70
```

**Role substitution.** `carrier_kind` is a **closed** choice in frozen §4.2.6.
Step 2 filters for `event_payload_digest`, step 3 filters for `artifact_ref`, so
a misspelled or invented third role fell through both — and step 4 admitted the
row on its `(source, target, class)` triple alone, which is the coordinate set
that survives when the role is erased. A carrier with no role, a nonexistent
path, and a green gate.

**Multiplicity collapse.** Step 3 unions target domains and steps 4/5 compare
sets, so a byte-for-byte duplicate reduced to one observation. Seventy rows, a
sixty-nine-edge graph, `PASS`. Frozen §4.2.6 does allow several carriers for one
edge — the `NormalizedOutput` case — but that permits two *different* field
occurrences, not two declarations of one.

Both now fail a **ledger well-formedness** check that runs before the five
frozen steps rather than inside them: the role must be one of the two admitted
values, and no `(source, target, class, carrier_kind, path)` row may be declared
twice. Placed before coverage, because a row whose role is unknown is not a
carrier whose coverage can be assessed. The five steps keep their frozen order
and numbering. Corpus 27 → 29.

**What this round is actually evidence of.** §7 named three species and offered a
witness for each; two of those witnesses were old defects already repaired, so
the claim that the species were *live* rested on nothing. One round later both
turned out to be present in current code. The taxonomy was not a tidy
retrospective classification — it described where to look, an outside reader
looked there, and the defects were there.

It also closes §8.7, which asked *is there a place where duplicate `artifact_ref`
carriers should be a defect rather than a no-op?* That question sat in this
document for two revisions, correctly worded, next to code where the answer was
yes. Writing a question down is not attacking it, and an author who has written
the question is measurably not the person who will answer it.

### E-R10 — a design criterion, and a premise moved to its own layer

**The criterion, promoted from reviewer heuristic to a rule every acceptance
predicate here must satisfy:**

> **Every acceptance predicate must be at least as discriminating as the
> normative distinction it claims to enforce — or explicitly declare and pin the
> authority property that makes its coarser representation lossless.**

The second half matters more than the first, and is what stops E-R9 from
degenerating into *we found a class of error, so add dimensions everywhere*. A
verifier that carries the full normative identity at every step is not
automatically better; a week later it is checking its own imagined schema rather
than the frozen authority. Coarser representations stay legitimate. What is not
legitimate is a coarser representation whose losslessness is **silent**.

That also explains the three sub-species as three distinct information-losing
operations rather than a list that happened to reach three: projection loss
drops a coordinate of identity; role substitution keeps the coordinates and
erases semantic role; multiplicity collapse keeps identity and erases
cardinality. One question — *can two normatively distinct states collapse into
one state the verifier observes?* — covers all three.

**The premise moved from the extractor to the gate.** E-R9 pinned *class is
functionally determined by (source, target)* inside `extract`, which was the
wrong layer, and wrong in the exact way this document keeps being about. If a
legitimate supersede ever admits `(X, Y, A)` and `(X, Y, B)`, an extractor
failure would mean **the authority cannot be derived** — the authority
demoted to satisfy an implementation, arriving through a schema assumption
instead of through a derivability claim. Same defect as E-1's, with a false
moustache.

The premise now lives in the verifier, fails the verifier, and says what it
means:

```text
VERIFIER PREMISE INVALIDATED — <X> -> <Y> carries ['Causal', 'Intra'];
step 3 assumes class is functionally determined by (source, target).
Revise step 3 or supersede the verifier contract.
Do NOT alter the authority.
```

The last line is the load-bearing one. A supersede admitting such a pair is a
legitimate authoritative shape; what it invalidates is **this representation**.
Corpus 26 → 27, with a case asserting that shape is *declared*, never silently
accepted and never reported as a defect of the authority.

**On the epistemics, stated plainly.** The residual-risk wording promises no
closure, and should not. Author-written mutation cases can exhibit witnesses;
they cannot establish the absence of projection errors, because the same mental
model chooses both the representation and the mutation. A case count is a
record of what has been survived, not a bound on what remains — which is why
§2.4's number is a fact about the corpus and not a claim about the gate.

### E-R9 — the family, named; and one latent instance pinned

Not a finding. A generalisation of three, plus the one thing it immediately
turned up.

**The failure class.** E-R6, E-R7 and E-R8 are not three omissions. They are one
defect shape:

> **An authority can be correct while the verifier's observation granularity is
> weaker than the authority's subject.**

The frozen sentence quantifies over some normative object. The predicate
projects that object onto a weaker key. Where the projection is non-injective,
two normatively distinct states become one observed state, and the check reports
on the projection while appearing to report on the norm. E-R7 is the clean
demonstration: `struct_count` keyed by `source` collapsed row 56 and row 64 of
`CampaignEvent(HumanCommandRejected)`, so a swap preserved `struct_count == 1`
while the required pair no longer existed.

That reframes what E-R6 accomplished. It closed *where does the requirement come
from* and never asked *what is the identity of one requirement*. Two different
questions; answering the first is not evidence about the second.

**The chain to audit, and the single question at each link:**

```text
normative object -> extracted key -> multiplicity/counting -> comparison
                                                              predicate
                                                                 -> accepted witness

at every arrow:  can two normatively DISTINCT objects collapse into one state
                 the verifier observes?
```

Three named sub-species, each already witnessed here at least once:

| | shape | witnessed |
|---|---|---|
| **Projection loss** | identity is `(A,B,C)`, the check indexes `(A,B)` | E-R7 (target dropped), E-R8 (path dropped) |
| **Role substitution** | coordinates agree, but the verifier treats values of different roles as interchangeable | E-R7 — steps 4/5 never inspect `carrier_kind`, so a digest and an `ArtifactRef` are the same thing to them |
| **Multiplicity collapse** | existence or set membership checked where the norm requires exactly one, or requires distinguishing two identical-looking carriers | E-R3 (step 2 deduplicated where G-R11 counts) |

**What the lens found immediately: step 3 has a latent projection loss.** Step 3
keys field capability by `(path → source, target domain)` and never looks at
class — a three-field identity projected onto two. It is lossless *only* while no
frozen `(source, target)` pair carries two classes. That has always been true of
the registry, and **nothing pinned it**: the extractor asserts uniqueness of
`(source, target, class)` triples, which happily permits one pair with two
classes. The guard was a property of the data, not of the check.

Not exploitable today — a mis-classed carrier still fails steps 4 and 5, which
key on the full triple. But if a supersede ever admits such a pair, step 3 would
go blind with no diagnostic anywhere. The extractor now declares the dependency
and stops loudly if it breaks. Deliberately **not** widened speculatively: the
repair makes an implicit assumption explicit, which is the honest move when the
check is correct *given* an unstated premise.

**Residual risk, stated hard.** E-R7 does more than close a second P1: it refutes
the hypothesis that what remained after E-R6 was local omissions. The residual
risk for this gate now explicitly includes **semantic projection errors between
the frozen authority and the verifier's representation of it** — a class, not a
backlog. It is not closed by any number of mutation cases written by the author,
because the author's mental model is what chooses the key in the first place.

Corpus unchanged at 26; `graph.json` unchanged; zero rows, zero edges.

### E-R8 — the column nothing checked

**P1 (Codex, against `4eee8ec`) — a structural carrier's `path` was verified by
nothing.** Step 2 counted `(source, target)`. Step 3 only ever inspects
`artifact_ref` rows. Steps 4 and 5 compare `(source, target, class)` and ignore
paths entirely. So the one column that says *where in the wire this digest
actually lives* fell between all five steps:

```text
every structural path := "synthetic::does-not-exist"
exit 0  {'1': 'PASS', '2': 'PASS', '3': 'PASS', '4': 'PASS', '5': 'PASS'}
```

Frozen §4.2.6 lists *concrete carrier path* as a column of a ledger row. A
column nothing checks is not evidence — it is a comment with a schema around it,
which is the `_note` fields of E-R0 wearing a different hat.

`structural_commitments` was a bare list of event kinds, which could only ever
prove the digest existed *somewhere*. It is now a map **kind → concrete carrier
path**, extracted from the schema like every other fact, and step 2 requires each
structural row's path to equal the path the schema commits for that kind. Corpus
25 → 26.

**Three P1s in a row, all in step 2, all the same shape.** E-R6: the requirement
read its right-hand side from the term under test. E-R7: the requirement bound
sources but not targets. E-R8: the requirement bound the relation but not the
column that locates it. Each repair was correct and each left an adjacent
dimension unbound, because the author verified the thing just fixed rather than
re-deriving what the frozen sentence actually quantifies over. That is worth
recording as a pattern rather than three incidents — and all three were found
from outside, each on the commit that had just closed the previous one.

### E-R7 — the same wound, one level deeper

**P1 (CodeRabbit, against `4a5ad37`) — step 2 bound the digest to an event kind
but not to its payload.** E-R6 had just moved the requirement's right-hand side
to the frozen map. It moved the *sources* and left the *targets* free:
`struct_count` was keyed by `source` alone, and eleven of the twenty-one kinds
carry more than one frozen edge.

`CampaignEvent(HumanCommandRejected)` holds row 56 (`→ HumanCommandRequest`,
`Causal`) and row 64 (`→ HumanCommandRejectedPayload`, `Intra`). Swap which one
declares the digest, adjust the field facts to match, and:

```text
exit 0  {'1': 'PASS', '2': 'PASS', '3': 'PASS', '4': 'PASS', '5': 'PASS'}
digest now points at: HumanCommandRequest | contract requires: HumanCommandRejectedPayload
```

Step 2 counted one digest for that source and was satisfied. Steps 4 and 5 saw
two admitted edges, both carried, and were satisfied — they compare
`(source, target, class)` and never look at `carrier_kind`, so a digest and an
`ArtifactRef` are interchangeable to them. Nothing in the gate held the frozen
sentence *`expected_payload(k) = P` ⇒ EXACTLY ONE `CampaignEvent(k)
--event_payload_digest--> P`*, whose subject is a **pair**.

`struct_count` is now keyed by `(source, target)` and compared against the
frozen pairs: `want = 1` catches a required pair left uncarried, `want = 0`
catches the carrier kind appearing on any edge the frozen map does not name.
Corpus 24 → 25.

**Two rounds, one lesson.** E-R6 fixed *where the requirement comes from* and
did not ask *what the requirement is a requirement about*. A repair that
narrows an authority leak by one dimension while leaving another open is the
characteristic shape of fixing a finding rather than fixing a defect — and it
was found immediately, by the same reviewer, on the commit that claimed to have
closed it. The corpus now pins the pair, not the count.

### E-R6 — first external review: one P1 and three Majors, all confirmed

Four findings from two reviewers who did not write the gate. Every one was
reproduced before acceptance; none was taken on report.

#### P1 (Codex) — step 2 let the term under test set its own requirement

**P1 (Codex, against `a10d845`) — step 2 let the term under test set its own
requirement.** The count of structural carriers was compared against
`schema.structural_commitments`. So a schema declaring a **correct**
`payload_presence` while committing no `event_payload_digest` made *zero* the
expected count, and a ledger re-declaring all eleven payload edges as
`artifact_ref` carriers then satisfied steps 3, 4 and 5 on their own merits.

Verified before acceptance, by running it:

```text
exit 0  {'1': 'PASS', '2': 'PASS', '3': 'PASS', '4': 'PASS', '5': 'PASS'}
```

A full green over a realization carrying **none** of the eleven structural
digests the contract requires. Frozen 4.2.6 is unambiguous about where the
requirement lives — *`expected_payload(k) = P` ⇒ EXACTLY ONE ledger carrier
`CampaignEvent(k) --event_payload_digest--> P`* — and `expected_payload` is the
frozen map, not a schema self-declaration. `required_struct` is now derived from
it, and the schema's own commitments became a *third* term that must agree with
the frozen map rather than define it. Corpus 20 → 22.

Three things about this one are worth keeping.

**It is the same defect, three floors down.** E-R1's was *the gate checked the
ledger against itself*; E-R2's was *the frozen right-hand side taken on trust*.
This is that shape again, in the one sub-clause of one step where it had
survived four revisions of me looking for exactly it.

**§2.2 documented it in plain sight.** The line read `AND declared structural
carriers == schema.structural_commitments`. That is the defect, written out, in
the section explaining what each step compares — read four times by its author
without registering, because it describes what the code does and the author knew
what the code did. It is now written as what the *contract* requires.

**It is the answer to §8.4, and the answer arrived from outside.** That question
asked which plausible v2 drafting error the corpus still missed, on the
reasoning that the mutations and the gate shared one mental model. The uncovered
error turned out to be *model the payload as an `ArtifactRef` field rather than
the structural digest* — not exotic, not adversarial, a thing a drafter does on
a Tuesday. Twenty-two mutation cases written by the gate's own author did not
contain it; an independent reader found it within minutes of the PR leaving
draft. That is the correlated-authorship weakness demonstrated rather than
merely conceded, and it is worth more than the fix.

#### Major (CodeRabbit) — `assert` made every frozen check optional

Python strips `assert` under `-O` / `PYTHONOPTIMIZE`. Sixteen of them carried
the pin evidence and every structural fact. Reproduced:

```text
PYTHONOPTIMIZE=1  ->  extract SUCCEEDED with unevidenced pin -> ffffffff
```

One environment variable deleted the entire ceremony of §2.5 two revisions after
writing it, plus the 69-edge, 56/13, 21/11 and 47/20 facts — and `--check` then
printed `extract OK`. All sixteen are now `_require(...)` raising
`ExtractDefect`. The corpus pins it: *frozen checks survive PYTHONOPTIMIZE=1*.

There is an uncomfortable symmetry here. E-R4 built a ceremony so a re-pin could
not be a one-token edit, and shipped it guarded by a statement the interpreter
discards on request. The mechanism was sound and its enforcement was optional,
which is the failure mode this whole document is about, expressed in the
grammar of the language rather than in the design.

#### Major (CodeRabbit) — the harness skip was invisible in the report

`A1_V2_GATE_HARNESS=1` skipped both preflights **and added no report line**, so
a harness run was textually indistinguishable from a proven PASS; an env var
exported once in a shell disarms every later invocation in it. §8.6 carried the
weaker version of this — *a bypass that ships in the acceptance executable* —
and the recorded repair, deferred until after mechanism review, is now done:
steps 1–5 are an importable `run_steps()`, the corpus calls it directly, and the
executable contains no bypass at all. The variable is inert, with a regression
pinning that.

#### Major (CodeRabbit) — the ledger template taught the defect

`_carriers_schema.target` read *"exact CONCRETE target semantic kind (a
meta-target is expanded to its members here)"*. That instructs precisely the
mutation frozen clause 5 forbids and the corpus rejects — the template a drafter
copies from contradicted the contract it was there to serve. Corrected to state
that a meta-target is declared once, verbatim, with expansion checked only in
step 3.

#### Minor (CodeRabbit) — a preamble that hid its own exceptions

`a1-f-v2-convergence.md` §4 said the disposition columns are *"empty by
construction in this revision"* while `V1-N116` and `V1-N118` carry KEEP. An
auditor trusting the preamble skips the only two rows holding a disposition.
Now stated with its two exceptions.

#### What the round is evidence of

Corpus 20 → 24. Four defects, none found by twenty-two adversarial cases written
by the gate's own author, all found within the hour by readers who did not write
it. Two of them — `assert` under `-O`, and a skip that leaves no trace — are not
subtle; they are invisible from inside, because the author knows what the code
means and reads intent rather than text. That is the correlated-authorship
weakness demonstrated rather than conceded, and it is the argument for review
before the field set lands, not after.

### E-R5 — two adjectives that outran their witness

A precision round. No machinery changed; two claims did, because both said more
than the mechanism observes.

**"Append-only."** `pin_chain_defect` reads one snapshot of `PIN_HISTORY`.
Chain integrity of the observed history is not historical immutability of prior
entries: an editor can rewrite an old entry, repair the following links, and
present a valid chain. Now stated as *append-only by contract*, with the gap
named — proving the stronger property needs an external frozen witness (git
history, a countersigned record) that this floor does not have.

**"Land atomically, in one commit."** That remains the requirement on whoever
performs the ceremony, and point 7 keeps its wording. What the machine holds is
the weaker half: *the pin and its transition record must be jointly valid in
every accepted checked state*. Tree-state joint validity, not commit atomicity —
a pin moved in commit A and papered in commit B leaves a HEAD that passes,
because the extractor observes a tree and never observes how it was assembled.

E-R4's record above keeps its original wording, including "Points 4 and 7 are
made mechanical", as the evidence of what was overclaimed. The live text in §2.5
and the extractor's own docstring are what got corrected.

Neither point weakens the ceremony. They stop two attractive adjectives from
quietly acquiring semantics the code does not implement — which is the same
failure as an assertion dressed as a derivation, only smaller and better
camouflaged.

**Deliberately not done: §8.5.** The residual — `superseding_authority` is a
string nothing resolves, so the ceremony proves an authority was *named*, not
that it exists — stays open and unsolved on purpose. It is bounded, documented,
and does not invalidate the current pin. Closing it before external review would
destroy a probe worth more than the fix: whether an independent reviewer finds
that seam without being handed it. Given that the standing epistemic weakness
here is correlated authorship of implementation and mutations alike, one
deliberate unresolved seam is more informative than polishing the artifact until
its own author can no longer imagine an objection to it.

### E-R4 — the re-pin ceremony, answering §8.5

Not a review round. §8.5 asked whether *"re-pin deliberately"* was enough of a
procedure for `PINNED_BLOB`, or whether a supersede needs a recorded ceremony
here the way it does in the contract. The answer is the second, with a
constraint attached: **yes, because legitimate supersession is an expected
lifecycle event — but the procedure must consume an already-authorized Phase G
supersede rather than duplicate the supersede ceremony. `PINNED_BLOB` is an
evidence-binding update, not a new authority decision.**

That distinction is the whole content of §2.5. Three floors: Phase G supersede
decides legitimacy, the re-pin binds the implementation to the result, the
unchanged corpus verifies the binding changed no behaviour. The pin owns only
the middle one.

The load-bearing constraint is that **a pin failure must never itself authorize
the re-pin** — otherwise the safety property degrades into *hash changed,
therefore update hash*, which is checksum theatre. `AUTHORITY MOVED` now says
this in the failure text itself, since that text is what a future reader meets
at the moment of temptation, and a rule recorded only in a document three
directories away is a rule recorded for the innocent.

Points 4 and 7 of the ceremony are made mechanical rather than left as prose:
`ORIGINAL_BLOB` (never edited) plus append-only `PIN_HISTORY` plus
`pin_chain_defect`, which refuses extraction outright for a pin without
paperwork, paperwork without a pin, a missing field, or a chain that does not
run from the original blob to the pinned one. `graph.json` gained `original_blob`
and `pin_history`, so the evidence rides in the derived artifact. Corpus 18 → 20.

Points 1, 2, 3 and 5 stay human, and §2.5 says so plainly. Legitimacy is decided
by Phase G's mechanism, not by an extractor, and a check that claimed otherwise
would be asserting authority it does not hold — the same error E-1's derivability
claim made in E-R0, one floor down.

Zero rows, zero nodes, zero edges. The frozen registry is untouched, as in every
revision here.

### E-R3 — third independent review, one P1 and a staleness fix

**P1-10 — step 2 deduplicated where it had to count.** Frozen G-R11 requires
*exactly one* carrier for each of the eleven payload-bearing kinds and *exactly
zero* for the other ten. The implementation built a `set` of carrier sources, so
two identical structural rows for `CampaignEvent(CampaignCreated)` collapsed into
one and passed. Small, and located precisely inside the exact-cardinality
contract that G-R11 and G-R12 exist to enforce. Step 2 now counts per source;
the corpus gains *duplicate structural carrier for one event kind*. 18/18.

**Staleness.** The branch was 17 commits behind `main`. Verified before
rebasing: those commits touch only `docs/o7-invoke.md`,
`docs/tasks/mg-c-model-gate.md` and `src/invoke.rs`, and both pinned blobs —
A1 authority `3b26849c` and Phase G `450380ff` — are unchanged, so this is a
rebase and a re-run rather than any re-adjudication. Opening a review on a
knowingly stale base, one round after G-R10 caught exactly that, would have been
performance art.

**Carried as an open review target, not fixed.** `A1_V2_GATE_HARNESS=1` is still
a runtime preflight bypass, merely moved from a CLI flag into an environment
variable. It is not a false-green in ordinary invocation, but it is a bypass
living in the acceptance executable. The right shape is to lift steps 1–5 into
an importable function the corpus calls directly, leaving no bypass in the
shipped path at all. Recorded in §8 rather than done, because it is a
refactor of the machine under review and belongs after the mechanism review, not
inside it.

### E-R2 — second independent review, three P1s and two P2s

All five accepted. Every one sits in the *trust joint* rather than in a design
decision, and one of them was already encoded in the corpus as an expected PASS
— the most expensive kind of defect, because the test suite was defending it.

**P1-6 — the middle term was still on its honour.** `drafted =
bool(schema.get("extracted_from"))`, with `extractor` never inspected: a drafter
could point `extracted_from` at anything and hand-fill the three arrays. E-R1
had moved guarantee-by-comment from the ledger up one floor rather than removing
it. Closed by committing `tools/a1_v2_extract_schema.py` **now**, while there is
no v2 schema and therefore nothing to be tempted about, so that when the field
set lands the extractor's *implementation* changes and the gate's trust model
does not. A hand-filled facts file is now a corpus case.

**P1-7 — schema facts were too poor to express clause 1.** They held a bare path
list, so step 3 compared sets of paths. Swapping the targets of
`WorkOrder.input.candidate_ref` and `...materialization_ref` leaves both frozen
triples present globally: steps 3, 4 and 5 all pass while each field realizes
the wrong relation. Facts now carry per-field capability — source kind and the
complete permitted target domain — and step 3 compares domains per path. This is
also the realistic form of the "wrong direction" attack §8 of E-R1 asked about:
not a reversed arrow, but a correct edge on the wrong field.

**P1-8 — the meta-target was realized against frozen clause 5.** *"Declared ONCE
as a meta-target expansion, never as eleven separate edges."* E-R1's `faithful()`
expanded row 69 into eleven carriers, and its *expected-PASS* case kept only
`CampaignFeedItem → WorkOrder` — asserting that a `subject_refs` permitting one
of eleven kinds faithfully realizes a union of eleven. Steps 4 and 5 now match
the frozen triple exactly, and the expansion lives in step 3 against the field's
domain. Two cases pin it in both directions.

**P1-9 — `--graph` bypassed its own preflight.** The preflight checked the
default artifact; the gate then read the override. `--graph forged.json` gave
PASS followed by five steps measured against the forgery — a literal bypass of
the protection E-R1 had just introduced. Preflights are now bound with `--out`
to the path that will be read, and `--skip-preflight` is gone as a flag: a
documented skip is an invitation to make CI faster by deleting a check.

**P2 — the extractor was still partly transcription.** Message kinds, typed
supports and meta-target members were hand-kept constants beside a docstring
claiming nothing was hand-typed. The reviewer confirmed the values were correct;
they are now parsed from the document anyway, since a hand-kept copy of a frozen
set is a second registry, and this one decides what a `subject_refs` field may
carry.

**P2 — the preflight had no regression corpus.** Every case ran with
`--skip-preflight`, so P1-3's protection existed and nothing defended it. Four
preflight cases added: forged graph via `--graph`, hand-filled schema facts,
Phase G source blob mismatch, and a control asserting the real artifacts still
pass. 13 step cases + 4 preflight cases, **17/17**.

**Accepted from the review without change**, and recorded because they close
open questions rather than raising them:

```text
max_payload_bytes(kind) = 1 MiB for all eleven message kinds under current
    FD-1.4 — the sole exception is InteractionManifestV1, which is not the
    payload of any message kind. Recomputed if v2 supersedes those bounds.

fail-fast preflight is right: if the right-hand side is unproven, steps 1-5 are
    not extra diagnostics, they are numbers against an unknown ruler.

an explicitly chosen document ceiling is not architecturally worse than a
    derived one. If the number is normatively frozen and every implementation
    checks it, there is no drift; the derivable branch costs a constrained
    lexical JSON representation. The explicit ceiling is the likelier outcome
    absent a strong reason to introduce an encoding profile.
```

### E-R1 — first independent review, five P1s

All five accepted; two were verified against the frozen contract before being
accepted, since both turned on what A1 actually says rather than on taste.

**P1-1 — the gate checked the ledger against itself.** Steps 1, 2 and 3 all read
their left-hand side out of the ledger, so there was no schema-side input at all
and E-R0's "faithful realization" case passed by declaring the world it was
matched against. Worse, the reviewer showed E-R0's fourth synthetic case tested
the *easier* error: G-R12's failure mode is a schema that grows
`event_payload_digest` on a payload-free kind **with the ledger row omitted**,
and a ledger-only check is blind to that by construction. Repaired by §2.1's
three-way split and a committed test that reproduces the original failure mode.

**P1-2 — edge class had fallen out of identity.** `edge_key()` returned
`(source, target)`, so a mis-classed carrier passed. Now `(source, target,
class)`, with a mutation case. The meta-target expansion was also inverted,
admitting `(AnyCommittedEnvelope → member)` and thereby giving a target-position
union out-edges Phase G says it has none of; the extractor now asserts no
meta-target appears as a source.

**P1-3 — the frozen side was held on trust.** `graph.json` claimed to be
generated and no generator was committed, so the right-hand side was a second
editable registry. `tools/a1_v2_extract_graph.py` now derives it from the Phase G
document with the source pinned by blob (`450380ff`), and the gate runs
`--check` as a preflight that aborts the run on mismatch rather than reporting
five steps measured against a forged ruler.

**P1-4 — E-1's central claim was false.** Verified before accepting: FD-1.3 fixes
UTF-8, rejects a BOM and non-object roots, and states content is never
rewritten — and says nothing about insignificant whitespace, key order or number
form, while FD-1.2 deliberately made identity independent of serialization. So
bounded field values do not bound stored document bytes, which is what FD-1.8
sums into `ref.size`. E-R0 asserted derivability as a consequence of arithmetic
over the field set; it is available only under an encoding profile that
constrains lexical representation, which is a separate v2 decision. E-1 is
restated as *derived where derivable, explicitly chosen and justified where
not* — finite and protocol-hard either way.

**P1-5 — the dispositions reached past E-1.** `V1-N117` is a digest-domain row
closed under a size decision, in a revision whose own §6 said it decides no
digest domain; reverted to OPEN. `FD-1.8` is PARTIAL — size half consumed,
digest half open. `FD-1.4` is PARTIAL rather than "fully consumed": the rule is
decided, the value is owed.

**P2 — the discrimination tests were narrative.** Now
`tools/tests/test_a1_v2_ledger_gate.py`, twelve executable cases, 12/12 as
specified.

**Not disputed:** the S1 correction of §4 was independently confirmed against
blob `3b26849c`.

## 8. For the independent reviewer (revision E-R15)

The mechanism is now the object worth reviewing, and it is reviewable
independently of any field set — which is the argument for looking at it before
one grows on top.

1. **The schema extractor is a stub that returns the undeclared form.** Its
   `--check` therefore proves the committed facts are empty, not that they came
   from a schema. The trust model is fixed, but its first real test arrives with
   the first real schema. What should `--check` assert then — re-extraction
   equality, or something stronger, given the extractor and the schema will be
   written by the same person?
2. **Step 3 compares domains per path, with both sides authored by the drafter.**
   The schema side is now extractor-produced, which helps — but only once a
   schema exists. Is there a case where a *correct* extractor and a *correct*
   ledger still admit a wrong realization?
3. **Clause 5 as implemented.** A carrier's target is `AnyCommittedEnvelope`
   verbatim. That is faithful to the frozen text, but it means the ledger never
   names a concrete feed target anywhere. Does anything then check that a
   *runtime* `subject_refs` occurrence carries a member and not an arbitrary
   kind, or does that check now live only in the resolver?
4. **Answered, in the worst way available — see E-R6.** This asked which
   plausible v2 drafting error the corpus still missed, on the reasoning that
   its mutations and the gate shared one mental model. Codex found one within
   minutes of the PR leaving draft: modelling payloads as `ArtifactRef` fields
   instead of the structural digest exited 0. The question stands for whoever
   reads this next, now with a worked example of what it catches.
5. **Blob pinning — answered in E-R4, now open one level down.** §2.5 records
   the ceremony, and makes points 4 and 7 mechanical. Points 1–3 stay human by
   design: legitimacy is a Phase G act. The residual question is whether the
   *reference* in point 4 should be checkable rather than free text — today
   `superseding_authority` is a string nothing resolves, so the ceremony proves
   that an authority was named, not that it exists. What would make it
   resolvable without this floor re-deciding legitimacy?
6. **Answered in E-R6, and it was worse than recorded.** CodeRabbit's point
   was sharper than the one carried here: the skip emitted *no report line*, so
   a harness run was textually indistinguishable from a proven PASS, and an env
   var exported once in a shell disarms every later invocation in it. The
   recorded repair — lift steps 1–5 into an importable function — is done, and
   the variable is now inert with a regression pinning it.
9. **§7 (E-R13) is now a refutation surface: `step × dimension → verdict`.**
   Every cell is one of four claims — `PRESERVED`, `IRRELEVANT-BY-NORM`,
   `LOSSLESS-BY-PREMISE`, `AUTHORITY-UNDECIDED` — and each is falsifiable by a
   single witness. The job is not to re-read five hundred lines and feel uneasy.
   It is to find **one false cell**, and the cheapest refutation is a state pair
   the authority distinguishes and that cell claims the step does too.
   `AUTHORITY-UNDECIDED` cannot be refuted by argument, only closed by Phase G.

8. **Attack identity preservation, not mutations.** Three P1s in a row were
   one family (§7, E-R9): the verifier's observation granularity was weaker than
   the authority's subject. Walk `normative object → extracted key →
   multiplicity → comparison predicate → accepted witness`, and at each arrow
   ask only: *can two normatively distinct objects collapse into one state the
   verifier observes?* Look specifically for projection loss, role substitution
   and multiplicity collapse. Step 2's right-hand side has moved three times in
   three rounds and is the least trustworthy region of this gate; steps 1, 3, 4
   and 5 have had one author and no such pass.

7. **Answered from outside, in E-R11.** This asked whether duplicate
   `artifact_ref` carriers should be a defect rather than a no-op. They were a
   no-op: a byte-for-byte duplicate row collapsed under step 3's set union and
   steps 4/5's membership, so a seventy-row ledger passed against a sixty-nine
   edge graph. Asked here for two revisions and closed only when Codex supplied
   the witness — writing the question down is not the same as attacking it.
