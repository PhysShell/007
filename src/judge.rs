//! `o7 judge` — read-only FP-triage of analyzer findings.
//!
//! Per-file whole-file `claude -p`, classify each finding (real / false_positive /
//! uncertain), assemble the `fp-verdicts.json` overlay per the domain contract
//! (OwnAudit/docs/fp-judge/verdict-contract.md). Never edits, never gates.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use sha2::Sha256;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::record::RunRecord;

// ---------- findings.json (own-check shape) ----------

#[derive(Deserialize)]
struct FindingsFile {
    #[serde(default)]
    findings: Vec<Finding>,
}

#[derive(Deserialize)]
struct Finding {
    #[serde(default)]
    tool: String,
    path: String,
    #[serde(default)]
    line: i64,
    rule: String,
    #[serde(default)]
    category_name: String,
    message: String,
}

// ---------- deduped representative per finding_id ----------

#[derive(Clone, Serialize)]
struct Rep {
    #[serde(skip)]
    id: String,
    path: String,
    line: i64,
    rule: String,
    category_name: String,
    message: String,
    #[serde(skip)]
    lines: Vec<i64>,
}

/// Line-independent identity — matches the domain contract exactly.
fn finding_id(path: &str, rule: &str, message: &str) -> String {
    let mut h = Sha1::new();
    h.update(path.as_bytes());
    h.update([0x1f]);
    h.update(rule.as_bytes());
    h.update([0x1f]);
    h.update(message.as_bytes());
    h.finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>()[..16]
        .to_string()
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

// ---------- overlay (the fp-verdicts.json contract) ----------

#[derive(Serialize)]
struct Overlay {
    schema: u32,
    tool: String,
    generated_from: String,
    model: String,
    run_id: String,
    verdicts: BTreeMap<String, VerdictOut>,
}

#[derive(Serialize)]
struct VerdictOut {
    class: String,
    confidence: f64,
    reason: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    evidence: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    lines: Vec<i64>,
}

// ---------- raw judge output (per prompt.template.md) ----------

#[derive(Deserialize)]
struct RawVerdict {
    path: String,
    line: i64,
    rule: String,
    class: String,
    #[serde(default)]
    confidence: f64,
    #[serde(default)]
    reason: String,
    #[serde(default)]
    evidence: String,
}

// ---------- judge run-record meta ----------

#[derive(Serialize)]
struct JudgeMeta {
    kind: &'static str,
    run_id: String,
    target: String,
    findings: String,
    generated_from: String,
    model: String,
    files_judged: usize,
    findings_total: usize,
    ids_total: usize,
    by_class: BTreeMap<String, usize>,
    session_ids: Vec<String>,
    cost_usd: Option<f64>,
}

#[derive(clap::Args)]
pub struct JudgeArgs {
    /// Source root for whole-file context (the scanned repo root).
    #[arg(long)]
    pub repo: PathBuf,
    /// findings.json to triage.
    #[arg(long)]
    pub findings: PathBuf,
    /// Rubric markdown (domain-owned).
    #[arg(long)]
    pub rubric: PathBuf,
    /// Prompt template.
    #[arg(long, default_value = "judge/prompt.template.md")]
    pub template: PathBuf,
    /// Model.
    #[arg(long, default_value = "opus")]
    pub model: String,
    /// Overlay output path (fp-verdicts.json). Also written into the run-record.
    #[arg(long)]
    pub out: Option<PathBuf>,
    /// Run-store label (default: repo folder name).
    #[arg(long)]
    pub target: Option<String>,
    /// Private run store root.
    #[arg(long, default_value = "runs")]
    pub runs_dir: PathBuf,
    /// Judge only findings whose path == this (single file).
    #[arg(long)]
    pub only: Option<String>,
    /// Cap files judged (cost control; 0 = all).
    #[arg(long, default_value_t = 0)]
    pub max_files: usize,
    /// Plan only — print files/ids/calls, do not call claude.
    #[arg(long)]
    pub dry_run: bool,
}

pub fn run(a: &JudgeArgs) -> Result<()> {
    let repo = a
        .repo
        .canonicalize()
        .with_context(|| format!("repo not found: {}", a.repo.display()))?;
    let target = a.target.clone().unwrap_or_else(|| {
        repo.file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "target".into())
    });

    let findings_bytes =
        std::fs::read(&a.findings).with_context(|| format!("reading {}", a.findings.display()))?;
    let generated_from = sha256_hex(&findings_bytes);
    let ff: FindingsFile = serde_json::from_slice(&findings_bytes)
        .with_context(|| format!("parsing {}", a.findings.display()))?;
    let rubric = std::fs::read_to_string(&a.rubric)
        .with_context(|| format!("reading {}", a.rubric.display()))?;
    let template = std::fs::read_to_string(&a.template)
        .with_context(|| format!("reading {}", a.template.display()))?;

    // Dedupe -> reps keyed by finding_id (first-seen order preserved).
    let mut reps: Vec<Rep> = Vec::new();
    let mut idx: BTreeMap<String, usize> = BTreeMap::new();
    let mut findings_total = 0usize;
    for f in &ff.findings {
        if let Some(only) = &a.only {
            if &f.path != only {
                continue;
            }
        }
        findings_total += 1;
        let id = finding_id(&f.path, &f.rule, &f.message);
        if let Some(&i) = idx.get(&id) {
            reps[i].lines.push(f.line);
        } else {
            idx.insert(id.clone(), reps.len());
            reps.push(Rep {
                id,
                path: f.path.clone(),
                line: f.line,
                rule: f.rule.clone(),
                category_name: f.category_name.clone(),
                message: f.message.clone(),
                lines: vec![f.line],
            });
        }
    }
    if reps.is_empty() {
        anyhow::bail!("no findings to judge (check --findings / --only)");
    }
    let tool = ff
        .findings
        .first()
        .map(|f| f.tool.clone())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "own-check".into());

    // Group reps by path (first-seen order).
    let mut files: Vec<String> = Vec::new();
    let mut by_file: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (i, r) in reps.iter().enumerate() {
        if !by_file.contains_key(&r.path) {
            files.push(r.path.clone());
        }
        by_file.entry(r.path.clone()).or_default().push(i);
    }
    if a.max_files > 0 && files.len() > a.max_files {
        eprintln!(
            "[o7 judge] capping {} files -> {} (--max-files)",
            files.len(),
            a.max_files
        );
        files.truncate(a.max_files);
    }
    let lines_by_id: BTreeMap<String, Vec<i64>> = reps
        .iter()
        .map(|r| (r.id.clone(), r.lines.clone()))
        .collect();
    let key_to_id: BTreeMap<(String, i64, String), String> = reps
        .iter()
        .map(|r| ((r.path.clone(), r.line, r.rule.clone()), r.id.clone()))
        .collect();

    println!(
        "[o7 judge] {findings_total} findings -> {} unique ids across {} files",
        reps.len(),
        files.len()
    );
    if a.dry_run {
        for f in &files {
            let n = by_file.get(f).map(|v| v.len()).unwrap_or(0);
            println!("  {n:>3}  {f}");
        }
        println!(
            "[o7 judge] dry-run: {} claude call(s) would run",
            files.len()
        );
        return Ok(());
    }

    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs();
    let run_id = format!("judge-{secs}-{}", std::process::id());
    let rec = RunRecord::create(&a.runs_dir, &target, &run_id)?;

    let mut verdicts: BTreeMap<String, VerdictOut> = BTreeMap::new();
    let mut by_class: BTreeMap<String, usize> = BTreeMap::new();
    let mut session_ids: Vec<String> = Vec::new();
    let mut cost_total = 0f64;
    let mut cost_any = false;

    for (fi, file) in files.iter().enumerate() {
        let fif: Vec<&Rep> = by_file
            .get(file)
            .map(|ids| ids.iter().map(|&i| &reps[i]).collect())
            .unwrap_or_default();
        let fif_json = serde_json::to_string_pretty(&fif)?;

        // Confine reads to the repo root: an absolute or `../` path smuggled into
        // findings.json must not pull arbitrary files into the prompt — that is another
        // exfil channel the tool sandbox can't stop. `repo` is already canonicalized.
        let src_path = match repo.join(file).canonicalize() {
            Ok(p) if p.starts_with(&repo) => p,
            Ok(p) => {
                eprintln!(
                    "[o7 judge] warn: {file} resolves outside --repo ({}) — skipped",
                    p.display()
                );
                continue;
            }
            Err(e) => {
                eprintln!("[o7 judge] warn: {file}: cannot resolve source ({e}) — skipped");
                continue;
            }
        };
        let src = std::fs::read_to_string(&src_path)
            .with_context(|| format!("reading source {}", src_path.display()))?;

        let prompt = template
            .replace("{{RUBRIC}}", &rubric)
            .replace("{{FILE_PATH}}", file)
            .replace("{{FINDINGS_IN_FILE}}", &fif_json)
            .replace("{{FILE_CONTENT}}", &src);

        println!(
            "[o7 judge] ({}/{}) {file} — {} finding(s)",
            fi + 1,
            files.len(),
            fif.len()
        );

        let (result_text, sid, cost) = call_claude(&repo, &prompt, &a.model)?;
        if let Some(s) = sid {
            session_ids.push(s);
        }
        if let Some(c) = cost {
            cost_total += c;
            cost_any = true;
        }

        rec.write_text(&format!("raw.{}.txt", sanitize(file)), &result_text)?;

        let arr = extract_json_array(&result_text)
            .with_context(|| format!("no JSON array in claude output for {file}"))?;
        let raws: Vec<RawVerdict> =
            serde_json::from_str(&arr).with_context(|| format!("parsing verdicts for {file}"))?;

        // The prompt mandates "one object per finding above, in the same order",
        // so pair verdicts to findings positionally when the counts match. That is
        // the only reliable identity: two findings sharing (path, line, rule) but
        // differing in `message` have distinct `finding_id`s, yet `RawVerdict` never
        // echoes `message` back — so a (path, line, rule) tuple lookup is lossy by
        // construction and silently drops one. Fall back to the tuple map only when
        // the model breaks the count contract.
        let paired: Vec<(String, &RawVerdict)> = if raws.len() == fif.len() {
            raws.iter()
                .zip(fif.iter())
                .filter_map(|(rv, rep)| {
                    // Position is trustworthy only when the echoed tuple matches: that
                    // both confirms the model kept order and is the only way to split a
                    // message-collision (identical tuple, distinct finding_id). On a
                    // mismatch the model reordered/merged — trusting position here would
                    // silently attach the verdict to the WRONG finding_id, so recover by
                    // key instead, and skip if even that is unknown.
                    if rv.path == rep.path && rv.line == rep.line && rv.rule == rep.rule {
                        return Some((rep.id.clone(), rv));
                    }
                    let key = (rv.path.clone(), rv.line, rv.rule.clone());
                    match key_to_id.get(&key) {
                        Some(id) => {
                            eprintln!(
                                "[o7 judge] warn: {file}: verdict ({}, {}, {}) out of position \
                                 — recovered by key, not position",
                                rv.path, rv.line, rv.rule
                            );
                            Some((id.clone(), rv))
                        }
                        None => {
                            eprintln!(
                                "[o7 judge] warn: {file}: verdict for unknown finding {key:?} \
                                 — skipped"
                            );
                            None
                        }
                    }
                })
                .collect()
        } else {
            eprintln!(
                "[o7 judge] warn: {file}: model returned {} verdict(s) for {} finding(s) \
                 — pairing by (path, line, rule) key instead of position",
                raws.len(),
                fif.len()
            );
            raws.iter()
                .filter_map(|rv| {
                    let key = (rv.path.clone(), rv.line, rv.rule.clone());
                    match key_to_id.get(&key) {
                        Some(id) => Some((id.clone(), rv)),
                        None => {
                            eprintln!(
                                "[o7 judge] warn: verdict for unknown finding {key:?} — skipped"
                            );
                            None
                        }
                    }
                })
                .collect()
        };

        for (id, rv) in paired {
            *by_class.entry(rv.class.clone()).or_default() += 1;
            verdicts.insert(
                id.clone(),
                VerdictOut {
                    class: rv.class.clone(),
                    confidence: rv.confidence,
                    reason: rv.reason.clone(),
                    evidence: rv.evidence.clone(),
                    lines: lines_by_id.get(&id).cloned().unwrap_or_default(),
                },
            );
        }
    }

    let overlay = Overlay {
        schema: 1,
        tool,
        generated_from: generated_from.clone(),
        model: a.model.clone(),
        run_id: run_id.clone(),
        verdicts,
    };
    rec.write_json("fp-verdicts.json", &overlay)?;
    if let Some(out) = &a.out {
        if let Some(parent) = out.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        std::fs::write(out, serde_json::to_string_pretty(&overlay)?)?;
    }

    let meta = JudgeMeta {
        kind: "judge",
        run_id: run_id.clone(),
        target,
        findings: a.findings.to_string_lossy().to_string(),
        generated_from,
        model: a.model.clone(),
        files_judged: files.len(),
        findings_total,
        ids_total: reps.len(),
        by_class: by_class.clone(),
        session_ids,
        cost_usd: if cost_any { Some(cost_total) } else { None },
    };
    rec.write_json("meta.json", &meta)?;

    println!("[o7 judge] {run_id}: {by_class:?}");
    if cost_any {
        println!("[o7 judge] cost ~${cost_total:.4}");
    }
    let overlay_at = a
        .out
        .as_ref()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| rec.dir.join("fp-verdicts.json").display().to_string());
    println!("[o7 judge] overlay -> {overlay_at}");
    println!("[o7 judge] record -> {}", rec.dir.display());
    Ok(())
}

/// Read-only claude call. The whole file is already in the prompt, so no tool is
/// needed: `--tools ""` disables every built-in tool (closed-by-default, so no
/// current or future tool can run) and `--strict-mcp-config` refuses any ambient
/// MCP server — a prompt-injection payload in the judged file gets no read /
/// network / exfil path. With no tools available there is no permission prompt to
/// block a headless run, so no `--permission-mode` override is needed — and passing
/// `bypassPermissions` would refuse to run as root. Returns (result text, session_id, cost).
fn call_claude(
    cwd: &Path,
    prompt: &str,
    model: &str,
) -> Result<(String, Option<String>, Option<f64>)> {
    use std::io::Write as _;
    // Feed the prompt (whole source file included) via stdin, not argv: a large file
    // would blow the OS argument-size limit before claude starts, and argv is readable
    // in local process listings (`ps`), leaking proprietary source. `claude -p` with no
    // prompt argument reads it from stdin.
    let mut child = Command::new("claude")
        .current_dir(cwd)
        .arg("-p")
        .arg("--model")
        .arg(model)
        // Read-only by construction: no built-in tools, no ambient MCP servers.
        // The file is already in the prompt, so classification needs no tool call —
        // and with no tools there is no permission prompt to hang on, so no
        // permission-mode override is needed (bypassPermissions also refuses root).
        .arg("--tools")
        .arg("")
        .arg("--strict-mcp-config")
        .arg("--output-format")
        .arg("json")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("spawning `claude` (installed? logged in? on PATH?)")?;
    child
        .stdin
        .take()
        .context("claude stdin unavailable")?
        .write_all(prompt.as_bytes())
        .context("writing prompt to claude stdin")?;
    let out = child.wait_with_output().context("waiting for `claude`")?;
    if !out.status.success() {
        anyhow::bail!("claude failed: {}", String::from_utf8_lossy(&out.stderr));
    }
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    // `--output-format json` => envelope { result, session_id, total_cost_usd, ... }
    match serde_json::from_str::<serde_json::Value>(&stdout) {
        Ok(v) => {
            let text = v
                .get("result")
                .and_then(|r| r.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| stdout.clone());
            let sid = v
                .get("session_id")
                .and_then(|s| s.as_str())
                .map(|s| s.to_string());
            let cost = v.get("total_cost_usd").and_then(|c| c.as_f64());
            Ok((text, sid, cost))
        }
        Err(_) => Ok((stdout, None, None)),
    }
}

/// Slice the first `[ .. ]` out of the model's text (tolerates ```json fences / stray prose).
fn extract_json_array(s: &str) -> Option<String> {
    let start = s.find('[')?;
    let end = s.rfind(']')?;
    (end > start).then(|| s[start..=end].to_string())
}

fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finding_id_is_stable_and_16_hex() {
        let a = finding_id("ViewModels/MixedViewModel.cs", "OWN001", "event 'QuoteReceived' ...");
        let b = finding_id("ViewModels/MixedViewModel.cs", "OWN001", "event 'QuoteReceived' ...");
        assert_eq!(a, b, "same inputs -> same id");
        assert_eq!(a.len(), 16, "id is 16 hex chars");
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn finding_id_splits_on_message_same_tuple() {
        // The collision case: same (path, rule) — and same line at the call site —
        // but different message must yield DISTINCT ids, or one overlay entry is lost.
        let quote = finding_id("ViewModels/MixedViewModel.cs", "OWN001", "'QuoteReceived' subscribed");
        let down = finding_id("ViewModels/MixedViewModel.cs", "OWN001", "'Disconnected' subscribed");
        assert_ne!(quote, down, "different message -> different finding_id");
    }

    #[test]
    fn finding_id_depends_on_path_and_rule() {
        let base = finding_id("a.cs", "OWN001", "m");
        assert_ne!(base, finding_id("b.cs", "OWN001", "m"), "path matters");
        assert_ne!(base, finding_id("a.cs", "OWN-TIMER", "m"), "rule matters");
    }

    #[test]
    fn extract_json_array_tolerates_fences_and_prose() {
        let s = "sure, here:\n```json\n[{\"class\":\"real\"}]\n```\nhope that helps";
        assert_eq!(
            extract_json_array(s).as_deref(),
            Some("[{\"class\":\"real\"}]")
        );
        assert_eq!(extract_json_array("no array here").as_deref(), None);
    }

    #[test]
    fn sanitize_keeps_only_path_safe_chars() {
        assert_eq!(sanitize("ViewModels/Mixed.cs"), "ViewModels_Mixed_cs");
        assert_eq!(sanitize("a b\\c"), "a_b_c");
    }
}
