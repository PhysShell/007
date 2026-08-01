use anyhow::{Context, Result};
use std::path::Path;
use std::process::Command;

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

/// Q-Deck A0: capture this run's CUMULATIVE candidate state — everything
/// changed since the conversation's own immutable `base_commit`, never a
/// delta against the previous run's own patch (`docs/q-deck/a0-candidate-state.md`
/// §1/§4). Stages everything first (`add -A`, same staging `diff_vs_base`
/// already relies on for untracked files), then produces a portable,
/// binary-safe cumulative patch plus the exact tree OID that patch
/// represents — the same tree `git write-tree` would produce from THIS
/// worktree's current index, independent of whatever the patch bytes
/// happen to look like to a human.
///
/// # Errors
/// Any underlying `git` failure (a non-worktree directory, a `base_commit`
/// this repo does not have, an I/O failure).
pub fn capture_cumulative_candidate(
    worktree: &Path,
    base_commit: &str,
) -> Result<(String, String)> {
    run_git(worktree, &["add", "-A"])?;
    let patch = run_git(
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
    let tree_oid = run_git(worktree, &["write-tree"])?.trim().to_string();
    Ok((patch, tree_oid))
}

/// Q-Deck A0: apply a cumulative candidate patch into a fresh worktree
/// already created at the patch's own `base_commit`, then return the
/// resulting tree OID for the caller to compare against the parent's own
/// receipt (`docs/q-deck/a0-candidate-state.md` §5). `--index` updates the
/// Git index atomically with the working tree, so the immediately-following
/// `write-tree` reflects exactly what was applied — a conflicting, partial,
/// or malformed patch makes `git apply` itself fail non-zero, which this
/// function surfaces as a plain error; no partial-apply state is ever left
/// looking materialized.
///
/// # Errors
/// The patch fails to apply cleanly (conflict, malformed, wrong base), or
/// any underlying `git` failure.
pub fn apply_candidate_patch(worktree: &Path, patch: &str) -> Result<String> {
    // An empty cumulative patch is a legitimate outcome (a run that changed
    // nothing relative to base) — `git apply` itself treats an empty input
    // as an error ("No valid patches in input"), not a harmless no-op, so
    // it is special-cased here: nothing to apply, the worktree's own
    // just-checked-out tree (identical to `base_commit`'s) is already the
    // correct result.
    if patch.trim().is_empty() {
        return Ok(run_git(worktree, &["write-tree"])?.trim().to_string());
    }
    // A patch file argument, not stdin: piping a nontrivial patch through
    // stdin while also capturing stdout/stderr risks the classic pipe
    // deadlock (this process blocked writing stdin while `git apply` is
    // blocked writing output no one has read yet). A plain temp file inside
    // the (already-created, still-empty-of-conflicting-names) worktree
    // sidesteps that entirely; removed unconditionally once `git apply`
    // returns, success or failure.
    let patch_file = worktree.join(".o7-candidate-patch.tmp");
    std::fs::write(&patch_file, patch.as_bytes()).context("writing a temporary patch file")?;
    let result = run_git(
        worktree,
        &[
            "apply",
            "--index",
            "--binary",
            patch_file.to_string_lossy().as_ref(),
        ],
    );
    let _ = std::fs::remove_file(&patch_file);
    result.map_err(|e| anyhow::anyhow!("git apply --index --binary failed: {e}"))?;
    Ok(run_git(worktree, &["write-tree"])?.trim().to_string())
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
