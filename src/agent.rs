use anyhow::{Context, Result};
use std::path::Path;
use std::process::Command;

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
/// obfuscation can slip it). The *real* guardrail is the throwaway worktree:
/// the main checkout is untouchable regardless. Passed to claude as
/// `--disallowedTools`.
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

/// Run one agent full-auto in `workdir`. `claude` is wired first; `codex` is Phase 2.
pub fn run(
    engine: Engine,
    workdir: &Path,
    task: &str,
    model: &str,
    max_turns: u32,
) -> Result<AgentRun> {
    match engine {
        Engine::Claude => run_claude(workdir, task, model, max_turns),
        Engine::Codex => anyhow::bail!("codex engine is Phase 2 — not wired yet"),
    }
}

fn run_claude(workdir: &Path, task: &str, model: &str, max_turns: u32) -> Result<AgentRun> {
    // Headless, full-auto, structured. `bypassPermissions` = no nagging; safe
    // only because the worktree contains the blast radius (see design Q5/Q6).
    let mut cmd = Command::new("claude");
    cmd.current_dir(workdir)
        .arg("-p")
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

    Ok(AgentRun {
        stdout: String::from_utf8_lossy(&out.stdout).to_string(),
        exit_code: out.status.code(),
    })
    // TODO(phase-2): parse claude JSON stdout for session_id + total_cost_usd
    // and thread them into RunMeta.
}
