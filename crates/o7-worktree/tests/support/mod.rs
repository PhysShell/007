//! Shared test scaffolding: build real git repositories in a tempdir, with hooks,
//! smudge filters, and an fsmonitor configured, so the substrate can be proven to run
//! none of them.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    dead_code,
    unreachable_pub
)]

use std::path::{Path, PathBuf};
use std::process::Command;

use o7_worktree::{effective_uid, CommittedRevision, HardenedGit, RunId, StateRoot};
use tempfile::TempDir;

/// A throwaway git repository plus an isolated HOME, so no global/user config leaks in.
pub struct TestRepo {
    pub dir: TempDir,
    pub home: TempDir,
}

impl TestRepo {
    pub fn init() -> Self {
        Self::init_with_object_format("sha1")
    }

    /// A repository whose object store uses `object_format` (`"sha1"` or `"sha256"`).
    pub fn init_with_object_format(object_format: &str) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let home = tempfile::tempdir().unwrap();
        let repo = Self { dir, home };
        repo.git(&["init", "-q", "-b", "main", "--object-format", object_format]);
        repo.git(&["config", "user.email", "t@example.com"]);
        repo.git(&["config", "user.name", "Test"]);
        repo.git(&["config", "commit.gpgsign", "false"]);
        repo
    }

    pub fn path(&self) -> &Path {
        self.dir.path()
    }

    /// Run a plain (non-hardened) git command in the repo with an isolated HOME. Used
    /// only to BUILD fixtures — the substrate under test uses `HardenedGit`.
    pub fn git(&self, args: &[&str]) -> String {
        let out = Command::new("git")
            .args(args)
            .current_dir(self.dir.path())
            .env("HOME", self.home.path())
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_TERMINAL_PROMPT", "0")
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    pub fn write(&self, rel: &str, contents: &[u8]) {
        let path = self.dir.path().join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, contents).unwrap();
    }

    pub fn write_exec(&self, rel: &str, contents: &[u8]) {
        self.write(rel, contents);
        let path = self.dir.path().join(rel);
        std::fs::set_permissions(&path, std::os::unix::fs::PermissionsExt::from_mode(0o755))
            .unwrap();
    }

    pub fn symlink(&self, rel: &str, target: &str) {
        let path = self.dir.path().join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::os::unix::fs::symlink(target, path).unwrap();
    }

    pub fn add_all(&self) {
        self.git(&["add", "-A"]);
    }

    /// Run a plain git command feeding `stdin`, returning stdout.
    pub fn git_stdin(&self, args: &[&str], stdin: &[u8]) -> String {
        use std::io::Write as _;
        let mut child = Command::new("git")
            .args(args)
            .current_dir(self.dir.path())
            .env("HOME", self.home.path())
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(stdin).unwrap();
        let out = child.wait_with_output().unwrap();
        assert!(
            out.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim().to_owned()
    }

    /// Build (via plumbing that bypasses git's own path protections) a commit whose tree
    /// contains an executable `.git/hooks/pre-commit` alongside a normal file, and return
    /// its revision. Used to prove the substrate rejects reserved git-metadata paths.
    pub fn commit_with_dotgit_hook(&self, hook_body: &[u8]) -> CommittedRevision {
        let blob = self.git_stdin(&["hash-object", "-w", "--stdin"], hook_body);
        let hooks = self.git_stdin(
            &["mktree"],
            format!("100755 blob {blob}\tpre-commit\n").as_bytes(),
        );
        let dotgit = self.git_stdin(
            &["mktree"],
            format!("040000 tree {hooks}\thooks\n").as_bytes(),
        );
        let normal = self.git_stdin(&["hash-object", "-w", "--stdin"], b"ok\n");
        let root = self.git_stdin(
            &["mktree"],
            format!("040000 tree {dotgit}\t.git\n100644 blob {normal}\ta.txt\n").as_bytes(),
        );
        let commit = self.git_stdin(&["commit-tree", &root, "-m", "evil"], b"");
        CommittedRevision::from_object_id(commit).unwrap()
    }

    pub fn commit(&self, message: &str) -> CommittedRevision {
        self.git(&["commit", "-q", "-m", message]);
        self.head()
    }

    pub fn head(&self) -> CommittedRevision {
        let git = self.hardened();
        git.resolve_commit("HEAD").unwrap()
    }

    pub fn hardened(&self) -> HardenedGit {
        HardenedGit::new(self.dir.path())
    }
}

/// A state root in its own tempdir (outside any repo).
pub fn state_root() -> (TempDir, StateRoot) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("state");
    let sr = StateRoot::open_or_create(root).unwrap();
    (dir, sr)
}

pub fn run_id(s: &str) -> RunId {
    RunId::new(s).unwrap()
}

pub fn read(path: &Path) -> Vec<u8> {
    std::fs::read(path).unwrap()
}

pub fn mode_of(path: &Path) -> u32 {
    use std::os::unix::fs::MetadataExt as _;
    std::fs::symlink_metadata(path).unwrap().mode() & 0o777
}

pub fn owner_of(path: &Path) -> u32 {
    use std::os::unix::fs::MetadataExt as _;
    std::fs::symlink_metadata(path).unwrap().uid()
}

pub fn our_uid() -> u32 {
    effective_uid()
}

pub fn absolute_paths_under(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                out.push(path.clone());
                if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                    walk(&path, out);
                }
            }
        }
    }
    walk(root, &mut out);
    out
}
