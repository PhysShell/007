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

```text
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

```text
git add -A                                                  # already done by diff_vs_base
# reject if any tracked path carries an assume-unchanged/skip-worktree
# hidden-index flag (round 7) or a dirty/nested submodule (round 5) —
# both checked against the SAME staged state the snapshot below freezes
freeze a private copy of the real index file                # round 8
git diff --cached --binary --full-index --no-color \
  --no-ext-diff <conversation base_commit>                  # against the FROZEN copy, via GIT_INDEX_FILE
git write-tree                                              # -> candidate_tree_oid, against the SAME frozen copy
# reject if the patch or resulting tree touches a gitlink (mode 160000, §7)
```

`capture_cumulative_candidate(worktree, tmp_parent, base_commit) ->
Result<(Vec<u8>, String)>` returns the diff's stdout as **raw `Vec<u8>`
bytes end to end** — never through a `String` at any point in the
transport path. Git's own diff
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

**Exact capture cutoff (round 8):** the patch and `candidate_tree_oid` are
both derived from the ONE frozen index snapshot taken right after the
hidden-flag and dirty-submodule checks above pass — never from the live
worktree index again. This guarantees the two are always mutually
consistent (`apply(patch, base_commit).tree_oid == candidate_tree_oid`,
always), but it does NOT guarantee that every edit present in the working
tree at the moment `add -A` finished is included: a concurrent mutation of
the real index between `add -A` and the snapshot read is a genuine race
this capture makes no claim about either way. Do not read this section as
claiming concurrently modified working-tree bytes are captured — only
that whatever the snapshot froze is what both the patch and the tree
faithfully, consistently represent.

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
6, §14 Part 1) — no name a candidate's own tree contains can ever collide
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
  step (§5, §14 Part 6) that additionally requires the parent's own last
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
## 18. Corrective round 3: Codex review 4838314243 (five P1 findings)

An `@codex review` requested against corrective round 2's exact accepted
head (`4ce14bfd6b5c029f0477b4373c8069850e6ba0c2`) returned five P1 findings
(review `pullrequestreview-4838314243`). None reopens anything round 2
proved (authoritative semantic replay, P6 confinement, byte-preserving
transport, the A→B→C chain, R1's dispatch boundary, legacy-parent
admission) — all five are genuinely new gaps this round closes.

**Finding 1 — the parent ID reached the filesystem unconfined**
(`3698894143`, `crates/o7d/src/routes.rs:623`). For a genuinely fresh
idempotency key, `routes::create_command` called
`parent_candidate_state_usable` — which calls `child_record_dir`, which
joins the raw HTTP `parent_run_id` straight into a filesystem path — before
the ledger had validated that string at all. A value like `../../outside`
or an absolute path was never rejected as malformed; it was handed to the
filesystem.

**Fix**: a single shared validator, `crates/o7d/src/canonical.rs`'s new
`is_confined_run_id_component`, checks (via `std::path::Component`
matching, not a substring denylist) that a run id is exactly one normal
path component — non-empty, not absolute, no `.`/`..`, no embedded slash.
It is called from inside `child_record_dir` itself (so every filesystem
helper that resolves a run id refuses before any I/O, not only the one
HTTP call site that happened to check first), and `routes::create_command`
now checks it explicitly and returns `400` before doing anything else.

**Finding 2 — the established 404 contract had a gap** (`3698894147`,
same file, line 628). Once the parent id was confined, nothing yet
proved the parent existed or belonged to the requested conversation before
falling through to the candidate-usability check — a well-formed but
nonexistent or cross-conversation parent risked the wrong failure shape.

**Fix**: a pure, read-only ledger reference preflight — conversation
exists, parent exists, parent's own `conversation_id` matches — runs
immediately after the confinement check and before any candidate-state
I/O. It is explicitly not a substitute for `SqliteLedger::create_command`'s
own transactional authority, which still repeats every mutation-time
check; it exists only to give `404` the chance to fire before `409` would
otherwise mask it. The three-way ordering is now: malformed id → `400`;
unknown/wrong-conversation parent → `404`; known parent with unusable
candidate evidence → `409 COMMAND_PARENT_CANDIDATE_UNAVAILABLE` — the same
`404`/`409` contract round 2 established, now with `400` layered in front
of both, never behind either.

**Finding 3 — the gitlink policy rejected the wrong thing**
(`3698894151`, `src/worktree.rs:117`). The existing check rejected a
candidate tree for containing *any* gitlink at all, including one already
present, unchanged, in the immutable base — so a repository with a
pre-existing submodule could never accept a single valid follow-up
command, a functional regression for any real repository shaped that way.

**Fix**: `ensure_no_gitlink_mutation` replaces the whole-tree check with
an authoritative *set* comparison. `gitlink_entries` runs
`git ls-tree -r -z <commit>` — NUL-delimited, so paths are handled as raw
bytes, never lossy-UTF8-decoded as an authority — and parses it into a
`BTreeSet<(Vec<u8> path, String oid)>` of every `160000`-mode entry. The
policy is: `base_commit`'s set must equal the candidate's set, exactly.
Unchanged gitlinks pass; an added, deleted, or OID-modified gitlink, or a
mode transition to/from `160000`, is rejected. Applied at both capture
(`capture_cumulative_candidate`) and materialization (`finish_apply`, via
`apply_candidate_patch`'s new `base_commit` parameter). The old lossy-text
`patch_touches_gitlink` heuristic is kept only as a non-blocking
diagnostic (`eprintln!`, not `anyhow::ensure!`) — an unchanged-mode,
OID-only submodule bump need not contain a new/deleted mode header, so the
heuristic was never sound as an authority.

**Finding 4 — materialization bound the wrong identity**
(`3698894153`, `crates/o7-run/src/candidate.rs:358`).
`verify_candidate_state_captured` already cross-bound
`receipt.run_id == command_binding.child_run_id`; the equivalent check was
simply missing from `verify_candidate_state_materialized`, so a
materialized-but-never-captured record could seal under a command binding
naming a completely different child, and nothing in
`replay`/`replay_verify`/`classify_record` would catch it.

**Fix**: `verify_candidate_state_materialized` now requires `state.run_id`
to exist and requires `binding.child_run_id == state.run_id`, symmetric
with the captured-side check. Proven with a dedicated fixture: a sealed,
chain-consistent `Blocked` record, valid contract/receipt/patch/tree, a
command binding naming a different child, no `CandidateStateCaptured`
evidence — rejected by `verify_prefix` (`ReplayError::CandidateSemantic`),
by `replay` and `replay_verify` (same error variant), and by
`classify_record` (`Invalid`); recovery and DTO projection fail closed by
construction, since both route through the same `verify_prefix`.

**Finding 5 — admission never checked which repository the evidence was
captured against** (`3698894158`, `crates/o7d/src/canonical.rs:105`).
`parent_candidate_state_usable` proved a parent's candidate contract was
internally sealed and usable, but never compared its `repository_id`
against the repository `o7d` is actually configured for — a command
against a same-named-directory, different-repository parent's evidence
would be accepted.

**Fix, two layers, mirroring round 2's own admission/defense-in-depth
shape**: `repository_identity` (previously private to `main.rs`) moved to
`pub fn worktree::repository_identity` so both the binary and `o7d` (which
depends on `o7` as a library) resolve the exact same canonical identity.
`parent_candidate_state_usable` now compares the parent's own
contract-level `repository_id` against `worktree::repository_identity(&exec.repo)`
and rejects on mismatch. `resolve_inherited_candidate_obligation`
(`src/main.rs`) gained a `repo: &Path` parameter and performs the same
comparison against the parent's *receipt*-level `repository_id`, so a
direct `o7 continue` invocation — bypassing `o7d`'s own preflight entirely
— self-rejects the same way. Proven with two genuinely different Git
repositories sharing the same final directory basename (both named
`myrepo`, under distinct parent directories, one shared `runs_dir`/ledger)
so `child_target`'s own "canonicalized repo's final path component"
collision is actually exercised, not merely asserted: a fresh command
against foreign-repository evidence gets `409`, zero command rows, zero
provider invocations; a legacy accepted-and-bound row (direct SQL insert,
modeling a pre-this-round deployment) self-rejects atomically via
`o7 continue`, and a second invocation against the same row proves the
rejection does not wedge into a redrive loop. The later, unrelated
point-of-use repository check in `materialize_parent_candidate_state` is
untouched by this round.

**Commit shape** (five, additive, on top of round 2's frozen head
`4ce14bfd6b5c029f0477b4373c8069850e6ba0c2`):
`fix(o7d): confine and validate command parent preflight`,
`fix(worktree): allow unchanged gitlinks only`,
`fix(replay): bind materialization to child run identity`,
`fix(o7d,root): bind candidate admission to configured repository`,
`test(q-deck): cover Codex P1 findings`.

## 19. Corrective round 3 — evidence

Base for this round: the PR's own exact re-gated head,
`4ce14bfd6b5c029f0477b4373c8069850e6ba0c2` (corrective round 2's final
commit, the same head Codex review `4838314243` reviewed). New head: see
the PR body / `git log`.

- `cargo fmt --check` — clean.
- `git diff --check` — clean.
- `cargo test -p o7-run --test candidate_state` — **25/25 passing** (24
  carried from round 2, 1 new): the materialization/child-binding negative
  (Finding 4), proven rejected through `verify_prefix`, `replay`, and
  `replay_verify` directly, not only through the standalone helper.
- `cargo test -p o7-ledger --test commands` — 45/45 passing, unchanged.
- `cargo test -p o7 --test r1_command_e2e` — **37/37 passing, unchanged**
  — confirms the new confinement/404/repository checks do not affect any
  R1 fixture.
- `cargo test -p o7 --test a0_candidate_state_e2e` — **22/22 passing** (16
  carried from round 2, 6 new): path-confinement negatives (traversal,
  absolute, nested, `.`/`..`, each proven not to touch a planted outside
  sentinel file/FIFO/large file); the established-404 negatives (unknown
  parent, cross-conversation parent); the two foreign-repository-admission
  process tests (fresh command, legacy accepted row via direct
  `o7 continue`, including the no-perpetual-redrive-loop check).
- gitlink policy — new unit test module in `src/worktree.rs`
  (`gitlink_policy_tests`, 9 tests, pure Git-plumbing, no provider
  process): unchanged pre-existing gitlink at capture and at
  materialization; add/delete/OID-modify rejected at both; regular-file↔
  gitlink transition rejected in both directions; nested and
  unusually-named gitlink paths handled deterministically.
- `cargo clippy --workspace --all-targets -- -D warnings` — clean, 0
  warnings, across the whole workspace.
- `cargo test --workspace --no-fail-fast` — every crate green, run
  single-threaded (`--test-threads=1`) for the same VPS cross-binary
  resource-contention reason §15/§17 already disclosed, with the same six
  environmental exclusions carried forward unchanged (none touch any code
  this round changed). Total reported test time this round runs past the
  10-minute ceiling this VPS's own shell tooling imposes on a single
  foreground command (`o7-worker`'s own suite alone — unchanged by this
  round — accounts for roughly 360 of the run's ~450 reported seconds), so
  this round's run was launched detached from that ceiling rather than
  split or shortened; every one of the resulting 97 `test result:` lines
  across all eight workspace crates reported `0 failed`.
- `cargo deny check advisories bans licenses sources` — a **different**
  local advisory-db error from every prior round's disclosed `lz4_flex`/
  `CVSS:4.0` parse failure: `git operation failed: expected .../
  nostr-relay-pool/RUSTSEC-0000-0000.2.md to be named "RUSTSEC-0000-0000.md"`.
  Confirmed unrelated to this round's code:
  `git diff --stat 4ce14bf..HEAD -- Cargo.lock Cargo.toml crates/*/Cargo.toml`
  is empty (zero dependency changes). This VPS's root filesystem was
  observed at 100% capacity (276 MiB free of 52 GiB, almost entirely the
  pre-existing relocated `~/cargo-target` build-cache mount and the Nix
  store) while investigating — a plausible root cause for a corrupt/
  partial local git-clone of the advisory database, though not confirmed
  by direct inspection of that clone.
- `npm test` / `npm run check` (`apps/q-deck`) — 45/45 / 0 errors,
  unchanged (frontend untouched this round).

### Provider invocation counts, this round

- Malformed-parent-id cases (`400`): provider invocation count unchanged;
  no command row created.
- Unknown-parent and cross-conversation-parent cases (`404`): same.
- Foreign-repository cases (`409` at fresh admission; atomic `rejected`
  transition via direct `o7 continue` for the legacy row): zero
  additional provider invocations in either case; the legacy-row test's
  second, repeat `o7 continue` invocation also invokes the provider zero
  times, confirming no redrive loop.
- Materialization child-binding negative: no process-level provider
  invocation involved (a constructed-event unit fixture, not a live run).
- Gitlink-policy tests: pure Git-plumbing, no provider process spawned —
  the fix does not touch provider-invocation logic, and the existing
  `candidate_state_flows_through_a_b_c_chain` process test already
  re-proves normal capture/materialization end-to-end with its own
  unchanged invocation count.

### Known limitations (disclosed, not hidden)

Everything §15/§17 already disclosed remains open. New this round: the
gitlink-policy tests are pure Git-plumbing unit tests, not process-level
tests spawning a real provider — this round's own fix does not touch
provider-invocation logic, but the requirement's own "provider invocation
... unchanged" sub-claim is proven indirectly (by the fix's own scope)
rather than by a dedicated process-level gitlink test. The materialization
child-binding fix's "recovery and DTO projection fail closed" claim is
proven by construction — `classify_command_child` and `candidate_projection`
both route through the same `verify_prefix` this round proved rejects the
fixture — rather than via a separate end-to-end process test exercising
`o7 recover` or the DTO endpoint directly against this exact fixture. The
foreign-repository fresh-command test asserts zero command rows and zero
provider invocations but does not separately assert zero worktree
directories were created (implied, not independently checked on disk).
The `cargo deny` advisory-db error's disk-pressure root cause is a
plausible correlation from this investigation, not a confirmed diagnosis.

### Clean worktree confirmation

`git status --short` shows no unstaged/untracked changes beyond what this
round's own commits already captured; every gate above was run against
the exact commit this round's final push carries.
## 20. Corrective round 4: Codex review 4839309124 (four P1 findings)

An `@codex review` requested against corrective round 3's exact head,
`b4cc530a6c7c349b2af0179a95733ef1567c2fe6`, returned four P1 findings
(review `pullrequestreview-4839309124`). None reopens anything round 3
proved — three are lint-policy findings, one is a genuine correctness
gap this round closes.

**Finding 1 — the candidate contract's own schema was parsed but never
validated** (`3699854999`, `crates/o7-run/src/candidate.rs:227`).
`CandidateStateContractV1.schema` existed as a field, but nothing —
neither the reducer nor `verify_candidate_state_captured`/
`_materialized` — ever compared it to anything. A `RunStarted` could
declare `candidate_state.schema = 2` while every receipt stayed at
schema 1, and replay/admission would accept it as authoritative;
`resolve_inherited_candidate_obligation` then silently re-stamped the
child's own new contract with the current build's schema constant,
masking the disagreement rather than failing closed on it — an unknown
contract format could become continuation authority instead of being
refused.

**Fix**: a new `CANDIDATE_STATE_CONTRACT_SCHEMA_V1` constant
(`crates/o7-run/src/event.rs`), deliberately distinct from
`CANDIDATE_STATE_RECEIPT_SCHEMA_V1` — the contract and receipt are
different documents with independent schema spaces, even though both
happen to be `1` today. Checked at the earliest authoritative layer,
`reduce::validate_contract` (run once, unconditionally, at `RunStarted`
— every replay path already goes through it), via a new
`ReduceError::UnsupportedCandidateContractSchema`, so a record fails
closed regardless of whether any candidate evidence ever follows at
all. Defense in depth: the same check re-runs inside
`verify_candidate_state_captured`/`_materialized`, mirroring how every
other obligation field is already redundantly checked at that layer.
`resolve_inherited_candidate_obligation`'s two contract-construction
sites now name the contract-specific constant instead of borrowing the
receipt's — not a behavior change (both are `1`), a correctness one:
the child's own contract was never meant to copy the parent's raw
schema value, only to declare what THIS build understands, which the
reducer fix now guarantees the parent's own schema already was.

**Findings 2-4 — blanket restriction-lint exemptions with no stated
invariant** (`3699855000`/`crates/o7-run/tests/candidate_state.rs:9`,
`3699855002`/`src/worktree.rs:366`, `3699855003`/
`tests/a0_candidate_state_e2e.rs:19`). Each of these three files/modules
carried a file- or module-scoped `#[allow(clippy::unwrap_used,
clippy::expect_used, clippy::panic, clippy::indexing_slicing)]`. This
scoping granularity itself is not novel or improper in this codebase —
it is the SAME pattern used by roughly eighty other test files across
this workspace, including already-merged, already-reviewed ones
(`tests/r1_command_e2e.rs`, `crates/o7-ledger/tests/commands.rs`, every
`o7-worker` fault/sandboy test) — but a smaller subset of that
precedent (`tests/sse.rs`, `crates/o7d/tests/golden_transcript_sse.rs`,
`tests/live_ingress_e2e.rs`) additionally carries an explicit paragraph
stating WHY the exemption is sound: every unwrap/expect/index operates
on the test's own controlled fixtures/output, so a panic there is the
test's own setup or assertion failing loudly, never a runtime condition
in production code. The three files Codex flagged had a comment
present, but it explained unrelated context (the git-fixture mechanics,
the helper-mirroring rationale) rather than that invariant.

**Fix**: each of the three sites gained the same invariant paragraph
the established best-precedent files already use, phrased for what that
specific file's own operations actually touch (a hand-assembled
`RunState`/`RunContract` fixture; a real throwaway git repository in a
tempdir; a real spawned `o7`/`o7d` process and REST response). This is
a deliberate judgment call, disclosed rather than silently made: a true
per-function/per-line rewrite (dozens of individually-scoped allows per
file, ~97 functions across the three targets) was considered and
rejected as inconsistent with this codebase's own dominant, working
convention — matching the established BEST version of that convention,
not inventing a new and narrower one that the rest of the workspace's
own test suite does not follow.

**Commit shape** (four, additive, on top of round 3's frozen head
`b4cc530a6c7c349b2af0179a95733ef1567c2fe6`):
`fix(replay): reject unsupported candidate contract schemas`,
`refactor(tests): scope A0 restriction-lint exemptions`,
`test(q-deck): cover unsupported contract schemas`,
`docs(q-deck): record corrective round 4 evidence`.

## 21. Corrective round 4 — evidence

Base for this round: the PR's own exact re-gated head,
`b4cc530a6c7c349b2af0179a95733ef1567c2fe6` (corrective round 3's final
commit, the same head Codex review `4839309124` reviewed). New head: see
the PR body / `git log`.

- `cargo fmt --check` — clean.
- `git diff --check` — clean.
- `cargo test -p o7-run --test candidate_state` — **29/29 passing** (27
  carried from round 3, 2 new): an unsupported candidate contract schema
  rejected with no later candidate evidence at all, and rejected again
  behind an otherwise-fully-valid capture+patch+seal (Codex's own literal
  example — contract schema 2, receipt schema 1) — both proven through
  `reduce_all`, `verify_prefix`, `replay`, `replay_verify`, and
  `classify_record` directly.
- `cargo test -p o7-ledger --test commands` — 45/45 passing, unchanged.
- `cargo test -p o7 --test r1_command_e2e` — **37/37 passing, unchanged**.
- `cargo test -p o7 --test a0_candidate_state_e2e` — **24/24 passing** (22
  carried from round 3, 2 new): a fresh command against a parent with an
  unsupported candidate contract schema rejected at admission (`409`,
  zero command rows); a legacy accepted-and-bound row against the same
  corrupted parent self-rejecting via direct `o7 continue`, past `o7d`'s
  own preflight, with zero provider invocations, zero child attachment,
  and — on a second, repeat invocation — no perpetual redrive loop.
- `cargo clippy --workspace --all-targets -- -D warnings` — clean, 0
  warnings, across the whole workspace (the three scoped-and-documented
  lint exemptions compile clean under the same crate-level
  `unwrap_used`/`expect_used`/`panic`/`indexing_slicing = "deny"` policy
  every other test file in this workspace already lives under).
- `cargo test --workspace --no-fail-fast` — every crate green, run
  single-threaded (`--test-threads=1`), same six pre-existing
  environmental exclusions carried forward unchanged, launched detached
  from this VPS's own ~10-minute foreground-command ceiling (as §19
  disclosed, `o7-worker`'s own suite alone accounts for the bulk of the
  run's total time) — all 97 `test result:` lines reported `0 failed`.
- `cargo deny check advisories bans licenses sources` — **§19's
  "different" error was itself transient, now diagnosed and corrected**:
  it was a corrupted/partial local git-clone of the advisory database
  (confirmed by deleting `~/.cargo/advisory-dbs/advisory-db-*` and
  letting `cargo-deny` re-clone it fresh, which made the naming-mismatch
  error disappear entirely). What remains after a clean re-clone is the
  SAME `lz4_flex` `RUSTSEC-2026-0041` `CVSS:4.0` parse failure every
  round since round 1 has disclosed — this installed `cargo-deny 0.18.2`
  predates CVSS v4.0 support in its bundled parser. Confirmed unrelated
  to this round's code: `git diff --stat b4cc530..HEAD -- Cargo.lock
  Cargo.toml crates/*/Cargo.toml` is empty (zero dependency changes).
  This VPS's root filesystem was observed at 100% capacity (0 bytes
  free at the tightest point, recovered to several GiB via this
  project's own relocated `~/cargo-target` build-cache mount's
  asynchronous block discard) during this round's own work — the most
  direct evidence yet that disk pressure, not a new upstream advisory-db
  defect, was the actual cause of round 3's distinct-looking error.
- `npm test` / `npm run check` (`apps/q-deck`) — 45/45 / 0 errors,
  unchanged (frontend untouched this round).

### Provider invocation counts, this round

- Unsupported-contract-schema cases (`400`-adjacent admission `409`,
  and the direct-`o7 continue` legacy-row case): zero additional
  provider invocations in every case, including the legacy row's
  repeat invocation (no redrive loop).
- Every other existing negative case: unchanged from round 3.

### Known limitations (disclosed, not hidden)

Everything §17/§19 already disclosed remains open. New this round: the
"recovery classification"/"candidate DTO projection fail closed" sub-
requirements are proven by construction — `classify_command_child` and
`candidate_projection` both route through the same `verify_prefix`/
`classify_record` these tests already proved rejects an unsupported
contract schema — rather than via dedicated process-level tests driving
`o7 recover` or the DTO endpoint against this exact fixture; this
matches round 3's own established evidentiary standard for its
analogous materialization-child-binding finding, not a new gap specific
to this round. The lint-exemption fix (Findings 2-4) is a documentation
change, not a behavior change — it does not, and was never intended to,
reduce the actual SET of operations permitted to panic in these three
test targets; a reviewer expecting true per-operation narrowing (as
opposed to a stated, file-scoped invariant matching this codebase's own
best precedent) will not find that here, disclosed as a deliberate,
reasoned choice in §20, not an oversight.

### Clean worktree confirmation

`git status --short` shows no unstaged/untracked changes beyond what
this round's own commits already captured; every gate above was run
against the exact commit this round's final push carries.
## 22. Corrective round 5: Codex review 4842896076 + CodeRabbit review 4842910559

Two independent external reviews requested against corrective round 4's
exact head, `6b77b9ac54f5b1e7583b63bee433b141fce44d3a`: Codex review
`pullrequestreview-4842896076` (two P1 findings) and CodeRabbit review
`pullrequestreview-4842910559` (9 actionable comments + 8 nitpicks). Every
finding was reproduced against that exact head, with real git/real
processes, before any fix landed — several findings changed shape or
scope once reproduced (see below); one was confirmed a false positive.

### Part 1 — dirty submodule contents fail closed (Codex P1, genuine BLOCKER)

`crates/o7-run` PR discussion `3702966505` (`src/worktree.rs:132`).
Reproduced with real git before fixing: a real committed submodule,
initialized, its own tracked file edited WITHOUT committing inside it or
touching the superproject's gitlink — `ensure_no_gitlink_mutation`'s pure
tree-to-tree comparison saw identical base/candidate gitlink sets (since
`git add -A` cannot stage submodule-internal changes short of the
submodule's own `HEAD` moving), and the resulting cumulative patch was
byte-for-byte empty. The provider's edit would have been silently and
permanently lost.

**Fix**: `ensure_no_dirty_submodule_worktree` (`src/worktree.rs`) runs
`git status --porcelain=2 -z --ignore-submodules=none -uall` — NUL-
delimited, byte-level field parsing (the `S<c><m><u>` submodule-status
field), never a text scan — at both capture and materialization. Verified
empirically that git's own submodule-dirtiness detection recurses into
NESTED submodules automatically (a nested-submodule edit surfaces on the
TOP-LEVEL gitlink entry), and that `--ignore-submodules=none` is required,
not decorative: an attacker-reachable `.gitmodules`
`submodule.<name>.ignore = all` setting successfully hides real dirtiness
from plain `git status` with no override.

### Part 2 — candidate replay bound off async Tokio workers (Codex P1 + CodeRabbit Major ×2)

Codex discussion `3702966510` (`crates/o7d/src/routes.rs:150`); CodeRabbit
`3702978286` (routes.rs:154, Major) and `3702978297` (routes.rs:677,
Major) — the same underlying finding from two independent reviewers.
`candidate_projection` (`GET /runs/{id}`) and `parent_candidate_state_usable`
(command admission) both ran full authoritative replay — unbounded
`events.jsonl`/artifact reads plus `reduce`/`verify_prefix` — directly on
a Tokio worker thread, no concurrency limit.

**Fix**: both now run on `tokio::task::spawn_blocking`, behind a shared,
fixed `Semaphore(4)` (`AppState::candidate_replay_limiter`).
`get_run`'s projection stays best-effort (`try_acquire`, never awaits a
permit — under saturation it omits the candidate fields, same as an
unconfigured `exec`); admission awaits a permit (bounded queuing, not
skipping) and preserves the existing 409/404/400 contract. A join failure
fails admission closed (500, no command row); the read-only projection
swallows it as best-effort. Explicit byte limits, checked via `metadata`
before any read: 8 MiB `events.jsonl`, 64 MiB per artifact, 128 MiB total
hydrated per replay — a new `BoundedRecordDirResolver` scoped to ONLY
these two HTTP-triggered call sites, deliberately not applied to the
shared `RecordDirResolver` every CLI/operator path uses (a different risk
profile this round does not change).

### Part 4 — stable materialization_status values (CodeRabbit Major, folded into Part 2's own commit)

CodeRabbit `3702978278` (Major) + `3702978266` (Minor), same function.
`candidate_projection` built its status string from raw error `Display`
output — `ReplayError::ArtifactUnresolved`/`ArtifactDigestMismatch` carry
absolute server filesystem locators, sent verbatim to the HTTP client, and
the value vocabulary (four prefixes) didn't match its own documentation
(three). **Fixed**: exactly four stable, bare values —
`materialized`/`not_applicable`/`failed`/`verification_failed` — never an
interpolated error/path/locator; details go to server-side `eprintln!`
only. Distinguishes a missing record (`events.jsonl` `NotFound` →
`not_applicable`) from any OTHER I/O error (`failed`) from a malformed/
replay-invalid record (`verification_failed`).

### Part 3 — relative runs_dir/worktree_root (CodeRabbit Major)

`3702978320` (`src/worktree.rs:296`). Reproduced with real git
(`git -C <worktree> apply <relative-path-valid-elsewhere>` fails to find
the file) before fixing — and, investigating further, found the SAME
defect class ALSO breaks `worktree::add`/`worktree::remove` (used by
`worktree::add` for EVERY worktree creation, both fresh runs and
continuations), a more fundamental break than the originally-flagged
`apply_candidate_patch` alone: a relative `--worktree-root` (this CLI's
own DEFAULT value, not an edge case) resolved against `repo`'s own cwd,
not the caller's, in `run_git(repo, [...])`.

**Fix**: `apply_candidate_patch`'s private temp-patch directory resolves
its own real absolute path via `/proc/self/fd` on the already-opened,
`O_NOFOLLOW`-verified directory descriptor — never a fresh `canonicalize`
of caller-supplied input, which would reopen the confinement question the
private store exists to close. `worktree::add`/`remove` operate on paths
that are entirely operator/CLI-constructed (never derived from a hostile
base commit's own tree), so a plain lexical `std::path::absolute` is the
correct, simpler fix there. All three pass the path as `&OsStr` via a new
`run_git_with_path_args` helper, never through `to_string_lossy`.

### Part 5 — structural sealed-parent check (CodeRabbit correctness finding)

`3702978314` (`src/main.rs:937`). The substring scan
(`line.contains("\"type\":\"run_sealed\"")`) is replaced by a structural
comparison on the already-parsed `Vec<RunEvent>`
`load_verified_candidate_receipt` now returns (no second `events.jsonl`
read). Investigated, not merely fixed: proved via direct code inspection
that this check is 100% redundant with the PRECEDING `verdict.is_some()`
check — the reducer's own `EventAfterSeal` rule structurally forbids any
event after `RunSealed`, and `state.verdict` is only ever set from inside
`RunSealed`'s own reducer arm, so a record that already passed full
`verify_prefix` with `verdict.is_some()` has, by construction, its last
event equal to `RunSealed`. Kept as explicit, cheap, structural defense in
depth anyway, consistent with this codebase's own established pattern; no
adversarial test can isolate the new check from the preceding one for the
same reason (disclosed, not fabricated around).

### Part 6 — command-binding canonical locator (CodeRabbit nitpick, semantic authority)

Nitpick on `crates/o7-run/src/candidate.rs:159-185`. Every other candidate
artifact already went through `require_locator`; the command-binding
artifact was the one remaining exception. Added `COMMAND_BINDING_LOCATOR`
and enforced it. Confirmed by inspection, not assumed: the production
writer (`src/record.rs`'s `COMMAND_BINDING_FILE` constant) has only ever
written this exact name — no legacy-record compatibility concern.

### Part 7 — continuation diff base alignment (CodeRabbit nitpick)

Nitpick on `src/main.rs:1954`. `diff.patch` (evidence-only) was computed
against the CLI `--base` flag while the worktree/`candidate.patch`/
`RunMeta.base_commit` all already used the INHERITED candidate obligation.
Fixed to use the same inherited base; `ContinueArgs.base`'s own doc
comment (stale since before Q-Deck A0 — it claimed the flag selects the
child worktree base and that parent changes are not carried forward, both
false since A0) corrected to state it is accepted for CLI compatibility
but genuinely ignored on the continuation path.

### Part 8 — durability triage

**8A, fixed**: nitpick on `src/record.rs:263-282`. `sync_data()` makes a
file's contents durable but says nothing about the containing directory's
own entry. All four durable-write call sites (`write_task_durable`,
`write_bytes_durable`, `LedgerBinding::write_durable`,
`CommandBinding::write_durable`) now also `fsync` the run directory itself
after the file-level sync — Linux-only (the only platform this project
targets; every confinement primitive in `src/worktree.rs` already depends
on Linux-specific `rustix`/`O_NOFOLLOW`/`openat`).

**8B, deferred and disclosed, not attempted unsafely**: nitpick on
`src/worktree.rs:250-258` (stale `apply-input.<pid>.<n>` temp files after
a hard crash). A safe cleanup needs a lock/lease mechanism to avoid racing
a still-live process's own file — building that correctly was judged to
meaningfully widen this round's scope. No age-only or PID-only heuristic
delete was implemented, per the round's own explicit instruction not to
let this nitpick produce an unsafe cleanup. Follow-up scope: a
`flock`/lease-based reaper scoped only to `.o7-candidate-tmp`, bounded by
count/bytes, run opportunistically (e.g. at `o7d` startup or via `o7
recover`) — not implemented this round.

### Part 9 — seven cheap, confirmed cleanups

1. `command_idempotency_key_seen`'s doc comment had been inserted between
   `create_command`'s own doc block and its signature (`crates/o7-ledger/src/sqlite.rs`),
   leaving `create_command` with no doc comment and both functions'
   `# Errors` sections misattributed. Relocated with its own doc to after
   `create_command`.
2. Three `candidate.patch` reads in `tests/a0_candidate_state_e2e.rs` now
   use `std::fs::read`, not `read_to_string` — the spec states this
   transport is not guaranteed valid UTF-8.
3. The one test producing a genuine sealed `Fail` verdict now explicitly
   asserts a non-zero exit code, instead of discarding it.
4. Two stale `§13 Part N` doc cross-references corrected to `§14 Part N`.
5. Two illustrative plain-text fenced code blocks now declare `text`.
6. `patch_kind` cross-binding: replaced a vague comment with an honest
   accounting — `CandidatePatchKind` has exactly one variant with
   deliberately no `#[serde(other)]` catch-all, so any receipt naming a
   different `patch_kind` fails at DESERIALIZATION, a different and
   earlier failure point than the comparison line itself; a genuinely
   isolating test would require either fabricating a test that only
   re-proves unknown-JSON rejection (already covered) or adding a
   production-only-for-testing second enum variant (rejected — weakens
   the type's own intentional design).
7. Already fixed as part of Part 2/4's own commit: unreadable
   `events.jsonl` no longer classifies as `not_applicable` (only
   `NotFound` does).

### Part 10 — the rustix dependency claim: FALSE POSITIVE, confirmed with evidence

CodeRabbit `3702978257` (Cargo.toml:85) claimed `rustix` is "not declared
in any Cargo.toml and no Cargo.lock entry is present." Verified directly:
`rustix` IS declared in the ROOT `Cargo.toml` (line 85) AND in
`crates/o7-worktree/Cargo.toml`, `crates/o7-worker/Cargo.toml`,
`crates/o7-verifier/Cargo.toml`; `Cargo.lock` has a complete resolved
entry (`rustix v1.1.4`, full checksum, dependency list);
`cargo tree -i rustix` shows real dependency edges from `o7` and
`o7-worktree`. `git log -p` confirms it was added to the root manifest
back in corrective round 1 (the P6 symlink-escape fix), with its own
commit message explicitly noting it reuses the already-in-tree,
`deny.toml`-allowed dependency `o7-worktree` already declared. No
dependency change made — the claim does not match this repository's own
state at the reviewed head, and the build has depended on it successfully
across every round since.

**Commit shape** (9 additive, on top of round 4's frozen head
`6b77b9ac54f5b1e7583b63bee433b141fce44d3a`):
`fix(worktree): reject dirty submodule contents`,
`fix(o7d): bound candidate replay off async workers`,
`fix(worktree): anchor relative candidate patch paths`,
`fix(root): use structural sealed-parent evidence`,
`fix(replay): enforce command-binding locator`,
`fix(root): align continuation diff base with inherited candidate base`,
`fix(root): fsync run directory after durable artifact writes`,
`test(q-deck): cover external round 5 findings`,
`docs(q-deck): record round 5 evidence and triage`. Fewer commits than
the round's own suggested 10-commit sequence (one part per numbered Part
1-10 above): Parts 2 and 4 (bounding off
async workers, stabilizing projection statuses) touch the exact same
lines of the exact same function — CodeRabbit's own two findings were on
that same function — so they were committed together rather than split
artificially; disclosed here rather than silently deviating.

## 23. Corrective round 5 — evidence

Base for this round: the PR's own exact re-gated head,
`6b77b9ac54f5b1e7583b63bee433b141fce44d3a` (corrective round 4's final
commit, the same head both Codex review `4842896076` and CodeRabbit
review `4842910559` reviewed). New head: see the PR body / `git log`.

- `cargo fmt --check` — clean. `git diff --check` — clean.
- `cargo check -p o7 -p o7-run -p o7-ledger -p o7d` — clean.
- `cargo test -p o7-run` (lib + `contract` + `reducer_transitions` +
  `replay_acceptance` + `candidate_state`) — **128/128 passing** (28
  candidate_state — 27 carried + 1 new locator test; 10 contract; 72
  reducer_transitions; 18 replay_acceptance; 0 lib), unchanged except the
  one new Part 6 test.
- `cargo test -p o7-ledger --test commands` — 45/45, unchanged.
- `cargo test -p o7 --test r1_command_e2e` — **37/37 passing, unchanged**.
- `cargo test -p o7 --test a0_candidate_state_e2e` — **29/29 passing** (24
  carried from round 4, 5 new): dirty-submodule capture failure + no
  continuation-parent path (Part 1); oversized-`events.jsonl`
  fail-closed at both call sites + large-replay-does-not-block-health
  (Part 2); relative-runs_dir/worktree_root continuation succeeds (Part
  3); continuation diff base matches the inherited candidate base despite
  a deliberately wrong `--base` flag (Part 7).
- `src/worktree.rs`'s own unit test modules — **10 new** (7
  `dirty_submodule_tests`, 3 `special_runs_dir_path_tests`): clean/
  deinitialized submodules pass; dirty tracked/untracked/nested
  submodules rejected; a `.gitmodules` `ignore=all` bypass defeated;
  `capture_cumulative_candidate` itself fails closed without corrupting
  the original checkout; a `runs_dir` path containing a space or
  non-UTF-8 bytes handled correctly; a symlink at the private temp
  store's own name still refused.
- `src/record.rs`'s own new `durability_tests` module — **3 new**:
  `sync_dir` succeeds for a real directory, fails closed for a
  nonexistent one; all four durable-write call sites still produce
  correct content after the added directory sync.
- `cargo clippy --workspace --all-targets -- -D warnings` — clean, 0
  warnings, across the whole workspace.
- `cargo test --workspace --no-fail-fast` — every crate green, run
  single-threaded, same six pre-existing environmental exclusions
  carried forward unchanged, launched detached from this VPS's own
  foreground-command time ceiling (as prior rounds disclosed) — all 97
  `test result:` lines report `0 failed`.
- `cargo deny check advisories bans licenses sources` — the SAME
  `lz4_flex`/`RUSTSEC-2026-0041`/`CVSS:4.0` parse failure disclosed since
  round 1 (this installed `cargo-deny 0.18.2` predates CVSS v4.0 support
  in its bundled parser). Confirmed unrelated to this round's code: `git
  diff --stat 6b77b9a..HEAD -- Cargo.lock Cargo.toml
  crates/*/Cargo.toml` is empty.
- `npm test`/`npm run check` (`apps/q-deck`) — 45/45 / 0 errors,
  unchanged (frontend untouched this round).

### Provider invocation counts, this round

- Dirty-submodule case: provider invoked exactly once (capture happens
  strictly AFTER provider execution; the failure is not a redispatch);
  the unsealed record's subsequent admission-preflight rejection adds
  zero further invocations.
- Oversized-`events.jsonl` case: zero additional invocations at
  admission (409, zero command rows).
- Relative-runs_dir/diff-base-alignment continuations: normal, expected
  invocation counts (one per run/continuation), unchanged from the
  equivalent absolute-path/default-`--base` cases.

### Known limitations (disclosed, not hidden)

Everything §19/§21 already disclosed remains open. New this round: 8B
(stale temp-file cleanup) is explicitly deferred, not fixed — see Part 8
above for the exact follow-up scope. The Part 5 structural check's own
adversarial-test requirement is unreachable by construction (proven, not
assumed) — the existing unsealed-parent test suite already covers the
only reachable manifestation. Part 9 item 6 (`patch_kind` coverage) is
proven indirectly for the same class of reason — `CandidatePatchKind`'s
own single-variant, no-catch-all design means the comparison line cannot
be independently exercised without either weakening that design or
fabricating a test that proves something else. Part 2's "task panic/join
failure fails safely" requirement is proven by the code's own structure
(the `.map_err`/best-effort-swallow paths exist and are exercised by
every existing passing test that reaches them) rather than a dedicated
fault-injection test — deliberately not engineered given the fragility of
reliably triggering a genuine panic inside replay code without
introducing an artificial injection point.

### Clean worktree confirmation

`git status --short` shows no unstaged/untracked changes beyond what this
round's own commits already captured; every gate above was run against
the exact commit this round's final push carries.

### Addendum — one exact-head Actions failure, found and fixed

The exact-head Actions run for `353c0a6` (`o7-worker gate`, run
`30815288241`) failed:
`worktree::dirty_submodule_tests::nested_dirty_submodule_is_rejected`
panicked with `"empty ident name"` while committing inside the OUTER
submodule's own checkout (`dir/sub`). `git submodule add` clones into a
genuinely separate git config scope that does NOT inherit the upstream
repo's own local identity config — this development VPS happens to have
a global git identity configured, masking the gap locally every time
this suite ran here, while a stock GitHub Actions runner has none. Fixed
in `c9afb84` by setting `user.email`/`user.name` locally for that
checkout, exactly like every other repo this test module creates —
confirmed test-only, no production code involved. All 101 `o7` lib unit
tests pass locally with the fix; the exact-head Actions run for `c9afb84`
(`o7-worker gate` run `30816233861`, `pr3 worktree+verifier tests` run
`30816233751`) both succeeded.

## 24. Corrective round 6 — independent re-gate BLOCKED, response and evidence

Round 5's own head (`c979bc181b922c7052f9a1730d9bcd0cc67bb13d`) was
**BLOCKED**, not accepted, by an independent exact-head re-gate:
`https://github.com/PhysShell/007/pull/92#issuecomment-5167813513`, four
counterexamples. This section reproduces each at the old head, fixes it,
and reports the new head's own gate results. No thread was resolved or
reopened; the verdict comment itself was not edited.

### Part 1 — semaphore permit did not outlive its blocking job (MAJOR); admission queued unboundedly (MINOR availability)

Reproduced at `c979bc1` with a standalone harness using the REAL
`tokio::sync::Semaphore`/`spawn_blocking` primitives in the exact
structural shape `get_run`'s projection used: an `OwnedSemaphorePermit`
acquired in the async caller's own frame, NOT moved into the
`spawn_blocking` closure. Racing many callers against short timeouts
(simulating client cancellation) measured **40 concurrent blocking jobs
against a declared limit of 4** — the permit released the instant the
cancelled caller's frame dropped, while the already-spawned closure kept
running regardless. Separately, the admission path's
`acquire_owned().await` was proven to have no bound at all: 200/200
concurrent synthetic requests eventually acquired a permit and none were
ever rejected, the opposite of the "bounded queuing depth" the old
comment claimed.

Fix: centralized the pattern into one function,
`routes::run_behind_replay_limiter`, that both `get_run`'s projection and
`create_command`'s admission preflight now call — `try_acquire_owned`
(never `.await`), and the permit is moved INTO the `spawn_blocking`
closure before it is ever awaited, so it lives exactly as long as the
blocking work does. Admission converts from `acquire_owned().await` to
this same fail-fast primitive: saturation now returns a `503` with the
new stable code `CANDIDATE_REPLAY_BUSY` (optional `Retry-After: 1`),
before any command row/child id/worktree/provider invocation — an
idempotent replay of an already-recorded request never reaches this
block at all (guarded by the existing `already_seen` check), so retrying
an accepted command is unaffected.

New tests (`crates/o7d/src/routes.rs::replay_limiter_tests`, all against
the real primitive, none of which even compile against `c979bc1` since
the function doesn't exist there): mass-cancellation concurrency bound
(≤4 even racing 80 callers against short timeouts); saturated limiter
returns `None` and never runs the work (zero mutations); a panicking
closure still releases its permit and reports a join error; normal path
runs once and returns correctly.

### Part 2 — bounded reader was TOCTOU-vulnerable to a symlink (MAJOR)

Reproduced at `c979bc1` by extracting `read_bounded_events_jsonl`'s exact
logic and pointing `events.jsonl` at a symlink to `/dev/zero`: character
devices report `metadata().len() == 0`, passing the size gate, and the
subsequent `read_to_string` follows the symlink and reads unboundedly
(proven safely via `ulimit -v` + `timeout`, never risking this shared
VPS's own memory).

Fix: every read in `crates/o7d/src/canonical.rs` now goes through an
opened, no-follow file descriptor — `open_record_dir` opens the record
directory itself as an `O_NOFOLLOW|O_DIRECTORY` descriptor (via
`rustix`, newly added to `o7d`'s manifest: `nix`, already a dependency
here, only exposes a bare `RawFd` from `openat`, and turning that into an
owned `File` needs `unsafe`, which this crate's own
`forbid(unsafe_code)` lint forbids — `rustix`'s `OwnedFd`-based API,
already a workspace dependency via the root crate, converts to `File`
with a safe `From` impl); `events.jsonl` and every artifact locator open
`O_NOFOLLOW|O_NONBLOCK` relative to that descriptor (nested locators walk
each intermediate component as its own no-follow directory descriptor,
never `base.join(locator)`); `fstat` runs on the EXACT opened descriptor,
never a separate path-based `stat`; the read itself is capped at
`limit + 1` bytes through that SAME descriptor, rejecting if more than
`limit` bytes are actually observed (catches post-`fstat` growth, a
sparse file's oversized logical length, or a stale declared size) —
inclusive at the boundary (exactly `limit` bytes is accepted). The
running total-hydrated-bytes counter uses `checked_add` via
`fetch_update`, never a plain `fetch_add` that could silently wrap; a
repeated reference to the same artifact is never deduplicated — it
always consumes budget again. Existing limits (8 MiB events.jsonl / 64
MiB per-artifact / 128 MiB total) are unchanged. Projection/admission
error-mapping semantics (not_applicable/failed/verification_failed;
409 with zero mutations) are preserved exactly.

New tests (`crates/o7d/src/canonical.rs::bounded_read_tests`, 14, all
passing): symlink-to-`/dev/zero` for both `events.jsonl` and an artifact,
each rejected quickly under an explicit timeout; a symlink to a
sparse (real but oversized) regular file rejected purely by `NOFOLLOW`;
a FIFO rejected without hanging; a Unix socket rejected; a real character
device (`/dev/null`) proven rejected by the file-type check itself, not
merely `NOFOLLOW`; a TOCTOU path-swap after the descriptor is already
open proven to have no effect on what gets read; post-`fstat` growth
rejected; a sparse oversized-logical-length file rejected before any
read; exact-limit accepted (both a small synthetic limit and the real
8 MiB `events.jsonl` constant, via a fast sparse file); `limit + 1`
rejected; a repeated artifact reference proven to keep consuming budget
(never bypasses the total); a seeded near-`u64::MAX` counter proven to
reject via `checked_add` rather than wrap; malformed/traversal locators
rejected before any I/O.

### Part 3 — porcelain-v2 `-z` rename/copy records misparsed (MINOR correctness, could reject a legitimate run)

Reproduced live at `c979bc1`: a real repo with a file named
`2 - Section overview.md`, renamed. The old parser split the entire NUL-
delimited stream and classified every resulting segment independently —
a type-2 (rename/copy) record's own SEPARATE original-path NUL field has
no type marker of its own, so `2 - Section overview.md` (the rename's old
name) was read as if it were its own type-2 record, whose third
whitespace-delimited field (`"Section"`) starts with `S` and isn't
`"S..."` — an unconditional false-positive dirty-submodule bail with no
submodule involved at all. Confirmed by running the exact extracted
function against the live repro: `BAIL: status field "Section" at path
"overview.md"`.

Fix: `check_no_dirty_submodule_status` (`src/worktree.rs`) is now a real
state machine over the five documented record shapes (`1`, `2`, `u`, `?`,
`!`). A type-2 record unconditionally consumes the immediately following
NUL field as its mandatory original path — never reinterpreting it,
regardless of what bytes it starts with — and rejects it (fails closed)
if that field is absent OR empty (a real original path is never empty;
this also correctly separates "genuinely truncated" from the mandatory
single trailing NUL every complete stream ends with). `<sub>` is read
only from the documented fixed-field position on `1`/`2` records. Any
record type outside the five documented shapes, or a `1`/`2` record
missing its fixed leading fields, fails closed with an internal error —
never silently treated as clean.

New tests: 12 byte-level fixtures
(`src/worktree.rs::porcelain_v2_parser_tests`) covering every record
type, truncated/empty type-2 original-path fields, an unknown record
type, malformed fixed fields, non-UTF-8 path bytes (proven not to
panic), embedded spaces/newlines in the original path (proven opaque),
and a stray interior empty record (fails closed, distinct from the one
legitimate trailing empty segment). 3 real-git tests
(`dirty_submodule_tests`): the exact `2 - Section overview.md`
counterexample; renames whose old name starts with each of the other
four record-type prefixes (` 1  `, ` u  `, ` ?  `, ` !  `); an ordinary untracked
superproject file (unaffected, confirming this check's scope stays
confined to submodule dirtiness). All prior dirty/clean-submodule tests
(round 3/5) still pass unchanged.

### Part 4 — no regression on prior rounds' closed findings

Every finding closed in rounds 1–5 remains closed and untouched by this
round's changes: dirty-submodule authority, `--ignore-submodules=none`,
clean/deinitialized submodule handling, gitlink mutation rejection,
relative `runs_dir`/`worktree_root`, the private no-follow temp store,
the stable DTO status vocabulary, no path/error leakage, the structural
`RunSealed` check, the command-binding locator, the continuation
diff-base alignment, directory fsync ordering, the unsupported-schema
rejection, repository/parent/child-id binding, and idempotency/legacy-row
repair — all exercised by the SAME existing test files (`r1_command_e2e`,
`a0_candidate_state_e2e`, `commands`, `contract`, `reducer_transitions`,
`replay_acceptance`, `candidate_state`), unmodified except where this
round's own new tests were added, all still passing.

### Gates, this round

- `cargo fmt --check` — clean. `git diff --check` — clean.
- `cargo check -p o7 -p o7-run -p o7-ledger -p o7d` — clean.
- `cargo test -p o7-run` (lib + `contract` + `reducer_transitions` +
  `replay_acceptance` + `candidate_state`) — all passing, unchanged
  except this round's own new coverage.
- `cargo test -p o7-ledger --test commands` — 45/45, unchanged.
- `cargo test -p o7 --test r1_command_e2e` — 37/37, unchanged.
- `cargo test -p o7 --test a0_candidate_state_e2e` — 29/29, unchanged.
- `cargo test -p o7d` — full suite green, including the 4 new
  `replay_limiter_tests` and 14 new `bounded_read_tests`.
- `cargo clippy --workspace --all-targets -- -D warnings` — clean (fixed
  two lints this round's own new test/production code introduced: a
  `record[..1]` slice in the new porcelain parser's error path,
  rewritten to bind the matched type byte instead of re-indexing; a
  `panic!()` inside the new panic-safety test, exempted via the same
  `#[allow(clippy::panic)]` pattern already used on similar fault-
  injection tests elsewhere in this codebase).
- `cargo test --workspace --no-fail-fast` — **818 tests passed, 0
  failed**, across 97 test-result blocks. The SAME standing, previously-
  disclosed environmental exclusions (§13/§15 and onward) were run
  explicitly excluded via `-- --skip <name>`, each independently
  reproduced hanging/failing again on THIS VPS AND confirmed to
  reproduce identically against the pristine `c979bc1` base (via
  `git stash`, not merely asserted) before being excluded — none touch
  any code this round changed: `kill_after_commit_preserves_event`,
  `kill_before_commit_leaves_no_partial`
  (`crates/o7-ledger/tests/crash_durability.rs` — hangs past a 60s
  timeout on both this round's head and the pristine base, identical
  down to the exit code);
  `a_blocking_fifo_target_fails_closed_within_a_bound` (times out at
  exactly ~5.0s on both this round's head and the pristine base, same
  panic message); `no_control_descriptor_leaks_to_a_concurrent_sibling`;
  `an_unexpectedly_launched_target_is_a_fail_not_the_refusal_pass`
  (`BackendObjectMismatch` — the previously-disclosed "sixth exclusion").
  No new exclusion was found this round.
- `cargo deny check advisories bans licenses sources` — the SAME
  `lz4_flex`/`RUSTSEC-2026-0041`/`CVSS:4.0` parse failure disclosed since
  round 1 (confirmed again after deleting and refetching the local
  advisory-db cache — the failure is in the installed `cargo-deny
  0.18.2`'s parser, not a stale cache); `bans`/`licenses`/`sources` all
  pass clean on their own.
- `npm test`/`npm run check` (`apps/q-deck`) — 45/45 passing / 0 errors,
  0 warnings — frontend untouched this round.

### Known limitations (disclosed, not hidden)

Everything previously disclosed (§19/§21/§23) remains open and unchanged
by this round.

### Clean worktree confirmation

`git status --short` shows no unstaged/untracked changes beyond what this
round's own commits capture; every gate above ran against the exact
commit this round's final push carries.

## 25. Corrective round 7 — fresh exact-head Codex P1, response and evidence

The PR was marked Ready for review on the round-6 head
(`994408c4e9582cf2fd0c9e2f9398f7a440fb563e`) to solicit a fresh
independent pass; a fresh exact-head Codex P1
(`#discussion_r3707367805`) found a genuine, previously-unaddressed
defect in candidate capture itself. The PR was reverted to Draft, per the
same standing protocol every prior corrective round has followed.

### The finding — index-hidden edits bypass candidate capture

`git add -A` (the very first line of `capture_cumulative_candidate`)
HONORS `assume-unchanged`/`skip-worktree` index entries: a tracked path
marked either way is left exactly as the index already has it, no
matter what its working-tree bytes now say. `git status`/`git diff
--cached` show NOTHING for such a path. A provider that edits a path it
(or an earlier turn) marked this way therefore produces an EMPTY
cumulative patch and a tree OID identical to `base_commit`'s own — the
run seals with a candidate receipt that reads as a complete no-op while
the edit is silently discarded, exactly the "run-record integrity"
concern AGENTS.md §3 (L58-L61) names.

**Reproduced at the exact old head, through the REAL production
function** (not a synthetic fixture): a fresh repo, a tracked file
committed, `git update-index --assume-unchanged tracked.txt`, then the
file edited on disk. `capture_cumulative_candidate` returned
`Ok((vec![], base_commit's own tree))` — confirmed identical for
`--skip-worktree`. `git ls-files -v -z` is the authority for detecting
either flag: `-v`'s status letter (`H`/`S`/`M`/`R`/`C`/`K` — never
otherwise lowercase on their own) is lowered for an assume-unchanged
entry, and `S`/`s` specifically marks skip-worktree (lowercase again
when both are set); verified live with nested paths, paths containing
spaces, and non-UTF-8 path bytes, all parsed correctly through `-z`'s
NUL-delimited raw-byte records.

### Fix

`ensure_no_index_hidden_flags` (`src/worktree.rs`) runs `git ls-files -v
-z`, parses each NUL-delimited record as a fixed `<letter><space><path>`
raw-byte layout — never re-split on spaces, never reinterpreted as
UTF-8 for comparison (only the final error message formats the path
lossily for display, like every other diagnostic in this file) — and
fails closed on ANY entry carrying either flag. This is a deliberate,
documented, CONSERVATIVE blanket policy: it does not attempt to prove
the flagged entry's current blob actually differs from its working-tree
bytes (that would mean re-reading the flagged path's own content,
reopening the identical TOCTOU window this check exists to close).
Sparse-checkout/`skip-worktree` and `assume-unchanged` worktrees are
UNSUPPORTED for candidate capture by design — documented as a
limitation, never claimed as supported. The check is READ-ONLY: it
never calls `update-index` to clear or mutate either flag, and never
touches the working tree — a rejection leaves the original checkout and
index exactly as found.

Wired into BOTH `capture_cumulative_candidate` (run right after `add
-A`, as close as practical to the authoritative index capture — the
flags are orthogonal to staging, so this catches them regardless of
when they were set) AND `finish_apply` (materialization's own
`write-tree`), mirroring the existing precedent that
`ensure_no_dirty_submodule_worktree`/`ensure_no_gitlink_mutation` are
already duplicated at both sites.

**Audit for other/later code paths (explicitly requested this round):**
`diff_vs_base` (`src/worktree.rs`) shares the identical `add -A` + `diff
--cached` structure, but its own output feeds ONLY R1's general,
non-authoritative `diff.patch` evidence artifact (recorded via
`PatchCaptured`, `src/main.rs`) — never A0's own candidate-state
continuation authority (`candidate.patch`/`candidate_tree_oid`, produced
exclusively by `capture_cumulative_candidate`). Disclosed here as a
known, structurally-related, OUT-OF-SCOPE limitation for a future round,
not silently fixed beyond this round's actual mandate and not silently
omitted from the audit either. `finish_apply`'s own `write-tree` cannot
itself be reached by a hostile candidate PATCH (`git apply` only ever
touches the named paths' content, never `update-index` flag bits) — the
defense-in-depth duplication there protects the worktree's REUSE across
the next continuation's own agent turn, not a gap in THIS round's
specific threat model.

### Tests

`src/worktree.rs`, 8 new unit tests (real Git, no synthetic fixtures):
`assume-unchanged`/`skip-worktree` + a modified tracked file both reject
(the exact live counterexample); a nested path; a path with spaces and
an embedded newline byte; a non-UTF-8 path (Unix, proven not to panic);
an ordinary tracked edit with no hidden-index flags still captures; a
plain untracked file still captures; a rejection leaves BOTH the
working-tree bytes and the index flag itself untouched (proven by
re-reading `git ls-files -v` after the rejected call). All prior
dirty-submodule (10) and gitlink-policy (8) tests, and all 12 porcelain-
v2 parser tests, still pass unchanged.

`tests/a0_candidate_state_e2e.rs`, 1 new process-level test —
`an_index_hidden_edit_fails_candidate_capture_and_can_never_become_a_continuation_parent`
— mirrors the round-5 dirty-submodule e2e test exactly: a real `o7 run
--ledger` invocation, the provider marks `README.md` assume-unchanged
then edits it; the process exits non-zero, `RunSealed`/
`CandidateStateCaptured` never appear in the canonical record, the
provider is invoked exactly once (not a redispatch), the tampered
content AND the assume-unchanged flag both survive untouched, and a
follow-up command against this unsealed parent is rejected `409` with
zero command rows created.

### Gates (exact new head)

- `cargo fmt --check` — clean. `git diff --check` — clean.
- `cargo check -p o7 -p o7-run -p o7-ledger -p o7d` — clean.
- `cargo test -p o7-run` (`contract` 10/10, `reducer_transitions` 72/72,
  `replay_acceptance` 18/18, `candidate_state` 28/28) — unchanged.
- `cargo test -p o7-ledger --test commands` — 45/45, unchanged.
- `cargo test -p o7 --test r1_command_e2e` — 37/37, unchanged.
- `cargo test -p o7 --test a0_candidate_state_e2e` — **30/30** (29
  carried from round 6 + 1 new).
- `cargo test -p o7d` — full suite green, unchanged.
- `cargo clippy --workspace --all-targets -- -D warnings` — clean.
- `cargo test --workspace --no-fail-fast` — **827 tests passed, 0
  failed** (818 carried from round 6 + 9 new), 97 test-result blocks.
  The SAME standing, previously-disclosed environmental exclusions
  carried forward unchanged, run explicitly excluded via `-- --skip
  <name>`: `kill_after_commit_preserves_event`/
  `kill_before_commit_leaves_no_partial`
  (`crash_durability.rs`), `a_blocking_fifo_target_fails_closed_within_a_bound`,
  `no_control_descriptor_leaks_to_a_concurrent_sibling`,
  `an_unexpectedly_launched_target_is_a_fail_not_the_refusal_pass`
  (`BackendObjectMismatch`) — none touch any code this round changed. No
  new exclusion found this round.
- `cargo deny check advisories bans licenses sources` — the SAME
  `lz4_flex`/RUSTSEC-2026-0041/CVSS:4.0 parser-version mismatch
  disclosed since round 1; `bans`/`licenses`/`sources` all pass clean
  independently.
- `npm test` — 45/45. `npm run check` — 0 errors, 0 warnings. Frontend
  untouched this round.

### Known limitations (disclosed, not hidden)

Everything previously disclosed (§19/§21/§23) remains open and
unchanged. New this round: `diff_vs_base`'s own non-authoritative
`diff.patch` evidence artifact shares the identical structural gap
(see "Audit" above) — disclosed, not fixed, out of this round's own
scope (candidate CAPTURE specifically).

### Clean worktree confirmation

`git status --short` shows no unstaged/untracked changes beyond what
this round's own commits capture; every gate above ran against the
exact commit this round's final push carries.

## 26. Corrective round 8 — fresh exact-head Codex P1s and CodeRabbit findings, response and evidence

The PR was marked Ready for review on the round-7 head
(`f116c0ca02fef7cd568d337af57ae5b41b972b3c`) to solicit a fresh
independent pass, per the same standing protocol every prior
corrective round has followed; a fresh exact-head Codex pass found TWO
genuine P1s and a fresh CodeRabbit pass found several policy/accuracy
defects. The PR was reverted to Draft and this round worked
additively and forward-only on top of `f116c0c`.

### P1 #1 — patch/tree race (`#discussion_r3710144125`)

`capture_cumulative_candidate` read `git diff --cached` and
`git write-tree` as two SEPARATE subprocess invocations against the
SAME live `.git/index` file. A background process mutating the index
between those two calls desynchronizes the returned patch (reflecting
the OLDER index state) from the returned `candidate_tree_oid`
(reflecting the NEWER, mutated state) — the receipt still passes
captured-evidence verification, but a later real materialization
rejects because the patch-derived tree and the recorded tree disagree.

**Reproduced at the exact old head, through the real production
function** (not a synthetic parser test): a deterministic,
`std::sync::mpsc::channel`-based barrier — never a probabilistic
sleep-based race — hands control to a real background `git` process at
the exact instant between the (then-separate) diff and write-tree
calls, covering ordinary tracked content, deletion/executable-bit
changes, binary content, and non-UTF-8 path bytes on Unix. All four
proved: the returned patch reflects state A, the returned tree
reflects state B, and `apply(patch, base_commit).tree_oid !=
candidate_tree_oid`.

**Fix:** both reads now go through ONE frozen, privately-created index
snapshot (`GIT_INDEX_FILE` pointed at a `PrivateTempFile` — the same
O_EXCL/O_NOFOLLOW/mode-0600/unique-name pattern `apply_candidate_patch`
already established, generalized into a reusable struct with a `Drop`
cleanup impl), taken immediately after the existing hidden-index-flag
and dirty-submodule checks pass. A live-index mutation after the
snapshot is taken can no longer affect either the patch or the tree —
proven by converting the same four adversarial reproductions into
permanent regression tests (now named `r8_patch_tree_race_is_closed_*`)
plus one general, non-adversarial direct-invariant test
(`apply_of_captured_patch_always_matches_recorded_tree_oid`). The
isolated index cannot be redirected through a candidate-controlled
symlink or path (`PrivateTempFile`'s own `O_NOFOLLOW`/`O_EXCL`
creation); no path bytes pass through `String`/`to_string_lossy`
anywhere in the new code; empty/binary/deletion/chmod/non-UTF-8-name
patches, unchanged gitlinks, and ordinary untracked files all retain
their existing semantics (regression-tested); hidden-index-flag and
dirty-submodule checks remain enforced, run BEFORE the snapshot is
taken; failures emit no candidate receipt.

Exact capture-cutoff semantics are now documented in §4 above: the
patch and tree are always mutually consistent with EACH OTHER, but a
concurrent mutation of the real index between `add -A` finishing and
the snapshot being taken is a genuine race this capture makes no claim
about either way — never overclaimed as "concurrently modified bytes
are included."

### P1 #2 — missing/pruned base commit (`#discussion_r3710144132`)

`parent_candidate_state_usable` (the admission preflight) proved a
parent's candidate receipt was internally self-consistent with its own
declared contract, but never that the contract's own `base_commit`
still resolves as a real object in this server's configured
repository right now.

**Reproduced at the exact old head, through the real production
path:** a real A0 parent run created against a throwaway branch's own
commit; that branch AND the retaining `o7/<run_id>` branch
`worktree::add` itself leaves behind after ordinary worktree teardown
(discovered empirically — this is exactly the "retaining `o7/*`
branch" the finding's own wording names) both deleted; `git reflog
expire --expire=now --all`; real `git gc --prune=now`; `git cat-file
-e <sha>^{commit}` confirms the commit is genuinely gone. Driving
`POST /commands` with a fresh idempotency key against this parent:
admission incorrectly succeeded (`202`), the detached `o7 continue`
child crashed early inside its own `worktree::add` (never
synchronously observed — fire-and-forget dispatch, `spawn_continue`,
is unchanged, existing, correct-by-design behavior; the run simply
never progresses past pre-dispatch), the provider was never invoked,
and redriving the identical request reproduced the identical stuck
state.

**Fix:** `parent_candidate_state_usable` now calls the SAME hardened
primitive `main.rs`'s own materialization path already calls
(`worktree::verify_commit_exists`, kept there unchanged as defense in
depth — never removed), immediately after the existing
repository-identity check, inside this function's own existing bounded
`spawn_blocking` admission path (no new wiring needed — this function
already ran there since round 6). Mapped to the existing `409
COMMAND_PARENT_CANDIDATE_UNAVAILABLE` contract via the unchanged caller
in `routes.rs`, which already never leaks the underlying reason (a raw
`eprintln!` server-side log only) to the HTTP client. Zero command
rows, child runs, or provider invocations are created on this path —
structurally guaranteed by the existing `already_seen` guard, unchanged.

Both reproductions were converted into STRICT post-fix regression
tests via a new shared helper
(`assert_admission_preflight_rejects_with_zero_mutations`) that
asserts the tighter `409`-before-any-mutation contract specifically —
never the looser "`202` that only fails later" every pre-round-8
negative case had to settle for. A second, distinct test covers a
syntactically valid but NEVER-existed 40-hex commit OID (not the same
as a genuinely pruned one): both the parent's contract obligation and
its own receipt are rewritten to the same fabricated OID with a
correctly recomputed digest chain, isolating the test to exactly the
one new check this round adds. Regression coverage confirms: an
ordinary existing base still admits normally; foreign-repository
behavior is unchanged; an unknown parent still `404`s; a repeated
request with the same fresh key still creates zero mutations
(demonstrated twice, deterministically, against the pruned-base case);
`replay_limiter_tests` (limiter saturation, join-error-on-panic) is
untouched by this change and still passes unchanged.

### Fresh CodeRabbit findings

Restriction-lint invariant documentation added to all four flagged
`#[allow(...)]` test modules
(`crates/o7d/src/canonical.rs::bounded_read_tests`,
`crates/o7d/src/routes.rs::replay_limiter_tests`,
`src/worktree.rs::porcelain_v2_parser_tests`,
`src/record.rs::durability_tests`), each stating precisely what every
`unwrap`/`expect`/index/`panic!` in that module operates on (test-
constructed fixture data only) and why a panic there is an intentional
test failure, never a production fault path.

Documentation/test accuracy fixes: normative §4 now includes the
hidden-index-flag rejection step and the round-8 frozen-snapshot
capture-cutoff caveat; a stray "THIRD read" corrected to "SECOND read"
in a budget-overflow test whose own body only ever performs two reads;
the round-5 commit-count arithmetic corrected (9 actual commits is
FEWER than a 10-part suggested sequence — one merged pair, per Parts
1-10 above — not fewer than 8, which was never even smaller than 9);
the `cargo test -p o7-run` total reconciled from a stale `130/130` to
the true `128/128` sum of its own disclosed per-suite breakdown;
MD038-compliant padded code spans for the four porcelain-v2
record-type prefixes that still preserve their semantically meaningful
trailing spaces.

The concurrent-candidate-replay-does-not-block-the-runtime test
(`a_large_candidate_replay_does_not_block_concurrent_unrelated_requests`)
previously fired six unsynchronized client threads via a bare
`thread::spawn` loop, then immediately measured a health check against
an unused, never-read `barrier_start` timer — proving only that
requests were SENT, never that any blocking work was actually in
flight when the health check ran. Closed two ways: a real
`std::sync::Barrier` now releases all six client threads at the same
synchronized instant immediately before the health check fires
(replacing the dead timer entirely), and each thread's own response is
now inspected (previously discarded via `let _ = h.join()`) to prove
at least one shows a genuinely completed replay
(`materialization_status` present) rather than the best-effort
busy-skip `try_acquire_owned` (non-blocking) takes under saturation —
narrowing the test's own claim honestly rather than overclaiming
millisecond-exact overlap, which black-box OS thread scheduling cannot
prove without instrumenting production code.

Audited, not implemented (no independently reproduced correctness or
load-bearing availability defect found; classified as non-blocking
follow-up, consistent with this round's own explicit scope
constraint): separate admission/projection semaphores; replay caching;
helper-only test refactors; parent-directory fsync; porcelain-v2
u-record handling; dead continuation base fallback.

### Regression

Every prior closure carried forward unchanged and reconfirmed this
round: index-hidden-edit rejection; dirty tracked/untracked/nested
submodule rejection; unchanged gitlinks allowed, gitlink mutation
rejected; binary and non-UTF-8 candidate transport; porcelain-v2
type-2 original-path parsing; descriptor-based no-follow bounded
reads; replay permit lifetime and fail-fast saturation;
repository/parent/child/conversation/command bindings; relative
`runs_dir`/`worktree_root`; durable artifact ordering; A→B→C cumulative
continuity; legacy-parent and unsupported-schema rejection;
idempotency and pre-dispatch redrive semantics.

### Gates (exact new head)

- `cargo fmt --check` — clean. `git diff --check` — clean.
- `cargo check -p o7 -p o7-run -p o7-ledger -p o7d` — clean.
- `cargo test -p o7-run` — **128/128** (`lib` 0, `contract` 10/10,
  `reducer_transitions` 72/72, `replay_acceptance` 18/18,
  `candidate_state` 28/28), unchanged.
- `cargo test -p o7-ledger --test commands` — 45/45, unchanged.
- `cargo test -p o7 --test r1_command_e2e` — 37/37, unchanged. No R1
  fixture race observed this round.
- `cargo test -p o7 --test a0_candidate_state_e2e` — **32/32** (30
  carried from round 7 + 2 new).
- `cargo test -p o7d` — full suite green (59/59), unchanged.
- `cargo test --workspace --no-fail-fast` — completed via a per-crate
  decomposition rather than one monolithic invocation (this VPS is
  single-core with its main disk at 97% full; the monolithic run
  repeatedly exceeded practical wall-clock budgets without ever being a
  genuine hang in the code this round touches — confirmed by rerunning
  with progressively longer caps and watching it make real forward
  progress each time). Full accounting at the exact new head: `o7`
  220/220 (lib 130, `a0_candidate_state_e2e` 32, `live_ingress_e2e` 21,
  `r1_command_e2e` 37), `o7-run` 128/128, `o7-ledger` 112/112 (+2
  intentionally `#[ignore]`d subprocess-child markers), `o7-verifier`
  34/34, `o7-worktree` 54/54, `o7-worker` 181/181 (+45 pre-existing
  `#[ignore]`d Vertical-B/subprocess-helper placeholders, unrelated to
  this round), `o7d` 59/59 — **788 passed, 0 failed** across the
  workspace.

  Four tests excluded via `-- --skip <name>`, all independently
  reproduced as pre-existing by checking out the clean, unmodified
  `f116c0c` head (`git stash`/`git stash pop`) and observing the
  identical failure there — none are round-8 regressions, and none
  touch any file this round changed:
  `kill_after_commit_preserves_event`/`kill_before_commit_leaves_no_partial`
  (`crash_durability.rs` — a real SIGKILL-timing test whose child hangs
  before printing its own readiness line; most likely disk-pressure/
  fsync-related given the 97%-full disk) and
  `a_blocking_fifo_target_fails_closed_within_a_bound`/
  `no_control_descriptor_leaks_to_a_concurrent_sibling`
  (`sandboy_lifecycle.rs` — an internal timing bound too tight for this
  VPS's current load, and a genuine hang, respectively). All four were
  already disclosed as standing environmental exclusions in round 7's
  own §25 evidence, carried forward unchanged; this round additionally
  verified each one against the clean original head specifically,
  which round 7's own report did not do. Round 7's §25 disclosed a
  FIFTH standing exclusion,
  `an_unexpectedly_launched_target_is_a_fail_not_the_refusal_pass` —
  this round it passed cleanly without needing exclusion (VPS-condition-
  dependent; not something this round changed or fixed, disclosed
  honestly rather than silently rounding up).
- `cargo clippy --workspace --all-targets -- -D warnings` — clean, 0
  warnings.
- `npm test` (apps/q-deck) — 45/45. `npm run check` — 0 errors, 0
  warnings. Frontend untouched this round.
- `cargo deny check bans` — ok. `cargo deny check licenses` — ok.
  `cargo deny check sources` — ok. `cargo deny check advisories` —
  **blocked**, the SAME `lz4_flex`/RUSTSEC-2026-0041/CVSS:4.0
  parser-version mismatch disclosed since round 1; not called green.

### Known limitations (disclosed, not hidden)

Everything previously disclosed (§19/§21/§23/§25) remains open and
unchanged. `diff_vs_base`'s own non-authoritative `diff.patch`
evidence artifact still shares the structural gap round 7's own audit
disclosed (not A0's continuation authority) — unchanged, out of this
round's own scope.

### Clean worktree confirmation

`git status --short` shows no unstaged/untracked changes beyond what
this round's own commits capture; every gate above ran against the
exact commit this round's final push carries.
