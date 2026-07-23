//! The hardened, read-only git access used to materialize a committed revision.
//!
//! Every invariant in PR 3's worktree goal ("do not execute repository hooks,
//! filters, fsmonitor, external diff, or other repository-controlled helpers") is
//! upheld here two ways:
//!
//! 1. **Plumbing only.** We use `rev-parse`, `cat-file`, and `ls-tree`. None of these
//!    run a checkout, so no smudge/clean filter and no `post-checkout`/`post-index`
//!    hook is ever invoked, and blob bytes come out of the object store *verbatim*
//!    (`cat-file blob` does not apply filters).
//! 2. **Hardened environment, as defense in depth.** The child git process runs with
//!    a cleared environment and an explicit allowlist, with system/global/repo config
//!    that could point at a helper neutralized: `core.hooksPath=/dev/null`,
//!    `core.fsmonitor=false`, `GIT_CONFIG_NOSYSTEM=1`, and `GIT_CONFIG_GLOBAL`/
//!    `GIT_CONFIG_SYSTEM` pointed at `/dev/null`. `GIT_EXTERNAL_DIFF`, askpass, and any
//!    inherited `GIT_*` are dropped by the clear.
//!
//! The worktree is materialized straight from the object store (see
//! [`crate::materialize`]), so a genuine `git checkout` — the one operation that would
//! run filters and hooks — never happens at all.

use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::os::unix::fs::MetadataExt as _;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::identity::{CanonicalRepoId, CommittedRevision, IdentityError};

/// A failure reading the repository through hardened git.
#[derive(Debug, thiserror::Error)]
pub enum GitError {
    #[error("spawning git failed: {0}")]
    Spawn(#[source] std::io::Error),
    #[error("git {command} failed (status {status}): {stderr}")]
    Command {
        command: String,
        status: String,
        stderr: String,
    },
    #[error("git {command} produced output that could not be parsed: {detail}")]
    Parse { command: String, detail: String },
    #[error("resolving the git common directory failed: {0}")]
    CommonDir(#[source] std::io::Error),
    #[error(transparent)]
    Identity(#[from] IdentityError),
}

/// One entry of a recursively-listed tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeEntry {
    /// The raw git mode (e.g. `0o100644`, `0o100755`, `0o120000`, `0o160000`).
    pub mode: u32,
    /// The object id.
    pub oid: String,
    /// The path relative to the tree root (git uses `/` separators, verbatim bytes).
    pub path: PathBuf,
}

impl TreeEntry {
    #[must_use]
    pub fn is_regular_file(&self) -> bool {
        self.mode == 0o100_644 || self.mode == 0o100_755
    }
    #[must_use]
    pub fn is_executable(&self) -> bool {
        self.mode == 0o100_755
    }
    #[must_use]
    pub fn is_symlink(&self) -> bool {
        self.mode == 0o120_000
    }
    /// A gitlink (submodule commit pointer). Its bytes live in another repository, so
    /// it is never materialized (that would pull in uncommitted-here content).
    #[must_use]
    pub fn is_gitlink(&self) -> bool {
        self.mode == 0o160_000
    }
}

/// Hardened, read-only git bound to one repository.
#[derive(Debug, Clone)]
pub struct HardenedGit {
    repo: PathBuf,
}

impl HardenedGit {
    /// Bind to the repository containing (or at) `repo`.
    #[must_use]
    pub fn new(repo: impl Into<PathBuf>) -> Self {
        Self { repo: repo.into() }
    }

    /// The path this git was bound to.
    #[must_use]
    pub fn repo(&self) -> &Path {
        &self.repo
    }

    /// The canonical identity of this repository: the absolute, symlink-resolved
    /// common git directory plus its filesystem identity.
    ///
    /// # Errors
    /// [`GitError`] if git cannot resolve the common dir or it cannot be stat-ed.
    pub fn canonical_repo_id(&self) -> Result<CanonicalRepoId, GitError> {
        let raw = self.run_text(&["rev-parse", "--path-format=absolute", "--git-common-dir"])?;
        let path = PathBuf::from(raw.trim_end_matches(['\n', '\r']));
        let canonical = path.canonicalize().map_err(GitError::CommonDir)?;
        let meta = std::fs::symlink_metadata(&canonical).map_err(GitError::CommonDir)?;
        Ok(CanonicalRepoId {
            git_common_dir: canonical,
            dev: meta.dev(),
            ino: meta.ino(),
        })
    }

    /// Resolve `rev` to the concrete committed object it names, proving it is a real
    /// commit already in the object store (`<rev>^{commit}` + `--verify`).
    ///
    /// # Errors
    /// [`GitError`] if the ref does not resolve to an existing commit object.
    pub fn resolve_commit(&self, rev: &str) -> Result<CommittedRevision, GitError> {
        // `^{commit}` forces peeling to a commit; `--verify` guarantees a single
        // unambiguous, existing object (never a range, never a missing ref).
        let spec = format!("{rev}^{{commit}}");
        let out = self.run_text(&["rev-parse", "--verify", "--end-of-options", &spec])?;
        Ok(CommittedRevision::from_object_id(
            out.trim_end_matches(['\n', '\r']).to_owned(),
        )?)
    }

    /// List every blob / symlink / gitlink reachable from `revision`, recursively.
    ///
    /// Uses `ls-tree -r -z`: recursion reports leaves (blobs, symlinks, gitlinks) and
    /// never bare tree entries, so parent directories are created from the leaf paths.
    ///
    /// # Errors
    /// [`GitError`] on a git failure or an unparseable record.
    pub fn list_tree(&self, revision: &CommittedRevision) -> Result<Vec<TreeEntry>, GitError> {
        // -z: NUL-terminated records with verbatim (unquoted) paths, so a path with
        // spaces or odd bytes is unambiguous. Record: "<mode> SP <type> SP <oid> TAB <path>".
        let raw = self.run_bytes(&[
            OsStr::new("ls-tree"),
            OsStr::new("-r"),
            OsStr::new("-z"),
            OsStr::new("--full-tree"),
            OsStr::new("--end-of-options"),
            OsStr::new(revision.as_str()),
        ])?;
        parse_ls_tree(&raw).map_err(|detail| GitError::Parse {
            command: "ls-tree".to_owned(),
            detail,
        })
    }

    /// Read a blob's bytes verbatim from the object store (no filters applied).
    ///
    /// # Errors
    /// [`GitError`] if the object is missing or git fails.
    pub fn cat_blob(&self, oid: &str) -> Result<Vec<u8>, GitError> {
        self.run_bytes(&[
            OsStr::new("cat-file"),
            OsStr::new("blob"),
            OsStr::new("--end-of-options"),
            OsStr::new(oid),
        ])
    }

    // ---- internals ----

    /// The explicit environment for every git child: cleared, then exactly this
    /// allowlist. Nothing is inherited (no `GIT_*`, no `GIT_EXTERNAL_DIFF`, no
    /// askpass, no proxy). Config that could name a helper is neutralized.
    fn base_command(&self) -> Command {
        let mut cmd = Command::new("git");
        cmd.env_clear();
        let env: BTreeMap<&str, &str> = [
            // Minimal PATH so git can find its own installed binary; we only invoke
            // plumbing, which needs no external helper on PATH.
            ("PATH", "/usr/bin:/bin"),
            // Ignore system/global config entirely — those are the usual places a
            // hooksPath / fsmonitor / filter / alias could be pointed at a helper.
            ("GIT_CONFIG_NOSYSTEM", "1"),
            ("GIT_CONFIG_GLOBAL", "/dev/null"),
            ("GIT_CONFIG_SYSTEM", "/dev/null"),
            // No interactive prompts, no credential/askpass helpers, stable parsing.
            ("GIT_TERMINAL_PROMPT", "0"),
            ("GIT_OPTIONAL_LOCKS", "0"),
            ("GIT_ASKPASS", "/bin/false"),
            ("HOME", "/dev/null"),
            ("LC_ALL", "C"),
        ]
        .into_iter()
        .collect();
        cmd.envs(env);
        cmd.arg("-C").arg(&self.repo);
        // Defense in depth on top of plumbing-only: even a repo-local config pointing
        // at a helper is overridden per invocation.
        cmd.arg("-c").arg("core.hooksPath=/dev/null");
        cmd.arg("-c").arg("core.fsmonitor=false");
        cmd.arg("-c").arg("core.fsmonitorHookVersion=0");
        cmd.arg("-c").arg("core.autocrlf=false");
        cmd.arg("-c").arg("core.symlinks=true");
        cmd.arg("-c").arg("diff.external=");
        cmd.stdin(std::process::Stdio::null());
        cmd
    }

    fn run(&self, args: &[&OsStr]) -> Result<Vec<u8>, GitError> {
        let mut cmd = self.base_command();
        cmd.args(args);
        let out = cmd.output().map_err(GitError::Spawn)?;
        if out.status.success() {
            Ok(out.stdout)
        } else {
            Err(GitError::Command {
                command: display_args(args),
                status: out.status.to_string(),
                stderr: String::from_utf8_lossy(&out.stderr).trim().to_owned(),
            })
        }
    }

    fn run_bytes(&self, args: &[&OsStr]) -> Result<Vec<u8>, GitError> {
        self.run(args)
    }

    fn run_text(&self, args: &[&str]) -> Result<String, GitError> {
        let os: Vec<OsString> = args.iter().map(OsString::from).collect();
        let refs: Vec<&OsStr> = os.iter().map(OsString::as_os_str).collect();
        let bytes = self.run(&refs)?;
        String::from_utf8(bytes).map_err(|e| GitError::Parse {
            command: display_args(&refs),
            detail: format!("non-utf8 output: {e}"),
        })
    }
}

fn display_args(args: &[&OsStr]) -> String {
    args.iter()
        .map(|a| a.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Parse `ls-tree -r -z` output into entries.
///
/// Each record is `"<mode> SP <type> SP <oid> TAB <path>"` terminated by NUL. Paths
/// are verbatim bytes (never quoted under `-z`); we keep them as an `OsString` via the
/// unix byte representation so a non-UTF-8 path survives.
fn parse_ls_tree(raw: &[u8]) -> Result<Vec<TreeEntry>, String> {
    use std::os::unix::ffi::OsStrExt as _;

    let mut entries = Vec::new();
    for record in raw.split(|&b| b == 0) {
        if record.is_empty() {
            continue;
        }
        // Split header (up to the TAB) from the path (after it).
        let tab = record
            .iter()
            .position(|&b| b == b'\t')
            .ok_or_else(|| "ls-tree record has no TAB separator".to_owned())?;
        let header = record
            .get(..tab)
            .ok_or_else(|| "ls-tree record header slice out of range".to_owned())?;
        let path_bytes = record
            .get(tab + 1..)
            .ok_or_else(|| "ls-tree record path slice out of range".to_owned())?;
        let header =
            std::str::from_utf8(header).map_err(|e| format!("ls-tree header not utf8: {e}"))?;
        let mut fields = header.split(' ');
        let mode = fields.next().ok_or_else(|| "missing mode".to_owned())?;
        let _type = fields.next().ok_or_else(|| "missing type".to_owned())?;
        let oid = fields.next().ok_or_else(|| "missing oid".to_owned())?;
        if fields.next().is_some() {
            return Err("ls-tree header had unexpected extra fields".to_owned());
        }
        let mode =
            u32::from_str_radix(mode, 8).map_err(|e| format!("mode {mode:?} is not octal: {e}"))?;
        let path = PathBuf::from(std::ffi::OsStr::from_bytes(path_bytes));
        entries.push(TreeEntry {
            mode,
            oid: oid.to_owned(),
            path,
        });
    }
    Ok(entries)
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn parses_modes_types_and_paths() {
        // A blob, an executable, a symlink, a gitlink, and a path with a space.
        let raw = b"100644 blob aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\treadme.md\0\
100755 blob bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\tbin/run.sh\0\
120000 blob cccccccccccccccccccccccccccccccccccccccc\tlink\0\
160000 commit dddddddddddddddddddddddddddddddddddddddd\tvendor/sub\0\
100644 blob eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee\ta file\0";
        let entries = parse_ls_tree(raw).expect("parse");
        assert_eq!(entries.len(), 5);
        assert!(entries[0].is_regular_file() && !entries[0].is_executable());
        assert!(entries[1].is_executable());
        assert!(entries[2].is_symlink());
        assert!(entries[3].is_gitlink());
        assert_eq!(entries[4].path, PathBuf::from("a file"));
    }

    #[test]
    fn rejects_a_record_without_a_tab() {
        let raw = b"100644 blob aaaa no-tab-here\0";
        assert!(parse_ls_tree(raw).is_err());
    }
}
