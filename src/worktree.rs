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
    for record in out.split(|&b| b == 0) {
        if record.is_empty() {
            continue;
        }
        // Only "ordinary changed" (`1 ...`) and "renamed/copied" (`2 ...`) entry
        // lines carry a `<sub>` field, at the same fixed position (the 3rd
        // whitespace-delimited token) in both; untracked (`?`)/ignored (`!`)/
        // unmerged (`u`) lines never do and are irrelevant here.
        if record.first() != Some(&b'1') && record.first() != Some(&b'2') {
            continue;
        }
        let mut fields = record.splitn(4, |&b| b == b' ');
        let _line_type = fields.next();
        let _xy = fields.next();
        let Some(sub) = fields.next() else {
            continue;
        };
        if sub.first() == Some(&b'S') && sub != b"S..." {
            let path = fields.next().unwrap_or_default();
            anyhow::bail!(
                "a submodule working tree has uncommitted content (status field {:?}) at \
                 path {:?} — dirty submodule contents are unsupported and would be silently \
                 lost by candidate-state capture",
                String::from_utf8_lossy(sub),
                String::from_utf8_lossy(path)
            );
        }
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
    // Q-Deck A0 corrective round 5 (Codex P1, Part 1): checked immediately
    // after staging, right before this run's own content is read for the
    // patch/tree — the working-tree state `git status` inspects here is the
    // exact same state `write-tree` below captures; nothing else touches
    // this worktree between the two calls.
    ensure_no_dirty_submodule_worktree(worktree)
        .context("candidate capture's own dirty-submodule-worktree check")?;
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
        let result = capture_cumulative_candidate(dir, &base);
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
        let (patch, _tree_oid) = capture_cumulative_candidate(repo.path(), &base).unwrap();
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
