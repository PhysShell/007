# 007 B1 Task Context — case-0001

> Rendered only from the structured projection. No new facts are authored here.

**Task:** Continue B1 from the fixed captured state toward the next allowed experiment, without raising B1's authority and without changing the A-series.

**Task id:** case-0001-continue-b1-v0

**Selector:** o7.b1.selector/v0 (impl sha256:8e2dd816e6fdd82f6af4ec559e012e5f82484231ba36cc5f38f7e7759dacb029)

**Required topics:** b1-scope

**Preferred topics:** authority-boundary, holdout-readiness

**Cutoff:** end of claude-code-session-c-snapshot-2

## Goal
- B1 tests one narrow hypothesis: selected project-state observables can be projected into a concrete task's context (typed state + task-conditioned projection), preserving working state, reducing context cost, and improving next-step execution.  _(obs-b1-hypothesis, authority: controller_or_repo_record, status: current)_

## Current state
- A chatgpt platform/account export is pending; it would supersede/confirm the three interim captures. Its absence blocks promotion to SOURCE_SET_COMPLETE but does NOT block the development experiment.  _(obs-account-export-pending, authority: controller_or_repo_record, status: pending)_
- case-0001-claude-reconstruction-v0 is an advisory agent_reconstruction negative control (authority: advisory, agent_claim). It is never RAW, never contributes to expected state, and exists only to be measured against RAW.  _(obs-negative-control-advisory, authority: controller_or_repo_record, status: current)_
- Sources are three distinct classes with different authority, never lumped as one RAW: platform captures/snapshots (RAW), deterministic derived transcripts (provenance to a RAW digest), and agent reconstructions (advisory agent_claim negative controls, never RAW).  _(obs-source-classes-authority, authority: controller_or_repo_record, status: current)_

## Constraints & forbidden actions
- B1 must not drive A-series transitions; B1/B2 do not block each other and neither blocks the A-series (A0 candidate-state continuity onward).  _(obs-b1-independent-a-series, authority: controller_or_repo_record, status: current)_
- B1 is READ-ONLY R&D and NON-AUTHORITATIVE: nothing in it gains authority over campaign or admission state without an explicit promotion decision.  _(obs-b1-readonly-boundary, authority: controller_or_repo_record, status: current)_
- case-0001 is a golden development fixture whose questions were shaped on its own material; it can debug the representation but cannot prove generalization. Only holdout cases (questions fixed and digest-bound before compaction) support generalization claims; case-0001 is permanently excluded from holdout.  _(obs-holdout-no-generalization, authority: controller_or_repo_record, status: current)_
- Promotion to SOURCE_SET_COMPLETE requires: a platform/account export (or an explicitly accepted equivalent) for the chatgpt conversations, a final session C snapshot, and all digest and byte-prefix checks passing.  _(obs-source-set-not-complete-conditions, authority: controller_or_repo_record, status: current)_

## Next permitted action
- At the cutoff, the next allowed step was to implement the two deterministic extractors v0 (user-visible derived transcripts) and build state-observables schema v0 from the captured set.  _(obs-next-step-extractors-schema, authority: controller_or_repo_record, status: current)_

## Evidence pointers
- obs-b1-independent-a-series: b1-readme research/b1-context/README.md
- obs-b1-readonly-boundary: b1-readme research/b1-context/README.md
- obs-next-step-extractors-schema: case-0001-manifest research/b1-context/fixtures/case-0001/manifest.yaml; case-0001-source-selectors research/b1-context/fixtures/case-0001/source-selectors.yaml
- obs-b1-hypothesis: b1-readme research/b1-context/README.md
- obs-holdout-no-generalization: b1-holdout-readme research/b1-context/holdout/README.md; b1-readme research/b1-context/README.md
- obs-source-set-not-complete-conditions: case-0001-manifest research/b1-context/fixtures/case-0001/manifest.yaml
- obs-account-export-pending: case-0001-manifest research/b1-context/fixtures/case-0001/manifest.yaml
- obs-negative-control-advisory: case-0001-manifest research/b1-context/fixtures/case-0001/manifest.yaml; case-0001-claude-reconstruction-v0 sha256:37110a62967504fb94eac52cd1e361117069e151a84d4b188ec4c63047c58056
- obs-source-classes-authority: b1-readme research/b1-context/README.md; case-0001-manifest research/b1-context/fixtures/case-0001/manifest.yaml

## Omitted (with reasons)
- obs-bd-one-conversation: not relevant to this task: topics capture-topology,source-capture share nothing with required b1-scope or preferred authority-boundary,holdout-readiness
- obs-byte-prefix-scope: not relevant to this task: topics capture-topology,source-capture share nothing with required b1-scope or preferred authority-boundary,holdout-readiness
- obs-nc-claim-bd-separate: not eligible: authority agent_claim is never authoritative
- obs-nc-claim-no-session-e: not eligible: authority agent_claim is never authoritative
- obs-nc-divergence-bd: not relevant to this task: topics capture-topology,negative-control share nothing with required b1-scope or preferred authority-boundary,holdout-readiness
- obs-nc-divergence-e: not relevant to this task: topics capture-topology,negative-control share nothing with required b1-scope or preferred authority-boundary,holdout-readiness
- obs-session-e-separate: not relevant to this task: topics capture-topology,source-capture share nothing with required b1-scope or preferred authority-boundary,holdout-readiness
- obs-snapshot2-supersedes-1: not relevant to this task: topics capture-topology,source-capture share nothing with required b1-scope or preferred authority-boundary,holdout-readiness
- obs-source-set-status-interim: not relevant to this task: topics source-capture share nothing with required b1-scope or preferred authority-boundary,holdout-readiness
