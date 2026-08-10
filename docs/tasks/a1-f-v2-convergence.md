# A1-F v2 convergence ledger

**Status: INVENTORY ADMITTED — PHASE G AUTHORIZED, NO ADJUDICATION YET.**

This document is not normative and never will be. It is a migration checklist
with one job: make sure nothing frozen in A1-F v1 disappears silently between
two Markdown files, which is the traditional life cycle of an important
requirement.

The single normative path stays `docs/q-deck/a1-authority-contracts.md`. There
is no `-v2-final-really-final.md`; we are trying to improve engineering
practice, not reproduce SharePoint.

**No dispositions are recorded in this revision.** Every disposition column is
empty on purpose. A person classifies a list remarkably fast — usually before
finishing copying it — and a disposition written during transcription is a
disposition nobody adjudicated.

## 1. Exact inputs

```yaml
superseded_baseline:
  commit:        b84e9419e751179319925bbc57a434df3583a29a
  document:      docs/q-deck/a1-authority-contracts.md
  document_blob: 7db92f1b3dc9d7040da074956a0b3f2f200174c8
  status:        ACCEPTED / CLOSED / FROZEN
  rounds:        R1, R2, R3, R4, R5, R5.1, R5.2

prototype_design_input:
  reviewed_commit: 37502e3ce5c397a7437445aafb88c13d84ba4ac0
  status:          REVIEWED_WITH_KNOWN_W1_TAIL
  known_open_item:
    W1: public cause variants still permit construction-API bypass

prototype_final_input:
  commit:    TBD
  condition: exact W1-only corrective descendant of reviewed_commit
```

The blob digest was verified against git, not transcribed:
`git rev-parse b84e9419...:docs/q-deck/a1-authority-contracts.md` yields
`7db92f1b3dc9d7040da074956a0b3f2f200174c8`. Every inventory row below was
extracted from **that blob** by script, not retyped.

Pinning the prototype in two states is deliberate: the inventory does not depend
on the prototype at all, so a mechanical Rust tail must not block it — and
equally, `37502e3` must not be described as a clean final design input while a
known construction bypass remains in it. When W1 closes, exactly one line
(`prototype_final_input.commit`) changes; the baseline inventory is not
rewritten. A revision-bound protocol whose own design input said "latest branch"
would be a very human way to lose the plot.

## 2. Process invariants

```text
A1-F v1 is superseded, not invalidated. Its frozen decisions, corrective-round
record, and 5.4 matrix are regression evidence that v2 must account for, not
obsolete prose that v2 may silently replace.

KEEP_V1_MODEL is a legitimate Phase-G outcome.

Default disposition for every existing v1 support object is KEEP_V1_MODEL.
The burden of proof lies on:
  - promoting a support object to envelope-bearing,
  - introducing a new graph node,
  - changing imported-root semantics,
  - replacing rank semantics with another authority model.
Existing prototype code is evidence, never a presumption in favour of the
changed model.

No dispositions are recorded until inventory completeness is independently
admitted (section 6).

post-dispatch ambiguity safety != automatic pre-dispatch redrive.
The FD-14.7a barrier is MUST-V0 and is carried semantically unweakened,
regardless of how v2 represents active_execution, and regardless of whether
SafeRedriveV2 is deferred. The word "SafeRedrive" must not take the safety
catch with it when it leaves.

Crash, replay, ordering and race invariants are never reclassified as
TYPE_PROOF merely because some values became closed enums. A closed enum
proves a value is unrepresentable, not that an inter-event state is
unreachable.
```

## 3. Frozen-decision inventory

Extracted from the normative body of the baseline blob (source lines 1-2395;
the section 9 revision history is excluded, since its prose mentions FD ids
historically rather than defining them). Line numbers are 1-based in that blob.

### 3.1 Top-level decisions
| ID | Source line | Title | Disposition | V0 impact | Version impact | Rationale |
|---|---|---|---|---|---|---|
| `FD-1` | 111 | artifact model, digests, encoding, bounds, unknown fields, versions | | | | |
| `FD-2` | 416 | the evidence graph is acyclic by construction | | | | |
| `FD-3` | 497 | raw provider evidence is separate from adapter-normalized output | | | | |
| `FD-4` | 529 | untrusted report vs controller-accepted artifact | | | | |
| `FD-5` | 542 | authority direction: lineage, head, contract, time | | | | |
| `FD-6` | 590 | duplicate identity: replay vs conflict | | | | |
| `FD-7` | 619 | no model-supplied executable authority | | | | |
| `FD-8` | 641 | replay never invokes a provider | | | | |
| `FD-9` | 655 | post-dispatch ambiguity fails closed | | | | |
| `FD-10` | 681 | provider invocation identity grains, and the shape that carries them | | | | |
| `FD-11` | 740 | the receipt must prove *this* execution produced *this* artifact | | | | |
| `FD-12` | 789 | transition authority | | | | |
| `FD-13` | 816 | evidence is bound to the head it examined | | | | |
| `FD-14` | 824 | campaign state, `state_version`, and the V0 reducer | | | | |
| `FD-15` | 1105 | human command binding and honest actor attestation | | | | |

### 3.2 Sub-decisions

Every `**FD-x.y ...**` definition in the normative body is listed, with no
filtering for "carries an independent obligation" — that judgement is itself
adjudication and belongs to Phase G. Mechanical means complete.
| ID | Source line | Title | Disposition | V0 impact | Version impact | Rationale |
|---|---|---|---|---|---|---|
| `FD-1.1` | 113 | The artifact model | | | | |
| `FD-1.2` | 130 | Envelope digests are computed by field framing, not by JSON | | | | |
| `FD-1.3` | 189 | Encoding and null policy | | | | |
| `FD-1.4` | 200 | Per-object bounds (protocol hard maxima) | | | | |
| `FD-1.5` | 219 | Aggregate bounds (evidence closure) | | | | |
| `FD-1.6` | 341 | Unknown fields and versions | | | | |
| `FD-1.7` | 364 | Media types | | | | |
| `FD-1.8` | 370 | Artifact refs, and what they identify | | | | |
| `FD-1.9` | 398 | `ArtifactKindV1` — the complete closed set | | | | |
| `FD-2.1` | 418 | Rank rule | | | | |
| `FD-2.2` | 434 | Lineage and causation are identifiers, not digests | | | | |
| `FD-2.3` | 440 | No acceptance pointer inside an immutable evidence object | | | | |
| `FD-2.4` | 454 | Imported authority roots | | | | |
| `FD-2.5` | 484 | The resolver's duties | | | | |
| `FD-5.1` | 544 | Lineage | | | | |
| `FD-5.2` | 561 | Head | | | | |
| `FD-5.3` | 567 | Contract | | | | |
| `FD-5.4` | 571 | Time | | | | |
| `FD-10.1` | 693 | The receipt is execution-level | | | | |
| `FD-10.2` | 700 | Boundary classification stays per dispatch | | | | |
| `FD-10.3` | 704 | Retry names its grain | | | | |
| `FD-14.1` | 833 | The state | | | | |
| `FD-14.2` | 837 | The reducer is pure, and genesis is a separate function | | | | |
| `FD-14.3` | 913 | The event log has two classes | | | | |
| `FD-14.4` | 981 | Guards come from FD-12 | | | | |
| `FD-14.5` | 986 | Attention lifecycle is derived, not stored on the artifact | | | | |
| `FD-14.6` | 1005 | `HUMAN_REQUIRED` remembers where it came from, and every exit names its target | | | | |
| `FD-14.7a` | 1043 | An ambiguous execution keeps the dispatch slot | | | | |
| `FD-14.7` | 1069 | Leaving `HUMAN_REQUIRED`, and what a late attention event may not do | | | | |
| `FD-14.8` | 1099 | What A1 does not freeze here | | | | |
| `FD-15.1` | 1107 | Binding | | | | |
| `FD-15.2` | 1113 | Claim and observation are separate objects | | | | |

**Provenance note.** `FD-14.7a` is defined at source line 1043, *before*
`FD-14.7` at line 1069. That ordering is a fact of the baseline (R5.2 inserted
the barrier ahead of the exit rule) and is recorded, not normalised.
Renumbering during transcription is how a supersede quietly becomes a rewrite.

## 4. Negative-matrix regression inventory

Every row of the baseline's 5.4 matrix (blob source lines 2150-2319), copied
mechanically. Source IDs are assigned by position in the exact baseline table —
not by semantic name. Two people will independently invent
`AMBIGUOUS_SLOT_RETAINED` and then reconcile names instead of rows. A mnemonic
column may be added later; it is never the identity.

`semantic_disposition`, `proof_disposition`, `v2_oracle` and `rationale` are
**empty by construction** in this revision.

Legend, for later use — two independent coordinates:

```text
semantic_disposition: KEEP | SUPERSEDE | MOVE_POST_V0 | REMOVE_WITH_RATIONALE
proof_disposition:    TYPE_PROOF | STATIC_KAT | V0_NEGATIVE_TEST | POST_V0_TEST

N = X + Y + Z + R
X = TYPE_PROOF / STATIC_KAT      Y = V0_NEGATIVE_TEST
Z = POST_V0_TEST                 R = REMOVED_WITH_RATIONALE

Y is the negative-test budget of A1-V0, and part of its definition of done.
```

### 4.1 Encoding, bounds, identity

| Source ID | Line | Source case | Source expected outcome | Cited FD | semantic_disposition | proof_disposition | v2_oracle | rationale |
|---|---|---|---|---|---|---|---|---|
| V1-N001 | 2159 | malformed `CoderReport` / `ReviewerReport` | parse rejection, no acceptance, no dispatch | — | | | | |
| V1-N002 | 2160 | unsupported `envelope_version`, `message_kind_version`, or `campaign_protocol_version` | refused, never best-effort parsed (FD-1.6) | `FD-1.6` | | | | |
| V1-N003 | 2161 | unknown field present | rejected at parse time (FD-1.6) | `FD-1.6` | | | | |
| V1-N004 | 2162 | explicit JSON `null` in an optional field | rejected (FD-1.3) | `FD-1.3` | | | | |
| V1-N005 | 2163 | payload exceeding a per-object bound | rejected, never truncated (FD-1.4) | `FD-1.4` | | | | |
| V1-N006 | 2164 | `artifact_refs` whose declared sizes exceed `max_direct_referenced_bytes` | rejected before any read (FD-1.5) | `FD-1.5` | | | | |
| V1-N007 | 2165 | a closure exceeding `max_reachable_closure_bytes`/`_objects` | whole resolution rejected, never partially accepted (FD-1.5) | `FD-1.5` | | | | |
| V1-N008 | 2166 | payload-declared refs (e.g. `coder_report_ref`) pushing the total past a bound, with `envelope.artifact_refs` small | rejected — `immediate_refs` is the union, not the envelope list (FD-1.5) | `FD-1.5` | | | | |
| V1-N009 | 2167 | a typed referenced object whose own payload refs a huge subtree | counted and bounded at the depth it appears; rejected if over (FD-1.5) | `FD-1.5` | | | | |
| V1-N010 | 2168 | a closure whose object count is inflated by repeated refs | deduplicated by `(kind, digest)`; not a rejection (FD-1.5) | `FD-1.5` | | | | |
| V1-N011 | 2169 | stored object whose real size ≠ declared `size` | integrity failure, rejected (FD-1.5) | `FD-1.5` | | | | |
| V1-N012 | 2170 | campaign policy budget above the protocol hard maximum | refused at campaign creation (FD-1.5) | `FD-1.5` | | | | |
| V1-N013 | 2171 | duplicate `message_id`, same envelope digest | idempotent replay; `state_version` unchanged (FD-6) | `FD-6` | | | | |
| V1-N014 | 2172 | duplicate `message_id`, different envelope digest | `IdConflict`, fail closed, attention raised (FD-6) | `FD-6` | | | | |
| V1-N015 | 2173 | same payload bytes, different `expected_input_head` | `IdConflict`, not a replay (FD-6) | `FD-6` | | | | |
| V1-N016 | 2174 | redelivery with a different `created_at` | replay; stored envelope and `first_observed_at` unchanged (FD-5.4) | `FD-5.4` | | | | |

### 4.2 Provenance

| Source ID | Line | Source case | Source expected outcome | Cited FD | semantic_disposition | proof_disposition | v2_oracle | rationale |
|---|---|---|---|---|---|---|---|---|
| V1-N017 | 2180 | provider-produced artifact with no receipt ref | rejected (§3.0) | — | | | | |
| V1-N018 | 2181 | controller-derived artifact carrying a receipt ref | rejected (FD-11) | `FD-11` | | | | |
| V1-N019 | 2182 | valid receipt from a *different* execution attached to a valid report | `ReceiptIncongruent`, rejected (FD-11) | `FD-11` | | | | |
| V1-N020 | 2183 | receipt whose `prompt_digest` / `tool_policy_digest` / `adapter_version` / `role` / `campaign_id` / `round_id` differs from the envelope | `ReceiptIncongruent`, rejected (FD-11) | `FD-11` | | | | |
| V1-N021 | 2184 | `final_normalized_output_ref.digest` ≠ envelope `payload_digest` | `ReceiptIncongruent`, rejected (FD-11) | `FD-11` | | | | |
| V1-N022 | 2185 | controller edits the normalized bytes before enveloping | breaks the digest equality above; rejected (FD-1.1) | `FD-1.1` | | | | |
| V1-N023 | 2186 | receipt with `execution_outcome = dispatch_ambiguous` | no artifact from it accepted; attention raised (FD-9) | `FD-9` | | | | |
| V1-N024 | 2187 | receipt with one ambiguous dispatch among completed ones | `execution_outcome` derives to `dispatch_ambiguous` (§3.12) | — | | | | |
| V1-N025 | 2188 | dispatch 0 `reached`+`completed`, dispatch 1 `not_reached` | `execution_outcome = incomplete`, **never** `failed_pre_dispatch`; no whole-execution retry (§3.12) | — | | | | |
| V1-N026 | 2189 | every dispatch `not_reached` | `failed_pre_dispatch`; safe redrive with a fresh grain (FD-9, §3.12) | `FD-9` | | | | |
| V1-N027 | 2190 | `execution_outcome = completed` whose last dispatch has no `normalized_output_ref` | malformed receipt, rejected before FD-11 (§3.12) | `FD-11` | | | | |
| V1-N028 | 2191 | `final_normalized_output_ref` ≠ the last dispatch's `normalized_output_ref` | malformed receipt, rejected — the report would otherwise bind to a non-terminal blob (§3.12) | — | | | | |
| V1-N029 | 2192 | manifest referencing a `dispatch_id` absent from the receipt | rejected (§3.12.1) | — | | | | |
| V1-N030 | 2193 | model alias recorded as a resolved backend identity | rejected; `resolution.status` must be honest (FD-3) | `FD-3` | | | | |
| V1-N031 | 2194 | retry attempted without established non-dispatch | refused; `dispatch_ambiguous` raised (FD-9) | `FD-9` | | | | |
| V1-N032 | 2195 | retry that names no grain, or a continuation carrying `retry_of_dispatch_id` | refused (FD-10.3) | `FD-10.3` | | | | |

### 4.3 Graph and references

| Source ID | Line | Source case | Source expected outcome | Cited FD | semantic_disposition | proof_disposition | v2_oracle | rationale |
|---|---|---|---|---|---|---|---|---|
| V1-N033 | 2201 | a ref that violates the rank rule | rejected (FD-2.1) | `FD-2.1` | | | | |
| V1-N034 | 2202 | `artifact_ref` outside owned CAS | inert, unresolvable, rejected (FD-1.8) | `FD-1.8` | | | | |
| V1-N035 | 2203 | ref whose declared `kind` disagrees with the slot's expected kind | rejected; the slot wins (FD-2.5) | `FD-2.5` | | | | |
| V1-N036 | 2204 | the same bytes referenced through two typed slots | two distinct closure nodes, both accounted (FD-2.5) | `FD-2.5` | | | | |
| V1-N037 | 2205 | rank-0 bytes that happen to parse as a typed object | never parsed, never promoted (FD-2.5) | `FD-2.5` | | | | |
| V1-N038 | 2206 | agent-supplied `ArtifactRef` naming a new imported authority root | refused; imports come from the registry/binding (FD-2.4) | `FD-2.4` | | | | |
| V1-N039 | 2207 | imported A0 ref failing its own owner's validation | referencing artifact rejected (FD-2.4) | `FD-2.4` | | | | |

### 4.4 Authority and transitions

| Source ID | Line | Source case | Source expected outcome | Cited FD | semantic_disposition | proof_disposition | v2_oracle | rationale |
|---|---|---|---|---|---|---|---|---|
| V1-N040 | 2213 | `claimed_head` ≠ controller-derived candidate head | fail closed, no review dispatch (§3.3) | — | | | | |
| V1-N041 | 2214 | stale `reviewed_head` | no `ReviewVerdict`; report retained as evidence (FD-5.2) | `FD-5.2` | | | | |
| V1-N042 | 2215 | wrong `contract_digest` | rejected (FD-5.3) | `FD-5.3` | | | | |
| V1-N043 | 2216 | lineage fields inconsistent with the canonical campaign binding | rejected (FD-5.1) | `FD-5.1` | | | | |
| V1-N044 | 2217 | unknown `finding_id` referenced by a directive | rejected (§3.7) | — | | | | |
| V1-N045 | 2218 | directive with a different `scope_ref` digest | refused, `SCOPE_EXPANSION_REFUSED` (§3.7) | — | | | | |
| V1-N046 | 2219 | reviewer proposes a shell command as required evidence | mapped to a registry id or `HUMAN_REQUIRED`; never executed (FD-7) | `FD-7` | | | | |
| V1-N047 | 2220 | reviewer execution holding mutation credentials | refused before dispatch (§4) | — | | | | |
| V1-N048 | 2221 | gate result whose bound head is not the current candidate head | diagnostic only, no transition (FD-13) | `FD-13` | | | | |
| V1-N049 | 2222 | transition attempted with an unsatisfied guard | `TransitionRejected`; neither counter advances (FD-14.4) | `FD-14.4` | | | | |
| V1-N050 | 2223 | evidence-only event (feed item, report received, rejected command) | `last_accepted_sequence` advances; `state_version` unchanged **and no other field changes** (FD-14.3) | `FD-14.3` | | | | |
| V1-N051 | 2224 | an evidence-only event that would mutate any other state field | contract violation; the kind belongs in the authority class (FD-14.3) | `FD-14.3` | | | | |
| V1-N052 | 2225 | replay of the same log twice | identical `CampaignStateV1`, zero provider calls (FD-8, FD-14.2) | `FD-8`, `FD-14.2` | | | | |
| V1-N053 | 2226 | log whose `sequence` has a gap, or whose `event_digest` chain breaks | refused; never replayed "as far as it goes" (FD-14.2) | `FD-14.2` | | | | |
| V1-N054 | 2227 | log not beginning with `CampaignCreated` at `sequence = 0` | refused (FD-14.2) | `FD-14.2` | | | | |
| V1-N055 | 2228 | a second `CampaignCreated` later in the log | refused (FD-14.2) | `FD-14.2` | | | | |
| V1-N056 | 2229 | event whose `state_version_after` does not match its kind's class | refused before folding (§3.15) | — | | | | |
| V1-N057 | 2230 | `previous_event_digest` at `sequence = 0` that is not `Digest256::genesis()` | refused (§3.15) | — | | | | |
| V1-N058 | 2231 | a `source_ref` whose `kind`, `media_type`, or `size` changed while its digest did not | different `event_digest`; chain break detected (§3.15) | — | | | | |
| V1-N059 | 2232 | `CoderReportReceived` carrying a rank-3 `source_ref` | accepted — `source_ref` is provenance for evidence kinds, and its expected rank is per-kind (§3.15) | — | | | | |
| V1-N060 | 2233 | event payload bytes re-serialized differently | different `event_payload_digest`, different `event_digest` (§3.15) | — | | | | |
| V1-N061 | 2234 | event whose payload or `evidence_refs` blow the closure bounds | rejected before folding (FD-1.5) | `FD-1.5` | | | | |
| V1-N062 | 2235 | `GateResultsAccepted` with a `fail` on a required gate | result stored, `phase` stays `GATING`; only `HumanAttentionRaised` may reach `HUMAN_REQUIRED` (§3.15.1) | — | | | | |
| V1-N063 | 2236 | `CiResultsAccepted` whose required check is `unavailable` | aggregate `conclusion ≠ passed`; `phase` stays `CI_WAIT` (§3.15.2) | — | | | | |
| V1-N064 | 2237 | `ReviewVerdictAccepted` while `last_ci_results.conclusion ≠ passed` | guard fails; `TransitionRejected` — a correct head does not make a red CI green (§3.15.1) | — | | | | |
| V1-N065 | 2238 | `ReviewVerdict.verdict = blocked` | verdict stored, `phase` stays `REVIEWING`; escalation goes through an attention (§3.15.1) | — | | | | |
| V1-N066 | 2239 | any event other than `HumanAttentionRaised` setting `phase = HUMAN_REQUIRED` | contract violation (FD-14.7) | `FD-14.7` | | | | |
| V1-N067 | 2240 | `HumanAttentionRaised` while `phase = READY_TO_MERGE` | guard fails; the ready-to-merge notice is a feed item, not an attention (§3.9) | — | | | | |
| V1-N068 | 2241 | `HumanAttentionRaised` reusing a `RESOLVED` or `SUPERSEDED` `attention_id` | guard fails — ids are unique for the campaign's lifetime (§3.9) | — | | | | |
| V1-N069 | 2242 | `AttentionSuperseded` whose superseded attention is `RESOLVED` | guard fails (§3.15.1) | — | | | | |
| V1-N070 | 2243 | terminal event leaving `active_round_id` or `active_execution` set | contract violation; terminal canonicalization is atomic (FD-14.2) | `FD-14.2` | | | | |
| V1-N071 | 2244 | `ANSWER_QUESTION` for a `question_id` not in the named entry's `open_question_ids` | rejected — a question nobody escalated was never asked (FD-14.6) | `FD-14.6` | | | | |
| V1-N072 | 2245 | `ANSWER_QUESTION` naming an entry whose `required_decision_kind ≠ answer_question` | rejected (FD-14.6) | `FD-14.6` | | | | |
| V1-N073 | 2246 | a `CoderReport` question that no attention escalated | never enters any entry; unanswerable by construction (FD-14.6) | `FD-14.6` | | | | |
| V1-N074 | 2247 | `GateResultsAccepted` omitting a required gate id | set-equality guard fails; no smaller denominator (§3.15.2) | — | | | | |
| V1-N075 | 2248 | `GateResultsAccepted` carrying a producer-authored `required` flag | rejected as an unknown field; requiredness comes from state (§3.15.2) | — | | | | |
| V1-N076 | 2249 | `GateResultsAccepted` with a duplicate `gate_id` | guard fails (§3.15.2) | — | | | | |
| V1-N077 | 2250 | `GateResultsAccepted` whose `gate_registry_digest` ≠ the campaign binding | guard fails (§3.15.2) | — | | | | |
| V1-N078 | 2251 | `CiResultsAccepted` missing a required check id | guard fails; absence is not a pass (§3.15.2) | — | | | | |
| V1-N079 | 2252 | a fold implementation reading CAS directly | contract violation — `fold` consumes `ResolvedCampaignEventV1` only (FD-14.2) | `FD-14.2` | | | | |
| V1-N080 | 2253 | closure checking attempted in `verify_wire` | contract violation — payload refs are unknowable without CAS (FD-14.2) | `FD-14.2` | | | | |
| V1-N081 | 2254 | `WorkOrderIssued` while `active_execution` is present | guard fails; one execution at a time (§3.15.1) | — | | | | |
| V1-N082 | 2255 | receipt `provider_execution_id` ≠ `active_execution.provider_execution_id` | guard fails; the id has a canonical origin (§3.15.1) | — | | | | |
| V1-N083 | 2256 | a `CorrectiveDirective` expecting a follow-up `WorkOrder` to start the round | contract violation — the directive starts the execution itself (§3.7) | — | | | | |
| V1-N084 | 2257 | gate `error` on a required gate | not green; `phase` stays `GATING`; escalation via attention (§3.15.2) | — | | | | |
| V1-N085 | 2258 | aggregate treating `error` as `fail` or as `pass` | contract violation — the distinction exists in `GateOutcome` for a reason (§3.15.2) | — | | | | |
| V1-N086 | 2259 | `raised_at_state_version` recording the pre-state | rejected; must equal `state_version_after`, or every screen is stale on arrival (§3.15.3) | — | | | | |
| V1-N087 | 2260 | `suspended_from_phase` on the artifact disagreeing with the transition | rejected (§3.15.3) | — | | | | |
| V1-N088 | 2261 | `applied_at_state_version` ≠ the decision's `state_version_after` | rejected (§3.15.3) | — | | | | |
| V1-N089 | 2262 | `SELECT_ATTENTION_ACTION` whose id is not in that entry's `offered_action_ids` | rejected from state alone, without re-reading the artifact (§3.14) | — | | | | |
| V1-N090 | 2263 | `ANSWER_QUESTION` without `attention_id` | rejected — question ownership must be decidable by the fold (§3.10) | — | | | | |
| V1-N091 | 2264 | the same `question_id` open in two active attentions | guard fails at raise time (§3.15.1) | — | | | | |
| V1-N092 | 2265 | `AttentionResolved` leaving its questions open | contract violation; the entry's questions are cleared with it (§3.15.1) | — | | | | |
| V1-N093 | 2266 | a `WorkOrder` whose `budget_policy_digest` ≠ the campaign policy | guard fails (§3.15.1) | — | | | | |
| V1-N094 | 2267 | a receipt whose `request.budget_policy_digest` ≠ the campaign policy | `ReceiptIncongruent` (FD-11) | `FD-11` | | | | |
| V1-N095 | 2268 | a receipt whose `provider_execution_id` ≠ the campaign's `active_execution` | `ReceiptIncongruent` (FD-11) | `FD-11` | | | | |
| V1-N096 | 2269 | `CANCEL` carrying an `attention_id` | rejected by the schema (§3.10) | — | | | | |
| V1-N097 | 2270 | `CANCEL` refused because no attention is open | contract violation — the attention guard does not apply to `CANCEL` (§3.15.1) | — | | | | |
| V1-N098 | 2271 | an `ArtifactRef.kind` outside `ArtifactKindV1` | rejected; the set is closed (FD-1.9) | `FD-1.9` | | | | |
| V1-N099 | 2272 | an imported A0 ref spelled differently from `o7_run::event::ArtifactKind` | rejected — imported spellings are reused verbatim (FD-1.9) | `FD-1.9` | | | | |
| V1-N100 | 2273 | `required_decision_kind = answer_question` with empty `question_ids` | rejected (§3.9) | — | | | | |
| V1-N101 | 2274 | `required_decision_kind = choose_resolution` with no options | rejected (§3.9) | — | | | | |
| V1-N102 | 2275 | `resolve_event` called on `CampaignCreated` with a policy argument | contract violation — genesis uses `resolve_genesis` (FD-14.2) | `FD-14.2` | | | | |
| V1-N103 | 2276 | `budget_policy_digest` not recomputable from the four carried values | seed fails (FD-1.5, §3.15.2) | `FD-1.5` | | | | |
| V1-N104 | 2277 | replay that needs a value only present in a later `WorkOrder` | impossible — genesis carries the policy (§3.15.2) | — | | | | |
| V1-N105 | 2278 | resolution failure (unparseable payload, slot-mismatched source) | `ResolutionFailed`; the event is never folded (FD-14.2) | `FD-14.2` | | | | |
| V1-N106 | 2279 | two implementations framing an enum by different tag bytes | impossible — enums are framed by frozen ASCII name (FD-1.2) | `FD-1.2` | | | | |
| V1-N107 | 2280 | `AttentionResolved` arriving while `phase = CANCEL_REQUESTED` | attention state updates; phase unchanged — a late resolution never un-cancels (FD-14.7) | `FD-14.7` | | | | |
| V1-N108 | 2281 | resuming action while another attention is `OPEN` | that attention resolves; phase stays `HUMAN_REQUIRED`; target not applied (FD-14.7) | `FD-14.7` | | | | |
| V1-N109 | 2282 | exit from `HUMAN_REQUIRED` leaving `suspended_from_phase` set | contract violation; cleared in the same step (FD-14.7, §3.14) | `FD-14.7` | | | | |
| V1-N110 | 2283 | `ProviderExecutionRecorded` for an execution that is not `active_execution` | guard fails; `TransitionRejected` (§3.15.1) | — | | | | |
| V1-N111 | 2284 | `ProviderExecutionRecorded` with `execution_outcome = dispatch_ambiguous` | `active_execution` **retained** and marked `unresolved`; not cleared (FD-14.7a) | `FD-14.7a` | | | | |
| V1-N112 | 2285 | new `WorkOrder`/`ReviewRequest`/`CorrectiveDirective` after an ambiguous execution | guard fails — the dispatch slot is still held, durably across restart (FD-14.7a) | `FD-14.7a` | | | | |
| V1-N113 | 2286 | crash between `ProviderExecutionRecorded(ambiguous)` and the attention event | replay reaches a state that still blocks dispatch (FD-14.7a) | `FD-14.7a` | | | | |
| V1-N114 | 2287 | any V1 event clearing an `unresolved` execution short of a terminal transition | contract violation — only `CANCEL` or supersede end it (FD-14.7a) | `FD-14.7a` | | | | |
| V1-N115 | 2288 | a `HumanCommandRequest` whose `producer_execution_id` names a provider execution | rejected — the human case is the controller's ingress identity (FD-10) | `FD-10` | | | | |
| V1-N116 | 2289 | an `ArtifactRef` to a message kind whose `size` covers only the envelope | rejected — `size` is envelope + payload together (FD-1.8) | `FD-1.8` | | | | |
| V1-N117 | 2290 | an `ArtifactRef` whose `digest` is the payload digest rather than the envelope digest | rejected (FD-1.8) | `FD-1.8` | | | | |
| V1-N118 | 2291 | a closure of small envelopes over large payloads sized by envelopes alone | rejected; the true cost is charged before reading (FD-1.5, FD-1.8) | `FD-1.5`, `FD-1.8` | | | | |
| V1-N119 | 2292 | an event declaring itself authority-bearing against its kind | impossible — class is a function of `event_kind`, not a field (FD-14.3) | `FD-14.3` | | | | |
| V1-N120 | 2293 | `GateResultsAccepted` whose `bound_head` ≠ `current_candidate_head` | guard fails; `TransitionRejected`, neither counter advances (§3.15, FD-13) | `FD-13` | | | | |
| V1-N121 | 2294 | `ReviewVerdictAccepted` while `last_gate_results.bound_head` is a stale head | guard fails; no `READY_TO_MERGE` (§3.15) | — | | | | |
| V1-N122 | 2295 | new candidate accepted after gates passed | `last_gate_results`/`last_ci_results` cleared; `READY_TO_MERGE` unreachable until re-run (§3.14) | — | | | | |

### 4.5 Human lane

| Source ID | Line | Source case | Source expected outcome | Cited FD | semantic_disposition | proof_disposition | v2_oracle | rationale |
|---|---|---|---|---|---|---|---|---|
| V1-N123 | 2301 | human command with stale `expected_campaign_state_version` | rejected (FD-15.1) | `FD-15.1` | | | | |
| V1-N124 | 2302 | human command with stale `expected_head` or contract digest | rejected (FD-15.1) | `FD-15.1` | | | | |
| V1-N125 | 2303 | request asserting its own attestation or transport | rejected — no such field exists (FD-15.2) | `FD-15.2` | | | | |
| V1-N126 | 2304 | `authenticated` recorded with no `authenticator_id` | rejected (FD-15.2) | `FD-15.2` | | | | |
| V1-N127 | 2305 | loopback caller | recorded as `loopback_unauthenticated` + `claimed_identity`, never as an attested operator (FD-15.2) | `FD-15.2` | | | | |
| V1-N128 | 2306 | `unattested` actor | refused (FD-15.2) | `FD-15.2` | | | | |
| V1-N129 | 2307 | answer targeting a superseded `question_id` | not delivered to the coder (§3.10) | — | | | | |
| V1-N130 | 2308 | answer declaring `revise_contract` | effect `contract_revision_requested`; phase stays `HUMAN_REQUIRED`; no autonomous dispatch; `SUPERSEDED` only later, via `CampaignSuperseded` naming an existing successor (§3.10, FD-14.6) | `FD-14.6` | | | | |
| V1-N131 | 2309 | `CampaignSuperseded` naming a successor that does not exist | guard fails; `TransitionRejected` (§3.15.1) | — | | | | |
| V1-N132 | 2310 | attention action outside the server-provided set | rejected (§3.9) | — | | | | |
| V1-N133 | 2311 | ACK on an open attention | one `HumanDecisionRecorded` (`state_version` +1); attention state `ACKNOWLEDGED`, never `RESOLVED`; `phase` unchanged (FD-14.5, FD-14.6) | `FD-14.5`, `FD-14.6` | | | | |
| V1-N134 | 2312 | an attention artifact carrying a `lifecycle` field | rejected as an unknown field — lifecycle is derived state (FD-1.6, FD-14.5) | `FD-1.6`, `FD-14.5` | | | | |
| V1-N135 | 2313 | `AttentionResolved` while another attention is still `OPEN` | phase stays `HUMAN_REQUIRED`; no resume (§3.15) | — | | | | |
| V1-N136 | 2314 | second attention raised while already `HUMAN_REQUIRED` | `suspended_from_phase` preserved, not overwritten (§3.15) | — | | | | |
| V1-N137 | 2315 | `retry_failed_step` with no `suspended_from_phase` | guard fails; `TransitionRejected` (FD-14.6) | `FD-14.6` | | | | |
| V1-N138 | 2316 | resume with no stored `suspended_from_phase` | impossible — the field is required iff `phase = HUMAN_REQUIRED` (§3.14) | — | | | | |
| V1-N139 | 2317 | controller publishes an `action_id` outside the closed V1 set | refused at attention creation (FD-14.6) | `FD-14.6` | | | | |
| V1-N140 | 2318 | `ReviewerReport` carrying `reviewer.identity` / `.model` / `.prompt_version` | rejected as unknown fields; provenance is derived from the receipt (§3.5, §3.6) | — | | | | |
| V1-N141 | 2319 | the same ACK redelivered | idempotent replay; `state_version` unchanged (FD-6) | `FD-6` | | | | |

## 5. Inventory counts

Derived by script from the baseline blob, not asserted.

```text
top-level frozen decisions      15
sub-decisions                   32
5.4 source rows                 141

5.4 rows by group
  Encoding, bounds, identity       16
  Provenance                       16
  Graph and references             7
  Authority and transitions        83
  Human lane                       19

duplicate source IDs            0   (V1-N001 .. V1-N141, contiguous)
rows citing no FD               55
```

Rows citing each decision (a row may cite more than one):

```text
  FD-1.1     1
  FD-1.2     1
  FD-1.3     1
  FD-1.4     1
  FD-1.5     10
  FD-1.6     3
  FD-1.8     4
  FD-1.9     2
  FD-2.1     1
  FD-2.4     2
  FD-2.5     3
  FD-3       1
  FD-5.1     1
  FD-5.2     1
  FD-5.3     1
  FD-5.4     1
  FD-6       4
  FD-7       1
  FD-8       1
  FD-9       3
  FD-10      1
  FD-10.3    1
  FD-11      7
  FD-13      2
  FD-14.2    9
  FD-14.3    3
  FD-14.4    1
  FD-14.5    2
  FD-14.6    7
  FD-14.7    4
  FD-14.7a   4
  FD-15.1    2
  FD-15.2    4
```

**Two numbers worth stating plainly, because both were misremembered before this
extraction ran.** The matrix is **141 rows**, not the ~40 that everyone involved
would have guessed; and the `FD-14.7a` cluster is **four** oracles, not three.
Neither error was careless — both were made by people who had read the document
closely, days earlier. That is the entire argument for section 6.

The 141 figure also moves a planning assumption: if a large share of those rows
classify as `V0_NEGATIVE_TEST`, A1-V0 becomes a fixture-writing project. The
`X / Y / Z / R` split is therefore not bookkeeping — it is the mechanism that
keeps the vertical vertical.

## 6. Independent inventory admission

```text
Producer and reviewer independently derive inventories from the exact
superseded baseline (blob 7db92f1b...). The reviewer does not review this
ledger; the reviewer brings their own list, and the two sets are compared.

If the inventories differ in:
  - row count,
  - row boundaries,
  - source text,
  - FD attribution,
  - duplicate detection,
then inventory admission FAILS CLOSED.

No producer inventory is presumed authoritative.
On existence, the reviewer's enumeration wins: a row they found and this
ledger lacks is a row. Disposition remains the producer's to argue, with a
recorded rationale.

Resolution requires returning to the exact baseline and adjudicating the
disputed source item. Only the reconciled inventory becomes the convergence
ledger input. No Phase G and no v2 drafting begins while any discrepancy
remains.
```

```yaml
producer_inventory:
  fds:    15 top-level + 32 sub
  rows:   141
  method: script extraction from blob 7db92f1b..., see section 5
  defects_found_during_extraction:
    - a multiline sub-decision heading (FD-14.6) was missed by the first pattern
    - revision-history prose produced a false FD-14.6 match; the body was bounded

independent_inventory:
  status: ADMITTED
  reviewer_method: independent enumeration from exact frozen blob
  baseline_blob: 7db92f1b3dc9d7040da074956a0b3f2f200174c8
  fds: 15 top-level + 32 sub
  rows: 141
  groups:
    encoding_bounds_identity: 16    # lines 2159-2174
    provenance: 16                  # lines 2180-2195
    graph_and_references: 7         # lines 2201-2207
    authority_and_transitions: 83   # lines 2213-2295
    human_lane: 19                  # lines 2301-2319
  fd_14_7a_rows: 4                  # V1-N111 .. V1-N114, source lines 2284-2287

reconciliation:
  status: ADMITTED
  discrepancies: 0
  matched:
    - row count
    - row boundaries
    - source-line mapping
    - source case text
    - expected outcome text
    - FD attribution
    - category boundaries
    - duplicate detection and contiguity (V1-N001 .. V1-N141)
    - FD inventory (15 top-level + 32 sub)

baseline_recheck:
  at_commit: 69ccef5db148b21f5e671d68761fce383a34b407
  document_blob: 7db92f1b3dc9d7040da074956a0b3f2f200174c8   # unchanged
  relation_to_baseline: ahead_by 8, behind_by 0, merge-base b84e9419
  contract_touched: no
```

**Reviewer evidence note.** The independent reviewer derived the inventory from
the frozen baseline *before* consulting this ledger, and used `f67c1de` only at
the reconciliation step. Reconciliation found no mismatch in count, boundaries,
source text, expected outcome, FD attribution, or duplicate detection.

Doing it the other way round — read the answer, then confirm the answer — would
have been a magnificent independent verification of nothing at all.

**INVENTORY ADMITTED. Phase G is authorized.** Dispositions remain empty; the
next content work is the graph adjudication of section 7 and nothing else.

## 7. Phase G — graph adjudication

**DECIDED — AWAITING INDEPENDENT REVIEW.** Written as a standalone decision in
`docs/tasks/a1-f-v2-phase-g.md`, before any v2 drafting, because the node set
determines ranks, edges, imported roots, closure and digest domains.

```yaml
phase_g:
  document: docs/tasks/a1-f-v2-phase-g.md
  status:   DECIDED (revision G-R7), awaiting re-review
  review_1: CHANGES_REQUESTED — four P1s, all accepted; the central
            conclusion changed from 13 envelope kinds to 11
  review_2: CHANGES_REQUESTED — two P1s, both accepted; the count was not
            challenged and did not move
  review_3: CHANGES_REQUESTED — two P1s, both accepted; node universe, the
            count of 11, and CampaignRunBinding-as-support all APPROVED
  review_4: CHANGES_REQUESTED — one P1: the edge ledger had been measured
            against the prototype instead of the frozen contract. Node
            universe, count, support boundary, binding lifecycle, wrapper
            boundary and rank model all APPROVED and unchanged
  review_5: CHANGES_REQUESTED — two P1s, both accepted. The field-path
            deferral was APPROVED, but the rebuilt ledger stopped at node
            CLASSES (near-cartesian, and not even a superset — it had no
            event-payload -> message row while frozen
            CampaignTerminalErrorPayloadV1.evidence_refs carries rank <= 4),
            and its 40 KEEP dispositions conflated a reference SURFACE with a
            semantic relation. Closed by a new layer: the semantic edge
            registry, field paths still owed to the v2 draft
  review_6: CHANGES_REQUESTED — four P1s, all accepted; the new LAYER was
            approved and every finding landed inside it. (a) stripping rank
            from the 8 open surfaces never re-admitted the V0 edges they must
            carry — frozen ReviewRequestV1.evidence_refs is required and its
            ordering normative (contract, diff, deterministic evidence), and
            the registry gave ReviewRequest none of the three; (b) sources
            keyed on the log root / "the payload" could not reject
            CoderReportReceived.source_ref -> ReviewVerdict, leaving the wire
            to fix an admission the registry claims to own; (c)
            CorrectiveDirective -> ReviewVerdict was Intra though frozen
            §3.15.1 gives CorrectiveDirectiveIssued a NEW active_round_id;
            (d) two distinct A0 wrapper kinds were merged, and the directive's
            input-state edges were unearned since CampaignRunBinding is
            already the execution-to-input-state authority
  review_7: CHANGES_REQUESTED — three P1s, all accepted; §3 untouched and four
            G-R6 decisions upheld on re-examination (CI exclusion, directive
            retraction, event/payload discrimination, Causal classification).
            (a) the frozen extractor was FLAT: ReviewVerdictV1.findings is typed
            "as §3.5, validated" and §3.5 carries findings[].evidence_refs, so
            the baseline is 41 slots / 32 exact / 9 open, and extraction is now
            recursive — exactly five cross-schema type refs exist, four scalar,
            so there is no second hidden slot; (b) zeroing reviewer AND verdict
            evidence redefined a BOUND contract — docs/autonomy-controller.md
            (accepted c5b3ae0b, PR #93), which frozen §0 says A1 consumes and
            never redefines, lists "evidence references" in the ReviewVerdict
            minimum; both surfaces NARROWED to ContractDocument/Diff/GateLog,
            with the transitive-only alternative recorded as available and not
            taken; (c) AnyCommittedEnvelope is a META-TARGET, not a terminal —
            it resolves to a concrete typed message whose closure must be
            traversed and charged, and calling it terminal under-accounted the
            FD-1.5 budget. Bookkeeping: 20 terminal kinds + 1 meta-target
  evidence: 37502e3 edge registry — 59 entries (53 envelope-source + 6 A2
            transition) plus a global AnyCommittedEnvelope causation rule;
            metric is "specific V0 consumer in-degree" with an explicit
            exclusion rule, derived by script
  result:
    envelope_bearing_kinds: 11   # v1 unchanged — KEEP_V1_MODEL won the
                                 # boundary question
    new_support_authority: CampaignRunBinding   # REQUIRED_V0, not a message
    kept_as_support:  ProviderInvocationReceipt, InteractionManifest,
                      ScopeContractV1, CampaignEventPayload
    out_of_v0:        ArtifactImported, RunArtifactSource,
                      EstablishedNonDispatchEvidence
    edge_model:       exact registry authoritative; derived rank defined over
                      the Intra TYPED-NODE subgraph (messages + typed support
                      objects), never over Causal edges
    binding_lifecycle: pre-dispatch binding admission required — one binding
                      identity established durably before the first dispatch;
                      a controller obligation, not a promotion argument, and
                      not an entry in the edge registry
    exact_edge_universe: baseline is the FROZEN contract, not the prototype —
                      41 ArtifactRef slots extracted from blob 7db92f1b by a
                      RECURSIVE extractor (a flat one missed
                      ReviewVerdictV1.findings[].evidence_refs, reachable only
                      through "as §3.5"); the 59
                      prototype rows are evidence (15 match a frozen slot by
                      name, 38 do not). The 41 slots split 32 EXACT / 9 OPEN,
                      and only the exact ones become semantic edges: a generic
                      ArtifactRef-valued field creates no graph authority
    semantic_registry: 69 edges, exact source kind -> exact target kind + class
                      (56 Intra / 13 Causal). Sources are DISCRIMINATED: the
                      registry is keyed on CampaignEvent(<event_kind>) and on
                      <PayloadVariant>, never on the log root or "the payload",
                      because frozen 3.15.1/3.15.2 fix targets per variant. All
                      21 frozen event kinds appear. No wildcard source and no
                      class row; exactly one sanctioned open TARGET
                      (AnyCommittedEnvelope, for CampaignFeedItem causation),
                      carried as a visible row. Acyclicity checked by machine
                      over the 47 typed nodes (26 Intra typed->typed edges).
                      Phase G closes edge MEANING; the v2 draft owes the
                      field-path spelling, and every field must realize exactly
                      one admitted edge — adding an unlisted relation reopens or
                      supersedes Phase G
    open_surfaces:    9 of the 41 frozen slots name no target kind. Each now has
                      a disposition: ReviewRequest.evidence_refs, ReviewerReport
                      .findings[].evidence_refs and ReviewVerdict.findings[]
                      .evidence_refs each NARROWED to ContractDocument, Diff,
                      GateLog; CampaignFeedItem.subject_refs SANCTIONED OPEN as
                      a meta-target; the other five have NO admitted target,
                      which is a real V0 restriction rather than a deferral
    campaign_event:   source-only log root, never an ArtifactRef target; its
                      payload is a legitimate target and is traversed by the
                      closure resolver under artifact-closure bounds
    naming:           CandidateReceipt vs CandidateAdmissionReceipt and
                      ProviderExecutionReceipt vs ProviderInvocationReceipt are
                      an OPEN supersede decision under FD-1.9, not a transcription
                      side effect
    wrapper_boundary: PARSED_BUT_GRAPH_TERMINAL
    e_v0_4:           class only (manifest = typed support object); ALL
                      numeric bounds deferred to the v2 wire/bounds draft
```

The scope statement below is retained as written, so the decision can be checked
against the mandate it was given rather than against a mandate rewritten to fit
the answer. Phase G is
written and independently reviewed *before* the main v2 draft, because the node
set determines ranks, edges, imported roots, closure and digest domains;
drafting v2 while the node set is open means drafting it twice.

Phase G answers five classes of question:

1. final authoritative node universe;
2. envelope-bearing vs support-object boundary;
3. edge model — exact registry, with rank derived if still useful;
4. imported-root and closure semantics;
5. typed external wrapper semantics.

Per-object test, applied to every candidate:

```text
Does it independently require:
  - logical message identity?
  - causation / lineage?
  - acceptance / idempotency lifecycle?
  - independent replay addressability?
  - controller / provider / human producer authority?
If the answers are insufficient, it remains a support object.
```

Question 5 exists because it is a live seam, not a formality. The baseline's
FD-2.4 says imported roots are *never parsed by A1*, while typed A0 wrappers
imply A1 knows their shape — the same conflict that in R5.1 forced rank 0 to be
redefined as "terminal in the reference graph" rather than "opaque bytes". The
fork:

```text
OPAQUE_TERMINAL
  A1 knows only enough to hand the reference to the owning resolver;
  it does not parse the referenced authority representation.

PARSED_BUT_GRAPH_TERMINAL
  A1 owns a typed wrapper schema and validates that wrapper's local structure,
  but the referenced A0/R1 authority remains terminal in the A1 evidence graph,
  and semantic validation belongs to the owning layer or an explicit
  cross-layer resolver.

OTHER
  requires explicit new justification.
```

Objects known to need this adjudication (the list is completed in Phase G, not
here): `ProviderExecutionReceipt`, `InteractionManifest`, `CampaignRunBinding`,
`ArtifactImported`, and the typed A0/R1 wrappers.

`ArtifactImported` carries the strictest burden: v1 refused `ArtifactAcceptance`
precisely because an acceptance-like node sits between ranks 3 and 4 and forces
re-ranking (FD-2.3). If it returns, its necessity must follow from a real V0
import consumer. Schema symmetry gets no vote.

## 7a. Implementation-derived adjudication input

The first implementation attempt (`o7-a1-contracts`, PR #124 at `b2ba165`) was
written contract-preserving against blob `7db92f1b` and, in one step, surfaced
four facts that six rounds of contract review did not. They are recorded here as
**input**, not as dispositions: three are implementation-level and one is a
contract seam that cannot be repaired inside a contract-preserving PR without
quietly authoring a new FD by hand.

PR #124 is reclassified accordingly:

```yaml
pr_124:
  commit: b2ba165f16b6ea092b6c305fde2c85893fc787a5
  role:   EMPIRICAL IMPLEMENTATION PROBE for frozen A1-F v1
  not:    accepted A1-V0 step 1
  step_2: NOT AUTHORIZED
  merge:  NOT AUTHORIZED
  reason: it implements v1, while the sanctioned line is
          INVENTORY ADMITTED -> Phase G -> v2 convergence -> freeze -> A1-V0
```

| ID | Finding | Level | Verified against |
|---|---|---|---|
| `E-V0-1` | An unchecked public `Deserialize` defeats the intended parse boundary | implementation | `json.rs:84-89`, `envelope.rs:63-65` |
| `E-V0-2` | Schema-specific `Text` bounds disappear behind a generic `Text` | implementation | frozen lines 1193-1194, 1575 |
| `E-V0-3` | `CommitId` needs repository-object-format context to be checkable | implementation + wording | frozen line 1166 |
| `E-V0-4` | FD-1.4 and FD-1.8 do not jointly determine `ArtifactRef` max size | **contract** | frozen FD-1.4 block, FD-1.8 block |

**E-V0-1.** `parse_payload` is `validate_document` followed by
`serde_json::from_value`; it never calls `validate()`. And because `EnvelopeV1`
derives `Deserialize` publicly, `serde_json::from_str::<EnvelopeV1>` bypasses the
whole pre-deserialization walk — null policy, depth, array and string bounds, BOM
policy. PR #124's own review question 3 ("can the global rules be bypassed by
parse ordering?") therefore answers **NO**. The shape this wants is the one this
project keeps rediscovering: a wire type that deserializes, a validation step,
and a checked type with no public unchecked constructor. The same applies to
`ArtifactRef`.

**E-V0-2.** The frozen envelope table bounds `producer_adapter_version` at 128
bytes and `model_identity` at 256; both are carried as the generic `Text`, which
admits 65536, and `validate()` does not narrow them. An acceptance gap, not a
test gap.

**E-V0-3.** The frozen scalar is "full object id, the repository's
object-format width". The implementation accepts 40 *or* 64 unconditionally,
which means a 40-hex id passes in a SHA-256 repository merely because another
repository format exists where it would be valid. Wire syntax and resolved
validity want separating: a wire claim (lowercase hex, candidate full width)
versus a checked value (width equals the bound repository's object format). A
step with no repository context should not pretend to have proved the second
half.

**E-V0-4 — the contract seam.** FD-1.4 bounds *payloads* and *evidence blobs*.
It never bounds stored **envelope** bytes. FD-1.8, added in R5.2, then defines a
ref to an envelope-bearing artifact as covering `stored envelope bytes + stored
payload bytes`. No maximum for that sum is derivable from the frozen text: the
implementation's `1 MiB + min(64 MiB, 1 MiB)` is invented, and so is any other
number one might propose, including a tidy `2 * MAX_CONTROL_ARTIFACT_BYTES`.

A second seam sits beside it. FD-1.4 lists `manifest` among the 64 MiB evidence
blobs while also bounding "any typed A1 payload" at 1 MiB, and §3.12.1 makes
`InteractionManifestV1` a typed A1 object. The manifest is therefore bounded at
1 MiB and at 64 MiB simultaneously.

Both halves are traceable to this document's own history: the FD-1.4 list dates
from the original freeze, the FD-1.8 summing rule from R5.2, and no round
reconciled them. That is precisely why this is convergence input rather than a
bug report — a contract-preserving PR that "fixed" it would be writing a new FD
under a commit message that claims to preserve the old one.

Phase G and the v2 draft own the repair. Candidate shapes, recorded without
choosing between them: bound the envelope explicitly; or define ref size over
the payload only and carry the envelope's own size separately; or classify
typed non-envelope objects out of the evidence-blob list and give them the
control-artifact bound.

## 8. Version schedule

```text
envelope_version = 2            decided: the envelope changes fundamentally
campaign_protocol_version       open: 2 iff reducer/event semantics change
message_kind_version per kind   OUTPUT of Phase G, not an input — until the
                                node set is final, it is unknown whose payload
                                changed shape
```

## 9. What this revision deliberately does not contain

- no dispositions;
- no Phase G decisions;
- no edits to `docs/q-deck/a1-authority-contracts.md`;
- no content carried over from the types prototype;
- no improved wording of matrix rows. They are copied as a forensic examiner
  copies, not as an editor edits. A row whose phrasing is improved during
  transcription is a row whose reconciliation has just become an argument
  about English.
