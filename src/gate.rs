use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::Path;
use std::process::Command;

use crate::verdict::{StepVerdict, Verdict};

/// `.007/gate.toml` — the per-target-repo gate manifest.
///
/// Unknown fields are tolerated on purpose (serde ignores them; no
/// `deny_unknown_fields`) so the manifest can grow without breaking an older
/// `o7`. `schema` is bumped only on a genuinely breaking change.
#[derive(Debug, Deserialize)]
pub struct GateManifest {
    #[allow(dead_code)]
    #[serde(default = "default_schema")]
    pub schema: u32,
    #[serde(default)]
    pub gate: Vec<GateStep>,
}

fn default_schema() -> u32 {
    1
}

#[derive(Debug, Clone, Deserialize)]
pub struct GateStep {
    pub name: String,
    pub cmd: String,
    #[serde(default = "default_true")]
    pub required: bool,
    /// `Some("windows")` → run on the Windows host (Phase 2, OwnAudit gates).
    /// `None`/other → run via bash in the worktree (the MVP path).
    #[serde(default)]
    pub env: Option<String>,
}

fn default_true() -> bool {
    true
}

impl GateManifest {
    pub fn load(path: &Path) -> Result<GateManifest> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("reading gate manifest {}", path.display()))?;
        Self::parse(&text).with_context(|| format!("parsing gate manifest {}", path.display()))
    }

    /// Parse a gate manifest from TOML text. Entry point for fuzzing the
    /// untrusted-config parser without touching the filesystem.
    pub fn parse(text: &str) -> Result<GateManifest> {
        toml::from_str(text).context("parsing gate manifest TOML")
    }

    /// Run every step in `workdir`, writing per-step logs into `gate_out`.
    /// Returns per-step verdicts (feed to `Verdict::reduce`).
    pub fn run(&self, workdir: &Path, gate_out: &Path) -> Result<Vec<StepVerdict>> {
        std::fs::create_dir_all(gate_out)?;
        let mut out = Vec::new();

        for step in &self.gate {
            // MVP exercises the unix/bash path only. `env == "windows"` (OwnAudit's
            // FlaUI/ClrMD/Roslyn gates on the host) is Phase 2 — skip loudly for now.
            if step.env.as_deref() == Some("windows") {
                eprintln!(
                    "[o7] gate '{}' tagged env=windows — skipped (Phase 2)",
                    step.name
                );
                out.push(StepVerdict {
                    name: step.name.clone(),
                    required: false,
                    verdict: Verdict::NotApplicable,
                    exit_code: None,
                    log: String::new(),
                });
                continue;
            }

            println!("[o7]   gate: {} :: {}", step.name, step.cmd);
            let result = Command::new("bash")
                .arg("-lc")
                .arg(&step.cmd)
                .current_dir(workdir)
                .output();

            let (verdict, exit_code, combined) = match result {
                Ok(o) => {
                    let mut buf = String::new();
                    buf.push_str(&String::from_utf8_lossy(&o.stdout));
                    buf.push_str(&String::from_utf8_lossy(&o.stderr));
                    let v = if o.status.success() {
                        Verdict::Pass
                    } else {
                        Verdict::Fail
                    };
                    (v, o.status.code(), buf)
                }
                Err(e) => (Verdict::Error, None, format!("failed to spawn bash: {e}")),
            };

            let log_name = format!("{}.log", sanitize(&step.name));
            std::fs::write(gate_out.join(&log_name), &combined)?;

            out.push(StepVerdict {
                name: step.name.clone(),
                required: step.required,
                verdict,
                exit_code,
                log: format!("gate/{log_name}"),
            });
        }

        Ok(out)
    }
}

fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect()
}
