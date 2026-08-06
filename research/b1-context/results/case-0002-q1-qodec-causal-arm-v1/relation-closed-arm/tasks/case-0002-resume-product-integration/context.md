# relation-closed context: case-0002-resume-product-integration
producer: o7.b1.relation-closed-control/v0

## selected (6)
- obs-round0-closed [current/controller_or_repo_record] Round 0 is CLOSED: plane-coder, plane-reviewer and plane-record are all FROZEN.
- obs-next-permitted-step [current/controller_or_repo_record] The next permitted step is a baseline application of the current B1 v0 design to case-0002 before any improvement; schema, selector and evaluation contracts remain unfrozen and no downstream (qodec-arm, holdout) is started.
- obs-repo-authority-04108e7 [current/controller_or_repo_record] The authoritative record of R21 acceptance is the frozen plane-record — evidence head 04108e7 with its frozen CI and evidence artifacts and PR #16 state — not the reviewer's relayed acceptance. Product head is 940c7629; PR #16 is open, draft, unmerged.
- obs-plane-record-frozen [current/controller_or_repo_record] The plane-record was frozen as one coherent qodec repository-state snapshot (cutoff 2026-08-05T08:52:33Z, 120 request/response envelopes, all HTTP 200, 90 raw CAS objects) and bound externally to metadata head 427b677.
- obs-metadata-head [current/controller_or_repo_record] The authoritative external metadata head is qodec 427b677 (tree 125f8c4, docs-only forward from 3973192); the authoritative lock at that head is SOURCE-LOCK-v6.yaml.
- obs-reviewer-durability-resolved [current/controller_or_repo_record] Round 0 imported the reviewer export into the durable private CAS (cas:sha256:ed518b33, 5963644 bytes, stored 0444, round-trip verified, encrypted offsite in restic/R2), resolving the durability the frozen lock had left open.

## relations (8)
- obs-round0-closed -supersedes-> obs-plane-record-not-frozen-v6
- obs-reviewer-durability-resolved -supersedes-> obs-reviewer-durability-unresolved-v6
- obs-reviewer-durability-resolved -derived_from-> obs-plane-reviewer
- obs-plane-record-frozen -part_of-> obs-round0-closed
- obs-evidence-head-ci-empty -supports-> obs-repo-authority-04108e7
- obs-repo-authority-04108e7 -supersedes-> obs-reviewer-acceptance-claim
- obs-oracle-topology-constraint -depends_on-> obs-repo-authority-04108e7
- obs-repo-authority-04108e7 -supports-> obs-next-permitted-step

