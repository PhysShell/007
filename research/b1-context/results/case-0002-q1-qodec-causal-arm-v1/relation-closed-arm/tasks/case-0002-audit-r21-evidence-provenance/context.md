# relation-closed context: case-0002-audit-r21-evidence-provenance
producer: o7.b1.relation-closed-control/v0

## selected (12)
- obs-reviewer-window [current/deterministic_derivative] The reviewer window runs from node 264366ab (review at accepted head 3ba3a38, discovery that R21 is needed) to node d5b0a463 (exact-head acceptance of evidence head 04108e7): 258 nodes, reproduced independently by both parties.
- obs-repo-authority-04108e7 [current/controller_or_repo_record] The authoritative record of R21 acceptance is the frozen plane-record — evidence head 04108e7 with its frozen CI and evidence artifacts and PR #16 state — not the reviewer's relayed acceptance. Product head is 940c7629; PR #16 is open, draft, unmerged.
- obs-evidence-head-ci-empty [current/controller_or_repo_record] The evidence head 04108e7 carries no CI run: check-runs 0, workflow runs 0, combined status pending — a frozen-empty result recorded from the plane-record, not inferred. Baseline 3ba3a38 and product head 940c7629 each have 11 check-runs, combined status success, 3 workflow runs.
- obs-lock-supersession-chain [current/controller_or_repo_record] SOURCE-LOCK v6 supersedes v5 (which mislabelled itself revision 4, left the reviewer digest disagreement open, and listed a publication decision among Round 0 blockers); v5 superseded v4 (whose reviewer run was recorded NOT RUN); v4 superseded v3 (which bound Round 0 to a tree that does not contain it, and whose parsers skipped malformed lines).
- obs-profiler-fail-closed [current/controller_or_repo_record] Both JSONL tools were hardened to fail closed on unreadable input; well-formed blobs reproduced every published count byte-for-byte, so the measurement contract is unchanged.
- obs-plane-coder [current/platform_capture] plane-coder is the Claude Code coder session, frozen by record-prefix through record 17111 (17112 records, sha256 8392de80...); the whole-file digest is unstable (append-only), so only the prefix digest is authoritative.
- obs-reviewer-digest-disagreement [current/deterministic_derivative] The one value that did not reproduce was the reviewer profile digest; the exact cause is the literal input path serialised into the report ('/tmp/chatgpt-export.json' vs 'chatgpt-export.json', a five-byte diff on one field), with zero differing measured values. Status CLOSED.
- obs-window-invariant [current/deterministic_derivative] window_invariant v0 computed (not asserted) that the window blob is exactly the anchored slice of the prefix blob: exit 0, byte_identical true, every check passed, output digest 881f1b3b...
- obs-oracle-topology-constraint [current/controller_or_repo_record] The chosen R21 constraint routes against what the oracle table targets, not against what it may target, with routing derived once and the ledger asked before the verdict. The evidence chain (16 commits 3ba3a38..04108e7) freezes the preservation run, its inputs, its allowed-outcomes envelope and its comparison.
- obs-output-path-dependence [current/controller_or_repo_record] The profiler output embeds the literal input path, so any recorded expectation must name the path argument, not only the corpus; canonical arguments are 'chatgpt-export.json' and 'claude-session.window-r21.jsonl'. Recorded rather than fixed in this revision.
- obs-round0-closed [current/controller_or_repo_record] Round 0 is CLOSED: plane-coder, plane-reviewer and plane-record are all FROZEN.
- obs-reviewer-durability-resolved [current/controller_or_repo_record] Round 0 imported the reviewer export into the durable private CAS (cas:sha256:ed518b33, 5963644 bytes, stored 0444, round-trip verified, encrypted offsite in restic/R2), resolving the durability the frozen lock had left open.

## relations (12)
- obs-profiler-fail-closed -supersedes-> obs-profiler-prior-defect
- obs-evidence-head-ci-empty -supports-> obs-repo-authority-04108e7
- obs-reviewer-window -supports-> obs-reviewer-acceptance-claim
- obs-repo-authority-04108e7 -supersedes-> obs-reviewer-acceptance-claim
- obs-oracle-topology-constraint -depends_on-> obs-repo-authority-04108e7
- obs-oracle-topology-constraint -depends_on-> obs-window-invariant
- obs-oracle-topology-constraint -depends_on-> obs-output-path-dependence
- obs-output-path-dependence -blocks-> obs-oracle-topology-constraint
- obs-lock-supersession-chain -derived_from-> obs-v3-binding-error
- obs-repo-authority-04108e7 -supports-> obs-next-permitted-step
- obs-reviewer-durability-resolved -supersedes-> obs-reviewer-durability-unresolved-v6
- obs-round0-closed -supersedes-> obs-plane-record-not-frozen-v6

