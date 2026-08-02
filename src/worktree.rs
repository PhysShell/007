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
pub fn add(repo: &Path, base: &str, path: &Path, branch: &str) -> Result<()> {
    let p = path.to_string_lossy();
    run_git(repo, &["worktree", "add", "-b", branch, p.as_ref(), base])
        .with_context(|| format!("git worktree add at {}", path.display()))?;
    Ok(())
}

/// Remove a worktree (its branch is left dangling; fine for throwaway runs).
pub fn remove(repo: &Path, path: &Path) -> Result<()> {
    let p = path.to_string_lossy();
    run_git(repo, &["worktree", "remove", "--force", p.as_ref()])?;
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
/// # Errors
/// Any underlying `git` failure (a non-worktree directory, a `base_commit`
/// this repo does not have, an I/O failure), or the resulting tree contains
/// a gitlink (submodule) entry — explicitly unsupported.
pub fn capture_cumulative_candidate(
    worktree: &Path,
    base_commit: &str,
) -> Result<(Vec<u8>, String)> {
    run_git_bytes(worktree, &["add", "-A"])?;
    let patch = run_git_bytes(
        worktree,
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
    let tree_oid = run_git(worktree, &["write-tree"])?.trim().to_string();
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

    let tmp_path = tmp_dir.join(&name);
    let result = run_git(
        worktree,
        &[
            "apply",
            "--index",
            "--binary",
            tmp_path.to_string_lossy().as_ref(),
        ],
    );
    let _ = fs::unlinkat(&dir_fd, cname.as_c_str(), fs::AtFlags::empty());
    result.map_err(|e| anyhow::anyhow!("git apply --index --binary failed: {e}"))?;
    finish_apply(worktree, base_commit)
}

fn finish_apply(worktree: &Path, base_commit: &str) -> Result<String> {
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
        let result = capture_cumulative_candidate(dir, &base);
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
        let result = capture_cumulative_candidate(dir, &base);
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
        let result = capture_cumulative_candidate(dir, &base);
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
        let result = capture_cumulative_candidate(dir, &base);
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
        let result = capture_cumulative_candidate(dir, &base);
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
        let result = capture_cumulative_candidate(dir, &base);
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
        assert!(capture_cumulative_candidate(dir, &base).is_ok());

        // Changed OID at the same nested/weird path: still rejected.
        add_gitlink(dir, "deeply/nested/vendor lib with spaces", OID_B);
        assert!(capture_cumulative_candidate(dir, &base).is_err());
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
        let (patch, _tree) = capture_cumulative_candidate(dir, &base).unwrap();

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
