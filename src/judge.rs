//! `o7 judge` — read-only FP-triage of analyzer findings.
//!
//! Per-file whole-file backend call (`claude -p` or `codex exec`), classify each
//! finding (real / false_positive / uncertain), assemble the `fp-verdicts.json`
//! overlay per the domain contract
//! (OwnAudit/docs/fp-judge/verdict-contract.md). Never edits, never gates.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use sha2::Sha256;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;

use crate::agent::Engine;
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
pub fn finding_id(path: &str, rule: &str, message: &str) -> String {
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

// The model occasionally drops an echo field (`rule` seen missing on real runs).
// A missing field must NOT abort parsing of the whole array. The echo fields are
// `Option` — NOT blanket-defaulted — so the pairing logic can tell "the model
// omitted this" (absent -> trust position) from "the model asserted a value"
// (present -> a real cross-check / reorder signal). A blanket `default` collapses
// both into ""/0 and feeds fabricated values into identity matching, which — since
// `Finding.line` also defaults to 0 — can key-match a real line-0 finding and
// misattribute the verdict. `class` is the one load-bearing field: kept a plain
// String and validated against the schema enum downstream (empty / unknown ->
// counted malformed, never written to the overlay).
#[derive(Deserialize)]
struct RawVerdict {
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    line: Option<i64>,
    #[serde(default)]
    rule: Option<String>,
    #[serde(default)]
    class: String,
    #[serde(default)]
    confidence: f64,
    #[serde(default)]
    reason: String,
    #[serde(default)]
    evidence: String,
}

/// Valid verdict classes, per `judge/fp-verdicts.schema.json` (the overlay
/// contract). Anything else — empty, wrong-case, hyphenated, prose — is malformed
/// and must not reach the overlay, or the domain merge fails schema validation on
/// a run `o7` already reported as successful.
fn normalized_class(class: &str) -> Option<&'static str> {
    match class {
        "real" => Some("real"),
        "false_positive" => Some("false_positive"),
        "uncertain" => Some("uncertain"),
        _ => None,
    }
}

/// Pair a file's raw verdicts to finding ids.
///
/// The prompt mandates "one object per finding above, in the same order", so when
/// counts match **position is the identity**: it is the only thing that can split a
/// message-collision (same `(path,line,rule)`, distinct `finding_id`) and — because
/// each finding appears once — it never assigns two verdicts to one id. The echoed
/// tuple is used only as a *cross-check*: a COMPLETE echo that lands at the wrong
/// position is surfaced as a warning (possible reorder), but we still trust
/// position. Silently repairing by key (the old behaviour) could map two verdicts
/// onto one id and drop another — the bug this replaces.
///
/// When counts DON'T match, position is meaningless, so fall back to key recovery,
/// dropping any verdict whose complete echo matches no finding.
///
/// Pure (no I/O): returns the pairing plus human-readable warnings, so it is unit-
/// and property-testable like the repo's other model-output parsers.
fn pair_verdicts<'a>(
    raws: &'a [RawVerdict],
    fif: &[&Rep],
    key_to_id: &BTreeMap<(String, i64, String), String>,
) -> (Vec<(String, &'a RawVerdict)>, Vec<String>) {
    let mut warnings = Vec::new();
    let echo = |rv: &RawVerdict| match (rv.path.as_deref(), rv.line, rv.rule.as_deref()) {
        (Some(p), Some(l), Some(r)) => Some((p.to_string(), l, r.to_string())),
        _ => None,
    };

    if raws.len() == fif.len() {
        let paired = raws
            .iter()
            .zip(fif.iter())
            .map(|(rv, rep)| {
                if let Some(key) = echo(rv) {
                    let at_position = key.0 == rep.path && key.1 == rep.line && key.2 == rep.rule;
                    if !at_position && key_to_id.contains_key(&key) {
                        warnings.push(format!(
                            "verdict echo ({}, {}, {}) matches a different finding than its \
                             position ({}) — trusting position per the ordered-output contract",
                            key.0, key.1, key.2, rep.id
                        ));
                    }
                }
                (rep.id.clone(), rv)
            })
            .collect();
        (paired, warnings)
    } else {
        warnings.push(format!(
            "model returned {} verdict(s) for {} finding(s) — pairing by (path, line, rule) key \
             instead of position",
            raws.len(),
            fif.len()
        ));
        let paired = raws
            .iter()
            .filter_map(|rv| match echo(rv) {
                Some(key) => match key_to_id.get(&key) {
                    Some(id) => Some((id.clone(), rv)),
                    None => {
                        warnings.push(format!(
                            "verdict for unknown finding ({}, {}, {}) — skipped",
                            key.0, key.1, key.2
                        ));
                        None
                    }
                },
                None => {
                    warnings
                        .push("verdict with incomplete echo and mismatched count — skipped".into());
                    None
                }
            })
            .collect();
        (paired, warnings)
    }
}

// ---------- judge run-record meta ----------

#[derive(Serialize)]
struct JudgeMeta {
    kind: &'static str,
    run_id: String,
    target: String,
    findings: String,
    generated_from: String,
    provider: &'static str,
    model: String,
    files_judged: usize,
    /// Files slated to run but skipped (unreadable source / malformed output /
    /// backend failure). `> 0` means the overlay is PARTIAL and the run exits
    /// non-zero — coverage automation must read this, not just the exit code.
    files_skipped: usize,
    /// Verdicts dropped for an invalid `class` (empty / not in the schema enum).
    findings_malformed: usize,
    findings_total: usize,
    ids_total: usize,
    by_class: BTreeMap<String, usize>,
    session_ids: Vec<String>,
    cost_usd: Option<f64>,
}

// ---------- backend agent provider ----------
//
// The judge dispatches to two subprocess CLIs, both subscription-auth, no API
// keys: `claude -p` (Claude Max) and `codex exec` (ChatGPT, via `codex login`).
// The backend is the shared `agent::Engine`; `--provider` selects it (or `auto`
// infers it from the model id via `model_family`).

/// Which vendor a model id belongs to. Single source of truth for BOTH
/// `--provider auto` routing and the `--provider codex` footgun guard, so the
/// two can never drift (they used to be two separate hardcoded lists). Prefix
/// matching — not exact — so versioned aliases (`opus-4.5`, `gpt-5.5`) classify
/// correctly. `opus`/`sonnet`/... are tested before the `o1`/`o3`/... OpenAI
/// prefixes, so `opus` resolves to Claude, not the `o`-series.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Family {
    Claude,
    OpenAI,
    /// Unrecognized id — auto-routes to Claude (the historical default) but is
    /// NOT blocked by the codex guard, so an explicit `--provider codex --model
    /// <new-openai-id>` still works before we've taught this list the new name.
    Unknown,
}

fn model_family(model: &str) -> Family {
    let m = model.to_ascii_lowercase();
    if m.starts_with("claude")
        || m.starts_with("opus")
        || m.starts_with("sonnet")
        || m.starts_with("haiku")
        || m.starts_with("fable")
        || m.starts_with("mythos")
    {
        Family::Claude
    } else if m.starts_with("gpt")
        || m.starts_with("o1")
        || m.starts_with("o3")
        || m.starts_with("o4")
        || m.starts_with("o5")
        || m.contains("codex")
    {
        Family::OpenAI
    } else {
        Family::Unknown
    }
}

/// Resolve `--provider` (`claude` | `codex` | `auto`) against the model id.
/// `auto` routes OpenAI-family ids to codex and everything else (incl. unknown
/// ids) to claude, using the shared `model_family` classifier.
fn resolve_provider(flag: &str, model: &str) -> Result<Engine> {
    match flag.to_ascii_lowercase().as_str() {
        "claude" => Ok(Engine::Claude),
        "codex" => Ok(Engine::Codex),
        "auto" => Ok(match model_family(model) {
            Family::OpenAI => Engine::Codex,
            Family::Claude | Family::Unknown => Engine::Claude,
        }),
        other => anyhow::bail!("unknown --provider '{other}' (want: claude | codex | auto)"),
    }
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
    /// Model. `opus`/`sonnet` -> claude; `gpt*`/`o*`/`*codex*` -> codex.
    #[arg(long, default_value = "opus")]
    pub model: String,
    /// Backend agent CLI: `claude` | `codex` | `auto` (infer from --model).
    #[arg(long, default_value = "auto")]
    pub provider: String,
    /// Parallel per-file workers. Bounded — respects backend rate limits; a burst
    /// of 100+ concurrent calls would throttle. `1` = fully sequential.
    #[arg(long, default_value_t = 4)]
    pub jobs: usize,
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
    /// Plan only — print files/ids/calls, do not call the backend.
    #[arg(long)]
    pub dry_run: bool,
}

/// Extra retry attempts per file on a transient backend failure (429 / flaky
/// spawn), with short linear backoff. Matters most under `--jobs` concurrency.
const JUDGE_RETRIES: u32 = 2;

/// One file's mergeable outcome, produced by a worker and folded into the run's
/// maps single-threaded. Empty on a skip (unreadable source, backend failure after
/// retries, unparseable output) — `session_id`/`cost` may still be set if the call
/// succeeded but parsing didn't.
#[derive(Default)]
struct FileResult {
    verdicts: Vec<(String, VerdictOut)>,
    /// Valid classes (real / false_positive / uncertain) of the recorded verdicts,
    /// folded into the run's `by_class` summary. Kept separate from `verdicts` only
    /// so the merge stays a plain count.
    classes: Vec<String>,
    session_id: Option<String>,
    cost: Option<f64>,
    /// This file contributed nothing to the overlay (unreadable source, backend
    /// failure after retries, unparseable output). Counted in the merge even when
    /// `verdicts` is empty, so a worker failure can't let a partial run pass as clean.
    file_skipped: bool,
    /// Verdicts dropped on this file for an invalid `class` (empty / not in the
    /// schema enum). Summed into the run's `findings_malformed`.
    findings_malformed: usize,
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
    let provider = resolve_provider(&a.provider, &a.model)?;
    // The default model is `opus` (claude), so `--provider codex` with no `--model`
    // would ship a Claude-family name to codex and 400. Catch that footgun early with
    // a hint instead of a raw upstream error. `model_family` is the SAME classifier
    // that drives auto-routing, so the guard can't drift from it — and it prefix-
    // matches, so a versioned alias like `opus-4.5` is still caught.
    if provider == Engine::Codex && model_family(&a.model) == Family::Claude {
        anyhow::bail!(
            "--provider codex needs an OpenAI model (e.g. --model gpt-5.5), got '{}'",
            a.model
        );
    }

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
            // `i` was just stored in `idx` for this id → always < reps.len().
            #[allow(clippy::indexing_slicing)]
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
            "[o7 judge] dry-run: {} {} call(s) would run",
            files.len(),
            provider.label()
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
    // Coverage bookkeeping: a file skipped for ANY reason (unreadable source,
    // malformed output, backend failure) leaves its findings unjudged. Count both so
    // the overlay/meta can't silently pass as complete.
    let mut files_skipped = 0usize;
    let mut findings_malformed = 0usize;

    // Judge files with a bounded worker pool: calls are independent per file and the
    // overlay is a finding_id -> verdict MAP assembled after the fact, so files
    // finishing out of order changes nothing (docs/performance.md). Bounded, not
    // unbounded — a burst of 100+ concurrent claude/codex calls would trip the
    // subscription rate limits. Model calls run on the workers; the merge below stays
    // single-threaded, so all pairing/dedup logic is unchanged and deterministic.
    let total = files.len();
    // Bound workers by the file count too: `--jobs 1000` on a 3-file run shouldn't
    // spawn 1000 idle scoped threads (keeps the "bounded" contract local, Codex).
    let jobs = a.jobs.max(1).min(total.max(1));
    println!("[o7 judge] judging {total} file(s), {jobs} worker(s)");

    let next = AtomicUsize::new(0);
    let (tx, rx) = mpsc::channel::<FileResult>();
    std::thread::scope(|scope| {
        // Shadow shared state as shared refs so each `move` worker copies the ref.
        let files = &files;
        let repo = &repo;
        let template = &template;
        let rubric = &rubric;
        let reps = &reps;
        let by_file = &by_file;
        let key_to_id = &key_to_id;
        let lines_by_id = &lines_by_id;
        let rec = &rec;
        let next = &next;
        let model = a.model.as_str();
        for _ in 0..jobs {
            let tx = tx.clone();
            scope.spawn(move || loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                let Some(file) = files.get(i) else { break };
                let res = judge_one_file(
                    i + 1,
                    total,
                    file,
                    provider,
                    repo,
                    model,
                    template,
                    rubric,
                    reps,
                    by_file,
                    key_to_id,
                    lines_by_id,
                    rec,
                    JUDGE_RETRIES,
                );
                // The collector outlives every worker (the scope joins before the
                // drain below), so a send failure means rx was dropped — impossible
                // here; bail out of the loop rather than panic if it ever happens.
                if tx.send(res).is_err() {
                    break;
                }
            });
        }
    });
    // Close the original sender so the collector's `for res in rx` terminates once
    // every worker (each held a clone) has finished and dropped its own sender.
    drop(tx);
    // Single-threaded merge — deterministic, and the only writer of these maps. The
    // scope above already joined all workers, so this just drains the buffered channel.
    for res in rx {
        // A worker that produced nothing (unreadable source, backend failure,
        // unparseable output) still counts as a skipped file even with empty
        // `verdicts` — otherwise a partial overlay silently reads as complete.
        if res.file_skipped {
            files_skipped += 1;
        }
        findings_malformed += res.findings_malformed;
        if let Some(s) = res.session_id {
            session_ids.push(s);
        }
        if let Some(c) = res.cost {
            cost_total += c;
            cost_any = true;
        }
        for class in res.classes {
            *by_class.entry(class).or_default() += 1;
        }
        for (id, v) in res.verdicts {
            verdicts.insert(id, v);
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
        provider: provider.label(),
        model: a.model.clone(),
        // Honest coverage: judged = slated minus skipped, not the planned count.
        files_judged: files.len() - files_skipped,
        files_skipped,
        findings_malformed,
        findings_total,
        ids_total: reps.len(),
        by_class: by_class.clone(),
        session_ids,
        cost_usd: if cost_any { Some(cost_total) } else { None },
    };
    rec.write_json("meta.json", &meta)?;

    println!("[o7 judge] {run_id}: {by_class:?}");
    if findings_malformed > 0 {
        println!("[o7 judge] {findings_malformed} malformed verdict(s) dropped (invalid class)");
    }
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

    // The overlay + meta are on disk (they record the partial result and the skip
    // count); now surface the partial coverage as a non-zero exit so a wrapper /
    // dashboard can't read a silently-incomplete triage as a clean, complete run.
    if files_skipped > 0 {
        anyhow::bail!(
            "{files_skipped} of {} file(s) skipped — overlay is PARTIAL; \
             see the warnings above and raw.*.txt in {}",
            files.len(),
            rec.dir.display()
        );
    }
    Ok(())
}

/// Judge a single file: build the prompt, call the backend (with retry), parse and
/// pair the verdicts. Fully error-isolated — every failure warns and returns an
/// empty/partial `FileResult` rather than propagating, so one bad file never aborts
/// the batch. The whole per-file pipeline runs on a worker thread; only reads shared
/// (read-only) state, so it is `Send`/`Sync`-safe by construction.
#[allow(clippy::too_many_arguments)]
fn judge_one_file(
    seq: usize,
    total: usize,
    file: &str,
    provider: Engine,
    repo: &Path,
    model: &str,
    template: &str,
    rubric: &str,
    reps: &[Rep],
    by_file: &BTreeMap<String, Vec<usize>>,
    key_to_id: &BTreeMap<(String, i64, String), String>,
    lines_by_id: &BTreeMap<String, Vec<i64>>,
    rec: &RunRecord,
    retries: u32,
) -> FileResult {
    let mut out = FileResult::default();

    // `by_file` only ever stores valid indices into `reps`.
    #[allow(clippy::indexing_slicing)]
    let fif: Vec<&Rep> = by_file
        .get(file)
        .map(|ids| ids.iter().map(|&i| &reps[i]).collect())
        .unwrap_or_default();
    let fif_json = match serde_json::to_string_pretty(&fif) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[o7 judge] warn: {file}: serializing findings failed ({e}) — skipped");
            out.file_skipped = true;
            return out;
        }
    };

    // Confine reads to the repo root: an absolute or `../` path smuggled into
    // findings.json must not pull arbitrary files into the prompt — that is another
    // exfil channel the tool sandbox can't stop. `repo` is already canonicalized.
    let src_path = match repo.join(file).canonicalize() {
        Ok(p) if p.starts_with(repo) => p,
        Ok(p) => {
            eprintln!(
                "[o7 judge] warn: {file} resolves outside --repo ({}) — skipped",
                p.display()
            );
            out.file_skipped = true;
            return out;
        }
        Err(e) => {
            eprintln!("[o7 judge] warn: {file}: cannot resolve source ({e}) — skipped");
            out.file_skipped = true;
            return out;
        }
    };
    let src = match std::fs::read_to_string(&src_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[o7 judge] warn: {file}: reading source failed ({e}) — skipped");
            out.file_skipped = true;
            return out;
        }
    };

    let prompt = template
        .replace("{{RUBRIC}}", rubric)
        .replace("{{FILE_PATH}}", file)
        .replace("{{FINDINGS_IN_FILE}}", &fif_json)
        .replace("{{FILE_CONTENT}}", &src);

    println!(
        "[o7 judge] ({seq}/{total}) {file} — {} finding(s)",
        fif.len()
    );

    let (result_text, sid, cost) = match call_agent_retry(provider, repo, &prompt, model, retries) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("[o7 judge] warn: {file}: backend call failed ({e}) — skipped");
            out.file_skipped = true;
            return out;
        }
    };
    // The call happened (cost incurred, session exists) — carry these even if the
    // parse below fails, so the run-record accounts for every call made.
    out.session_id = sid;
    out.cost = cost;

    if let Err(e) = rec.write_text(&format!("raw.{}.txt", sanitize(file)), &result_text) {
        eprintln!("[o7 judge] warn: {file}: could not persist raw output ({e})");
    }

    // A malformed output for ONE file must not abort the batch. The raw text is
    // already persisted above, so warn, skip this file, keep going.
    let arr = match extract_json_array(&result_text) {
        Some(a) => a,
        None => {
            eprintln!(
                "[o7 judge] warn: {file}: no JSON array in {} output — skipped (raw saved)",
                provider.label()
            );
            out.file_skipped = true;
            return out;
        }
    };
    let raws: Vec<RawVerdict> = match serde_json::from_str(&arr) {
        Ok(r) => r,
        Err(e) => {
            eprintln!(
                "[o7 judge] warn: {file}: parsing verdicts failed ({e}) — skipped (raw saved)"
            );
            out.file_skipped = true;
            return out;
        }
    };

    // Pair verdicts to finding ids via the shared, option-aware helper (position when
    // counts match, key-recovery otherwise) instead of reimplementing it here — the
    // inline version compared `RawVerdict`'s `Option` echo fields as if non-optional
    // and duplicated the warnings (CodeRabbit).
    let (paired, warnings) = pair_verdicts(&raws, &fif, key_to_id);
    for w in warnings {
        eprintln!("[o7 judge] warn: {file}: {w}");
    }

    for (id, rv) in paired {
        // Validate `class` against the schema enum — not just non-empty. A wrong-case
        // / hyphenated / prose class is malformed: count it (a first-class counter,
        // not a magic `by_class` key that could collide with a model-emitted class)
        // and keep it out of the overlay, so the domain merge never sees a value the
        // overlay schema forbids.
        let Some(class) = normalized_class(&rv.class) else {
            out.findings_malformed += 1;
            eprintln!(
                "[o7 judge] warn: {file}: invalid class {:?} for {id} — not recorded",
                rv.class
            );
            continue;
        };
        out.classes.push(class.to_string());
        out.verdicts.push((
            id.clone(),
            VerdictOut {
                class: class.to_string(),
                confidence: rv.confidence,
                reason: rv.reason.clone(),
                evidence: rv.evidence.clone(),
                lines: lines_by_id.get(&id).cloned().unwrap_or_default(),
            },
        ));
    }
    out
}

/// `call_agent` with light bounded retry. Under `--jobs` concurrency a transient
/// backend hiccup (rate-limit 429, flaky spawn) shouldn't drop the file on the first
/// miss. Bounded, not infinite: a hard error (bad model, auth) burns the retries and
/// returns the last error, which the caller turns into a skip.
fn call_agent_retry(
    provider: Engine,
    cwd: &Path,
    prompt: &str,
    model: &str,
    retries: u32,
) -> Result<(String, Option<String>, Option<f64>)> {
    let mut attempt = 0u32;
    loop {
        match call_agent(provider, cwd, prompt, model) {
            Ok(v) => return Ok(v),
            Err(e) if attempt < retries => {
                attempt += 1;
                eprintln!("[o7 judge] retry {attempt}/{retries} after backend error: {e}");
                std::thread::sleep(std::time::Duration::from_millis(500 * u64::from(attempt)));
            }
            Err(e) => return Err(e),
        }
    }
}

/// Dispatch one read-only judge call to the selected backend. Both return the
/// same `(result_text, session_id, cost_usd)` shape; codex has no single-envelope
/// session/cost on a subscription, so those come back `None`.
fn call_agent(
    provider: Engine,
    cwd: &Path,
    prompt: &str,
    model: &str,
) -> Result<(String, Option<String>, Option<f64>)> {
    match provider {
        Engine::Claude => call_claude(cwd, prompt, model),
        Engine::Codex => call_codex(cwd, prompt, model),
    }
}

/// Read-only `codex exec` call — the ChatGPT-subscription backend (`codex login`),
/// mirror of `call_claude`. Flags pinned against codex-cli 0.142.5:
/// - `--sandbox read-only`: codex's native no-write mode. The whole source file is
///   already in the prompt, so the model needs no tools; even if a prompt-injection
///   payload in the judged file coaxed a shell command, the sandbox denies the
///   write. (Unlike the claude path we don't also hard-disable network here — codex
///   has no one-flag equivalent — but read-only + nothing-to-do keeps the blast
///   radius to reads.)
/// - `-` reads the prompt from stdin, not argv: a large embedded source file would
///   blow the OS arg-size limit and argv is world-readable in `ps` (source leak).
/// - `--output-last-message <FILE>`: codex writes ONLY the agent's final message
///   there. We read that and ONLY that — stdout is discarded (`Stdio::null`),
///   because it also carries codex's session preamble whose stray `[` could fool
///   `extract_json_array`. If the file comes back empty we fail loud rather than
///   fall back to stdout (see below).
/// - `--skip-git-repo-check` so a non-git scan root never hard-fails; `--ephemeral`
///   so a 150-call batch doesn't litter session history; `--color never` for clean
///   logs.
///
/// codex on a subscription emits no per-call dollar-cost / session envelope, so
/// those come back `None`.
fn call_codex(
    cwd: &Path,
    prompt: &str,
    model: &str,
) -> Result<(String, Option<String>, Option<f64>)> {
    use std::io::Write as _;
    // Unique temp path for codex's final-message output (`-o`). Runtime clock +
    // pid is plenty unique for a serial judge loop; we delete it right after.
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let last_msg =
        std::env::temp_dir().join(format!("o7-codex-{}-{nanos}.txt", std::process::id()));

    let mut child = Command::new("codex")
        .current_dir(cwd)
        .arg("exec")
        .arg("--model")
        .arg(model)
        .arg("--sandbox")
        .arg("read-only")
        .arg("--skip-git-repo-check")
        .arg("--ephemeral")
        .arg("--color")
        .arg("never")
        .arg("--output-last-message")
        .arg(&last_msg)
        .arg("-") // read the prompt from stdin
        .stdin(Stdio::piped())
        // We read the answer from `--output-last-message`, never from stdout, so
        // discard stdout instead of buffering codex's whole session transcript in
        // memory per call — and it removes one pipe that could fill and deadlock the
        // stdin write below.
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .context("spawning `codex` (installed? `codex login` done? on PATH?)")?;
    // Write the prompt, but do NOT `?`-return on failure yet: if codex died early
    // (bad flag, not logged in) the write hits EPIPE, and returning here would both
    // leave the child unreaped AND discard its stderr (the real reason). Drop stdin
    // to send EOF, always `wait_with_output` (which reaps), then report the write
    // error together with codex's stderr.
    let mut stdin = child.stdin.take().context("codex stdin unavailable")?;
    let write_res = stdin.write_all(prompt.as_bytes());
    drop(stdin);
    let out = child.wait_with_output().context("waiting for `codex`")?;
    if let Err(e) = write_res {
        let _ = std::fs::remove_file(&last_msg);
        anyhow::bail!(
            "writing prompt to codex stdin failed ({e}); codex stderr: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    if !out.status.success() {
        let _ = std::fs::remove_file(&last_msg);
        anyhow::bail!("codex failed: {}", String::from_utf8_lossy(&out.stderr));
    }
    let text = std::fs::read_to_string(&last_msg).unwrap_or_default();
    let _ = std::fs::remove_file(&last_msg);
    // Do NOT fall back to scraping stdout when the final-message file is empty:
    // codex's stdout carries the session preamble/event log, and a stray `[` in it
    // fools `extract_json_array` into slicing a bogus "array" out of log text —
    // silently-wrong verdicts, the exact hazard `--output-last-message` exists to
    // avoid. An empty final message means no usable answer, so fail loud; the caller
    // persists raw output and (with batch resilience) skips just this file.
    if text.trim().is_empty() {
        anyhow::bail!(
            "codex produced no final message (--output-last-message empty); \
             refusing to scrape stdout as verdicts"
        );
    }
    Ok((text, None, None))
}

/// Read-only claude call. The whole file is already in the prompt, so no tool is
/// needed: `--tools ""` disables every built-in tool (closed-by-default, so no
/// current or future tool can run) and `--strict-mcp-config` refuses any ambient
/// MCP server — a prompt-injection payload in the judged file gets no read /
/// network / exfil path. `--permission-mode default` is passed explicitly so the run
/// never inherits an ambient `bypassPermissions` default (which refuses to run as
/// root); with no tools there is nothing to prompt for, so `default` never blocks a
/// headless run. Returns (result text, session_id, cost).
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
        // Pin an explicit non-bypass mode so we never inherit an ambient
        // `permissions.defaultMode = bypassPermissions` (which refuses to run as root).
        .arg("--permission-mode")
        .arg("default")
        // Read-only by construction: no built-in tools, no ambient MCP servers.
        // With no tools there is nothing to prompt for, so `default` never hangs.
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
pub fn extract_json_array(s: &str) -> Option<String> {
    let start = s.find('[')?;
    let end = s.rfind(']')?;
    (end > start).then(|| s[start..=end].to_string())
}

/// Parse an own-check `findings.json` from raw bytes and return the finding
/// count. A stable entry point for fuzzing the untrusted-input deserializer
/// without exposing the internal `FindingsFile` shape.
pub fn parse_findings_json(bytes: &[u8]) -> Result<usize> {
    let ff: FindingsFile = serde_json::from_slice(bytes)?;
    Ok(ff.findings.len())
}

pub fn sanitize(s: &str) -> String {
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

/// Kani proofs — bounded, symbolic "never panics / holds for all inputs" checks
/// on the string helpers (slice-boundary safety is exactly Kani's sweet spot).
/// Compiled only under `cargo kani`; invisible to normal builds.
#[cfg(kani)]
mod kani_proofs {
    use super::{extract_json_array, sanitize};

    /// `extract_json_array` must never panic — the `s[start..=end]` slice has to
    /// land on char boundaries for *any* input (bounded here for tractability).
    #[kani::proof]
    #[kani::unwind(6)]
    fn extract_json_array_never_panics() {
        let bytes: [u8; 4] = kani::any();
        let len: usize = kani::any();
        kani::assume(len <= bytes.len());
        if let Ok(s) = core::str::from_utf8(&bytes[..len]) {
            if let Some(arr) = extract_json_array(s) {
                assert!(arr.as_bytes().first() == Some(&b'['));
                assert!(arr.as_bytes().last() == Some(&b']'));
            }
        }
    }

    /// `sanitize` must never panic and must only emit path-safe bytes.
    #[kani::proof]
    #[kani::unwind(6)]
    fn sanitize_is_panic_free_and_path_safe() {
        let bytes: [u8; 4] = kani::any();
        let len: usize = kani::any();
        kani::assume(len <= bytes.len());
        if let Ok(s) = core::str::from_utf8(&bytes[..len]) {
            for c in sanitize(s).chars() {
                assert!(c.is_ascii_alphanumeric() || c == '-' || c == '_');
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finding_id_is_stable_and_16_hex() {
        let a = finding_id(
            "ViewModels/MixedViewModel.cs",
            "OWN001",
            "event 'QuoteReceived' ...",
        );
        let b = finding_id(
            "ViewModels/MixedViewModel.cs",
            "OWN001",
            "event 'QuoteReceived' ...",
        );
        assert_eq!(a, b, "same inputs -> same id");
        assert_eq!(a.len(), 16, "id is 16 hex chars");
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn finding_id_splits_on_message_same_tuple() {
        // The collision case: same (path, rule) — and same line at the call site —
        // but different message must yield DISTINCT ids, or one overlay entry is lost.
        let quote = finding_id(
            "ViewModels/MixedViewModel.cs",
            "OWN001",
            "'QuoteReceived' subscribed",
        );
        let down = finding_id(
            "ViewModels/MixedViewModel.cs",
            "OWN001",
            "'Disconnected' subscribed",
        );
        assert_ne!(quote, down, "different message -> different finding_id");
    }

    #[test]
    fn model_family_classifies_versioned_aliases() {
        // The guard-bypass fix: prefix match, not exact — so a dated alias still
        // classifies as its family (exact `== "opus"` let `opus-4.5` slip to codex).
        assert_eq!(model_family("opus"), Family::Claude);
        assert_eq!(model_family("opus-4.5"), Family::Claude);
        assert_eq!(model_family("claude-sonnet-5"), Family::Claude);
        assert_eq!(model_family("Sonnet"), Family::Claude); // case-insensitive
        assert_eq!(model_family("gpt-5.5"), Family::OpenAI);
        assert_eq!(model_family("o3-mini"), Family::OpenAI);
        assert_eq!(model_family("gpt-5-codex"), Family::OpenAI);
        // `opus` is tested before the `o`-series, so it never falls to OpenAI.
        assert_eq!(model_family("mystery-model"), Family::Unknown);
    }

    #[test]
    fn resolve_provider_and_guard() {
        // auto routing follows the family; unknown ids default to claude.
        assert_eq!(
            resolve_provider("auto", "opus-4.5").ok(),
            Some(Engine::Claude)
        );
        assert_eq!(
            resolve_provider("auto", "gpt-5.5").ok(),
            Some(Engine::Codex)
        );
        assert_eq!(
            resolve_provider("auto", "mystery").ok(),
            Some(Engine::Claude)
        );
        // explicit flags win regardless of model.
        assert_eq!(resolve_provider("codex", "opus").ok(), Some(Engine::Codex));
        assert_eq!(
            resolve_provider("claude", "gpt-5.5").ok(),
            Some(Engine::Claude)
        );
        assert!(resolve_provider("nonsense", "opus").is_err());
        // The footgun guard fires for a versioned Claude alias under codex — the
        // exact case the old `== "opus"` check let through.
        let is_guarded = |m: &str| {
            resolve_provider("codex", m).ok() == Some(Engine::Codex)
                && model_family(m) == Family::Claude
        };
        assert!(is_guarded("opus-4.5"));
        assert!(is_guarded("haiku-3.5"));
        assert!(!is_guarded("gpt-5.5"));
        assert!(!is_guarded("some-new-openai-id")); // unknown → not blocked
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

    // ---- property tests: the pure functions must hold on arbitrary input,
    // including untrusted bytes (finding messages, the model's raw output). ----

    use proptest::prelude::*;

    proptest! {
        /// `finding_id` never panics and always yields 16 lowercase hex chars,
        /// for any path/rule/message (incl. newlines, control chars, unicode).
        #[test]
        fn prop_finding_id_shape(p in "(?s).*", r in "(?s).*", m in "(?s).*") {
            let id = finding_id(&p, &r, &m);
            prop_assert_eq!(id.len(), 16);
            prop_assert!(id.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()));
        }

        /// Same inputs → same id (the domain contract both sides rely on).
        #[test]
        fn prop_finding_id_deterministic(p in "(?s).*", r in "(?s).*", m in "(?s).*") {
            prop_assert_eq!(finding_id(&p, &r, &m), finding_id(&p, &r, &m));
        }

        /// The dedup-critical property: on the same (path, rule), a different
        /// `message` must produce a different `finding_id` — else the overlay
        /// silently drops a verdict (the bug the collision-fix guards).
        #[test]
        fn prop_finding_id_splits_on_message(
            p in "(?s).*", r in "(?s).*", m1 in "(?s).*", m2 in "(?s).*"
        ) {
            prop_assume!(m1 != m2);
            prop_assert_ne!(finding_id(&p, &r, &m1), finding_id(&p, &r, &m2));
        }

        /// `extract_json_array` never panics on arbitrary model output, and when
        /// it returns Some the slice is bracket-delimited.
        #[test]
        fn prop_extract_json_array_safe(s in "(?s).*") {
            if let Some(arr) = extract_json_array(&s) {
                prop_assert!(arr.starts_with('['));
                prop_assert!(arr.ends_with(']'));
            }
        }

        /// `sanitize` output is path-safe (used to build filenames from findings
        /// paths) and is a 1:1 char map — never panics, never changes length.
        #[test]
        fn prop_sanitize_is_path_safe(s in "(?s).*") {
            let out = sanitize(&s);
            prop_assert!(out.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'));
            prop_assert_eq!(out.chars().count(), s.chars().count());
        }

        /// The core pairing invariant that the misattribution/overwrite fixes rest
        /// on: when verdict count == finding count, every verdict is placed by
        /// POSITION — so the paired ids equal the findings' ids in order, EXACTLY,
        /// no matter what the model echoed (complete, dropped, or reordered). This
        /// rules out both the dropped-verdict and duplicate-id-overwrite bugs.
        #[test]
        fn prop_pair_counts_match_is_positional_bijection(mask in any::<u32>(), take in 1usize..=7) {
            let lines = [10i64, 20, 30, 40, 50, 60, 70];
            let reps: Vec<Rep> = lines
                .iter()
                .take(take)
                .enumerate()
                .map(|(i, &l)| test_rep(&format!("id{i}"), "a.cs", l, "OWN001"))
                .collect();
            let fif: Vec<&Rep> = reps.iter().collect();
            let k = test_ktoi(&reps);
            // Each verdict either echoes its finding faithfully or drops the whole
            // echo (Option::None) — both must still land on the positional id.
            let raws: Vec<RawVerdict> = reps
                .iter()
                .enumerate()
                .map(|(i, r)| {
                    if (mask >> i) & 1 == 1 {
                        test_raw(None, None, None, "real")
                    } else {
                        test_raw(Some(&r.path), Some(r.line), Some(&r.rule), "real")
                    }
                })
                .collect();
            let (paired, _warn) = pair_verdicts(&raws, &fif, &k);
            let got: Vec<&str> = paired.iter().map(|(id, _)| id.as_str()).collect();
            let want: Vec<&str> = reps.iter().map(|r| r.id.as_str()).collect();
            prop_assert_eq!(got, want);
        }
    }

    // ---- pairing + class validation (the batch-resilience core) ----

    fn test_rep(id: &str, path: &str, line: i64, rule: &str) -> Rep {
        Rep {
            id: id.into(),
            path: path.into(),
            line,
            rule: rule.into(),
            category_name: String::new(),
            message: String::new(),
            lines: vec![line],
        }
    }

    fn test_raw(
        path: Option<&str>,
        line: Option<i64>,
        rule: Option<&str>,
        class: &str,
    ) -> RawVerdict {
        RawVerdict {
            path: path.map(Into::into),
            line,
            rule: rule.map(Into::into),
            class: class.into(),
            confidence: 0.0,
            reason: String::new(),
            evidence: String::new(),
        }
    }

    fn test_ktoi(reps: &[Rep]) -> BTreeMap<(String, i64, String), String> {
        reps.iter()
            .map(|r| ((r.path.clone(), r.line, r.rule.clone()), r.id.clone()))
            .collect()
    }

    #[test]
    fn pair_dropped_echo_keeps_position() {
        // The real observed failure: the model drops `rule` on one verdict. Counts
        // match, so both must still land on their positional ids — no misattribution.
        let reps = vec![
            test_rep("idA", "a.cs", 1, "OWN001"),
            test_rep("idB", "a.cs", 2, "OWN001"),
        ];
        let fif: Vec<&Rep> = reps.iter().collect();
        let k = test_ktoi(&reps);
        let raws = vec![
            test_raw(Some("a.cs"), Some(1), Some("OWN001"), "real"),
            test_raw(Some("a.cs"), Some(2), None, "false_positive"), // dropped rule
        ];
        let (paired, _w) = pair_verdicts(&raws, &fif, &k);
        let ids: Vec<&str> = paired.iter().map(|(id, _)| id.as_str()).collect();
        assert_eq!(ids, vec!["idA", "idB"]);
    }

    #[test]
    fn pair_reorder_never_overwrites_or_drops() {
        // The duplicate-id-overwrite bug: model reorders echoes (pos0 echoes B,
        // pos1 echoes A). Position is trusted, so each id appears exactly once and
        // none is dropped; the reorder is surfaced as a warning, not silently
        // "repaired" by key (which mapped two verdicts to one id).
        let reps = vec![
            test_rep("idA", "a.cs", 1, "OWN001"),
            test_rep("idB", "a.cs", 2, "OWN001"),
        ];
        let fif: Vec<&Rep> = reps.iter().collect();
        let k = test_ktoi(&reps);
        let raws = vec![
            test_raw(Some("a.cs"), Some(2), Some("OWN001"), "real"), // echoes B
            test_raw(Some("a.cs"), Some(1), Some("OWN001"), "false_positive"), // echoes A
        ];
        let (paired, warnings) = pair_verdicts(&raws, &fif, &k);
        let ids: Vec<&str> = paired.iter().map(|(id, _)| id.as_str()).collect();
        assert_eq!(
            ids,
            vec!["idA", "idB"],
            "position trusted, no overwrite/drop"
        );
        assert!(!warnings.is_empty(), "reorder is surfaced, not silent");
    }

    #[test]
    fn pair_count_mismatch_recovers_by_key() {
        let reps = vec![
            test_rep("idA", "a.cs", 1, "OWN001"),
            test_rep("idB", "a.cs", 2, "OWN001"),
        ];
        let fif: Vec<&Rep> = reps.iter().collect();
        let k = test_ktoi(&reps);
        // One verdict for two findings -> key path; echoes B.
        let raws = vec![test_raw(Some("a.cs"), Some(2), Some("OWN001"), "real")];
        let (paired, _w) = pair_verdicts(&raws, &fif, &k);
        assert_eq!(paired.len(), 1);
        assert_eq!(paired.first().map(|(id, _)| id.as_str()), Some("idB"));
    }

    #[test]
    fn pair_count_mismatch_skips_unknown_and_incomplete() {
        let reps = vec![test_rep("idA", "a.cs", 1, "OWN001")];
        let fif: Vec<&Rep> = reps.iter().collect();
        let k = test_ktoi(&reps);
        // Two verdicts for one finding -> key path. One matches no finding, one has
        // an incomplete echo — both dropped, none misattributed.
        let raws = vec![
            test_raw(Some("a.cs"), Some(9), Some("OWN999"), "real"), // unknown
            test_raw(None, None, None, "real"),                      // incomplete
        ];
        let (paired, _w) = pair_verdicts(&raws, &fif, &k);
        assert!(paired.is_empty());
    }

    #[test]
    fn normalized_class_enforces_schema_enum() {
        assert_eq!(normalized_class("real"), Some("real"));
        assert_eq!(normalized_class("false_positive"), Some("false_positive"));
        assert_eq!(normalized_class("uncertain"), Some("uncertain"));
        // Everything else is malformed — never written to the overlay.
        assert_eq!(normalized_class(""), None);
        assert_eq!(normalized_class("Real"), None);
        assert_eq!(normalized_class("false-positive"), None);
        assert_eq!(normalized_class("fp"), None);
        assert_eq!(normalized_class("_malformed"), None);
    }
}
