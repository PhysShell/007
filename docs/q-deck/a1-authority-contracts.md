# Q-Deck A1: coder/reviewer/human authority contracts

## Status

**ACCEPTED / CLOSED / FROZEN.**

This document is the normative source for the A1 slice named in
`docs/autonomy-controller.md` ("A1 ReviewVerdict and CorrectiveDirective
contracts"). It was accepted at exact head `b61540a` after five corrective
rounds and a final exact-head consistency pass, then **amended once more before
merge** by R5.2 (§9), which closed four P1 findings from external review on PR
#123. The frozen baseline is the merged head of that PR. From there the
supersede path (§7) applies: corrections are forward-only and versioned, exactly
as they are for the artifacts this contract governs.

Design input: issue #95 (`A1: coder/reviewer/human contracts (draft)`), which
carries the rationale and discussion history. Where this document and that draft
differ, this document is authoritative.

Corrective rounds R1, R2, R3, R4, R5, R5.1, and R5.2 are recorded in §9.

**Superseded twice since incorporation: S1 and S2 (§9).** S1 corrects FD-1.4
and nothing else; S2 corrects FD-1.3 and nothing else, requiring member names to
be unique within every A1 JSON object. Both are applications of §7 after the
merge, as distinct from the R-rounds, which were pre-incorporation corrections.

Each changes this document's blob and therefore its `contract_digest`. Neither
changes any `envelope_version`, `message_kind_version`, or
`campaign_protocol_version`, because no payload shape, envelope, rank, or
reducer semantics moved (§7.2). **An implementation is bound to the current
blob, not to a version number**, so a consumer still bound to a superseded blob
is bound to superseded authority however conformant its behaviour looks.

What the freeze covers: the wire schema of every message kind — field names,
types, required/optional status, null policy, bounds, and the authority that
establishes each value — plus the digest, identity, evidence closure, and
transition rules. Opaque id string forms and additive reason codes remain
extensible under FD-1.6.

## Purpose

007 already produces, separately and correctly: exact candidate state (A0),
sealed worktrees, provider invocation with recorded evidence
(`docs/o7-invoke.md`), digest-chained run records with independent replay
(`o7 replay`, `src/main.rs:388` → `events::replay_record`), exact-head CI, and
an admission discipline (`docs/decision-and-admission-protocol.md`).

What does not yet exist is the object that turns those into a *permitted state
transition*. A1 introduces exactly one thing:

```text
a model emitted something
  != the system holds it as a fact
  != the system is permitted to proceed
```

Every rule below exists to keep those three apart while still letting one
autonomous corrective loop run unattended.

## 0. What this contract binds to and may not redefine

| Bound contract | Authoritative artifact | What A1 consumes, never redefines |
|---|---|---|
| A0 candidate state | `docs/q-deck/a0-candidate-state.md` (accepted head `52627c3`, merged `f1ac458`) | `CandidateRef` representation, base-commit semantics, the one-cumulative-patch model, `RepositoryIdentity`, materialization attestation, sealing/materialization ordering |
| R1 command vertical | `docs/q-deck/r1-command.md` §11.1–§11.2 | the durable dispatch boundary, `ValidUnsealedPreDispatch` vs `ValidUnsealedDispatchAmbiguous`, the four `DispatchProgress` values, fail-closed post-dispatch ambiguity |
| Controller architecture | `docs/autonomy-controller.md` (accepted `c5b3ae0b`, PR #93) | campaign phases, the transition-authority principle, the `ReviewVerdict` minimum |
| Admission protocol | `docs/decision-and-admission-protocol.md` §5, §6, §7 | diagnostic vs admission evidence, exact-head review, head drift invalidates acceptance |
| Canonical digest discipline | `crates/o7-run/src/event.rs` (module docs, `RunEvent::compute_digest`, `frame`) | digests by explicit length-prefixed field framing, never by hashing a serialized JSON blob |
| Provider call primitive | `docs/o7-invoke.md` | engine set, capability profile `read-only-data`, the `PASS`/`FAIL_*`/`BLOCKED_*` status vocabulary, the `stdout.raw` / `result.json` artifact split, key handling |

A1 adds no new claim about any of these. In particular it does **not** strengthen
R1's delivery semantics into an exactly-once claim, and it does not weaken the A0
sealing boundary into "the process exited".

## 1. Roles, topology, and the authority boundary

Three roles, one controller, no direct model-to-model channel:

```text
Controller -> Coder:      WorkOrder
Coder      -> Controller: CoderReport          (untrusted)
Controller:               CandidateReceipt     (derived behind the A0 seal)
Controller -> Reviewer:   ReviewRequest
Reviewer   -> Controller: ReviewerReport       (untrusted)
Controller:               ReviewVerdict        (validated, accepted)
Controller -> Coder:      CorrectiveDirective
Controller -> Human:      CampaignFeedItem | HumanAttentionRequest
Human      -> Controller: HumanCommandRequest  (untrusted)
Controller:               HumanDecision        (recorded, accepted)
```

GitHub PR comments are a human-readable **projection** of these objects. No agent
consumes a mutable PR comment as authoritative input; a projection is never an
input channel.

### 1.1 Untrusted input vs controller-accepted artifact

```text
untrusted input          controller-accepted artifact
---------------          ----------------------------
CoderReport           -> CandidateReceipt
ReviewerReport        -> ReviewVerdict
HumanCommandRequest   -> HumanDecision
```

Free-form model prose is an attachment on an artifact, never the artifact.

### 1.2 Delivery semantics (R1-faithful)

```text
artifact creation and acceptance   durable and idempotent
artifact delivery and observation  may occur more than once

duplicate message_id + same envelope digest  -> idempotent replay (FD-6)
duplicate message_id + other envelope digest -> conflict, fail closed (FD-6)
```

At-most-once applies to **provider dispatch** in the supported single-host model,
not to message delivery. A1 makes no exactly-once claim about anything.

## 2. The frozen decisions

### FD-1 — artifact model, digests, encoding, bounds, unknown fields, versions

**FD-1.1 The artifact model.** An A1 artifact is a pair:

- a **payload** — the typed body of one `message_kind`, stored as its own
  immutable byte string in 007-owned content-addressed storage;
- an **envelope** — the record that identifies, binds, and references it.

`payload_digest` is SHA-256 over *those exact stored payload bytes*, lowercase
hex, 64 chars — the existing `o7_run::event::Digest256::of_bytes` form. The
envelope's own identity is its framed digest (FD-1.2), which commits to
`payload_digest`. No reader ever re-serializes anything to verify an identity.

**For a provider-produced artifact, the payload bytes ARE the adapter-normalized
provider output bytes.** The controller attaches an envelope; it never edits,
re-orders, re-encodes, or enriches the body. This is what makes the congruence
check of FD-11 meaningful, and it is also what stops the controller from quietly
improving a model's claims on the way in.

**FD-1.2 Envelope digests are computed by field framing, not by JSON.** Following
the precedent frozen in `crates/o7-run/src/event.rs` (module docs: "Digests are
computed by explicit field FRAMING (length-prefixed), not by hashing a serialized
JSON blob, so they are byte-stable regardless of map ordering or serializer
whitespace"):

```text
h = SHA-256
h.update(b"o7-a1-envelope\0v1\0")
frame(envelope_version.to_le_bytes())
frame(message_kind name)
frame(message_kind_version.to_le_bytes())
frame(message_id)
frame(root_goal_id) frame(task_id) frame(campaign_id) frame(round_id)
frame(causation_id) frame(correlation_id)
frame(producer_role name)
frame(producer_execution_id) frame(producer_adapter_version)
frame(model_identity)
frame(prompt_digest) frame(tool_policy_digest)
frame(contract_digest) frame(expected_input_head)
frame(payload_digest)
frame(provider_execution_receipt_ref digest, or the empty string when absent)
frame(artifact_refs count.to_le_bytes()), then for each ref in stored order:
    frame(artifact_kind name) frame(media_type) frame(digest) frame(size.to_le_bytes())
```

`frame(x)` is `u64-le length prefix || bytes`, identical to
`crates/o7-run/src/event.rs:906`. Absent optional fields are framed as the empty
string; they are never skipped, so "absent" and "empty" hash distinctly from
"different value", and the field order is fixed forever.

**Enums are framed by name, not by tag byte.** Every `… name` in every framing
in this document — here and in §3.15 — is the frozen `snake_case` ASCII name of
the variant exactly as written in the §3 schemas — `work_order`, `coder_report`, `reviewer`, `review_verdict_accepted` —
length-prefixed like any other field. `ArtifactRef.kind` follows the same rule
over the closed set frozen in FD-1.9. `o7-run`
assigns numeric tags (`ArtifactKind::Task => 1`, `Diff => 2`, …) and is right to:
those bytes never leave one crate. A1 is a cross-implementation wire contract,
where an unassigned "tag byte" means two conforming implementations can pick
`work_order = 0` and `work_order = 1` and both be correct while computing
different digests. Framing the name removes the coordination problem instead of
adding four numbering tables to maintain — and a renamed variant becomes a
different digest, which is the desired alarm, not a bug.

`created_at` is deliberately **excluded** from the framing (FD-5.4).

**No canonical-JSON scheme is introduced.** Payload identity is the digest of
stored bytes; envelope identity is field framing. Neither requires two
serializers to agree on whitespace or key order, so the whole class of
"semantically identical, digest-unequal" failures disappears without anyone
inventing a normalization standard.

The corollary is equally deliberate and has teeth elsewhere in this document: a
payload re-serialized with different whitespace is a *different* payload. Bytes
are the artifact. Nothing in A1 may require two independently serialized payloads
to be "byte-identical" in some field — equality between payloads is expressed by
sharing one immutable referenced document and comparing its digest (see
`ScopeContractV1`, §3.13).

**FD-1.3 Encoding and null policy.** Stored payload bytes are UTF-8 JSON with a
top-level object. Invalid UTF-8, a leading BOM, and non-object top-level values
are rejected at ingest. **No Unicode normalization is performed** — content is
never rewritten, and a digest is never computed over a normalized copy of
something stored differently.

One uniform null policy, everywhere in A1: **an optional field is either absent
or carries a value. Explicit JSON `null` is rejected at parse time.** There is no
field in this contract where `null` and absent mean different things, because
that distinction has never once been worth what it costs.

**Member names are unique within every JSON object, and a duplicate name is
rejected at parse time. No first-wins or last-wins interpretation is
permitted.** (S2, §9.)

RFC 8259 says names *should* be unique and leaves the behaviour undefined when
they are not, so first-wins and last-wins are both conforming readings of the
same bytes. That is tolerable for a document format and not for this one: FD-1.2
computes an envelope's identity by framing its **fields**, so two conforming A1
implementations reading identical stored bytes would frame different fields and
compute different digests. Admitting a duplicate reopens precisely the class of
failure FD-1.2 exists to close, on the one input an attacker fully controls.

Note the interaction with the null policy above, because it is the reason this
rule had to be stated rather than left implied. In `{"x": null, "x": 1}` the null
is physically present in the stored bytes, so the null policy already refuses the
document — but only for an implementation that examines the bytes. One that
reduces the document through a JSON library first may never see it, since the
library has already discarded the shadowed member. The uniqueness rule makes both
implementations refuse the same document for the same reason, which is what the
null policy meant all along.

**FD-1.4 Per-object bounds (protocol hard maxima).**

```text
typed A1 JSON object, except the one below       <=      1 MiB
InteractionManifestV1                            <=     64 MiB
opaque evidence blob (diff, raw provider bytes,
  gate log, patch)                               <=     64 MiB
JSON nesting depth                               <=     32
array length, any array                          <=   4096
string length, any single string field           <=  65536 bytes
opaque id length                                 <=    256 bytes
artifact_refs per envelope                       <=    256
interaction_sequence entries per manifest        <=   4096
dispatches per execution receipt                 <=    256
findings per ReviewerReport / ReviewVerdict      <=    256
```

`InteractionManifestV1` is a typed A1 object and keeps every rule that follows
from that — FD-1.7 fixes its media type, FD-2 gives it its own rank — but its
size bound is the evidence one.

The reason is its grain. There is **one manifest per execution**, not per
dispatch: §3.12 nests every dispatch record inside a single receipt per
`provider_execution_id` and gives that receipt one `interaction_manifest_ref`,
and §3.12.1 binds the manifest to the same execution id. So a single manifest
indexes up to 256 dispatches with up to 4096 `interaction_sequence` entries
**in total**, each carrying refs and ids. That is an index of an execution's
whole externally observable history, which is not the shape the 1 MiB
typed-object ceiling was written for.

**Size and typedness are separate questions**, and this is the one object where
they answer differently.

Exceeding any bound is a parse-time rejection, never a truncation. A truncated
artifact that still parses is the failure mode these bounds exist to forbid.

**FD-1.5 Aggregate bounds (evidence closure).** Per-object bounds do not bound
the cost of *resolving* an artifact. 256 refs × 64 MiB is 16 GiB of direct
references alone, and the rank DAG of FD-2 permits a branching reachable closure
beneath that. The graph being acyclic does not make it small; an acyclic graph
kills a process just as efficiently as a cyclic one, with better manners.

Two layers, frozen:

```text
protocol hard maxima (compile-time constants, never configurable)
  max_direct_referenced_bytes        128 MiB   # sum over immediate_refs (below)
  max_reachable_closure_bytes        256 MiB   # sum over the deduplicated closure
  max_reachable_closure_objects     2048
  max_refs_per_execution            4096       # all refs minted by one execution

campaign policy (selected per campaign, MUST be <= the hard maximum)
  evidence_budget_bytes               64 MiB   # default for a V0 campaign
  closure_object_budget              512
```

Two invariants on those numbers, checked at campaign creation and in a unit test,
because a bound that contradicts another bound is decoration:

```text
max_direct_referenced_bytes <= max_reachable_closure_bytes
      (direct refs are a subset of the closure)
campaign policy value       <= the corresponding hard maximum
```

The **effective** bound for a resolution is `min(hard maximum, campaign policy)`.

`budget_policy_digest` is framed over exactly those values, in this order, so
two implementations cannot disagree about what the campaign's policy was:

```text
h.update(b"o7-a1-budget-policy\0v1\0")
frame(max_provider_turns.to_le_bytes())
frame(max_wall_time_seconds.to_le_bytes())
frame(evidence_budget_bytes.to_le_bytes())
frame(closure_object_budget.to_le_bytes())
```

The values themselves are carried in `CampaignCreated` and held in
`CampaignStateV1.budget_policy` (§3.14), so a replay is self-contained: the
digest proves nothing was changed, and the numbers say what the bound actually
was. A digest without its preimage would have been a receipt for a value nobody
can read.

**The reference set of an artifact is not its `artifact_refs` list.** Almost
every typed payload carries its own `ArtifactRef`-valued slots —
`CandidateReceipt.coder_report_ref`, `ReviewRequest.candidate_receipt_ref` /
`scope_ref` / `evidence_refs`, `ReviewVerdict.reviewer_report_ref`,
`CorrectiveDirective.review_verdict_ref`, `HumanDecision.command_request_ref`,
and so on. Nothing requires those to be duplicated into `envelope.artifact_refs`,
and duplicating them would be the worse fix: two lists of one truth. So the
bound is defined over the union:

```text
immediate_refs(node) :=

  node is an A1 artifact:
        envelope.artifact_refs
      ∪ every ArtifactRef-valued slot declared by the payload's §3 schema
      ∪ provider_execution_receipt_ref, when present

  node is a CampaignEventV1 (§3.15):
        source_ref, when present
      ∪ evidence_refs
      ∪ every ArtifactRef-valued slot declared by its event payload's schema
```

A campaign event is not envelope-bearing, but it reaches artifacts exactly like
one does — `source_ref`, `evidence_refs`, and payload slots such as `scope_ref`,
`receipt_ref`, gate `log_ref`s, and CI `observation_ref`s. **Every event is
resolved under this same closure, with the same bounds, before it is folded**
(FD-14.2). An admission path that resolves artifacts carefully and then walks
event references freely would have moved the hole rather than closed it.

Closure traversal is frozen as an algorithm, not as an intention:

```text
resolve_closure(root):
  # the root payload is bounded by max_control_artifact_bytes (FD-1.4), so this
  # parse is cheap and happens before any evidence blob is touched
  parse root payload under the schema of its declared message_kind
  seen  := {}                      # typed object identity: (ref.kind, ref.digest)
  bytes := 0 ; objects := 0
  queue := immediate_refs(root)
  if sum(declared sizes of deduplicated queue) > effective(max_direct_referenced_bytes):
      REJECT before reading anything
  while queue not empty:
    ref := pop(queue)
    if ref in seen: continue       # deduplicate BEFORE accounting
    seen += ref
    objects += 1
    bytes += ref.size              # the DECLARED size, accounted before any read
    if bytes   > effective(max_reachable_closure_bytes):   REJECT whole resolution
    if objects > effective(max_reachable_closure_objects): REJECT whole resolution
    verify the stored object(s) against the ref per FD-1.8 — for an
      envelope-bearing artifact that is BOTH halves: envelope framing == ref.digest,
      payload bytes == envelope.payload_digest, and the two sizes summing to ref.size
    if the slot expects a typed object (FD-2.5):
        parse it under that slot's schema
        enqueue every ArtifactRef-valued slot that schema declares
    else:
        do not parse, do not enqueue          # rank-0 bytes stay bytes
```

Accounting uses the *declared* `size` before reading, so an oversized blob is
refused rather than streamed. A stored object whose real size disagrees with its
declared `size` is an integrity failure. Resolution is all-or-nothing: a closure
that exceeds a bound is never partially accepted, and the artifact that
referenced it is rejected — it does not degrade into "accepted with missing
evidence".

**Cumulative per-campaign evidence storage is deliberately NOT bounded here.**
A campaign-wide byte ceiling is only an invariant if the reducer carries a
canonical running total, and A1 does not define one (FD-14.5). Rather than
publish a number that nothing enforces, A1 bounds each resolution and leaves
cumulative storage accounting to A2, where the state that would have to track it
already lives.

**FD-1.6 Unknown fields and versions.** Every A1 type deserializes with
`#[serde(deny_unknown_fields)]`, and every enum is closed — no `#[serde(other)]`
catch-all. Both follow the A0 precedent: `CandidateStateReceiptV1`
(`crates/o7-run/src/candidate.rs:78`) and `CandidatePatchKind`
(`crates/o7-run/src/event.rs:363–371`, whose doc comment states the rule
outright: "an unrecognized/future value on the wire fails closed at
deserialization … there is deliberately no `#[serde(other)]` catch-all").

Three independent version fields:

- `envelope_version: u32` — envelope framing and field set. Frozen v1 = `1`.
- `message_kind_version: u32` — the payload schema of one kind. Every kind
  frozen here is at version `1`.
- `campaign_protocol_version: u32` — the reducer semantics of FD-14. v1 = `1`.

An unrecognized value of any of them is refused; the artifact is never parsed "as
well as we can". This mirrors `RUN_EVENT_SCHEMA_VERSION`'s rule in
`crates/o7-run/src/event.rs:25`: "A reader that encounters a different version
must refuse to replay it as if it understood it."

A campaign is bound to one version set at creation. A mid-campaign version change
is a supersede (§7), never an in-place upgrade.

**FD-1.7 Media types.** `application/vnd.o7.a1.<kind>+json; v=<version>` for
typed artifacts; evidence blobs carry their own concrete type
(`application/json`, `text/x-diff`, `application/octet-stream`). Media type is
part of every `ArtifactRef` and part of the envelope framing: the same bytes
under a different declared type are a different reference (FD-2.5).

**FD-1.8 Artifact refs, and what they identify.** `ArtifactRef = (kind,
media_type, digest, size)` into 007-owned CAS only. There is no path field and no
URL field to populate. An agent-supplied path or URL appearing anywhere in a
payload is inert text and is never dereferenced (FD-7).

An envelope-bearing artifact is stored as **two** byte strings (FD-1.1), so a ref
to one has to say which — otherwise `size` charges half the cost of what the
resolver reads, and `digest` names an object nobody agreed on. Frozen:

```text
ref to an envelope-bearing artifact (the eleven message kinds):
    digest = the envelope digest (FD-1.2), which commits to payload_digest
    size   = stored envelope bytes + stored payload bytes, together

ref to any other object (rank-0 blobs, the execution receipt, the interaction
manifest, ScopeContractV1, a campaign event payload):
    digest = that object's own content digest
    size   = that object's own stored size
```

One ref therefore covers the whole artifact, and integrity stays provable in both
halves: the stored envelope must reproduce `ref.digest` under FD-1.2 framing, and
the payload bytes must hash to that envelope's `payload_digest` (FD-1.1). The
size check is `envelope_bytes + payload_bytes == ref.size`, so FD-1.5 charges the
true cost of a resolution before reading either half — 65 typed artifacts with
small envelopes and 1 MiB payloads now cost 65 MiB against the budget, which is
what they actually cost.

**FD-1.9 `ArtifactKindV1` — the complete closed set.** `kind` is a digest input
(FD-1.2) and every enum in A1 is closed (FD-1.6), so an incomplete list would
put implementations back where the tag-byte problem left them: inventing
spellings and computing different digests. Every `ArtifactRef` slot in §3 resolves
to exactly one of these, and nothing else is a valid `kind`.

| Group | `snake_case` spellings |
|---|---|
| the eleven A1 message kinds | `work_order`, `coder_report`, `candidate_receipt`, `review_request`, `reviewer_report`, `review_verdict`, `corrective_directive`, `campaign_feed_item`, `human_attention_request`, `human_command_request`, `human_decision` |
| A1 typed non-envelope objects | `provider_execution_receipt`, `interaction_manifest`, `scope_contract`, `campaign_event_payload` |
| imported A0 kinds — **spelled exactly as `o7_run::event::ArtifactKind` already serializes them** (`crates/o7-run/src/event.rs:113–155`, `#[serde(rename_all = "snake_case")]`) | `candidate_state` (an A0 `CandidateRef`), `candidate_patch`, `worktree` (the materialization attestation), `policy`, `task`, `diff`, `gate_log`, `sandbox_report`, `provider_session`, `command_binding` |
| A1 evidence blobs | `contract_document`, `verifier_registry`, `canonical_provider_request`, `raw_provider_bytes`, `normalized_output`, `provider_message`, `tool_arguments`, `tool_result`, `usage_record`, `cost_record`, `diagnostic_log`, `ci_observation`, `termination_observation`, `detail_document` |

Reusing A0's spellings verbatim rather than minting parallel ones means a
`candidate_state` reference denotes the same object on both sides of the
boundary — which is the entire point of importing a kind instead of redefining
it (FD-2.4).

### FD-2 — the evidence graph is acyclic by construction

**FD-2.1 Rank rule.** Authority flows one way, and content-addressed references
must flow the same way. Instead of a runtime cycle check:

| Rank | Class | May reference |
|---|---|---|
| 0 | opaque evidence bytes and imported authority roots (FD-2.4): canonical provider request, raw provider bytes, adapter-normalized output, usage/cost records, diffs, patches, gate logs, external contract documents, A0 `CandidateRef`, materialization attestation — **never parsed by A1**; and one local typed leaf, `ScopeContractV1`, which A1 does parse but which declares no outgoing refs | nothing |
| 1 | `InteractionManifestV1` | rank 0 |
| 2 | `ProviderExecutionReceiptV1` | rank ≤ 1 |
| 3 | untrusted reports: `CoderReport`, `ReviewerReport`, `HumanCommandRequest` | rank ≤ 2 |
| 4 | controller-accepted artifacts: `CandidateReceipt`, `ReviewVerdict`, `HumanDecision` | rank ≤ 3 |
| 5 | controller-issued instructions and notices: `WorkOrder`, `ReviewRequest`, `CorrectiveDirective`, `HumanAttentionRequest`, `CampaignFeedItem` | rank ≤ 4 |

**A content-addressed reference may only target a strictly lower rank.** Rank
monotonicity implies acyclicity; no cycle detector is needed, and the property is
checkable on one artifact in isolation.

**FD-2.2 Lineage and causation are identifiers, not digests.** `causation_id`,
`correlation_id`, `campaign_id`, `round_id`, `question_id`, `attention_id`,
`finding_id`, `review_id`, `command_id` are opaque ids and are therefore *not
edges in this graph*. That is what lets a `CoderReport` (rank 3) be caused by a
`WorkOrder` (rank 5) without inverting the reference direction.

**FD-2.3 No acceptance pointer inside an immutable evidence object.** A
`ProviderExecutionReceiptV1` never carries a reference to an artifact accepted
from it; the forward directions are `CandidateReceipt.coder_report_ref` and
`ReviewVerdict.reviewer_report_ref`. Back-links needed by projections, indexes,
or a UI live in projections. A projection may compute any inverse it likes; it
never becomes canonical.

**On `ArtifactAcceptance`:** it does **not** exist in V1, and this table is not
built to absorb it. A dedicated acceptance record would sit between rank 3 and
rank 4 and would require re-ranking those classes. Adding it therefore requires a
supersede of FD-2, a new `envelope_version`, and new `message_kind_version`s —
not an additive patch. Reserving a rank now for an object whose necessity is
unestablished would be the same mistake in a more expensive font.

**FD-2.4 Imported authority roots.** Some content-addressed references leave the
A1 type system entirely:

```text
ImportedAuthorityRoot :=
    A0 CandidateRef
  | A0 / o7-worktree MaterializationAttestation
  | accepted external Contract document
  | controller gate/verifier registry definition
```

`ScopeContractV1` (§3.13) is **not** an imported root: it is an A1-owned typed
leaf that A1 parses under its own schema — the controller needs its
`allowed_paths` to derive `diff_scope`. It sits at rank 0 because it declares no
outgoing content references, not because it is opaque. Rank 0 therefore means
"terminal in the reference graph", and parseability is decided per slot (FD-2.5),
not per rank.

Frozen rules for imported roots:

- They occupy **virtual rank 0**. A1 never parses them into A1 semantics and
  never re-validates them under A1 rules; each is validated against its own
  frozen schema and digest contract by the crate that owns it (A0 objects by
  `o7-run`/`o7-worktree`, registry definitions by the controller's registry).
- The **authority to import** comes from the controller-owned registry or the
  campaign binding — never from an incoming `ArtifactRef`. An agent cannot
  introduce a new imported root by naming one.
- An imported root that fails its own owner's validation fails the A1 artifact
  that referenced it, closed.

**FD-2.5 The resolver's duties.** `ArtifactRef.kind` is a claim by whoever wrote
the reference. Frozen:

- Every reference slot has one expected `kind` and expected media type, fixed by
  the schemas in §3. The resolver checks the stored object against the *slot's*
  expectation, not against the sender's declaration; a mismatch fails closed.
- Rank-0 bytes are never parsed and never promoted to an authority object because
  they happen to look like one. Only a slot that expects a typed object causes a
  parse (FD-1.5).
- The same bytes referenced through two different typed slots are **two distinct
  nodes** in the closure. Deduplication is by `(kind, digest)`, never by digest
  alone.

### FD-3 — raw provider evidence is separate from adapter-normalized output

Per **dispatch**, three distinct artifacts with three distinct digests:

```text
canonical_request_ref     the exact provider-facing request after adapter
                          construction — not the logical WorkOrder
raw_provider_event_ref    provider bytes/events as received, when capturable
normalized_output_ref     adapter-normalized payload, PRE-envelope bytes
```

In V0 these map onto artifacts `o7 invoke` already writes
(`docs/o7-invoke.md`, "Artifacts and `meta.json` mapping"): `stdout.raw` is "the
raw HTTP response body, byte-for-byte … the unmodified provider evidence";
`result.json` is "the extracted (normalized) content value".

**A 2xx response is not an accepted artifact.** The ladder is three steps, and
every step is a separate recorded act with its own rank:

```text
normalized provider output (o7 invoke result.json)        rank 0
controller ingest + A1 envelope  -> ReviewerReport        rank 3
controller validation            -> ReviewVerdict         rank 4
```

`o7 invoke`'s `PASS` means only: HTTP 2xx, content parsed, schema-valid. It is
the precondition of step one, not a shortcut to step three.

Where the provider or adapter cannot establish a fact, it is recorded as
unavailable under an explicit status. A requested model alias is never recorded
as a resolved backend identity.

### FD-4 — untrusted report vs controller-accepted artifact

Acceptance is an act of the controller, recorded as a canonical event, with the
accepted artifact referencing the raw report forward (FD-2.3). The report's own
`status`/`verdict` field is an input to validation, never its outcome:

- A `CoderReport` may say `candidate_produced`. That claim is checked against the
  controller-derived candidate (FD-5); a mismatch fails closed.
- A `ReviewerReport` may say `accepted`. Its JSON authorizes nothing. Only a
  controller-issued `ReviewVerdict` enables a transition (FD-12).
- A `HumanCommandRequest` may claim anything, including its own actor identity.
  Only a `HumanDecision` is authority (FD-15).

### FD-5 — authority direction: lineage, head, contract, time

**FD-5.1 Lineage.** The controller never adopts lineage from an incoming
envelope. It resolves the expected `(root_goal_id, task_id, campaign_id,
round_id)` from the causation target and the canonical campaign binding, then
verifies the carried fields match. Mismatch fails closed. Comparing one incoming
envelope against another incoming envelope is not verification.

The controller mints and durably binds `root_goal_id`, `task_id`, and
`campaign_id` atomically **before any agent dispatch** — the natural extension of
R1's "durable acceptance before provider invocation". A campaign without a
complete lineage binding cannot exist, so no later backfill or heuristic linking
is ever required.

v1 active topology: exactly one root goal, one task under it, at most one active
campaign executing that task. A replacement execution mints a new `campaign_id`
and records `supersedes` against the prior terminal campaign; it never mutates or
reuses it.

**FD-5.2 Head.** Every artifact that reasons about code carries
`expected_input_head`, verified against `CampaignStateV1.current_candidate_head`
at the moment of use. A `ReviewerReport` whose `reviewed_head` is not the current
candidate head is stale: retained as evidence, never promoted to a
`ReviewVerdict`.

**FD-5.3 Contract.** `contract_digest` lives in the envelope and is verified
against the campaign binding. Payloads never restate an envelope-owned field —
two copies of one fact in one artifact is a divergence waiting for a maintainer.

**FD-5.4 Time.** `created_at` is the producer's observation. It is metadata: it
is excluded from the envelope framing (FD-1.2), it is never an ordering
primitive, and it never participates in identity — the rule already frozen for
run events (`crates/o7-run/src/event.rs:622`: "Metadata only — NEVER the ordering
key"). Order is the canonical append sequence plus the causation graph.

Because it is excluded from identity, one envelope digest can arrive attached to
several serializations differing only in `created_at`. Frozen resolution:

```text
the FIRST accepted occurrence is stored canonically, verbatim
the controller records first_observed_at on the acceptance record (controller-owned)
a redelivery NEVER mutates the stored envelope, created_at, or first_observed_at
redelivery observation times live only in the delivery log / projection
```

So a redelivered artifact is a replay (FD-6), and the canonical record still has
exactly one `created_at`: the one that came with the occurrence that was accepted.

### FD-6 — duplicate identity: replay vs conflict

```text
same message_id, same envelope digest   -> idempotent replay:
                                           return the existing accepted artifact,
                                           perform no new side effect,
                                           dispatch nothing,
                                           do not advance state_version
same message_id, other envelope digest  -> IdConflict: fail closed,
                                           no acceptance, no dispatch,
                                           HumanAttentionRequest raised
```

The comparison is the **envelope digest** (FD-1.2), not `payload_digest` alone:
the envelope digest commits to `payload_digest`, every `artifact_ref`, the
lineage fields, and the receipt reference — so two artifacts agreeing on payload
bytes but disagreeing on which candidate head, which contract, or which provider
execution they belong to are a conflict, not a replay.

Identity scope: `message_id` is unique within a `campaign_id`; `idempotency_key`
on a `HumanCommandRequest` is unique within a `campaign_id`, and reuse with a
different request is the same `IdConflict` — matching R1's existing `409` for
"idempotency key reused with a different request" (`docs/q-deck/r1-command.md`
§8).

An `IdConflict` never resolves itself by minting a new id. Two different payloads
claiming one identity means the system's view of who said what is broken, and
that is a human-facing fact.

### FD-7 — no model-supplied executable authority

```text
reviewer.required_regression_evidence = "run this shell command"
controller: shell -c reviewer_text            # NEVER
```

- Required evidence is named by `required_evidence_gate_ids`, resolved against
  the controller-owned gate/verifier registry, plus `verifier_policy_digest`. An
  unknown id fails closed.
- A reviewer may *propose* new evidence in prose. The controller maps it to a
  known registry id or raises `HUMAN_REQUIRED`. It never executes a model-authored
  string.
- `CoderReport.diagnostic_runs[].command_recorded` is forensic text, never
  re-executed. Per `docs/decision-and-admission-protocol.md` §5 it is diagnostic
  evidence, not admission evidence: `GATING → CI_WAIT` is satisfied only by
  controller-owned gate/CI runs against the receipt head.
- Every `artifact_ref` resolves inside 007-owned CAS only (FD-1.8, FD-2.5).
- A recorded `tool_call_requested` interaction is an observation, not an
  authorization. Only controller-owned registry resolution, policy validation,
  and execution produce an accepted `tool_result` interaction.

### FD-8 — replay never invokes a provider

Reducer replay, campaign replay, reconciliation, recovery, and historical
verification reconstruct results from immutable recorded evidence. None may call
a provider to rebuild an earlier answer.

This generalizes the property `o7 replay` already has for a single run record
(`src/main.rs:388` → `events::replay_record`: chain continuity, per-event
digests, artifact content digests, verdict recomputation — no provider call on
that path) to the campaign level.

A new provider call is always a *new* canonical execution with a fresh identity.
It never silently completes or replaces the history of an earlier one.

### FD-9 — post-dispatch ambiguity fails closed

R1 froze the durable dispatch boundary and its classification
(`docs/q-deck/r1-command.md` §11.1–§11.2). A1 generalizes it from command
continuation to **every model role, including read-only ones**:

```text
dispatch boundary not reached (established) -> safe redrive: fresh dispatch_id,
                                               explicit retry_of (FD-10)
dispatch occurred or may have occurred,
  outcome unknown                           -> dispatch_ambiguous:
                                               no redrive, no completion,
                                               no rejection, no mutation;
                                               HumanAttentionRequest raised
```

Classification is **per `dispatch_id`** (FD-10). An execution containing one
ambiguous dispatch is an ambiguous execution: `execution_outcome` is
`dispatch_ambiguous`, and no artifact produced under it may be accepted.
Ambiguity never collapses into "the execution retried fine".

A fresh identifier does not make a duplicate side effect safe. A read-only
reviewer invocation is still an external side effect: repeating it can produce a
different verdict, which rewrites campaign history exactly as effectively as
repeating a coder invocation rewrites code.

### FD-10 — provider invocation identity grains, and the shape that carries them

Two grains, both mandatory:

```text
provider_execution_id   one bounded role execution (one coder execution or one
                        reviewer execution), spanning its whole tool loop and
                        every continuation dispatch inside it
dispatch_id             one external provider request (one HTTPS request, or
                        one CLI process invocation)
```

**FD-10.1 The receipt is execution-level.** A per-dispatch receipt cannot be
referenced by a single `provider_execution_receipt_ref` without losing the rest
of a multi-dispatch tool loop, and a receipt carrying one `dispatch_id` beside a
whole-execution manifest does not say which of the two it represents. Frozen: one
`ProviderExecutionReceiptV1` per execution, with dispatch records **nested inside
it** (§3.12). The envelope references exactly that one object.

**FD-10.2 Boundary classification stays per dispatch.** Each nested dispatch
record carries its own `dispatch_boundary` and `outcome`; the execution's
`execution_outcome` is derived and fails closed (FD-9).

**FD-10.3 Retry names its grain.**

```text
whole-execution retry   new provider_execution_id, retry_of_execution_id set
single-dispatch retry   new dispatch_id in the SAME execution,
                        retry_of_dispatch_id set
tool-loop continuation  new dispatch_id, kind = continuation, NOT a retry,
                        no retry_of_*
new session             new provider_execution_id, no retry_of_*;
                        a fresh conversation, not a repetition
```

A retry is permitted only when non-dispatch is established (FD-9). The full
incarnation taxonomy (run/attempt/session/campaign incarnations) stays deferred
to A2; what A1 freezes is that no recovery or retry code may be written without
naming which grain it operates on.

`producer_execution_id` in the envelope has exactly three cases, one per
producer role — a mandatory field with an undefined case is a field an
implementation has to invent:

```text
producer_role = coder | reviewer   the provider_execution_id of that execution
producer_role = controller         the controller's own execution identity
producer_role = human              the controller's INGRESS execution identity:
                                   the identity of the ingress that accepted the
                                   request bytes
```

The human case names who *accepted* the bytes, never who authored them. A human
does not execute anything inside 007, so there is no execution of theirs to
name; authorship remains the untrusted `claimed_actor_identity` on the request
and the observed `authentication_strength` on the decision (FD-15.2). The
artifact stays rank 3 and untrusted — an ingress identity is provenance for the
acceptance, not an endorsement of the content.

### FD-11 — the receipt must prove *this* execution produced *this* artifact

Presence and resolvability of a receipt establish nothing about provenance: a
valid receipt from an unrelated execution attached to a valid report is a
cryptographically tidy lie, and it is the worst genre of lie because it reads as
an audit.

For every artifact whose `producer_role` is `coder` or `reviewer`, acceptance
requires **all twelve congruence predicates** below — nine envelope/receipt field
equalities, the completed-outcome requirement, and two campaign bindings —
checked before any other semantic validation:

```text
envelope.producer_execution_id      == receipt.provider_execution_id
envelope.producer_role              == receipt.producer_role
envelope.producer_adapter_version   == receipt.producer_adapter_version
envelope.prompt_digest              == receipt.request.prompt_digest
envelope.tool_policy_digest         == receipt.request.tool_policy_digest
envelope.model_identity             == receipt.model.requested_model
envelope.payload_digest             == receipt.final_normalized_output_ref.digest
envelope.campaign_id                == receipt.campaign_id
envelope.round_id                   == receipt.round_id
receipt.execution_outcome           == completed

receipt.request.budget_policy_digest
    == CampaignStateV1.budget_policy_digest

receipt.provider_execution_id
    == CampaignStateV1.active_execution.provider_execution_id
       (which the initiating WorkOrder / ReviewRequest / CorrectiveDirective set
        from its own target_provider_execution_id — §3.1)
```

The last two arrived in R5.1, once budget policy became a canonical campaign
fact and executions gained a canonical origin. Without them an execution could
run under a different budget policy, or under an identity the campaign never
authorized, and still satisfy every other predicate — a provenance chain that
verifies beautifully and proves the wrong campaign.

Any mismatch fails closed as `ReceiptIncongruent`, with no acceptance and no
transition. The `payload_digest` equality is what makes the chain load-bearing:
it is only checkable because a provider-produced payload *is* the normalized
output bytes (FD-1.1), so the receipt binds the exact bytes that became the
artifact, not merely an execution that happened nearby in time.

Conversely, an artifact whose `producer_role` is `controller` or `human` must
carry **no** `provider_execution_receipt_ref`. A controller-derived artifact
holding a provider receipt is claiming a provenance it does not have.

### FD-12 — transition authority

Extends the table in `docs/autonomy-controller.md` ("Transition authority"). Each
row names the single canonical artifact that authorizes the transition; the
reducer that enforces it is frozen in FD-14 and implemented in A1-V0.

| Transition | Authorizing canonical artifact | Never sufficient |
|---|---|---|
| campaign creation → `BUILDING` | `CampaignCreated`, seeded with the lineage bound before any dispatch (§3.15) | a task description alone |
| `BUILDING` → a coder round | `WorkOrder` accepted, `scope_ref` bound | a prompt |
| `BUILDING` → `GATING` | `CandidateReceipt` (controller-derived behind the A0 seal), `claim_check.claimed_head_matches = true` | `CoderReport.status = candidate_produced` |
| `GATING` → `CI_WAIT` | controller-owned gate results bound to `candidate_head` | `CoderReport.diagnostic_runs` |
| `CI_WAIT` → `REVIEWING` | required CI results bound to the same exact head | a green workflow on another head |
| `REVIEWING` → `CORRECTING` | `ReviewVerdict.verdict = changes_requested` at the current head | `ReviewerReport` saying so |
| `REVIEWING` → `READY_TO_MERGE` | `ReviewVerdict.verdict = accepted`, `reviewed_head == current_candidate_head`, no drift, required gates green | `ReviewerReport.verdict = accepted` |
| `CORRECTING` → `BUILDING` | `CorrectiveDirective` derived from an accepted `ReviewVerdict`, same `scope_ref` digest | reviewer prose |
| any → `HUMAN_REQUIRED` | `HumanAttentionRequest` (controller-raised); the phase left behind is stored as `suspended_from_phase` | an agent asking for a human |
| `HUMAN_REQUIRED` → a **named** phase | `HumanDecision` bound to the exact head, contract digest, and `state_version` the human saw; the target comes from the frozen action table (FD-14.6) and the exit rule of FD-14.7, never from implementation policy | an acknowledged alert — ACK leaves the phase unchanged |
| any → `CANCEL_REQUESTED` | `HumanDecision` (CANCEL) | a UI flag |
| `CANCEL_REQUESTED` → `CANCELLED` | the observed cancellation sequence completed (§3.10) | the request alone |
| merge | **not authorized by A1.** Merge stays manual, outside the system | any artifact in this document |

A later event never retroactively makes an earlier unsafe transition valid. If
the candidate head changes after review, the review is stale and the campaign
returns to the appropriate verification state
(`docs/decision-and-admission-protocol.md` §7).

### FD-13 — evidence is bound to the head it examined

Gate and CI results are admission evidence only when bound to the exact
`candidate_head` they ran against, per
`docs/decision-and-admission-protocol.md` §5. A result whose head is unknown, or
whose head is not the current candidate head, is diagnostic evidence: recorded,
never transition-bearing.

### FD-14 — campaign state, `state_version`, and the V0 reducer

A1-V0 must accept a verdict, move to `CORRECTING`, accept a new candidate, reach
`READY_TO_MERGE`, replay to the same state after restart, and validate
`expected_campaign_state_version` on a human command. That *is* a reducer.
Calling it a small orchestration loop would not change what it does, and bytes
are famously unmoved by euphemism. So A1 freezes the minimum reducer contract
here, A1-V0 implements exactly that minimum, and A2 extends it.

**FD-14.1 The state.** `CampaignStateV1` (§3.14) is the total state a V0
campaign needs. It is derived, never authored: no artifact carries it, and no
producer may submit it.

**FD-14.2 The reducer is pure, and genesis is a separate function.** A fold
whose first event creates the very state it folds into is a small lie about
initialization, so the seed is its own function:

A wire event carries a `source_ref` and an `event_payload_digest` — not the
values a transition depends on. `ReviewVerdictAccepted` needs the verdict's
`verdict`; `CandidateAccepted` needs the receipt's `candidate_head`;
`GateResultsAccepted` needs the payload's `results[]`. A pure function cannot go
and get those. So the boundary is explicit, in three stages:

```text
verify_wire:   CampaignEventV1 x log  -> ok | ChainInvalid
               sequence contiguity, digest chain, event_digest recomputation,
               state-version arithmetic. No CAS, no parsing — everything here
               is decidable from the event header and the log.

resolve_genesis: CampaignEventV1(CampaignCreated) x CAS x bound registries
                                     -> ResolvedCampaignEventV1 x CampaignPolicy
               the genesis payload is read under the PROTOCOL hard maximum
               (1 MiB, FD-1.4) because the campaign policy is what it contains;
               its own closure is then resolved under the policy it just yielded

resolve_event: CampaignEventV1 x CAS x CampaignPolicy x bound registries
                                     -> ResolvedCampaignEventV1 | ResolutionFailed
               1. fetch the event payload (bounded by max_control_artifact_bytes)
                  and parse it under its §3.15.2 schema
               2. fetch and slot-check source_ref under its §3 schema (FD-2.5)
               3. compute immediate_refs — which needs the parsed payload, so it
                  cannot happen before this stage
               4. resolve the closure under the FD-1.5 bounds
               5. attach the immutable registry/policy views the guards need

seed:  ResolvedCampaignEventV1(CampaignCreated)       -> CampaignStateV1
fold:  (CampaignStateV1, ResolvedCampaignEventV1)     -> CampaignStateV1
                                                       | TransitionRejected

replay(log) :=
  let (g, policy) = resolve_genesis(verify_wire(log[0]))
  fold*( seed(g), map(e -> resolve_event(verify_wire(e), policy), log[1..]) )
```

Closure checking lives in `resolve_event`, not in `verify_wire`, for a reason
worth stating once: `immediate_refs` of an event includes the refs declared by
its **payload**, and the payload is a separate CAS object. A stage with no CAS
access cannot enumerate them. `verify_wire` is the part that needs nothing but
bytes already in hand; everything requiring a fetch is one stage later, under
bounds, with the fetch itself bounded by the protocol maximum.

`ResolvedCampaignEventV1` is **not a new message kind and never persisted**: it
is the reducer's in-memory input type, holding the wire header plus the verified
payload, the verified source artifact when one is named, and the bound authority
views. Every one of those is content-addressed and immutable, so resolution is a
deterministic function of the log plus CAS — which is what keeps replay
reproducible while `fold` itself stays pure, total, clock-free, and I/O-free
(FD-8). Everything below that says "the fold reads X" means X arrived through
resolution.

Frozen well-formedness of a campaign log: `log[0].event_kind = CampaignCreated`
at `sequence = 0` with `state_version_before = 0` and `state_version_after = 1`;
`CampaignCreated` appears exactly once and never at any other sequence; sequences
are contiguous from 0; and the digest chain of §3.15 is continuous. A log failing
any of these is not replayed "as far as it goes" — it is refused.

**Terminal canonicalization.** Every transition into `CANCELLED`, `SUPERSEDED`,
or `TERMINAL_ERROR` clears, in the same step: `active_round_id`,
`active_execution`, and `suspended_from_phase`. Otherwise `CampaignCancelled`
would leave a terminal state carrying an `active_round_id` that §3.14 forbids in
terminal phases — an invariant violated by the very event that ends the campaign.

`fold` is total, deterministic, and has no clock, no I/O, and no provider
(FD-8). Given the same log and the same `campaign_protocol_version`, replay
yields the same `CampaignStateV1` byte for byte — the campaign-level analogue of
the per-run property `o7 replay` already verifies. Byte equality is only
meaningful with a canonical container order, so: every collection in
`CampaignStateV1` is stored sorted ascending by its id.

**FD-14.3 The event log has two classes.** The wire type is `CampaignEventV1`
(§3.15); class is a **function of `event_kind`**, never a field on the event —
a producer must not be able to declare its own event authority-bearing.

```text
authority-bearing (advance state_version by exactly 1):
  CampaignCreated              (via seed, 0 -> 1)
  WorkOrderIssued
  CandidateAccepted
  GateResultsAccepted
  CiResultsAccepted
  ReviewRequested
  ReviewVerdictAccepted
  CorrectiveDirectiveIssued
  HumanAttentionRaised
  HumanDecisionRecorded
  AttentionResolved
  AttentionSuperseded
  ProviderExecutionRecorded
  CampaignCancelled
  CampaignSuperseded
  CampaignTerminalError

evidence-only (never advance state_version, and never touch state at all):
  CoderReportReceived
  ReviewerReportReceived
  HumanCommandRejected
  TransitionRejected
  CampaignFeedItemEmitted
```

**Evidence-only means inert.** Frozen:

```text
an evidence-only event MUST NOT modify CampaignStateV1 in any field
except last_accepted_sequence.
```

Otherwise two states could share one `state_version` while differing in
substance, and a human's stale-command binding would stop meaning "the state you
were looking at". `ProviderExecutionRecorded` clears `active_execution`, which is
a real state change, so R3 moved it to the authority-bearing class rather than
letting it mutate state under an evidence label. That is the simpler of the two
honest repairs and it costs one `state_version` increment per execution.

There is no separate `CancelRequested` event: the `HumanDecision` whose `effect`
is `cancel_requested` *is* that transition, and minting a second event for one
act would advance `state_version` twice for a single human decision.

Frozen rule:

```text
state_version increases by exactly 1 on each accepted authority-bearing event.
Evidence events, projections, feed items, rejected commands, rejected
transitions, redelivery, and idempotent replay never change it.
last_accepted_sequence advances on EVERY accepted event of either class.
```

An acknowledgement is not an exception: the ACK's own `HumanDecisionRecorded` is
authority-bearing (+1) — a human who has acknowledged is looking at a different
campaign than one who has not — while the attention's derived move to
`ACKNOWLEDGED` happens inside that same fold step and carries no second
increment. ACK still never means `RESOLVED`.

Two counters, because they answer two different questions: `last_accepted_sequence`
is where the log is, `state_version` is what a human was looking at. A human's
stale-command check must not fire because a feed item scrolled past.

**FD-14.4 Guards come from FD-12.** A `fold` that receives an authority-bearing
event whose guard is unsatisfied returns `TransitionRejected` and advances
neither counter. The rejection is appended to the log as an evidence-only
`TransitionRejected` event; it never mutates state.

**FD-14.5 Attention lifecycle is derived, not stored on the artifact.** An
accepted artifact is immutable (§7), so a `lifecycle` field that walks from
`OPEN` to `ACKNOWLEDGED` inside a frozen `HumanAttentionRequest` would violate
the contract that artifact belongs to. The field is therefore gone from §3.9, and
attention state lives where mutable things belong — in the reduced state:

```text
AttentionRaised (HumanAttentionRaised)              -> OPEN
HumanDecisionRecorded(effect = acknowledged,
                      attention_id = A)             -> A: ACKNOWLEDGED
AttentionResolved(A)                                -> A: RESOLVED
AttentionSuperseded(A, superseded_by = B)           -> A: SUPERSEDED
```

`ACKNOWLEDGED` never becomes `RESOLVED` by itself, and only the controller emits
`AttentionResolved` — a human seeing a problem still does not make the problem
leave. What any of these events may do to `phase` is governed by FD-14.7, not by
the attention transition alone.

**FD-14.6 `HUMAN_REQUIRED` remembers where it came from, and every exit names its
target.** `HUMAN_REQUIRED` is entered from `BUILDING`, `GATING`, `CI_WAIT`,
`REVIEWING`, or `CORRECTING`, and the fold cannot be total unless the way back is
in the state rather than in an implementation's judgement. Frozen:
`CampaignStateV1.suspended_from_phase` is present **iff** `phase =
HUMAN_REQUIRED`, and it records the phase that was left.

The V1 attention-action set is closed — a controller may publish a subset, never
a new id — and each action's target phase is frozen:

| Decision | Target phase | Guard | Also |
|---|---|---|---|
| `ACK` | unchanged (`HUMAN_REQUIRED`) | the attention is `OPEN` | attention → `ACKNOWLEDGED` |
| `ANSWER_QUESTION`, `declared_scope_effect = none` | `BUILDING` | the named `AttentionEntry` exists and is `OPEN`/`ACKNOWLEDGED`; its `required_decision_kind = answer_question`; `question_id ∈ entry.open_question_ids`; controller finds no scope change | that id leaves `entry.open_question_ids`; when the entry has none left it becomes `RESOLVED` |
| `ANSWER_QUESTION`, `declared_scope_effect = revise_contract` | unchanged (`HUMAN_REQUIRED`); effect `contract_revision_requested`; no further autonomous dispatch | same membership guard | question closed; attention stays open |
| action `gather_more_evidence` | `GATING` | `current_candidate_head` present | attention → `RESOLVED` |
| action `retry_failed_step` | `suspended_from_phase` | that field is present | attention → `RESOLVED` |
| action `cancel_campaign` | `CANCEL_REQUESTED` | — | unconditional (FD-14.7 rule 5) |
| `CANCEL` | `CANCEL_REQUESTED` | none — `CANCEL` carries no `attention_id`, and the attention guard does not apply to it | unconditional |

Every phase target above is additionally subject to FD-14.7 rule 3: if another
attention remains `OPEN` or `ACKNOWLEDGED`, the phase stays `HUMAN_REQUIRED`.

**There is no `provide_answer` action.** `ANSWER_QUESTION` already exists, and
unlike a generic action id it can carry the answer's bytes; a second path that
transports no answer would be a mechanism for losing one.
`accept_residual_risk` is not in the V1 set (§3.10). An `action_id` outside this
table cannot be published by the controller or selected by a client.

**How a question becomes state.** `CoderReportReceived` is inert (FD-14.3), so a
question in a report changes nothing by itself. The controller selects which
questions to escalate and raises an attention carrying their ids; that
`HumanAttentionRaised` is what puts them in that attention's own
`AttentionEntry.open_question_ids` (§3.14) — there is no campaign-wide question
set. A question that
was never escalated is never answerable — which is correct, since nobody was
asked.

**FD-14.7a An ambiguous execution keeps the dispatch slot.** FD-9 forbids
redriving an execution whose dispatch outcome is unknown. That prohibition has to
live in the *state*, not only in the prose, because the two events involved —
`ProviderExecutionRecorded` and the `HumanAttentionRaised` that escalates it —
are separate appends, and a crash between them is exactly the case the rule
exists for. Frozen:

```text
ProviderExecutionRecorded(execution_outcome = dispatch_ambiguous)
    marks active_execution.unresolved = true and does NOT clear it

while active_execution is present, WorkOrderIssued / ReviewRequested /
CorrectiveDirectiveIssued all fail their guard — so no new dispatch is
admissible, and that block is durable across a restart

no V1 event clears an unresolved execution
```

The only exits are `CANCEL` (→ `CANCEL_REQUESTED` → `CANCELLED`) and
`CampaignSuperseded`, both of which end the campaign and clear the slot under
terminal canonicalization (FD-14.2). A1 deliberately adds no automatic
resolution path, mirroring R1's own choice for the same condition
(`docs/q-deck/r1-command.md` §11.6: "this round adds no automatic path for that
resolution"). A fresh identifier does not make a duplicate side effect safe, and
neither does a fresh campaign phase.

**FD-14.7 Leaving `HUMAN_REQUIRED`, and what a late attention event may not do.**
The action table alone is not enough: an attention resolved after the campaign
has already moved on could otherwise resurrect a phase nobody asked for. Frozen:

```text
1. Only HumanAttentionRaised may set phase = HUMAN_REQUIRED (§3.15.1).

2. An event may restore suspended_from_phase ONLY when the current phase is
   HUMAN_REQUIRED. In any other phase an attention event updates attention
   state and leaves phase exactly as it is.

3. The campaign leaves HUMAN_REQUIRED only when no attention remains OPEN or
   ACKNOWLEDGED. A decision that names an action target resolves its own
   attention in the same fold step; if others remain open, that attention still
   becomes RESOLVED but the phase stays HUMAN_REQUIRED and the target is not
   applied.

4. On any actual exit from HUMAN_REQUIRED, suspended_from_phase is cleared in
   the same step — the iff invariant of §3.14 never holds transiently false.

5. CANCEL is exempt from (3) and unconditional: it sets CANCEL_REQUESTED
   regardless of open attentions. Once the phase is CANCEL_REQUESTED, CANCELLED,
   SUPERSEDED, or TERMINAL_ERROR, no attention event may change it — a late
   AttentionResolved can never un-cancel a campaign.
```

Rule 5 is the one that stops the most embarrassing sequence: human cancels, a
reconciler resolves a stale attention a second later, and the campaign
cheerfully returns to `BUILDING`.

**FD-14.8 What A1 does not freeze here.** Progress frontier and `NO_PROGRESS`
semantics, terminal/escalation taxonomy beyond the phases listed, external
reconciliation, cumulative per-campaign evidence accounting (FD-1.5), budget
accounting beyond a stop condition, and the full incarnation taxonomy — all A2
(issue #94 §3, §5).

### FD-15 — human command binding and honest actor attestation

**FD-15.1 Binding.** Every `HumanCommandRequest` carries the binding fields in
§3.10 and is rejected unless `expected_campaign_state_version`,
`expected_contract_digest`, and (where applicable) `expected_head` all match
current canonical state. This closes the stale-screen TOCTOU window: a decision
applies to what the human actually saw, or it does not apply.

**FD-15.2 Claim and observation are separate objects.** `o7d` has no
authentication mechanism today — it binds loopback by default and refuses a
non-loopback bind without an explicit `--allow-non-loopback` flag
(`crates/o7d/src/main.rs:24–92`). That is a deployment policy, not an
authenticator, and the record must not imply otherwise.

A loopback connection proves exactly one thing: *the connection arrived over
loopback transport*. It does not prove that the sender is a human, that the
sender is the operator, that `actor_identity` belongs to the sender, or that the
local process is uncompromised. Every local process is local; the kernel does not
issue humanity at `accept()`.

Frozen split:

```text
HumanCommandRequest (untrusted, rank 3)
  claimed_actor_identity: ...        # a claim, and named as one

HumanDecision (accepted, rank 4)
  actor:
    claimed_identity: ...            # copied verbatim from the request
    authentication_strength: loopback_unauthenticated
                           | authenticated
                           | unattested
    observed_transport:    loopback | non_loopback
    authenticator_id:      absent | <id of the authenticator that verified it>
```

- `authentication_strength` and `observed_transport` are determined by the
  controller from the transport it actually observed. They are never read from
  the request, and the request has no field in which to assert them.
- `authenticated` requires a real authenticator; `authenticator_id` is then
  mandatory. Absent an authenticator, that value is unreachable.
- `loopback_unauthenticated` is admissible in V0 **only** under an explicit
  deployment policy, and it is recorded on every decision it produced — so an
  audit reads "a loopback caller claiming to be X", which is what happened,
  rather than "operator X", which was never established.
- `unattested` is refused.

## 3. Frozen wire schemas

**Notation.** Every table gives: field, type, required, constraints, and the
authority that establishes the value. Global rules that are not repeated per
row: explicit `null` is rejected everywhere (FD-1.3); optional means
absent-or-value; all bounds of FD-1.4 apply; payloads never restate an
envelope-owned field (FD-5.3).

Shared scalar types:

| Type | Form | Constraints |
|---|---|---|
| `Id` | opaque non-empty UTF-8 string, never parsed for meaning | ≤ 256 bytes |
| `Digest256` | lowercase hex SHA-256 | exactly 64 chars |
| `CommitId` | full object id, the repository's object-format width | never abbreviated (`docs/decision-and-admission-protocol.md` §4) |
| `ArtifactRef` | `{kind, media_type, digest: Digest256, size: u64}` | CAS-only (FD-1.8); slot-checked (FD-2.5) |
| `Text` | UTF-8 string | ≤ 65536 bytes unless stated |
| `Timestamp` | RFC 3339 UTC | metadata only (FD-5.4) |

Eleven envelope-bearing message kinds (§3.1–§3.11); three referenced typed
objects that carry no envelope of their own — `ProviderExecutionReceiptV1`
(§3.12), `InteractionManifestV1` (§3.12.1), `ScopeContractV1` (§3.13); one
derived object no producer may author (`CampaignStateV1`, §3.14); and the
reducer's own log entry (`CampaignEventV1`, §3.15).

### 3.0 Common envelope v1

| Field | Type | Req | Constraints | Authority |
|---|---|---|---|---|
| `envelope_version` | u32 | yes | `= 1` | protocol |
| `message_kind` | enum (11 kinds) | yes | closed enum | protocol |
| `message_kind_version` | u32 | yes | `= 1` | protocol |
| `message_id` | `Id` | yes | unique within `campaign_id` | producer; conflict per FD-6 |
| `root_goal_id` | `Id` | yes | — | controller binding (FD-5.1) |
| `task_id` | `Id` | yes | — | controller binding |
| `campaign_id` | `Id` | yes | — | controller binding |
| `round_id` | `Id` | no | absent only for campaign-scope artifacts | controller binding |
| `causation_id` | `Id` | no | absent only for the campaign-initiating `WorkOrder` | controller resolution |
| `correlation_id` | `Id` | yes | — | controller |
| `producer_role` | enum{`controller`,`coder`,`reviewer`,`human`} | yes | closed | controller (from the dispatch it made) |
| `producer_execution_id` | `Id` | yes | FD-10 | controller (minted before dispatch) |
| `producer_adapter_version` | `Text` | yes | ≤ 128 bytes | adapter build identity |
| `model_identity` | `Text` | iff role ∈ {coder, reviewer} | ≤ 256 bytes; logical identity for routing/policy, **not** runtime evidence | controller routing decision |
| `prompt_digest` | `Digest256` | iff role ∈ {coder, reviewer} | — | controller (prompt it sent) |
| `tool_policy_digest` | `Digest256` | iff role ∈ {coder, reviewer} | — | controller policy |
| `contract_digest` | `Digest256` | yes | must equal the campaign binding | campaign binding (FD-5.3) |
| `expected_input_head` | `CommitId` | iff the artifact reasons about code | — | controller state (FD-5.2) |
| `payload_digest` | `Digest256` | yes | digest of the exact stored payload bytes | computed |
| `artifact_refs` | `[ArtifactRef]` | yes (may be empty) | ≤ 256; rank rule (FD-2.1); closure bounds (FD-1.5) | producer; slot-checked |
| `provider_execution_receipt_ref` | `ArtifactRef` | iff role ∈ {coder, reviewer}; **forbidden** otherwise | congruence per FD-11 | controller |
| `created_at` | `Timestamp` | yes | metadata only; excluded from framing | producer observation |

`first_observed_at` is **not** an envelope field: it is controller-owned and
recorded on the acceptance record (FD-5.4).

### 3.1 WorkOrderV1 (Controller → Coder, rank 5)

| Field | Type | Req | Constraints | Authority |
|---|---|---|---|---|
| `role` | enum{`coder`} | yes | closed | controller |
| `goal.summary` | `Text` | yes | ≤ 8192 bytes | controller (from the frozen contract) |
| `input.base_sha` | `CommitId` | yes | — | A0 campaign base |
| `input.candidate_ref` | `ArtifactRef` | no | A0 `CandidateRef`; absent for the first round | imported root (FD-2.4) |
| `input.materialization_attestation_ref` | `ArtifactRef` | iff `candidate_ref` present | A0/`o7-worktree` attestation | imported root |
| `scope_ref` | `ArtifactRef` | yes | `ScopeContractV1` (§3.13) | controller-owned document |
| `required_evidence.gate_ids` | `[Id]` | yes | ≤ 256; every id resolvable in the registry | gate registry (FD-7) |
| `required_evidence.acceptance_case_ids` | `[Id]` | yes (may be empty) | ≤ 256 | contract |
| `verifier_policy_digest` | `Digest256` | yes | — | controller policy |
| `budget.max_provider_turns` | u32 | yes | ≥ 1 | campaign policy |
| `budget.max_wall_time_seconds` | u32 | yes | ≥ 1 | campaign policy |
| `budget.evidence_budget_bytes` | u64 | yes | ≤ the FD-1.5 hard maximum | campaign policy |
| `budget.closure_object_budget` | u32 | yes | ≤ `max_reachable_closure_objects` | campaign policy |
| `budget_policy_digest` | `Digest256` | yes | over the four budget values, framed per FD-1.5; must equal `CampaignStateV1.budget_policy_digest` | computed |
| `target_provider_execution_id` | `Id` | yes | the identity the coder execution will run under; minted by the controller **before** dispatch | controller |

`target_provider_execution_id` is what makes `active_execution` traceable to a
canonical source. It cannot be the envelope's `producer_execution_id`: on a
controller-issued artifact that field is the *controller's* execution identity
(FD-10), not the coder's. Every artifact that starts a provider execution
carries this field — `WorkOrder`, `ReviewRequest`, and `CorrectiveDirective` —
and the corresponding event requires `active_execution` to be absent and sets it
to exactly `{role, target_provider_execution_id}`. `ProviderExecutionRecorded`
then compares the receipt's `provider_execution_id` against an id whose origin
exists in canonical history.

The coder never receives "address the review comments". It receives the frozen
contract identity, concrete findings via a `CorrectiveDirective`, and explicit
scope limits.

### 3.2 CoderReportV1 (Coder → Controller, untrusted, rank 3)

| Field | Type | Req | Constraints | Authority |
|---|---|---|---|---|
| `status` | enum{`candidate_produced`,`failed`,`question_blocked`} | yes | closed | **claim** |
| `claimed_head` | `CommitId` | iff `status = candidate_produced` | checked against the derived candidate | **claim** |
| `claimed_state_digest` | `Digest256` | no | — | **claim** |
| `change_summary` | `Text` | yes | ≤ 16384 bytes | advisory |
| `intent` | `Text` | no | ≤ 16384 bytes | advisory |
| `claims[].claim_id` | `Id` | yes | unique within the report | advisory |
| `claims[].statement` | `Text` | yes | ≤ 4096 bytes | advisory |
| `claims[].evidence_refs` | `[ArtifactRef]` | yes (may be empty) | rank ≤ 2 | advisory |
| `diagnostic_runs[].command_recorded` | `Text` | yes | ≤ 4096 bytes; **never executed** (FD-7) | forensic |
| `diagnostic_runs[].result` | enum{`passed`,`failed`,`unknown`} | yes | closed | diagnostic only (FD-13) |
| `diagnostic_runs[].artifact_ref` | `ArtifactRef` | no | rank 0 | diagnostic only |
| `known_residuals` | `[Text]` | yes (may be empty) | ≤ 256 entries | advisory |
| `questions[].question_id` | `Id` | yes | unique within the campaign | producer |
| `questions[].text` | `Text` | yes | ≤ 8192 bytes | advisory |

There is no field by which a coder can emit `accepted`, and no field the
controller treats as admission evidence.

### 3.3 CandidateReceiptV1 (controller-derived, rank 4)

Derived **behind the A0 sealing boundary**, consumed as a capability and not
redefined:

```text
seal_candidate(worktree_attestation, producer_execution) -> CandidateRef
```

Quiescence means no live holder of write authority remains — a durably revoked or
advanced write-capability/lease epoch under which no previously issued writer
capability remains valid. Process termination and descendant absence are proof
mechanisms, not the definition (`docs/q-deck/a0-candidate-state.md`).

| Field | Type | Req | Constraints | Authority |
|---|---|---|---|---|
| `candidate_ref` | `ArtifactRef` | yes | A0 `CandidateRef` | imported root, produced by `seal_candidate` |
| `candidate_head` | `CommitId` | yes | — | controller-observed |
| `candidate_tree_identity` | `Text` | yes | tree object id | controller-observed |
| `base_ancestry` | `[CommitId]` | yes | ≤ 256 | controller-observed |
| `repository_identity` | A0 `RepositoryIdentity` | yes | — | `o7-worktree` canonical repo id |
| `changed_paths` | `[Text]` | yes | ≤ 4096 | controller-observed diff |
| `file_modes` | `[Text]` | yes | parallel to `changed_paths` | controller-observed |
| `diff_scope` | enum{`within_scope`,`out_of_scope`} | yes | computed against `scope_ref` | controller |
| `admission_profile` | enum{`LIGHTWEIGHT`,`STANDARD`,`STRICT`,`CRITICAL`} | yes | ≥ `STRICT` for autonomous mutation; ambiguity → stricter | controller classification over observed paths (#94 §4) |
| `applicable_gate_ids` | `[Id]` | yes | registry ids | gate registry |
| `sealed_under_epoch` | `Text` | yes | the revoked/advanced write epoch | A0 sealing boundary |
| `coder_report_ref` | `ArtifactRef` | yes | rank 3, forward reference (FD-2.3) | controller |
| `claim_check.claimed_head_matches` | bool | yes | `false` ⇒ fail closed, no review dispatch | controller comparison |

```text
coder says:      docs only
controller sees: verifier changed
-> CRITICAL or HUMAN_REQUIRED
```

### 3.4 ReviewRequestV1 (Controller → Reviewer, rank 5)

| Field | Type | Req | Constraints | Authority |
|---|---|---|---|---|
| `candidate_receipt_ref` | `ArtifactRef` | yes | rank 4 | controller |
| `candidate_head` | `CommitId` | yes | from the receipt, never from a `CoderReport` | controller |
| `base_sha` | `CommitId` | yes | — | campaign base |
| `scope_ref` | `ArtifactRef` | yes | same digest as the `WorkOrder`'s | controller |
| `required_properties` | `[Text]` | yes | ≤ 256 entries | contract |
| `required_evidence_gate_ids` | `[Id]` | yes | registry ids | gate registry |
| `verifier_policy_digest` | `Digest256` | yes | — | controller policy |
| `evidence_refs` | `[ArtifactRef]` | yes | **ordered**: contract, diff, deterministic evidence first | controller |
| `coder_report_ref` | `ArtifactRef` | no | delivered last, labeled advisory | controller |
| `target_provider_execution_id` | `Id` | yes | the reviewer execution's identity, minted before dispatch (§3.1) | controller |

Input ordering is normative, not stylistic.

### 3.5 ReviewerReportV1 (Reviewer → Controller, untrusted, rank 3)

| Field | Type | Req | Constraints | Authority |
|---|---|---|---|---|
| `reviewed_head` | `CommitId` | yes | stale ⇒ never promoted (FD-5.2) | **claim** |
| `verdict` | enum{`accepted`,`changes_requested`,`blocked`} | yes | closed; authorizes nothing (FD-4) | **claim** |
| `findings[].finding_id` | `Id` | yes | unique within the report | producer |
| `findings[].severity` | enum{`blocker`,`major`,`minor`,`note`} | yes | closed | **claim** |
| `findings[].affected_property` | `Text` | yes | ≤ 4096 bytes | **claim** |
| `findings[].evidence_refs` | `[ArtifactRef]` | yes (may be empty) | rank ≤ 2; resolvable | **claim** |
| `findings[].required_change` | `Text` | yes | ≤ 8192 bytes | **claim** |
| `findings[].required_evidence_gate_ids` | `[Id]` | yes (may be empty) | **registry ids only**; unknown ⇒ `HUMAN_REQUIRED` (FD-7) | gate registry |
| `findings[].proposed_evidence_prose` | `Text` | no | ≤ 4096 bytes; inert, never executed | **claim** |
| `properties_checked` | `[Text]` | yes | ≤ 256 entries | **claim** |
| `properties_preserved` | `[Text]` | yes | ≤ 256 entries | **claim** |
| `residual_risks` | `[Text]` | yes (may be empty) | ≤ 256 entries | **claim** |

There is no `review_id` field, and — since R2 — no `reviewer.identity`,
`reviewer.model`, or `reviewer.prompt_version` either. A model telling the
controller which model it was is not evidence; it is a sentence. Reviewer
provenance is derived controller-side in §3.6 from the execution receipt and the
prompt registry. Under `deny_unknown_fields` (FD-1.6), a report that includes
those fields anyway is rejected at parse time.

### 3.6 ReviewVerdictV1 (controller-accepted, rank 4)

Accepted only when **all** hold:

```text
FD-11 congruence passes
ReviewerReport schema-valid, supported version, within bounds
reviewed_head == CampaignStateV1.current_candidate_head
envelope.contract_digest == the campaign binding
every finding_id unique; every required_evidence_gate_id resolves
every evidence_ref resolvable, rank rule and closure bounds satisfied
reviewer independence preconditions recorded (§4)
```

| Field | Type | Req | Constraints | Authority |
|---|---|---|---|---|
| `review_id` | `Id` | yes | controller-minted | controller |
| `reviewed_head` | `CommitId` | yes | `== current_candidate_head` | controller-verified |
| `verdict` | enum{`accepted`,`changes_requested`,`blocked`} | yes | closed | controller acceptance of the claim |
| `findings` | as §3.5, validated | yes | every `finding_id` exists in the referenced report | derived from the report |
| `properties_checked` / `properties_preserved` / `residual_risks` | as §3.5 | yes | — | derived from the report |
| `reviewer.provider_identity` | `Text` | yes | — | `receipt.provider.identity` |
| `reviewer.requested_model` | `Text` | yes | — | `receipt.model.requested_model` |
| `reviewer.resolved_model` | `Text` | no | absent unless `receipt.model.resolution.status = provider_reported` | receipt (FD-3) |
| `reviewer.prompt_digest` | `Digest256` | yes | — | `receipt.request.prompt_digest` |
| `reviewer.prompt_version` | `Text` | no | absent when the digest is not in the registry | controller registry lookup of `prompt_digest` |
| `reviewer_report_ref` | `ArtifactRef` | yes | rank 3; the source of truth for every finding field | controller |
| `accepted_under.verifier_policy_digest` | `Digest256` | yes | — | controller policy |
| `accepted_under.gate_registry_digest` | `Digest256` | yes | — | gate registry |

The verdict's derived fields are a validated projection of the report, and
`reviewer_report_ref` remains the source of truth: the controller does not
improve a reviewer's findings on the way through.

### 3.7 CorrectiveDirectiveV1 (Controller → Coder, rank 5)

| Field | Type | Req | Constraints | Authority |
|---|---|---|---|---|
| `derived_from_review_id` | `Id` | yes | must name an accepted verdict | controller |
| `review_verdict_ref` | `ArtifactRef` | yes | rank 4 | controller |
| `target_finding_ids` | `[Id]` | yes | non-empty; every id exists in that verdict | controller |
| `required_changes` | `[Text]` | yes | ≤ 256 entries, ≤ 8192 bytes each | derived from the verdict |
| `required_evidence_gate_ids` | `[Id]` | yes | registry ids | gate registry |
| `scope_ref` | `ArtifactRef` | yes | **digest identical** to the `WorkOrder`'s `scope_ref` | controller |
| `budget_policy_digest` | `Digest256` | yes | must equal `CampaignStateV1.budget_policy_digest`; the four values are **not** repeated here — the directive inherits the immutable campaign policy and restates only its digest | campaign policy |
| `target_provider_execution_id` | `Id` | yes | the corrective coder execution's identity, minted before dispatch (§3.1) | controller |

A `CorrectiveDirective` **starts the next coder execution directly**; it does not
transition to `BUILDING` and then wait for a fresh `WorkOrder`. The topology
(§1) and the V0 cycle (§5.3) both say directive → coder, and R5 makes the
reducer agree: `CorrectiveDirectiveIssued` sets `active_execution` and opens a
new round, exactly as `WorkOrderIssued` does for the first round. The alternative
— a directive followed by a second `WorkOrder` referencing it — was the other
consistent protocol, and picking neither was the only wrong option.

Scope equality is expressed as one immutable `ScopeContractV1` referenced by
digest from both artifacts (§3.13) — not as a "byte-identical" comparison between
two independently serialized payloads, which FD-1.2 deliberately makes
meaningless.

A directive that would reference a different `scope_ref` digest, or a different
`contract_digest`, is not a directive: it is a contract revision, and it fails
closed into `HUMAN_REQUIRED` with reason `SCOPE_EXPANSION_REFUSED`.
`changes_requested` is never permission to modify the frozen goal, acceptance
contract, verifier, or baseline.

Rounds are **forward-only**: a corrective round produces a new candidate head and
never amends, rebases, or force-pushes an earlier one
(`docs/decision-and-admission-protocol.md` §8).

### 3.8 CampaignFeedItemV1 (Controller → Human, rank 5)

Informative lifecycle history. No acknowledgement, no decision, and never a
transition (FD-14.3).

| Field | Type | Req | Constraints | Authority |
|---|---|---|---|---|
| `item_kind` | enum{`candidate_produced`,`gates_completed`,`ci_completed`,`review_completed`,`round_started`,`command_rejected`,`execution_recorded`,`ready_to_merge`} | yes | closed | controller |
| `summary` | `Text` | yes | ≤ 4096 bytes | controller |
| `subject_refs` | `[ArtifactRef]` | yes (may be empty) | rank ≤ 4 | controller |
| `at_sequence` | u64 | yes | the log sequence being described | controller |
| `at_state_version` | u64 | yes | unchanged by this item | controller |

### 3.9 HumanAttentionRequestV1 (Controller → Human, rank 5)

| Field | Type | Req | Constraints | Authority |
|---|---|---|---|---|
| `attention_id` | `Id` | yes | stable canonical identity | controller |
| `dedupe_key` | `Text` | yes | ≤ 512 bytes; **computed by the controller**, never by an agent | controller |
| `candidate_head` | `CommitId` | no | absent when no candidate exists yet | controller state |
| `reason.code` | enum: `HUMAN_REQUIRED`, `CODER_QUESTION_BLOCKED`, `EXTERNAL_DRIFT`, `ID_CONFLICT`, `RECEIPT_INCONGRUENT`, `DISPATCH_AMBIGUOUS`, `AGENT_FAILED`, `NO_PROGRESS`, `CONFLICTING_EVIDENCE`, `BUDGET_EXHAUSTED`, `EVIDENCE_BUDGET_EXCEEDED`, `CI_FAILED_REPEATEDLY`, `SCOPE_EXPANSION_REFUSED`, `UNMAPPED_EVIDENCE_PROPOSAL`, `CONTRACT_REVISION_REQUESTED` | yes | closed; additive codes need a new kind version | controller |
| `reason.summary` | `Text` | yes | ≤ 4096 bytes | controller |
| `severity` | enum{`info`,`attention`,`urgent`} | yes | closed | controller |
| `required_decision_kind` | enum{`ack`,`choose_resolution`,`answer_question`} | yes | closed; there is no `none` — an attention nobody can act on is a feed item (§3.8) | controller |
| `question_ids` | `[Id]` | iff `answer_question` | **non-empty**, ≤ 256; every id from an accepted `CoderReport`'s `questions[]`, selected by the controller | controller |
| `options[].action_id` | `Id` | iff `choose_resolution` | **at least one**; each from the closed V1 action set (FD-14.6); a client may only select, never compose | controller |
| `options[].consequence` | `Text` | yes with the option | ≤ 4096 bytes | controller |
| `evidence_refs` | `[ArtifactRef]` | yes | rank ≤ 4 | controller |
| `suspended_from_phase` | Phase (§3.14) | yes | one of `BUILDING`,`GATING`,`CI_WAIT`,`REVIEWING`,`CORRECTING` — never `READY_TO_MERGE` or a terminal phase | reducer (FD-14.6) |
| `raised_at_state_version` | u64 | yes | what the human is looking at | reducer (FD-14) |

The decision surface must be actionable, which is now schema-enforced rather
than merely asserted:

```text
answer_question    => question_ids non-empty, options absent
choose_resolution  => options non-empty, question_ids absent
ack                => neither questions nor options
```

An attention carrying neither is what §3.8 calls a feed item.

There is **no `lifecycle` field**: this artifact is immutable like every other
accepted artifact, and a status that walks from `OPEN` to `ACKNOWLEDGED` inside a
frozen record would break that. Attention state is derived by the reducer
(FD-14.5) and lives in `CampaignStateV1.attention`. Being raised *is* `OPEN`.

Repeated reconciliation resolves to the same `attention_id` via `dedupe_key` and
appends no event at all; occurrence counts live in projection. A new head or a
new problem class mints a **new** identity, and the previous one is closed by an
explicit `AttentionSuperseded`. **ACK ≠ RESOLVED.**

Identity is frozen tightly enough that two implementations cannot disagree:

```text
attention_id is unique for the lifetime of the campaign — never reused,
  not even after RESOLVED or SUPERSEDED
HumanAttentionRaised   requires the id to be UNKNOWN to the state
AttentionSuperseded    requires the superseded one to be OPEN or ACKNOWLEDGED,
                       and the successor to be OPEN
```

Reusing a `SUPERSEDED` id was previously permitted by the guard and forbidden by
the prose, which left "replace the record" and "append a second one" both
defensible. Neither is now reachable.

**`READY_TO_MERGE` is not an attention.** It is a phase the reducer has already
reached, and the notification is a `CampaignFeedItem` (§3.8). Raising an
attention there would have to store `suspended_from_phase = READY_TO_MERGE`,
which §3.14 forbids — and the required V0 happy path (§5.3) walks straight into
it, so this was not an edge case but the main road. Merge stays manual either
way; what changes is that the system stops pretending a human owes it a decision
it cannot express.

### 3.10 HumanCommandRequestV1 (Human → Controller, untrusted, rank 3)

| Field | Type | Req | Constraints | Authority |
|---|---|---|---|---|
| `command_id` | `Id` | yes | — | client |
| `idempotency_key` | `Text` | yes | ≤ 256 bytes; unique within the campaign | client; conflict per FD-6 |
| `command` | enum{`ACK`,`CANCEL`,`ANSWER_QUESTION`,`SELECT_ATTENTION_ACTION`} | yes | closed | client |
| `claimed_actor_identity` | `Text` | yes | ≤ 256 bytes; **a claim** (FD-15.2) | client |
| `authorization_context` | `Text` | no | ≤ 4096 bytes; opaque, non-authoritative | client |
| `expected_campaign_state_version` | u64 | yes | must equal current (FD-15.1) | reducer state |
| `expected_contract_digest` | `Digest256` | yes | must equal the campaign binding | campaign binding |
| `expected_head` | `CommitId` | iff the command concerns a candidate | must equal `current_candidate_head` | reducer state |
| `attention_id` | `Id` | iff `ACK`, `SELECT_ATTENTION_ACTION`, or `ANSWER_QUESTION` | must name an `OPEN`/`ACKNOWLEDGED` attention entry; `ANSWER_QUESTION` needs it too, or the fold cannot tell which attention the question belongs to | controller |
| `selected_action_id` | `Id` | iff `SELECT_ATTENTION_ACTION` | must be in that entry's `offered_action_ids` (§3.14) | controller |
| `question_id` | `Id` | iff `ANSWER_QUESTION` | must be in the named attention entry's `open_question_ids` (FD-14.6) | controller |
| `answer.text` | `Text` | iff `ANSWER_QUESTION` | ≤ 16384 bytes | human |
| `answer.declared_scope_effect` | enum{`none`,`revise_contract`} | iff `ANSWER_QUESTION` | closed | human declaration |

There is deliberately **no** field for attestation, transport, or authenticator:
those are observations, and a request cannot observe itself (FD-15.2).

**ANSWER_QUESTION resolution:**

```text
declared none + controller finds no scope change -> delivered as clarification;
                                                    phase BUILDING (FD-14.6)
declared revise_contract                         -> effect contract_revision_requested;
                                                    phase stays HUMAN_REQUIRED;
                                                    no further autonomous dispatch
ambiguous                                        -> HUMAN_REQUIRED, explicit re-ask
```

A contract revision never continues the same campaign, and it also does not
terminate it in the same breath. The old campaign cannot become `SUPERSEDED` at
the moment of the answer, because the successor it would name does not exist yet
— and `CampaignStateV1` requires `superseded_by` in that phase. So the sequence
has one authority point per fact:

```text
HumanDecision(revise_contract)  -> contract_revision_requested, still HUMAN_REQUIRED
out of band                     -> the new contract is frozen; a successor
                                   campaign is minted with `supersedes`
CampaignSuperseded{superseded_by}-> phase SUPERSEDED, superseded_by recorded
```

V1 has **no** `ContractRevisionProposal` artifact; adding one would be a new
message kind under §7, not a footnote.

**CANCEL is a process, not a flag:**

```text
HumanDecisionRecorded(effect = cancel_requested) -> phase CANCEL_REQUESTED
-> prevent new dispatches
-> request termination of the active execution
-> observe process/worktree state
-> cleanup or preserve forensic state
-> CampaignCancelled (observations attached)     -> phase CANCELLED
```

A UI showing "cancelled" while the coder is still writing files is a lovely
interface to the wrong reality.

Deferred post-v1: `PAUSE`, `RESUME`, `REQUEST_MORE_EVIDENCE` as a standalone
command, `APPROVE_RESIDUAL_RISK`, `REJECT_CANDIDATE`, `AUTHORIZE_MERGE`.
`accept_residual_risk` exists in the vocabulary and is not offered in v1.

### 3.11 HumanDecisionV1 (controller-accepted, rank 4)

| Field | Type | Req | Constraints | Authority |
|---|---|---|---|---|
| `decision_id` | `Id` | yes | controller-minted | controller |
| `command_request_ref` | `ArtifactRef` | yes | rank 3 | controller |
| `command` | enum as §3.10 | yes | closed | controller-verified |
| `actor.claimed_identity` | `Text` | yes | copied verbatim from the request | **claim, preserved as one** |
| `actor.authentication_strength` | enum{`loopback_unauthenticated`,`authenticated`,`unattested`} | yes | `unattested` ⇒ refused; never read from the request | controller observation |
| `actor.observed_transport` | enum{`loopback`,`non_loopback`} | yes | never read from the request | controller observation |
| `actor.authenticator_id` | `Id` | iff `authenticated` | — | the authenticator |
| `effect` | enum{`acknowledged`,`cancel_requested`,`answer_delivered`,`attention_action_selected`,`contract_revision_requested`} | yes | closed | reducer (FD-14) |
| `applied_at_state_version` | u64 | yes | the version this decision advanced *to* | reducer |

### 3.12 ProviderExecutionReceiptV1 (rank 2, no envelope)

One receipt per `provider_execution_id`, with dispatch records nested (FD-10.1).

| Field | Type | Req | Constraints | Authority |
|---|---|---|---|---|
| `schema_version` | u32 | yes | `= 1` | protocol |
| `provider_execution_id` | `Id` | yes | — | controller (minted before dispatch) |
| `retry_of_execution_id` | `Id` | no | present iff a whole-execution retry (FD-10.3) | controller |
| `campaign_id` / `round_id` | `Id` | yes | congruence per FD-11 | controller binding |
| `producer_role` | enum{`coder`,`reviewer`} | yes | closed | controller |
| `producer_adapter_version` | `Text` | yes | ≤ 128 bytes | adapter build |
| `provider.identity` | `Text` | yes | ≤ 256 bytes | adapter |
| `provider.endpoint_mode` | `Text` | yes | ≤ 128 bytes | adapter |
| `provider.request_id` | `Text` | no | absent when the provider returned none | provider |
| `model.requested_model` | `Text` | yes | ≤ 256 bytes | controller routing |
| `model.resolution.status` | enum{`provider_reported`,`fingerprint_only`,`unavailable`} | yes | closed | adapter observation |
| `model.resolution.provider_reported_model` | `Text` | iff `provider_reported` | never inferred from an alias (FD-3) | provider |
| `model.resolution.backend_fingerprint` | `Text` | iff `fingerprint_only` | — | adapter observation |
| `request.prompt_digest` | `Digest256` | yes | congruence per FD-11 | controller |
| `request.tool_policy_digest` | `Digest256` | yes | congruence per FD-11 | controller policy |
| `request.decoding_policy_digest` | `Digest256` | yes | — | adapter |
| `request.budget_policy_digest` | `Digest256` | yes | — | campaign policy |
| `dispatches[]` | list | yes | non-empty; ≤ 256 | see below |
| `interaction_manifest_ref` | `ArtifactRef` | yes | rank 1 (§3.12.1) | controller |
| `final_normalized_output_ref` | `ArtifactRef` | iff `execution_outcome = completed` | rank 0; **its digest is the artifact's `payload_digest`** (FD-11) | adapter |
| `execution_outcome` | enum{`completed`,`refused`,`incomplete`,`failed_pre_dispatch`,`dispatch_ambiguous`} | yes | derived from `dispatches`, fail-closed (FD-9) | controller |

Each `dispatches[]` entry:

| Field | Type | Req | Constraints | Authority |
|---|---|---|---|---|
| `dispatch_id` | `Id` | yes | unique within the execution | controller |
| `sequence` | u32 | yes | 0-based, contiguous | controller |
| `kind` | enum{`initial`,`continuation`} | yes | a continuation is **not** a retry (FD-10.3) | controller |
| `retry_of_dispatch_id` | `Id` | no | present iff a single-dispatch retry; forbidden when `kind = continuation` | controller |
| `canonical_request_ref` | `ArtifactRef` | yes | rank 0; the exact provider-facing request | adapter |
| `raw_provider_event_ref` | `ArtifactRef` | no | rank 0; `o7 invoke`'s `stdout.raw` in V0 | provider bytes |
| `normalized_output_ref` | `ArtifactRef` | no | rank 0; `o7 invoke`'s `result.json` in V0 | adapter |
| `dispatch_boundary` | enum{`not_reached`,`reached`,`ambiguous`} | yes | R1 §11 semantics, per dispatch | durable boundary record |
| `outcome` | enum{`completed`,`refused`,`incomplete`,`failed_pre_dispatch`,`dispatch_ambiguous`} | yes | closed | controller |
| `provider_error_code` | `Text` | no | e.g. an `o7 invoke` `BLOCKED_*`/`FAIL_*` kind | adapter |
| `usage_ref` / `cost_ref` | `ArtifactRef` | no | rank 0 | provider |

Derivation rule for `execution_outcome`, frozen:

```text
if any dispatch.dispatch_boundary == ambiguous
   or any dispatch.outcome        == dispatch_ambiguous
       -> dispatch_ambiguous

else if EVERY dispatch.dispatch_boundary == not_reached
       -> failed_pre_dispatch

else if the LAST dispatch.dispatch_boundary == not_reached
       -> incomplete

else   -> the last dispatch's own outcome
```

The third branch is the one that matters. An execution whose first dispatch
`reached` the boundary and completed, and whose continuation never left the
building, has **already produced an external side effect**. Labelling it
`failed_pre_dispatch` would invite a whole-execution retry — a picturesque way to
repeat the history half of this document exists to forbid. `failed_pre_dispatch`
means every dispatch stayed pre-boundary, and nothing weaker.

**Terminal-output binding.** FD-11 proves a report's bytes equal some blob named
by the receipt; this rule is what makes that blob the execution's actual final
answer:

```text
execution_outcome == completed
  =>  the last dispatch has boundary == reached
  and the last dispatch has outcome  == completed
  and the last dispatch has normalized_output_ref present
  and receipt.final_normalized_output_ref == that dispatch's normalized_output_ref
      (same kind AND same digest)
```

Any other pairing is a malformed receipt, rejected before FD-11 runs.

The receipt carries no reference to any artifact accepted from it (FD-2.3).

#### 3.12.1 InteractionManifestV1 (rank 1, no envelope)

| Field | Type | Req | Constraints | Authority |
|---|---|---|---|---|
| `schema_version` | u32 | yes | `= 1` | protocol |
| `provider_execution_id` | `Id` | yes | must equal the receipt's | controller |
| `interaction_sequence[].sequence` | u32 | yes | 0-based, contiguous | controller |
| `interaction_sequence[].dispatch_id` | `Id` | iff `kind ≠ tool_result` | must exist in the receipt's `dispatches` | controller |
| `interaction_sequence[].kind` | enum{`provider_request`,`provider_continuation`,`tool_call_requested`,`tool_result`,`provider_error`} | yes | closed | observation |
| `interaction_sequence[].input_ref` / `.output_ref` | `ArtifactRef` | per kind | rank 0 | observation |
| `interaction_sequence[].tool_call_id` | `Id` | iff a tool kind | pairs request with result | observation |
| `interaction_sequence[].tool_id` | `Id` | iff `tool_call_requested` | registry id; recording ≠ authorizing (FD-7) | registry |
| `interaction_sequence[].arguments_ref` / `.result_ref` | `ArtifactRef` | per kind | rank 0 | observation |

The manifest records externally observable requests, responses, tool calls, tool
results, errors, and ordering. It claims no access to hidden provider reasoning.
A `tool_result` entry exists only for a tool the controller itself resolved and
executed. Partial history is still evidence: if dispatch occurred but the final
result is unavailable, the execution is `dispatch_ambiguous` and recovery never
asks the provider to recreate the missing answer (FD-8, FD-9).

### 3.13 ScopeContractV1 (rank 0 local typed leaf, no envelope)

| Field | Type | Req | Constraints | Authority |
|---|---|---|---|---|
| `schema_version` | u32 | yes | `= 1` | protocol |
| `scope_id` | `Id` | yes | — | controller |
| `allowed_paths` | `[Text]` | yes | ≤ 4096 entries | contract |
| `forbidden_paths` | `[Text]` | yes (may be empty) | ≤ 4096 entries | contract |
| `frozen_properties` | `[Text]` | yes (may be empty) | ≤ 256 entries | contract |

Immutable. `WorkOrder` and every `CorrectiveDirective` in the campaign reference
the same digest; scope equality is digest equality (§3.7).

### 3.14 CampaignStateV1 (derived, never authored)

No producer may submit this object, and it carries no envelope: it is the output
of the FD-14 fold.

| Field | Type | Req | Constraints | Authority |
|---|---|---|---|---|
| `schema_version` | u32 | yes | `= 1` | protocol |
| `campaign_protocol_version` | u32 | yes | `= 1`; bound at campaign creation | protocol |
| `campaign_id` | `Id` | yes | — | controller binding |
| `state_version` | u64 | yes | +1 per accepted authority-bearing event only (FD-14.3) | reducer |
| `last_accepted_sequence` | u64 | yes | +1 per accepted event of either class | reducer |
| `phase` | enum{`BUILDING`,`GATING`,`CI_WAIT`,`REVIEWING`,`CORRECTING`,`HUMAN_REQUIRED`,`READY_TO_MERGE`,`CANCEL_REQUESTED`,`CANCELLED`,`SUPERSEDED`,`TERMINAL_ERROR`} | yes | closed; terminal = `CANCELLED`, `SUPERSEDED`, `TERMINAL_ERROR` | reducer |
| `suspended_from_phase` | Phase | **iff** `phase = HUMAN_REQUIRED` | one of `BUILDING`,`GATING`,`CI_WAIT`,`REVIEWING`,`CORRECTING` (FD-14.6) | reducer |
| `current_candidate_head` | `CommitId` | no | absent until the first `CandidateAccepted` | reducer |
| `contract_digest` | `Digest256` | yes | the campaign binding | reducer |
| `scope_digest` | `Digest256` | yes | the `ScopeContractV1` digest | reducer |
| `active_round_id` | `Id` | no | absent in terminal phases | reducer |
| `active_execution` | `{role, provider_execution_id, unresolved}` | no | absent when no execution is in flight; `unresolved = true` marks an execution whose dispatch outcome is ambiguous (FD-9) | reducer |
| `attention` | `[AttentionEntry]` | yes (may be empty) | ≤ 256; sorted ascending by `attention_id` | reducer (FD-14.5) |
| `budget_policy` | `{max_provider_turns, max_wall_time_seconds, evidence_budget_bytes, closure_object_budget}` | yes | from `CampaignCreated`; immutable for the campaign | seed (FD-1.5) |
| `budget_policy_digest` | `Digest256` | yes | framed over the four values above (FD-1.5) | seed |
| `required_gate_ids` | `[Id]` | no | absent until the first `CandidateAccepted`; then exactly the receipt's `applicable_gate_ids`, sorted | reducer (§3.15.2) |
| `required_ci_check_ids` | `[Id]` | yes (may be empty) | from `CampaignCreated`, sorted; immutable for the campaign | seed |
| `gate_registry_digest` | `Digest256` | yes | from `CampaignCreated`; immutable for the campaign | seed |
| `last_gate_results` | `{bound_head, all_required_passed}` | no | absent before the first `GateResultsAccepted` | reducer |
| `last_ci_results` | `{bound_head, conclusion}` | no | absent before the first `CiResultsAccepted` | reducer |
| `supersedes` / `superseded_by` | `Id` | no | campaign lineage (FD-5.1); `superseded_by` required iff `phase = SUPERSEDED` | reducer |

`AttentionEntry` — the decision surface the fold has to reproduce, kept with the
attention it belongs to rather than in a parallel flat list:

| Field | Type | Req | Constraints |
|---|---|---|---|
| `attention_id` | `Id` | yes | unique for the campaign's lifetime (§3.9) |
| `state` | enum{`OPEN`,`ACKNOWLEDGED`,`RESOLVED`,`SUPERSEDED`} | yes | closed |
| `required_decision_kind` | enum{`ack`,`choose_resolution`,`answer_question`} | yes | copied from the raised artifact |
| `offered_action_ids` | `[Id]` | yes (may be empty) | exactly the `options[].action_id` published; sorted |
| `open_question_ids` | `[Id]` | yes (may be empty) | the still-unanswered subset of the artifact's `question_ids`; sorted |

There is no top-level `open_question_ids`. A flat list could not tell the fold
which attention a question belonged to, could not reproduce the
`selected_action_id ∈ offered options` check once the artifact had scrolled out
of state, and would strand a resolved attention's questions as permanently open.
The same three facts live in one place instead:

```text
ANSWER_QUESTION       finds the question inside its own attention entry
SELECT_ATTENTION_ACTION  checks selected_action_id ∈ that entry's offered_action_ids
AttentionResolved
  / AttentionSuperseded  closes that entry's remaining open_question_ids

a question_id may be open in at most ONE active attention at a time
```

Head-bound gate and CI results are kept in state because FD-12's guards are
stated over them: `READY_TO_MERGE` requires that the recorded results are bound
to the *current* head, so a head change silently invalidates them (FD-13) instead
of leaving a stale green in the guard's line of sight.


### 3.15 CampaignEventV1 (the reducer's wire contract, no envelope)

Without this type, two implementations could implement all eleven message kinds
identically and still build different campaign logs — making `CampaignStateV1`
deterministic only *within* one implementation, which would take some of the
ceremony out of the word "protocol".

| Field | Type | Req | Constraints | Authority |
|---|---|---|---|---|
| `schema_version` | u32 | yes | `= 1` | protocol |
| `campaign_protocol_version` | u32 | yes | `= 1`; equal to the campaign binding | protocol |
| `campaign_id` | `Id` | yes | equal to the binding | controller |
| `sequence` | u64 | yes | 0-based, contiguous, gapless | controller |
| `previous_event_digest` | `Digest256` | yes | at `sequence = 0` it is exactly `Digest256::genesis()` — the all-zero canonical digest already frozen for run events (`crates/o7-run/src/event.rs:50–53`) | chain |
| `event_digest` | `Digest256` | yes | framed digest of this event (below) | computed |
| `event_kind` | enum (21 kinds, FD-14.3) | yes | closed; **class is a function of kind, never a field** | protocol |
| `state_version_before` | u64 | yes | must equal the folded state | reducer, self-checking |
| `state_version_after` | u64 | yes | `+1` for authority-bearing kinds, `+0` for evidence-only | reducer, self-checking |
| `source_ref` | `ArtifactRef` | per kind (§3.15.1) | the expected `kind` and its allowed rank are fixed **per `event_kind`**, not globally | controller |
| `evidence_refs` | `[ArtifactRef]` | yes (may be empty) | ≤ 256; rank ≤ 4 | controller |
| `event_payload_digest` | `Digest256` | iff the kind carries a payload (§3.15.2) | SHA-256 over the exact stored payload bytes, as for every other artifact (FD-1.1) | computed |

The field is `source_ref`, not `authority_ref`, because the same slot carries the
`CoderReport` of a `CoderReportReceived` — a rank-3 untrusted report that
authorizes nothing (FD-4). Making the word "authority" cover that would be a
small lie in a load-bearing place. So:

```text
authority-bearing event  ->  source_ref is the transition authority
evidence-only event      ->  source_ref is provenance only, never authority
```

`state_version_before`/`_after` are stored, not merely computed, so a log
tampered into a different transition count fails its own arithmetic before the
reducer has to notice.

**The payload is a separate stored object.** `CampaignEventPayloadV1` is stored
as its own immutable byte string in CAS, exactly like an artifact payload
(FD-1.1); the event carries only its digest. There is no inline payload field and
therefore no second serialization anyone has to agree on.

Event digest framing, same discipline as everything else (FD-1.2):

```text
h.update(b"o7-a1-campaign-event\0v1\0")
frame(schema_version.to_le_bytes()) frame(campaign_protocol_version.to_le_bytes())
frame(campaign_id) frame(sequence.to_le_bytes())
frame(previous_event_digest)
frame(event_kind name)
frame(state_version_before.to_le_bytes()) frame(state_version_after.to_le_bytes())
frame(source_ref present flag byte: 0 or 1), then when present the FULL ref:
    frame(artifact_kind name) frame(media_type) frame(digest) frame(size.to_le_bytes())
frame(evidence_refs count.to_le_bytes()), then each ref framed identically
frame(event_payload_digest, or the empty string when absent)
```

The `source_ref` is framed in full — `kind`, `media_type`, `digest`, `size` —
not by digest alone. A ref whose declared kind or media type changed while its
digest stayed the same is a different reference (FD-1.7, FD-2.5), and the event
digest has to notice.

#### 3.15.1 Per-kind contract

`A` = authority-bearing, `E` = evidence-only. `source_ref` names the expected
artifact kind and its rank; `—` means the kind carries no `source_ref`.

Two invariants hold for every row and are checked before the guard:

```text
evidence-only events MUST NOT modify CampaignStateV1 at all,
  except last_accepted_sequence.
HumanAttentionRaised is the ONLY event that may set phase = HUMAN_REQUIRED.
```

| `event_kind` | Class | `source_ref` | Guard (over the pre-state) | Effect on state |
|---|---|---|---|---|
| `CampaignCreated` | A | — | seed only; `sequence = 0` | seeds state; `phase = BUILDING` |
| `WorkOrderIssued` | A | `WorkOrder` (5) | `phase = BUILDING`; `active_execution` absent; `scope_ref` digest matches `state.scope_digest`; `budget_policy_digest` matches state | `phase = BUILDING`; new `active_round_id`; `active_execution = {coder, work_order.target_provider_execution_id}` |
| `CoderReportReceived` | E | `CoderReport` (3) | an active coder execution exists | none |
| `ProviderExecutionRecorded` | A | — | `active_execution` present, not already `unresolved`, and its `provider_execution_id` equals the payload's, which equals the receipt's | `execution_outcome = dispatch_ambiguous` → `active_execution.unresolved := true`, **not cleared**; any other outcome → `active_execution` cleared |
| `CandidateAccepted` | A | `CandidateReceipt` (4) | `phase = BUILDING`; `claim_check.claimed_head_matches`; receipt congruence passed (FD-11) | `current_candidate_head` := head; `required_gate_ids` := the receipt's `applicable_gate_ids`; `phase = GATING`; `last_gate_results`/`last_ci_results` cleared |
| `GateResultsAccepted` | A | — | `phase = GATING`; `bound_head == current_candidate_head`; set equality and registry checks of §3.15.2 | `last_gate_results := {bound_head, all_required_passed}`; `phase = CI_WAIT` **iff** `all_required_passed`, otherwise **phase unchanged** (`GATING`) |
| `CiResultsAccepted` | A | — | `phase = CI_WAIT`; `bound_head == current_candidate_head`; every `required_ci_check_id` present, no duplicates | `last_ci_results := {bound_head, conclusion}`; `phase = REVIEWING` **iff** `conclusion = passed`, otherwise **phase unchanged** (`CI_WAIT`) |
| `ReviewRequested` | A | `ReviewRequest` (5) | `phase = REVIEWING`; `active_execution` absent; `candidate_head == current_candidate_head` | `active_execution = {reviewer, review_request.target_provider_execution_id}` |
| `ReviewerReportReceived` | E | `ReviewerReport` (3) | an active reviewer execution exists | none |
| `ReviewVerdictAccepted` | A | `ReviewVerdict` (4) | `phase = REVIEWING`; `reviewed_head == current_candidate_head`; **and** `last_gate_results.bound_head == current_candidate_head` **and** `last_gate_results.all_required_passed` **and** `last_ci_results.bound_head == current_candidate_head` **and** `last_ci_results.conclusion = passed` | `accepted` → `READY_TO_MERGE`; `changes_requested` → `CORRECTING`; `blocked` → **phase unchanged** (`REVIEWING`) |
| `CorrectiveDirectiveIssued` | A | `CorrectiveDirective` (5) | `phase = CORRECTING`; `active_execution` absent; `scope_ref` digest unchanged; every `target_finding_id` in the referenced verdict | `phase = BUILDING`; new `active_round_id`; `active_execution = {coder, directive.target_provider_execution_id}` |
| `HumanAttentionRaised` | A | `HumanAttentionRequest` (5) | `phase ∈ {BUILDING, GATING, CI_WAIT, REVIEWING, CORRECTING, HUMAN_REQUIRED}` — never `READY_TO_MERGE`, `CANCEL_REQUESTED`, or terminal; `attention_id` **unknown** to the state; no `question_id` already open in another active entry; congruence of §3.15.3 | `attention += AttentionEntry{OPEN, decision kind, offered actions, questions}`; `suspended_from_phase := phase` **only if** `phase ≠ HUMAN_REQUIRED`; `phase = HUMAN_REQUIRED` |
| `HumanDecisionRecorded` | A | `HumanDecision` (4) | binding checks passed (FD-15.1); `authentication_strength ≠ unattested`; **and, for `ACK` / `SELECT_ATTENTION_ACTION` / `ANSWER_QUESTION` only**, the named `AttentionEntry` is `OPEN` or `ACKNOWLEDGED` and satisfies that command's own guard (FD-14.6). `CANCEL` names no attention and requires none | per the FD-14.6 table, under the exit rule of FD-14.7 |
| `AttentionResolved` | A | — | that attention is `OPEN` or `ACKNOWLEDGED` | attention → `RESOLVED`, its `open_question_ids` cleared; the phase effect is governed by FD-14.7 |
| `AttentionSuperseded` | A | — | the superseded one is `OPEN` or `ACKNOWLEDGED`; the successor is known and `OPEN`; the two ids differ | attention → `SUPERSEDED`, its `open_question_ids` cleared; phase effect per FD-14.7 |
| `HumanCommandRejected` | E | `HumanCommandRequest` (3) | — | none |
| `TransitionRejected` | E | — | — | none |
| `CampaignFeedItemEmitted` | E | `CampaignFeedItem` (5) | — | none |
| `CampaignCancelled` | A | — | `phase = CANCEL_REQUESTED`; the §3.10 sequence observed | `phase = CANCELLED`; terminal canonicalization (FD-14.2) |
| `CampaignSuperseded` | A | — | `phase` non-terminal; the successor campaign exists | `phase = SUPERSEDED`; `superseded_by := payload.superseded_by`; terminal canonicalization |
| `CampaignTerminalError` | A | — | `phase` non-terminal | `phase = TERMINAL_ERROR`; terminal canonicalization |

Three rows changed shape in R3 and the reason is the same in all three:
`GateResultsAccepted(failed)`, `CiResultsAccepted(failed)`, and
`ReviewVerdictAccepted(blocked)` previously moved the campaign to
`HUMAN_REQUIRED` directly, which set that phase without setting
`suspended_from_phase` and without opening an attention — violating the state
invariant of §3.14 and contradicting FD-12, which names
`HumanAttentionRequest` as the authority for that transition. Now each records
its result and leaves the phase alone; the controller then raises an attention,
and *that* event performs the transition. FD-12's table is now literally true.

`all_required_passed` and the aggregate CI `conclusion` are **derived by the
reducer** from the payload's per-item results (§3.15.2), never carried as
producer-authored summary fields. A summary a producer can author is a summary a
producer can get wrong.

Gate outcome values reuse the vocabulary already frozen for run events —
`pass`, `fail`, `warn`, `blocked`, `not_applicable`, `error`
(`o7_run::event::GateOutcome`, `crates/o7-run/src/event.rs:174–188`) — rather
than inventing a second, subtly different gate vocabulary one crate away from the
first. `error` was missing from A1's copy through R4, which would have forced a
gate that could not produce a trustworthy result to be reported as something it
was not; the existing enum carries it precisely because "the gate crashed" is not
"the gate failed" (its own doc comment: "A harness error, distinct from a domain
`Fail`"), and A1 must not re-collapse a distinction the lower layer went to the
trouble of making.

`CampaignCreated` is consumed by `seed`, never by `fold` (FD-14.2). Every other
kind is a `fold` input.

#### 3.15.2 Event payload schemas

Eleven kinds carry a payload. Each is stored as its own immutable byte string under
`event_payload_digest`, parses under FD-1.3 and FD-1.6 like every other typed
object, and declares its `ArtifactRef` slots explicitly so FD-1.5 can account for
them.

`CampaignCreatedPayloadV1`

| Field | Type | Req | Constraints |
|---|---|---|---|
| `root_goal_id` / `task_id` | `Id` | yes | the durable binding (FD-5.1) |
| `contract_digest` | `Digest256` | yes | the campaign binding |
| `scope_ref` | `ArtifactRef` | yes | `ScopeContractV1`, rank 0 |
| `base_sha` | `CommitId` | yes | the A0 campaign base |
| `required_ci_check_ids` | `[Id]` | yes (may be empty) | ≤ 256; the campaign's required CI checks, fixed at creation |
| `gate_registry_digest` | `Digest256` | yes | the registry snapshot this campaign is bound to |
| `budget.max_provider_turns` | u32 | yes | ≥ 1 |
| `budget.max_wall_time_seconds` | u32 | yes | ≥ 1 |
| `budget.evidence_budget_bytes` | u64 | yes | ≤ `max_reachable_closure_bytes` |
| `budget.closure_object_budget` | u32 | yes | ≤ `max_reachable_closure_objects` |
| `budget_policy_digest` | `Digest256` | yes | framed over the four values above (FD-1.5); recomputed and checked at seed |

The four values are carried, not just their digest. A digest alone cannot tell a
replaying reader that the campaign's budget was 64 MiB and 512 objects — and
resolution needs the effective bound *before* the first `WorkOrder` exists, since
`CampaignCreated`'s own closure must already be resolved under campaign policy.
That is why `resolve_genesis` is a separate function from `resolve_event`
(FD-14.2) rather than the same one taking a policy it cannot yet have: the
genesis payload is read under the protocol hard maximum for a control artifact
(1 MiB, FD-1.4), which is enough to obtain the policy, and every resolution
after that uses `min(hard maximum, campaign policy)`. `seed` copies the policy
into state. A `WorkOrder` repeats the four values **and** the digest; a
`CorrectiveDirective` inherits the immutable state policy and repeats only the
digest — duplicating the numbers into every directive would be a fourth copy of
one fact.

`ProviderExecutionRecordedPayloadV1`

| Field | Type | Req | Constraints |
|---|---|---|---|
| `provider_execution_id` | `Id` | yes | must equal `active_execution.provider_execution_id` |
| `receipt_ref` | `ArtifactRef` | yes | `ProviderExecutionReceiptV1`, rank 2 |
| `execution_outcome` | enum as §3.12 | yes | closed; must equal the receipt's |

`GateResultsAcceptedPayloadV1`

| Field | Type | Req | Constraints |
|---|---|---|---|
| `bound_head` | `CommitId` | yes | FD-13 |
| `gate_registry_digest` | `Digest256` | yes | must equal `CampaignStateV1.gate_registry_digest` |
| `results[].gate_id` | `Id` | yes | registry id; **no duplicates** |
| `results[].outcome` | enum{`pass`,`fail`,`warn`,`blocked`,`not_applicable`,`error`} | yes | closed |
| `results[].log_ref` | `ArtifactRef` | no | rank 0 |

There is deliberately **no `required` flag here.** R3 removed the
producer-authored aggregate; leaving a producer-authored requiredness mask would
have moved the same lie one field to the left, where a controller could mark an
inconvenient gate `required = false` and let an arithmetically honest reducer
compute a green result. Requiredness comes from state:

```text
{ results[].gate_id }  ==  CampaignStateV1.required_gate_ids   (exact set equality)
no duplicate gate_id
payload.gate_registry_digest == state.gate_registry_digest
then, and only then:
all_required_passed := every required gate's outcome ∈ {pass, warn}
    (fail, blocked, not_applicable, and error are each NOT green)
```

`required_gate_ids` is set by `CandidateAccepted` from the receipt's
controller-derived `applicable_gate_ids` (§3.3), so the required set is fixed by
the observed diff before any gate runs. A missing gate is a set-equality failure,
not a silently smaller denominator.

`CiResultsAcceptedPayloadV1`

| Field | Type | Req | Constraints |
|---|---|---|---|
| `bound_head` | `CommitId` | yes | FD-13 |
| `checks[].check_id` | `Id` | yes | **no duplicates** |
| `checks[].conclusion` | enum{`passed`,`failed`,`timed_out`,`unavailable`} | yes | closed |
| `checks[].observation_ref` | `ArtifactRef` | no | rank 0 |

Requiredness is again state, not payload:

```text
CampaignStateV1.required_ci_check_ids  ⊆  { checks[].check_id }
no duplicate check_id
aggregate conclusion := passed
    iff every id in required_ci_check_ids has conclusion == passed
```

Extra non-required checks may be reported and are ignored by the aggregate; a
*missing* required check is a guard failure, not a pass by omission. `unavailable`
is not `passed` — an unobtainable answer is not a green one, the same rule
`ERROR ≠ FAIL ≠ PASS` already carries in `AGENTS.md`.

`AttentionResolvedPayloadV1`

| Field | Type | Req | Constraints |
|---|---|---|---|
| `attention_id` | `Id` | yes | must be `OPEN` or `ACKNOWLEDGED` |
| `resolution_code` | enum{`condition_cleared`,`obsolete_head`,`resolved_by_decision`,`external_change_absorbed`} | yes | closed |
| `summary` | `Text` | yes | ≤ 4096 bytes |

`AttentionSupersededPayloadV1`

| Field | Type | Req | Constraints |
|---|---|---|---|
| `attention_id` | `Id` | yes | the one being superseded |
| `superseded_by` | `Id` | yes | an `OPEN` attention; must differ |

`HumanCommandRejectedPayloadV1`

| Field | Type | Req | Constraints |
|---|---|---|---|
| `rejection_code` | enum{`stale_state_version`,`stale_head`,`stale_contract`,`unknown_attention`,`action_not_offered`,`superseded_question`,`unattested_actor`,`id_conflict`} | yes | closed |
| `detail` | `Text` | yes | ≤ 4096 bytes |

`TransitionRejectedPayloadV1`

| Field | Type | Req | Constraints |
|---|---|---|---|
| `attempted_event_kind` | enum as §3.15.1 | yes | closed |
| `guard` | `Text` | yes | ≤ 512 bytes; names the guard clause that failed |
| `reason_code` | enum{`phase_mismatch`,`head_mismatch`,`missing_evidence`,`scope_change`,`unknown_reference`,`invariant_violation`} | yes | closed |
| `detail_ref` | `ArtifactRef` | no | rank 0 |

`CampaignCancelledPayloadV1`

| Field | Type | Req | Constraints |
|---|---|---|---|
| `termination_observation_refs` | `[ArtifactRef]` | yes | rank 0; ≤ 256 |
| `worktree_disposition` | enum{`cleaned`,`preserved_for_forensics`} | yes | closed |

`CampaignSupersededPayloadV1`

| Field | Type | Req | Constraints |
|---|---|---|---|
| `superseded_by` | `Id` | yes | the successor campaign, which must already exist |
| `reason_code` | enum{`contract_revision`,`terminal_failure_replacement`,`operator_replacement`} | yes | closed |

`CampaignTerminalErrorPayloadV1`

| Field | Type | Req | Constraints |
|---|---|---|---|
| `reason_code` | enum{`integrity_failure`,`id_conflict_unresolved`,`receipt_incongruent_unresolved`,`budget_exhausted`,`invariant_violation`} | yes | closed |
| `summary` | `Text` | yes | ≤ 4096 bytes |
| `evidence_refs` | `[ArtifactRef]` | yes (may be empty) | rank ≤ 4; ≤ 256 |


#### 3.15.3 Reducer-owned field congruence

Several fields on accepted artifacts are reducer-owned facts about the very
transition that carries them (§3.9, §3.11). Nothing until R5 required them to
agree with it, and one of them was a live UX trap: `HumanAttentionRaised` is
authority-bearing, so if `raised_at_state_version` recorded the *pre*-state, every
screen built from that attention would be stale the instant it appeared —
cryptographically rigorous and completely unusable.

Frozen equalities, checked in `fold` against the resolved event:

```text
HumanAttentionRequest.raised_at_state_version
    == event.state_version_after

HumanAttentionRequest.suspended_from_phase
    == pre_state.phase                    when pre_state.phase != HUMAN_REQUIRED
    == pre_state.suspended_from_phase     otherwise

HumanAttentionRequest.candidate_head
    == pre_state.current_candidate_head   when the field is present

HumanDecision.applied_at_state_version
    == event.state_version_after

HumanCommandRequest.expected_campaign_state_version
    == event.state_version_before

CandidateReceipt.candidate_head
    == the head this event sets as current_candidate_head

ReviewVerdict.reviewed_head
    == pre_state.current_candidate_head
```

The `expected_campaign_state_version` line already follows from FD-15.1, but a
replay contract that leaves it implicit is a replay contract that two
implementations can disagree about, so it is a predicate here as well.

## 4. Reviewer independence (mechanically enforced)

Not prompt-requested. Enforced by construction, and each item is recorded on the
execution receipt or refused before dispatch:

```text
fresh provider session — no continuation from the coder session
no coder transcript in the reviewer's context
detached exact-head worktree, freshly attested
no repository mutation credentials
no GitHub mutation credentials
separate prompt identity and tool-policy identity (digests in the envelope)
distinct provider_execution_id (FD-10)
```

Anything less is the same agent handed a pair of glasses and asked to be
objective.

v1 permits the same provider family for both roles. Cross-family review stays in
the existing consensus backlog.

## 5. A1-V0 — the first vertical, and what it must prove

The contract is frozen; A1-V0 implementation begins once this document is
merged to `main`.

### 5.1 Role assignment

```text
coder      claude CLI, mutation confined to its own worktree
controller 007 — terminates writer authority, seals, derives CandidateReceipt,
           folds the campaign log (FD-14)
reviewer   arliai — read-only, schema-bound, no mutation credentials at all
human      Q-Deck feed + attention requests, v1 command set only
```

Arli AI is the natural first reviewer backend for a mechanical reason:
`docs/o7-invoke.md` freezes it as a direct HTTPS call with **no subprocess and no
tool surface at all**, `tool_choice: "none"`, a compile-time constant endpoint,
and a `tool_calls`-present response rejected as `FAIL_INVALID_OUTPUT`. Reviewer
independence then holds across two different execution surfaces, not merely two
prompts. Its artifact split maps directly onto the receipt: `stdout.raw` →
`raw_provider_event_ref`, `result.json` → `normalized_output_ref` and, for the
final dispatch, `final_normalized_output_ref` — whose digest must equal the
`ReviewerReport`'s `payload_digest` (FD-11).

`--engine codex` is not a V0 coder: `docs/o7-invoke.md` records that none of its
confinement flags has been exercised against a real `codex` binary, so its honest
posture today is "ambient context refused, writes denied" — not "no shell", not
"no network". It becomes eligible once that gap is live-verified, which is not A1
work.

### 5.2 Scope of the V0 implementation

```text
in scope:  the 11 message kinds; ProviderExecutionReceiptV1, InteractionManifestV1,
           ScopeContractV1; CampaignEventV1 with its digest chain and its eleven
           payload schemas; verify_wire -> resolve_event -> fold over
           CampaignStateV1, incl. the HUMAN_REQUIRED exit rule (FD-14.7) and the
           reducer-owned field congruence (§3.15.3); the FD-11 receipt
           congruence and the §3.12 terminal-output binding; closure resolution
           over immediate_refs for BOTH artifacts and events under FD-1.5
           bounds; one live corrective cycle
out:       progress frontier, NO_PROGRESS, reconciliation, webhooks,
           full incarnation taxonomy, cumulative per-campaign evidence
           accounting, budget accounting beyond a stop condition
```

### 5.3 The live corrective cycle V0 must demonstrate

One real campaign, end to end, with no human relaying anything between agents:

```text
coder produces a candidate containing a real defect
-> controller seals, derives CandidateReceipt, dispatches review
-> reviewer finds the defect, returns a schema-bound ReviewerReport
-> controller accepts it as ReviewVerdict{changes_requested}
-> controller issues one CorrectiveDirective (same scope_ref digest)
-> coder fixes at a new head
-> reviewer accepts the exact new head
-> controller emits a ready_to_merge CampaignFeedItem
-> merge stays manual
```

Plus a full campaign replay after restart that reaches the same `CampaignStateV1`
— identical `state_version`, `phase`, and `current_candidate_head` — with **zero
provider invocations** (FD-8). The provider invocation count is the proof,
exactly as it was for A0.

### 5.4 Required negative tests

Happy path is not acceptance. Every row is a required test with a frozen expected
outcome.

**Encoding, bounds, identity**

| Input | Required outcome |
|---|---|
| malformed `CoderReport` / `ReviewerReport` | parse rejection, no acceptance, no dispatch |
| unsupported `envelope_version`, `message_kind_version`, or `campaign_protocol_version` | refused, never best-effort parsed (FD-1.6) |
| unknown field present | rejected at parse time (FD-1.6) |
| explicit JSON `null` in an optional field | rejected (FD-1.3) |
| duplicate member name in any object, at any depth — the required test must include a **nested** object and not only the top-level one | rejected at parse time; never first-wins or last-wins (FD-1.3, S2). A top-level-only case does not discharge this row: some deserializers refuse top-level duplicates on their own, so passing it can mean the document layer was never exercised at all |
| duplicate member name where one occurrence is `null` | rejected; an implementation that reduces the document through a JSON library before applying the null policy will not see the null and must not be admitted on that basis (FD-1.3, S2) |
| payload exceeding a per-object bound | rejected, never truncated (FD-1.4) |
| `artifact_refs` whose declared sizes exceed `max_direct_referenced_bytes` | rejected before any read (FD-1.5) |
| a closure exceeding `max_reachable_closure_bytes`/`_objects` | whole resolution rejected, never partially accepted (FD-1.5) |
| payload-declared refs (e.g. `coder_report_ref`) pushing the total past a bound, with `envelope.artifact_refs` small | rejected — `immediate_refs` is the union, not the envelope list (FD-1.5) |
| a typed referenced object whose own payload refs a huge subtree | counted and bounded at the depth it appears; rejected if over (FD-1.5) |
| a closure whose object count is inflated by repeated refs | deduplicated by `(kind, digest)`; not a rejection (FD-1.5) |
| stored object whose real size ≠ declared `size` | integrity failure, rejected (FD-1.5) |
| campaign policy budget above the protocol hard maximum | refused at campaign creation (FD-1.5) |
| duplicate `message_id`, same envelope digest | idempotent replay; `state_version` unchanged (FD-6) |
| duplicate `message_id`, different envelope digest | `IdConflict`, fail closed, attention raised (FD-6) |
| same payload bytes, different `expected_input_head` | `IdConflict`, not a replay (FD-6) |
| redelivery with a different `created_at` | replay; stored envelope and `first_observed_at` unchanged (FD-5.4) |

**Provenance**

| Input | Required outcome |
|---|---|
| provider-produced artifact with no receipt ref | rejected (§3.0) |
| controller-derived artifact carrying a receipt ref | rejected (FD-11) |
| valid receipt from a *different* execution attached to a valid report | `ReceiptIncongruent`, rejected (FD-11) |
| receipt whose `prompt_digest` / `tool_policy_digest` / `adapter_version` / `role` / `campaign_id` / `round_id` differs from the envelope | `ReceiptIncongruent`, rejected (FD-11) |
| `final_normalized_output_ref.digest` ≠ envelope `payload_digest` | `ReceiptIncongruent`, rejected (FD-11) |
| controller edits the normalized bytes before enveloping | breaks the digest equality above; rejected (FD-1.1) |
| receipt with `execution_outcome = dispatch_ambiguous` | no artifact from it accepted; attention raised (FD-9) |
| receipt with one ambiguous dispatch among completed ones | `execution_outcome` derives to `dispatch_ambiguous` (§3.12) |
| dispatch 0 `reached`+`completed`, dispatch 1 `not_reached` | `execution_outcome = incomplete`, **never** `failed_pre_dispatch`; no whole-execution retry (§3.12) |
| every dispatch `not_reached` | `failed_pre_dispatch`; safe redrive with a fresh grain (FD-9, §3.12) |
| `execution_outcome = completed` whose last dispatch has no `normalized_output_ref` | malformed receipt, rejected before FD-11 (§3.12) |
| `final_normalized_output_ref` ≠ the last dispatch's `normalized_output_ref` | malformed receipt, rejected — the report would otherwise bind to a non-terminal blob (§3.12) |
| manifest referencing a `dispatch_id` absent from the receipt | rejected (§3.12.1) |
| model alias recorded as a resolved backend identity | rejected; `resolution.status` must be honest (FD-3) |
| retry attempted without established non-dispatch | refused; `dispatch_ambiguous` raised (FD-9) |
| retry that names no grain, or a continuation carrying `retry_of_dispatch_id` | refused (FD-10.3) |

**Graph and references**

| Input | Required outcome |
|---|---|
| a ref that violates the rank rule | rejected (FD-2.1) |
| `artifact_ref` outside owned CAS | inert, unresolvable, rejected (FD-1.8) |
| ref whose declared `kind` disagrees with the slot's expected kind | rejected; the slot wins (FD-2.5) |
| the same bytes referenced through two typed slots | two distinct closure nodes, both accounted (FD-2.5) |
| rank-0 bytes that happen to parse as a typed object | never parsed, never promoted (FD-2.5) |
| agent-supplied `ArtifactRef` naming a new imported authority root | refused; imports come from the registry/binding (FD-2.4) |
| imported A0 ref failing its own owner's validation | referencing artifact rejected (FD-2.4) |

**Authority and transitions**

| Input | Required outcome |
|---|---|
| `claimed_head` ≠ controller-derived candidate head | fail closed, no review dispatch (§3.3) |
| stale `reviewed_head` | no `ReviewVerdict`; report retained as evidence (FD-5.2) |
| wrong `contract_digest` | rejected (FD-5.3) |
| lineage fields inconsistent with the canonical campaign binding | rejected (FD-5.1) |
| unknown `finding_id` referenced by a directive | rejected (§3.7) |
| directive with a different `scope_ref` digest | refused, `SCOPE_EXPANSION_REFUSED` (§3.7) |
| reviewer proposes a shell command as required evidence | mapped to a registry id or `HUMAN_REQUIRED`; never executed (FD-7) |
| reviewer execution holding mutation credentials | refused before dispatch (§4) |
| gate result whose bound head is not the current candidate head | diagnostic only, no transition (FD-13) |
| transition attempted with an unsatisfied guard | `TransitionRejected`; neither counter advances (FD-14.4) |
| evidence-only event (feed item, report received, rejected command) | `last_accepted_sequence` advances; `state_version` unchanged **and no other field changes** (FD-14.3) |
| an evidence-only event that would mutate any other state field | contract violation; the kind belongs in the authority class (FD-14.3) |
| replay of the same log twice | identical `CampaignStateV1`, zero provider calls (FD-8, FD-14.2) |
| log whose `sequence` has a gap, or whose `event_digest` chain breaks | refused; never replayed "as far as it goes" (FD-14.2) |
| log not beginning with `CampaignCreated` at `sequence = 0` | refused (FD-14.2) |
| a second `CampaignCreated` later in the log | refused (FD-14.2) |
| event whose `state_version_after` does not match its kind's class | refused before folding (§3.15) |
| `previous_event_digest` at `sequence = 0` that is not `Digest256::genesis()` | refused (§3.15) |
| a `source_ref` whose `kind`, `media_type`, or `size` changed while its digest did not | different `event_digest`; chain break detected (§3.15) |
| `CoderReportReceived` carrying a rank-3 `source_ref` | accepted — `source_ref` is provenance for evidence kinds, and its expected rank is per-kind (§3.15) |
| event payload bytes re-serialized differently | different `event_payload_digest`, different `event_digest` (§3.15) |
| event whose payload or `evidence_refs` blow the closure bounds | rejected before folding (FD-1.5) |
| `GateResultsAccepted` with a `fail` on a required gate | result stored, `phase` stays `GATING`; only `HumanAttentionRaised` may reach `HUMAN_REQUIRED` (§3.15.1) |
| `CiResultsAccepted` whose required check is `unavailable` | aggregate `conclusion ≠ passed`; `phase` stays `CI_WAIT` (§3.15.2) |
| `ReviewVerdictAccepted` while `last_ci_results.conclusion ≠ passed` | guard fails; `TransitionRejected` — a correct head does not make a red CI green (§3.15.1) |
| `ReviewVerdict.verdict = blocked` | verdict stored, `phase` stays `REVIEWING`; escalation goes through an attention (§3.15.1) |
| any event other than `HumanAttentionRaised` setting `phase = HUMAN_REQUIRED` | contract violation (FD-14.7) |
| `HumanAttentionRaised` while `phase = READY_TO_MERGE` | guard fails; the ready-to-merge notice is a feed item, not an attention (§3.9) |
| `HumanAttentionRaised` reusing a `RESOLVED` or `SUPERSEDED` `attention_id` | guard fails — ids are unique for the campaign's lifetime (§3.9) |
| `AttentionSuperseded` whose superseded attention is `RESOLVED` | guard fails (§3.15.1) |
| terminal event leaving `active_round_id` or `active_execution` set | contract violation; terminal canonicalization is atomic (FD-14.2) |
| `ANSWER_QUESTION` for a `question_id` not in the named entry's `open_question_ids` | rejected — a question nobody escalated was never asked (FD-14.6) |
| `ANSWER_QUESTION` naming an entry whose `required_decision_kind ≠ answer_question` | rejected (FD-14.6) |
| a `CoderReport` question that no attention escalated | never enters any entry; unanswerable by construction (FD-14.6) |
| `GateResultsAccepted` omitting a required gate id | set-equality guard fails; no smaller denominator (§3.15.2) |
| `GateResultsAccepted` carrying a producer-authored `required` flag | rejected as an unknown field; requiredness comes from state (§3.15.2) |
| `GateResultsAccepted` with a duplicate `gate_id` | guard fails (§3.15.2) |
| `GateResultsAccepted` whose `gate_registry_digest` ≠ the campaign binding | guard fails (§3.15.2) |
| `CiResultsAccepted` missing a required check id | guard fails; absence is not a pass (§3.15.2) |
| a fold implementation reading CAS directly | contract violation — `fold` consumes `ResolvedCampaignEventV1` only (FD-14.2) |
| closure checking attempted in `verify_wire` | contract violation — payload refs are unknowable without CAS (FD-14.2) |
| `WorkOrderIssued` while `active_execution` is present | guard fails; one execution at a time (§3.15.1) |
| receipt `provider_execution_id` ≠ `active_execution.provider_execution_id` | guard fails; the id has a canonical origin (§3.15.1) |
| a `CorrectiveDirective` expecting a follow-up `WorkOrder` to start the round | contract violation — the directive starts the execution itself (§3.7) |
| gate `error` on a required gate | not green; `phase` stays `GATING`; escalation via attention (§3.15.2) |
| aggregate treating `error` as `fail` or as `pass` | contract violation — the distinction exists in `GateOutcome` for a reason (§3.15.2) |
| `raised_at_state_version` recording the pre-state | rejected; must equal `state_version_after`, or every screen is stale on arrival (§3.15.3) |
| `suspended_from_phase` on the artifact disagreeing with the transition | rejected (§3.15.3) |
| `applied_at_state_version` ≠ the decision's `state_version_after` | rejected (§3.15.3) |
| `SELECT_ATTENTION_ACTION` whose id is not in that entry's `offered_action_ids` | rejected from state alone, without re-reading the artifact (§3.14) |
| `ANSWER_QUESTION` without `attention_id` | rejected — question ownership must be decidable by the fold (§3.10) |
| the same `question_id` open in two active attentions | guard fails at raise time (§3.15.1) |
| `AttentionResolved` leaving its questions open | contract violation; the entry's questions are cleared with it (§3.15.1) |
| a `WorkOrder` whose `budget_policy_digest` ≠ the campaign policy | guard fails (§3.15.1) |
| a receipt whose `request.budget_policy_digest` ≠ the campaign policy | `ReceiptIncongruent` (FD-11) |
| a receipt whose `provider_execution_id` ≠ the campaign's `active_execution` | `ReceiptIncongruent` (FD-11) |
| `CANCEL` carrying an `attention_id` | rejected by the schema (§3.10) |
| `CANCEL` refused because no attention is open | contract violation — the attention guard does not apply to `CANCEL` (§3.15.1) |
| an `ArtifactRef.kind` outside `ArtifactKindV1` | rejected; the set is closed (FD-1.9) |
| an imported A0 ref spelled differently from `o7_run::event::ArtifactKind` | rejected — imported spellings are reused verbatim (FD-1.9) |
| `required_decision_kind = answer_question` with empty `question_ids` | rejected (§3.9) |
| `required_decision_kind = choose_resolution` with no options | rejected (§3.9) |
| `resolve_event` called on `CampaignCreated` with a policy argument | contract violation — genesis uses `resolve_genesis` (FD-14.2) |
| `budget_policy_digest` not recomputable from the four carried values | seed fails (FD-1.5, §3.15.2) |
| replay that needs a value only present in a later `WorkOrder` | impossible — genesis carries the policy (§3.15.2) |
| resolution failure (unparseable payload, slot-mismatched source) | `ResolutionFailed`; the event is never folded (FD-14.2) |
| two implementations framing an enum by different tag bytes | impossible — enums are framed by frozen ASCII name (FD-1.2) |
| `AttentionResolved` arriving while `phase = CANCEL_REQUESTED` | attention state updates; phase unchanged — a late resolution never un-cancels (FD-14.7) |
| resuming action while another attention is `OPEN` | that attention resolves; phase stays `HUMAN_REQUIRED`; target not applied (FD-14.7) |
| exit from `HUMAN_REQUIRED` leaving `suspended_from_phase` set | contract violation; cleared in the same step (FD-14.7, §3.14) |
| `ProviderExecutionRecorded` for an execution that is not `active_execution` | guard fails; `TransitionRejected` (§3.15.1) |
| `ProviderExecutionRecorded` with `execution_outcome = dispatch_ambiguous` | `active_execution` **retained** and marked `unresolved`; not cleared (FD-14.7a) |
| new `WorkOrder`/`ReviewRequest`/`CorrectiveDirective` after an ambiguous execution | guard fails — the dispatch slot is still held, durably across restart (FD-14.7a) |
| crash between `ProviderExecutionRecorded(ambiguous)` and the attention event | replay reaches a state that still blocks dispatch (FD-14.7a) |
| any V1 event clearing an `unresolved` execution short of a terminal transition | contract violation — only `CANCEL` or supersede end it (FD-14.7a) |
| a `HumanCommandRequest` whose `producer_execution_id` names a provider execution | rejected — the human case is the controller's ingress identity (FD-10) |
| an `ArtifactRef` to a message kind whose `size` covers only the envelope | rejected — `size` is envelope + payload together (FD-1.8) |
| an `ArtifactRef` whose `digest` is the payload digest rather than the envelope digest | rejected (FD-1.8) |
| a closure of small envelopes over large payloads sized by envelopes alone | rejected; the true cost is charged before reading (FD-1.5, FD-1.8) |
| an event declaring itself authority-bearing against its kind | impossible — class is a function of `event_kind`, not a field (FD-14.3) |
| `GateResultsAccepted` whose `bound_head` ≠ `current_candidate_head` | guard fails; `TransitionRejected`, neither counter advances (§3.15, FD-13) |
| `ReviewVerdictAccepted` while `last_gate_results.bound_head` is a stale head | guard fails; no `READY_TO_MERGE` (§3.15) |
| new candidate accepted after gates passed | `last_gate_results`/`last_ci_results` cleared; `READY_TO_MERGE` unreachable until re-run (§3.14) |

**Human lane**

| Input | Required outcome |
|---|---|
| human command with stale `expected_campaign_state_version` | rejected (FD-15.1) |
| human command with stale `expected_head` or contract digest | rejected (FD-15.1) |
| request asserting its own attestation or transport | rejected — no such field exists (FD-15.2) |
| `authenticated` recorded with no `authenticator_id` | rejected (FD-15.2) |
| loopback caller | recorded as `loopback_unauthenticated` + `claimed_identity`, never as an attested operator (FD-15.2) |
| `unattested` actor | refused (FD-15.2) |
| answer targeting a superseded `question_id` | not delivered to the coder (§3.10) |
| answer declaring `revise_contract` | effect `contract_revision_requested`; phase stays `HUMAN_REQUIRED`; no autonomous dispatch; `SUPERSEDED` only later, via `CampaignSuperseded` naming an existing successor (§3.10, FD-14.6) |
| `CampaignSuperseded` naming a successor that does not exist | guard fails; `TransitionRejected` (§3.15.1) |
| attention action outside the server-provided set | rejected (§3.9) |
| ACK on an open attention | one `HumanDecisionRecorded` (`state_version` +1); attention state `ACKNOWLEDGED`, never `RESOLVED`; `phase` unchanged (FD-14.5, FD-14.6) |
| an attention artifact carrying a `lifecycle` field | rejected as an unknown field — lifecycle is derived state (FD-1.6, FD-14.5) |
| `AttentionResolved` while another attention is still `OPEN` | phase stays `HUMAN_REQUIRED`; no resume (§3.15) |
| second attention raised while already `HUMAN_REQUIRED` | `suspended_from_phase` preserved, not overwritten (§3.15) |
| `retry_failed_step` with no `suspended_from_phase` | guard fails; `TransitionRejected` (FD-14.6) |
| resume with no stored `suspended_from_phase` | impossible — the field is required iff `phase = HUMAN_REQUIRED` (§3.14) |
| controller publishes an `action_id` outside the closed V1 set | refused at attention creation (FD-14.6) |
| `ReviewerReport` carrying `reviewer.identity` / `.model` / `.prompt_version` | rejected as unknown fields; provenance is derived from the receipt (§3.5, §3.6) |
| the same ACK redelivered | idempotent replay; `state_version` unchanged (FD-6) |

### 5.5 The v1-lite cut

Safe to cut — the four autonomy properties survive:

- one campaign in flight (mirrors R1's single in-flight command);
- human commands: the §3.10 v1 set only;
- no push notifications: feed + SSE, tier named honestly as **v1-lite** (timely
  only while the client is open; "operational v1" means background-capable
  delivery and is not claimed until it exists);
- merge manual, triggered by the `ready_to_merge` feed item (§3.8) — not an
  attention request, which the reducer would reject in that phase (§3.9);
- reviewer = same provider family, different execution surface per §4;
- reconciliation = polling only, no webhooks.

Not cuttable — object identity, not ceremony:

- exact-head binding everywhere, including human decisions;
- controller-derived candidate descriptor behind the A0 sealing boundary;
- the raw-report/accepted-artifact split;
- receipt congruence (FD-11);
- no model-supplied executable authority;
- provider execution evidence and no-recall-on-replay;
- fail-closed directive validation;
- attention and decision records as canonical events;
- forward-only corrective rounds.

## 6. Out of scope

- No architect or planner role in the core loop.
- No multi-agent negotiation, no model-to-model channel, no PR comments as agent
  input.
- No automatic merge, under any artifact defined here.
- No A5 goal-graph runtime. `root_goal_id` is identity and lineage only.
- No A2 reducer *extensions*: progress frontier, `NO_PROGRESS`, terminal taxonomy
  beyond FD-14's phases, external reconciliation, full incarnation taxonomy
  (issue #94 §3, §5). The V0 fold of FD-14 is in scope precisely because A1-V0
  cannot execute without it.
- No `ArtifactAcceptance` artifact (FD-2.3) and no `ContractRevisionProposal`
  artifact (§3.10); both require a new message kind under §7.
- No capability transport beyond registry-bound action references
  (`docs/architecture/capability-fd-transport.md` stays its own slice and gains a
  concrete consumer once A1-V0 has real actions to authorize).
- No redefinition of the accepted A0 contract.
- No constrained decoder, no Kani/TLA+ modelling commitment. Determinism levels
  D3/D4 are not prerequisites; A1 correctness depends only on D0 (deterministic
  admission) and D2 (deterministic historical replay from recorded evidence).

## 7. Supersede path

1. A superseding revision of this document, naming the exact decisions it
   replaces by FD number.
2. A new `message_kind_version` and media type for every artifact whose payload
   shape changes; a new `envelope_version` for envelope or rank changes; a new
   `campaign_protocol_version` for reducer-semantics changes (FD-1.6).
3. Recorded migration semantics: replay-for-verification keeps the recorded
   version's semantics; replay-for-resume is where version gates bite (#94 §5).
4. No in-place mutation of an accepted artifact, ever. Correction is forward-only
   at this level too.

An in-flight campaign is never migrated mid-flight: it is superseded, with the
supersedes relation recorded (FD-5.1).

## 8. Order after this freeze

```text
A1-F   this contract, accepted at b61540a          <- done
A1-V0  the 11 kinds + receipt + FD-14 fold + one real corrective cycle (§5)
                                                   <- next, after this merges
SB-B   capability transport, with A1 actions as its concrete consumer
A2     reducer extensions: progress frontier, NO_PROGRESS, terminal taxonomy,
       reconciliation, full incarnation taxonomy
```

`research/b1-context/` continues in parallel and remains read-only and
non-authoritative: it must not drive an A-series transition.

## 9. Revision history

### Acceptance

```text
accepted exact head   b61540a   (after R5.1, following a final exact-head pass)
amended pre-merge     R5.2       (four P1s from external review on PR #123)
frozen baseline       the merged head of PR #123
superseded            S1 — FD-1.4 only, the first §7 application after merge
                      S2 — FD-1.3 only, member-name uniqueness
status                ACCEPTED / CLOSED / FROZEN
rounds                R1, R2, R3, R4, R5, R5.1, R5.2 — every finding corrected
                      forward; no round amended an earlier one in place
next                  A1-V0 (§5), and not before this document merges
```

### S2 — FD-1.3 said nothing about duplicate member names

The second supersede under §7, and the first one raised by an *implementation
review* rather than by an implementer stuck between two readings.

```text
replaces      FD-1.3 only
reason        the null policy and the unknown-field policy together left one
              ambiguity unaddressed: a JSON object carrying the same member
              name twice. RFC 8259 leaves the behaviour undefined, so
              first-wins and last-wins are both conforming
decision      member names are unique within every A1 JSON object; a duplicate
              is rejected at parse time; no first-wins or last-wins reading is
              permitted
no change to  envelope_version, message_kind_version, campaign_protocol_version
              — no payload shape, field set, framing, rank or reducer semantics
              moved. This changes an admission rule, exactly as S1 did
effect        this document's blob changes, so contract_digest changes; there is
              no in-flight campaign to migrate (§7)
```

**How it surfaced.** A1-V0 step 2 replaced the document layer's materialising
walk with a streaming scan, under an explicit promise to change *when* a
structural bound fires and never *which* documents are admitted. External review
found a class of documents where that promise broke.

Grounded exactly, so the claim survives the implementation changing:

```text
old materialising path
    a6625bc6473e3029a3309ddd7f2795ce57516a60 (merged, PR #124)
    crates/o7-a1-contracts/src/json.rs::validate_document
    builds a serde_json::Value, then walks it. The object map keeps the LAST
    duplicate, so a shadowed member is discarded before any rule runs.

reviewed streaming path
    777e2fbe38f98668d15d8378f23f4298af2b963b (PR #132)
    crates/o7-a1-contracts/src/scan.rs
    crates/o7-a1-contracts/src/json.rs
    streams the bytes and never reduces them, so the null is seen.

observed property
    for a schema with one known member `known`:
    {"known": null, "known": 1}   old artifact admission = admit
                                  streaming artifact admission = reject
    {"known": 2, "known": 1}      old artifact admission = admit
                                  streaming artifact admission = reject
```

`777e2fb` is the revision the finding was made on: the exact head both external
reviewers read on PR #132. The divergence is documented and witnessed by test in
the follow-up commit `f1b9ce9847dab390541c2b97faded43add6d7d58` on the same
branch, but provenance belongs to the revision where it was found, not to the
one where it was written down.

The two spellings are refused by different layers — the scan refuses the
shadowed null because the null is genuinely present, the typed schema refuses
the plain duplicate as a repeated field. Both refuse the artifact, which is the
level this row is stated at.

The second case is the one this supersede exists for: no rule in `B1` refuses
it, and an implementation that did would have been enforcing a rule the contract
does not contain.

**Why that is a contract gap rather than an implementation choice.** The
shadowed-null case does follow from FD-1.3 as already written — the null is
present, and the rule says explicit null is rejected. The plain case,
`{"known": 2, "known": 1}`, does not follow from anything: FD-1.6 governs *unknown*
fields and versions, not ambiguity in general, and no other decision addresses
it. An implementation refusing it would have been enforcing a rule the contract
does not contain, which is the same defect as inventing a bound — the mistake
FD-1.8's `size` lower bound was corrected for during A1-V0 step 1.

So the argument from FD-1.2 is strong enough to *justify a supersede* and not
strong enough to *be* the contract. Ratifying "this already follows from B1"
would have quietly widened the frozen admission set by reasoning, which is the
one thing §7 exists to prevent. The implementation is right; it simply needed
authority, and this is that authority.

**Scope discipline.** The rule is stated for A1 JSON objects generally rather
than per-schema, because the failure is a property of the encoding and not of
any one payload. It adds no field, moves no bound, and names no new type.

What that does **not** mean is that a conforming implementation needs no change.
Every consumer bound to `B1` must rebind to `B2`, because an implementation is
bound to exact contract bytes and not to a version number (§7, and the Status
section above). An implementation whose parser already refuses duplicates needs
no change *to its duplicate-detection logic* — and still needs the rebind, or it
goes on validating campaigns against superseded authority while looking
conformant. One that deduped silently has a defect it can now see, and the same
rebind to do.

### S1 — FD-1.4 classified `InteractionManifestV1` under two bounds at once

The first supersede under §7, and the first correction made *after* the contract
was incorporated rather than before. Numbered S-, not R5.3: the R-rounds were
review of an unmerged document, and collapsing the two would erase the
distinction between "corrected before it became authority" and "corrected while
it was".

```text
replaces      FD-1.4 only
reason        the original text simultaneously classified InteractionManifestV1
              under the 1 MiB typed-object bound ("any typed A1 payload") and
              under the 64 MiB bound whose examples name "manifest"
decision      InteractionManifestV1 uses the 64 MiB hard maximum; it remains a
              typed A1 object for every other purpose, notably FD-1.7 media
              types and its own FD-2 rank. The grain is per EXECUTION, not per
              dispatch (§3.12, §3.12.1): one manifest indexes up to 256
              dispatches and up to 4096 interaction_sequence entries in total
no change to  envelope_version, message_kind_version, campaign_protocol_version
              — no payload shape, envelope, rank or reducer semantics moved
effect        this document's blob changes, so contract_digest changes; there is
              no in-flight campaign to migrate (§7)
```

**How it surfaced.** An implementation had to pick a bound and could not, because
both readings were literally present. It chose 64 MiB, recorded the choice as a
reading rather than a fact, and said so — which is the behaviour the contract
wants from an implementer who finds an ambiguity: resolve it visibly, do not
resolve it silently. Two independent reviewers then confirmed the text was
self-contradictory rather than merely unclear. FD-1.4 now says what was meant.

**What made the original text wrong rather than terse.** One predicate was
answering two questions. "Typed A1 payload" decides the media type (FD-1.7), the
rank (FD-2), and the size bound — and for `InteractionManifestV1` the first two
answers are *yes* while the third is *no*. A classification used for more than
one purpose is correct only until the purposes disagree, and this is where they
did.

The rounds below are preserved in the order they happened. Each one is a record
of what the contract got wrong before it was frozen, which is the part worth
keeping: a freeze is only as trustworthy as the list of things it stopped being.

### R5.2 — four P1s from external review, closed before merge (`3a92cea`)

The status flip was not the last word: opening PR #123 ran the external review
the freeze was waiting for, and it found four genuine defects. Each was checked
against the text before being accepted — an automated reviewer is a claim, not a
verdict (FD-4 applies to reviewers of this document too) — and all four held.

1. **An ambiguous execution freed the dispatch slot.**
   `ProviderExecutionRecorded` cleared `active_execution` unconditionally, so a
   crash between it and the escalating attention left a state where
   `WorkOrderIssued` passed its guard and redrove an execution FD-9 forbids
   redriving. The prohibition now lives in the state: an ambiguous outcome keeps
   `active_execution` and marks it `unresolved`, no V1 event clears it, and the
   only exits are `CANCEL` or supersede — the same choice R1 §11.6 made for the
   same condition. This was the serious one: without it, A1-V0 would have
   implemented a duplicate-side-effect path the document spends a section
   forbidding.
2. **`producer_execution_id` had no defined value for human artifacts.** It is
   mandatory on every envelope, while FD-10 covered only provider-produced and
   controller-derived cases — so V0 could not envelope an ACK without inventing
   an identity grain. Frozen as three cases, the human one being the
   controller's **ingress** execution identity: who accepted the bytes, never
   who authored them.
3. **`ArtifactRef` did not say which of an artifact's two byte strings it
   identified.** FD-1.1 stores envelope and payload separately, so a ref
   charging one `size` undercounted every resolution: 65 small envelopes over
   1 MiB payloads passed a 64 MiB budget while the resolver read past it. Frozen:
   for an envelope-bearing artifact the ref's digest is the envelope digest and
   its size is both halves together, with integrity provable in each half.
4. **§5.5 still called the ready-to-merge handoff an attention request** — a
   leftover from R4, contradicting §3.9 and §5.3, and one the reducer would have
   rejected outright.

That a freeze ceremony immediately preceded four real findings is worth leaving
in the record rather than tidying away. The gate worked; the header was early.

### R5.1 — consistency patch on the synced head (`6c92870`)

Not an exploratory round: five contradictions between sections that earlier
rounds edited at different times, plus two schema tightenings.

1. **FD-14.6 still spoke of a top-level `open_question_ids`** that §3.14 had
   already deleted in favour of per-attention entries — one section requiring
   `state.attention[A].open_question_ids` while another described
   `state.open_question_ids`. The normative guard now names the entry, and the
   acceptance rows follow.
2. **`CANCEL` could not pass its own event guard.** The schema forbids
   `attention_id` on `CANCEL` while `HumanDecisionRecorded` required a referenced
   attention unconditionally, so the one command FD-14.6 calls unconditional
   would have been rejected every time. The attention guard is now scoped to
   `ACK` / `SELECT_ATTENTION_ACTION` / `ANSWER_QUESTION`.
3. **`ArtifactKindV1` was a partial list.** R5 fixed enum framing but left the
   `kind` set incomplete — `coder_report`, `candidate_receipt`, `review_verdict`
   and the rest are all in frozen slots, so implementations would have had to
   invent spellings for a digest input, reintroducing exactly the divergence R5
   removed. FD-1.9 now freezes the complete closed set, reusing
   `o7_run::event::ArtifactKind`'s own serialized spellings verbatim for imported
   kinds.
4. **The genesis signature contradicted the genesis prose.** `resolve_event` took
   a campaign policy that, for `CampaignCreated`, lives inside the payload the
   function has yet to read. Split into `resolve_genesis` (reads under the
   protocol hard maximum, yields the policy) and `resolve_event` (takes it), with
   `replay` rewritten accordingly.
5. **The new canonical budget was not bound to provider executions.** FD-11
   verified ten predicates and none compared `receipt.request.budget_policy_digest`
   to the campaign's, so an execution could run under a foreign budget policy and
   still pass. Two predicates added — budget policy, and
   `receipt.provider_execution_id == active_execution.provider_execution_id`,
   whose origin is the initiating artifact's `target_provider_execution_id`.
   Twelve now, and the wording says so in both places.

Tightenings: an attention's decision surface must be non-empty and single-kind
(`answer_question` ⇒ questions, `choose_resolution` ⇒ options, `ack` ⇒ neither),
which turns "an attention nobody can act on is a feed item" from a sentence into
a schema; and `CorrectiveDirective` restates only the budget *digest* rather than
re-carrying four numbers that are already immutable campaign state.


### R5 — fifth corrective round (review of `8009535`), plus the main sync

R4's six confirmed closed. Six local P1s remained, all fixable inside existing
schemas; this round is implementation-readiness rather than architecture.

1. **`verify` was asked to bound a closure it could not see.** It promised
   FD-1.5 checking over `immediate_refs`, which includes refs declared by the
   event *payload* — a separate CAS object that only the next stage fetches, and
   `verify` has no CAS. Renamed to `verify_wire` (sequence, chain, digest,
   version arithmetic — everything decidable from bytes already in hand), with
   payload fetch, slot-checking, `immediate_refs`, and closure resolution moved
   into the five numbered steps of `resolve_event`. `replay` now spells the
   pipeline out.
2. **`active_execution.provider_execution_id` came from an ellipsis.** State
   held `{role, provider_execution_id}` while no artifact carried the id, and
   the envelope's `producer_execution_id` is the *controller's* identity on a
   controller-issued artifact (FD-10), so it could not be borrowed. `WorkOrder`,
   `ReviewRequest`, and `CorrectiveDirective` now carry
   `target_provider_execution_id`, minted before dispatch; the corresponding
   event requires `active_execution` absent and sets it exactly. The corrective
   path was also stuck between two protocols — the topology said directive →
   coder while the reducer only moved to `BUILDING` — and R5 picks one:
   `CorrectiveDirectiveIssued` starts the execution itself.
3. **Attention state lost its decision surface.** A flat `open_question_ids`
   could not tell the fold which attention owned a question, could not reproduce
   the `selected_action_id ∈ options` check, and stranded a resolved attention's
   questions as permanently open. Replaced by `AttentionEntry{attention_id,
   state, required_decision_kind, offered_action_ids, open_question_ids}`;
   `ANSWER_QUESTION` now carries `attention_id`; a `question_id` may be open in
   at most one active attention.
4. **Reducer-owned fields did not have to agree with their own transition.**
   §3.15.3 freezes seven equalities. The load-bearing one:
   `raised_at_state_version == state_version_after` — since
   `HumanAttentionRaised` is authority-bearing, recording the pre-state would
   have made every screen stale at birth, which is rigorous and useless.
5. **`GateOutcome::Error` was unrepresentable.** A1's copy of the vocabulary
   stopped at `not_applicable` while the enum it claims to reuse carries `error`
   for "the gate could not produce a trustworthy result" — and the citation range
   was wrong too (`174–188`, not `172–184`). Added, and explicitly not green:
   collapsing a crashed gate into `fail` or `pass` would undo a distinction the
   lower layer made deliberately, and the same rule now stated in `AGENTS.md`'s
   "missing evidence is not a passed check".
6. **Campaign budget was not replay-self-contained.** `CampaignCreated` carried
   only `budget_policy_digest`, so a replaying reader could verify that the
   policy was unchanged without ever learning what it was — while
   `resolve_event` needs the effective bound at genesis, before any `WorkOrder`
   exists. The four values are now carried in the genesis payload and held in
   `CampaignStateV1.budget_policy`, the digest framing over them is frozen, and
   genesis itself is read under the protocol hard maximum.

Editorial: the payload-schema count survived R4 as "the ten eleven event payload
schemas" and is now one number; the framing pseudocode says `… name` rather than
`tag byte`, matching the rule below it; and `ArtifactRef.kind`'s canonical wire
spelling is stated, since that name is a digest input too.

**Main sync.** The branch merged `origin/main` at `8025a80` before this round (55
commits of drift). None of the contracts A1-F binds to moved — `a0-candidate-state`,
`r1-command`, `autonomy-controller`, `decision-and-admission-protocol`,
`o7-invoke`, `crates/o7-run`, `crates/o7d` are all untouched by that range — so
the R1–R4 grounding claims were re-checked and stand. `AGENTS.md` gained a
"diagnosing is not repairing" section whose third line ("missing evidence is not
a passed check") is the same principle FD-13 and §3.15.2's set-equality rule
enforce for gates, and `docs/invariant-registry.md` (ratified design, not started)
is where §5.4's matrix would eventually register as executable witnesses.


### R4 — fourth corrective round (review of `3c30974`)

R3's seven confirmed closed. Six remaining holes plus one invariant gap, all
inside existing schemas and the reducer algebra — no new persistent message kind
was needed to close any of them.

1. **`fold` could not reach the data it folds on.** The wire event carries a
   `source_ref` and an `event_payload_digest`; the transitions need the verdict's
   `verdict`, the receipt's `candidate_head`, the payload's `results[]`. A pure,
   I/O-free function cannot fetch those, so the signature was quietly asking for
   a magic trick. FD-14.2 now names three stages —
   `verify` → `resolve_event` → `fold` — with `ResolvedCampaignEventV1` as the
   reducer's in-memory input: the wire header plus the verified payload, the
   verified source artifact, and the content-addressed registry views the guards
   need. Not persisted, not a message kind. `fold` stays genuinely pure and
   replay honestly means resolve-then-fold.
2. **The required V0 happy path violated a state invariant.** `HumanAttentionRaised`
   admitted any non-terminal phase, so the ready-to-merge notice stored
   `suspended_from_phase = READY_TO_MERGE`, which §3.14 forbids — and §5.3
   *requires* that exact sequence, so this was the main road, not an edge case.
   `READY_TO_MERGE` is now a `CampaignFeedItem`; attentions may be raised only
   from the five suspendable phases; and `required_decision_kind` lost its `none`
   variant, since an attention nobody can act on was always a feed item wearing a
   decision's coat.
3. **The question lane was unreachable.** `open_question_ids` existed, and
   nothing ever put an id in it — `CoderReportReceived` is correctly inert, so
   the `provide_answer` guard could never be satisfied and the attention had no
   field for questions. Now `HumanAttentionRequestV1` carries controller-selected
   `question_ids` under `required_decision_kind = answer_question`,
   `HumanAttentionRaised` moves them into `open_question_ids`, and
   `ANSWER_QUESTION` closes them. The generic `provide_answer` action is deleted:
   `ANSWER_QUESTION` already exists and, unlike an action id, can carry the
   answer's bytes.
4. **Attention identity was ambiguous.** The guard allowed re-raising a
   `SUPERSEDED` id while the prose said a new problem mints a new identity,
   leaving "replace the record" and "append a second" both defensible. Frozen:
   ids are unique for the campaign's lifetime, `HumanAttentionRaised` requires an
   unknown id, and `AttentionSuperseded` requires the superseded one to be `OPEN`
   or `ACKNOWLEDGED`.
5. **Digest tag bytes were never assigned.** Four framings said "tag byte" with
   no numbering, so two conforming implementations could pick different bytes and
   compute different digests. Enums are now framed by their frozen `snake_case`
   ASCII name. `o7-run`'s numeric tags are right for bytes that never leave one
   crate; a cross-implementation wire contract is better served by removing the
   coordination problem than by adding four numbering tables.
6. **Requiredness could still manufacture a false green.** R3 removed the
   producer-authored aggregate, but `results[].required` let a defective
   controller mark an inconvenient gate as optional and have an honest reducer
   agree. Both `required` flags are gone. The gate set must equal
   `CampaignStateV1.required_gate_ids` — taken from the receipt's
   controller-derived `applicable_gate_ids`, fixed by the observed diff before any
   gate runs — with no duplicates and a matching registry digest; the CI required
   set comes from the campaign binding. A missing gate is a set-equality failure,
   not a smaller denominator.
7. **Terminal transitions left the state non-canonical.** `CampaignCancelled`,
   `CampaignSuperseded`, and `CampaignTerminalError` did not clear
   `active_round_id`, `active_execution`, or `suspended_from_phase`, so the event
   that ends a campaign could violate §3.14 on its way out. Terminal
   canonicalization is now part of every terminal transition.

Editorial: "the ten event payload schemas (eleven of them)" is one number again,
the status header records R1–R4 rather than R1 alone, and FD-1.1's stray
reference to "the congruence check of FD-13" points at FD-11, where congruence
actually lives.


### R3 — third corrective round (review of `5e2f33c`)

R2's blockers confirmed closed. The seven remaining findings all sat inside the
new `CampaignEventV1` and the transition algebra — bugs in the arithmetic rather
than in the prose, which is the better place to find them and a much better time.

1. **The event had no unambiguous wire contract.** `event_payload_digest` was
   framed but never declared; per-kind payloads were prose lists; `source_ref`
   was committed by digest alone, so its `kind`/`media_type`/`size` could change
   without changing `event_digest`; and the genesis link was "a genesis value".
   Fixed without inventing canonical JSON: the payload is a separate stored byte
   string with `event_payload_digest = SHA-256(exact stored bytes)` (FD-1.1
   again), §3.15.2 gives all eleven payload kinds full field tables, the ref is
   framed in full, and genesis is exactly `Digest256::genesis()` — the all-zero
   digest already frozen for run events.
2. **`authority_ref` was untypeable for evidence kinds.** It was declared rank
   4–5 while `CoderReportReceived`, `ReviewerReportReceived`, and
   `HumanCommandRejected` reference rank-3 objects, so a conforming
   `CoderReportReceived` could not be encoded. Renamed to `source_ref`, with the
   expected kind and rank fixed per `event_kind`: transition authority for
   authority-bearing kinds, provenance for evidence kinds. The word "authority"
   no longer has to pretend a `CoderReport` authorizes something.
3. **Event references escaped closure accounting.** `immediate_refs` was defined
   only for envelope-bearing artifacts, while events reach artifacts through
   `source_ref`, `evidence_refs`, and payload slots. `immediate_refs` is now
   defined over both node kinds, and every event is resolved under the same
   bounds before it is folded.
4. **An evidence-only event mutated canonical state.**
   `ProviderExecutionRecorded` cleared `active_execution` without advancing
   `state_version`, so two materially different states could share one version —
   and a human's stale-command binding would have stopped meaning what it says.
   Frozen: evidence-only events touch nothing but `last_accepted_sequence`, and
   `ProviderExecutionRecorded` moves to the authority-bearing class.
5. **Three transitions entered `HUMAN_REQUIRED` through the back door.**
   `GateResultsAccepted(failed)`, `CiResultsAccepted(failed)`, and
   `ReviewVerdictAccepted(blocked)` set that phase without setting
   `suspended_from_phase` and without opening an attention — breaking the §3.14
   invariant and contradicting FD-12. Each now records its result and leaves the
   phase alone; `HumanAttentionRaised` is the sole entry to `HUMAN_REQUIRED`, and
   FD-12's table is literally true.
6. **`revise_contract` terminated the campaign twice, the first time
   impossibly.** It moved to `SUPERSEDED`, which requires `superseded_by`, at a
   moment when the successor campaign did not exist — and then blocked the very
   `CampaignSuperseded` event that could have named it. Now the decision records
   `contract_revision_requested` and stays `HUMAN_REQUIRED`; `CampaignSuperseded`
   ends the campaign later, when the successor is a fact.
7. **Leaving `HUMAN_REQUIRED` was not safe against late attention events.**
   FD-14.7 freezes five rules: only `HumanAttentionRaised` may enter that phase;
   `suspended_from_phase` may be restored only from it; exit requires no `OPEN`
   or `ACKNOWLEDGED` attention remaining; the field is cleared atomically on
   exit; and `CANCEL_REQUESTED` and every terminal phase are immune to attention
   events — a stale resolution can never un-cancel a campaign.

Cleanups: `ReviewVerdictAccepted` now requires `all_required_passed` and CI
`conclusion = passed`, not merely results bound to the right head — a correct
head does not turn a red CI green; aggregate gate/CI verdicts are derived by the
reducer from per-item results rather than carried as producer-authored summaries;
an `unavailable` required check is explicitly not `passed`; and the acceptance
matrix rows that still described ACK as evidence-only and `revise_contract` as
immediately superseding were corrected to match the single normative semantics.


### R2 — second corrective round (review of `d3dbca7`)

R1's six blockers were confirmed closed; six new ones were found, all deeper —
the document had started behaving like a protocol, which is where protocols
begin to bite.

1. **Closure bounds were escapable through payload refs.** Traversal seeded only
   from `envelope.artifact_refs`, while nearly every typed payload carries its
   own `ArtifactRef` slots that nothing required to be mirrored there. FD-1.5 now
   defines `immediate_refs` as the union of envelope refs, every
   `ArtifactRef`-valued slot the payload's schema declares, and the receipt ref;
   traversal parses the (1 MiB-bounded) root payload first and enqueues typed
   objects' declared slots at every depth. Refs are not duplicated into the
   envelope — two lists of one truth is the worse repair.
2. **`execution_outcome` could claim `failed_pre_dispatch` after a real side
   effect.** A completed dispatch 0 followed by an unsent continuation derived to
   `failed_pre_dispatch`, which is exactly the label that would authorize a
   whole-execution retry. The derivation now has four branches:
   `failed_pre_dispatch` requires that *every* boundary is `not_reached`, and a
   last dispatch that never left the building yields `incomplete`. Added the
   terminal-output binding: `completed` requires the last dispatch to be
   `reached`/`completed` with a `normalized_output_ref`, and
   `final_normalized_output_ref` must be exactly that ref — so FD-11 now proves
   the report is the execution's *final* answer, not merely some blob it
   mentioned.
3. **The reducer had semantics but no wire contract.** `AcceptedEvent` was a list
   of names, six of which corresponded to no message kind at all, so two
   conforming implementations could build different logs. §3.15 freezes
   `CampaignEventV1`: sequence, digest chain, stored
   `state_version_before`/`_after`, `authority_ref`, per-kind payloads, and a
   per-kind table of guards and state effects — with event class a function of
   `event_kind`, never a field a producer could set. Genesis is honest now:
   `seed(CampaignCreated) -> CampaignStateV1` is separate from
   `fold(state, event)`, and log well-formedness is frozen.
4. **Attention lifecycle mutated an immutable artifact.** `lifecycle` is removed
   from `HumanAttentionRequestV1`; being raised *is* `OPEN`, and
   `OPEN → ACKNOWLEDGED → RESOLVED/SUPERSEDED` is derived by the reducer into
   `CampaignStateV1.attention` via explicit events (FD-14.5).
5. **`HUMAN_REQUIRED → resumed` was not a transition.** "Resumed" is not a phase,
   and nothing stored where to return. Added
   `CampaignStateV1.suspended_from_phase` (present iff `phase =
   HUMAN_REQUIRED`), a closed V1 action set, and a frozen decision → target-phase
   table (FD-14.6), so the fold is total without hidden implementation policy. A
   second attention no longer overwrites the way back with `HUMAN_REQUIRED`
   itself.
6. **`max_evidence_bytes_per_campaign` was decorative.** Nothing tracked a
   cumulative total, so the number was a claim no code could keep. Removed from
   A1-F; per-resolution closure bounds stay, and cumulative storage accounting
   moves to A2 where the state that would carry it already lives.

Non-blocking corrections in the same round: `ScopeContractV1` is reclassified as
a rank-0 **local typed leaf** rather than an opaque imported root — rank 0 now
means "terminal in the reference graph", and parseability is decided per slot;
reviewer provenance (`identity`, `model`, `prompt_version`) is removed from
`ReviewerReportV1` and derived controller-side in `ReviewVerdictV1` from the
receipt plus a prompt-registry lookup, because a model telling the controller
which model it was is not evidence; and FD-11's predicate count is stated
uniformly everywhere rather than drifting between "equalities" and "checks", so
R3 does not open with an audit of English nouns. (R2 counted ten; R5.1 added two
campaign bindings and the count is twelve today — §9's R5.1 entry.)

Also added in R2: `CampaignStateV1` carries `last_gate_results` and
`last_ci_results` (head-bound), which `CandidateAccepted` clears — the guard for
`READY_TO_MERGE` is stated over them, so a new head cannot leave a stale green in
the guard's line of sight; a terminal `SUPERSEDED` phase for contract revision;
and per-attention question tracking for the answer guard.


### R1 — first corrective round (review of `144ebf6`)

Six freeze-blocking findings, all accepted and corrected forward:

1. **Receipt collapsed the grain split it declared.** A single receipt carrying
   one `dispatch_id` beside a whole-execution manifest could not say which grain
   it represented, and a multi-dispatch tool loop had no correct single receipt
   to reference. Replaced by an execution-level `ProviderExecutionReceiptV1` with
   nested dispatch records, a frozen `execution_outcome` derivation, and
   per-dispatch boundary classification (FD-10.1, FD-10.2, §3.12).
2. **Presence of a receipt proved nothing.** Added FD-11: nine congruence
   equalities between envelope and receipt, including `payload_digest ==
   final_normalized_output_ref.digest`, which is only checkable because FD-1.1
   now freezes that a provider-produced payload *is* the normalized output bytes.
   A valid receipt from an unrelated execution is now `ReceiptIncongruent`.
3. **"Same shape minus some fields" is not a wire schema.** §3 now carries
   complete field tables for all eleven message kinds plus the receipt, manifest,
   `ScopeContractV1`, and `CampaignStateV1` — type, required, constraints, and
   authority per field, with one global null policy (FD-1.3).
   `ContractRevisionProposal` was removed from the semantics rather than left
   dangling: `revise_contract` supersedes the campaign (§3.10).
4. **A1-V0 required the reducer A2 was going to write later.** FD-14 freezes
   `CampaignStateV1`, a pure fold, the authority-bearing/evidence-only event
   split, and the `state_version` rule (+1 per authority-bearing event only,
   with `last_accepted_sequence` tracking the log separately). A2 keeps the
   extensions; V0 gets the minimum it cannot run without, and
   `expected_campaign_state_version` finally has an origin.
5. **`loopback_local_operator` overclaimed.** Loopback proves transport, not
   humanity, identity, or an uncompromised local process. Split into a claim on
   the request (`claimed_actor_identity`) and controller observations on the
   decision (`authentication_strength`, `observed_transport`, `authenticator_id`),
   with no field in the request able to assert any of them (FD-15.2).
6. **Bounds were per-object only.** 256 refs × 64 MiB is 16 GiB before the DAG
   even branches. Added FD-1.5: protocol hard maxima, a campaign policy budget
   that `budget_policy_digest` actually commits to, and a frozen closure
   traversal that deduplicates by `(kind, digest)`, accounts declared sizes
   before reading, and rejects all-or-nothing.

Point corrections in the same round: A0 and other foreign references are now
**imported authority roots** at virtual rank 0 with resolver duties and an
anti-promotion rule (FD-2.4, FD-2.5); FD-3's rank ladder was corrected (normalized
output is rank 0, the report rank 3, the verdict rank 4); scope equality moved
from an unmeetable "byte-identical" comparison to one immutable
`ScopeContractV1` compared by digest (§3.13); `created_at` gained a
`first_observed_at` companion and an explicit redelivery rule (FD-5.4); the
`ArtifactAcceptance` question is answered honestly — absent in V1, and adding it
requires superseding FD-2 rather than an additive rank; and the document status
was corrected to `PROPOSED FREEZE / REVIEW REQUIRED / NON-AUTHORITATIVE` until a
review actually accepts it.
