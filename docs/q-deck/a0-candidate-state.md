# Q-Deck A0: candidate-state continuity

## Purpose

R1 (`docs/q-deck/r1-command.md`) proved that a follow-up **Command** continues
the same provider session (`--resume <session_id>`) but explicitly does NOT
carry the parent run's file state forward — every `o7 continue` starts a
fresh worktree at its own `--base` with zero carry-over (R1 §"Known,
deliberate exclusions", "No worktree/diff carryover across a command").
Alpha A0 closes that gap: a follow-up command must continue not only the
provider's conversational memory but the EXACT cumulative file state the
prior sealed run produced.

This is additive on top of the frozen R1 contract. Every R1 invariant is
preserved unchanged:

- A command never reopens a sealed run (§0).
- Every command still mints a fresh `CommandId`/`RunId`/ledger run/attempt/
  worktree/canonical event stream/sealed verdict (§2/§9.5).
- The parent-validity gates (sealed, true-tail — §5) are unchanged.
- The durable dispatch boundary (`AgentStarted`, §11) and its fail-closed
  `ValidUnsealedDispatchAmbiguous` classification are unchanged and are NOT
  weakened by anything here — see §5 below for how A0's own failure modes
  compose with it.
- The exact-Command-lineage binding (`CommandBindingCaptured`, §10.3) is
  unchanged; A0 adds a parallel identity check, it does not replace this one.

## 1. The frozen candidate-state model

**One immutable conversation base commit + one cumulative Git binary patch
per sealed run (relative to that SAME base, never to the previous run's own
patch) + one expected Git tree OID.**

Every run in a conversation stores its full cumulative state relative to the
conversation's ORIGINAL, immutable base commit — never a delta against the
previous child. Concretely, if a conversation's base is commit `X`:

```
Run A candidate  = (changes A) relative to X
Command B candidate = (changes A + B) relative to X
Command C candidate = (changes A + B + C) relative to X
```

This lets any child materialize independently and non-sequentially: fresh
worktree at `X` → apply ONE cumulative patch → verify the resulting tree
OID. It never requires walking or re-applying a chain of prior patches, and
a child never depends on any ancestor's worktree still existing.

**The conversation's base commit is fixed once, by the conversation's first
ledger-tracked run** (`o7 run --ledger`, or the top-level run that starts a
Q-Deck conversation) — its own `--base` resolves to a commit, and that
commit becomes every descendant's base for the rest of the conversation's
life. A command continuation's own `--base` CLI flag is IGNORED for
candidate-state purposes once a parent candidate receipt exists (see §3) —
the base commit is inherited from the parent's own receipt, not
re-resolved. (The flag still exists for the plain, non-continuation `o7 run`
path, where there is no parent to inherit from.)

## 2. New canonical events and artifacts (`crates/o7-run/src/event.rs`)

Following the exact precedent `ProviderSessionCaptured`/`CommandBindingCaptured`
set in R1's fifth round: evidence-only (never verdict-bearing), at-most-once
per run, referenced by digest like every other artifact, with a
`#[serde(skip_serializing_if = "Option::is_none")]` field on `RunState` so
the frozen `normalized_digest()` known-answer test stays byte-identical for
every pre-A0 sealed record.

**New `ArtifactKind::CandidateState`** (9th variant) — a candidate-state
receipt JSON artifact. The SAME kind is used both when a run CAPTURES its own
candidate state and when a child COPIES a parent's receipt into its own
record to MATERIALIZE from it — both are "a candidate-state receipt",
just captured vs. consumed.

**`RunEventKind::CandidateStateCaptured { receipt: ArtifactRef }`** — emitted
by the run that JUST finished (both `execute_live` and `continue_execute`,
the two ledger-backed paths — never the bare, no-`--ledger` `execute`, which
can never become a parent), immediately after `PatchCaptured`, before the
gate loop. `RunState.candidate_state: Option<ArtifactRef>` holds it.

**`RunEventKind::CandidateStateMaterialized { source_run_id: RunId,
candidate_receipt: ArtifactRef, expected_tree_oid: String, actual_tree_oid:
String }`** — emitted by a command-continuation CHILD, in `continue_execute`,
immediately after `CommandBindingCaptured` and BEFORE `AgentStarted` (the
durable dispatch boundary — §5). `candidate_receipt` references the CHILD's
OWN local copy of the parent's receipt bytes (`parent_candidate_receipt.json`,
copied into the child's own run directory), so replaying the child alone —
without the parent's directory still existing — is self-contained, exactly
like `RunStarted.task` is a self-contained copy of the command text, not a
live reference to something external. `RunState.candidate_materialized:
Option<CandidateMaterialization>` (a new small struct: the same four fields)
holds it. The reducer defensively refuses (`ReduceError`) any stream where
`expected_tree_oid != actual_tree_oid` — by construction `continue_execute`
never appends this event unless it already verified they match, so a stream
where they disagree is tamper/corruption, not a legitimate outcome.

Both new events participate in the SAME digest-chain framing every other
event does (`fold_kind`/`tag()`/`name()`); nothing about the chain or replay
primitive itself changes.

## 3. The candidate-state receipt (`src/record.rs`)

A new durable sidecar, `candidate_state_receipt.json`, written the same way
`ProviderSessionReceipt`/`CommandBinding` are (`write_durable`: open, write,
`sync_data`):

```rust
pub struct CandidateStateReceipt {
    pub schema: u32,
    pub repository_id: o7_worktree::identity::CanonicalRepoId,
    pub base_commit: String,
    pub run_id: String,
    pub conversation_id: String,
    pub parent_run_id: Option<String>,
    pub candidate_tree_oid: String,
    pub patch_locator: String,   // "candidate.patch", alongside this file
    pub patch_sha256: String,
    pub patch_size: u64,
    pub patch_kind: String,      // "git-binary-cumulative-patch-v1"
}
```

Load-bearing identity (every field above except `schema`/`patch_locator`)
must be verified before a child may materialize from this receipt. The raw
cumulative patch itself lives alongside it as `candidate.patch` — a
SEPARATE file from R1's own `diff.patch` (which stays exactly what it was:
this run's own diff against its OWN `--base`, unrelated to conversation-wide
cumulative continuity).

`repository_id` reuses `o7_worktree::identity::CanonicalRepoId` (already
built, already Serialize/Deserialize, already exactly "the absolute,
symlink-resolved common git directory plus its filesystem identity" —
detects a later path-reuse-with-different-inode) rather than reinventing a
repo-identity primitive. `o7-worktree` becomes a new dependency of the root
`o7` crate for exactly this type plus `HardenedGit::canonical_repo_id()`;
its heavier `materialize`/`store`/`reap` machinery (built for a different
problem — materializing an already-committed revision under a strict
security preflight, not applying an out-of-band patch) is NOT reused here.

## 4. Capture algorithm (`src/worktree.rs`, extended)

In the run's own (already-isolated) worktree, once the agent has finished
and R1's own `diff_vs_base`/`PatchCaptured` already ran:

```
git add -A                                                  # already done by diff_vs_base
git diff --cached --binary --full-index --no-color \
  --no-ext-diff <conversation base_commit>                  # NOT this run's own --base
git write-tree                                              # -> candidate_tree_oid
```

`git write-tree` requires the index to reflect the working tree first —
`add -A` (already run by `diff_vs_base` immediately before this) covers new
files, modifications, deletions, and the executable bit; `--binary
--full-index` on the diff preserves binary blobs and renders new-file/delete
hunks as full content, not delta-only headers that would be meaningless
without the base blob; `--no-color --no-ext-diff` keep the patch bytes
portable and free of any repo-configured external diff driver. Symlinks are
preserved as Git already represents them (a blob mode `120000`) — no special
handling needed beyond what `add -A`/`diff --binary` already do.

Explicitly out of scope, and must fail closed rather than silently drop
content: ignored files (by design — `add -A` does not stage them), Git LFS
working-tree pointers vs. real content (captured as whatever `git diff`
sees, which is the pointer file unless LFS smudge already ran — not
specially handled), submodule mutations (a modified submodule pointer is
itself a valid diff hunk Git can represent, but a child touching the
INSIDE of a submodule's own working tree is not — the negative matrix
requires this fail closed, not silently ignored), and nested repositories
(`.git` inside a subdirectory) similarly.

## 5. Materialization algorithm and ordering (`src/main.rs`, `continue_run`/`continue_execute`)

Before the provider is ever invoked, a command-continuation child run must:

1. Look up the parent run (already done today, R1 §"exact step order" step 4)
   — additionally require it belong to the same conversation and be the
   conversation's true tail (unchanged R1 checks, §5).
2. Locate the parent's own flat record directory
   (`runs/<target>/<parent_run_id>/`) and replay-verify its canonical record
   in full (`o7_run::replay::verify_prefix`) — the SAME chain/digest/
   reducer/artifact check every other canonical-record consumer in this
   codebase uses, never a partial/ad-hoc check.
3. Require the verified state to carry exactly one `candidate_state`
   artifact (`RunState.candidate_state`) — its absence is a fail-closed
   negative case (§6), not a fallback to R1's old zero-carryover behavior.
4. Read `candidate_state_receipt.json` from the parent's directory, verify
   its digest matches the `candidate_state` artifact reference, and verify:
   `schema` supported; `repository_id` matches this repo's own freshly
   computed `CanonicalRepoId`; `conversation_id` matches; `run_id` matches
   the parent's own canonical run id; `base_commit` is a real, resolvable
   commit in this repository.
5. Copy the receipt's bytes into the CHILD's own run directory as
   `parent_candidate_receipt.json` (durable write, before `RunStarted` —
   same crash-window discipline as `ledger_binding.json`/`command_binding.json`).
6. Create a fresh worktree at the receipt's own `base_commit` (NOT the
   child's `--base` CLI flag) — `worktree::add(&repo, &receipt.base_commit,
   &wt, "o7/{run_id}")`, exactly R1's existing call, with the base
   substituted.
7. Apply the patch fail-closed: `git apply --index --binary
   candidate.patch` inside the fresh worktree. `--index` updates the Git
   index atomically with the working tree so the very next `git write-tree`
   reflects exactly what was applied — a conflicting, partial, or malformed
   patch makes `git apply` itself fail non-zero, which this step treats as
   a hard error (no partial-apply state is ever left as "materialized").
8. `git write-tree` → `actual_tree_oid`; compare byte-for-byte against the
   receipt's `candidate_tree_oid`. A mismatch is a hard error — the
   materialized state is not what the parent attested to, whether from a
   tampered patch, a tampered receipt, or (in principle) a Git version
   difference in patch application; this contract does not attempt to
   distinguish those, it only refuses to proceed.
9. Only once 1-8 all succeed: durably append `CandidateStateMaterialized`
   (§2) to the child's own canonical stream — inside `continue_execute`,
   immediately after `CommandBindingCaptured`, BEFORE `AgentStarted`.
10. Only then: append `AgentStarted` and invoke the provider — R1's existing
    ordering and its own `sync_data()`-before-spawn guarantee are
    unchanged; `AgentStarted` remains the sole durable dispatch boundary.

**Why steps 1-9 need no new redrive/lifecycle machinery.** Every failure
in steps 1-8 happens strictly BEFORE `AgentStarted` is ever appended. R1's
sixth round already established `dispatch_progress()` (`src/recovery.rs`)
returns `None` — meaning `ChildRecordState::ValidUnsealedPreDispatch`, safe
to redrive with a fresh id once the process is provably dead — for ANY
unsealed record that never reached `AgentStarted`, regardless of what else
it contains. A materialization failure therefore ALREADY produces a
canonical record R1's own existing classifier treats as safely,
automatically redrivable — no new `ChildRecordState` variant, no new HTTP
error code, no new command status. `continue_execute` reports the failure
as a plain `anyhow::Error`; `continue_run`'s existing post-`outcome`
handling (R1 §"third corrective round", `if !ledger_run_exists(...) {
mark_rejected() }` else leave `started`) is unchanged and already correct
here, since `attach_run` (and therefore `ledger_run_exists`) has already
happened by this point (right after `RunStarted`) — the command stays
`started`, safely pre-dispatch-redrivable via the EXISTING stuck-command
discovery/redrive path, with zero widening of the fail-closed-after-
dispatch contract R1's sixth round froze. A materialization failure NEVER
produces `ValidUnsealedDispatchAmbiguous`, `ValidSealed`, or a new terminal
command status — it is, and stays, indistinguishable from any other
pre-dispatch crash R1 already knows how to recover from.

This also means A0 adds **zero new HTTP error codes and zero new `o7d`
routes.rs redrive-decision logic** — `src/recovery.rs::classify_command_child`
and the redrive decision in `crates/o7d/src/routes.rs` are untouched. The
only `o7d`-adjacent change is read-only exposure of candidate-state fields
on the existing run-detail DTO (§8, via a new helper added to
`crates/o7d/src/canonical.rs` alongside, but never called by, the existing
redrive classifier) — additive, and nowhere near the redrive decision path.

## 6. Pre-provider failure semantics — the negative matrix

Every case below occurs strictly before `AgentStarted`, so — per §5 — every
one already resolves via R1's EXISTING pre-dispatch-redrive machinery. The
required proof for each is **provider invocation count == 0**, checked by a
process-level test using the real deterministic `claude` fixture.

Implemented and proven this round, ten process-level tests in
`tests/a0_candidate_state_e2e.rs` (each asserting provider invocation
count stays exactly as it was before the negative command, and the
command stays `started` — never rejected, never falsely completed):

1. `missing_candidate_receipt_never_invokes_the_provider` — parent has no
   candidate receipt at all (stands in for the "legacy sealed parent
   predates A0" case too — deliberately NOT a fallback to R1's old
   zero-carryover behavior; A0 makes a valid receipt mandatory for every
   command continuation once live).
2. `missing_patch_file_never_invokes_the_provider` — receipt present, the
   patch file it references is missing.
3. `tampered_candidate_receipt_never_invokes_the_provider` — the receipt's
   own `candidate_tree_oid` is altered post-hoc, so the tree materialization
   independently computes no longer matches it (this is also the proof for
   "receipt tampered" generally — any load-bearing field's corruption is
   caught the same way, by the same tree-OID/identity checks in §5 step 8).
4. `tampered_patch_content_never_invokes_the_provider` — the patch's own
   bytes are altered; its digest no longer matches the receipt.
5. `wrong_base_commit_never_invokes_the_provider` — the receipt's
   `base_commit` does not exist in this repository.
6. `wrong_repository_identity_never_invokes_the_provider` — the receipt's
   `repository_id` does not match this repository's own computed identity.
7. `wrong_conversation_id_never_invokes_the_provider` — the receipt's
   `conversation_id` does not match the command's own conversation.
8. `a_patch_apply_conflict_never_invokes_the_provider` — the patch does not
   apply cleanly against the fresh worktree at `base_commit` (a genuine
   `git apply` conflict, not merely a digest mismatch).
9. `two_concurrent_retries_against_a_failing_materialization_both_fail_closed`
   — two simultaneous same-key retries against a command whose
   materialization is failing converge on the SAME child run id; neither
   ever invokes the provider.
10. `a_same_key_retry_succeeds_once_the_materialization_cause_is_fixed` —
    fixing the underlying cause and retrying with the SAME idempotency key,
    after the staleness bound, redrives and completes normally — proving a
    pre-dispatch materialization failure is NOT permanently wedged.

**Not re-implemented as NEW A0 tests, deliberately**: "parent is not
sealed" and "parent is not the conversation's true tail" are unchanged,
pre-existing R1 checks enforced synchronously inside `create_command`'s
own ledger transaction, entirely before a child `RunId` is ever minted —
they never reach A0's own materialization code at all, and R1's own test
suite (`tests/r1_command_e2e.rs`, re-run unchanged and green this round)
already covers them.

Explicitly deferred to a future corrective round (disclosed, not hidden —
matching this project's standing discipline): duplicate candidate receipts
within one parent record; an unsupported/future receipt schema version;
a Git path-traversal payload inside the patch; a submodule-mutation patch;
a parent whose own canonical replay independently fails for a reason other
than a missing/tampered candidate receipt; the specific crash-window
matrix as individually triggered/killed sub-cases (§5's ordering already
makes every one of them resolve to the same pre-dispatch-safe outcome the
tests above already prove — the CI-realistic crash windows R1 itself
required real `SIGKILL` proof for were POST-dispatch ones; A0 introduces
no new post-dispatch window, so the proof burden is smaller here by
construction, but per-window explicit kill-and-observe tests are still
valuable future work); an externally-modified original repository
checkout mid-materialization.

## 7. Idempotency and concurrency

Unchanged from R1: same command + same idempotency key never invokes the
provider twice and always returns the authoritative child run id; a
concurrent same-key retry converges via R1's existing CAS/lock-loser
machinery. A0 adds no new idempotency surface — a materialization failure
is, from the redrive path's point of view, indistinguishable from any other
pre-`AgentStarted` failure R1 already redrives safely (§5).

## 8. Q-Deck projection (`crates/o7d`)

The existing run-detail endpoint (`GET /api/v1/runs/{run_id}`) gains
read-only fields, populated from the run's own canonical record when
present: candidate source run id, candidate tree OID, materialization
status (`materialized` / `failed: <reason>` / not applicable for a
non-continuation run), and — on failure — a stable failure code. No diff
viewer, no raw patch bytes exposed to the browser (`candidate.patch` stays
server-side, exactly like `diff.patch` already does).

## 9. Operator discovery (`o7 recover`)

**Not extended this round** — `o7 recover`'s existing `--repo`/`--runs-dir`
discovery reporting (R1 §11.5) is unchanged. The REST-facing projection
(§8) covers the immediate Q-Deck UI need; teaching the CLI's own
discovery report to ALSO summarize candidate-state materialization status
per stuck command (mirroring §8's fields, read-only, same discipline) is
explicitly deferred to a future round rather than rushed in alongside
everything else here.

## 10. Explicitly out of scope for this slice

No diff viewer UI. No patch-delta chains (§1 already rules this out by
design, not merely by omission). No pushing candidate state to a remote.
No automatic conflict resolution. No reuse of a live parent worktree. No
synthetic commits into the user's own branch. No change to R1's frozen
dispatch-boundary/ambiguous-outcome semantics. No Sandboy/`o7-worker`
changes. No executor qualification work. No Alpha A1 work of any kind.

## 11. Commit sequence (additive, no amend/rebase/squash/force-push)

1. `71800fc` `docs(q-deck): define A0 candidate-state continuity contract` —
   this file, frozen before any implementation.
2. `78ed788` `feat(run): capture canonical cumulative candidate state` —
   §2-4.
3. `5fe3950` `feat(root): materialize verified parent candidate state` — §5.
4. `458f34b` `test(q-deck): prove cumulative candidate-state continuity` —
   §6, the A→B→C E2E and the ten negative cases.
5. `ca590d0` `feat(o7d): expose candidate-state lineage to command
   children` — §8-9.
6. `docs(q-deck): record A0 evidence and limitations` — this file, final
   update with the results below (§12).

## 12. Evidence — this round's exact gate results

Exact new head after commit 6: see the PR body / `git log` (this section
intentionally doesn't pin its own commit's hash into itself). Base:
`main` at `eead3b775cf0d5d1ea567b6eb496a555637c1f95` (R1's own merge
commit, PR #90).

- `cargo fmt --check` — clean.
- `git diff --check` — clean.
- `cargo check -p o7 -p o7-run -p o7-ledger -p o7d` — clean.
- `cargo test -p o7-run` — 72 unit + 18 `replay_acceptance` = 90 passing.
- `cargo test -p o7-ledger --test commands` — 39/39 passing, unchanged.
- `cargo test -p o7 --test r1_command_e2e` — **37/37 passing, unchanged**
  — R1's own full suite re-proven against this round's restructuring
  (worktree creation for continuations moved from `continue_run` into
  `continue_execute`; every existing R1 test's parent run now also
  captures a real candidate-state receipt "for free," since capture is
  unconditional on both ledger-backed paths).
- `cargo test -p o7 --test a0_candidate_state_e2e` — **11/11 passing**:
  the A→B→C chain plus the ten negative cases (§6).
- `cargo test -p o7d` — every existing suite green, unchanged (`api`,
  `golden_transcript_rest`, `golden_transcript_sse`, `sse`,
  `verdict_fidelity`, unit tests).
- `cargo clippy --workspace --all-targets -- -D warnings` — clean, 0
  warnings, across the whole workspace.
- `cargo test --workspace --no-fail-fast` — **every crate in the
  workspace green, 0 failures**, including doc-tests, with the five
  environmental exclusions R1 already disclosed and carries forward
  unchanged (none of them touch any A0 code): `kill_after_commit_preserves_event`,
  `kill_before_commit_leaves_no_partial`
  (`crates/o7-ledger/tests/crash_durability.rs`, a pre-existing VPS hang
  reproduced against the pristine R0.7 base too),
  `a_blocking_fifo_target_fails_closed_within_a_bound`,
  `no_control_descriptor_leaks_to_a_concurrent_sibling`, and
  `a_live_launch_executes_the_sealed_target_not_a_swapped_source`
  (`crates/o7-worker/tests/sandboy_lifecycle.rs`, timing-sensitive under
  this VPS's contention, confirmed passing standalone) — `o7-worker`/
  `o7-sandbox-protocol` remain byte-for-byte untouched by A0.
- `cargo deny check advisories bans licenses sources` — fails to load the
  local advisory-db snapshot on this dev VPS (the same pre-existing
  `RUSTSEC-2026-0041`/`lz4_flex` `CVSS:4.0` parse error every prior R1
  round already disclosed) — **zero new external dependencies**: the
  only `Cargo.lock` change this round is adding `o7-worktree`, already an
  in-workspace crate, as a new dependency edge of the root `o7` package.
- `npm test` (`apps/q-deck`) — 45/45 pass, 8 files, unchanged (A0 does
  not touch the frontend).
- `npm run check` (`apps/q-deck`) — 0 errors, 0 warnings, 182 files,
  unchanged.

### Provider invocation counts (the load-bearing proof)

- `candidate_state_flows_through_a_b_c_chain`: exactly 3 invocations
  total (Run A, Command B, Command C), one each, ever.
- Every one of the ten negative tests: invocation count stays at exactly
  what it was before the negative command (1, from the real parent run)
  — the provider is NEVER invoked a second time for a command whose
  materialization fails.
- `two_concurrent_retries_against_a_failing_materialization_both_fail_closed`:
  invocation count stays at 1 across BOTH concurrent retries; both
  converge on the same child run id.
- `a_same_key_retry_succeeds_once_the_materialization_cause_is_fixed`:
  invocation count is 1 after the broken first attempt (provider never
  invoked), then 2 after the cause is fixed and a same-key retry
  redrives and completes — exactly one successful invocation, never more.

### What this round confirms about its own central design claim

Zero new `o7d` HTTP error codes, zero new `ChildRecordState`/command
lifecycle statuses, zero changes to `src/recovery.rs`'s redrive
classifier or `crates/o7d/src/routes.rs`'s redrive decision — every one
of the ten negative cases resolves through R1's existing pre-dispatch-
redrive machinery exactly as §5 predicted before any test was written.
