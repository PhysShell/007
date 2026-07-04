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

/// Absolute path of the shared `.git` dir backing `worktree`. A linked
/// worktree is not self-contained — its index/HEAD live under
/// `.git/worktrees/<name>` and objects/refs in the shared store — so a sandbox
/// that rw-binds only the worktree breaks every `git` op inside it. This is
/// what the sandbox must additionally bind.
pub fn git_common_dir(worktree: &Path) -> Result<std::path::PathBuf> {
    let out = run_git(
        worktree,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )?;
    Ok(std::path::PathBuf::from(out.trim()))
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
