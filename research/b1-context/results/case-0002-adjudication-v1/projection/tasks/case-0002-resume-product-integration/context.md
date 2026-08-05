# 007 B1 Task Context — case-0002

> Rendered only from the structured projection. No new facts are authored here.

**Task:** Resume product integration: restore the active state — what is done, what blocks, and the next permitted step.

**Task id:** case-0002-resume-product-integration

**Selector:** o7.b1.selector/v0 (impl sha256:8e2dd816e6fdd82f6af4ec559e012e5f82484231ba36cc5f38f7e7759dacb029)

**Required topics:** product-integration

**Preferred topics:** round-0

**Cutoff:** case-0002 Round 0 CLOSED — plane-record frozen 2026-08-05T08:52:33Z, bound to qodec metadata head 427b677

## Current state
- Round 0 is CLOSED: plane-coder, plane-reviewer and plane-record are all FROZEN.  _(obs-round0-closed, authority: controller_or_repo_record, status: current)_
- The authoritative record of R21 acceptance is the frozen plane-record — evidence head 04108e7 with its frozen CI and evidence artifacts and PR #16 state — not the reviewer's relayed acceptance. Product head is 940c7629; PR #16 is open, draft, unmerged.  _(obs-repo-authority-04108e7, authority: controller_or_repo_record, status: current)_
- The plane-record was frozen as one coherent qodec repository-state snapshot (cutoff 2026-08-05T08:52:33Z, 120 request/response envelopes, all HTTP 200, 90 raw CAS objects) and bound externally to metadata head 427b677.  _(obs-plane-record-frozen, authority: controller_or_repo_record, status: current)_
- The authoritative external metadata head is qodec 427b677 (tree 125f8c4, docs-only forward from 3973192); the authoritative lock at that head is SOURCE-LOCK-v6.yaml.  _(obs-metadata-head, authority: controller_or_repo_record, status: current)_
- Round 0 imported the reviewer export into the durable private CAS (cas:sha256:ed518b33, 5963644 bytes, stored 0444, round-trip verified, encrypted offsite in restic/R2), resolving the durability the frozen lock had left open.  _(obs-reviewer-durability-resolved, authority: controller_or_repo_record, status: current)_

## Next permitted action
- The next permitted step is a baseline application of the current B1 v0 design to case-0002 before any improvement; schema, selector and evaluation contracts remain unfrozen and no downstream (qodec-arm, holdout) is started.  _(obs-next-permitted-step, authority: controller_or_repo_record, status: current)_

## Evidence pointers
- obs-round0-closed: round0-manifest sha256:3e33addf358f486c39d72e01b6e9d13d025672f6ecabf4425453d29a4939e139
- obs-next-permitted-step: round0-manifest case-0002-round0-source-lock.yaml#not_started
- obs-repo-authority-04108e7: plane-record sha256:3e33addf358f486c39d72e01b6e9d13d025672f6ecabf4425453d29a4939e139
- obs-plane-record-frozen: plane-record sha256:3e33addf358f486c39d72e01b6e9d13d025672f6ecabf4425453d29a4939e139
- obs-metadata-head: round0-manifest case-0002-round0-source-lock.yaml#metadata_binding
- obs-reviewer-durability-resolved: round0-manifest sha256:ed518b334485ef5421aab0229a4e4a0dc9bc489cb78d1eba00b5eb50d286e7e8

## Omitted (with reasons)
- obs-authority-model: not relevant to this task: topics authority-model share nothing with required product-integration or preferred round-0
- obs-evidence-head-ci-empty: not relevant to this task: topics oracle-topology,r21-evidence share nothing with required product-integration or preferred round-0
- obs-lock-supersession-chain: not relevant to this task: topics supersession share nothing with required product-integration or preferred round-0
- obs-oracle-topology-constraint: not relevant to this task: topics oracle-topology,r21-evidence share nothing with required product-integration or preferred round-0
- obs-output-path-dependence: not relevant to this task: topics measurement,oracle-topology share nothing with required product-integration or preferred round-0
- obs-plane-coder: not relevant to this task: topics measurement,source-planes share nothing with required product-integration or preferred round-0
- obs-plane-record-not-frozen-v6: not eligible: superseded_by obs-round0-closed
- obs-plane-reviewer: not relevant to this task: topics source-planes share nothing with required product-integration or preferred round-0
- obs-profiler-fail-closed: not relevant to this task: topics measurement share nothing with required product-integration or preferred round-0
- obs-profiler-prior-defect: not eligible: superseded_by obs-profiler-fail-closed
- obs-relay-rule: not relevant to this task: topics authority-model share nothing with required product-integration or preferred round-0
- obs-reviewer-acceptance-claim: not eligible: authority agent_claim is never authoritative
- obs-reviewer-digest-disagreement: not relevant to this task: topics measurement share nothing with required product-integration or preferred round-0
- obs-reviewer-durability-unresolved-v6: not eligible: superseded_by obs-reviewer-durability-resolved
- obs-reviewer-window: not relevant to this task: topics r21-evidence,reviewer-acceptance share nothing with required product-integration or preferred round-0
- obs-source-data-origin: not relevant to this task: topics source-planes share nothing with required product-integration or preferred round-0
- obs-v3-binding-error: not eligible: status rejected not in force
- obs-window-invariant: not relevant to this task: topics measurement share nothing with required product-integration or preferred round-0
