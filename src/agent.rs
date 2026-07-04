use anyhow::{Context, Result};

use crate::sandbox::{Profile, Sandbox};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Engine {
    Claude,
    Codex,
}

impl Engine {
    /// Stable lowercase name for logs, `meta.json`, and error messages. Shared by
    /// every caller so the string can't drift between the two subcommands.
    pub fn label(self) -> &'static str {
        match self {
            Engine::Claude => "claude",
            Engine::Codex => "codex",
        }
    }
}

impl std::str::FromStr for Engine {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self> {
        match s {
            "claude" => Ok(Engine::Claude),
            "codex" => Ok(Engine::Codex),
            other => anyhow::bail!("unknown engine '{other}' (expected: claude | codex)"),
        }
    }
}

/// Irreversible ops the agent must never run.
///
/// Best-effort string deny-list (defense-in-depth, not a sandbox — command
/// obfuscation can slip it; see `docs/opencode-postmortem.md` § permissions).
/// The *real* guardrail is the OS boundary in [`crate::sandbox`]; this list
/// only spares well-behaved runs a pointless destructive turn. Passed to
/// claude as `--disallowedTools`.
pub const DENY: &[&str] = &[
    "Bash(rm -rf*)",
    "Bash(git reset --hard*)",
    "Bash(git clean*)",
    "Bash(git push*)",
];

pub struct AgentRun {
    pub stdout: String,
    pub exit_code: Option<i32>,
}

/// Run one agent full-auto inside `sandbox` (cwd = its worktree). `claude` is
/// wired first; `codex` is Phase 2.
pub fn run(
    engine: Engine,
    sandbox: &Sandbox,
    task: &str,
    model: &str,
    max_turns: u32,
) -> Result<AgentRun> {
    match engine {
        Engine::Claude => run_claude(sandbox, task, model, max_turns),
        Engine::Codex => anyhow::bail!("codex engine is Phase 2 — not wired yet"),
    }
}

fn run_claude(sandbox: &Sandbox, task: &str, model: &str, max_turns: u32) -> Result<AgentRun> {
    // Headless, full-auto, structured. `bypassPermissions` = no nagging; safe
    // only because the sandbox contains the blast radius: the OS boundary
    // (bwrap) is the guardrail, the worktree is the writable surface.
    let mut cmd = sandbox.command("claude", Profile::Agent);
    cmd.arg("-p")
        .arg(task)
        .arg("--model")
        .arg(model)
        .arg("--permission-mode")
        .arg("bypassPermissions")
        .arg("--output-format")
        .arg("json")
        .arg("--max-turns")
        .arg(max_turns.to_string());
    for d in DENY {
        cmd.arg("--disallowedTools").arg(d);
    }

    let out = cmd
        .output()
        .context("spawning `claude` (installed? logged in? on PATH?)")?;

    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    // A sandboxed spawn that dies before the CLI runs (`bwrap: execvp claude:
    // No such file or directory`, missing bind, …) yields empty stdout + a
    // nonzero exit. Recording that as a normal run would fake an agent result —
    // fail loudly with stderr instead (it names the path to `--sandbox-ro` in).
    if stdout.trim().is_empty() && !out.status.success() {
        anyhow::bail!(
            "claude produced no output (exit {:?}); stderr:\n{}",
            out.status.code(),
            String::from_utf8_lossy(&out.stderr)
        );
    }

    Ok(AgentRun {
        stdout,
        exit_code: out.status.code(),
    })
    // TODO(phase-2): parse claude JSON stdout for session_id + total_cost_usd
    // and thread them into RunMeta.
}
