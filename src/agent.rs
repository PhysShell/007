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

/// A provider's own session/conversation identity (e.g. Claude's
/// `session_id`) — Q-Deck R1's `docs/q-deck/r1-command.md` §0: opaque to
/// `o7-run`, never a substitute for the canonical `RunId`, never accepted
/// directly from an HTTP client (§8), never logged as a credential.
///
/// Validated non-empty and free of control characters at construction —
/// `new` is the ONE place a raw string becomes trusted. Parsing a fresh
/// provider response is lenient (an absent/malformed value there is just
/// "no session captured this time", §9.1); reading one BACK out of durable
/// storage to drive a continuation is strict (§6's fail-closed rule) —
/// both paths go through this same constructor so "valid" means the same
/// thing in both places.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct ProviderSessionId(String);

/// A string that cannot be a [`ProviderSessionId`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ProviderSessionIdError {
    #[error("provider session id is empty")]
    Empty,
    #[error("provider session id contains a control character")]
    ControlCharacter,
}

impl ProviderSessionId {
    /// # Errors
    /// [`ProviderSessionIdError`] if `raw` is empty or contains a control
    /// character (defense against a stray newline/embedded-secret-looking
    /// value ending up somewhere this gets logged or framed as one CLI arg).
    pub fn new(raw: impl Into<String>) -> Result<Self, ProviderSessionIdError> {
        let raw = raw.into();
        if raw.is_empty() {
            return Err(ProviderSessionIdError::Empty);
        }
        if raw.chars().any(char::is_control) {
            return Err(ProviderSessionIdError::ControlCharacter);
        }
        Ok(Self(raw))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

pub struct AgentRun {
    pub stdout: String,
    pub exit_code: Option<i32>,
    /// The provider's own session identity, if this call's structured
    /// output carried a valid one (§9.1) — `None` for a malformed/absent
    /// value, an older non-JSON output shape, or a provider with no
    /// concept of session continuity. Never fails the run by itself; a
    /// caller that specifically NEEDS a session for a future continuation
    /// checks this (or the durably-stored copy) explicitly.
    pub session_id: Option<ProviderSessionId>,
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

/// Continue a PRIOR provider session with a new command — Q-Deck R1's
/// distinct continuation primitive (`docs/q-deck/r1-command.md` §9.2), not
/// a pile of new nullable parameters bolted onto [`run`]. `command` is
/// carried as exactly ONE `argv` element via `std::process::Command` —
/// never a shell, never a concatenated string — so it cannot be
/// reinterpreted regardless of quotes, `$()`, backslashes, or embedded
/// newlines it contains.
///
/// # Errors
/// Any failure spawning or running the provider process.
pub fn continue_session(
    session_id: &ProviderSessionId,
    workdir: &Path,
    command: &str,
    model: &str,
    max_turns: u32,
) -> Result<AgentRun> {
    let mut cmd = Command::new("claude");
    cmd.current_dir(workdir)
        .arg("--resume")
        .arg(session_id.as_str())
        .arg("-p")
        .arg(command)
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
        .context("spawning `claude --resume` (installed? logged in? on PATH?)")?;
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let session_id = parse_session_id(&stdout);

    Ok(AgentRun {
        stdout,
        exit_code: out.status.code(),
        session_id,
    })
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
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let session_id = parse_session_id(&stdout);

    Ok(AgentRun {
        stdout,
        exit_code: out.status.code(),
        session_id,
    })
}

/// Parse the SAME structured JSON envelope `src/judge.rs::call_claude`
/// already relies on: a single top-level object,
/// `{"result": "...", "session_id": "...", "total_cost_usd": ...}`
/// (`--output-format json`'s real, previously-observed shape — not a
/// guessed one). Lenient by design: any failure (non-JSON stdout, no
/// `session_id` field, or a value [`ProviderSessionId::new`] itself
/// rejects) is `None`, not an error — a run whose structured output
/// happens not to carry a usable session id is still a fully valid run,
/// just not one a FUTURE command can continue from (see the strict
/// re-validation on read-back, `docs/q-deck/r1-command.md` §6).
fn parse_session_id(stdout: &str) -> Option<ProviderSessionId> {
    let value: serde_json::Value = serde_json::from_str(stdout).ok()?;
    let raw = value.get("session_id")?.as_str()?;
    ProviderSessionId::new(raw).ok()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn parses_session_id_from_the_real_claude_json_envelope() {
        let stdout = r#"{"result": "done", "session_id": "abc-123", "total_cost_usd": 0.02}"#;
        assert_eq!(
            parse_session_id(stdout),
            Some(ProviderSessionId::new("abc-123").unwrap())
        );
    }

    #[test]
    fn missing_session_id_field_is_none_not_an_error() {
        let stdout = r#"{"result": "done", "total_cost_usd": 0.02}"#;
        assert_eq!(parse_session_id(stdout), None);
    }

    #[test]
    fn non_json_stdout_is_none_not_a_panic() {
        assert_eq!(parse_session_id("not json at all"), None);
        assert_eq!(parse_session_id(""), None);
    }

    #[test]
    fn an_empty_session_id_value_is_rejected_as_none() {
        let stdout = r#"{"result": "done", "session_id": ""}"#;
        assert_eq!(parse_session_id(stdout), None);
    }

    #[test]
    fn a_session_id_containing_a_control_character_is_rejected() {
        let stdout = r#"{"result": "done", "session_id": "abc\ndef"}"#;
        assert_eq!(parse_session_id(stdout), None);
    }

    #[test]
    fn a_session_id_that_is_not_a_string_is_none() {
        let stdout = r#"{"result": "done", "session_id": 12345}"#;
        assert_eq!(parse_session_id(stdout), None);
    }

    #[test]
    fn provider_session_id_new_validates_the_same_rules_directly() {
        assert!(ProviderSessionId::new("").is_err());
        assert!(ProviderSessionId::new("has\tcontrol").is_err());
        assert!(ProviderSessionId::new("sess-abc-123").is_ok());
    }
}
