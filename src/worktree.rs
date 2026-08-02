use anyhow::{Context, Result};
use std::ffi::CString;
use std::os::fd::OwnedFd;
use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use rustix::fs::{self, Mode, OFlags};

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

/// Q-Deck A0: detect a gitlink (submodule-mode, `160000`) entry anywhere in
/// `commit`'s own tree — a submodule mutation is explicitly unsupported
/// (`docs/q-deck/a0-candidate-state.md` §7) and must fail closed rather than
/// silently capture/apply a bare gitlink pointer with no actual content.
///
/// # Errors
/// Any underlying `git` failure.
pub fn tree_has_gitlink(dir: &Path, commit: &str) -> Result<bool> {
    let out = run_git(dir, &["ls-tree", "-r", commit])?;
    Ok(out.lines().any(|line| line.starts_with("160000 commit ")))
}

/// Q-Deck A0: the SAME gitlink check as [`tree_has_gitlink`], applied to a
/// cumulative patch's own text — a candidate patch that introduces or
/// mutates a `160000` gitlink entry (rather than one already present,
/// unchanged, in `base`) must fail closed at capture time, not silently
/// produce a receipt a later materialization would have to reject alone.
#[must_use]
pub fn patch_touches_gitlink(patch: &[u8]) -> bool {
    // Git's own extended-header line for a mode change/new entry at
    // submodule mode — present verbatim in the patch text for any diff that
    // adds, removes, or changes a gitlink, regardless of `--binary`.
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
    anyhow::ensure!(
        !patch_touches_gitlink(&patch),
        "the candidate patch introduces or mutates a gitlink (submodule) entry — unsupported"
    );
    let tree_oid = run_git(worktree, &["write-tree"])?.trim().to_string();
    anyhow::ensure!(
        !tree_has_gitlink(worktree, &tree_oid)?,
        "the resulting candidate tree contains a gitlink (submodule) entry — unsupported"
    );
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
/// resulting tree contains a gitlink, or any underlying `git`/i/o failure.
pub fn apply_candidate_patch(runs_dir: &Path, worktree: &Path, patch: &[u8]) -> Result<String> {
    // An empty cumulative patch is a legitimate outcome (a run that changed
    // nothing relative to base) — `git apply` itself treats an empty input
    // as an error ("No valid patches in input"), not a harmless no-op, so
    // it is special-cased here: nothing to apply, the worktree's own
    // just-checked-out tree (identical to `base_commit`'s) is already the
    // correct result.
    if patch.is_empty() {
        return finish_apply(worktree);
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
    finish_apply(worktree)
}

fn finish_apply(worktree: &Path) -> Result<String> {
    let tree_oid = run_git(worktree, &["write-tree"])?.trim().to_string();
    anyhow::ensure!(
        !tree_has_gitlink(worktree, &tree_oid)?,
        "materializing the candidate patch produced a tree containing a gitlink (submodule) \
         entry — unsupported"
    );
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
