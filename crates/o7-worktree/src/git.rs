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
use std::io::Read as _;
use std::os::unix::fs::MetadataExt as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::identity::{CanonicalRepoId, CommittedRevision, IdentityError};

/// Wall-clock ceiling for any single git child. A well-behaved repository resolves,
/// lists, and reads well under this; a hung or deadlocked git is killed rather than
/// blocking the caller forever.
const GIT_WALL_CLOCK: Duration = Duration::from_secs(120);
/// Byte ceiling for a control command's stdout (`init`, `update-ref`, `read-tree`,
/// `rev-parse`): these emit at most a few lines, so a flood here is pathological.
const MAX_CONTROL_STDOUT: u64 = 16 * 1024 * 1024;
/// Byte ceiling for `ls-tree` stdout. Sized for very large trees while still bounding
/// memory against a git that ignores the tree it was asked to list.
const MAX_LS_TREE_STDOUT: u64 = 1024 * 1024 * 1024;
/// Byte ceiling for `cat-file blob` stdout — a hard backstop over the per-blob budget
/// the materialize preflight already enforces, so a whole-blob read is never unbounded.
const MAX_BLOB_STDOUT: u64 = 1024 * 1024 * 1024;
/// Byte ceiling for any child's stderr — only ever used to build an error message.
const MAX_STDERR: u64 = 256 * 1024;
/// How often the wait loop polls the child while enforcing the wall-clock deadline.
const POLL_INTERVAL: Duration = Duration::from_millis(5);

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
    #[error("git {command} did not finish within {timeout:?}; the process was killed")]
    Timeout {
        command: String,
        timeout: std::time::Duration,
    },
    #[error("git {command} produced more than {cap} bytes on {stream}; the process was killed")]
    OutputTooLarge {
        command: String,
        stream: &'static str,
        cap: u64,
    },
    #[error("draining git {command} output failed: {source}")]
    Drain {
        command: String,
        #[source]
        source: std::io::Error,
    },
    #[error("git {command} produced output that could not be parsed: {detail}")]
    Parse { command: String, detail: String },
    #[error("resolving the git common directory failed: {0}")]
    CommonDir(#[source] std::io::Error),
    #[error("writing the detached worktree gitdir metadata at {path:?} failed: {source}")]
    Metadata {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
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
    /// The blob size in bytes as recorded by `ls-tree -l`, when the entry is a blob
    /// (`None` for gitlinks, whose size git reports as `-`). Used by the preflight to
    /// enforce blob and cumulative byte budgets BEFORE any bytes are read or written.
    pub size: Option<u64>,
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
        // spaces or odd bytes is unambiguous. -l: include the blob size, so the preflight
        // can enforce byte budgets before reading. Record:
        // "<mode> SP <type> SP <oid> SP <size> TAB <path>".
        let raw = self.run(
            &[
                OsStr::new("ls-tree"),
                OsStr::new("-r"),
                OsStr::new("-z"),
                OsStr::new("-l"),
                OsStr::new("--full-tree"),
                OsStr::new("--end-of-options"),
                OsStr::new(revision.as_str()),
            ],
            MAX_LS_TREE_STDOUT,
        )?;
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
        self.run(
            &[
                OsStr::new("cat-file"),
                OsStr::new("blob"),
                OsStr::new("--end-of-options"),
                OsStr::new(oid),
            ],
            MAX_BLOB_STDOUT,
        )
    }

    /// Turn an already-created (owner-only, empty) directory into a REAL, self-contained
    /// detached git worktree of `revision`, WITHOUT a checkout:
    ///
    /// 1. `git init` a fresh gitdir inside the directory — self-contained, so there is no
    ///    admin entry in the source repository and no repo-global prune is ever needed to
    ///    clean it up (deletion is just removing the directory);
    /// 2. TEMPORARILY borrow the source repository's objects via `objects/info/alternates`
    ///    so the committed revision is reachable;
    /// 3. detach `HEAD` at `revision` (`update-ref --no-deref`, no working-tree change);
    /// 4. populate the index from the committed tree (`read-tree`, no smudge, no checkout);
    /// 5. COPY the whole object closure reachable from `revision` into this worktree's own
    ///    object database (`rev-list --objects` piped into `pack-objects`) and DELETE the
    ///    alternates file, so the worktree no longer depends on the source repository.
    ///
    /// The working-tree bytes are then written from the object store by
    /// [`crate::materialize`], so no smudge filter, hook, or fsmonitor ever runs — yet the
    /// result is a genuine, self-contained git worktree: `git -C <dir> rev-parse HEAD` is
    /// the revision, `git -C <dir> status` is clean, and both keep working even if the
    /// source repository's objects later vanish.
    ///
    /// # Errors
    /// [`GitError`] if any git step fails or the alternates file cannot be written/removed.
    pub fn init_detached_worktree(
        &self,
        worktree_dir: &Path,
        repo: &CanonicalRepoId,
        revision: &CommittedRevision,
    ) -> Result<(), GitError> {
        use std::os::unix::ffi::OsStrExt as _;

        self.run_in(
            worktree_dir,
            &[OsStr::new("init"), OsStr::new("-q")],
            MAX_CONTROL_STDOUT,
        )?;

        // Borrow the source objects for the duration of the build. Written directly under
        // our owner-only `.git`; removed in step 5 once the closure has been copied in.
        let alternates = worktree_dir.join(".git/objects/info/alternates");
        let mut line = repo.git_common_dir.join("objects").into_os_string();
        line.push("\n");
        std::fs::write(&alternates, line.as_bytes()).map_err(|source| GitError::Metadata {
            path: alternates.clone(),
            source,
        })?;

        let oid = OsStr::new(revision.as_str());
        self.run_in(
            worktree_dir,
            &[
                OsStr::new("update-ref"),
                OsStr::new("--no-deref"),
                OsStr::new("HEAD"),
                oid,
            ],
            MAX_CONTROL_STDOUT,
        )?;
        self.run_in(
            worktree_dir,
            &[OsStr::new("read-tree"), oid],
            MAX_CONTROL_STDOUT,
        )?;

        // Copy the reachable object closure into our own objectdb, then cut the tether.
        self.copy_object_closure(worktree_dir, revision)?;
        std::fs::remove_file(&alternates).map_err(|source| GitError::Metadata {
            path: alternates.clone(),
            source,
        })?;
        Ok(())
    }

    /// Copy the entire object closure reachable from `revision` (the commit, its history,
    /// and every tree/blob) from the borrowed source objects into `worktree_dir`'s own
    /// object database, so it becomes self-contained.
    ///
    /// `rev-list --objects --no-object-names <oid>` enumerates the closure as bare oids;
    /// `pack-objects` reads that list on stdin and writes a single pack into
    /// `.git/objects/pack`. Both run inside the worktree, where the alternates tether is
    /// still present, so the source objects are readable exactly for this copy.
    fn copy_object_closure(
        &self,
        worktree_dir: &Path,
        revision: &CommittedRevision,
    ) -> Result<(), GitError> {
        let oids = self.run_in(
            worktree_dir,
            &[
                OsStr::new("rev-list"),
                OsStr::new("--objects"),
                OsStr::new("--no-object-names"),
                OsStr::new("--end-of-options"),
                OsStr::new(revision.as_str()),
            ],
            MAX_LS_TREE_STDOUT,
        )?;

        // `pack-objects <base>` writes <base>-<hash>.pack/.idx; the objects then live in
        // this worktree's own pack directory, no longer borrowed from the source.
        let pack_base = worktree_dir.join(".git/objects/pack/o7-closure");
        self.run_in_stdin(
            worktree_dir,
            &[OsStr::new("pack-objects"), pack_base.as_os_str()],
            oids,
            MAX_CONTROL_STDOUT,
        )?;
        Ok(())
    }

    // ---- internals ----

    /// The explicit environment for every git child, run with `-C dir`: cleared, then
    /// exactly this allowlist. Nothing is inherited (no `GIT_*`, no `GIT_EXTERNAL_DIFF`,
    /// no askpass, no proxy). Every place a repository could name a helper — hooks,
    /// fsmonitor, filters, external diff, attributes/excludes files, editors,
    /// auto-gc/maintenance — is neutralized per invocation, so even a checkout-shaped
    /// command (`read-tree`, `init`) runs no repository-controlled code.
    fn base_command_in(&self, dir: &Path) -> Command {
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
            // Ignore the system-wide gitattributes (a filter/diff could be named there).
            ("GIT_ATTR_NOSYSTEM", "1"),
            // Never honor refs/replace (a committed replace ref must not silently change
            // the materialized tree) and never lazily fetch a missing object over the
            // network (a partial-clone gap must fail, not phone home).
            ("GIT_NO_REPLACE_OBJECTS", "1"),
            ("GIT_NO_LAZY_FETCH", "1"),
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
        cmd.arg("-C").arg(dir);
        // Defense in depth on top of plumbing-only: even a repo-local config pointing at a
        // helper is overridden per invocation. Highest-precedence `-c` flags win over any
        // included/repo config.
        for kv in [
            "core.hooksPath=/dev/null",
            "core.fsmonitor=false",
            "core.fsmonitorHookVersion=0",
            "core.autocrlf=false",
            "core.symlinks=true",
            "core.attributesFile=/dev/null",
            "core.excludesFile=/dev/null",
            "core.editor=false",
            "core.pager=cat",
            "diff.external=",
            "gc.auto=0",
            "maintenance.auto=false",
        ] {
            cmd.arg("-c").arg(kv);
        }
        cmd.stdin(std::process::Stdio::null());
        cmd
    }

    fn run(&self, args: &[&OsStr], max_stdout: u64) -> Result<Vec<u8>, GitError> {
        self.run_in(&self.repo, args, max_stdout)
    }

    fn run_in(&self, dir: &Path, args: &[&OsStr], max_stdout: u64) -> Result<Vec<u8>, GitError> {
        let mut cmd = self.base_command_in(dir);
        cmd.args(args);
        run_bounded(
            cmd,
            &display_args(args),
            GIT_WALL_CLOCK,
            None,
            max_stdout,
            MAX_STDERR,
        )
    }

    /// Like [`Self::run_in`] but feeds `stdin` to the child (e.g. an oid list to
    /// `pack-objects`). The bytes are written on their own thread so a child that stops
    /// reading cannot deadlock the drain.
    fn run_in_stdin(
        &self,
        dir: &Path,
        args: &[&OsStr],
        stdin: Vec<u8>,
        max_stdout: u64,
    ) -> Result<Vec<u8>, GitError> {
        let mut cmd = self.base_command_in(dir);
        cmd.args(args);
        run_bounded(
            cmd,
            &display_args(args),
            GIT_WALL_CLOCK,
            Some(stdin),
            max_stdout,
            MAX_STDERR,
        )
    }

    fn run_text(&self, args: &[&str]) -> Result<String, GitError> {
        let os: Vec<OsString> = args.iter().map(OsString::from).collect();
        let refs: Vec<&OsStr> = os.iter().map(OsString::as_os_str).collect();
        let bytes = self.run(&refs, MAX_CONTROL_STDOUT)?;
        String::from_utf8(bytes).map_err(|e| GitError::Parse {
            command: display_args(&refs),
            detail: format!("non-utf8 output: {e}"),
        })
    }
}

/// Run a prepared command with a wall-clock deadline and independent per-stream byte
/// caps, killing the child rather than blocking forever or buffering without bound.
///
/// stdout and stderr are each drained on their own thread into a `cap + 1`-bounded
/// buffer, so neither a flooding writer nor a full-pipe stall can wedge the other. The
/// wait loop polls the child until it exits or the deadline passes; on the deadline (or
/// a wait error) the child is killed and reaped so no zombie or reader thread is left
/// behind. A child that exits but overran its stdout cap is reported as
/// [`GitError::OutputTooLarge`], never returned as truncated-but-successful output.
fn run_bounded(
    mut cmd: Command,
    command: &str,
    wall_clock: Duration,
    stdin: Option<Vec<u8>>,
    max_stdout: u64,
    max_stderr: u64,
) -> Result<Vec<u8>, GitError> {
    use std::io::Write as _;
    use std::os::unix::process::CommandExt as _;

    cmd.stdin(if stdin.is_some() {
        Stdio::piped()
    } else {
        Stdio::null()
    });
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    // Put the child in its own process group (pgid == child pid) so that on the deadline
    // we can SIGKILL the WHOLE group: a git that forked a helper cannot leave an orphan
    // holding the output pipe open and blocking our drain threads. `process_group` is the
    // safe, `unsafe`-free way to request this (no `pre_exec`).
    cmd.process_group(0);
    let mut child = cmd.spawn().map_err(GitError::Spawn)?;
    let pgid = nix::unistd::Pid::from_raw(i32::try_from(child.id()).unwrap_or(0));

    // Feed stdin on its own thread: a child that stops reading (or dies) must not block us.
    let stdin_join = match stdin {
        Some(bytes) => child.stdin.take().map(|mut pipe| {
            std::thread::spawn(move || {
                // A broken pipe (child exited early) is not our error to report.
                let _ = pipe.write_all(&bytes);
                let _ = pipe.flush();
                // Dropping `pipe` closes the write end so the child sees EOF.
            })
        }),
        None => None,
    };

    let stdout = child.stdout.take().ok_or_else(|| GitError::Drain {
        command: command.to_owned(),
        source: std::io::Error::other("child stdout pipe was not captured"),
    })?;
    let stderr = child.stderr.take().ok_or_else(|| GitError::Drain {
        command: command.to_owned(),
        source: std::io::Error::other("child stderr pipe was not captured"),
    })?;
    let out_join = std::thread::spawn(move || drain_capped(stdout, max_stdout));
    let err_join = std::thread::spawn(move || drain_capped(stderr, max_stderr));

    let deadline = Instant::now() + wall_clock;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                if Instant::now() >= deadline {
                    kill_group(pgid, &mut child);
                    let _ = out_join.join();
                    let _ = err_join.join();
                    if let Some(j) = stdin_join {
                        let _ = j.join();
                    }
                    return Err(GitError::Timeout {
                        command: command.to_owned(),
                        timeout: wall_clock,
                    });
                }
                std::thread::sleep(POLL_INTERVAL);
            }
            Err(source) => {
                kill_group(pgid, &mut child);
                let _ = out_join.join();
                let _ = err_join.join();
                if let Some(j) = stdin_join {
                    let _ = j.join();
                }
                return Err(GitError::Spawn(source));
            }
        }
    };

    if let Some(j) = stdin_join {
        let _ = j.join();
    }
    let (stdout, stdout_overflow) = join_drain(out_join, command)?;
    let (stderr, _stderr_overflow) = join_drain(err_join, command)?;

    if stdout_overflow {
        return Err(GitError::OutputTooLarge {
            command: command.to_owned(),
            stream: "stdout",
            cap: max_stdout,
        });
    }
    if status.success() {
        Ok(stdout)
    } else {
        Err(GitError::Command {
            command: command.to_owned(),
            status: status.to_string(),
            stderr: String::from_utf8_lossy(&stderr).trim().to_owned(),
        })
    }
}

/// SIGKILL the child's whole process group, then reap the leader so no zombie remains.
/// Killing the group (not just the leader) reaps any helper the child forked, which
/// releases the output pipe so the drain threads' `read_to_end` can return.
fn kill_group(pgid: nix::unistd::Pid, child: &mut std::process::Child) {
    let _ = nix::sys::signal::killpg(pgid, nix::sys::signal::Signal::SIGKILL);
    // Belt-and-braces: also kill the leader directly in case the group could not be
    // signalled (e.g. pgid was unavailable), then reap it.
    let _ = child.kill();
    let _ = child.wait();
}

/// Read up to `cap + 1` bytes so an overrun is observable, truncating to `cap` and
/// reporting `overflow = true` when the stream had more.
fn drain_capped(reader: impl std::io::Read, cap: u64) -> std::io::Result<(Vec<u8>, bool)> {
    let mut buf = Vec::new();
    let read = reader.take(cap.saturating_add(1)).read_to_end(&mut buf)? as u64;
    let overflow = read > cap;
    if overflow {
        buf.truncate(usize::try_from(cap).unwrap_or(usize::MAX));
    }
    Ok((buf, overflow))
}

/// Join a drain thread, mapping a panicked reader or an i/o error to [`GitError::Drain`].
fn join_drain(
    handle: std::thread::JoinHandle<std::io::Result<(Vec<u8>, bool)>>,
    command: &str,
) -> Result<(Vec<u8>, bool), GitError> {
    handle
        .join()
        .map_err(|_| GitError::Drain {
            command: command.to_owned(),
            source: std::io::Error::other("output-draining thread panicked"),
        })?
        .map_err(|source| GitError::Drain {
            command: command.to_owned(),
            source,
        })
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
        // `-l` right-justifies the size, so the header has run-together spaces; split on
        // whitespace. The path is AFTER the TAB, so it is never in `header`.
        let mut fields = header.split_whitespace();
        let mode = fields.next().ok_or_else(|| "missing mode".to_owned())?;
        let _type = fields.next().ok_or_else(|| "missing type".to_owned())?;
        let oid = fields.next().ok_or_else(|| "missing oid".to_owned())?;
        let size_field = fields.next().ok_or_else(|| "missing size".to_owned())?;
        if fields.next().is_some() {
            return Err("ls-tree header had unexpected extra fields".to_owned());
        }
        let mode =
            u32::from_str_radix(mode, 8).map_err(|e| format!("mode {mode:?} is not octal: {e}"))?;
        // Blobs carry a byte size; a gitlink's size is reported as `-`.
        let size = if size_field == "-" {
            None
        } else {
            Some(
                size_field
                    .parse::<u64>()
                    .map_err(|e| format!("size {size_field:?} is not a number: {e}"))?,
            )
        };
        let path = PathBuf::from(std::ffi::OsStr::from_bytes(path_bytes));
        entries.push(TreeEntry {
            mode,
            oid: oid.to_owned(),
            size,
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
    fn parses_modes_types_sizes_and_paths() {
        // `-l` format: "<mode> <type> <oid> <size> TAB <path>" (size right-justified, `-`
        // for a gitlink). A blob, an executable, a symlink, a gitlink, and a spaced path.
        let raw = b"100644 blob aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa       12\treadme.md\0\
100755 blob bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb        7\tbin/run.sh\0\
120000 blob cccccccccccccccccccccccccccccccccccccccc        9\tlink\0\
160000 commit dddddddddddddddddddddddddddddddddddddddd        -\tvendor/sub\0\
100644 blob eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee        3\ta file\0";
        let entries = parse_ls_tree(raw).expect("parse");
        assert_eq!(entries.len(), 5);
        assert!(entries[0].is_regular_file() && !entries[0].is_executable());
        assert_eq!(entries[0].size, Some(12));
        assert!(entries[1].is_executable());
        assert!(entries[2].is_symlink());
        assert!(entries[3].is_gitlink());
        assert_eq!(entries[3].size, None); // gitlink size is `-`
        assert_eq!(entries[4].path, PathBuf::from("a file"));
        assert_eq!(entries[4].size, Some(3));
    }

    #[test]
    fn rejects_a_record_without_a_tab() {
        let raw = b"100644 blob aaaa 5 no-tab-here\0";
        assert!(parse_ls_tree(raw).is_err());
    }

    #[test]
    fn a_hung_child_is_killed_at_the_wall_clock_deadline() {
        // A child that never exits must not block the caller: the deadline fires, the
        // process is killed, and Timeout is returned promptly (well under the sleep).
        let mut cmd = Command::new("/bin/sh");
        cmd.args(["-c", "sleep 30"]);
        let start = Instant::now();
        let result = run_bounded(
            cmd,
            "sleep 30",
            Duration::from_millis(150),
            None,
            1024,
            1024,
        );
        let elapsed = start.elapsed();
        assert!(
            matches!(result, Err(GitError::Timeout { .. })),
            "expected Timeout, got {result:?}"
        );
        assert!(
            elapsed < Duration::from_secs(5),
            "runner should return near the deadline, took {elapsed:?}"
        );
    }

    #[test]
    fn a_child_that_overruns_the_stdout_cap_is_rejected_not_truncated() {
        // The child writes far more than the cap but exits on its own; the runner must
        // surface OutputTooLarge rather than silently returning a truncated success.
        let mut cmd = Command::new("/bin/sh");
        cmd.args(["-c", "printf 'x%.0s' $(seq 1 500)"]);
        let result = run_bounded(cmd, "flood", Duration::from_secs(10), None, 100, 1024);
        assert!(
            matches!(
                result,
                Err(GitError::OutputTooLarge {
                    stream: "stdout",
                    cap: 100,
                    ..
                })
            ),
            "expected OutputTooLarge, got {result:?}"
        );
    }

    #[test]
    fn a_well_behaved_child_returns_its_bytes() {
        let mut cmd = Command::new("/bin/sh");
        cmd.args(["-c", "printf 'hello'"]);
        let out = run_bounded(cmd, "echo", Duration::from_secs(10), None, 1024, 1024)
            .expect("well-behaved child succeeds");
        assert_eq!(out, b"hello");
    }
}
