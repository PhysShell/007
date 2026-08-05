# 007 B1 Task Context — case-0002

> Rendered only from the structured projection. No new facts are authored here.

**Task:** Audit R21 evidence provenance: why the current solution is considered active, which records support it, what is superseded, which agent claims were wrong, and which repository outcome is the authority.

**Task id:** case-0002-audit-r21-evidence-provenance

**Selector:** o7.b1.selector/v0 (impl sha256:8e2dd816e6fdd82f6af4ec559e012e5f82484231ba36cc5f38f7e7759dacb029)

**Required topics:** r21-evidence

**Preferred topics:** measurement, reviewer-acceptance, supersession

**Cutoff:** case-0002 Round 0 CLOSED — plane-record frozen 2026-08-05T08:52:33Z, bound to qodec metadata head 427b677

## Current state
- The reviewer window runs from node 264366ab (review at accepted head 3ba3a38, discovery that R21 is needed) to node d5b0a463 (exact-head acceptance of evidence head 04108e7): 258 nodes, reproduced independently by both parties.  _(obs-reviewer-window, authority: deterministic_derivative, status: current)_
- The authoritative record of R21 acceptance is the frozen plane-record — evidence head 04108e7 with its frozen CI and evidence artifacts and PR #16 state — not the reviewer's relayed acceptance. Product head is 940c7629; PR #16 is open, draft, unmerged.  _(obs-repo-authority-04108e7, authority: controller_or_repo_record, status: current)_
- The evidence head 04108e7 carries no CI run: check-runs 0, workflow runs 0, combined status pending — a frozen-empty result recorded from the plane-record, not inferred. Baseline 3ba3a38 and product head 940c7629 each have 11 check-runs, combined status success, 3 workflow runs.  _(obs-evidence-head-ci-empty, authority: controller_or_repo_record, status: current)_
- SOURCE-LOCK v6 supersedes v5 (which mislabelled itself revision 4, left the reviewer digest disagreement open, and listed a publication decision among Round 0 blockers); v5 superseded v4 (whose reviewer run was recorded NOT RUN); v4 superseded v3 (which bound Round 0 to a tree that does not contain it, and whose parsers skipped malformed lines).  _(obs-lock-supersession-chain, authority: controller_or_repo_record, status: current)_
- Both JSONL tools were hardened to fail closed on unreadable input; well-formed blobs reproduced every published count byte-for-byte, so the measurement contract is unchanged.  _(obs-profiler-fail-closed, authority: controller_or_repo_record, status: current)_
- plane-coder is the Claude Code coder session, frozen by record-prefix through record 17111 (17112 records, sha256 8392de80...); the whole-file digest is unstable (append-only), so only the prefix digest is authoritative.  _(obs-plane-coder, authority: platform_capture, status: current)_
- The one value that did not reproduce was the reviewer profile digest; the exact cause is the literal input path serialised into the report ('/tmp/chatgpt-export.json' vs 'chatgpt-export.json', a five-byte diff on one field), with zero differing measured values. Status CLOSED.  _(obs-reviewer-digest-disagreement, authority: deterministic_derivative, status: current)_
- window_invariant v0 computed (not asserted) that the window blob is exactly the anchored slice of the prefix blob: exit 0, byte_identical true, every check passed, output digest 881f1b3b...  _(obs-window-invariant, authority: deterministic_derivative, status: current)_

## Constraints & forbidden actions
- The chosen R21 constraint routes against what the oracle table targets, not against what it may target, with routing derived once and the ledger asked before the verdict. The evidence chain (16 commits 3ba3a38..04108e7) freezes the preservation run, its inputs, its allowed-outcomes envelope and its comparison.  _(obs-oracle-topology-constraint, authority: controller_or_repo_record, status: current)_

## Risks
- The profiler output embeds the literal input path, so any recorded expectation must name the path argument, not only the corpus; canonical arguments are 'chatgpt-export.json' and 'claude-session.window-r21.jsonl'. Recorded rather than fixed in this revision.  _(obs-output-path-dependence, authority: controller_or_repo_record, status: current)_

## Evidence pointers
- obs-reviewer-window: source-lock-v6 sha256:14cfb81f5ea7b0fb567aec1f49a612040539f3a653ecb25218b09c102db62b76
- obs-repo-authority-04108e7: plane-record sha256:3e33addf358f486c39d72e01b6e9d13d025672f6ecabf4425453d29a4939e139
- obs-evidence-head-ci-empty: plane-record sha256:3e33addf358f486c39d72e01b6e9d13d025672f6ecabf4425453d29a4939e139
- obs-lock-supersession-chain: source-lock-v6 sha256:14cfb81f5ea7b0fb567aec1f49a612040539f3a653ecb25218b09c102db62b76
- obs-profiler-fail-closed: source-lock-v6 sha256:14cfb81f5ea7b0fb567aec1f49a612040539f3a653ecb25218b09c102db62b76
- obs-plane-coder: source-lock-v6 sha256:14cfb81f5ea7b0fb567aec1f49a612040539f3a653ecb25218b09c102db62b76
- obs-reviewer-digest-disagreement: source-lock-v6 sha256:14cfb81f5ea7b0fb567aec1f49a612040539f3a653ecb25218b09c102db62b76
- obs-window-invariant: source-lock-v6 sha256:14cfb81f5ea7b0fb567aec1f49a612040539f3a653ecb25218b09c102db62b76
- obs-oracle-topology-constraint: plane-record sha256:3e33addf358f486c39d72e01b6e9d13d025672f6ecabf4425453d29a4939e139
- obs-output-path-dependence: source-lock-v6 sha256:14cfb81f5ea7b0fb567aec1f49a612040539f3a653ecb25218b09c102db62b76

## Omitted (with reasons)
- obs-authority-model: not relevant to this task: topics authority-model share nothing with required r21-evidence or preferred measurement,reviewer-acceptance,supersession
- obs-metadata-head: not relevant to this task: topics round-0 share nothing with required r21-evidence or preferred measurement,reviewer-acceptance,supersession
- obs-next-permitted-step: not relevant to this task: topics product-integration,round-0 share nothing with required r21-evidence or preferred measurement,reviewer-acceptance,supersession
- obs-plane-record-frozen: not relevant to this task: topics product-integration,round-0 share nothing with required r21-evidence or preferred measurement,reviewer-acceptance,supersession
- obs-plane-record-not-frozen-v6: not eligible: superseded_by obs-round0-closed
- obs-plane-reviewer: not relevant to this task: topics source-planes share nothing with required r21-evidence or preferred measurement,reviewer-acceptance,supersession
- obs-profiler-prior-defect: not eligible: superseded_by obs-profiler-fail-closed
- obs-relay-rule: not relevant to this task: topics authority-model share nothing with required r21-evidence or preferred measurement,reviewer-acceptance,supersession
- obs-reviewer-acceptance-claim: not eligible: authority agent_claim is never authoritative
- obs-reviewer-durability-resolved: not relevant to this task: topics durability,round-0 share nothing with required r21-evidence or preferred measurement,reviewer-acceptance,supersession
- obs-reviewer-durability-unresolved-v6: not eligible: superseded_by obs-reviewer-durability-resolved
- obs-round0-closed: not relevant to this task: topics product-integration,round-0 share nothing with required r21-evidence or preferred measurement,reviewer-acceptance,supersession
- obs-source-data-origin: not relevant to this task: topics source-planes share nothing with required r21-evidence or preferred measurement,reviewer-acceptance,supersession
- obs-v3-binding-error: not eligible: status rejected not in force
