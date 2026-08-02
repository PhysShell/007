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

**`ArtifactKind::CandidateState`** (9th variant) — a candidate-state
receipt JSON artifact. The SAME kind is used both when a run CAPTURES its own
candidate state and when a child COPIES a parent's receipt into its own
record to MATERIALIZE from it — both are "a candidate-state receipt",
just captured vs. consumed.

**`ArtifactKind::CandidatePatch`** (10th variant, added corrective round 1) —
the raw cumulative patch bytes, a DISTINCT kind from `CandidateState` so a
consumer can never mistake a patch artifact reference for a receipt
reference or vice versa. Used both for a run's own `candidate.patch` and for
a child's local copy `parent_candidate.patch`.

**`RunEventKind::CandidateStateCaptured { receipt: ArtifactRef }`** — emitted
by the run that JUST finished (both `execute_live` and `continue_execute`,
the two ledger-backed paths — never the bare, no-`--ledger` `execute`, which
can never become a parent), immediately after `PatchCaptured`, before the
gate loop. `RunState.candidate_state: Option<ArtifactRef>` holds it.

**`RunEventKind::CandidateStateMaterialized { source_run_id: RunId,
source_receipt: ArtifactRef, source_patch: ArtifactRef, materialized_tree_oid:
String }`** (reshaped corrective round 1 — see §14; this event was never
merged before the reshape, so there is no backward-compatibility burden)
— emitted by a command-continuation CHILD, in `continue_execute`,
immediately after `CommandBindingCaptured` and BEFORE `AgentStarted` (the
durable dispatch boundary — §5). `source_receipt`/`source_patch` reference
the CHILD's OWN local copies of the parent's receipt AND patch bytes
(`parent_candidate_receipt.json`, `parent_candidate.patch`, both copied into
the child's own run directory), so replaying the child alone — without the
parent's directory still existing — is fully self-contained, exactly like
`RunStarted.task` is a self-contained copy of the command text, not a live
reference to something external. `RunState.candidate_materialized:
Option<CandidateMaterialization>` (same four fields) holds it.

The OLD shape carried a second field pair on this same event,
`expected_tree_oid`/`actual_tree_oid`, and the pure reducer refused any
stream where they disagreed. That was a **vacuous check** (P8, §14): both
values were written by the same caller from variables that always agreed by
construction, so the "proof" never bound the tree OID against anything
independent — a tampered writer could always make its own two fields agree
with each other. The reshaped event carries only ONE tree OID
(`materialized_tree_oid`), and proving it against something independent — the
copied source receipt's own `candidate_tree_oid`, resolved and digest-verified
from a SEPARATE artifact — is now the job of the semantic layer (§14,
`o7_run::candidate::verify_candidate_state_materialized`), not the pure
reducer, which has no artifact resolver to check against.

Both events participate in the SAME digest-chain framing every other event
does (`fold_kind`/`tag()`/`name()`); nothing about the chain or replay
primitive itself changes.

## 3. The candidate-state receipt (`crates/o7-run/src/candidate.rs`, moved corrective round 1)

The receipt's typed schema now lives in `o7-run` — the same lower,
canonical/evidence crate the writer, the materializer, the reducer/replay
semantic verifier, tests, and DTO projection all share as ONE type, instead
of the root crate privately owning a shape only it could interpret:

```rust
pub const CANDIDATE_STATE_RECEIPT_SCHEMA_V1: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CandidateStateReceiptV1 {
    pub schema: u32,
    pub repository_id: RepositoryIdentity,
    pub base_commit: String,
    pub run_id: RunId,
    pub conversation_id: String,
    #[serde(default)]
    pub parent_run_id: Option<RunId>,
    pub candidate_tree_oid: String,
    pub patch_kind: CandidatePatchKind,
    pub patch: ArtifactRef,
}
```

`#[serde(deny_unknown_fields)]`: a receipt with an extra field is a foreign
or corrupted record, rejected at parse time rather than silently accepted
with the extra field ignored. `patch` is a canonical `ArtifactRef`
(`kind: ArtifactKind::CandidatePatch`, locator `candidate.patch`, digest,
size) — never a bag of decorative strings (`patch_locator`/`patch_sha256`/
`patch_size` as three independent fields) a consumer could forget to
cross-check against each other. `patch_kind` is the typed
`CandidatePatchKind` enum (currently one variant,
`GitBinaryCumulativePatchV1`, `#[serde(rename_all = "snake_case")]`, no
catch-all `#[serde(other)]` — an unrecognized kind fails closed at
deserialization, not at some later ad-hoc string comparison).

`repository_id` is now `o7_run::event::RepositoryIdentity` — a small,
dependency-free mirror of `o7_worktree::identity::CanonicalRepoId` (same
fields: the absolute, symlink-resolved common git directory plus its
filesystem `dev`/`ino` identity), defined directly in `o7-run` rather than
pulling `o7-worktree` in as a dependency of a crate whose own design
doctrine is "dependency-light, recomputable anywhere" (`serde`/`sha2`/
`thiserror` only). The root `o7` crate remains the ONE place the conversion
happens (`repository_identity()` in `src/main.rs`, calling
`HardenedGit::canonical_repo_id()` and copying its fields across) — `o7-run`
itself never depends on `o7-worktree`, avoiding a dependency-cycle risk and
keeping the canonical schema recomputable by any tool that only has
`o7-run` and the artifact bytes, without needing the heavier worktree crate
at all.

**Load-bearing identity and the contract authority it is bound against**
(§14, Part 2): a receipt's `conversation_id`/`repository_id`/`base_commit`/
`patch_kind` are cross-checked, at every point of use, against the run's OWN
`RunContract.candidate_state: Option<CandidateStateContractV1>` obligation —
not merely internal self-consistency. A new top-level `o7 run --ledger`
creates this obligation from its own freshly resolved `--base`/repository
before `RunStarted`. A continuation child INHERITS it EXACTLY from the
verified parent's own contract (`resolve_inherited_candidate_obligation`) —
it is never re-derived from the child's own `--base` CLI flag (which stays
purely decorative for a continuation once a parent candidate receipt
exists) and never taken as a bare self-claim from the receipt itself. This
is the fix for a receipt whose fields are all internally consistent but
whose CONTENT was never actually checked against any authority outside
itself.

The raw cumulative patch itself lives alongside the receipt as
`candidate.patch` — a SEPARATE file from R1's own `diff.patch` (which stays
exactly what it was: this run's own diff against its OWN `--base`, unrelated
to conversation-wide cumulative continuity), and referenced by the
receipt's own `patch: ArtifactRef` field rather than a raw filename string.

## 4. Capture algorithm (`src/worktree.rs`, extended; byte-preserving corrective round 1)

In the run's own (already-isolated) worktree, once the agent has finished
and R1's own `diff_vs_base`/`PatchCaptured` already ran:

```
git add -A                                                  # already done by diff_vs_base
git diff --cached --binary --full-index --no-color \
  --no-ext-diff <conversation base_commit>                  # NOT this run's own --base
git write-tree                                              # -> candidate_tree_oid
```

`capture_cumulative_candidate(worktree, base_commit) -> Result<(Vec<u8>,
String)>` returns the diff's stdout as **raw `Vec<u8>` bytes end to end** —
never through a `String` at any point in the transport path. Git's own diff
output for an arbitrary repository is not guaranteed valid UTF-8 (a tracked
file with invalid-UTF-8 content, a binary blob, a non-UTF-8 filename on
Unix), and a lossy or lossless text round-trip anywhere in this path would
silently corrupt exactly the bytes the whole A0 model exists to preserve
byte-for-byte. `git write-tree` requires the index to reflect the working
tree first — `add -A` (already run by `diff_vs_base` immediately before
this) covers new files, modifications, deletions, and the executable bit;
`--binary --full-index` on the diff preserves binary blobs and renders
new-file/delete hunks as full content, not delta-only headers that would be
meaningless without the base blob; `--no-color --no-ext-diff` keep the
patch bytes portable and free of any repo-configured external diff driver.
Symlinks are preserved as Git already represents them (a blob mode
`120000`) — no special handling needed beyond what `add -A`/`diff --binary`
already do.

The ONLY place patch bytes are ever read as text is a heuristic,
after-the-fact scan for a gitlink (submodule, mode `160000`) mutation
(`patch_touches_gitlink`, §7) — a `String::from_utf8_lossy` used purely to
grep for Git's own extended-header lines; it never replaces, mutates, or
re-derives the actual stored patch bytes, which stay exactly what `git
diff` produced. Git's own stdout/stderr may still be formatted lossily for
DIAGNOSTICS after a git invocation fails (`run_git`'s error path) — that is
never the patch transport itself.

Explicitly out of scope, and must fail closed rather than silently drop
content: ignored files (by design — `add -A` does not stage them), Git LFS
working-tree pointers vs. real content (captured as whatever `git diff`
sees, which is the pointer file unless LFS smudge already ran — not
specially handled), gitlink/submodule mutations (§7 — checked deterministically
at capture, both against the patch text and the resulting tree), and nested
repositories (`.git` inside a subdirectory) similarly.

## 5. Materialization algorithm and ordering (`src/main.rs`, `continue_run`/`continue_execute`; hardened corrective round 1)

This round split what used to be one step ("look up and verify the parent")
into an EARLY, pre-`RunStarted` contract-inheritance step and a LATE,
post-`attach_run` full-materialization step — see §14, Part 6, for why the
split does not weaken any R1 redrive guarantee. Both steps use
`o7_run::candidate::verify_candidate_state_captured` (§14, Part 4) for the
parent receipt's own semantic checks, instead of the ad-hoc field
comparisons the original round hand-rolled.

**Early, in `continue_run`, before `RunStarted`/`attach_run`:**

1. Look up the parent run (already done today, R1 §"exact step order" step 4)
   — additionally require it belong to the same conversation and be the
   conversation's true tail (unchanged R1 checks, §5).
2. `resolve_inherited_candidate_obligation`: locate the parent's own flat
   record directory, replay-verify its canonical record in full
   (`o7_run::replay::verify_prefix`), semantically verify its own captured
   receipt (`verify_candidate_state_captured` — schema, contract
   cross-binding, nested patch artifact digest), cross-check
   `conversation_id`/`run_id` against this command's own binding, and copy
   the verified receipt's own `conversation_id`/`repository_id`/
   `base_commit`/`patch_kind` into the CHILD's own
   `CandidateStateContractV1` obligation — inherited, never re-derived from
   the child's own `--base` CLI flag or from the receipt's bare self-claim.
   A failure here leaves NO run directory ever created for the child — the
   same fail-closed shape as the pre-existing "re-validate the parent's
   provider session" check this same caller already performs at the same
   point, and already classified `Absent`/safely redrivable by R1's own
   discovery report (§5's "why steps need no new redrive machinery",
   unchanged).

**Late, in `continue_execute`, after `attach_run`, before `AgentStarted`
(`materialize_parent_candidate_state`):**

3. Re-resolve the parent's directory and call
   `load_verified_candidate_receipt` again — full `verify_prefix` PLUS
   `verify_candidate_state_captured` — this time additionally requiring the
   parent to be **genuinely terminal, not merely a valid prefix that
   happens to carry a receipt**: the verified state's `verdict` must be
   `Some` (sealed), and the record's own last raw event line must be a
   `run_sealed` event. A parent truncated right after
   `CandidateStateCaptured` but never actually sealed — or one whose
   `RunSealed` event was stripped after the fact — is refused here even
   though step 2 already passed for it; the point-of-use check re-verifies
   the parent is STILL the authoritative true tail, not a stale
   pre-lock snapshot.

   *(This corrects an overclaim in the previous round of this document:
   "replay-verify its canonical record in full... never a partial/ad-hoc
   check" described `verify_prefix` alone as sufficient proof. It is
   necessary but not sufficient — `verify_prefix` proves the STREAM's
   internal chain/digest/artifact integrity; it has no opinion on whether
   the receipt's CONTENT means anything or whether the record is actually
   sealed. Full acceptance authority requires the semantic layer on top,
   which is what this step now runs.)*

   *(Corrective round 2 update: as of this round, `o7_run::replay::
   verify_prefix` ITSELF now runs the semantic layer automatically
   whenever candidate evidence is present — see §16, Part 1. The
   explicit separate call to `verify_candidate_state_captured` this
   step (and step 2 above) still makes is no longer strictly necessary
   for VERIFICATION — `verify_prefix` alone now already proves it — but
   is still needed here for DATA EXTRACTION: `verify_prefix` returns
   only the reduced `RunState`, never the parsed
   `CandidateStateReceiptV1` itself, which this caller needs to read
   `base_commit`/`patch.locator`/etc. from. This is a deliberate,
   harmless double-check kept for that reason, not a residual gap.)*

4. Read the receipt's own patch bytes (`std::fs::read` against the
   receipt's own already-digest-verified `patch.locator` — safe, since
   `verify_candidate_state_captured` already proved the digest matches).
   Copy BOTH the receipt bytes (`parent_candidate_receipt.json`) AND the
   patch bytes (`parent_candidate.patch`, new this round) into the CHILD's
   own run directory (durable writes, before `RunStarted` — same
   crash-window discipline as `ledger_binding.json`/`command_binding.json`).
   Copying the patch too (not just the receipt) is what makes the child's
   own materialized record fully self-contained — deleting the parent's run
   directory after sealing cannot break semantic replay of the child alone.
5. Create a fresh worktree at the receipt's own `base_commit` (NOT the
   child's `--base` CLI flag) — `worktree::add(&repo, &receipt.base_commit,
   &wt, "o7/{run_id}")`, exactly R1's existing call, with the base
   substituted.
6. Apply the patch fail-closed via `worktree::apply_candidate_patch(runs_dir,
   worktree, &patch_bytes)` (§14, Part 1) — bytes in, never a `String`. The
   patch input is written to a **private, confined temporary file OUTSIDE
   the candidate-controlled worktree**: `<runs_dir>/.o7-candidate-tmp/`, a
   sibling of every run/worktree directory, never inside any checkout a
   base commit or a candidate patch could have staged content into. Per
   write: a fresh directory-fd opened `O_DIRECTORY | O_NOFOLLOW`; a unique
   filename (`apply-input.<pid>.<counter>`); `openat` with
   `O_WRONLY | O_CREATE | O_EXCL | O_NOFOLLOW | O_CLOEXEC`, mode `0600`;
   `write_all` then `sync_all` before `git apply` ever runs; the temp file
   is unlinked on BOTH the success and the failure path; the path itself is
   passed to `Command::arg` (never interpolated into a shell string).
   `git apply --index --binary <private tmp path>` — `--index` updates the
   Git index atomically with the working tree so the very next
   `git write-tree` reflects exactly what was applied; a conflicting,
   partial, or malformed patch makes `git apply` itself fail non-zero,
   which this step treats as a hard error (no partial-apply state is ever
   left as "materialized"). An empty cumulative patch (a run that changed
   nothing relative to base) is a legitimate, special-cased no-op — `git
   apply` itself errors on empty input, so this case skips straight to
   `write-tree` on the freshly checked-out tree.

   **This closes P6** (§14): the ORIGINAL A0 round's own fix for a
   stdin-pipe deadlock introduced a fixed-name temp file,
   `.o7-candidate-patch.tmp`, written via plain `std::fs::write` INSIDE the
   child's own worktree — a worktree checked out at `receipt.base_commit`,
   a value the candidate patch chain does not control, but whose CONTENT
   (the base commit's own tree) can. A base tree containing a symlink at
   that exact fixed name, pointing anywhere on the filesystem, made that
   write follow the symlink and overwrite an arbitrary outside file with
   attacker-influenceable patch bytes — reachable BEFORE any provider
   invocation, entirely inside trusted plumbing. The private, no-follow,
   `O_EXCL` temp store outside the checkout closes this: no name the
   candidate's own tree contains can ever collide with, or redirect, this
   write, because the write never happens inside the checkout at all.
7. `git write-tree` → `materialized_tree_oid`, and verify no gitlink entry
   resulted (§7). Compare byte-for-byte against the receipt's
   `candidate_tree_oid`. A mismatch is a hard error — the materialized
   state is not what the parent attested to, whether from a tampered
   patch, a tampered receipt, or (in principle) a Git version difference in
   patch application; this contract does not attempt to distinguish those,
   it only refuses to proceed.
8. Only once 1-7 all succeed: durably append `CandidateStateMaterialized`
   (§2) to the child's own canonical stream — inside `continue_execute`,
   immediately after `CommandBindingCaptured`, BEFORE `AgentStarted`.
9. Only then: append `AgentStarted` and invoke the provider — R1's existing
   ordering and its own `sync_data()`-before-spawn guarantee are
   unchanged; `AgentStarted` remains the sole durable dispatch boundary.

**Structural ordering, enforced by the pure reducer** (§14, Part 5, new this
round): `RunStarted → CommandBindingCaptured → CandidateStateMaterialized →
AgentStarted → AgentExited → PatchCaptured → CandidateStateCaptured → ... →
RunSealed`. Materialization requires a prior command binding; is forbidden
after `AgentStarted`; `AgentStarted` for a command-child under a candidate
obligation is forbidden without prior materialization; duplicate
materialization is forbidden; an initial (non-continuation) run cannot carry
materialization at all. `CandidateStateCaptured` requires a prior
`PatchCaptured` and a terminal agent outcome. An undischarged candidate-state
obligation at `RunSealed` is a `Blocked` verdict contribution — the same
bucket an unmet gate/policy/sandbox requirement already falls into — not a
structural `ReduceError`. A legacy/pre-A0 contract carrying no obligation at
all stays fully replay-compatible, unchanged.

**Why the early/late split (§14, Part 6) needs no new redrive/lifecycle
machinery.** The EARLY step (`resolve_inherited_candidate_obligation`,
before `RunStarted`/`attach_run`) can fail with NO run directory ever
created for the child at all — already classified `Absent`, safely
redrivable, by R1's own discovery report; this is not a new failure class,
it is the SAME shape as the pre-existing "re-validate the parent's provider
session" check performed at the identical point. The LATE step
(`materialize_parent_candidate_state`, steps 3-7 above) happens strictly
BEFORE `AgentStarted` is ever appended, exactly as the original round
established: R1's sixth round already established `dispatch_progress()`
(`src/recovery.rs`) returns `None` — meaning
`ChildRecordState::ValidUnsealedPreDispatch`, safe to redrive with a fresh
id once the process is provably dead — for ANY unsealed record that never
reached `AgentStarted`, regardless of what else it contains. A late
materialization failure therefore ALREADY produces a canonical record R1's
own existing classifier treats as safely, automatically redrivable — no new
`ChildRecordState` variant, no new HTTP error code, no new command status.
`continue_execute` reports the failure as a plain `anyhow::Error`;
`continue_run`'s existing post-`outcome` handling (R1 §"third corrective
round", `if !ledger_run_exists(...) { mark_rejected() } else leave
started`) is unchanged and already correct here, since `attach_run` (and
therefore `ledger_run_exists`) has already happened by this point — the
command stays `started`, safely pre-dispatch-redrivable via the EXISTING
stuck-command discovery/redrive path, with zero widening of the
fail-closed-after-dispatch contract R1's sixth round froze. Neither failure
class EVER produces `ValidUnsealedDispatchAmbiguous`, `ValidSealed`, or a
new terminal command status — both are, and stay, indistinguishable from
any other pre-dispatch crash R1 already knows how to recover from.

This also means A0 adds **zero new HTTP error codes and zero new `o7d`
routes.rs redrive-decision logic** — `src/recovery.rs::classify_command_child`
and the redrive decision in `crates/o7d/src/routes.rs` are untouched. The
only `o7d`-adjacent change is read-only exposure of candidate-state fields
on the existing run-detail DTO (§9, via a new helper added to
`crates/o7d/src/canonical.rs` alongside, but never called by, the existing
redrive classifier) — additive, and nowhere near the redrive decision path.

## 6. Pre-provider failure semantics — the negative matrix

Every case below occurs strictly before `AgentStarted`, so — per §5 — every
one already resolves via R1's EXISTING pre-dispatch-redrive machinery. The
required proof for each is **provider invocation count == 0**, checked by a
process-level test using the real deterministic `claude` fixture. Since
corrective round 1 split verification into an early (pre-`RunStarted`) and
late (post-`attach_run`) step (§5), the child run_id for several of these
cases is now never bound to a ledger row at all — the test helper for "the
command never completes" accepts EITHER a `404` (no ledger row ever
created) or a `200` with a non-`completed` status as equally valid proof;
which one a given case hits depends on whether its defect is reachable
before or only after `attach_run`, not on any weakening of the guarantee
itself.

Implemented and proven this round, eleven process-level tests in
`tests/a0_candidate_state_e2e.rs` (each asserting provider invocation
count stays exactly as it was before the negative command, and the
command never completes — never rejected, never falsely completed):

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
   caught the same way, by the same tree-OID/identity checks in §5 step 7).
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
11. `a_symlink_at_the_old_temp_files_exact_name_never_escapes_the_confined_temp_store`
    (corrective round 1, the **P6 regression proof**) — the fixture repo's
    OWN base commit contains a real symlink at the exact old fixed name,
    `.o7-candidate-patch.tmp`, pointing OUTSIDE the repository at a sentinel
    file. A real Run A and a real command continuation, both with a
    non-empty cumulative patch, run to completion: the provider is invoked
    exactly twice (once per run, the hostile symlink notwithstanding), the
    external sentinel's bytes are byte-for-byte unchanged, and an
    independent scratch checkout of the materialized tree shows the
    symlink itself survives, intact, still pointing at the same sentinel —
    never followed, never replaced, never deleted. Proves the private,
    outside-the-checkout temp store (§5 step 6) actually closes the escape
    the original fixed-name in-worktree temp file created.

Additionally, thirteen unit tests in `crates/o7-run/tests/candidate_state.rs`
(no process/provider involved — pure reducer and semantic-layer coverage,
faster and more exhaustive than a process-level test can afford to be for
this class of defect) prove the structural-ordering rules of §5 and the
**P8 semantic-verification fix** directly: a synthetic
`CandidateStateMaterialized` event whose `materialized_tree_oid` disagrees
with its own copied source receipt's `candidate_tree_oid` passes generic
`verify_prefix` (proving the generic chain/digest layer has no opinion on
receipt content) but fails
`o7_run::candidate::verify_candidate_state_materialized` with a message
naming the disagreement explicitly — the direct proof that a synthetic
record with receipt tree `X` and event tree `Y` (`X != Y`) fails semantic
replay. Also covered: an unsupported receipt schema, an unknown JSON field
(`#[serde(deny_unknown_fields)]`), and every structural-ordering negative
from §5 (materialization without/after command binding, `AgentStarted`
before materialization, duplicate materialization, capture before patch or
before agent-terminal, a pre-A0 legacy contract staying replay-compatible).

**Not re-implemented as NEW A0 tests, deliberately**: "parent is not
sealed" and "parent is not the conversation's true tail" are unchanged,
pre-existing R1 checks enforced synchronously inside `create_command`'s
own ledger transaction, entirely before a child `RunId` is ever minted —
they never reach A0's own materialization code at all, and R1's own test
suite (`tests/r1_command_e2e.rs`, re-run unchanged and green this round)
already covers them.

**Closed this round, no longer deferred**: an unsupported/future receipt
schema version now fails closed (unit-tested,
`a_receipt_with_an_unsupported_schema_fails_semantic_verification`); a
submodule-mutation patch now fails closed (§7, unit-level gitlink-detection
coverage in `worktree.rs`, though not yet a dedicated process-level E2E
test — see below).

Explicitly deferred to a future corrective round (disclosed, not hidden —
matching this project's standing discipline): duplicate candidate receipts
within one parent record; a dedicated PROCESS-level (not merely unit-level)
test for a Git path-traversal payload inside a receipt/patch locator, and
for a submodule-mutation patch reaching a real command continuation; a
parent whose own canonical replay independently fails for a reason other
than a missing/tampered candidate receipt; a dedicated process-level
non-UTF-8/binary-content A→B→C chain test (byte-preservation is proven at
the unit level and by construction of the `Vec<u8>` transport, §4, but not
yet exercised end-to-end through a real command continuation with
non-UTF-8 fixture content); the specific crash-window matrix as
individually triggered/killed sub-cases (§5's ordering already makes every
one of them resolve to the same pre-dispatch-safe outcome the tests above
already prove — the CI-realistic crash windows R1 itself required real
`SIGKILL` proof for were POST-dispatch ones; A0 introduces no new
post-dispatch window, so the proof burden is smaller here by construction,
but per-window explicit kill-and-observe tests are still valuable future
work); an externally-modified original repository checkout mid-
materialization; background GC of stale private temp-store residue (§5
step 6's temp file is unlinked on every completion/failure path this round
already exercises, but a crash between `openat`/write and the following
`unlinkat` — e.g. `SIGKILL` mid-write — can still leave a residual file in
`<runs_dir>/.o7-candidate-tmp/`; it is inert, mode `0600`, owned by the
`o7` process's own user, confined to the private store, and never read
back by anything — but no periodic sweeper reclaims it yet).

## 7. Git construct policy (new, corrective round 1)

What A0's cumulative-patch model supports and what it must fail closed on,
made explicit rather than left to be inferred from scattered capture/
materialization prose:

**Supported and tested** (§6): regular text files; arbitrary
non-UTF-8 file contents; binary files; new files; deletion; the executable
bit; symlinks as ordinary repository entries (mode `120000`); an empty
cumulative patch (a run that changed nothing relative to base).

**Explicitly rejected, fail closed**: any gitlink (submodule, mode `160000`)
mutation — introducing one, changing one, or removing one. Checked
deterministically at TWO points, both via `git ls-tree -r <commit>` scanning
for a `160000 commit ` entry (`worktree::tree_has_gitlink`): once on the
resulting tree right after capture (`capture_cumulative_candidate`), and
again on the resulting tree right after materialization (`finish_apply`).
Capture additionally runs a heuristic text scan of the patch itself
(`patch_touches_gitlink`) for Git's own extended-header lines
(`new mode`/`old mode`/`new file mode`/`deleted file mode` alongside
`160000`), catching a gitlink mutation before even computing the tree —
belt-and-suspenders with the tree-level check, not a replacement for it. A
patch touching `.git` itself, an unsafe/traversal patch path, an
unsupported patch format, or an unresolved index conflict all surface as a
plain `git apply` failure (non-zero exit), the same hard-error path §5 step
6 already treats as fail-closed.

An UNCHANGED, pre-existing gitlink already present in `base_commit` and
never touched by a candidate patch is not rejected — only a candidate's own
patch introducing, mutating, or deleting one is. This round does not add a
dedicated process-level E2E test for a provider that mutates a gitlink
mid-run (the unit-level detection in `worktree.rs` is exercised directly,
not yet through a full command-continuation fixture) — disclosed as
deferred, §6.

Never executed, by design, at any point in this pipeline: Git hooks,
checkout filters (clean/smudge), arbitrary executables, or any
user-provided Git argument — every `git` invocation in `worktree.rs` uses a
fixed, hardcoded argument list; no part of a candidate patch's own content
ever becomes a shell command or a `git` CLI flag.

## 8. Idempotency and concurrency

Unchanged from R1: same command + same idempotency key never invokes the
provider twice and always returns the authoritative child run id; a
concurrent same-key retry converges via R1's existing CAS/lock-loser
machinery. A0 adds no new idempotency surface — a materialization failure
is, from the redrive path's point of view, indistinguishable from any other
pre-`AgentStarted` failure R1 already redrives safely (§5).

## 9. Q-Deck projection (`crates/o7d`)

The existing run-detail endpoint (`GET /api/v1/runs/{run_id}`) gains
read-only fields, populated from the run's own canonical record when
present: candidate source run id, candidate tree OID, materialization
status (`materialized` / `failed: <reason>` / not applicable for a
non-continuation run), and — on failure — a stable failure code. No diff
viewer, no raw patch bytes exposed to the browser (`candidate.patch` stays
server-side, exactly like `diff.patch` already does).

## 10. Operator discovery (`o7 recover`)

**Not extended this round** — `o7 recover`'s existing `--repo`/`--runs-dir`
discovery reporting (R1 §11.5) is unchanged. The REST-facing projection
(§9) covers the immediate Q-Deck UI need; teaching the CLI's own
discovery report to ALSO summarize candidate-state materialization status
per stuck command (mirroring §9's fields, read-only, same discipline) is
explicitly deferred to a future round rather than rushed in alongside
everything else here.

## 11. Explicitly out of scope for this slice

No diff viewer UI. No patch-delta chains (§1 already rules this out by
design, not merely by omission). No pushing candidate state to a remote.
No automatic conflict resolution. No reuse of a live parent worktree. No
synthetic commits into the user's own branch. No change to R1's frozen
dispatch-boundary/ambiguous-outcome semantics. No Sandboy/`o7-worker`
changes. No executor qualification work. No Alpha A1 work of any kind.

## 12. Commit sequence (additive, no amend/rebase/squash/force-push)

1. `71800fc` `docs(q-deck): define A0 candidate-state continuity contract` —
   this file, frozen before any implementation.
2. `78ed788` `feat(run): capture canonical cumulative candidate state` —
   §2-4.
3. `5fe3950` `feat(root): materialize verified parent candidate state` — §5.
4. `458f34b` `test(q-deck): prove cumulative candidate-state continuity` —
   §6, the A→B→C E2E and the ten negative cases.
5. `ca590d0` `feat(o7d): expose candidate-state lineage to command
   children` — §9-10.
6. `92a9879` `docs(q-deck): record A0 evidence and limitations` — this
   file, final update for the original round (§13).

**Corrective round 1** (`docs/q-deck/a0-candidate-state.md` §14-15;
additive on top of `92a9879`, same discipline — no amend/rebase/squash/
force-push):

7. `918c2e8` `refactor(run): define typed candidate-state evidence` —
   §14 Part 2-3: `CandidateStateReceiptV1`, `RepositoryIdentity`,
   `CandidatePatchKind`, `CandidateStateContractV1`, the reshaped
   `CandidateStateMaterialized` event, the semantic-verification layer
   (`crates/o7-run/src/candidate.rs`), structural-ordering `ReduceError`
   variants, and 13 new unit tests.
8. `7d51208` `fix(worktree): confine byte-preserving patch application` —
   §14 Part 1: `Vec<u8>` capture/apply, the private no-follow `O_EXCL` temp
   store, gitlink detection (§7).
9. `828f364` `fix(root): require sealed authoritative parent candidates` —
   §14 Part 4-6: the root crate wired to the typed schema and semantic
   verification; the early/late parent-verification split; the
   self-contained child record (copied receipt AND patch).
10. `33f6b89` `test(q-deck): cover A0 confinement and replay adversaries` —
    §14 Part 8: the P6 symlink-escape process-level regression test, and
    the relaxed "never completes" assertion helper.
11. `7014bcc` `docs(q-deck): record the first corrective-round evidence` —
    this file, §14-15, final update with that round's results.

**Corrective round 2** (§16-17; additive on top of `7014bcc`, same
discipline):

12. `7abd4f1` `fix(replay): make candidate semantics authoritative` — §16
    Part 1-2: `verify_prefix_core`/`verify_prefix` split; candidate
    semantic verification now runs automatically inside `verify_prefix`;
    wider cross-binding against a run's own `CommandBindingCaptured`
    evidence (new `CommandBindingFacts`).
13. `25ebf61` `fix(o7d): reject parents without usable candidate state` —
    §16 Part 5: `parent_candidate_state_usable` admission preflight; the
    new `COMMAND_PARENT_CANDIDATE_UNAVAILABLE` 409; the idempotency-key
    peek that keeps replays exempt from the new preflight.
14. `c437f55` `fix(root): terminate unattached legacy-parent commands` —
    §16 Part 6: the atomic `mark_command_rejected_if_unattached_and_bound`;
    `continue_run`'s early-failure path now actually invokes it.
15. `3af641f` `fix(o7d): correct candidate lineage projection` — the
    always-null DTO field-name defect fix, in its own narrow commit.
16. `9f8f63e` `test(q-deck): cover semantic replay and legacy-parent
    admission` — §16 Part 4/7/8: the restructured P8 proof, ten new
    cross-binding negatives, three new legacy-parent-admission process
    tests, one sealed-non-Pass-verdict-still-usable test.
17. `docs(q-deck): record the second corrective-round evidence` — this
    file, §16-17, final update with this round's results.

## 13. Evidence — original A0 round's exact gate results (historical, unchanged)

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

## 14. Corrective round 1: confined byte transport and semantic replay

An independent re-gate against this PR's exact original head
(`92a98796dec0d108f4b1ea66d784789ef2906ada`) found two real defects and a
related cluster of narrower ones. Both defects survived the original
round's own review because the round proved the CHAIN was tamper-evident
without proving the chain's CONTENT meant anything, and reused a plumbing
fix (a temp file) without re-examining what controlled the filesystem
namespace it was written into. Neither is a rebuild of the frozen A0
model (§1) — the cumulative-patch-relative-to-one-immutable-base design
and the pre-dispatch materialization ordering already stood; this round
closes the trust boundary underneath them.

**P6 — symlink escape via a fixed-name temp file (`src/worktree.rs`).**
The original round's OWN fix for a `git apply` stdin-pipe deadlock wrote
the candidate patch to `<child-worktree>/.o7-candidate-patch.tmp` via
plain `std::fs::write` — a fixed name, inside a worktree checked out at
`receipt.base_commit`, which a base commit's own tree fully controls. A
base tree containing a symlink at that exact name, pointing anywhere on
the filesystem, turned this trusted-plumbing write into an arbitrary
outside-file overwrite with attacker-influenceable patch bytes — reachable
BEFORE any provider invocation, entirely inside code the original review
had reason to trust. Fixed by removing the in-worktree temp file entirely
and writing to a private, per-`runs_dir` confined store instead (§5 step
6, §13 Part 1) — no name a candidate's own tree contains can ever collide
with or redirect this write again, because the write no longer happens
inside any checkout. Proven by a real process-level regression test that
bakes the exact old symlink into a fixture repo's base commit and drives a
real command continuation through it (§6 item 11).

**P8 — vacuous semantic proof (`crates/o7-run/src/{event,reduce}.rs`).**
Canonical replay (`verify_prefix`) proved a `CandidateStateMaterialized`
event's own artifact reference resolved and its digest matched — but the
event's OLD shape carried both `expected_tree_oid` and `actual_tree_oid` as
two fields on the SAME event, written by the SAME caller from variables
that agreed by construction. A tampered or malicious writer could always
make its own two self-declared fields agree with each other; nothing in
the chain ever bound the claim against a second, independent source. Fixed
by reshaping the event to carry ONE tree OID
(`materialized_tree_oid`) and moving the actual proof — comparing it
against the copied SOURCE receipt's own `candidate_tree_oid`, independently
resolved and digest-verified from a separate artifact — into a new
semantic-verification layer (`o7_run::candidate`) built ON TOP of, not
inside, the generic replay machinery (the pure reducer has no artifact
resolver to check against; acceptance authority now lives in full replay,
matching the spec's own architectural requirement). Proven by a unit test
that constructs a receipt with tree `X` and an event with tree `Y != X`,
shows generic `verify_prefix` has no opinion on it (passes), and shows
`verify_candidate_state_materialized` fails with a message naming the
disagreement (§6, the thirteen new `crates/o7-run/tests/candidate_state.rs`
tests).

*(Corrective round 2 correction: "generic `verify_prefix` has no opinion
on it (passes)" above was true when this round shipped, and was itself
the residual defect an independent re-gate then found — the semantic
layer existed but was never actually REACHED by `verify_prefix`/
`replay`/`classify_record`, so a production consumer calling any of
those (as every real caller in this codebase does) got no protection
from it at all. §16/§17 record the fix: `verify_prefix` now runs this
semantic layer itself, and the corresponding unit test was rewritten
to prove `verify_prefix` — not just the standalone helper — rejects the
mismatch.)*

**The related major cluster**, closed alongside the two blockers:

- The receipt schema was private to the root crate and loosely typed
  (bare strings for locator/digest/size/kind). Moved into `o7-run` as one
  `#[serde(deny_unknown_fields)]` typed schema (§3), shared by writer,
  materializer, semantic verifier, and tests — an unrecognized field or
  patch kind now fails closed at deserialization, not at some later ad-hoc
  string comparison.
- A receipt's fields were checked for INTERNAL self-consistency but never
  bound against an independent authority. Added
  `RunContract.candidate_state: Option<CandidateStateContractV1>` (§3) —
  a top-level run establishes it fresh; a continuation child INHERITS it
  exactly from the verified parent, never re-derives it from its own
  `--base` flag or takes it as a receipt self-claim.
- The child's record depended on the parent's own directory still
  existing (only the receipt was copied, not the patch). Now both the
  receipt AND the patch bytes are copied into the child's own directory
  before materialization is recorded (§5 step 4) — deleting the parent
  after sealing cannot break semantic replay of the child alone.
- Writer-invalid event orderings (materialization without a command
  binding, `AgentStarted` before materialization, duplicate
  materialization, capture before patch or before an agent terminal
  outcome) were not structurally rejected. Added to the pure reducer (§5,
  `ReduceError` variants); an undischarged candidate-state obligation at
  seal time is a `Blocked` verdict, the same bucket every other unmet gate/
  policy/sandbox obligation already falls into.
- The point-of-use parent check accepted "a valid prefix that happens to
  carry a receipt" rather than requiring genuine terminal sealing. Split
  into an early contract-inheritance step and a late full-materialization
  step (§5, §13 Part 6) that additionally requires the parent's own last
  canonical event to be `RunSealed`, not merely a verdict being present
  somewhere in a valid prefix.
- Gitlink (submodule) mutations were undocumented and unchecked. Added
  deterministic `160000`-mode detection at both capture and materialization
  (§7), plus a heuristic patch-text scan at capture.

**Head discipline.** Every commit in this round is additive on top of the
PR's exact re-gated head, `92a98796dec0d108f4b1ea66d784789ef2906ada` — no
amend, rebase, squash, or force-push; R1 (PR #90, already merged to `main`)
and Alpha A1 were not touched. §12 lists the exact additive commit
sequence; §15 records this round's own gate results.

## 15. Corrective round 1 — evidence

Base for this round: the PR's own exact re-gated head,
`92a98796dec0d108f4b1ea66d784789ef2906ada` (the original A0 round's final
commit, itself additive on top of R1's merge commit `eead3b7`). New head:
see the PR body / `git log` (this section intentionally doesn't pin its
own commit's hash into itself, matching §13's own precedent).

- `cargo fmt --check` — clean.
- `git diff --check` — clean.
- `cargo check -p o7 -p o7-run -p o7-ledger -p o7d` — clean.
- `cargo test -p o7-run` — 72 unit + 18 `replay_acceptance` = 90 passing,
  unchanged from §13.
- `cargo test -p o7-run --test candidate_state` — **13/13 passing** (new
  this round): the structural-ordering rules and the P8 semantic-
  verification proof (§14).
- `cargo test -p o7-run --test replay_acceptance` — 18/18 passing,
  unchanged.
- `cargo test -p o7-ledger --test commands` — 39/39 passing, unchanged.
- `cargo test -p o7 --test r1_command_e2e` — **37/37 passing, unchanged**
  — R1's own full suite re-proven again against this round's restructuring
  (the early/late parent-verification split, §5).
- `cargo test -p o7 --test a0_candidate_state_e2e` — **12/12 passing**: the
  original eleven (ten negatives + the A→B→C chain) plus the new P6
  symlink-escape regression test (§6 item 11), with the "never completes"
  assertion helper relaxed to accept either a `404` (no ledger row ever
  created, for cases the early step now catches) or a `200` with a
  non-`completed` status (the late-step cases), both provably safe.
- `cargo clippy --workspace --all-targets -- -D warnings` — clean, 0
  warnings, across the whole workspace.
- `cargo test --workspace --no-fail-fast` — every crate green with the
  SAME five pre-existing environmental exclusions §13 already disclosed
  and carries forward unchanged, none of which touch any A0/corrective-
  round-1 code: `kill_after_commit_preserves_event`,
  `kill_before_commit_leaves_no_partial`
  (`crates/o7-ledger/tests/crash_durability.rs`, reproduced hanging again
  on this VPS this round, matching §13's own note that it reproduces
  against a pristine base too), `a_blocking_fifo_target_fails_closed_within_a_bound`,
  `no_control_descriptor_leaks_to_a_concurrent_sibling`, and
  `a_live_launch_executes_the_sealed_target_not_a_swapped_source`
  (`crates/o7-worker/tests/sandboy_lifecycle.rs`, timing-sensitive under
  this VPS's single-core contention) — run explicitly excluded via
  `-- --skip <name>` (each skipped test is one of the five named above)
  rather than masked.

  **A sixth, newly observed exclusion, same file, same class**: with the
  five above skipped, `an_unexpectedly_launched_target_is_a_fail_not_the_
  refusal_pass` (also `crates/o7-worker/tests/sandboy_lifecycle.rs`) failed
  with `spawn_probe did not return within the bound for a launched
  target` — reproduced standalone (`cargo test -p o7-worker --test
  sandboy_lifecycle an_unexpectedly_launched_target_is_a_fail_not_the_
  refusal_pass -- --exact`, isolated, no other tests running concurrently)
  in 25s, the identical "spawn probe timing bound" shape as the other four
  disclosed exclusions in this same file. `git diff --stat
  eead3b7..HEAD -- crates/o7-worker crates/o7-sandbox-protocol` is EMPTY —
  zero lines changed in either crate across the entire A0 + corrective-
  round-1 history, on either branch point — so this is not a regression
  this PR introduced; it is the same VPS single-core spawn-timing
  environment already responsible for the other four exclusions in this
  file, newly tripping a fifth specific test in it. Disclosed, not masked,
  same as the rest.
- `cargo deny check advisories bans licenses sources` — fails to load the
  local advisory-db snapshot on this dev VPS (the same pre-existing
  `RUSTSEC-2026-0041`/`lz4_flex` `CVSS:4.0` parse error every prior round
  already disclosed, unchanged) — **zero new external dependencies this
  round**: the only `Cargo.lock` change is `rustix` (already an in-tree,
  `deny.toml`-allowed dependency of `o7-worktree`) becoming a new
  dependency EDGE of the root `o7` package too, not a new external
  package.
- `npm test` (`apps/q-deck`) — 45/45 pass, 8 files, unchanged (this round
  does not touch the frontend).
- `npm run check` (`apps/q-deck`) — 0 errors, 0 warnings, 182 files,
  unchanged.

### Provider invocation counts, this round

- `a_symlink_at_the_old_temp_files_exact_name_never_escapes_the_confined_temp_store`:
  exactly 2 invocations (Run A, the command child) despite the hostile
  symlink in the base commit; the external sentinel file's bytes are
  byte-for-byte unchanged; an independent scratch checkout of the
  materialized tree shows the symlink itself intact, unfollowed.
- Every negative case in §6 (ten original plus the new symlink test):
  invocation count stays at exactly what it was before the negative
  command — the provider is never invoked a second time.
- The A→B→C chain (`candidate_state_flows_through_a_b_c_chain`): unchanged
  from §13, exactly 3 invocations total.

### Byte-preservation and confinement, this round

- No `String::from_utf8_lossy`/`from_utf8`/`to_string_lossy` touches the
  actual patch BYTES transport path anywhere from `git diff` stdout
  through storage through `git apply` input — verified by direct
  inspection of `worktree.rs`'s `capture_cumulative_candidate`/
  `apply_candidate_patch`/`run_git_bytes`. The only text conversions
  remaining are: `run_git`'s stderr-only diagnostic formatting after a
  git invocation already failed, and `patch_touches_gitlink`'s heuristic
  post-hoc text scan (never mutates the stored bytes).
- The private temp store (`<runs_dir>/.o7-candidate-tmp/`) uses
  `O_EXCL | O_NOFOLLOW | O_CREATE`, mode `0600`, a directory-fd opened
  `O_NOFOLLOW`, and a unique per-write filename
  (`apply-input.<pid>.<counter>`) — no name collision or symlink
  substitution reachable from anything a candidate's own tree controls.
- Artifact locators (`candidate_state_receipt.json`, `candidate.patch`,
  `parent_candidate_receipt.json`, `parent_candidate.patch`) remain exact,
  fixed, canonical filenames — never taken from an absolute path, a `..`
  segment, or a nested arbitrary path in any receipt/event field. A
  dedicated process-level test for a hostile SYMLINK specifically at one
  of these canonical names (beyond the P6 temp-file case already proven)
  is not yet written — disclosed as deferred, §6.

### Known limitations carried into this round (disclosed, not hidden)

See §6's "Closed this round"/"Explicitly deferred" paragraphs for the full,
itemized list. In short: duplicate-receipt and unsupported-schema-in-a-
real-command-continuation are unit-tested but not yet process-level;
path-traversal-in-a-patch-locator and a submodule-mutation patch reaching
a real continuation are unit-tested (gitlink detection, artifact-locator
confinement) but likewise not yet process-level E2E; a dedicated non-UTF-8/
binary-content A→B→C process-level test does not yet exist (byte
preservation is proven by construction and at the unit level, §4); a crash
between opening the private temp file and unlinking it (e.g. `SIGKILL`
mid-write) can leave an inert, `0600`, private-store-confined residual
file with no periodic sweeper yet; the individually-triggered crash-window
sub-cases remain future work, unchanged from §13's own note. Frontend
(`apps/q-deck`) remains entirely out of this slice, unchanged.

### Clean worktree confirmation

`git status --short` shows no unstaged/untracked changes beyond what this
round's own commits already captured; every gate above was run against the
exact commit this round's final push carries.

## 16. Corrective round 2: authoritative semantic replay and legacy-parent admission

An independent re-gate against corrective round 1's exact head
(`7014bcc6f19ec8f4f7a3134b6ec21815b67640b1`) confirmed P6/byte-preservation
stayed closed, but found the P8 fix, while real, was not yet
AUTHORITATIVE, plus a genuine wedging defect in the round's own defense-
in-depth. Neither is a rebuild of anything already accepted (the cumulative
model, private temp store, byte-preserving transport, R1's dispatch
boundary, and the command redrive classifier are all explicitly unchanged
this round) — this round closes the gap between "the right check exists
somewhere in this codebase" and "every real production caller actually
runs it."

**Defect 1 — candidate semantics existed but were not reached.**
`o7_run::candidate::verify_candidate_state_captured`/
`verify_candidate_state_materialized` (§14) were real, correct, and
unit-tested — but `verify_prefix`, `replay`, `replay_verify`, and
`classify_record` never called them. Every ACTUAL production consumer —
`o7 replay`, `src/recovery.rs`'s `classify_command_child` (the decision
behind `o7d`'s own redrive path AND `o7 recover`'s discovery report), this
round's own new admission preflight, `o7d`'s candidate-lineage DTO
projection — goes through one of those four functions, never the
candidate helpers directly. So a syntactically valid, digest-consistent,
internally-self-consistent candidate receipt with semantically MEANINGLESS
content (the exact synthetic X/Y tree mismatch §14's own unit test proved
the HELPER catches) sailed through every real caller untouched.

**Fix**: `crates/o7-run/src/replay.rs`'s old `verify_prefix` body is now
`pub(crate) fn verify_prefix_core` — everything it always did (chain/
digest/reducer/artifact-digest), unreachable from outside the crate under
a name that could be mistaken for full verification. The NEW `pub fn
verify_prefix` (same name, same signature — every existing call site
needed zero changes) runs `verify_prefix_core`, then — only when candidate
evidence is actually present in the reduced state — the candidate semantic
layer, mapping any failure to a new `ReplayError::CandidateSemantic`. A
record with no candidate evidence at all (every pre-A0/pre-round-1 record)
is entirely unaffected: the semantic step is simply never invoked, so
`verify_prefix` stays byte-for-byte identical to `verify_prefix_core`'s own
result for it — zero backward-compatibility risk.

**Defect 1's own related cluster, closed alongside it**:
- Cross-binding was too narrow: `verify_candidate_state_captured` only
  checked "an initial run's receipt has no parent" in isolation; it never
  proved a CONTINUATION's receipt actually agrees with its own
  `CommandBindingCaptured` evidence. New dependency-free
  `CommandBindingFacts` (mirroring the root crate's own `CommandBinding`,
  same rationale as `RepositoryIdentity` mirroring `CanonicalRepoId` in
  round 1) lets the semantic layer resolve and parse a run's own command-
  binding artifact itself; both captured and materialized checks now
  cross-bind `run_id`/`conversation_id`/`parent_run_id` against it
  symmetrically.
- Locators were checked for KIND but not for the exact canonical NAME —
  new `require_locator` checks applied to every receipt/patch/child-local-
  copy `ArtifactRef` (`candidate_state_receipt.json`, `candidate.patch`,
  `parent_candidate_receipt.json`, `parent_candidate.patch`).
- `verify_candidate_state_materialized` gained a `contract` parameter and
  now cross-binds the copied source receipt against BOTH this run's own
  command binding (`source_run_id == parent_run_id`) AND its own candidate
  contract (`conversation_id`/`repository_id`/`base_commit`/`patch_kind`) —
  not merely the internal receipt/event pair round 1 proved.
- The always-null DTO field-name bug (`candidate_projection` reading a raw
  JSON field, `actual_tree_oid`, that stopped existing the moment round 1
  reshaped the event) is fixed in its own narrow commit, and the whole
  function now runs full `verify_prefix` instead of trusting raw JSON —
  it can no longer project `"materialized"` for a record full replay would
  reject.

**What a standalone materialized child record still does and does not
prove** (explicitly not overclaimed): it proves its IMMEDIATE source
lineage (this exact parent run id, this exact tree) and that its own
copied evidence is internally consistent and cross-bound against this
run's own contract/binding. It does NOT re-prove the parent's entire
ancestry chain back to the conversation's original base run — that would
require copying the parent's own command binding too, which this round
does not do. The parent's own terminal, sealed state is proven separately,
at the point of use, by the (unchanged) `materialize_parent_candidate_state`
— this round did not weaken or remove that check.

**Defect 2 — a legacy/unusable parent could wedge a conversation forever.**
`continue_run`'s `mark_rejected` closure existed, but the candidate-
obligation resolution most likely to need it ran BEFORE the closure was
even defined — its failure propagated via a bare `?`, returning straight
out of `continue_run` with NO terminal transition ever attempted. A
command whose candidate obligation could not be inherited (a pre-A0
parent reached directly via the CLI, or a stale accepted row from before
this round's own new preflight existed) was left `started`/`accepted`
forever: invisible to `stuck_commands` (scoped to unbound rows) and to
`reconcile_completed_commands` (scoped to a sealed child that never
existed).

**Fix, two layers**:
1. **Admission preflight** (`crates/o7d/src/canonical.rs`'s new
   `parent_candidate_state_usable`, wired into `routes::create_command`):
   for a GENUINELY fresh acceptance (checked via a new, pure read-only
   `command_idempotency_key_seen` peek, so a replay of a request accepted
   before this preflight existed is never retroactively rejected), the
   parent must pass full authoritative replay, be sealed, declare a
   candidate obligation, and have a captured receipt — else a new stable
   `409 COMMAND_PARENT_CANDIDATE_UNAVAILABLE`, before any command row,
   child run, or worktree ever exists.
2. **Defense in depth** (`src/main.rs`'s `continue_run`, for whatever
   reaches this code path DESPITE the preflight — a direct CLI invocation,
   a race, or a row accepted by an older deployment): `mark_rejected` is
   now defined and reachable BEFORE the candidate-obligation resolution
   call, using a new atomic, single-transaction
   `mark_command_rejected_if_unattached_and_bound` (replacing the old
   check-then-act `ledger_run_exists`-then-`mark_command_rejected` pair,
   which had its own separate-read/separate-write race window) — checks
   AND writes, atomically, that the command is still bound to this run id
   in a non-terminal status AND that run id has never attached a ledger
   row.

**Part 8 — canonical replay validity is not continuation eligibility.** A
sealed `Blocked`/`Fail`/`Error` parent is an honest, replay-valid record of
an unsuccessful run; `parent_candidate_state_usable` checks
`state.verdict.is_none()` (not sealed) — never the verdict's VALUE — as
its sealed/unsealed predicate, so a non-Pass verdict never by itself makes
a parent's candidate state unusable. Proven directly by a new process-level
test: a parent sealed with `Fail` (a required gate genuinely failing)
still accepts a valid follow-up command normally.

**Existing guarantees re-verified, unchanged**: P6 symlink confinement;
byte-preserving transport; the A→B→C cumulative chain; R1's `AgentStarted`
dispatch boundary; pre-dispatch materialization failures stay redrivable;
post-dispatch unsealed state stays ambiguous and never auto-redriven;
same-key concurrent requests converge; the provider is never invoked a
second time; child evidence stays self-contained — every relevant existing
test suite was re-run and stayed green (§17).

## 17. Corrective round 2 — evidence

Base for this round: the PR's own exact re-gated head,
`7014bcc6f19ec8f4f7a3134b6ec21815b67640b1` (corrective round 1's final
commit). New head: see the PR body / `git log`.

- `cargo fmt --check` — clean.
- `git diff --check` — clean.
- `cargo check -p o7 -p o7-run -p o7-ledger -p o7d` — clean.
- `cargo test -p o7-run` — 72 unit + 18 `replay_acceptance` = 90 passing,
  unchanged.
- `cargo test -p o7-run --test candidate_state` — **24/24 passing** (13
  carried from round 1, 11 new this round): the restructured P8 proof
  (now shown failing through `verify_prefix`/`classify_record`
  themselves, not just the standalone helper) plus ten new cross-binding
  negatives.
- `cargo test -p o7-run --test replay_acceptance` — 18/18, unchanged.
- `cargo test -p o7-ledger --test commands` — **45/45 passing** (39
  carried, 6 new): `mark_command_rejected_if_unattached_and_bound`'s own
  four cases and `command_idempotency_key_seen`'s two.
- `cargo test -p o7 --test r1_command_e2e` — **37/37 passing, unchanged**
  — confirms the new admission preflight does not affect any R1 fixture
  (every fixture in this suite already goes through `o7 run --ledger`,
  which has unconditionally captured candidate state since round 1, so
  every one of its parents is already A0-usable).
- `cargo test -p o7 --test a0_candidate_state_e2e` — **16/16 passing** (12
  carried, 4 new): every existing tampering negative now accepts either
  the new `409` (caught at admission) or the original `202`-then-never-
  completes (caught only at materialization) as equally valid proof; two
  tests' own tampering technique was fixed to keep the record chain-
  consistent so each fails for the reason its name claims, not an
  incidental digest mismatch; three new legacy-parent-admission process
  tests (fresh/already-accepted/racing); one new sealed-non-Pass-verdict
  test.
- `cargo test -p o7d` — every existing suite green, unchanged.
- `cargo clippy --workspace --all-targets -- -D warnings` — clean, 0
  warnings, across the whole workspace.
- `cargo test --workspace --no-fail-fast` — every crate green with the
  SAME six environmental exclusions §15 already disclosed, run
  single-threaded (`--test-threads=1`) to avoid this VPS's own cross-
  binary resource-contention flakiness under the default parallel
  runner — none of the six touch any code this round changed.
- `cargo deny check advisories bans licenses sources` — same pre-existing
  local advisory-db parse error every prior round disclosed — **zero new
  dependencies this round**: `Cargo.lock`/`Cargo.toml` are byte-for-byte
  unchanged.
- `npm test` / `npm run check` (`apps/q-deck`) — 45/45 / 0 errors,
  unchanged (frontend untouched).

### Provider invocation counts, this round

- Every negative case in the process-level suite: invocation count stays
  at exactly what it was before the negative command, whether caught at
  admission (`409`, before any command row) or at materialization (`202`,
  before `AgentStarted`).
- The new legacy-parent tests: Case A (fresh command, pre-A0 parent) — 0
  additional invocations, no command row. Case D (already-accepted legacy
  row, direct `o7 continue`) — 0 additional invocations, command
  transitions to `rejected`. Case E (two racing processes) — 0 additional
  invocations from either, exactly one lock winner rejects the command
  once.
- The A→B→C chain and the sealed-non-Pass-verdict test: unchanged/expected
  invocation counts (3 total; 1 for the Fail-verdict parent, then a normal
  second invocation for its accepted follow-up).

### Known limitations (disclosed, not hidden)

Everything §15 already disclosed remains open (duplicate-receipt/
unsupported-schema/path-traversal/submodule-mutation process-level tests;
non-UTF-8 A→B→C process-level test; no temp-residue sweeper). New this
round: `load_verified_candidate_receipt` (`src/main.rs`) still calls
`verify_candidate_state_captured` explicitly after `verify_prefix` —
deliberately kept (§5's own note) because it needs the parsed receipt
object itself, which `verify_prefix` does not return, not because the
verification is still missing from `verify_prefix`. The admission
preflight (`parent_candidate_state_usable`) does not itself verify
`base_commit` actually exists as a real Git commit in this repository —
that check remains where it always was, in `materialize_parent_candidate_state`
at the point of use, since the preflight never touches Git at all (a
deliberate scope boundary: admission answers "does this parent's OWN
record prove usable candidate state," not "will Git actually accept this
patch," which only a real worktree can answer). No background GC added
this round. Frontend remains entirely out of this slice.

### Clean worktree confirmation

`git status --short` shows no unstaged/untracked changes beyond what this
round's own commits already captured; every gate above was run against
the exact commit this round's final push carries.
