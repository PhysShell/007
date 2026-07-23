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
        let dir = tempfile::tempdir().unwrap();
        let home = tempfile::tempdir().unwrap();
        let repo = Self { dir, home };
        repo.git(&["init", "-q", "-b", "main"]);
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
