# QODEC has moved

QODEC (the token-aware lossless codec lab) was extracted from this repository
into its own standalone repository:

**Standalone repository:** https://github.com/PhysShell/qodec

**Provenance finalization commit** (the standalone repository's own
self-hash-locked `MIGRATION_PROVENANCE.json`, describing the full extraction):
`0e1e7c77fb7998c147d8958c7bd2f85b5ac8bfbe`

**Migration provenance record hash:**
`sha256:93ffbbb6c44d58f9b47d546d7b167a039cf5f1462c20a3e9409fa82991af2feb`

**Migration source:** the verified tip of `PhysShell/007`'s stacked,
intentionally-unmerged scope-PR chain — `#54 -> #55 -> #56` — never a merge
commit; none of those three PRs were merged.

**Cleanup predecessor:** [`PhysShell/007#59`](https://github.com/PhysShell/007/pull/59)
retired the embedded workflows' automatic triggers as an intermediate safety
step, before this PR's full removal of the embedded `qodec/` tree, its
workflow files, and its QODEC-only Nix outputs.

**Migration tag status:** `migration-from-007-v1` — **pending external
publication**. This tag is intended to point at the provenance finalization
commit above (`0e1e7c77fb7998c147d8958c7bd2f85b5ac8bfbe`) in the standalone
repository, but no session in this environment has been able to push a tag
ref there (the environment's git proxy rejects `refs/tags/*` pushes with
HTTP 403; branch pushes are unaffected). The tag has not been fabricated or
recorded as published anywhere. A maintainer with full push credentials
needs to create and push it directly.

## What remains in `PhysShell/007`

All historical evidence of QODEC's development here is preserved and
unaltered:

- PRs [#54](https://github.com/PhysShell/007/pull/54),
  [#55](https://github.com/PhysShell/007/pull/55), and
  [#56](https://github.com/PhysShell/007/pull/56) — closed without merging,
  titles/bodies/comments/reviews untouched, branches not deleted.
- The [`n2d0-durable-evidence-v1`](https://github.com/PhysShell/007/releases/tag/n2d0-durable-evidence-v1)
  release, its tag, and all its assets.
- Every historical GitHub Actions run, artifact, discussion, receipt, and
  hash produced by the pre-migration QODEC work.
- The full git history of the (now-deleted) `qodec/` tree, reachable via any
  commit before this cleanup.

Nothing above was deleted, rewritten, or force-pushed by this cleanup.

## Where development continues

All new QODEC development, CI, and benchmark work happens only in
[`PhysShell/qodec`](https://github.com/PhysShell/qodec). This repository
(`PhysShell/007`) no longer contains an embedded copy of QODEC, its Nix
package/app/check outputs, or its GitHub Actions workflows.
