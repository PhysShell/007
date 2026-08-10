# A1-F v2 — Phase G: graph adjudication

**Status: DECIDED — AWAITING INDEPENDENT REVIEW.**

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

The decisive evidence is **in-degree**: which other objects reference it, from
how many distinct kinds, and across what edge class. An object nothing references
does not need an identity, whatever its schema looks like.

## 2. Evidence: the reference graph of the design input

Mechanically derived from `crates/o7-a1-protocol/src/edges.rs` at `37502e3` —
53 edges, counted by script rather than read off by eye.

| Candidate kind | out | **in** | referencing kinds |
|---|---:|---:|---|
| `WorkOrder` | 6 | 0 | — (round entry point) |
| `CoderReport` | 3 | 1 | CandidateAdmissionReceipt |
| `CandidateAdmissionReceipt` | 3 | 2 | ReviewRequest, … |
| `ReviewRequest` | 4 | 0 | — |
| `ReviewerReport` | 2 | 1 | ReviewVerdict |
| `ReviewVerdict` | 3 | 3 | — |
| `CorrectiveDirective` | 3 | 0 | — |
| **`ProviderInvocationReceipt`** | 9 | **3** | CoderReport, ReviewerReport, HumanAttentionRequest |
| **`InteractionManifest`** | 3 | **1** | ProviderInvocationReceipt only |
| `CampaignFeedItem` | 1 | 0 | — |
| `HumanAttentionRequest` | 5 | 1 | — |
| `HumanCommandRequest` | 1 | 1 | HumanDecision |
| `HumanDecision` | 3 | 0 | — |
| **`CampaignRunBinding`** | 5 | **3** | CandidateAdmissionReceipt, ProviderInvocationReceipt ×2 (one `Causal`) |
| **`ArtifactImported`** | 2 | **0** | **nothing** |

External wrapper in-degree: `CandidateStateReceipt` 6, `CandidateMaterialization`
3, `WorktreeMaterialization` 2, `RunContractCandidateState` 2,
`RunArtifactSource` 1 (from `ArtifactImported` alone),
`EstablishedNonDispatchEvidence` 1 (from the SafeRedrive cause alone).

## 3. Q1 + Q2 — node universe, and the envelope/support boundary

### 3.1 `CampaignRunBinding` → **PROMOTE to envelope-bearing**

Existence was never the question: three consumers reference it, and it is the
only object that bridges logical campaign/round/role to physical
execution/conversation/run/attempt plus the input state actually materialized.

Classification is settled by two facts the schema alone would not have shown:

- One of its in-edges is **`Causal`**, not `Intra`:
  `ProviderInvocationReceipt.cause.safe_redrive.prior_run_binding_ref`. A later
  round reaches back to a binding from an earlier one. Cross-round reachability
  is independent replay addressability, which is test 4.
- It is referenced from **two distinct producer lanes** — a controller-accepted
  admission receipt and a provider-execution receipt. An object referenced across
  lanes needs an identity that neither lane owns, which is test 1.

Tests 1, 2, 4 and 5 pass. Test 3 passes in the narrow but real sense that a
campaign must not accumulate two bindings for one execution.

### 3.2 `ProviderInvocationReceipt` → **PROMOTE to envelope-bearing**

This reverses a v1 classification, so the burden is highest here. It is met by
v1's own rules rather than by the prototype.

- **In-degree 3, from three distinct kinds** spanning coder, reviewer and human
  lanes. A support object referenced by three unrelated consumers is doing an
  identity's job without an identity.
- **v1 forces the receipt to violate FD-5.3.** The frozen receipt carries
  `campaign_id` and `round_id` in its own payload (§3.12), and FD-11 then spends
  two of its twelve congruence predicates checking that those in-band copies
  equal the envelope's. FD-5.3 says exactly the opposite: "Payloads never restate
  an envelope-owned field — two copies of one fact in one artifact is a
  divergence waiting for a maintainer." The receipt restates two, and the only
  reason is that it has no envelope to put them in. Promotion removes the
  duplication and lets FD-5's lineage rules apply once, in one place.
- **No cycle is created.** The concern behind v1's FD-2.3 was an acceptance
  pointer *inside* the receipt, and that rule is untouched: the receipt still
  carries no reference to any artifact accepted from it. Traced through the
  registry, `CoderReport → PIR → {InteractionManifest, CampaignRunBinding} →
  {External, CAS}` terminates. Acyclicity survives.
- Under promotion the receipt's own envelope has `producer_role = controller` and
  therefore, per FD-11, carries no receipt reference of its own. The receipt is a
  controller artifact describing a provider execution — which is what it always
  was, now stated in the type system instead of in prose.

**This is the single decision in Phase G most deserving of adversarial review.**
It changes a frozen classification, and its strongest counter-argument is
conservatism: v1 works today with the receipt as a support object, and FD-11's
congruence predicates already paper over the duplication. If the reviewer thinks
the FD-5.3 argument is post-hoc, that is the place to say so.

### 3.3 `InteractionManifest` → **KEEP as a typed support object**

In-degree 1, from `ProviderInvocationReceipt` alone. No consumer outside its own
receipt, no independent acceptance, and its identity is meaningful only relative
to the execution it describes. Tests 1, 3 and 4 fail. Promotion in the design
input is symmetry with the receipt, not a consumer.

That symmetry is precisely what the burden of proof exists to refuse. Two objects
that appear together in one paragraph are not thereby the same kind of thing.

### 3.4 `ArtifactImported` → **NOT PROVEN — out of A1-V0**

In-degree **zero**. No V0 object references it; its only edges are outgoing, to
`AnyImportableCas` and to `RunArtifactSource`.

Three questions were kept separate, and only the first two are answered here:

```text
run -> CAS import operation needed?     plausibly YES  (mechanism)
durable import proof needed?            plausibly YES  (mechanism)
independent graph node needed?          NOT PROVEN
envelope-bearing message needed?        NOT PROVEN
```

An import mechanism can exist as a controller procedure whose product is an
ordinary CAS object with a recorded provenance, without minting a node. And the
shape being proposed — a record standing between raw bytes and an accepted
artifact — is the `ArtifactAcceptance` object v1 examined and refused, because it
sits between ranks 3 and 4 and forces re-ranking (FD-2.3). Refusing it once on
reasoning and then admitting it later on a naming change would be the same
decision with worse provenance.

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
envelope-bearing kinds: 13
  the eleven v1 kinds, plus
  + CampaignRunBinding          (3.1, promoted on cross-lane + Causal in-edges)
  + ProviderInvocationReceipt   (3.2, promoted on in-degree + FD-5.3 conflict)

typed support objects:
    InteractionManifest         (3.3, kept)
    ScopeContractV1             (v1, unchanged)
    CampaignEventPayload        (v1, unchanged)

out of V0:
    ArtifactImported            (3.4, not proven)
    RunArtifactSource           (3.4, falls with it)
    EstablishedNonDispatchEvidence (3.5, post-V0)
```

Thirteen is not eleven and not fifteen. It is what the consumers support: two
objects earned promotion, one did not, and one has no consumer at all.

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
derived:    rank, computable from the registry, retained as a checkable
            property and as review shorthand, never as the rule
acyclicity: proved over the registry, not asserted from rank monotonicity
```

The registry's two edge classes are adopted as they stand: `Intra` (within one
round's derivation flow, must topologically sort at kind level) and `Causal`
(crossing rounds, chains or attention lineage; instance-acyclic by
create-before-reference). The distinction is load-bearing — §3.1's promotion
turned on an edge being `Causal` — and rank cannot express it at all, which is
independent evidence for this change.

One open target is sanctioned, unchanged from the design input's rationale:
`AnyCommittedEnvelope`, for `CampaignFeedItem` causation. `AnyImportableCas`
leaves with `ArtifactImported` (§3.4).

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

**In scope — the manifest's class.** FD-1.4 lists `manifest` among the 64 MiB
evidence blobs while also bounding "any typed A1 payload" at 1 MiB, and §3.12.1
makes `InteractionManifestV1` a typed object. It is bounded at both figures at
once. §3.3 settles the class: it is a **typed support object**, therefore it takes
the control-artifact bound and `manifest` leaves the evidence-blob list. If 4096
interaction entries cannot fit 1 MiB in practice, the correct response is to
revisit `MAX_INTERACTION_SEQUENCE` or the manifest's own shape — not to reclassify
an object to suit a number.

**Out of scope — the envelope-size bound.** FD-1.4 bounds payloads and blobs and
never bounds stored envelope bytes, while FD-1.8 defines an envelope-bearing ref's
size as envelope + payload together, so no maximum for that sum is derivable. That
is arithmetic between two frozen decisions, not a question about which objects
exist. It goes to the v2 draft with §3.6 as its input. Phase G deliberately does
not invent a number, because inventing one here is how a graph adjudication
quietly becomes a wire-format revision.

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

## 8. For the independent reviewer

Attack these four, in this order:

1. **§3.2, the receipt promotion.** It reverses a frozen classification. Is the
   FD-5.3 duplication argument load-bearing, or reasoning assembled after the
   prototype had already promoted it? Does any cycle appear that the registry
   trace missed?
2. **§3.4, `ArtifactImported` as NOT PROVEN.** Zero in-degree is strong evidence,
   but the design input is a prototype: is there a real V0 import consumer that
   simply has not been wired yet, which would make this a premature refusal?
3. **§4, rank demoted to derived.** Does anything in v1 depend on rank being
   normative in a way the edge registry cannot express?
4. **§3.6, the count.** Thirteen should be checked as a conclusion, not as a
   compromise between eleven and fifteen. If any of the four adjudications is
   wrong, the number is wrong with it.
