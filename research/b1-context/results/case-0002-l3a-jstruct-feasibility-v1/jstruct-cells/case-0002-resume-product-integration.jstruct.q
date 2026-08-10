%q1 jstruct
%q1 body
@rest {"cutoff":{"identity":"case-0002 Round 0 CLOSED — plane-record frozen 2026-08-05T08:52:33Z, bound to qodec metadata head 427b677","raw_source_digest":"sha256:3e33addf358f486c39d72e01b6e9d13d025672f6ecabf4425453d29a4939e139","raw_source_id":"case-0002-round0-source-lock"},"fixture_id":"case-0002","produced_by":"o7.b1.qodec-project-arm/v0","schema":"o7.b1.context/v0","selector":{"adapter_impl_digest":"sha256:3dd51b9f03854be87ee7682fd57a2451bf49f77ad4daca3a065fa4b1cbe2d2e0","qodec_implementation_sha256":"sha256:657237a941679e27e709fba2a18d78c4693d8ddb89f7645761cc602c7a4984dc","qodec_project_version":"qodec.project/v0","selector_id":"o7.b1.qodec-project-arm/v0"},"selectors":{"anchor_observation_ids":[],"anchor_rationale":{},"excluded_topics":[],"preferred_topics":["round-0"],"required_kinds":["decision","next_action","status"],"required_topics":["product-integration"]},"task_id":"case-0002-resume-product-integration"}
@omitted[observation_id,reason]
obs-authority-model|not_selected
obs-evidence-head-ci-empty|not_selected
obs-lock-supersession-chain|not_selected
obs-oracle-topology-constraint|not_selected
obs-output-path-dependence|not_selected
obs-plane-coder|not_selected
obs-plane-record-not-frozen-v6|ineligible
obs-plane-reviewer|not_selected
obs-profiler-fail-closed|not_selected
obs-profiler-prior-defect|ineligible
obs-relay-rule|not_selected
obs-reviewer-acceptance-claim|ineligible
obs-reviewer-digest-disagreement|not_selected
obs-reviewer-durability-unresolved-v6|ineligible
obs-reviewer-window|not_selected
obs-source-data-origin|not_selected
obs-v3-binding-error|ineligible
obs-window-invariant|not_selected

@relations[from,kind,to]
obs-plane-record-frozen|part_of|obs-round0-closed
obs-repo-authority-04108e7|supersedes|obs-reviewer-acceptance-claim
obs-repo-authority-04108e7|supports|obs-next-permitted-step
obs-reviewer-durability-resolved|derived_from|obs-plane-reviewer
obs-reviewer-durability-resolved|supersedes|obs-reviewer-durability-unresolved-v6
obs-round0-closed|supersedes|obs-plane-record-not-frozen-v6

@selected[authority,kind,observation_id,provenance,selection_reason,selection_score,statement,status,topics]
controller_or_repo_record|decision|obs-metadata-head|[{"source_class":"fixture_metadata","source_id":"round0-manifest","source_pointer":"case-0002-round0-source-lock.yaml#metadata_binding"}]|qodec-project: base_selected|1.0|The authoritative external metadata head is qodec 427b677 (tree 125f8c4, docs-only forward from 3973192); the authoritative lock at that head is SOURCE-LOCK-v6.yaml.|current|["round-0"]
controller_or_repo_record|next_action|obs-next-permitted-step|[{"source_class":"fixture_metadata","source_id":"round0-manifest","source_pointer":"case-0002-round0-source-lock.yaml#not_started"}]|qodec-project: base_selected|1.0|The next permitted step is a baseline application of the current B1 v0 design to case-0002 before any improvement; schema, selector and evaluation contracts remain unfrozen and no downstream (qodec-arm, holdout) is started.|current|["product-integration","round-0"]
controller_or_repo_record|evidence|obs-plane-record-frozen|[{"digest":"sha256:3e33addf358f486c39d72e01b6e9d13d025672f6ecabf4425453d29a4939e139","source_class":"repo_record","source_id":"plane-record"}]|qodec-project: base_selected|1.0|The plane-record was frozen as one coherent qodec repository-state snapshot (cutoff 2026-08-05T08:52:33Z, 120 request/response envelopes, all HTTP 200, 90 raw CAS objects) and bound externally to metadata head 427b677.|current|["round-0","product-integration"]
controller_or_repo_record|decision|obs-repo-authority-04108e7|[{"digest":"sha256:3e33addf358f486c39d72e01b6e9d13d025672f6ecabf4425453d29a4939e139","source_class":"repo_record","source_id":"plane-record","source_pointer":"plane-record.json#PR16+evidence_head_04108e7"}]|qodec-project: base_selected|1.0|The authoritative record of R21 acceptance is the frozen plane-record — evidence head 04108e7 with its frozen CI and evidence artifacts and PR #16 state — not the reviewer's relayed acceptance. Product head is 940c7629; PR #16 is open, draft, unmerged.|current|["r21-evidence","product-integration","oracle-topology"]
controller_or_repo_record|evidence|obs-reviewer-durability-resolved|[{"digest":"sha256:ed518b334485ef5421aab0229a4e4a0dc9bc489cb78d1eba00b5eb50d286e7e8","source_class":"fixture_metadata","source_id":"round0-manifest","source_pointer":"case-0002-round0-source-lock.yaml#reviewer_plane_source"}]|qodec-project: base_selected|1.0|Round 0 imported the reviewer export into the durable private CAS (cas:sha256:ed518b33, 5963644 bytes, stored 0444, round-trip verified, encrypted offsite in restic/R2), resolving the durability the frozen lock had left open.|current|["durability","round-0"]
controller_or_repo_record|status|obs-round0-closed|[{"digest":"sha256:3e33addf358f486c39d72e01b6e9d13d025672f6ecabf4425453d29a4939e139","source_class":"fixture_metadata","source_id":"round0-manifest","source_pointer":"case-0002-round0-source-lock.yaml#completion_state"}]|qodec-project: base_selected|1.0|Round 0 is CLOSED: plane-coder, plane-reviewer and plane-record are all FROZEN.|current|["round-0","product-integration"]