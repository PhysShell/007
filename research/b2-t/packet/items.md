# Classification items

41 items. Order is not meaningful. Each item is verbatim source text.

## item_001

```text
**Closure bounds were escapable through payload refs.** Traversal seeded only
   from `envelope.artifact_refs`, while nearly every typed payload carries its
   own `ArtifactRef` slots that nothing required to be mirrored there. FD-1.5 now
   defines `immediate_refs` as the union of envelope refs, every
   `ArtifactRef`-valued slot the payload's schema declares, and the receipt ref;
   traversal parses the (1 MiB-bounded) root payload first and enqueues typed
   objects' declared slots at every depth. Refs are not duplicated into the
   envelope — two lists of one truth is the worse repair.
```

## item_002

```text
**Attention state lost its decision surface.** A flat `open_question_ids`
   could not tell the fold which attention owned a question, could not reproduce
   the `selected_action_id ∈ options` check, and stranded a resolved attention's
   questions as permanently open. Replaced by `AttentionEntry{attention_id,
   state, required_decision_kind, offered_action_ids, open_question_ids}`;
   `ANSWER_QUESTION` now carries `attention_id`; a `question_id` may be open in
   at most one active attention.
```

## item_003

```text
**Leaving `HUMAN_REQUIRED` was not safe against late attention events.**
   FD-14.7 freezes five rules: only `HumanAttentionRaised` may enter that phase;
   `suspended_from_phase` may be restored only from it; exit requires no `OPEN`
   or `ACKNOWLEDGED` attention remaining; the field is cleared atomically on
   exit; and `CANCEL_REQUESTED` and every terminal phase are immune to attention
   events — a stale resolution can never un-cancel a campaign.
```

## item_004

```text
**`CANCEL` could not pass its own event guard.** The schema forbids
   `attention_id` on `CANCEL` while `HumanDecisionRecorded` required a referenced
   attention unconditionally, so the one command FD-14.6 calls unconditional
   would have been rejected every time. The attention guard is now scoped to
   `ACK` / `SELECT_ATTENTION_ACTION` / `ANSWER_QUESTION`.
```

## item_005

```text
**The question lane was unreachable.** `open_question_ids` existed, and
   nothing ever put an id in it — `CoderReportReceived` is correctly inert, so
   the `provide_answer` guard could never be satisfied and the attention had no
   field for questions. Now `HumanAttentionRequestV1` carries controller-selected
   `question_ids` under `required_decision_kind = answer_question`,
   `HumanAttentionRaised` moves them into `open_question_ids`, and
   `ANSWER_QUESTION` closes them. The generic `provide_answer` action is deleted:
   `ANSWER_QUESTION` already exists and, unlike an action id, can carry the
   answer's bytes.
```

## item_006

```text
**Bounds were per-object only.** 256 refs × 64 MiB is 16 GiB before the DAG
   even branches. Added FD-1.5: protocol hard maxima, a campaign policy budget
   that `budget_policy_digest` actually commits to, and a frozen closure
   traversal that deduplicates by `(kind, digest)`, accounts declared sizes
   before reading, and rejects all-or-nothing.
```

## item_007

```text
**`ArtifactRef` did not say which of an artifact's two byte strings it
   identified.** FD-1.1 stores envelope and payload separately, so a ref
   charging one `size` undercounted every resolution: 65 small envelopes over
   1 MiB payloads passed a 64 MiB budget while the resolver read past it. Frozen:
   for an envelope-bearing artifact the ref's digest is the envelope digest and
   its size is both halves together, with integrity provable in each half.
```

## item_008

```text
**Campaign budget was not replay-self-contained.** `CampaignCreated` carried
   only `budget_policy_digest`, so a replaying reader could verify that the
   policy was unchanged without ever learning what it was — while
   `resolve_event` needs the effective bound at genesis, before any `WorkOrder`
   exists. The four values are now carried in the genesis payload and held in
   `CampaignStateV1.budget_policy`, the digest framing over them is frozen, and
   genesis itself is read under the protocol hard maximum.
```

## item_009

```text
**A1-V0 required the reducer A2 was going to write later.** FD-14 freezes
   `CampaignStateV1`, a pure fold, the authority-bearing/evidence-only event
   split, and the `state_version` rule (+1 per authority-bearing event only,
   with `last_accepted_sequence` tracking the log separately). A2 keeps the
   extensions; V0 gets the minimum it cannot run without, and
   `expected_campaign_state_version` finally has an origin.
```

## item_010

```text
**`authority_ref` was untypeable for evidence kinds.** It was declared rank
   4–5 while `CoderReportReceived`, `ReviewerReportReceived`, and
   `HumanCommandRejected` reference rank-3 objects, so a conforming
   `CoderReportReceived` could not be encoded. Renamed to `source_ref`, with the
   expected kind and rank fixed per `event_kind`: transition authority for
   authority-bearing kinds, provenance for evidence kinds. The word "authority"
   no longer has to pretend a `CoderReport` authorizes something.
```

## item_011

```text
**`revise_contract` terminated the campaign twice, the first time
   impossibly.** It moved to `SUPERSEDED`, which requires `superseded_by`, at a
   moment when the successor campaign did not exist — and then blocked the very
   `CampaignSuperseded` event that could have named it. Now the decision records
   `contract_revision_requested` and stays `HUMAN_REQUIRED`; `CampaignSuperseded`
   ends the campaign later, when the successor is a fact.
```

## item_012

```text
**`GateOutcome::Error` was unrepresentable.** A1's copy of the vocabulary
   stopped at `not_applicable` while the enum it claims to reuse carries `error`
   for "the gate could not produce a trustworthy result" — and the citation range
   was wrong too (`174–188`, not `172–184`). Added, and explicitly not green:
   collapsing a crashed gate into `fail` or `pass` would undo a distinction the
   lower layer made deliberately, and the same rule now stated in `AGENTS.md`'s
   "missing evidence is not a passed check".
```

## item_013

```text
**FD-14.6 still spoke of a top-level `open_question_ids`** that §3.14 had
   already deleted in favour of per-attention entries — one section requiring
   `state.attention[A].open_question_ids` while another described
   `state.open_question_ids`. The normative guard now names the entry, and the
   acceptance rows follow.
```

## item_014

```text
**The genesis signature contradicted the genesis prose.** `resolve_event` took
   a campaign policy that, for `CampaignCreated`, lives inside the payload the
   function has yet to read. Split into `resolve_genesis` (reads under the
   protocol hard maximum, yields the policy) and `resolve_event` (takes it), with
   `replay` rewritten accordingly.
```

## item_015

```text
**Presence of a receipt proved nothing.** Added FD-11: nine congruence
   equalities between envelope and receipt, including `payload_digest ==
   final_normalized_output_ref.digest`, which is only checkable because FD-1.1
   now freezes that a provider-produced payload *is* the normalized output bytes.
   A valid receipt from an unrelated execution is now `ReceiptIncongruent`.
```

## item_016

```text
**`active_execution.provider_execution_id` came from an ellipsis.** State
   held `{role, provider_execution_id}` while no artifact carried the id, and
   the envelope's `producer_execution_id` is the *controller's* identity on a
   controller-issued artifact (FD-10), so it could not be borrowed. `WorkOrder`,
   `ReviewRequest`, and `CorrectiveDirective` now carry
   `target_provider_execution_id`, minted before dispatch; the corresponding
   event requires `active_execution` absent and sets it exactly. The corrective
   path was also stuck between two protocols — the topology said directive →
   coder while the reducer only moved to `BUILDING` — and R5 picks one:
   `CorrectiveDirectiveIssued` starts the execution itself.
```

## item_017

```text
**Attention lifecycle mutated an immutable artifact.** `lifecycle` is removed
   from `HumanAttentionRequestV1`; being raised *is* `OPEN`, and
   `OPEN → ACKNOWLEDGED → RESOLVED/SUPERSEDED` is derived by the reducer into
   `CampaignStateV1.attention` via explicit events (FD-14.5).
```

## item_018

```text
**Event references escaped closure accounting.** `immediate_refs` was defined
   only for envelope-bearing artifacts, while events reach artifacts through
   `source_ref`, `evidence_refs`, and payload slots. `immediate_refs` is now
   defined over both node kinds, and every event is resolved under the same
   bounds before it is folded.
```

## item_019

```text
**`HUMAN_REQUIRED → resumed` was not a transition.** "Resumed" is not a phase,
   and nothing stored where to return. Added
   `CampaignStateV1.suspended_from_phase` (present iff `phase =
   HUMAN_REQUIRED`), a closed V1 action set, and a frozen decision → target-phase
   table (FD-14.6), so the fold is total without hidden implementation policy. A
   second attention no longer overwrites the way back with `HUMAN_REQUIRED`
   itself.
```

## item_020

```text
**`fold` could not reach the data it folds on.** The wire event carries a
   `source_ref` and an `event_payload_digest`; the transitions need the verdict's
   `verdict`, the receipt's `candidate_head`, the payload's `results[]`. A pure,
   I/O-free function cannot fetch those, so the signature was quietly asking for
   a magic trick. FD-14.2 now names three stages —
   `verify` → `resolve_event` → `fold` — with `ResolvedCampaignEventV1` as the
   reducer's in-memory input: the wire header plus the verified payload, the
   verified source artifact, and the content-addressed registry views the guards
   need. Not persisted, not a message kind. `fold` stays genuinely pure and
   replay honestly means resolve-then-fold.
```

## item_021

```text
**Attention identity was ambiguous.** The guard allowed re-raising a
   `SUPERSEDED` id while the prose said a new problem mints a new identity,
   leaving "replace the record" and "append a second" both defensible. Frozen:
   ids are unique for the campaign's lifetime, `HumanAttentionRaised` requires an
   unknown id, and `AttentionSuperseded` requires the superseded one to be `OPEN`
   or `ACKNOWLEDGED`.
```

## item_022

```text
**Digest tag bytes were never assigned.** Four framings said "tag byte" with
   no numbering, so two conforming implementations could pick different bytes and
   compute different digests. Enums are now framed by their frozen `snake_case`
   ASCII name. `o7-run`'s numeric tags are right for bytes that never leave one
   crate; a cross-implementation wire contract is better served by removing the
   coordination problem than by adding four numbering tables.
```

## item_023

```text
**The required V0 happy path violated a state invariant.** `HumanAttentionRaised`
   admitted any non-terminal phase, so the ready-to-merge notice stored
   `suspended_from_phase = READY_TO_MERGE`, which §3.14 forbids — and §5.3
   *requires* that exact sequence, so this was the main road, not an edge case.
   `READY_TO_MERGE` is now a `CampaignFeedItem`; attentions may be raised only
   from the five suspendable phases; and `required_decision_kind` lost its `none`
   variant, since an attention nobody can act on was always a feed item wearing a
   decision's coat.
```

## item_024

```text
**Terminal transitions left the state non-canonical.** `CampaignCancelled`,
   `CampaignSuperseded`, and `CampaignTerminalError` did not clear
   `active_round_id`, `active_execution`, or `suspended_from_phase`, so the event
   that ends a campaign could violate §3.14 on its way out. Terminal
   canonicalization is now part of every terminal transition.
```

## item_025

```text
**`verify` was asked to bound a closure it could not see.** It promised
   FD-1.5 checking over `immediate_refs`, which includes refs declared by the
   event *payload* — a separate CAS object that only the next stage fetches, and
   `verify` has no CAS. Renamed to `verify_wire` (sequence, chain, digest,
   version arithmetic — everything decidable from bytes already in hand), with
   payload fetch, slot-checking, `immediate_refs`, and closure resolution moved
   into the five numbered steps of `resolve_event`. `replay` now spells the
   pipeline out.
```

## item_026

```text
**`producer_execution_id` had no defined value for human artifacts.** It is
   mandatory on every envelope, while FD-10 covered only provider-produced and
   controller-derived cases — so V0 could not envelope an ACK without inventing
   an identity grain. Frozen as three cases, the human one being the
   controller's **ingress** execution identity: who accepted the bytes, never
   who authored them.
```

## item_027

```text
**An evidence-only event mutated canonical state.**
   `ProviderExecutionRecorded` cleared `active_execution` without advancing
   `state_version`, so two materially different states could share one version —
   and a human's stale-command binding would have stopped meaning what it says.
   Frozen: evidence-only events touch nothing but `last_accepted_sequence`, and
   `ProviderExecutionRecorded` moves to the authority-bearing class.
```

## item_028

```text
**The new canonical budget was not bound to provider executions.** FD-11
   verified ten predicates and none compared `receipt.request.budget_policy_digest`
   to the campaign's, so an execution could run under a foreign budget policy and
   still pass. Two predicates added — budget policy, and
   `receipt.provider_execution_id == active_execution.provider_execution_id`,
   whose origin is the initiating artifact's `target_provider_execution_id`.
   Twelve now, and the wording says so in both places.
```

## item_029

```text
**The reducer had semantics but no wire contract.** `AcceptedEvent` was a list
   of names, six of which corresponded to no message kind at all, so two
   conforming implementations could build different logs. §3.15 freezes
   `CampaignEventV1`: sequence, digest chain, stored
   `state_version_before`/`_after`, `authority_ref`, per-kind payloads, and a
   per-kind table of guards and state effects — with event class a function of
   `event_kind`, never a field a producer could set. Genesis is honest now:
   `seed(CampaignCreated) -> CampaignStateV1` is separate from
   `fold(state, event)`, and log well-formedness is frozen.
```

## item_030

```text
**Reducer-owned fields did not have to agree with their own transition.**
   §3.15.3 freezes seven equalities. The load-bearing one:
   `raised_at_state_version == state_version_after` — since
   `HumanAttentionRaised` is authority-bearing, recording the pre-state would
   have made every screen stale at birth, which is rigorous and useless.
```

## item_031

```text
**Receipt collapsed the grain split it declared.** A single receipt carrying
   one `dispatch_id` beside a whole-execution manifest could not say which grain
   it represented, and a multi-dispatch tool loop had no correct single receipt
   to reference. Replaced by an execution-level `ProviderExecutionReceiptV1` with
   nested dispatch records, a frozen `execution_outcome` derivation, and
   per-dispatch boundary classification (FD-10.1, FD-10.2, §3.12).
```

## item_032

```text
**`max_evidence_bytes_per_campaign` was decorative.** Nothing tracked a
   cumulative total, so the number was a claim no code could keep. Removed from
   A1-F; per-resolution closure bounds stay, and cumulative storage accounting
   moves to A2 where the state that would carry it already lives.
```

## item_033

```text
**"Same shape minus some fields" is not a wire schema.** §3 now carries
   complete field tables for all eleven message kinds plus the receipt, manifest,
   `ScopeContractV1`, and `CampaignStateV1` — type, required, constraints, and
   authority per field, with one global null policy (FD-1.3).
   `ContractRevisionProposal` was removed from the semantics rather than left
   dangling: `revise_contract` supersedes the campaign (§3.10).
```

## item_034

```text
**`ArtifactKindV1` was a partial list.** R5 fixed enum framing but left the
   `kind` set incomplete — `coder_report`, `candidate_receipt`, `review_verdict`
   and the rest are all in frozen slots, so implementations would have had to
   invent spellings for a digest input, reintroducing exactly the divergence R5
   removed. FD-1.9 now freezes the complete closed set, reusing
   `o7_run::event::ArtifactKind`'s own serialized spellings verbatim for imported
   kinds.
```

## item_035

```text
**Three transitions entered `HUMAN_REQUIRED` through the back door.**
   `GateResultsAccepted(failed)`, `CiResultsAccepted(failed)`, and
   `ReviewVerdictAccepted(blocked)` set that phase without setting
   `suspended_from_phase` and without opening an attention — breaking the §3.14
   invariant and contradicting FD-12. Each now records its result and leaves the
   phase alone; `HumanAttentionRaised` is the sole entry to `HUMAN_REQUIRED`, and
   FD-12's table is literally true.
```

## item_036

```text
**`loopback_local_operator` overclaimed.** Loopback proves transport, not
   humanity, identity, or an uncompromised local process. Split into a claim on
   the request (`claimed_actor_identity`) and controller observations on the
   decision (`authentication_strength`, `observed_transport`, `authenticator_id`),
   with no field in the request able to assert any of them (FD-15.2).
```

## item_037

```text
**`execution_outcome` could claim `failed_pre_dispatch` after a real side
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
```

## item_038

```text
**An ambiguous execution freed the dispatch slot.**
   `ProviderExecutionRecorded` cleared `active_execution` unconditionally, so a
   crash between it and the escalating attention left a state where
   `WorkOrderIssued` passed its guard and redrove an execution FD-9 forbids
   redriving. The prohibition now lives in the state: an ambiguous outcome keeps
   `active_execution` and marks it `unresolved`, no V1 event clears it, and the
   only exits are `CANCEL` or supersede — the same choice R1 §11.6 made for the
   same condition. This was the serious one: without it, A1-V0 would have
   implemented a duplicate-side-effect path the document spends a section
   forbidding.
```

## item_039

```text
**The event had no unambiguous wire contract.** `event_payload_digest` was
   framed but never declared; per-kind payloads were prose lists; `source_ref`
   was committed by digest alone, so its `kind`/`media_type`/`size` could change
   without changing `event_digest`; and the genesis link was "a genesis value".
   Fixed without inventing canonical JSON: the payload is a separate stored byte
   string with `event_payload_digest = SHA-256(exact stored bytes)` (FD-1.1
   again), §3.15.2 gives all eleven payload kinds full field tables, the ref is
   framed in full, and genesis is exactly `Digest256::genesis()` — the all-zero
   digest already frozen for run events.
```

## item_040

```text
**§5.5 still called the ready-to-merge handoff an attention request** — a
   leftover from R4, contradicting §3.9 and §5.3, and one the reducer would have
   rejected outright.
```

## item_041

```text
**Requiredness could still manufacture a false green.** R3 removed the
   producer-authored aggregate, but `results[].required` let a defective
   controller mark an inconvenient gate as optional and have an honest reducer
   agree. Both `required` flags are gone. The gate set must equal
   `CampaignStateV1.required_gate_ids` — taken from the receipt's
   controller-derived `applicable_gate_ids`, fixed by the observed diff before any
   gate runs — with no duplicates and a matching registry digest; the CI required
   set comes from the campaign binding. A missing gate is a set-equality failure,
   not a smaller denominator.
```
