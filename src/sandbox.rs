//! OS-level confinement for `o7 run` subprocesses (agent + gate steps).
//!
//! Lesson learned from the OpenCode postmortem (`docs/opencode-postmortem.md`):
//! string filters over shell commands are not a boundary — shell is a language
//! (aliases, substitution, heredocs, interpreters, indirect execution), so the
//! only real guardrail is one the *process* cannot cross. This module supplies
//! that boundary with bubblewrap (`bwrap`):
//!
//! - read-only root (`--ro-bind / /`) — the whole FS visible but immutable;
//! - `tmpfs` over `/home` and `/root` — secrets (`~/.ssh`, `~/.config`, tokens,
//!   browser profiles, keyrings) are not merely write-protected, they are
//!   *invisible*; known toolchain prefixes are bound back read-only;
//! - the worktree (and the shared `.git` backing it) bound read-write — the
//!   blast radius stays where `diff_vs_base` can see it;
//! - `--clearenv` + a small allowlist — ambient env tokens don't ride along;
//! - `--unshare-all`, network re-shared only for the agent profile (the claude
//!   subscription API needs it); gate steps get **no network**, so a malicious
//!   `gate.toml` in a target repo can execute code but not exfiltrate;
//! - `--die-with-parent --new-session` — no orphans, no TIOCSTI tty pushback.
//!
//! Mode policy: `auto` requires bwrap and **fails hard** when it's missing.
//! Falling back to unsandboxed silently would be the false-sense-of-security
//! antipattern this module exists to kill — opting out is explicit
//! (`--sandbox none`) and loud.

use anyhow::{Context, Result};
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::Command;

/// What the operator asked for on the CLI (`--sandbox`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SandboxMode {
    /// Require bwrap; hard error if absent (no silent downgrade).
    Auto,
    /// Same as `Auto` today; reserved so future backends don't repurpose `auto`.
    Bwrap,
    /// Unsandboxed (pre-sandbox behavior). Loud warning at resolve time.
    None,
}

impl std::str::FromStr for SandboxMode {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self> {
        match s {
            "auto" => Ok(SandboxMode::Auto),
            "bwrap" => Ok(SandboxMode::Bwrap),
            "none" => Ok(SandboxMode::None),
            other => {
                anyhow::bail!("unknown sandbox mode '{other}' (expected: auto | bwrap | none)")
            }
        }
    }
}

/// Which confinement actually runs (after `resolve`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Backend {
    Bwrap,
    None,
}

/// Per-subprocess privilege profile. The difference is deliberate and small:
/// the agent needs the network (subscription API) and its own state dir; gate
/// steps — attacker-controlled if the target repo isn't trusted — get neither.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Profile {
    /// `claude` full-auto: network shared, `~/.claude`* bound read-write.
    Agent,
    /// `bash -lc <gate step>`: no network, no agent state.
    Gate,
}

/// Env vars that survive `--clearenv`. Everything else — `ANTHROPIC_API_KEY`,
/// `GITHUB_TOKEN`, `AWS_*`, whatever the parent shell had — stays outside.
const ENV_ALLOWLIST: &[&str] = &["PATH", "HOME", "TERM", "LANG", "LC_ALL"];

/// Home-relative toolchain prefixes bound back (read-only) over the `/home`
/// tmpfs when they exist. Narrow on purpose: `~/.local/share` as a whole would
/// re-expose keyrings; `~/.nvm`/`~/.volta`/`~/.bun` hold node toolchains that
/// `claude` (npm-installed) may live in. Anything else goes via `--sandbox-ro`.
const TOOL_PREFIXES: &[&str] = &[
    ".nvm",
    ".volta",
    ".bun",
    ".cargo",
    ".rustup",
    ".local/bin",
    ".local/share/claude",
    ".local/share/nvm",
    ".local/share/pnpm",
];

/// Agent state bound read-write for [`Profile::Agent`] only (auth/session live
/// here; the native/npm CLI updates them during a run).
const AGENT_STATE: &[&str] = &[".claude", ".claude.json"];

/// A resolved sandbox: knows the backend, the worktree, and the extra binds.
/// One instance per `o7 run`; build subprocesses via [`Sandbox::command`].
pub struct Sandbox {
    backend: Backend,
    /// Canonicalized worktree — the only default read-write surface.
    worktree: PathBuf,
    /// The shared `.git` behind the worktree. A linked worktree is not
    /// self-contained (index/HEAD under `.git/worktrees/<n>`, objects/refs in
    /// the shared store), so without this bind every `git` op inside breaks.
    git_common_dir: Option<PathBuf>,
    extra_ro: Vec<PathBuf>,
    extra_rw: Vec<PathBuf>,
}

impl Sandbox {
    /// Resolve `mode` against the machine and build the run's sandbox.
    /// `worktree` must exist (it is canonicalized here).
    pub fn new(
        mode: SandboxMode,
        worktree: &Path,
        git_common_dir: Option<PathBuf>,
        extra_ro: Vec<PathBuf>,
        extra_rw: Vec<PathBuf>,
    ) -> Result<Sandbox> {
        let backend = match mode {
            SandboxMode::Auto | SandboxMode::Bwrap => {
                if !bwrap_available() {
                    anyhow::bail!(
                        "sandbox: `bwrap` not found on PATH. Install bubblewrap (it is in \
                         the dev shell / `apt install bubblewrap`), or run unsandboxed \
                         EXPLICITLY with `--sandbox none` — there is no silent fallback."
                    );
                }
                Backend::Bwrap
            }
            SandboxMode::None => {
                eprintln!(
                    "[o7] WARNING: --sandbox none — agent and gate steps run UNCONFINED \
                     (full read of $HOME and secrets, arbitrary writes, open network). \
                     The worktree is cleanup convenience, not a boundary."
                );
                Backend::None
            }
        };
        let worktree = worktree
            .canonicalize()
            .with_context(|| format!("canonicalizing worktree {}", worktree.display()))?;
        Ok(Sandbox {
            backend,
            worktree,
            git_common_dir,
            extra_ro,
            extra_rw,
        })
    }

    /// Honest one-word status for run logs — never claim confinement that
    /// isn't there (that would be the OpenCode mistake in miniature).
    pub fn label(&self) -> &'static str {
        match self.backend {
            Backend::Bwrap => "bwrap-sandboxed",
            Backend::None => "UNSANDBOXED",
        }
    }

    /// Build the `Command` for `program` under this sandbox and `profile`.
    /// The working directory is the worktree in both backends; callers append
    /// program args as usual.
    pub fn command(&self, program: &str, profile: Profile) -> Command {
        match self.backend {
            Backend::None => {
                let mut cmd = Command::new(program);
                cmd.current_dir(&self.worktree);
                cmd
            }
            Backend::Bwrap => {
                let mut cmd = Command::new("bwrap");
                let home = std::env::var_os("HOME").map(PathBuf::from);
                let path = std::env::var_os("PATH");
                cmd.args(bwrap_args(
                    &self.worktree,
                    self.git_common_dir.as_deref(),
                    &self.extra_ro,
                    &self.extra_rw,
                    profile,
                    home.as_deref(),
                    path.as_deref(),
                ));
                cmd.arg(program);
                cmd
            }
        }
    }
}

fn bwrap_available() -> bool {
    Command::new("bwrap")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Pure builder for the bwrap argv (everything before the target program).
/// Kept free of I/O and env reads so the confinement policy is unit-testable.
///
/// Bind order is load-bearing: later mounts shadow earlier ones, so the
/// sequence is root(ro) → tmpfs blankets → selective re-binds.
fn bwrap_args(
    worktree: &Path,
    git_common_dir: Option<&Path>,
    extra_ro: &[PathBuf],
    extra_rw: &[PathBuf],
    profile: Profile,
    home: Option<&Path>,
    path_env: Option<&OsStr>,
) -> Vec<OsString> {
    let mut a: Vec<OsString> = Vec::new();
    let mut push = |s: &str| a.push(OsString::from(s));

    // 1. Whole FS visible, nothing writable.
    push("--ro-bind");
    push("/");
    push("/");
    push("--dev");
    push("/dev");
    push("--proc");
    push("/proc");
    push("--tmpfs");
    push("/tmp");
    // 2. Blank out user data: secrets become invisible, not just read-only.
    push("--tmpfs");
    push("/home");
    push("--tmpfs");
    push("/root");

    let mut bind = |kind: &str, p: &Path| {
        a.push(OsString::from(kind));
        a.push(p.as_os_str().to_os_string());
        a.push(p.as_os_str().to_os_string());
    };

    // 3. Selective re-binds over the tmpfs blankets.
    if let Some(home) = home {
        for rel in TOOL_PREFIXES {
            bind("--ro-bind-try", &home.join(rel));
        }
        if profile == Profile::Agent {
            for rel in AGENT_STATE {
                bind("--bind-try", &home.join(rel));
            }
        }
    }
    for p in extra_ro {
        bind("--ro-bind-try", p);
    }
    // 4. The blast radius: worktree + the shared .git that makes git work.
    bind("--bind", worktree);
    if let Some(git) = git_common_dir {
        bind("--bind-try", git);
    }
    for p in extra_rw {
        bind("--bind-try", p);
    }

    // 5. Env: clear, then re-admit the allowlist from the parent.
    a.push(OsString::from("--clearenv"));
    for key in ENV_ALLOWLIST {
        let val = match *key {
            "PATH" => path_env.map(OsStr::to_os_string),
            "HOME" => home.map(|h| h.as_os_str().to_os_string()),
            // Pure function: TERM/LANG/LC_ALL are cheap to re-read at the call
            // site if ever needed; today a sane fixed value beats leaking env.
            "TERM" => Some(OsString::from("dumb")),
            "LANG" => Some(OsString::from("C.UTF-8")),
            _ => None,
        };
        if let Some(val) = val {
            a.push(OsString::from("--setenv"));
            a.push(OsString::from(*key));
            a.push(val);
        }
    }

    // 6. Isolation: all namespaces unshared; network only for the agent.
    a.push(OsString::from("--unshare-all"));
    if profile == Profile::Agent {
        a.push(OsString::from("--share-net"));
    }
    a.push(OsString::from("--die-with-parent"));
    a.push(OsString::from("--new-session"));
    a.push(OsString::from("--chdir"));
    a.push(worktree.as_os_str().to_os_string());
    a.push(OsString::from("--"));
    a
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args_for(profile: Profile) -> Vec<OsString> {
        bwrap_args(
            Path::new("/work/wt"),
            Some(Path::new("/repo/.git")),
            &[PathBuf::from("/opt/venv")],
            &[],
            profile,
            Some(Path::new("/home/u")),
            Some(OsStr::new("/usr/bin")),
        )
    }

    fn pos(args: &[OsString], needle: &str) -> Option<usize> {
        args.iter().position(|a| a == OsStr::new(needle))
    }

    #[test]
    fn gate_profile_has_no_network_and_no_agent_state() {
        let args = args_for(Profile::Gate);
        assert!(pos(&args, "--share-net").is_none(), "gate must be offline");
        assert!(
            !args.iter().any(|a| a == OsStr::new("/home/u/.claude")),
            "gate must not see agent auth/session state"
        );
        assert!(pos(&args, "--unshare-all").is_some());
    }

    #[test]
    fn agent_profile_shares_net_and_binds_state() {
        let args = args_for(Profile::Agent);
        assert!(pos(&args, "--share-net").is_some(), "agent needs the API");
        assert!(args.iter().any(|a| a == OsStr::new("/home/u/.claude.json")));
    }

    #[test]
    fn secrets_are_blanked_before_selective_rebinds() {
        // Later bwrap mounts shadow earlier ones: the /home tmpfs must come
        // BEFORE any home re-bind, or the blanket hides the toolchains instead
        // of the secrets.
        let args = args_for(Profile::Agent);
        let tmpfs_home = pos(&args, "/home").unwrap_or(usize::MAX);
        let rebind = pos(&args, "/home/u/.nvm").unwrap_or(0);
        assert!(tmpfs_home < rebind, "tmpfs /home must precede re-binds");
    }

    #[test]
    fn env_is_cleared_and_worktree_writable() {
        let args = args_for(Profile::Gate);
        assert!(pos(&args, "--clearenv").is_some());
        let bind = pos(&args, "--bind").unwrap_or(usize::MAX);
        assert!(
            args.get(bind.saturating_add(1)) == Some(&OsString::from("/work/wt")),
            "worktree is the first plain --bind"
        );
        // extra_ro made it in as ro.
        assert!(args.iter().any(|a| a == OsStr::new("/opt/venv")));
        // and the run ends with `--chdir <worktree> --`.
        assert!(args.last() == Some(&OsString::from("--")));
    }

    #[test]
    fn mode_parses_and_rejects_unknown() {
        assert!("auto".parse::<SandboxMode>().is_ok());
        assert!("bwrap".parse::<SandboxMode>().is_ok());
        assert!("none".parse::<SandboxMode>().is_ok());
        assert!("docker".parse::<SandboxMode>().is_err());
    }
}
