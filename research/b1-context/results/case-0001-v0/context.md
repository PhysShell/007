# 007 B1 Task Context — case-0001

> Rendered only from the structured projection. No new facts are authored here.

**Task:** Continue B1 from the fixed captured state toward the next allowed experiment, without raising B1's authority and without changing the A-series.

**Cutoff:** end of claude-code-session-c-snapshot-2

## Goal
- B1 tests one narrow hypothesis: selected project-state observables can be projected into a concrete task's context (typed state + task-conditioned projection), preserving working state, reducing context cost, and improving next-step execution.  _(obs-b1-hypothesis, authority: controller_or_repo_record, status: current)_

## Current state
- A chatgpt platform/account export is pending; it would supersede/confirm the three interim captures. Its absence blocks promotion to SOURCE_SET_COMPLETE but does NOT block the development experiment.  _(obs-account-export-pending, authority: controller_or_repo_record, status: pending)_
- The case-0001 source set status is INTERIM_SOURCE_SET_CAPTURED; it is not SOURCE_SET_COMPLETE.  _(obs-source-set-status-interim, authority: controller_or_repo_record, status: current)_
- case-0001-claude-reconstruction-v0 is an advisory agent_reconstruction negative control (authority: advisory, agent_claim). It is never RAW, never contributes to expected state, and exists only to be measured against RAW.  _(obs-negative-control-advisory, authority: controller_or_repo_record, status: current)_
- claude-code-session-c-snapshot-2 (683 events) is the selected full snapshot of session C and supersedes snapshot-1 (368 events), which is retained only as a historical prefix source.  _(obs-snapshot2-supersedes-1, authority: controller_or_repo_record, status: current)_
- Sources are three distinct classes with different authority, never lumped as one RAW: platform captures/snapshots (RAW), deterministic derived transcripts (provenance to a RAW digest), and agent reconstructions (advisory agent_claim negative controls, never RAW).  _(obs-source-classes-authority, authority: controller_or_repo_record, status: current)_
- Former working labels B and D are a single long-running conversation (session-bd), not two conversations; capture-main-dialog carries both under one native conversation id.  _(obs-bd-one-conversation, authority: platform_capture, status: current)_
- Measured divergence of the negative control from captured topology: it treated former labels B and D as two separate conversations, whereas capture shows one conversation (session-bd). This is measurement data, not an error to fix.  _(obs-nc-divergence-bd, authority: controller_or_repo_record, status: current)_
- Measured divergence of the negative control: it did not account for session E as a distinct conversation, which capture shows exists (session-e).  _(obs-nc-divergence-e, authority: controller_or_repo_record, status: current)_
- Session E is a distinct, previously unaccounted conversation (capture-compaction, the Qodec context-compaction chat) with its own native conversation id, separate from sessions A, B/D and C.  _(obs-session-e-separate, authority: platform_capture, status: current)_

## Constraints & forbidden actions
- B1 must not drive A-series transitions; B1/B2 do not block each other and neither blocks the A-series (A0 candidate-state continuity onward).  _(obs-b1-independent-a-series, authority: controller_or_repo_record, status: current)_
- B1 is READ-ONLY R&D and NON-AUTHORITATIVE: nothing in it gains authority over campaign or admission state without an explicit promotion decision.  _(obs-b1-readonly-boundary, authority: controller_or_repo_record, status: current)_
- The byte-prefix invariant holds only between snapshots of the same append-only container representation (snapshot-2 must contain snapshot-1 as an exact byte prefix). A platform export is a separate capture path and may corroborate content and ordering without byte identity.  _(obs-byte-prefix-scope, authority: controller_or_repo_record, status: current)_
- case-0001 is a golden development fixture whose questions were shaped on its own material; it can debug the representation but cannot prove generalization. Only holdout cases (questions fixed and digest-bound before compaction) support generalization claims; case-0001 is permanently excluded from holdout.  _(obs-holdout-no-generalization, authority: controller_or_repo_record, status: current)_
- Promotion to SOURCE_SET_COMPLETE requires: a platform/account export (or an explicitly accepted equivalent) for the chatgpt conversations, a final session C snapshot, and all digest and byte-prefix checks passing.  _(obs-source-set-not-complete-conditions, authority: controller_or_repo_record, status: current)_

## Next permitted action
- At the cutoff, the next allowed step was to implement the two deterministic extractors v0 (user-visible derived transcripts) and build state-observables schema v0 from the captured set.  _(obs-next-step-extractors-schema, authority: controller_or_repo_record, status: current)_

## Evidence pointers
- obs-b1-hypothesis: b1-readme research/b1-context/README.md
- obs-account-export-pending: case-0001-manifest research/b1-context/fixtures/case-0001/manifest.yaml
- obs-source-set-status-interim: case-0001-manifest research/b1-context/fixtures/case-0001/manifest.yaml
- obs-negative-control-advisory: case-0001-manifest research/b1-context/fixtures/case-0001/manifest.yaml; case-0001-claude-reconstruction-v0 sha256:37110a62967504fb94eac52cd1e361117069e151a84d4b188ec4c63047c58056
- obs-snapshot2-supersedes-1: claude-code-session-c-snapshot-2 sha256:6a86185402fd69d2fe4aad425a257b06c9bcfc4b28decc0b690d9b314f4066b9; claude-code-session-c-snapshot-1 sha256:011638b28a57cf7a10356a8d35e9ee39124a376503365099e8fcd51cd3fc2c4f
- obs-source-classes-authority: b1-readme research/b1-context/README.md; case-0001-manifest research/b1-context/fixtures/case-0001/manifest.yaml
- obs-bd-one-conversation: capture-main-dialog sha256:d34c4b9ab2eb90885e270d14199110989cb65eaae17de341db647af49f11cfb4; case-0001-source-selectors research/b1-context/fixtures/case-0001/source-selectors.yaml
- obs-nc-divergence-bd: case-0001-manifest research/b1-context/fixtures/case-0001/manifest.yaml; case-0001-claude-reconstruction-v0 sha256:37110a62967504fb94eac52cd1e361117069e151a84d4b188ec4c63047c58056
- obs-nc-divergence-e: case-0001-manifest research/b1-context/fixtures/case-0001/manifest.yaml; case-0001-source-selectors research/b1-context/fixtures/case-0001/source-selectors.yaml
- obs-session-e-separate: capture-compaction sha256:4eac30055b0ff67c0fdd6e96903be9f70c2d734398744307bb6df8bbdced52c7; case-0001-source-selectors research/b1-context/fixtures/case-0001/source-selectors.yaml
- obs-b1-independent-a-series: b1-readme research/b1-context/README.md
- obs-b1-readonly-boundary: b1-readme research/b1-context/README.md
- obs-byte-prefix-scope: case-0001-manifest research/b1-context/fixtures/case-0001/manifest.yaml
- obs-holdout-no-generalization: b1-holdout-readme research/b1-context/holdout/README.md; b1-readme research/b1-context/README.md
- obs-source-set-not-complete-conditions: case-0001-manifest research/b1-context/fixtures/case-0001/manifest.yaml
- obs-next-step-extractors-schema: case-0001-manifest research/b1-context/fixtures/case-0001/manifest.yaml; case-0001-source-selectors research/b1-context/fixtures/case-0001/source-selectors.yaml

## Omitted (with reasons)
- obs-nc-claim-bd-separate: authority agent_claim is never authoritative
- obs-nc-claim-no-session-e: authority agent_claim is never authoritative
