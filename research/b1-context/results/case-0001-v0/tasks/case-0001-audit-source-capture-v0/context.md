# 007 B1 Task Context — case-0001

> Rendered only from the structured projection. No new facts are authored here.

**Task:** Audit source-capture integrity and completeness for case-0001: what was captured, how the sessions actually relate, and where the sealed advisory reconstruction diverged from the verified captures.

**Task id:** case-0001-audit-source-capture-v0

**Selector:** o7.b1.selector/v0 (impl sha256:8e2dd816e6fdd82f6af4ec559e012e5f82484231ba36cc5f38f7e7759dacb029)

**Required topics:** source-capture

**Preferred topics:** capture-topology, negative-control

**Cutoff:** end of claude-code-session-c-snapshot-2

## Current state
- Former working labels B and D are a single long-running conversation (session-bd), not two conversations; capture-main-dialog carries both under one native conversation id.  _(obs-bd-one-conversation, authority: platform_capture, status: current)_
- Session E is a distinct, previously unaccounted conversation (capture-compaction, the Qodec context-compaction chat) with its own native conversation id, separate from sessions A, B/D and C.  _(obs-session-e-separate, authority: platform_capture, status: current)_
- A chatgpt platform/account export is pending; it would supersede/confirm the three interim captures. Its absence blocks promotion to SOURCE_SET_COMPLETE but does NOT block the development experiment.  _(obs-account-export-pending, authority: controller_or_repo_record, status: pending)_
- The case-0001 source set status is INTERIM_SOURCE_SET_CAPTURED; it is not SOURCE_SET_COMPLETE.  _(obs-source-set-status-interim, authority: controller_or_repo_record, status: current)_
- claude-code-session-c-snapshot-2 (683 events) is the selected full snapshot of session C and supersedes snapshot-1 (368 events), which is retained only as a historical prefix source.  _(obs-snapshot2-supersedes-1, authority: controller_or_repo_record, status: current)_
- Measured divergence of the negative control from captured topology: it treated former labels B and D as two separate conversations, whereas capture shows one conversation (session-bd). This is measurement data, not an error to fix.  _(obs-nc-divergence-bd, authority: controller_or_repo_record, status: current)_
- Measured divergence of the negative control: it did not account for session E as a distinct conversation, which capture shows exists (session-e).  _(obs-nc-divergence-e, authority: controller_or_repo_record, status: current)_
- Sources are three distinct classes with different authority, never lumped as one RAW: platform captures/snapshots (RAW), deterministic derived transcripts (provenance to a RAW digest), and agent reconstructions (advisory agent_claim negative controls, never RAW).  _(obs-source-classes-authority, authority: controller_or_repo_record, status: current)_
- case-0001-claude-reconstruction-v0 is an advisory agent_reconstruction negative control (authority: advisory, agent_claim). It is never RAW, never contributes to expected state, and exists only to be measured against RAW.  _(obs-negative-control-advisory, authority: controller_or_repo_record, status: current)_

## Constraints & forbidden actions
- The byte-prefix invariant holds only between snapshots of the same append-only container representation (snapshot-2 must contain snapshot-1 as an exact byte prefix). A platform export is a separate capture path and may corroborate content and ordering without byte identity.  _(obs-byte-prefix-scope, authority: controller_or_repo_record, status: current)_
- Promotion to SOURCE_SET_COMPLETE requires: a platform/account export (or an explicitly accepted equivalent) for the chatgpt conversations, a final session C snapshot, and all digest and byte-prefix checks passing.  _(obs-source-set-not-complete-conditions, authority: controller_or_repo_record, status: current)_

## Evidence pointers
- obs-bd-one-conversation: capture-main-dialog sha256:d34c4b9ab2eb90885e270d14199110989cb65eaae17de341db647af49f11cfb4; case-0001-source-selectors research/b1-context/fixtures/case-0001/source-selectors.yaml
- obs-session-e-separate: capture-compaction sha256:4eac30055b0ff67c0fdd6e96903be9f70c2d734398744307bb6df8bbdced52c7; case-0001-source-selectors research/b1-context/fixtures/case-0001/source-selectors.yaml
- obs-byte-prefix-scope: case-0001-manifest research/b1-context/fixtures/case-0001/manifest.yaml
- obs-account-export-pending: case-0001-manifest research/b1-context/fixtures/case-0001/manifest.yaml
- obs-source-set-status-interim: case-0001-manifest research/b1-context/fixtures/case-0001/manifest.yaml
- obs-snapshot2-supersedes-1: claude-code-session-c-snapshot-2 sha256:6a86185402fd69d2fe4aad425a257b06c9bcfc4b28decc0b690d9b314f4066b9; claude-code-session-c-snapshot-1 sha256:011638b28a57cf7a10356a8d35e9ee39124a376503365099e8fcd51cd3fc2c4f
- obs-nc-divergence-bd: case-0001-manifest research/b1-context/fixtures/case-0001/manifest.yaml; case-0001-claude-reconstruction-v0 sha256:37110a62967504fb94eac52cd1e361117069e151a84d4b188ec4c63047c58056
- obs-nc-divergence-e: case-0001-manifest research/b1-context/fixtures/case-0001/manifest.yaml; case-0001-source-selectors research/b1-context/fixtures/case-0001/source-selectors.yaml
- obs-source-set-not-complete-conditions: case-0001-manifest research/b1-context/fixtures/case-0001/manifest.yaml
- obs-source-classes-authority: b1-readme research/b1-context/README.md; case-0001-manifest research/b1-context/fixtures/case-0001/manifest.yaml
- obs-negative-control-advisory: case-0001-manifest research/b1-context/fixtures/case-0001/manifest.yaml; case-0001-claude-reconstruction-v0 sha256:37110a62967504fb94eac52cd1e361117069e151a84d4b188ec4c63047c58056

## Omitted (with reasons)
- obs-b1-hypothesis: not relevant to this task: topics b1-scope share nothing with required source-capture or preferred capture-topology,negative-control
- obs-b1-independent-a-series: not relevant to this task: topics authority-boundary,b1-scope share nothing with required source-capture or preferred capture-topology,negative-control
- obs-b1-readonly-boundary: not relevant to this task: topics authority-boundary,b1-scope share nothing with required source-capture or preferred capture-topology,negative-control
- obs-holdout-no-generalization: not relevant to this task: topics authority-boundary,holdout-readiness share nothing with required source-capture or preferred capture-topology,negative-control
- obs-nc-claim-bd-separate: not eligible: authority agent_claim is never authoritative
- obs-nc-claim-no-session-e: not eligible: authority agent_claim is never authoritative
- obs-next-step-extractors-schema: not relevant to this task: topics b1-scope,holdout-readiness share nothing with required source-capture or preferred capture-topology,negative-control
