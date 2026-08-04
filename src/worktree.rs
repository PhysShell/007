use anyhow::{Context, Result};
use std::collections::BTreeSet;
use std::ffi::CString;
use std::os::fd::OwnedFd;
use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use rustix::fs::{self, Mode, OFlags};

/// Q-Deck A0 corrective round 1: convert `o7-worktree`'s own canonical
/// repository identity into `o7-run`'s dependency-free mirror of it
/// (`docs/q-deck/a0-candidate-state.md` §2) — the one conversion point
/// between the two. Q-Deck A0 corrective round 3 (Codex P1, Part 4): moved
/// here, into the library, from being a `main.rs`-private helper — `o7d`
/// (a separate crate, depending on `o7` as a library) now needs the exact
/// same canonical identity for the configured `exec.repo`, to cross-check
/// against a parent's own candidate-state contract before admission.
///
/// # Errors
/// The repository's canonical identity cannot be resolved (not a Git
/// repository, or an I/O failure).
pub fn repository_identity(repo: &Path) -> Result<o7_run::event::RepositoryIdentity> {
    let canonical = o7_worktree::git::HardenedGit::new(repo)
        .canonical_repo_id()
        .context("resolving this repository's canonical identity")?;
    Ok(o7_run::event::RepositoryIdentity {
        git_common_dir: canonical.git_common_dir.to_string_lossy().into_owned(),
        dev: canonical.dev,
        ino: canonical.ino,
    })
}

/// Create a throwaway git worktree of `repo` at `base`, on a fresh branch.
///
/// # Errors
/// `path` cannot be resolved to an absolute path, or the underlying `git
/// worktree add` fails.
pub fn add(repo: &Path, base: &str, path: &Path, branch: &str) -> Result<()> {
    // Q-Deck A0 corrective round 5 (CodeRabbit Major, same defect class as
    // `apply_candidate_patch`'s own fix): this call runs with
    // `current_dir(repo)` — a RELATIVE `path` (built from a relative
    // `--worktree-root`) would be resolved by `git` itself against `repo`,
    // not the CALLING process's own cwd, whenever they differ. `path`
    // itself is entirely operator/CLI-constructed (never derived from a
    // hostile base commit's own tree content, unlike the private
    // candidate-patch temp store), so a plain LEXICAL absolutization
    // (`std::path::absolute`, no filesystem access, no existence
    // requirement) against the process's own real cwd is the correct fix
    // here — no confinement/symlink question is reopened by it. Passed as
    // `&OsStr` so a non-UTF-8 path is never truncated/mangled.
    let abs_path = std::path::absolute(path)
        .with_context(|| format!("resolving an absolute path for {}", path.display()))?;
    run_git_with_path_args(
        repo,
        &["worktree", "add", "-b", branch],
        abs_path.as_os_str(),
        &[base],
    )
    .with_context(|| format!("git worktree add at {}", path.display()))?;
    Ok(())
}

/// Remove a worktree (its branch is left dangling; fine for throwaway runs).
///
/// # Errors
/// `path` cannot be resolved to an absolute path, or the underlying `git
/// worktree remove` fails.
pub fn remove(repo: &Path, path: &Path) -> Result<()> {
    // Q-Deck A0 corrective round 5: same fix as `add` above.
    let abs_path = std::path::absolute(path)
        .with_context(|| format!("resolving an absolute path for {}", path.display()))?;
    run_git_with_path_args(
        repo,
        &["worktree", "remove", "--force"],
        abs_path.as_os_str(),
        &[],
    )?;
    Ok(())
}

/// Stage everything the agent produced and diff it against `base`.
/// Staging (`add -A`) is what makes untracked new files show up in the patch.
pub fn diff_vs_base(worktree: &Path, base: &str) -> Result<String> {
    run_git(worktree, &["add", "-A"])?;
    run_git(worktree, &["diff", "--cached", base])
}

/// Resolve a ref to a commit sha.
pub fn rev_parse(dir: &Path, refname: &str) -> Result<String> {
    Ok(run_git(dir, &["rev-parse", refname])?.trim().to_string())
}

/// Q-Deck A0: verify `commit` names a real, existing commit OBJECT in this
/// repository — unlike a plain `rev-parse`, which happily echoes back any
/// sha-shaped string without proving the object actually exists.
///
/// # Errors
/// `commit` does not resolve to an existing commit object.
pub fn verify_commit_exists(dir: &Path, commit: &str) -> Result<()> {
    run_git(dir, &["cat-file", "-e", &format!("{commit}^{{commit}}")])?;
    Ok(())
}

/// A gitlink (submodule, mode `160000`) tree entry: its exact path as RAW
/// bytes (never a lossy UTF-8 string — a path containing invalid UTF-8 or
/// unusual bytes must still be authoritative) plus the object OID it
/// points at.
type GitlinkEntry = (Vec<u8>, String);

/// Q-Deck A0 corrective round 3 (Codex P1, Part 2): the exact SET of
/// gitlink (submodule, mode `160000`) entries in `commit`'s own tree,
/// keyed by (raw path bytes, object OID) — the authority
/// [`ensure_no_gitlink_mutation`] compares between the base and the
/// resulting candidate tree. Uses `git ls-tree -r -z` (NUL-delimited
/// records, Git's own machine-readable output mode) so a path containing
/// whitespace, newlines, or non-UTF-8 bytes is parsed exactly, never
/// corrupted or misread the way a lossy UTF-8 text scan would.
///
/// # Errors
/// Any underlying `git` failure.
fn gitlink_entries(dir: &Path, commit: &str) -> Result<BTreeSet<GitlinkEntry>> {
    let out = run_git_bytes(dir, &["ls-tree", "-r", "-z", commit])?;
    let mut entries = BTreeSet::new();
    for record in out.split(|&b| b == 0) {
        if record.is_empty() {
            continue;
        }
        // Git's own `ls-tree` record shape: "<mode> SP <type> SP <oid> TAB <path>".
        let Some(tab) = record.iter().position(|&b| b == b'\t') else {
            continue;
        };
        let (header, path_with_tab) = record.split_at(tab);
        let path = path_with_tab.get(1..).unwrap_or_default().to_vec();
        let header = String::from_utf8_lossy(header);
        let mut fields = header.splitn(3, ' ');
        let mode = fields.next().unwrap_or_default();
        let _object_type = fields.next();
        let oid = fields.next().unwrap_or_default();
        if mode == "160000" {
            entries.insert((path, oid.to_owned()));
        }
    }
    Ok(entries)
}

/// Q-Deck A0 corrective round 3 (Codex P1, Part 2): the FROZEN gitlink
/// policy, enforced as an authoritative set comparison rather than a
/// whole-tree "contains any gitlink" check — an unchanged gitlink already
/// present in `base_commit` is explicitly ALLOWED (the original whole-tree
/// check incorrectly rejected this, failing candidate capture for every
/// ledger-backed run against a repository with an untouched submodule);
/// every ADDED, DELETED, or OID-CHANGED gitlink between `base_commit` and
/// `candidate` (a commit-ish OR a raw tree OID — `git ls-tree` accepts
/// both) is REJECTED, including a mode transition to/from `160000` (a
/// regular file replaced by a gitlink, or vice versa, changes which set
/// contains that path, so it is caught the same way).
///
/// # Errors
/// Any underlying `git` failure, or the candidate's own gitlink set
/// disagrees with the base's.
fn ensure_no_gitlink_mutation(dir: &Path, base_commit: &str, candidate: &str) -> Result<()> {
    let base = gitlink_entries(dir, base_commit)?;
    let candidate_entries = gitlink_entries(dir, candidate)?;
    if base != candidate_entries {
        let added: Vec<_> = candidate_entries.difference(&base).collect();
        let removed: Vec<_> = base.difference(&candidate_entries).collect();
        anyhow::bail!(
            "the candidate tree mutates a gitlink (submodule) entry relative to the base \
             commit — unsupported (added: {}, removed/changed: {})",
            added.len(),
            removed.len()
        );
    }
    Ok(())
}

/// Q-Deck A0 corrective round 5 (Codex P1, Part 1): an authoritative check
/// for DIRTY content inside any submodule — `ensure_no_gitlink_mutation`'s
/// pure tree-to-tree comparison can never see this, because a dirty
/// submodule working tree (a tracked file edited but not committed inside
/// it, or a new untracked file) never changes the SUPERPROJECT's own
/// committed gitlink entry: `git add -A` at the superproject level cannot
/// stage submodule-internal changes short of the submodule's own `HEAD`
/// actually moving. Without this check, a provider editing a file inside an
/// already-initialized submodule produces a cumulative patch that is
/// byte-for-byte silent about the edit, and a candidate receipt that reads
/// as complete — the next continuation then materializes the OLD submodule
/// content, permanently and silently losing the change.
///
/// `git status --porcelain=2 -z --ignore-submodules=none -uall` is the
/// authority: it recurses into every INITIALIZED submodule (including
/// nested ones — verified empirically, a dirty nested submodule surfaces
/// as dirtiness on its own top-level gitlink entry) and, for every entry
/// that is a submodule, reports a 4-character `S<c><m><u>` field: `c`='C'
/// iff its own `HEAD` commit changed (redundant with
/// [`ensure_no_gitlink_mutation`]'s own comparison, checked here too for
/// defense in depth), `m`='M' iff it has tracked modifications, `u`='U' iff
/// it has untracked content. A DEINITIALIZED submodule (no working tree at
/// all) or a genuinely clean initialized one never appears in this output
/// at all — both are explicitly allowed. `-z` (NUL-delimited records) and
/// byte-level field parsing (never a lossy UTF-8 text scan or substring
/// match) make this authoritative rather than heuristic, exactly like
/// [`gitlink_entries`]. `--ignore-submodules=none` is REQUIRED, not
/// decorative: verified empirically that a `.gitmodules` entry declaring
/// `submodule.<name>.ignore = all` (an attacker-reachable, tracked file)
/// silently hides real dirtiness from a plain `git status` call with no
/// override — `--ignore-submodules=none` on the command line takes
/// precedence over that setting.
///
/// # Errors
/// Any underlying `git` failure, or any submodule entry reports tracked,
/// untracked, or commit-level dirtiness.
fn ensure_no_dirty_submodule_worktree(dir: &Path) -> Result<()> {
    let out = run_git_bytes(
        dir,
        &[
            "status",
            "--porcelain=2",
            "-z",
            "--ignore-submodules=none",
            "-uall",
        ],
    )?;
    check_no_dirty_submodule_status(&out)
}

/// Q-Deck A0 corrective round 6 (Part 3): a real `git status --porcelain=2
/// -z` state machine, factored out of [`ensure_no_dirty_submodule_worktree`]
/// so its byte-level parsing can be exercised directly with synthetic
/// fixtures, no real `git` process required.
///
/// The ORIGINAL implementation split the entire NUL-delimited output and
/// classified every resulting segment independently by its first byte. That
/// is blind to one thing porcelain-v2 -z guarantees: a type-`2`
/// (renamed/copied) record is `<record>\0<original-path>\0` — TWO separate
/// NUL-terminated fields for ONE logical record, where the second field (the
/// original path) carries NO type marker of its own and must never be
/// reinterpreted as an independent record. A rename whose OLD name happened
/// to start with `1 `/`2 ` followed by a space (e.g. a file literally named
/// `2 - Section overview.md`) would misparse that trailing field as a bogus
/// second status record — in this exact case, splitting it on spaces yields
/// a `<sub>`-position field of `"Section"`, which starts with `S` and isn't
/// `"S..."`, so the OLD code bails as if a submodule were dirty. This
/// version tracks state across records: every type-`2` record unconditionally
/// consumes the very next NUL field as its original path, never inspecting
/// or reclassifying it.
///
/// Every OTHER record type (`1`, `u`, `?`, `!`) is exactly one NUL field.
/// Only `1`/`2` carry a `<sub>` field (the 3rd whitespace-delimited token) —
/// dirty-submodule detection depends ONLY on that documented field, never on
/// a path substring match. Anything that doesn't parse as one of these five
/// documented record shapes — an unrecognized type byte, a `1`/`2` record
/// missing its fixed leading fields, a `2` record missing its mandatory
/// original-path field, a stray empty record anywhere but the single
/// trailing NUL `git` always emits after the last record — fails closed with
/// an internal error. Silently treating malformed output as "no dirty
/// submodule" would be exactly the kind of silent data loss this whole check
/// exists to prevent.
fn check_no_dirty_submodule_status(out: &[u8]) -> Result<()> {
    let mut records = out.split(|&b| b == 0).peekable();
    while let Some(record) = records.next() {
        if record.is_empty() {
            if records.peek().is_some() {
                anyhow::bail!(
                    "malformed git status output: an empty record appeared before the final \
                     trailing NUL"
                );
            }
            continue;
        }
        match record.first() {
            Some(type_byte @ (b'1' | b'2')) => {
                let mut fields = record.splitn(4, |&b| b == b' ');
                let _line_type = fields.next();
                let _xy = fields.next();
                let Some(sub) = fields.next() else {
                    anyhow::bail!(
                        "malformed git status output: a `{:?}` record is missing its fixed \
                         leading fields (expected at least `<type> <XY> <sub> ...`)",
                        *type_byte as char
                    );
                };
                if sub.first() == Some(&b'S') && sub != b"S..." {
                    let path = fields.next().unwrap_or_default();
                    anyhow::bail!(
                        "a submodule working tree has uncommitted content (status field {:?}) \
                         at path {:?} — dirty submodule contents are unsupported and would be \
                         silently lost by candidate-state capture",
                        String::from_utf8_lossy(sub),
                        String::from_utf8_lossy(path)
                    );
                }
                if *type_byte == b'2' {
                    // The mandatory original-path field: a SEPARATE NUL
                    // field belonging to THIS record, never independently
                    // classified — consumed and discarded here,
                    // unconditionally, regardless of what bytes it starts
                    // with. A genuinely truncated stream (the type-2
                    // record was the last thing present) and a present-
                    // but-empty field are indistinguishable from a bare
                    // `Option` alone — `split`'s own mandatory trailing
                    // empty segment after the stream's FINAL terminating
                    // NUL looks identical to "no more data" — so both are
                    // rejected here: a real original path is never empty.
                    match records.next() {
                        Some(orig) if !orig.is_empty() => {}
                        _ => anyhow::bail!(
                            "malformed git status output: a `2` (renamed/copied) record is \
                             missing its mandatory original-path field"
                        ),
                    }
                }
            }
            Some(b'u') | Some(b'?') | Some(b'!') => {
                // Unmerged/untracked/ignored: exactly one NUL field each,
                // no `<sub>` field, nothing further to consume.
            }
            other => {
                anyhow::bail!(
                    "malformed or unsupported git status output: unrecognized record type \
                     {other:?} (expected one of `1`, `2`, `u`, `?`, `!`)"
                );
            }
        }
    }
    Ok(())
}

/// Q-Deck A0 corrective round 7 (fresh exact-head Codex P1,
/// `#discussion_r3707367805`): before this run's own cumulative candidate
/// state (or, at materialization, [`finish_apply`]'s own resulting tree)
/// is ever trusted, prove the index carries no `assume-unchanged`/
/// `skip-worktree` entries.
///
/// `git add -A` HONORS both flags: a tracked path marked either way is
/// left exactly as the index already has it, no matter what its
/// working-tree bytes now say — `git status`/`git diff --cached` show
/// NOTHING for such a path. A genuinely edited file can therefore produce
/// an EMPTY cumulative patch and a tree OID identical to `base_commit`'s
/// own, letting the run seal with a candidate receipt that reads as a
/// complete no-op while silently discarding the edit — reproduced live
/// through `capture_cumulative_candidate` itself: `git update-index
/// --assume-unchanged f` (or `--skip-worktree`) then editing `f` produces
/// `Ok((vec![], base_commit's own tree))`.
///
/// `git ls-files -v -z` is the authority. `-v`'s status letter (`H`
/// cached, `S` skip-worktree, `M` unmerged, `R` removed, `C` modified,
/// `K` to-be-killed — never otherwise lowercase on their own) is lowered
/// for an ADDITIONALLY assume-unchanged entry, so any lowercase letter
/// means assume-unchanged regardless of the base status, and `S`/`s`
/// specifically marks skip-worktree (lowercase again when both flags are
/// set at once). `-z` keeps every path as raw, NUL-delimited bytes —
/// parsed here as a fixed `<letter><space><path>` byte layout, never
/// re-split on spaces or reinterpreted as UTF-8 for comparison; only the
/// final error message formats the path lossily for display, exactly
/// like every other diagnostic in this file.
///
/// A conservative, BLANKET policy: ANY index entry carrying either flag
/// rejects the whole capture, regardless of whether that specific
/// entry's current blob actually differs from its working-tree bytes
/// right now — proving that precisely would mean re-reading the flagged
/// path's own content, reopening exactly the TOCTOU window this check
/// exists to close. Sparse-checkout/`skip-worktree` and
/// `assume-unchanged` worktrees are UNSUPPORTED for candidate capture by
/// design — a documented limitation, never claimed as supported.
///
/// This check is READ-ONLY: it never calls `update-index` to clear or
/// mutate either flag on the caller's behalf, and never touches the
/// working tree — a rejection leaves the original checkout and index
/// exactly as found.
///
/// # Errors
/// Any underlying `git` failure, or at least one index entry carries
/// `assume-unchanged` or `skip-worktree`.
fn ensure_no_index_hidden_flags(worktree: &Path) -> Result<()> {
    let out = run_git_bytes(worktree, &["ls-files", "-v", "-z"])?;
    for record in out.split(|&b| b == 0) {
        if record.is_empty() {
            continue;
        }
        let Some(&letter) = record.first() else {
            continue;
        };
        let assume_unchanged = letter.is_ascii_lowercase();
        let skip_worktree = letter == b'S' || letter == b's';
        if !assume_unchanged && !skip_worktree {
            continue;
        }
        let path = record.get(2..).unwrap_or_default();
        let flag = match (assume_unchanged, skip_worktree) {
            (true, true) => "skip-worktree+assume-unchanged",
            (true, false) => "assume-unchanged",
            (false, true) => "skip-worktree",
            (false, false) => unreachable!("guarded by the check above"),
        };
        anyhow::bail!(
            "index entry {:?} carries {flag} — a hidden edit to this path would never appear \
             in `git status`/`git diff --cached`, silently producing an incomplete candidate \
             patch/tree; sparse-checkout/skip-worktree/assume-unchanged worktrees are \
             unsupported for candidate capture, this fails closed rather than trust an index \
             that may not reflect the working tree",
            String::from_utf8_lossy(path)
        );
    }
    Ok(())
}

/// Q-Deck A0 corrective round 3 (Codex P1, Part 2): a heuristic, NON-
/// AUTHORITATIVE early diagnostic only — Git's own extended-header line
/// for a mode change/new entry at submodule mode, scanned as lossy text
/// purely to log a hint sooner. An unchanged-MODE gitlink whose OID alone
/// changed (a submodule pointer bump with no mode-change header at all)
/// would NOT be caught by this heuristic — [`ensure_no_gitlink_mutation`]
/// is the actual authority, always run regardless of what this returns.
#[must_use]
pub fn patch_touches_gitlink(patch: &[u8]) -> bool {
    let text = String::from_utf8_lossy(patch);
    text.lines().any(|line| {
        line.contains("160000")
            && (line.starts_with("new mode")
                || line.starts_with("old mode")
                || line.starts_with("deleted file mode")
                || line.starts_with("new file mode"))
    })
}

/// Q-Deck A0 corrective round 1 (`docs/q-deck/a0-candidate-state.md` §1):
/// capture this run's CUMULATIVE candidate state — everything changed since
/// the conversation's own immutable `base_commit`, never a delta against
/// the previous run's own patch. Stages everything first (`add -A`, same
/// staging `diff_vs_base` already relies on for untracked files), then
/// produces a portable, binary-safe cumulative patch AS RAW BYTES (never a
/// lossy/lossless `String` round-trip — a patch is opaque binary transport,
/// not text) plus the exact tree OID that patch represents.
///
/// Q-Deck A0 corrective round 8 (fresh exact-head Codex P1,
/// `#discussion_r3710144125`): the patch and the tree OID are now derived
/// from ONE frozen index snapshot, never the worktree's own live index
/// twice. At the old head, `diff --cached` (the patch) and `write-tree`
/// (the tree) were two separate reads of the SAME live index — a
/// background process mutating that index between the two calls (e.g. the
/// provider's own leftover process) made the returned patch reflect one
/// index state while the returned tree reflected another, letting a run
/// seal with a receipt whose two halves silently disagree. Reproduced
/// through this exact function (see `dirty_submodule_tests::
/// r8_patch_tree_race_*`, a deterministic real-git barrier, no sleep):
/// ordinary content, deletion/executable-bit, binary content, and
/// non-UTF-8 paths all desynchronized identically at the old structure.
///
/// **Capture cutoff semantics, stated precisely and only as far as
/// proven:** the snapshot freezes the index EXACTLY as `git add -A` (plus
/// the hidden-flag/dirty-submodule checks) left it — any worktree edit
/// that has not yet been staged by that `add -A` was never part of what
/// gets frozen, whether it happens before or after this call returns. A
/// concurrent mutation to an ALREADY-staged path after the snapshot is
/// taken can never appear in the result, proven by the round-8 tests
/// above under real adversarial mutation. This function does NOT claim
/// (and nothing here proves) that a mutation happening to land in the
/// narrow window between `add -A` finishing and the snapshot copy
/// starting is impossible in principle — only that once the snapshot
/// exists, correctness is time-independent from that point on.
///
/// `tmp_parent` is a SERVER-owned directory (never the candidate-
/// controlled `worktree`) the private, unique, `O_EXCL`/`O_NOFOLLOW`
/// snapshot file is created under — the same confinement
/// [`apply_candidate_patch`]'s own private temp store already
/// established, so no candidate-planted symlink can redirect the write.
///
/// # Errors
/// Any underlying `git` failure (a non-worktree directory, a `base_commit`
/// this repo does not have, an I/O failure), or the resulting tree contains
/// a gitlink (submodule) entry — explicitly unsupported.
pub fn capture_cumulative_candidate(
    worktree: &Path,
    tmp_parent: &Path,
    base_commit: &str,
) -> Result<(Vec<u8>, String)> {
    capture_cumulative_candidate_with_hook(worktree, tmp_parent, base_commit, || {})
}

/// Same as [`capture_cumulative_candidate`], but `after_snapshot` runs
/// immediately after the index snapshot is frozen and before it is ever
/// read for the patch/tree — production always passes a no-op; ONLY the
/// round-8 regression tests use it, to deterministically prove a real
/// concurrent mutation of the LIVE index at exactly this point can no
/// longer desynchronize the returned patch from the returned tree.
fn capture_cumulative_candidate_with_hook(
    worktree: &Path,
    tmp_parent: &Path,
    base_commit: &str,
    after_snapshot: impl FnOnce(),
) -> Result<(Vec<u8>, String)> {
    run_git_bytes(worktree, &["add", "-A"])?;
    // Q-Deck A0 corrective round 7 (fresh exact-head Codex P1): checked
    // right after staging, as close as practical to the authoritative
    // index capture below — `add -A` itself does not clear
    // assume-unchanged/skip-worktree flags (they are orthogonal to
    // staging), so this catches them regardless of whether they were set
    // before or would still be set after.
    ensure_no_index_hidden_flags(worktree)
        .context("candidate capture's own index-hidden-flags check")?;
    // Q-Deck A0 corrective round 5 (Codex P1, Part 1): checked immediately
    // after staging, right before the index is frozen below — the
    // working-tree state `git status` inspects here is the exact same
    // state the snapshot freezes; nothing else touches this worktree
    // between the two calls.
    ensure_no_dirty_submodule_worktree(worktree)
        .context("candidate capture's own dirty-submodule-worktree check")?;

    // THE authority: one frozen copy of the index, taken now. Every read
    // below goes through this SAME file via `GIT_INDEX_FILE`, never the
    // worktree's own live index again.
    let index_path = resolve_git_index_path(worktree)
        .context("resolving this worktree's own real index file path")?;
    let index_bytes = std::fs::read(&index_path)
        .with_context(|| format!("reading the current index at {}", index_path.display()))?;
    let (snapshot, mut snapshot_file) = PrivateTempFile::create(tmp_parent, "candidate-index")
        .context("creating the private frozen-index snapshot file")?;
    {
        use std::io::Write as _;
        snapshot_file
            .write_all(&index_bytes)
            .and_then(|()| snapshot_file.sync_all())
            .context("writing the frozen index snapshot")?;
    }
    after_snapshot();

    let patch = run_git_bytes_with_index(
        worktree,
        snapshot.path(),
        &[
            "diff",
            "--cached",
            "--binary",
            "--full-index",
            "--no-color",
            "--no-ext-diff",
            base_commit,
        ],
    )?;
    if patch_touches_gitlink(&patch) {
        // Non-authoritative early diagnostic only (Part 2) — the real
        // rejection, if any, comes from `ensure_no_gitlink_mutation` below,
        // which is what actually distinguishes a mutation from an
        // unchanged base gitlink this heuristic cannot tell apart.
        eprintln!(
            "[o7] candidate capture: patch text hints at a gitlink mode change — deferring to \
             the authoritative tree comparison"
        );
    }
    let tree_oid = run_git_with_index(worktree, snapshot.path(), &["write-tree"])?
        .trim()
        .to_string();
    ensure_no_gitlink_mutation(worktree, base_commit, &tree_oid)
        .context("candidate capture's own gitlink policy check")?;
    Ok((patch, tree_oid))
}

/// Q-Deck A0 corrective round 1: apply a cumulative candidate patch (raw
/// bytes, never a `String`) into a fresh worktree already created at the
/// patch's own `base_commit`, then return the resulting tree OID.
///
/// The patch bytes are written to a UNIQUE, `O_EXCL`/`O_NOFOLLOW`, mode
/// `0600` regular file in a PRIVATE directory OUTSIDE the worktree
/// (`<runs_dir>/.o7-candidate-tmp/`) — never inside the candidate-
/// controlled checkout. This closes the symlink-escape hole a fixed-name
/// temp file INSIDE the worktree had: a base commit's own tree could
/// contain a symlink at that exact name, redirecting the write to
/// anywhere on the filesystem. The private directory is never populated by
/// anything the candidate's own tree controls, so no name collision or
/// symlink substitution there is reachable by a hostile patch/base commit.
///
/// `--index` updates the Git index atomically with the working tree, so the
/// immediately-following `write-tree` reflects exactly what was applied —
/// a conflicting, partial, or malformed patch makes `git apply` itself fail
/// non-zero, which this function surfaces as a plain error; no partial-
/// apply state is ever left looking materialized.
///
/// # Errors
/// The patch fails to apply cleanly (conflict, malformed, wrong base), the
/// resulting tree mutates a gitlink relative to `base_commit`, or any
/// underlying `git`/i/o failure.
pub fn apply_candidate_patch(
    runs_dir: &Path,
    worktree: &Path,
    base_commit: &str,
    patch: &[u8],
) -> Result<String> {
    // An empty cumulative patch is a legitimate outcome (a run that changed
    // nothing relative to base) — `git apply` itself treats an empty input
    // as an error ("No valid patches in input"), not a harmless no-op, so
    // it is special-cased here: nothing to apply, the worktree's own
    // just-checked-out tree (identical to `base_commit`'s) is already the
    // correct result.
    if patch.is_empty() {
        return finish_apply(worktree, base_commit);
    }

    let tmp_dir = runs_dir.join(".o7-candidate-tmp");
    std::fs::create_dir_all(&tmp_dir).with_context(|| {
        format!(
            "creating the private candidate-patch temp store at {}",
            tmp_dir.display()
        )
    })?;
    let dir_fd = open_dir_nofollow(&tmp_dir)
        .with_context(|| format!("opening {} O_NOFOLLOW", tmp_dir.display()))?;

    let name = format!(
        "apply-input.{}.{}",
        std::process::id(),
        TMP_COUNTER.fetch_add(1, Ordering::Relaxed)
    );
    let cname = CString::new(name.as_bytes())
        .context("candidate-patch temp filename contained a NUL byte")?;

    let file_fd = fs::openat(
        &dir_fd,
        cname.as_c_str(),
        OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::from_raw_mode(0o600),
    )
    .with_context(|| format!("creating a fresh, exclusive temp file {name:?}"))?;
    {
        use std::io::Write as _;
        let mut file = std::fs::File::from(file_fd);
        let write_then_sync = file.write_all(patch).and_then(|()| file.sync_all());
        if let Err(e) = write_then_sync {
            let _ = fs::unlinkat(&dir_fd, cname.as_c_str(), fs::AtFlags::empty());
            return Err(e).context("writing the candidate patch to its private temp file");
        }
    }

    // Q-Deck A0 corrective round 5 (CodeRabbit Major): `git apply` below
    // runs with `current_dir(worktree)` — a RELATIVE `tmp_path` (built from
    // a relative `runs_dir`, resolved against whatever the CALLING
    // process's own cwd happened to be) would be silently re-resolved
    // against `worktree` instead, since that's the SUBPROCESS's cwd, not
    // the caller's. Reproduced with real git: `git -C <worktree> apply
    // <relative-path-valid-from-elsewhere>` fails to find the file.
    //
    // The fix does not canonicalize `tmp_dir` (the path string) — that
    // would re-open the confinement question this same private store was
    // built to close, trusting a path an attacker-influenced `runs_dir`
    // argument could in principle still reference. Instead, the absolute
    // path is read back from the KERNEL's own view of the exact directory
    // already opened `O_NOFOLLOW` above (`dir_fd`), via `/proc/self/fd` —
    // the identity check already performed by `open_dir_nofollow` is what
    // this absolute path is anchored to, never a fresh, unverified
    // `canonicalize` of caller-supplied input.
    let tmp_dir_abs = absolute_path_of_open_dir(&dir_fd)
        .with_context(|| format!("resolving the real absolute path of {}", tmp_dir.display()))?;
    let tmp_path = tmp_dir_abs.join(&name);
    // Passed as `&OsStr` (via `Command::arg`, never through a `&str`/
    // `to_string_lossy` conversion) so a non-UTF-8 path is passed through
    // exactly, not lossily mangled.
    let result = run_git_with_path_args(
        worktree,
        &["apply", "--index", "--binary"],
        tmp_path.as_os_str(),
        &[],
    );
    let _ = fs::unlinkat(&dir_fd, cname.as_c_str(), fs::AtFlags::empty());
    result.map_err(|e| anyhow::anyhow!("git apply --index --binary failed: {e}"))?;
    finish_apply(worktree, base_commit)
}

/// The real, absolute path of an already-open directory file descriptor,
/// read back from the kernel via `/proc/self/fd` — anchored to the
/// directory identity that fd's own opener (`open_dir_nofollow`) already
/// verified, never a fresh `canonicalize` of a caller-supplied path string.
fn absolute_path_of_open_dir(dir_fd: &OwnedFd) -> Result<std::path::PathBuf> {
    use std::os::fd::AsRawFd as _;
    std::fs::read_link(format!("/proc/self/fd/{}", dir_fd.as_raw_fd()))
        .context("reading /proc/self/fd for the private candidate-patch temp store")
}

fn finish_apply(worktree: &Path, base_commit: &str) -> Result<String> {
    // Q-Deck A0 corrective round 7 (fresh exact-head Codex P1), defense in
    // depth: same check as `capture_cumulative_candidate`'s own, mirroring
    // how `ensure_no_gitlink_mutation`/`ensure_no_dirty_submodule_worktree`
    // are already duplicated at both sites. A freshly applied patch has no
    // way to itself SET either flag (`git apply` only ever touches the
    // named paths' content, never `update-index --assume-unchanged`/
    // `--skip-worktree` bits), but this worktree is reused for the next
    // continuation's own agent turn — a flag any earlier operator/tooling
    // step left set survives across that reuse, so this authoritative
    // write-tree stays protected the same way capture is.
    ensure_no_index_hidden_flags(worktree)
        .context("materialization's own index-hidden-flags check")?;
    // Q-Deck A0 corrective round 5 (Codex P1, Part 1), defense in depth:
    // same check as `capture_cumulative_candidate`'s own, mirroring how
    // `ensure_no_gitlink_mutation` is already duplicated at both sites.
    ensure_no_dirty_submodule_worktree(worktree)
        .context("materialization's own dirty-submodule-worktree check")?;
    let tree_oid = run_git(worktree, &["write-tree"])?.trim().to_string();
    ensure_no_gitlink_mutation(worktree, base_commit, &tree_oid)
        .context("materialization's own gitlink policy check")?;
    Ok(tree_oid)
}

static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

fn open_dir_nofollow(path: &Path) -> Result<OwnedFd, rustix::io::Errno> {
    fs::open(
        path,
        OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC | OFlags::RDONLY,
        Mode::empty(),
    )
}

/// Q-Deck A0 corrective round 8 (fresh exact-head Codex P1): a private,
/// unique, `O_EXCL`/`O_NOFOLLOW`, mode `0600` temp file created inside
/// `<parent>/.o7-candidate-tmp/` — the SAME private-store pattern
/// `apply_candidate_patch` already established, generalized here for a
/// second caller ([`capture_cumulative_candidate`]'s own index snapshot).
/// `parent` must be server-owned, never a candidate-controlled path (a
/// worktree), so no candidate-planted symlink can redirect the write —
/// callers pass a run's own private record directory. Fail-closed: a name
/// collision (extraordinarily unlikely given a PID+counter suffix) is a
/// hard error, never a silent overwrite. Deleted on `Drop`, regardless of
/// how the holder's own scope exits (success, error, or panic-unwind).
struct PrivateTempFile {
    dir_fd: OwnedFd,
    cname: CString,
    path: std::path::PathBuf,
}

impl PrivateTempFile {
    fn create(parent: &Path, prefix: &str) -> Result<(Self, std::fs::File)> {
        let tmp_dir = parent.join(".o7-candidate-tmp");
        std::fs::create_dir_all(&tmp_dir).with_context(|| {
            format!(
                "creating the private candidate temp store at {}",
                tmp_dir.display()
            )
        })?;
        let dir_fd = open_dir_nofollow(&tmp_dir)
            .with_context(|| format!("opening {} O_NOFOLLOW", tmp_dir.display()))?;
        let name = format!(
            "{prefix}.{}.{}",
            std::process::id(),
            TMP_COUNTER.fetch_add(1, Ordering::Relaxed)
        );
        let cname =
            CString::new(name.as_bytes()).context("private temp filename contained a NUL byte")?;
        let file_fd = fs::openat(
            &dir_fd,
            cname.as_c_str(),
            OFlags::RDWR | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::from_raw_mode(0o600),
        )
        .with_context(|| format!("creating a fresh, exclusive temp file {name:?}"))?;
        let tmp_dir_abs = absolute_path_of_open_dir(&dir_fd).with_context(|| {
            format!("resolving the real absolute path of {}", tmp_dir.display())
        })?;
        let path = tmp_dir_abs.join(&name);
        let file = std::fs::File::from(file_fd);
        Ok((
            Self {
                dir_fd,
                cname,
                path,
            },
            file,
        ))
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for PrivateTempFile {
    fn drop(&mut self) {
        let _ = fs::unlinkat(&self.dir_fd, self.cname.as_c_str(), fs::AtFlags::empty());
    }
}

/// Q-Deck A0 corrective round 8: the exact real index file `worktree`'s own
/// Git considers authoritative — resolved via `git rev-parse --git-path
/// index` rather than a hardcoded `worktree/.git/index`, because a LINKED
/// worktree's own index lives elsewhere entirely
/// (`<main-repo>/.git/worktrees/<name>/index`, confirmed empirically to be
/// reported as an absolute path already; a plain repository reports a
/// path relative to `worktree` itself, `.git/index`).
fn resolve_git_index_path(worktree: &Path) -> Result<std::path::PathBuf> {
    let raw = run_git(worktree, &["rev-parse", "--git-path", "index"])?;
    let raw = raw.trim();
    let path = Path::new(raw);
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        let abs_worktree = std::path::absolute(worktree)
            .with_context(|| format!("resolving an absolute path for {}", worktree.display()))?;
        Ok(abs_worktree.join(path))
    }
}

/// Like [`run_git_bytes`], but runs with `GIT_INDEX_FILE` pointed at
/// `index_file` — every index-reading operation (`diff --cached`,
/// `write-tree`) resolves against THAT file, never `worktree`'s own live
/// index, regardless of what happens to the live index concurrently.
fn run_git_bytes_with_index(dir: &Path, index_file: &Path, args: &[&str]) -> Result<Vec<u8>> {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_INDEX_FILE", index_file)
        .output()
        .with_context(|| format!("running git {args:?} against a frozen index snapshot"))?;
    if !out.status.success() {
        anyhow::bail!(
            "git {:?} (frozen index snapshot) failed: {}",
            args,
            String::from_utf8_lossy(&out.stderr)
        );
    }
    Ok(out.stdout)
}

/// Like [`run_git_bytes_with_index`], but returns stdout as a lossy
/// `String` — used only for `write-tree`'s own OID output, which is always
/// plain ASCII hex.
fn run_git_with_index(dir: &Path, index_file: &Path, args: &[&str]) -> Result<String> {
    Ok(String::from_utf8_lossy(&run_git_bytes_with_index(dir, index_file, args)?).into_owned())
}

fn run_git(dir: &Path, args: &[&str]) -> Result<String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .with_context(|| format!("running git {args:?}"))?;
    if !out.status.success() {
        anyhow::bail!(
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&out.stderr)
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

/// Same as [`run_git`], but for a path argument (positioned between
/// `leading_str_args` and `trailing_str_args`) that must be passed as raw
/// `OsStr` bytes — Q-Deck A0 corrective round 5 (CodeRabbit Major): a
/// non-UTF-8 path must never be truncated/mangled by a `to_string_lossy`
/// conversion just to fit `&[&str]`.
fn run_git_with_path_args(
    dir: &Path,
    leading_str_args: &[&str],
    path_arg: &std::ffi::OsStr,
    trailing_str_args: &[&str],
) -> Result<String> {
    let out = Command::new("git")
        .args(leading_str_args)
        .arg(path_arg)
        .args(trailing_str_args)
        .current_dir(dir)
        .output()
        .with_context(|| {
            format!("running git {leading_str_args:?} {path_arg:?} {trailing_str_args:?}")
        })?;
    if !out.status.success() {
        anyhow::bail!(
            "git {:?} {:?} {:?} failed: {}",
            leading_str_args,
            path_arg,
            trailing_str_args,
            String::from_utf8_lossy(&out.stderr)
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

/// Like [`run_git`], but returns raw stdout bytes unconditionally — used
/// wherever the output IS the candidate patch transport (never lossily
/// converted to/from `String`). Failure diagnostics (stderr) may still be
/// formatted lossily; only the patch bytes themselves must stay exact.
fn run_git_bytes(dir: &Path, args: &[&str]) -> Result<Vec<u8>> {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .with_context(|| format!("running git {args:?}"))?;
    if !out.status.success() {
        anyhow::bail!(
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&out.stderr)
        );
    }
    Ok(out.stdout)
}
/// Q-Deck A0 corrective round 3 (Codex P1, Part 2): unit tests for the
/// authoritative gitlink-mutation policy — a real git repository per test
/// (no o7d/ledger/process spawning needed; this is pure git plumbing),
/// gitlinks planted via `git update-index --add --cacheinfo 160000,<oid>,
/// <path>` (git never validates a gitlink's OID against a real object, so
/// a fake 40-hex SHA is sufficient and does not require an actual nested
/// repository). A placeholder DIRECTORY is created on disk at each gitlink
/// path — exactly like a real submodule's own checkout directory — because
/// `git add -A` (which both `capture_cumulative_candidate` and these tests'
/// own commit helper call) stages a DELETION for any index entry with no
/// corresponding working-tree path at all; without the placeholder, `add
/// -A` would silently wipe a manually-`update-index`d gitlink right back
/// out before it was ever committed.
///
/// Invariant for the restriction-lint allowance below (Q-Deck A0
/// corrective round 4, Codex P1): every `unwrap`/`expect`/index here
/// operates on a real git repository this test itself just created in a
/// throwaway `tempfile::tempdir()`, or on this module's own git-plumbing
/// output (`git ls-tree`, `git rev-parse`) — a panic means this test's own
/// fixture setup or a git invocation it fully controls broke, never a
/// runtime condition reachable through production code. Matches the
/// precedent in `crates/o7d/tests/golden_transcript_sse.rs`.
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]
mod gitlink_policy_tests {
    use super::*;

    fn run(dir: &Path, args: &[&str]) {
        let status = Command::new("git")
            .args(args)
            .current_dir(dir)
            .status()
            .expect("spawn git");
        assert!(status.success(), "git {args:?} failed");
    }

    fn init_repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        run(dir.path(), &["init", "-q"]);
        run(dir.path(), &["config", "user.email", "t@example.com"]);
        run(dir.path(), &["config", "user.name", "t"]);
        dir
    }

    /// Stage everything currently on disk, then commit. Any gitlink whose
    /// placeholder directory is still present survives this; one that was
    /// removed (directory deleted) is correctly staged as a deletion.
    fn commit_all(dir: &Path, msg: &str) -> String {
        run(dir, &["add", "-A"]);
        run(dir, &["commit", "-q", "-m", msg, "--allow-empty"]);
        rev_parse(dir, "HEAD").unwrap()
    }

    fn add_gitlink(dir: &Path, path: &str, oid: &str) {
        std::fs::create_dir_all(dir.join(path)).unwrap();
        run(
            dir,
            &[
                "update-index",
                "--add",
                "--cacheinfo",
                &format!("160000,{oid},{path}"),
            ],
        );
    }

    fn remove_gitlink(dir: &Path, path: &str) {
        run(dir, &["rm", "--cached", "-q", "-r", path]);
        let _ = std::fs::remove_dir_all(dir.join(path));
    }

    const OID_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const OID_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    #[test]
    fn unchanged_pre_existing_gitlink_succeeds_at_capture() {
        let repo = init_repo();
        let dir = repo.path();
        std::fs::write(dir.join("README"), "hi").unwrap();
        add_gitlink(dir, "vendor/lib", OID_A);
        let base = commit_all(dir, "base with gitlink");

        // An unrelated change — the gitlink's own placeholder directory is
        // left completely untouched, exactly like a real, un-modified
        // submodule.
        std::fs::write(dir.join("other.txt"), "unrelated change").unwrap();
        let result = capture_cumulative_candidate(dir, dir, &base);
        assert!(
            result.is_ok(),
            "unchanged base gitlink must not be rejected: {result:?}"
        );
    }

    #[test]
    fn added_gitlink_is_rejected_at_capture() {
        let repo = init_repo();
        let dir = repo.path();
        std::fs::write(dir.join("README"), "hi").unwrap();
        let base = commit_all(dir, "base without gitlink");

        add_gitlink(dir, "vendor/new-lib", OID_A);
        let result = capture_cumulative_candidate(dir, dir, &base);
        assert!(result.is_err(), "an ADDED gitlink must be rejected");
    }

    #[test]
    fn deleted_gitlink_is_rejected_at_capture() {
        let repo = init_repo();
        let dir = repo.path();
        std::fs::write(dir.join("README"), "hi").unwrap();
        add_gitlink(dir, "vendor/lib", OID_A);
        let base = commit_all(dir, "base with gitlink");

        remove_gitlink(dir, "vendor/lib");
        let result = capture_cumulative_candidate(dir, dir, &base);
        assert!(result.is_err(), "a DELETED gitlink must be rejected");
    }

    #[test]
    fn oid_modified_gitlink_is_rejected_at_capture() {
        let repo = init_repo();
        let dir = repo.path();
        std::fs::write(dir.join("README"), "hi").unwrap();
        add_gitlink(dir, "vendor/lib", OID_A);
        let base = commit_all(dir, "base with gitlink");

        // Same path, DIFFERENT OID — a submodule pointer bump. The
        // placeholder directory is already present from the base commit,
        // so `add -A` (inside capture) will not mistake this for a deletion.
        add_gitlink(dir, "vendor/lib", OID_B);
        let result = capture_cumulative_candidate(dir, dir, &base);
        assert!(
            result.is_err(),
            "a gitlink whose OID changed (unchanged mode) must be rejected"
        );
    }

    #[test]
    fn regular_file_replaced_by_gitlink_is_rejected_at_capture() {
        let repo = init_repo();
        let dir = repo.path();
        std::fs::write(dir.join("thing"), "a regular file").unwrap();
        let base = commit_all(dir, "base with a regular file");

        std::fs::remove_file(dir.join("thing")).unwrap();
        add_gitlink(dir, "thing", OID_A);
        let result = capture_cumulative_candidate(dir, dir, &base);
        assert!(
            result.is_err(),
            "a regular file replaced by a gitlink at the same path must be rejected"
        );
    }

    #[test]
    fn gitlink_replaced_by_regular_file_is_rejected_at_capture() {
        let repo = init_repo();
        let dir = repo.path();
        add_gitlink(dir, "thing", OID_A);
        let base = commit_all(dir, "base with a gitlink");

        remove_gitlink(dir, "thing");
        std::fs::write(dir.join("thing"), "now a regular file").unwrap();
        let result = capture_cumulative_candidate(dir, dir, &base);
        assert!(
            result.is_err(),
            "a gitlink replaced by a regular file at the same path must be rejected"
        );
    }

    #[test]
    fn nested_and_weird_name_gitlink_paths_are_handled_deterministically() {
        let repo = init_repo();
        let dir = repo.path();
        std::fs::write(dir.join("README"), "hi").unwrap();
        add_gitlink(dir, "deeply/nested/vendor lib with spaces", OID_A);
        let base = commit_all(dir, "base with a nested, weird-named gitlink");

        // Unchanged: still allowed.
        std::fs::write(dir.join("other.txt"), "unrelated").unwrap();
        assert!(capture_cumulative_candidate(dir, dir, &base).is_ok());

        // Changed OID at the same nested/weird path: still rejected.
        add_gitlink(dir, "deeply/nested/vendor lib with spaces", OID_B);
        assert!(capture_cumulative_candidate(dir, dir, &base).is_err());
    }

    #[test]
    fn materialization_applies_the_same_policy_as_capture() {
        let repo = init_repo();
        let dir = repo.path();
        std::fs::write(dir.join("README"), "hi").unwrap();
        add_gitlink(dir, "vendor/lib", OID_A);
        let base = commit_all(dir, "base with gitlink");

        // A cumulative patch that only touches an unrelated file — the base
        // gitlink stays untouched, so materialization (finish_apply, via
        // apply_candidate_patch) must succeed exactly like capture does.
        std::fs::write(dir.join("added.txt"), "hello").unwrap();
        let (patch, _tree) = capture_cumulative_candidate(dir, dir, &base).unwrap();

        // Reset back to base — `git reset --hard` restores the gitlink's
        // own placeholder directory automatically, exactly like checking
        // out a real submodule pointer does — then re-apply the SAME
        // patch, proving materialization's own gitlink check passes for an
        // unchanged base gitlink exactly like capture's did.
        run(dir, &["reset", "--hard", "-q", &base]);
        let runs_dir = tempfile::tempdir().unwrap();
        let result = apply_candidate_patch(runs_dir.path(), dir, &base, &patch);
        assert!(
            result.is_ok(),
            "materialization must accept an unchanged base gitlink: {result:?}"
        );
    }

    #[test]
    fn materialization_rejects_an_added_gitlink_via_the_patch() {
        let repo = init_repo();
        let dir = repo.path();
        std::fs::write(dir.join("README"), "hi").unwrap();
        let base = commit_all(dir, "base without gitlink");

        add_gitlink(dir, "vendor/new-lib", OID_A);
        run(dir, &["add", "-A"]);
        let patch = run_git_bytes(
            dir,
            &[
                "diff",
                "--cached",
                "--binary",
                "--full-index",
                "--no-color",
                "--no-ext-diff",
                &base,
            ],
        )
        .unwrap();

        run(dir, &["reset", "--hard", "-q", &base]);
        let runs_dir = tempfile::tempdir().unwrap();
        let result = apply_candidate_patch(runs_dir.path(), dir, &base, &patch);
        assert!(
            result.is_err(),
            "materialization must reject a patch that adds a gitlink: {result:?}"
        );
    }
}

/// Q-Deck A0 corrective round 5 (Codex P1, Part 1): unit tests for
/// [`ensure_no_dirty_submodule_worktree`] — REAL Git submodules (not the
/// fake `update-index --cacheinfo` gitlinks [`gitlink_policy_tests`] uses),
/// since `git status`'s own submodule-dirtiness detection requires a path
/// actually registered as a submodule (`.gitmodules` + `.git/config`), not
/// merely a `160000`-mode tree entry.
///
/// Invariant for the restriction-lint allowance below: every
/// `unwrap`/`expect`/index here operates on a real git repository this
/// test itself just created in a throwaway `tempfile::tempdir()` — a panic
/// means this test's own fixture setup or a fully-controlled git invocation
/// broke, never a runtime condition reachable through production code.
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]
mod dirty_submodule_tests {
    use super::*;

    fn run(dir: &Path, args: &[&str]) {
        let status = Command::new("git")
            .args(args)
            .current_dir(dir)
            .status()
            .expect("spawn git");
        assert!(status.success(), "git {args:?} failed");
    }

    /// Like [`run`], but the final path argument is a raw `OsStr` — for
    /// tests that need a non-UTF-8 filename, which cannot round-trip
    /// through a plain `&str` argument at all.
    fn run_os(dir: &Path, leading: &[&str], args: &[&str], path: &std::ffi::OsStr) {
        let status = Command::new("git")
            .args(leading)
            .args(args)
            .arg(path)
            .current_dir(dir)
            .status()
            .expect("spawn git");
        assert!(status.success(), "git {leading:?} {args:?} {path:?} failed");
    }

    fn init_repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        run(dir.path(), &["init", "-q"]);
        run(dir.path(), &["config", "user.email", "t@example.com"]);
        run(dir.path(), &["config", "user.name", "t"]);
        dir
    }

    fn commit_all(dir: &Path, msg: &str) -> String {
        run(dir, &["add", "-A"]);
        run(dir, &["commit", "-q", "-m", msg, "--allow-empty"]);
        rev_parse(dir, "HEAD").unwrap()
    }

    /// A real, upstream one-file submodule repo added as a real committed
    /// submodule at `path` inside `super_dir`. The returned `TempDir` is
    /// the upstream repo, kept alive for the caller's own lifetime.
    fn add_real_submodule(super_dir: &Path, path: &str) -> tempfile::TempDir {
        let upstream = init_repo();
        std::fs::write(upstream.path().join("tracked.txt"), "original\n").unwrap();
        commit_all(upstream.path(), "sub initial");
        run(
            super_dir,
            &[
                "-c",
                "protocol.file.allow=always",
                "submodule",
                "add",
                "-q",
                upstream.path().to_str().unwrap(),
                path,
            ],
        );
        upstream
    }

    #[test]
    fn unchanged_clean_initialized_submodule_passes() {
        let repo = init_repo();
        let dir = repo.path();
        let _upstream = add_real_submodule(dir, "sub");
        commit_all(dir, "add submodule");
        assert!(ensure_no_dirty_submodule_worktree(dir).is_ok());
    }

    #[test]
    fn deinitialized_submodule_with_unchanged_gitlink_passes() {
        let repo = init_repo();
        let dir = repo.path();
        let _upstream = add_real_submodule(dir, "sub");
        commit_all(dir, "add submodule");
        run(dir, &["submodule", "deinit", "-f", "sub"]);
        assert!(ensure_no_dirty_submodule_worktree(dir).is_ok());
    }

    #[test]
    fn dirty_tracked_file_inside_initialized_submodule_is_rejected() {
        let repo = init_repo();
        let dir = repo.path();
        let _upstream = add_real_submodule(dir, "sub");
        commit_all(dir, "add submodule");
        std::fs::write(dir.join("sub").join("tracked.txt"), "tampered\n").unwrap();
        let err = ensure_no_dirty_submodule_worktree(dir)
            .expect_err("dirty tracked content inside a submodule must be rejected");
        assert!(err.to_string().contains("dirty"), "got: {err}");
        // Original checkout not corrupted: the tampered content is still exactly
        // on disk, never reverted/wiped by the check itself.
        assert_eq!(
            std::fs::read_to_string(dir.join("sub").join("tracked.txt")).unwrap(),
            "tampered\n"
        );
    }

    #[test]
    fn dirty_untracked_file_inside_initialized_submodule_is_rejected() {
        let repo = init_repo();
        let dir = repo.path();
        let _upstream = add_real_submodule(dir, "sub");
        commit_all(dir, "add submodule");
        std::fs::write(dir.join("sub").join("new_untracked.txt"), "new\n").unwrap();
        let err = ensure_no_dirty_submodule_worktree(dir)
            .expect_err("untracked content inside a submodule must be rejected");
        assert!(err.to_string().contains("dirty"), "got: {err}");
    }

    #[test]
    fn nested_dirty_submodule_is_rejected() {
        let repo = init_repo();
        let dir = repo.path();
        let _upstream = add_real_submodule(dir, "sub");
        let nested_upstream = init_repo();
        std::fs::write(nested_upstream.path().join("n.txt"), "nested\n").unwrap();
        commit_all(nested_upstream.path(), "nested initial");
        run(
            &dir.join("sub"),
            &[
                "-c",
                "protocol.file.allow=always",
                "submodule",
                "add",
                "-q",
                nested_upstream.path().to_str().unwrap(),
                "nested_sub",
            ],
        );
        // `git submodule add` clones into a genuinely SEPARATE git config
        // scope (`dir/sub/.git` -> `dir/.git/modules/sub`) — it does NOT
        // inherit `sub`'s own upstream repo's LOCAL identity config, and a
        // CI runner (unlike this environment) may have no GLOBAL git
        // identity configured at all, making an un-configured `commit`
        // here fail with "empty ident name".
        run(&dir.join("sub"), &["config", "user.email", "t@example.com"]);
        run(&dir.join("sub"), &["config", "user.name", "t"]);
        commit_all(&dir.join("sub"), "add nested submodule");
        commit_all(dir, "record nested submodule bump");

        std::fs::write(
            dir.join("sub").join("nested_sub").join("n.txt"),
            "TAMPERED\n",
        )
        .unwrap();
        let err = ensure_no_dirty_submodule_worktree(dir)
            .expect_err("a nested dirty submodule must be rejected");
        assert!(err.to_string().contains("dirty"), "got: {err}");
    }

    /// Defense in depth: an attacker-reachable `.gitmodules` setting cannot
    /// hide real dirtiness from this check — verified empirically that
    /// plain `git status` (with no override) DOES get fooled by this, which
    /// is exactly why `--ignore-submodules=none` is passed explicitly.
    #[test]
    fn gitmodules_ignore_all_cannot_hide_dirty_content() {
        let repo = init_repo();
        let dir = repo.path();
        let _upstream = add_real_submodule(dir, "sub");
        commit_all(dir, "add submodule");
        run(
            dir,
            &["config", "-f", ".gitmodules", "submodule.sub.ignore", "all"],
        );
        std::fs::write(dir.join("sub").join("tracked.txt"), "tampered\n").unwrap();
        let err = ensure_no_dirty_submodule_worktree(dir)
            .expect_err("a .gitmodules ignore=all setting must not hide dirty submodule content");
        assert!(err.to_string().contains("dirty"), "got: {err}");
    }

    #[test]
    fn capture_cumulative_candidate_fails_closed_for_dirty_submodule() {
        let repo = init_repo();
        let dir = repo.path();
        let _upstream = add_real_submodule(dir, "sub");
        let base = commit_all(dir, "add submodule");
        std::fs::write(dir.join("sub").join("tracked.txt"), "tampered\n").unwrap();
        let result = capture_cumulative_candidate(dir, dir, &base);
        assert!(
            result.is_err(),
            "candidate capture must fail closed for a dirty submodule: {result:?}"
        );
        assert_eq!(
            std::fs::read_to_string(dir.join("sub").join("tracked.txt")).unwrap(),
            "tampered\n",
            "the original checkout must not be corrupted by the failed capture"
        );
    }

    /// Runs `git` and returns captured stdout, asserting success — like
    /// [`run`], but for callers that need the output (`git show`, `git
    /// cat-file`), not just a pass/fail signal.
    fn run_capture(dir: &Path, args: &[&str]) -> String {
        let out = Command::new("git")
            .args(args)
            .current_dir(dir)
            .output()
            .expect("spawn git");
        assert!(out.status.success(), "git {args:?} failed: {out:?}");
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    /// Q-Deck A0 corrective round 8 (fresh exact-head Codex P1,
    /// `#discussion_r3710144125`): drives the REAL, fixed
    /// `capture_cumulative_candidate_with_hook` with a DETERMINISTIC
    /// rendezvous — a real background thread spawning a REAL `git` process
    /// to mutate the live index, synchronized via channels (never `sleep`)
    /// so the mutation is GUARANTEED to land immediately after the index
    /// snapshot is frozen, exactly the point that desynchronized the patch
    /// from the tree at the old head (`c979bc1`..`f116c0c`, proven via the
    /// identical harness against the pre-fix structure before this fix
    /// landed — see the round-8 implementation report for that evidence).
    ///
    /// `setup` commits the initial content and returns `base_commit`, after
    /// making the edit (state A) the snapshot freezes. `mutate` is the
    /// background process's own real edit (state B) to the LIVE index,
    /// applied strictly after the snapshot already exists — proving it can
    /// no longer affect anything, since both the patch and the tree are now
    /// derived from the frozen copy, never the live index again.
    fn reproduce_patch_tree_race(
        setup: impl FnOnce(&Path) -> String,
        mutate: impl FnOnce(&Path) + Send + 'static,
    ) -> (tempfile::TempDir, String, Vec<u8>, String) {
        let repo = init_repo();
        let dir = repo.path().to_path_buf();
        let base = setup(&dir);
        let (go_tx, go_rx) = std::sync::mpsc::channel::<()>();
        let (done_tx, done_rx) = std::sync::mpsc::channel::<()>();
        let mutator_dir = dir.clone();
        let mutator = std::thread::spawn(move || {
            go_rx.recv().expect("main thread must signal go");
            mutate(&mutator_dir);
            done_tx.send(()).expect("main thread must still be waiting");
        });
        let result = capture_cumulative_candidate_with_hook(&dir, &dir, &base, || {
            go_tx.send(()).expect("mutator thread must still be alive");
            done_rx
                .recv()
                .expect("mutator thread must complete its real git mutation");
        });
        mutator.join().expect("mutator thread must not panic");
        let (patch, tree_oid) = result.expect("a fixed capture with no hidden flags must succeed");
        (repo, base, patch, tree_oid)
    }

    /// The direct invariant Round 8 requires for every successful capture:
    /// applying the exact captured patch to `base_commit` reproduces
    /// exactly the recorded `tree_oid` — never merely "a" tree.
    fn assert_apply_matches_recorded_tree(
        repo: &tempfile::TempDir,
        base: &str,
        patch: &[u8],
        tree_oid: &str,
    ) {
        let apply_dir = tempfile::tempdir().unwrap();
        run(
            repo.path(),
            &[
                "worktree",
                "add",
                "-q",
                apply_dir.path().to_str().unwrap(),
                base,
            ],
        );
        let materialized_tree = apply_candidate_patch(
            &tempfile::tempdir().unwrap().keep(),
            apply_dir.path(),
            base,
            patch,
        )
        .expect("applying the captured patch to its own declared base must succeed cleanly");
        assert_eq!(
            materialized_tree, tree_oid,
            "apply(captured_patch, base_commit).tree_oid must equal the recorded candidate_tree_oid"
        );
    }

    #[test]
    fn r8_patch_tree_race_is_closed_ordinary_tracked_content() {
        let (repo, base, patch, tree_oid) = reproduce_patch_tree_race(
            |dir| {
                std::fs::write(dir.join("tracked.txt"), "original\n").unwrap();
                let base = commit_all(dir, "init");
                std::fs::write(dir.join("tracked.txt"), "state A: first edit\n").unwrap();
                base
            },
            |dir| {
                std::fs::write(dir.join("tracked.txt"), "state B: mutator edit\n").unwrap();
                let status = Command::new("git")
                    .args(["add", "-A"])
                    .current_dir(dir)
                    .status()
                    .expect("spawn git");
                assert!(status.success());
            },
        );
        let dir = repo.path();

        // The returned PATCH reflects state A — frozen before the mutator
        // ever ran.
        let patch_text = String::from_utf8_lossy(&patch);
        assert!(
            patch_text.contains("state A: first edit"),
            "got: {patch_text}"
        );
        assert!(!patch_text.contains("state B"), "got: {patch_text}");

        // The returned TREE ALSO reflects state A — the mutator's real
        // `git add -A` to the LIVE index, run strictly after the snapshot
        // was taken, has NO effect on it.
        let tree_content = run_capture(dir, &["show", &format!("{tree_oid}:tracked.txt")]);
        assert!(
            tree_content.contains("state A: first edit"),
            "the recorded tree must be UNAFFECTED by the post-snapshot mutation: {tree_content:?}"
        );
        assert!(!tree_content.contains("state B"), "got: {tree_content:?}");

        assert_apply_matches_recorded_tree(&repo, &base, &patch, &tree_oid);
    }

    #[test]
    fn r8_patch_tree_race_is_closed_deletion_and_executable_bit() {
        let (repo, base, patch, tree_oid) = reproduce_patch_tree_race(
            |dir| {
                std::fs::write(dir.join("to_delete.txt"), "will be deleted\n").unwrap();
                std::fs::write(dir.join("script.sh"), "#!/bin/sh\necho hi\n").unwrap();
                let base = commit_all(dir, "init");
                std::fs::remove_file(dir.join("to_delete.txt")).unwrap();
                base
            },
            |dir| {
                // Executable-bit flip, staged by a real background `git`
                // process, strictly after the snapshot was already frozen.
                let mut perm = std::fs::metadata(dir.join("script.sh"))
                    .unwrap()
                    .permissions();
                std::os::unix::fs::PermissionsExt::set_mode(&mut perm, 0o755);
                std::fs::set_permissions(dir.join("script.sh"), perm).unwrap();
                let status = Command::new("git")
                    .args(["add", "-A"])
                    .current_dir(dir)
                    .status()
                    .expect("spawn git");
                assert!(status.success());
            },
        );
        let dir = repo.path();
        let patch_text = String::from_utf8_lossy(&patch);
        assert!(
            patch_text.contains("deleted file") || patch_text.contains("to_delete.txt"),
            "the patch must reflect the deletion captured before the mutator ran: {patch_text}"
        );
        assert!(
            !patch_text.contains("old mode") && !patch_text.contains("new mode"),
            "the patch must NOT reflect the mutator's later executable-bit flip: {patch_text}"
        );
        // The recorded tree must ALSO be unaffected by the mutator's
        // post-snapshot executable-bit flip: still non-executable.
        let ls_tree = run_capture(dir, &["ls-tree", &tree_oid, "script.sh"]);
        assert!(
            ls_tree.starts_with("100644"),
            "the recorded tree must be UNAFFECTED by the post-snapshot executable-bit flip: \
             {ls_tree:?}"
        );
        assert_apply_matches_recorded_tree(&repo, &base, &patch, &tree_oid);
    }

    #[test]
    fn r8_patch_tree_race_is_closed_binary_content() {
        let (repo, base, patch, tree_oid) = reproduce_patch_tree_race(
            |dir| {
                std::fs::write(dir.join("bin.dat"), [0u8, 1, 2, 3, 255, 254]).unwrap();
                let base = commit_all(dir, "init");
                std::fs::write(dir.join("bin.dat"), [10u8, 20, 30, 0, 255]).unwrap();
                base
            },
            |dir| {
                std::fs::write(dir.join("bin.dat"), [99u8, 98, 97, 0, 1, 2]).unwrap();
                let status = Command::new("git")
                    .args(["add", "-A"])
                    .current_dir(dir)
                    .status()
                    .expect("spawn git");
                assert!(status.success());
            },
        );
        let dir = repo.path();
        assert!(
            String::from_utf8_lossy(&patch).contains("GIT binary patch"),
            "a binary content change must produce a binary patch marker: {:?}",
            String::from_utf8_lossy(&patch)
        );
        let tree_bytes = Command::new("git")
            .args(["show", &format!("{tree_oid}:bin.dat")])
            .current_dir(dir)
            .output()
            .expect("spawn git")
            .stdout;
        assert_eq!(
            tree_bytes,
            vec![10u8, 20, 30, 0, 255],
            "the recorded tree must be UNAFFECTED by the mutator's post-snapshot binary content"
        );
        assert_apply_matches_recorded_tree(&repo, &base, &patch, &tree_oid);
    }

    #[cfg(unix)]
    #[test]
    fn r8_patch_tree_race_is_closed_non_utf8_path() {
        use std::os::unix::ffi::OsStrExt;
        let mut raw = b"bad_".to_vec();
        raw.extend_from_slice(&[0xFF, 0xFE]);
        raw.extend_from_slice(b"_name.txt");
        let name = std::ffi::OsStr::from_bytes(&raw).to_os_string();
        let name_for_mutate = name.clone();
        let (repo, base, patch, tree_oid) = reproduce_patch_tree_race(
            move |dir| {
                std::fs::write(dir.join(&name), "original\n").unwrap();
                let base = commit_all(dir, "init");
                std::fs::write(dir.join(&name), "state A\n").unwrap();
                base
            },
            move |dir| {
                std::fs::write(dir.join(&name_for_mutate), "state B\n").unwrap();
                let status = Command::new("git")
                    .args(["add", "-A"])
                    .current_dir(dir)
                    .status()
                    .expect("spawn git");
                assert!(status.success());
            },
        );
        let patch_text = String::from_utf8_lossy(&patch);
        assert!(patch_text.contains("state A"), "got: {patch_text}");
        assert!(!patch_text.contains("state B"), "got: {patch_text}");
        assert_apply_matches_recorded_tree(&repo, &base, &patch, &tree_oid);
    }

    /// Q-Deck A0 corrective round 7 (fresh exact-head Codex P1,
    /// `#discussion_r3707367805`): the exact live counterexample —
    /// `git update-index --assume-unchanged` on a tracked file, then
    /// editing it, made `capture_cumulative_candidate` (the REAL
    /// production function, not a synthetic fixture) return
    /// `Ok((vec![], base_commit's own tree))` at the old head: an empty
    /// patch and an unchanged tree OID despite the tampered content on
    /// disk. Must now fail closed instead.
    #[test]
    fn assume_unchanged_modified_tracked_file_rejects() {
        let repo = init_repo();
        let dir = repo.path();
        std::fs::write(dir.join("tracked.txt"), "original\n").unwrap();
        let base = commit_all(dir, "init");
        run(dir, &["update-index", "--assume-unchanged", "tracked.txt"]);
        std::fs::write(dir.join("tracked.txt"), "TAMPERED\n").unwrap();
        let result = capture_cumulative_candidate(dir, dir, &base);
        let err = result.expect_err("an assume-unchanged hidden edit must fail closed");
        assert!(
            format!("{err:?}").contains("assume-unchanged"),
            "got: {err:?}"
        );
    }

    /// Same counterexample, for `skip-worktree` instead.
    #[test]
    fn skip_worktree_modified_tracked_file_rejects() {
        let repo = init_repo();
        let dir = repo.path();
        std::fs::write(dir.join("tracked.txt"), "original\n").unwrap();
        let base = commit_all(dir, "init");
        run(dir, &["update-index", "--skip-worktree", "tracked.txt"]);
        std::fs::write(dir.join("tracked.txt"), "TAMPERED\n").unwrap();
        let result = capture_cumulative_candidate(dir, dir, &base);
        let err = result.expect_err("a skip-worktree hidden edit must fail closed");
        assert!(format!("{err:?}").contains("skip-worktree"), "got: {err:?}");
    }

    #[test]
    fn assume_unchanged_on_a_nested_path_rejects() {
        let repo = init_repo();
        let dir = repo.path();
        std::fs::create_dir_all(dir.join("a/b/c")).unwrap();
        std::fs::write(dir.join("a/b/c/nested.txt"), "original\n").unwrap();
        let base = commit_all(dir, "init");
        run(
            dir,
            &["update-index", "--assume-unchanged", "a/b/c/nested.txt"],
        );
        std::fs::write(dir.join("a/b/c/nested.txt"), "TAMPERED\n").unwrap();
        let err = capture_cumulative_candidate(dir, dir, &base)
            .expect_err("a nested assume-unchanged hidden edit must fail closed");
        assert!(
            format!("{err:?}").contains("assume-unchanged"),
            "got: {err:?}"
        );
    }

    #[test]
    fn skip_worktree_with_spaces_and_newline_in_path_rejects() {
        let repo = init_repo();
        let dir = repo.path();
        // A literal newline byte inside a filename is valid on Linux;
        // `git ls-files -z`'s NUL-delimited output is exactly what makes
        // this safe to parse without ambiguity.
        let name = "dir with spaces/file\nwith a newline.txt";
        std::fs::create_dir_all(dir.join("dir with spaces")).unwrap();
        std::fs::write(dir.join(name), "original\n").unwrap();
        let base = commit_all(dir, "init");
        run(dir, &["update-index", "--skip-worktree", name]);
        std::fs::write(dir.join(name), "TAMPERED\n").unwrap();
        let err = capture_cumulative_candidate(dir, dir, &base)
            .expect_err("skip-worktree on a path with spaces/newline must still be caught");
        assert!(format!("{err:?}").contains("skip-worktree"), "got: {err:?}");
    }

    #[cfg(unix)]
    #[test]
    fn assume_unchanged_on_a_non_utf8_path_rejects_without_panicking() {
        use std::os::unix::ffi::OsStrExt;
        let repo = init_repo();
        let dir = repo.path();
        let mut raw = b"bad_".to_vec();
        raw.extend_from_slice(&[0xFF, 0xFE]);
        raw.extend_from_slice(b"_name.txt");
        let name = std::ffi::OsStr::from_bytes(&raw);
        std::fs::write(dir.join(name), "original\n").unwrap();
        let base = commit_all(dir, "init");
        run_os(
            dir,
            &["-c", "core.quotepath=false"],
            &["update-index", "--assume-unchanged"],
            name,
        );
        std::fs::write(dir.join(name), "TAMPERED\n").unwrap();
        let err = capture_cumulative_candidate(dir, dir, &base)
            .expect_err("a non-UTF-8 path must still be caught, not silently skipped or panic");
        assert!(
            format!("{err:?}").contains("assume-unchanged"),
            "got: {err:?}"
        );
    }

    #[test]
    fn ordinary_tracked_edit_still_captures() {
        let repo = init_repo();
        let dir = repo.path();
        std::fs::write(dir.join("tracked.txt"), "original\n").unwrap();
        let base = commit_all(dir, "init");
        std::fs::write(dir.join("tracked.txt"), "edited\n").unwrap();
        let (patch, tree_oid) = capture_cumulative_candidate(dir, dir, &base)
            .expect("an ordinary tracked edit with no hidden-index flags must still capture");
        assert!(!patch.is_empty(), "the edit must appear in the patch");
        assert_ne!(
            tree_oid,
            rev_parse(dir, &format!("{base}^{{tree}}")).unwrap()
        );
    }

    #[test]
    fn untracked_file_still_captures() {
        let repo = init_repo();
        let dir = repo.path();
        let base = commit_all(dir, "init");
        std::fs::write(dir.join("new_untracked.txt"), "new\n").unwrap();
        let (patch, _tree_oid) = capture_cumulative_candidate(dir, dir, &base)
            .expect("a plain untracked file with no hidden-index flags must still capture");
        assert!(!patch.is_empty(), "the new file must appear in the patch");
    }

    /// Q-Deck A0 corrective round 8: the direct invariant required for
    /// EVERY successful candidate capture, no adversarial mutation
    /// involved — a plain, uncontested capture must still satisfy
    /// `apply(captured_patch, base_commit).tree_oid == candidate_tree_oid`.
    #[test]
    fn apply_of_captured_patch_always_matches_recorded_tree_oid() {
        let repo = init_repo();
        let dir = repo.path();
        std::fs::write(dir.join("a.txt"), "original\n").unwrap();
        std::fs::write(dir.join("b.txt"), "keep\n").unwrap();
        let base = commit_all(dir, "init");
        std::fs::write(dir.join("a.txt"), "edited\n").unwrap();
        std::fs::write(dir.join("c.txt"), "new\n").unwrap();
        std::fs::remove_file(dir.join("b.txt")).unwrap();
        let (patch, tree_oid) = capture_cumulative_candidate(dir, dir, &base)
            .expect("an ordinary multi-file change must capture");
        assert_apply_matches_recorded_tree(&repo, &base, &patch, &tree_oid);
    }

    #[test]
    fn rejection_leaves_index_flags_and_working_tree_bytes_untouched() {
        let repo = init_repo();
        let dir = repo.path();
        std::fs::write(dir.join("tracked.txt"), "original\n").unwrap();
        let base = commit_all(dir, "init");
        run(dir, &["update-index", "--assume-unchanged", "tracked.txt"]);
        std::fs::write(dir.join("tracked.txt"), "TAMPERED\n").unwrap();
        assert!(capture_cumulative_candidate(dir, dir, &base).is_err());
        // Working-tree bytes: untouched.
        assert_eq!(
            std::fs::read_to_string(dir.join("tracked.txt")).unwrap(),
            "TAMPERED\n",
            "a rejected capture must never revert/mutate the working tree"
        );
        // Index flag: still set — the check must never itself clear it.
        let out = Command::new("git")
            .args(["ls-files", "-v"])
            .current_dir(dir)
            .output()
            .expect("spawn git");
        let ls = String::from_utf8_lossy(&out.stdout);
        assert!(
            ls.lines().any(|l| l == "h tracked.txt"),
            "a rejected capture must never clear the user's own assume-unchanged flag: {ls:?}"
        );
    }

    /// Q-Deck A0 corrective round 6 (Part 3) — the exact live counterexample:
    /// the OLD implementation split `git status --porcelain=2 -z` output on
    /// NUL and classified every segment independently, so a type-2 rename
    /// record's separate original-path field (`2 - Section overview.md`) got
    /// misread as a bogus second type-2 record whose `<sub>`-position field
    /// (`"Section"`) starts with `S` and isn't `"S..."` — a false-positive
    /// dirty-submodule bail with no submodule involved at all. This must
    /// pass cleanly at the new head.
    #[test]
    fn rename_whose_old_name_looks_like_a_type2_record_is_not_rejected() {
        let repo = init_repo();
        let dir = repo.path();
        std::fs::write(dir.join("2 - Section overview.md"), "content\n").unwrap();
        std::fs::write(dir.join("other.txt"), "unrelated\n").unwrap();
        commit_all(dir, "initial");
        run(
            dir,
            &["mv", "2 - Section overview.md", "renamed-section.md"],
        );
        assert!(
            ensure_no_dirty_submodule_worktree(dir).is_ok(),
            "an ordinary rename must never be misread as a dirty submodule record"
        );
    }

    /// Renames whose OLD name starts with each OTHER record-type prefix
    /// (`1 `, `u `, `? `, `! `) followed by a space are equally capable of
    /// colliding with the naive per-segment classifier; all must pass.
    #[test]
    fn renames_with_other_record_prefix_lookalike_names_are_not_rejected() {
        for old_name in [
            "1 - looks like an ordinary-change record.md",
            "u - looks like an unmerged record.md",
            "? - looks like an untracked record.md",
            "! - looks like an ignored record.md",
        ] {
            let repo = init_repo();
            let dir = repo.path();
            std::fs::write(dir.join(old_name), "content\n").unwrap();
            commit_all(dir, "initial");
            run(dir, &["mv", old_name, "renamed.md"]);
            assert!(
                ensure_no_dirty_submodule_worktree(dir).is_ok(),
                "rename of {old_name:?} must not be misread as a dirty submodule record"
            );
        }
    }

    /// An ordinary untracked superproject file (no submodule involved at
    /// all) must never be rejected — confirms this check's scope stays
    /// confined to submodule dirtiness, not general worktree changes.
    #[test]
    fn ordinary_untracked_superproject_file_passes() {
        let repo = init_repo();
        let dir = repo.path();
        commit_all(dir, "initial");
        std::fs::write(dir.join("scratch.txt"), "untracked\n").unwrap();
        assert!(ensure_no_dirty_submodule_worktree(dir).is_ok());
    }
}

/// Q-Deck A0 corrective round 6 (Part 3): byte-level fixtures for
/// [`check_no_dirty_submodule_status`] — the actual porcelain-v2 -z state
/// machine, exercised directly with synthetic records so every documented
/// record shape (and every malformed variant that must fail closed) is
/// covered without spawning a real `git` process.
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]
mod porcelain_v2_parser_tests {
    use super::*;

    fn join(records: &[&[u8]]) -> Vec<u8> {
        let mut out = Vec::new();
        for r in records {
            out.extend_from_slice(r);
            out.push(0);
        }
        out
    }

    #[test]
    fn empty_output_passes() {
        assert!(check_no_dirty_submodule_status(b"").is_ok());
    }

    #[test]
    fn clean_type1_record_passes() {
        let out = join(&[b"1 .M S... 100644 100644 100644 aaaa bbbb path.txt"]);
        assert!(check_no_dirty_submodule_status(&out).is_ok());
    }

    #[test]
    fn dirty_type1_submodule_record_is_rejected() {
        let out = join(&[b"1 .M SC.M 160000 160000 160000 aaaa bbbb sub"]);
        let err = check_no_dirty_submodule_status(&out).expect_err("must reject dirty submodule");
        assert!(err.to_string().contains("dirty"), "got: {err}");
    }

    #[test]
    fn clean_type2_rename_record_consumes_original_path_without_reclassifying_it() {
        // The original path deliberately starts with `2 ` — proving it is
        // consumed as a field, never reinterpreted as its own record.
        let out = join(&[
            b"2 R. S... 100644 100644 100644 aaaa bbbb R100 new.txt",
            b"2 - Section overview.md",
        ]);
        assert!(check_no_dirty_submodule_status(&out).is_ok());
    }

    #[test]
    fn dirty_type2_rename_record_is_rejected() {
        let out = join(&[
            b"2 R. SC.M 160000 160000 160000 aaaa bbbb R100 sub_new",
            b"sub_old",
        ]);
        let err = check_no_dirty_submodule_status(&out).expect_err("must reject dirty submodule");
        assert!(err.to_string().contains("dirty"), "got: {err}");
    }

    #[test]
    fn truncated_type2_record_missing_original_path_fails_closed() {
        let out = join(&[b"2 R. S... 100644 100644 100644 aaaa bbbb R100 new.txt"]);
        let err = check_no_dirty_submodule_status(&out)
            .expect_err("a type-2 record missing its original-path field must fail closed");
        assert!(err.to_string().contains("original-path"), "got: {err}");
    }

    #[test]
    fn unknown_record_type_fails_closed() {
        let out = join(&[b"x something unexpected"]);
        let err = check_no_dirty_submodule_status(&out)
            .expect_err("an unknown record type must fail closed");
        assert!(err.to_string().contains("unrecognized"), "got: {err}");
    }

    #[test]
    fn malformed_type1_missing_fixed_fields_fails_closed() {
        let out = join(&[b"1"]);
        let err = check_no_dirty_submodule_status(&out)
            .expect_err("a type-1 record missing its fixed fields must fail closed");
        assert!(
            err.to_string().contains("fixed leading fields"),
            "got: {err}"
        );
    }

    #[test]
    fn non_utf8_path_bytes_do_not_panic() {
        let mut record = b"1 .M S... 100644 100644 100644 aaaa bbbb ".to_vec();
        record.extend_from_slice(&[0xFF, 0xFE, 0xFD]);
        let out = join(&[&record]);
        assert!(check_no_dirty_submodule_status(&out).is_ok());
    }

    #[test]
    fn non_utf8_path_in_dirty_submodule_message_does_not_panic() {
        let mut record = b"1 .M SC.M 160000 160000 160000 aaaa bbbb ".to_vec();
        record.extend_from_slice(&[0xFF, 0xFE, 0xFD]);
        let out = join(&[&record]);
        let err = check_no_dirty_submodule_status(&out).expect_err("must still reject");
        assert!(err.to_string().contains("dirty"), "got: {err}");
    }

    #[test]
    fn embedded_spaces_and_newlines_in_original_path_are_opaque() {
        let out = join(&[
            b"2 R. S... 100644 100644 100644 aaaa bbbb R100 new name.txt",
            b"old name with spaces\nand a newline.txt",
        ]);
        assert!(check_no_dirty_submodule_status(&out).is_ok());
    }

    #[test]
    fn untracked_ignored_unmerged_records_are_skipped() {
        let out = join(&[
            b"? untracked.txt",
            b"! ignored.txt",
            b"u UU S... 100644 100644 100644 100644 aaaa bbbb cccc conflict.txt",
        ]);
        assert!(check_no_dirty_submodule_status(&out).is_ok());
    }

    #[test]
    fn stray_empty_record_before_trailing_nul_fails_closed() {
        // Two consecutive NULs mid-stream — never legitimate git output.
        let mut out = join(&[b"1 .M S... 100644 100644 100644 aaaa bbbb a.txt"]);
        out.push(0); // an extra, interior empty record
        out.extend(join(&[b"? b.txt"]));
        let err = check_no_dirty_submodule_status(&out)
            .expect_err("an interior empty record must fail closed, not be silently skipped");
        assert!(err.to_string().contains("empty record"), "got: {err}");
    }
}

/// Q-Deck A0 corrective round 5 (CodeRabbit Major): unit tests for
/// [`apply_candidate_patch`]'s path handling — a `runs_dir` whose own final
/// path component contains a space or non-UTF-8 bytes must never be
/// truncated/mangled (the whole point of passing it as `&OsStr`, never
/// through `to_string_lossy`), and a symlink planted at the temp store's
/// exact expected name must still fail closed exactly like the existing
/// (absolute-`runs_dir`) confinement test already proves.
///
/// Invariant for the restriction-lint allowance below: every
/// `unwrap`/`expect`/index here operates on a real git repository this
/// test itself just created in a throwaway `tempfile::tempdir()` — a panic
/// means this test's own fixture setup broke, never a runtime condition
/// reachable through production code.
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]
mod special_runs_dir_path_tests {
    use super::*;

    fn run(dir: &Path, args: &[&str]) {
        let status = Command::new("git")
            .args(args)
            .current_dir(dir)
            .status()
            .expect("spawn git");
        assert!(status.success(), "git {args:?} failed");
    }

    fn init_repo_with_one_commit() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        run(dir.path(), &["init", "-q"]);
        run(dir.path(), &["config", "user.email", "t@example.com"]);
        run(dir.path(), &["config", "user.name", "t"]);
        std::fs::write(dir.path().join("f.txt"), "base\n").unwrap();
        run(dir.path(), &["add", "-A"]);
        run(dir.path(), &["commit", "-q", "-m", "base"]);
        dir
    }

    /// A `runs_dir` whose own final path component contains a literal
    /// space — a byte that would still round-trip through
    /// `to_string_lossy`, but is a cheap, always-available proxy for "not a
    /// bare identifier", exercising the `&OsStr` argument-passing path
    /// rather than a plain ASCII name.
    #[test]
    fn a_runs_dir_with_a_space_in_its_path_is_handled_correctly() {
        let repo = init_repo_with_one_commit();
        let base = rev_parse(repo.path(), "HEAD").unwrap();
        let (patch, _tree_oid) =
            capture_cumulative_candidate(repo.path(), repo.path(), &base).unwrap();
        std::fs::write(repo.path().join("f.txt"), "changed\n").unwrap();
        let patch2 = run_git_bytes(
            repo.path(),
            &[
                "diff",
                "--binary",
                "--full-index",
                "--no-color",
                "--no-ext-diff",
                &base,
            ],
        )
        .unwrap();
        run(repo.path(), &["checkout", "-q", "--", "."]);
        let _ = patch; // (only needed to prove capture succeeds above)

        let outer = tempfile::tempdir().unwrap();
        let runs_dir = outer.path().join("runs with a space");
        std::fs::create_dir_all(&runs_dir).unwrap();

        let result = apply_candidate_patch(&runs_dir, repo.path(), &base, &patch2);
        assert!(
            result.is_ok(),
            "a runs_dir containing a space must not break patch application: {result:?}"
        );
    }

    /// A `runs_dir` whose own final path component contains a byte
    /// sequence that is NOT valid UTF-8 (Unix-only: paths are raw bytes,
    /// not required to be valid UTF-8 at all) must still work — proving
    /// the `&OsStr` argument path never lossily mangles it into a
    /// different, wrong path.
    #[test]
    fn a_runs_dir_with_a_non_utf8_path_component_is_handled_correctly() {
        use std::os::unix::ffi::OsStrExt;

        let repo = init_repo_with_one_commit();
        let base = rev_parse(repo.path(), "HEAD").unwrap();
        std::fs::write(repo.path().join("f.txt"), "changed\n").unwrap();
        let patch = run_git_bytes(
            repo.path(),
            &[
                "diff",
                "--binary",
                "--full-index",
                "--no-color",
                "--no-ext-diff",
                &base,
            ],
        )
        .unwrap();
        run(repo.path(), &["checkout", "-q", "--", "."]);

        let outer = tempfile::tempdir().unwrap();
        // 0xFF is not valid UTF-8 in any position; a real filesystem path
        // component may still contain it on Unix.
        let bad_name = std::ffi::OsStr::from_bytes(b"runs-\xffbad");
        let runs_dir = outer.path().join(bad_name);
        std::fs::create_dir_all(&runs_dir).unwrap();

        let result = apply_candidate_patch(&runs_dir, repo.path(), &base, &patch);
        assert!(
            result.is_ok(),
            "a non-UTF-8 runs_dir path component must not break patch application: {result:?}"
        );
    }

    /// Same confinement guarantee the existing (absolute-`runs_dir`)
    /// symlink test already proves, reconfirmed here for the private temp
    /// store's own absolute-path resolution: a symlink planted at the
    /// temp file's exact expected name never lets the write escape.
    #[test]
    fn a_symlink_at_the_temp_stores_own_directory_name_is_never_followed() {
        let repo = init_repo_with_one_commit();
        let base = rev_parse(repo.path(), "HEAD").unwrap();
        std::fs::write(repo.path().join("f.txt"), "changed\n").unwrap();
        let patch = run_git_bytes(
            repo.path(),
            &[
                "diff",
                "--binary",
                "--full-index",
                "--no-color",
                "--no-ext-diff",
                &base,
            ],
        )
        .unwrap();
        run(repo.path(), &["checkout", "-q", "--", "."]);

        let outer = tempfile::tempdir().unwrap();
        let sentinel = tempfile::tempdir().unwrap();
        // A symlink at the EXACT name `open_dir_nofollow` would otherwise
        // open as a real directory — `O_NOFOLLOW` must refuse this.
        std::os::unix::fs::symlink(sentinel.path(), outer.path().join(".o7-candidate-tmp"))
            .unwrap();

        let result = apply_candidate_patch(outer.path(), repo.path(), &base, &patch);
        assert!(
            result.is_err(),
            "a symlink at the private temp store's own name must be refused, not followed"
        );
        assert_eq!(
            std::fs::read_dir(sentinel.path()).unwrap().count(),
            0,
            "the sentinel directory the symlink points at must stay untouched"
        );
    }
}
