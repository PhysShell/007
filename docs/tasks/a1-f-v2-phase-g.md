# A1-F v2 — Phase G: graph adjudication

**Status: DECIDED (revision G-R4) — AWAITING RE-REVIEW.**

Four review rounds. G-R1 moved the count from 13 to 11; G-R2 corrected the
binding lifecycle and the rank domain; G-R3 attempted the exact edge universe
and measured it against the wrong baseline; G-R4 rebuilds it on the frozen
contract. See §9.

Phase G is one decision, written and reviewed on its own, before any v2 drafting.
The node set determines ranks, edges, imported roots, closure and digest domains;
drafting v2 with the node set still open means drafting it twice.

## 0. Inputs and authority

```yaml
frozen_v1_baseline:
  commit: b84e9419e751179319925bbc57a434df3583a29a
  blob:   7db92f1b3dc9d7040da074956a0b3f2f200174c8
  role:   the model being superseded; its rules still bind the argument

design_input:
  commit: 37502e3ce5c397a7437445aafb88c13d84ba4ac0
  crate:  crates/o7-a1-protocol
  role:   EVIDENCE. Never authority.

implementation_probe:
  pr:     124 @ b2ba165
  role:   EVIDENCE for E-V0-4 only (see section 6)
```

**Prototype code is evidence, never a presumption.** The design input has fifteen
envelope kinds. That fact carries exactly as much authority as any other
observation about how somebody once wrote a Rust enum. The number this document
reaches is derived from consumers, not from a `#[derive]`.

## 1. Method

Default disposition for every existing v1 classification is `KEEP_V1_MODEL`. The
burden of proof lies on promoting a support object, introducing a node, changing
imported-root semantics, or replacing rank as the authority.

Per-object test, applied identically to every candidate:

```text
Does it independently require:
  1. logical message identity?
  2. causation / lineage?
  3. acceptance / idempotency lifecycle?
  4. independent replay addressability?
  5. controller / provider / human producer authority?
Insufficient answers -> it remains a support object.
```

The leading evidence is **in-degree**: which other objects reference it, from how
many distinct kinds, and across what edge class.

It is not sufficient evidence, and this document's own table shows why:
`WorkOrder` has in-degree 0 and is unambiguously a message — it is a round's
entry point, with its own lifecycle and identity. So zero degree establishes only

```text
no downstream reference-driven evidence for independent graph identity
```

and never "no identity needed". Where in-degree is zero, the second half of the
test — an independent lifecycle, acceptance or producer-authority requirement —
has to be answered on its own, not skipped because the first half came out
empty.

## 2. Evidence: the reference graph of the design input

Mechanically derived from `crates/o7-a1-protocol/src/edges.rs` at `37502e3`.
The metric needs a name and an exclusion rule, or an independent reviewer runs
"the same" script and gets a different number — which, after the 141-row
episode, would be a particularly avoidable way to repeat a lesson.

```text
registry total                                59 entries
  envelope-source edges          e(...)       53
  A2 attention-transition edges  t(...)        6
plus, in the registry KAT, one global rule:
  GLOBAL | causation.blob_ref | AnyCommittedEnvelope | Causal

specific V0 consumer in-degree(X) :=
  count of RETAINED V0 typed-node edges whose target is exactly X

  where a typed-node source is
    - an envelope-bearing message kind, OR
    - a typed support object / authority participating in the graph

  EXCLUDING
    - the 6 A2 transition-source edges          (not V0)
    - any open/generic target (AnyCommittedEnvelope, AnyImportableCas) —
      generic reachability is not evidence that a specific consumer needs
      this object
    - edges retained only for a POST-V0 feature (§3.5)
    - edges of objects that leave V0 (§3.4)

  "retained" is defined exactly by the V0 edge ledger of §4.1.
```

The definition was rewritten in G-R3. Until then it said "envelope-source edges",
which stopped being derivable the moment G-R1/G-R2 retyped three sources to
support: the numbers in the table were right, but they no longer followed from
the definition above them.

The open-target exclusion matters for honesty, not only for arithmetic: as a
prototype envelope kind, `ArtifactImported` is formally reachable through the
global causation rule, so its raw in-degree is not zero. Its *specific consumer*
in-degree is. The conclusion in §3.4 is unchanged, but it now rests on a metric
that says what it counts.

The table gives three separate quantities, because conflating them is how the
previous revision ended up with one table holding two projections and no labels.
`V0 out` and `V0 in` are over the retained V0 edge set (§4.1); `raw out` is the
prototype's own out-degree at `37502e3`, shown only where it differs.

| Candidate kind | V0 out | **V0 in** | raw out | referencing kinds |
|---|---:|---:|---:|---|
| `WorkOrder` | 6 | 0 | 6 | — (entry point; see §1 on why this is not evidence against message status) |
| `CoderReport` | 3 | 1 | 3 | CandidateAdmissionReceipt |
| `CandidateAdmissionReceipt` | 3 | 2 | 3 | ReviewRequest, … |
| `ReviewRequest` | 4 | 0 | 4 | — |
| `ReviewerReport` | 2 | 1 | 2 | ReviewVerdict |
| `ReviewVerdict` | 3 | 3 | 3 | incl. `ProviderInvocationReceipt` — a support node referencing a message |
| `CorrectiveDirective` | 3 | 0 | 3 | — |
| **`ProviderInvocationReceipt`** *(typed support)* | 6 | **3** | 9 | CoderReport, ReviewerReport, HumanAttentionRequest. Raw 9 includes the three SafeRedrive edges that §3.5 sends POST-V0 |
| **`InteractionManifest`** *(typed support)* | 3 | **1** | 3 | ProviderInvocationReceipt only |
| `CampaignFeedItem` | 1 | 0 | 1 | — |
| `HumanAttentionRequest` | 5 | 1 | 5 | — |
| `HumanCommandRequest` | 1 | 1 | 1 | HumanDecision |
| `HumanDecision` | 3 | 0 | 3 | — |
| **`CampaignRunBinding`** *(typed support)* | 5 | **2** | 5 | CandidateAdmissionReceipt, ProviderInvocationReceipt — both `Intra`, both controller-produced. A third raw in-edge, `safe_redrive.prior_run_binding_ref` (`Causal`), is excluded: its only consumer is POST-V0 (§3.1) |
| **`ArtifactImported`** *(leaves V0)* | — | **0** | 2 | **no specific consumer** (formally reachable only through the global open-target causation rule, which the metric excludes) |

External wrapper **raw explicit in-degree** — a deliberately different metric,
counted without the V0 exclusions so that POST-V0 references stay visible:
`CandidateStateReceipt` 6, `CandidateMaterialization` 3,
`WorktreeMaterialization` 2, `RunContractCandidateState` 2,
`RunArtifactSource` 1 (from `ArtifactImported` alone),
`EstablishedNonDispatchEvidence` 1 (from the SafeRedrive cause alone — a POST-V0
reference, which is exactly why this column is not the V0 metric).

## 3. Q1 + Q2 — node universe, and the envelope/support boundary

### 3.1 `CampaignRunBinding` → existence **REQUIRED_V0**, classification **TYPED SUPPORT AUTHORITY**

Existence was never the question: it is the only object bridging logical
campaign/round/role to physical execution/conversation/run/attempt plus the input
state actually materialized, and V0 needs that bridge.

Classification is a different question, and the first pass answered it with two
arguments that do not survive checking:

- **The `Causal` SafeRedrive edge cannot count.**
  `ProviderInvocationReceipt.cause.safe_redrive.prior_run_binding_ref` is the
  only cross-round in-edge, and SafeRedriveV2 is POST-V0 (§3.5). Using it as
  V0 promotion evidence lets a post-V0 consumer determine pre-V0 wire ontology,
  which is backwards.
- **"Two distinct producer lanes" is factually wrong.** `required_producer()` at
  `37502e3` (`edges.rs:538-541`) classifies `CoderReport` as `Provider("coder")`,
  `ReviewerReport` as `Provider("reviewer")`, `HumanCommandRequest` as `Human`,
  and *everything else*, including both `CandidateAdmissionReceipt` and
  `ProviderInvocationReceipt`, as `Controller`. Both referencing kinds are one
  lane.

What remains is specific V0 consumer in-degree **2**, both `Intra`, both
controller-produced. The envelope-specific question is then:

```text
Which V0 invariant requires of CampaignRunBinding
    a message_id, envelope causation/lineage, and an
    acceptance/idempotency lifecycle
that cannot be provided by
    typed support identity + the exact edge registry
    + a controller uniqueness invariant?
```

No such invariant was found. Content identity suffices, since it is referenced by
digest, and its lineage is carried in-band, which is exactly what a support
object is entitled to do (§3.2 explains why that is not a defect).

**Uniqueness, corrected.** G-R1 wrote that "one binding per execution" is
enforceable when the *referencing* artifact is accepted. That is the wrong
authority point, and wrong in the direction this project exists to avoid. The
design input allocates `run_id`/`attempt_id` durably *before* provider dispatch,
and the binding is the source of that physical identity, so the real order is:

```text
allocate execution E -> construct binding B -> PROVIDER DISPATCH
                                            -> ... later ...
                                            -> ProviderInvocationReceipt
                                            -> CandidateAdmissionReceipt
```

Checking uniqueness only when a later referencing artifact is accepted checks it
after the side effect has already happened. Two bindings for one execution could
both clear the pre-dispatch stretch, and rejecting one of them afterwards repairs
nothing. That is the exact inversion of R1's "durable acceptance before provider
invocation".

The invariant Phase G requires, without choosing an event schema for it:

```text
Pre-dispatch binding admission

Before the first provider dispatch for execution E, the controller durably
establishes exactly one binding identity B for E.

  E + same B       -> idempotent replay
  E + different B  -> conflict, fail closed, NO provider dispatch

After a restart, E resolves to the same B before the execution may continue.
```

Where that authority point lives — a campaign event, reducer state, or another
controller-owned durable record — is a v2 drafting decision, not a Phase G one.
What Phase G fixes is that it must exist and must precede dispatch.

**This is a controller/reducer lifecycle obligation and is not, by itself, a
reason to promote the binding to a message.** Requiring durable pre-dispatch
admission says the *controller* must hold state before acting; it says nothing
about the binding needing a `message_id`, envelope causation, or an acceptance
lifecycle of its own.

**Reopening trigger, recorded so it cannot be quietly skipped:** if the v2 draft
turns out to be unable to express pre-dispatch binding admission without giving
the binding a message lifecycle, this adjudication reopens. No present evidence
suggests it will.

Independently addressable graph authority is not the same thing as an
envelope-bearing message, and v1 already demonstrates the difference: the
execution receipt and the interaction manifest participate in the evidence DAG,
carry typed refs and are replay-addressable, without envelopes of their own.

**Disposition:** a new typed support authority in v2. Promotion remains available
if a later phase produces the envelope-specific invariant this one could not.

### 3.2 `ProviderInvocationReceipt` → **KEEP_V1_MODEL** (support object)

The first pass promoted it. The argument was wrong, and it is worth recording
exactly how, because the error is a general one.

**Withdrawn: the FD-5.3 argument.** The claim was that v1 forces the receipt to
violate FD-5.3 by carrying `campaign_id` and `round_id` in its own payload. It
does not. FD-5.3 governs *payloads of envelope-bearing artifacts* — "payloads
never restate an envelope-owned field". The receipt has no envelope by
construction, so those fields are not restating anything; they are its own
provenance.

**Withdrawn: "promotion removes two congruence predicates".** It does not. FD-11
checks `report.envelope.campaign_id == receipt.campaign_id` to prove the receipt
belongs to the same campaign and round as the report being accepted. That is
cross-object provenance congruence. After promotion it would simply read
`receipt.envelope.campaign_id` — the obligation survives verbatim. The first pass
mistook a cross-object equality proof for intra-object denormalization. Those are
different things, and one of them is not a defect at all.

**Insufficient on its own: in-degree 3.** A content-addressed support object may
have any number of consumers, and v1 demonstrates exactly that. Being referenced
is not being a message.

No envelope-specific invariant remains. Under this document's own default rule,
the disposition is `KEEP_V1_MODEL`: the receipt stays a support object, and the
frozen classification stands.

### 3.3 `InteractionManifest` → **KEEP as a typed support object**

In-degree 1, from `ProviderInvocationReceipt` alone. No consumer outside its own
receipt, no independent acceptance, and its identity is meaningful only relative
to the execution it describes. Tests 1, 3 and 4 fail. Promotion in the design
input is symmetry with the receipt, not a consumer.

That symmetry is precisely what the burden of proof exists to refuse. Two objects
that appear together in one paragraph are not thereby the same kind of thing.

### 3.4 `ArtifactImported` → **NOT PROVEN — out of A1-V0**

**Specific V0 consumer in-degree = 0** (§2). No V0 object references it; its only
edges are outgoing, to `AnyImportableCas` and to `RunArtifactSource`. Its *raw*
degree is not zero — the global open-target causation rule formally reaches it —
which is why the metric is named rather than assumed.

Three questions were kept separate, and only the first two are answered here:

```text
run -> CAS import operation needed?     plausibly YES  (mechanism)
durable import proof needed?            plausibly YES  (mechanism)
independent graph node needed?          NOT PROVEN
envelope-bearing message needed?        NOT PROVEN
```

An import mechanism can exist as a controller procedure whose product is an
ordinary CAS object with a recorded provenance, without minting a node.

v1's refusal of `ArtifactAcceptance` (FD-2.3) is worth remembering here as a
**caution against speculative nodes**, and not as proof: the two objects do the
same thing to a graph but not the same thing to a system, and treating the
resemblance as identity would import a conclusion instead of an argument. The
disposition rests on two findings together, since §1 establishes that the first
alone is not sufficient:

```text
specific V0 consumer in-degree = 0
AND no independent V0 message identity, lifecycle, acceptance or producer
    invariant has been demonstrated for it
=> node / message status NOT PROVEN
```

The second conjunct is what separates this from `WorkOrder`, which also has
in-degree 0 and is a message on the strength of exactly that second test.

Consequence: `RunArtifactSource`, whose only in-edge comes from
`ArtifactImported`, leaves the V0 wrapper set with it.

### 3.5 `EstablishedNonDispatchEvidence` → **POST_V0_DEPENDENT_ON_SAFE_REDRIVE**

In-degree 1, exclusively from `ExecutionCause::SafeRedrive`. With SafeRedriveV2
deferred, it has no V0 consumer. The design is preserved as a prepared record for
v2.1 rather than discarded.

This carries a standing condition, recorded because it is exactly the kind of
thing a word can quietly take with it: **the FD-14.7a barrier is MUST-V0 and is
unaffected.** Post-dispatch ambiguity safety is not automatic pre-dispatch
redrive. Deferring `EstablishedNonDispatchEvidence` defers the *resolution path*,
never the *barrier*.

### 3.6 Result — the v2 node universe

```text
envelope-bearing kinds: 11
  the eleven v1 kinds, unchanged — KEEP_V1_MODEL on the boundary question

typed support objects / authorities:
  + CampaignRunBinding          (3.1, REQUIRED_V0, new support authority)
    ProviderInvocationReceipt   (3.2, kept — promotion argument withdrawn)
    InteractionManifest         (3.3, kept)
    ScopeContractV1             (v1, unchanged)
    CampaignEventPayload        (v1, unchanged)

out of V0:
    ArtifactImported            (3.4, not proven)
    RunArtifactSource           (3.4, falls with it)
    EstablishedNonDispatchEvidence (3.5, post-V0)
```

**Eleven.** The envelope boundary of the frozen contract survives adjudication
unchanged, and V0 gains one new object that is an authority without being a
message.

The first pass reached thirteen. It got there by promoting two objects on
arguments that did not survive checking (§3.1, §3.2) — and the shape of that
error is worth keeping in view, because both promotions were reached after
reading a fifteen-variant enum. `KEEP_V1_MODEL` was declared the legitimate
default at the top of this document; the discipline is only real when the default
actually wins something.

## 4. Q3 — edge and rank model

**The exact edge registry becomes authoritative. Rank becomes derived.**

Evidence from v1's own history rather than from preference: rank had to be
redefined twice under pressure. R5.1 restated rank 0 from "opaque bytes" to
"terminal in the reference graph" for `ScopeContractV1`, and FD-2.4 had to carve
imported roots into it separately. A property that needs redefining every time a
new object arrives is behaving like a derived quantity being used as a primitive.

Frozen for v2:

```text
authority:  a closed per-kind allowed-edge registry — exact source, exact
            field-path tag, exact target, edge class

derived_rank :=
    topological level over the closed INTRA A1 TYPED-NODE subgraph, where the
    typed nodes are:
      - envelope-bearing message kinds, and
      - typed support objects / authorities that participate in the graph
        (ProviderInvocationReceipt, InteractionManifest, CampaignRunBinding,
         ScopeContract, CampaignEventPayload)
    Graph-terminal external wrapper and CAS targets terminate traversal.

    No global rank is defined over Causal edges, and none can be: the full
    graph does not topologically sort, because cross-round causal references
    are acyclic per instance rather than per kind.
    Retained as a checkable property and as review shorthand, never as the rule.

acyclicity: Intra   — proved by topological sort over the typed-node subgraph
            Causal  — proved per instance by create-before-reference
            never asserted from rank monotonicity
```

Two rounds of narrowing, in opposite directions, and the second is the one worth
explaining. The first pass wrote "rank, computable from the registry", which is
too wide. G-R1 then narrowed it to the *message-kind* subgraph, which is too
narrow — and wrong precisely because of what G-R1 had just decided. The three
demoted objects are not terminals: `ProviderInvocationReceipt` has out-degree 9,
`CampaignRunBinding` 5, `InteractionManifest` 3. A chain like

```text
CoderReport [message] -> ProviderInvocationReceipt [support]
                           |-> InteractionManifest [support]
                           `-> CampaignRunBinding  [support] -> External / CAS
```

would fall out of the proof entirely if the domain were messages only. Frozen v1
had this right and is the precedent: its ranks put `InteractionManifestV1` at 1
and `ProviderExecutionReceiptV1` at 2, beneath the reports — a ladder over the
whole typed reference graph, not over envelopes.

Consequence for the registry: a typed support object must be admissible as an
edge **source**, not only as a target. §4.1 discharges that, and closes the
mandate's "exact registry" clause.

Deciding an object is not a message does not make it stop being a node. Naming
the two classes separately is what makes that mistake visible instead of
structural.

The registry's two edge classes are adopted as they stand: `Intra` (within one
round's derivation flow, must topologically sort at kind level) and `Causal`
(crossing rounds, chains or attention lineage; instance-acyclic by
create-before-reference). The distinction is load-bearing and rank cannot
express it at all, which is independent evidence for this change: the two classes
are proved acyclic by different arguments — kind-level topological sort versus
per-instance create-before-reference — and a single scalar cannot carry two proof
obligations. (G-R1's earlier justification, that §3.1's promotion turned on a
`Causal` edge, died with that promotion and is not reused here.)

One open target is sanctioned, unchanged from the design input's rationale:
`AnyCommittedEnvelope`, for `CampaignFeedItem` causation. `AnyImportableCas`
leaves with `ArtifactImported` (§3.4).

### 4.1 The exact V0 edge universe

**G-R3's ledger measured against the wrong side, and this subsection replaces
it.** It enumerated all 59 rows of the prototype registry and labelled them
`KEEP` / `RETYPE` / `POST_V0` / `REMOVE` / `A2`. Every one of those labels is
relative to `37502e3`, so `KEEP` meant *unchanged relative to the prototype* —
while §0 of this document says the prototype is evidence and never authority,
and §1 makes `KEEP_V1_MODEL` the default. The registry, of all things, was
derived from the one input that has no authority.

The scale is not marginal. Matching every prototype tag against the frozen
schemas by name:

```text
prototype envelope-source rows                         53
  with a frozen v1 slot of the same name               15
  with NO frozen counterpart                           38
```

So 30 rows were labelled `KEEP` when the frozen contract does not contain them
at all. The clearest single case: `WorkOrder.goal.contract_blob → ContractBlob`
was `KEEP`, and the string `contract_blob` appears **zero** times in blob
`7db92f1b`. Frozen `WorkOrderV1` carries `scope_ref → ScopeContractV1` instead.
That is not an unchanged edge; it is a v1 surface removed and a prototype
surface proposed in its place, and the two need different words.

Two further consequences of the same error:

- `ProviderInvocationReceipt → CampaignRunBinding` was labelled `RETYPE`, but
  `CampaignRunBinding` does not exist in v1 at all. It cannot be a retyping of
  something that was never there; §3.1 argued its existence precisely because it
  is **new**.
- The prototype branch never implemented the reducer, so its registry has zero
  rows for `CampaignEventV1` and its payloads — while §4 of this document lists
  `CampaignEventPayload` among the typed nodes participating in the graph. A
  registry claiming to be closed while missing an entire declared node class is
  not closed.

#### 4.1.1 The baseline: the frozen v1 reference inventory

Extracted mechanically from blob `7db92f1b` — every schema row whose type is
`ArtifactRef` or `[ArtifactRef]`, across §3.0–§3.15.2.

```text
frozen ArtifactRef-valued slots                        40
  message payloads                                     18
  typed support objects                                11
  event payload schemas                                 7
  common envelope                                       2
  campaign event log root                               2
```

A first extraction pass returned 34 and a completeness check found six more,
carried in combined table rows (`usage_ref` / `cost_ref`, and the two
`interaction_sequence[]` pairs). The number is 40, and the miss is recorded
because an inventory whose own derivation was not checked is the failure mode
this track has now hit three times.

| # | owner | owner class | frozen field path | V0 disposition |
|---:|---|---|---|---|
| 1 | `Common envelope v1` | envelope | `artifact_refs` | KEEP |
| 2 | `Common envelope v1` | envelope | `provider_execution_receipt_ref` | KEEP |
| 3 | `WorkOrderV1` | message | `input.candidate_ref` | KEEP |
| 4 | `WorkOrderV1` | message | `input.materialization_attestation_ref` | KEEP |
| 5 | `WorkOrderV1` | message | `scope_ref` | KEEP |
| 6 | `CoderReportV1` | message | `claims[].evidence_refs` | KEEP |
| 7 | `CoderReportV1` | message | `diagnostic_runs[].artifact_ref` | KEEP |
| 8 | `CandidateReceiptV1` | message | `candidate_ref` | KEEP |
| 9 | `CandidateReceiptV1` | message | `coder_report_ref` | KEEP |
| 10 | `ReviewRequestV1` | message | `candidate_receipt_ref` | KEEP |
| 11 | `ReviewRequestV1` | message | `scope_ref` | KEEP |
| 12 | `ReviewRequestV1` | message | `evidence_refs` | KEEP |
| 13 | `ReviewRequestV1` | message | `coder_report_ref` | KEEP |
| 14 | `ReviewerReportV1` | message | `findings[].evidence_refs` | KEEP |
| 15 | `ReviewVerdictV1` | message | `reviewer_report_ref` | KEEP |
| 16 | `CorrectiveDirectiveV1` | message | `review_verdict_ref` | KEEP |
| 17 | `CorrectiveDirectiveV1` | message | `scope_ref` | KEEP |
| 18 | `CampaignFeedItemV1` | message | `subject_refs` | KEEP |
| 19 | `HumanAttentionRequestV1` | message | `evidence_refs` | KEEP |
| 20 | `HumanDecisionV1` | message | `command_request_ref` | KEEP |
| 21 | `ProviderExecutionReceiptV1` | typed support | `interaction_manifest_ref` | KEEP |
| 22 | `ProviderExecutionReceiptV1` | typed support | `final_normalized_output_ref` | KEEP |
| 23 | `ProviderExecutionReceiptV1` | typed support | `canonical_request_ref` | KEEP |
| 24 | `ProviderExecutionReceiptV1` | typed support | `raw_provider_event_ref` | KEEP |
| 25 | `ProviderExecutionReceiptV1` | typed support | `normalized_output_ref` | KEEP |
| 26 | `CampaignEventV1` | event log root | `source_ref` | KEEP |
| 27 | `CampaignEventV1` | event log root | `evidence_refs` | KEEP |
| 28 | `Event payload schemas` | event payload | `scope_ref` | KEEP |
| 29 | `Event payload schemas` | event payload | `receipt_ref` | KEEP |
| 30 | `Event payload schemas` | event payload | `results[].log_ref` | KEEP |
| 31 | `Event payload schemas` | event payload | `checks[].observation_ref` | KEEP |
| 32 | `Event payload schemas` | event payload | `detail_ref` | KEEP |
| 33 | `Event payload schemas` | event payload | `termination_observation_refs` | KEEP |
| 34 | `Event payload schemas` | event payload | `evidence_refs` | KEEP |
| 35 | `ProviderExecutionReceiptV1` | typed support | `dispatches[].usage_ref` | KEEP |
| 36 | `ProviderExecutionReceiptV1` | typed support | `dispatches[].cost_ref` | KEEP |
| 37 | `InteractionManifestV1` | typed support | `interaction_sequence[].input_ref` | KEEP |
| 38 | `InteractionManifestV1` | typed support | `interaction_sequence[].output_ref` | KEEP |
| 39 | `InteractionManifestV1` | typed support | `interaction_sequence[].arguments_ref` | KEEP |
| 40 | `InteractionManifestV1` | typed support | `interaction_sequence[].result_ref` | KEEP |

**Disposition.** Every frozen slot is `KEEP` under §1's default: no adjudication
in §3 removes a v1 reference surface. What §3 changed is the *node* set, and its
effect on this inventory is narrower than G-R3 implied:

```text
ProviderExecutionReceipt, InteractionManifest
    already typed support in v1 -> the retypings of G-R3 restore v1's own
    classification rather than change it, so their slots are unaffected

CampaignRunBinding
    new in v2 (§3.1) -> adds edges, removes none

ArtifactImported, RunArtifactSource, EstablishedNonDispatchEvidence
    absent from v1 -> nothing to remove from this inventory; the prototype rows
    that carried them are proposals that are not adopted (§4.1.2)
```

#### 4.1.2 The prototype rows, demoted to evidence

The 59 rows keep their value as evidence — they are how §3's in-degree arguments
were made — but they are classified against the baseline, not as the baseline:

```text
MATCHES_V1              15   a frozen slot exists with the same name
PROPOSED_REPLACEMENT         a prototype surface offered for a frozen one
PROPOSED_NEW                 a surface with no frozen counterpart
POST_V0                  3   the SafeRedrive path (§3.5)
REJECTED                 2   ArtifactImported's own edges (§3.4)
A2                       6   attention transitions, outside V0
```

The middle two classes together hold the 38 unmatched rows, and splitting them
row by row is **not a Phase G decision**. A `PROPOSED_REPLACEMENT` is a claim
about what a v2 payload should carry — `contract_blob` versus `scope_ref`,
whether `CorrectiveDirective` carries its own input state, whether
`HumanCommandRequest` gains an authenticated-principal reference. Those are
payload-schema questions, and §7 of this document already says payload shapes
are v2 drafting output.

#### 4.1.3 What Phase G closes, and the boundary it cannot cross

There is a real circularity in "close the exact registry": an exact registry is
keyed by field path, field paths are payload schema, and payload schemas are the
v2 draft. Phase G cannot write field paths for objects whose payloads it does
not decide without either adopting the prototype's schemas wholesale — which is
the authority inversion this subsection just corrected — or inventing v2 schemas
under the heading of graph adjudication.

So the registry closes at the level that actually determines what Phase G owns —
acyclicity, the rank domain, and closure traversal — and no further:

```text
CLOSED by Phase G — the admissible node-pair universe

  message        -> message | typed support | external wrapper | CAS
  typed support  -> message | typed support | external wrapper | CAS
  event log root -> message | typed support | event payload | CAS
  event payload  -> typed support | external wrapper | CAS
  external wrapper, CAS                        terminal, no outgoing A1 edges

  edge classes   Intra (kind-level topological sort)
                 Causal (per-instance create-before-reference)

  the 40 frozen slots above are the V0 edge set, plus exactly the edges
  §3.1's new object requires and nothing else

OWED to the v2 draft — and owed as an obligation, not a hope

  the field-path spelling of every slot, including CampaignRunBinding's own
  each of the 38 unmatched prototype rows adjudicated REPLACEMENT / NEW / REJECT
  the resulting registry re-checked against this node-pair universe
```

`typed support -> message` is in the list on evidence, not symmetry: frozen
`ProviderExecutionReceiptV1` has no such slot, but §3.1's own argument used
`ProviderInvocationReceipt.execution_cause.prior_verdict_ref → ReviewVerdict`,
and if v2 adopts that proposal the pair must already be admissible. It is the
one pair whose admission comes from a proposal rather than from the frozen
inventory, and it is flagged so the v2 draft cannot treat it as settled by
silence.

#### 4.1.4 `CampaignEventV1` is a source-only log root

The frozen contract does not make the campaign event an `ArtifactRef` target,
and nothing should. Frozen:

```text
CampaignEventV1     persisted log entry; source of source_ref and evidence_refs;
                    identified by its own framed digest and its chain position;
                    NEVER the target of an ArtifactRef

CampaignEventPayload  typed stored object; its own outgoing ref slots are the
                    seven in §4.1.1; a legitimate ArtifactRef target
```

This is a Phase G question rather than a drafting one because it decides whether
the closure resolver walks event references at all. It does: FD-14.2's
`resolve_event` enumerates `immediate_refs` over `source_ref`, `evidence_refs`
and the payload's declared slots, under the same bounds as artifact closure.
Event references are traversed exactly as deterministically as artifact
references, and the log root's exclusion from target position is what keeps that
traversal a DAG rather than a cycle through the log.

#### 4.1.5 The renamings are a supersede decision, not a script artefact

`CandidateReceipt` versus `CandidateAdmissionReceipt`, and
`ProviderExecutionReceipt` versus `ProviderInvocationReceipt`, are different
spellings of the same object, and FD-1.9 closes `ArtifactKindV1` spellings as a
digest input. G-R3 used the prototype's names throughout its ledger without
noticing that it was thereby proposing a wire change.

Phase G records the choice as open and owed:

```text
if v2 keeps the frozen spellings   no version consequence
if v2 adopts the prototype names   an explicit supersede of FD-1.9, with the
                                   envelope_version bump that already applies,
                                   recorded as a decision rather than inherited
                                   from whichever branch a script read
```

Phase G does not decide it. Phase G refuses to let it happen by accident.

## 5. Q4 + Q5 — imported roots, closure, and the typed external boundary

### 5.1 The wrapper classification: `PARSED_BUT_GRAPH_TERMINAL`

v1's FD-2.4 says imported roots are *never parsed by A1*, while every typed A0/R1
wrapper in the design input is parsed by A1 — `CandidateStateReceiptRefV1` reads
a `source_run_id`, checks `ArtifactKind::CandidateState`, and carries an exact
`RunEventId`. Both cannot be true. This is the same seam R5.1 hit with
`ScopeContractV1` and repaired by redefining rank 0; it recurs because the
underlying distinction was never named.

Frozen for v2:

```text
PARSED_BUT_GRAPH_TERMINAL

A1 owns and validates the wrapper's syntax and local invariants.
A1 may carry enough foreign identity to select an exact A0/R1 fact.
Semantic validity of that fact remains owned by the A0/R1 verifier.
Cross-layer resolution proves the boundary relation and nothing beyond it.
The foreign authority does not become an A1 graph node, and its internal
references are never traversed as A1 edges.
```

So "terminal" means terminal *in the graph*, and says nothing about parseability;
parseability is decided per slot. `OPAQUE_TERMINAL` remains available for any
future import that A1 genuinely hands off unread, and `OTHER` requires explicit
new justification.

V0 wrapper set, after §3.4 and §3.5 remove two:

```text
CandidateStateReceiptRef        in-degree 6
CandidateMaterializationRef     in-degree 3
WorktreeMaterializationRef      in-degree 2
RunContractCandidateStateRef    in-degree 2
```

### 5.2 Closure semantics

Unchanged from v1 in substance, with one clarification the above forces:

```text
closure traversal enters a wrapper to validate it, and stops there.
It never follows the foreign authority's own internal references.
Accounting charges the wrapper, not the foreign object graph behind it.
```

The FD-1.5 bounds, the deduplication key `(kind, digest)`, declared-size-before-read
accounting and all-or-nothing rejection are untouched by Phase G.

## 6. E-V0-4 — the part that belongs to Phase G, and the part that does not

E-V0-4 has two halves. Only one is a taxonomy question.

**In scope — the manifest's class, and nothing else.** §3.3 settles it: a
**typed support object, not envelope-bearing**. That is the whole of Phase G's
mandate here.

**Out of scope — every number.** The first pass went one step further and
concluded that the class implies the bound: typed support object, therefore the
1 MiB control-artifact bound, therefore `manifest` leaves the evidence-blob list.
That does not follow. Class and size are orthogonal — a support object may carry
its own per-kind hard bound, and the design input does exactly that, giving the
manifest 2 MiB. Deciding the bound from the class would have been Phase G quietly
amending FD-1.4, which is the failure this section was written to name and then
committed anyway.

So all of it defers to v2 wire/bounds drafting, together with the envelope-size
seam:

```text
is the manifest 1 MiB, 64 MiB, or a per-kind bound of its own?
does MAX_INTERACTION_SEQUENCE change instead?
what bounds stored envelope bytes, given FD-1.8 sums envelope + payload?
```

Phase G supplies the classification these questions need and answers none of
them.

`E-V0-1`, `E-V0-2` and `E-V0-3` are **not** Phase G material at all. They are
convergence inputs about the checked-parse boundary, scalar bounds and
repository-relative validation. Pulling them in because they sit nearby in
`e202ac1` would turn Phase G into "let us also rewrite the wire layer", which is
the failure this phase exists to prevent.

## 7. What Phase G did not decide

- envelope v2 field set, framing order, or any digest domain;
- `message_kind_version` per kind — an output of v2 drafting, once payload shapes
  are known;
- the envelope-size bound (§6);
- dispositions for any of the 141 negative-matrix rows or the 47 frozen decisions;
- anything about `E-V0-1`, `E-V0-2`, `E-V0-3`;
- whether the import mechanism of §3.4 exists as a controller procedure — only
  that it does not exist as a node.

## 8. For the independent reviewer (revision G-R4)

Node universe, the count of eleven, the support boundary, the binding lifecycle
class, the wrapper boundary and the rank model have all been approved. What is
left is §4.1. Attack these, in order:

0. **§4.1.1, the frozen inventory.** 40 slots, derived from blob `7db92f1b`, and
   the derivation was wrong once already (34, then six more found in combined
   rows). Re-derive it independently. If the count differs, everything built on
   it differs.
1. **§4.1.3, the boundary claim.** Phase G closes the node-pair universe and
   hands field-path spelling to the v2 draft, arguing that an exact registry is
   keyed by field path and field paths are payload schema. Is that a real
   circularity or a second deferral wearing an argument? The strongest attack is
   to show a field path Phase G *could* have fixed without deciding a payload.
2. **§4.1.3's `typed support -> message` pair.** It is admitted on the strength
   of a prototype proposal rather than a frozen slot. Should an unadopted
   proposal be able to widen the admissible universe at all?

1. **§3.1, `CampaignRunBinding` as support rather than message.** The negative is
   the load-bearing claim: *no* V0 invariant needs a `message_id`, envelope
   causation and an acceptance lifecycle here. One counterexample overturns it.
   G-R2 already found the near miss — binding uniqueness had been placed after
   dispatch — and repaired it as a controller obligation rather than a promotion.
   Is that repair sufficient, or does durable pre-dispatch admission smuggle in a
   message lifecycle under another name?
2. **§3.2, `ProviderInvocationReceipt` staying a support object.** Same shape: is
   there an envelope-specific invariant that neither pass has looked for?
3. **§2, the metric.** `specific V0 consumer in-degree` now has an explicit
   exclusion rule. Re-derive it independently and check the numbers — that is
   what the previous round's exclusions were missing.
4. **§3.4, `ArtifactImported` as NOT PROVEN.** Zero specific-consumer degree is
   strong, but the design input is a prototype: is there a real V0 import
   consumer merely unwired, which would make this premature?
5. **§4, derived rank over the Intra subgraph only.** Does anything need a rank
   across Causal edges, which by construction cannot have one?

## 9. Revision record

### G-R4 — fourth independent review, one P1

`CHANGES_REQUESTED` on a single finding, and it landed on the one place where
this document had stopped applying its own rule.

**§4.1 used the prototype as baseline.** G-R3 enumerated all 59 prototype rows
and labelled them `KEEP` / `RETYPE` / `POST_V0` / `REMOVE` / `A2` — every label
relative to `37502e3`. So `KEEP` meant "unchanged relative to the prototype",
while §0 calls the prototype evidence and never authority and §1 makes
`KEEP_V1_MODEL` the default. Three rounds spent defending that principle, and
then the registry — the most exact artefact in the document — was derived
entirely from the input with no authority.

Quantified during the fix: of 53 prototype envelope-source rows, **15** have a
frozen slot of the same name and **38** have none. Thirty rows were labelled
`KEEP` for edges the frozen contract does not contain. `WorkOrder.goal
.contract_blob` is the plainest: `contract_blob` occurs zero times in blob
`7db92f1b`, where the frozen `WorkOrderV1` carries `scope_ref → ScopeContractV1`.
`ProviderInvocationReceipt → CampaignRunBinding` was `RETYPE` for an object that
did not exist in v1 to be retyped. And the prototype branch never implemented
the reducer, so its registry has no rows at all for `CampaignEventV1` and its
payloads, which §4 simultaneously lists among the participating typed nodes.

Rebuilt in G-R4:

- **§4.1.1** — the baseline is now the frozen reference inventory: 40
  `ArtifactRef`-valued slots extracted from blob `7db92f1b`. The first pass
  returned 34; a completeness check found six more in combined table rows. The
  miss is recorded, since an inventory whose own derivation went unchecked is a
  failure this track has now hit three times.
- **§4.1.2** — the 59 prototype rows are demoted to evidence with their own
  classes, and the 38 unmatched rows are explicitly *not* adjudicated here:
  `contract_blob` versus `scope_ref` is a payload-schema question, and §7
  already assigns payload shapes to v2 drafting.
- **§4.1.3** — the boundary is stated rather than deferred. An exact registry is
  keyed by field path; field paths are payload schema; Phase G does not decide
  payload schemas. So G closes the admissible **node-pair universe** — which is
  what actually determines acyclicity, the rank domain and closure traversal —
  and records the field-path spelling as an obligation owed by the v2 draft,
  with the 38 unmatched rows named as its input.
- **§4.1.4** — `CampaignEventV1` is frozen as a source-only log root, never an
  `ArtifactRef` target, with its payload as a legitimate target. This is a Phase
  G question because it decides whether the closure resolver walks event
  references, and it does, under the same bounds as artifact closure.
- **§4.1.5** — `CandidateReceipt` versus `CandidateAdmissionReceipt` and
  `ProviderExecutionReceipt` versus `ProviderInvocationReceipt` are recorded as
  an open supersede decision. FD-1.9 closes `ArtifactKindV1` spellings as a
  digest input, so G-R3's silent use of the prototype's names was proposing a
  wire change by transcription. Phase G does not choose the names; it refuses to
  let them be chosen by which branch a script happened to read.

Nothing in §3 moved. The node universe, the count of eleven,
`CampaignRunBinding` as typed support authority, and the rank model are all as
G-R2 left them and as the third review approved them.

### G-R3 — third independent review, two P1s

`CHANGES_REQUESTED`; the node universe, the count of eleven, and
`CampaignRunBinding` as a typed support authority were all accepted, as was the
finding that pre-dispatch admission does not smuggle in a message lifecycle.
Both remaining findings accepted.

1. **Phase G had not closed its own Q3.** The mandate asks for the *exact
   registry*; G-R1 and G-R2 supplied a model and deferred the registry to v2
   drafting. That is not a small gap: after three retypings and one removal the
   prototype registry cannot serve as the v2 registry even mechanically, so the
   draft would have inherited the right to decide the graph under the heading
   "concrete shape". §4.1 now carries all 59 entries classified `KEEP` /
   `RETYPE` / `POST_V0` / `REMOVE` / `A2`, giving a retained V0 universe of 48.

   Deriving it surfaced two things the prose had missed: the SafeRedrive path is
   three edges rather than one, and
   `ProviderInvocationReceipt.execution_cause.prior_verdict_ref → ReviewVerdict`
   is a support node referencing a message — which is the sharpest available
   argument for G-R2's own rank domain.

2. **The consumer-degree metric had drifted from the ontology.** It still read
   "envelope-source edges", which stopped being derivable the moment G-R1/G-R2
   retyped three sources to support. The numbers were right; they no longer
   followed from the definition printed above them. Rewritten over retained
   typed-node edges, with `V0 out`, `V0 in` and `raw out` as three labelled
   columns instead of one table quietly holding two projections.

Tail, and the sharpest of the round: §1 claimed "an object nothing references
does not need an identity", and the table one page later refutes it —
`WorkOrder` has in-degree 0 and is plainly a message. Zero degree establishes
only the absence of reference-driven evidence; the lifecycle half of the test
still has to be answered. `ArtifactImported` now rests on both conjuncts
explicitly, which is what actually distinguishes it from `WorkOrder`.

Note carried to the v2 draft, not a Phase G decision: do not identify the
abstract binding identity `B` with a blob digest in advance. Phase G left digest
domains open deliberately, and a canonical-writer version change can make byte
identity and semantic binding identity separate questions.

### G-R2 — second independent review, two P1s

`CHANGES_REQUESTED`; the central conclusion of eleven envelope kinds was not
challenged and is unchanged. Both findings accepted.

1. **Binding uniqueness was enforced too late.** G-R1 delegated "one binding per
   execution" to the acceptance of a later referencing artifact — after the
   provider dispatch that the binding exists to identify. Two bindings for one
   execution could both clear the pre-dispatch stretch, and a post-factum
   rejection repairs nothing. Replaced by a **pre-dispatch binding admission**
   invariant: exactly one binding identity established durably before the first
   dispatch, same-B replay idempotent, different-B a fail-closed conflict with no
   dispatch, and the same resolution after restart. Recorded explicitly as a
   controller/reducer obligation that is *not* a reason to promote, with a
   reopening trigger if the v2 draft cannot express it without a message
   lifecycle.
2. **Derived rank was defined over the wrong graph.** G-R1 narrowed it to the
   `Intra` *message-kind* subgraph, one paragraph after demoting three objects
   that are not terminals — `ProviderInvocationReceipt` (out-degree 9),
   `CampaignRunBinding` (5), `InteractionManifest` (3). Half the receipt chain
   would have fallen out of the acyclicity proof. Widened to the `Intra` **typed-node**
   subgraph, following frozen v1's own precedent, where rank 1 was the manifest
   and rank 2 the receipt. Consequence recorded: a typed support object must be
   admissible as an edge source, not only as a target.

Tails: `ArtifactImported` now reads *specific V0 consumer in-degree = 0* rather
than a bare "in-degree zero"; the external wrapper column is labelled as raw
explicit degree, a deliberately different metric that keeps POST-V0 references
visible; and §4's justification no longer cites the promotion G-R1 cancelled.

The characteristic error this round: having correctly decided an object is not a
message, the document nearly forgot it is still a node — and still a pre-dispatch
authority. Which is a fair demonstration of why the two classes needed separate
names in the first place.

### G-R1 — first independent review, four P1s

`CHANGES_REQUESTED` on the first pass. All four accepted; the central conclusion
changed from thirteen envelope kinds to eleven.

1. **Evidence accounting was imprecise.** "53 edges" named the envelope-source
   projection, not the registry, which holds 59 entries plus a global
   `AnyCommittedEnvelope` causation rule. §2 now defines *specific V0 consumer
   in-degree* with an explicit exclusion rule, so an independent reviewer running
   "the same" script arrives at the same numbers. Raw in-degree for
   `ArtifactImported` is not zero; its specific-consumer degree is.
2. **`CampaignRunBinding`'s promotion evidence did not hold.** The `Causal`
   in-edge belongs to SafeRedrive, which is POST-V0 — letting it decide V0 wire
   ontology is backwards. And "two producer lanes" was simply false:
   `required_producer()` classifies both referencing kinds as `Controller`.
   Re-adjudicated as a typed support authority.
3. **`ProviderInvocationReceipt`'s promotion argument was wrong.** FD-5.3
   governs payloads of envelope-bearing artifacts; the receipt has none, so its
   `campaign_id`/`round_id` restate nothing. And FD-11's two congruence
   predicates survive promotion unchanged, because they prove cross-object
   provenance rather than police intra-object duplication. Reverted to
   `KEEP_V1_MODEL`.
4. **E-V0-4 drifted from taxonomy into bounds.** Class does not imply size — the
   design input gives the manifest its own 2 MiB bound — so concluding "typed
   support object, therefore 1 MiB, therefore out of the evidence-blob list" was
   Phase G amending FD-1.4 one paragraph after warning against exactly that. All
   numeric resolution deferred.

Also: derived rank narrowed to the `Intra` kind-level subgraph, since the full
graph does not topologically sort; and the `ArtifactAcceptance` analogy demoted
from proof to caution.

The pattern behind items 2 and 3 is one error, not two. The script proved
correctly *who references whom*; the document then took one step further and
treated "is referenced" as "is a message". Being referenced is a property of the
graph. Being a message is a claim about identity, lifecycle and acceptance, and it
has to be argued separately every time.
