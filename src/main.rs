//! 007 (`o7`) — private harness. MVP = one isolated, gated agent run.
//!
//! loop: worktree at <base> -> agent full-auto -> gate manifest -> verdict
//!       -> harvest run record into the private store.

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use o7::agent::{self, Engine};
use o7::gate::GateManifest;
use o7::judge;
use o7::record::{RunMeta, RunRecord};
use o7::verdict::Verdict;
use o7::worktree;

#[derive(Parser)]
#[command(name = "o7", version, about = "007 — one isolated, gated agent run")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Run one isolated, gated agent run against a target repo.
    Run(RunArgs),
    /// Judge: read-only FP-triage of analyzer findings -> fp-verdicts.json overlay.
    Judge(judge::JudgeArgs),
}

#[derive(Args)]
struct RunArgs {
    /// Target repo path.
    #[arg(long)]
    repo: PathBuf,
    /// Label for the run store (default: repo folder name).
    /// (name->path resolution via a targets.toml is a later nicety.)
    #[arg(long)]
    target: Option<String>,
    /// Base git ref for the worktree.
    #[arg(long, default_value = "HEAD")]
    base: String,
    /// Task file handed to the agent.
    #[arg(long)]
    task: PathBuf,
    /// Gate manifest (default: <repo>/.007/gate.toml).
    #[arg(long)]
    gate: Option<PathBuf>,
    /// Agent engine: claude (wired) | codex (Phase 2).
    #[arg(long, default_value = "claude")]
    engine: String,
    /// Model alias or id.
    #[arg(long, default_value = "opus")]
    model: String,
    /// Max agent turns.
    #[arg(long, default_value_t = 12)]
    max_turns: u32,
    /// Private run store root.
    #[arg(long, default_value = "runs")]
    runs_dir: PathBuf,
    /// Worktree root.
    #[arg(long, default_value = ".worktrees")]
    worktree_root: PathBuf,
    /// Keep the worktree after the run (default: remove it).
    #[arg(long)]
    keep_worktree: bool,
}

fn main() -> Result<()> {
    match Cli::parse().cmd {
        Cmd::Run(a) => run(a),
        Cmd::Judge(a) => judge::run(&a),
    }
}

fn run(a: RunArgs) -> Result<()> {
    let repo = a
        .repo
        .canonicalize()
        .with_context(|| format!("repo not found: {}", a.repo.display()))?;
    let target = a.target.clone().unwrap_or_else(|| {
        repo.file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "target".into())
    });
    let engine: Engine = a.engine.parse()?;
    let task = std::fs::read_to_string(&a.task)
        .with_context(|| format!("reading task file {}", a.task.display()))?;
    let gate_path = a
        .gate
        .clone()
        .unwrap_or_else(|| repo.join(".007").join("gate.toml"));
    let manifest = GateManifest::load(&gate_path)?;

    let secs = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    let run_id = format!("{secs}-{}", std::process::id());
    let base_commit = worktree::rev_parse(&repo, &a.base).unwrap_or_else(|_| a.base.clone());

    let wt = a.worktree_root.join(format!("{target}-{run_id}"));
    let branch = format!("o7/{run_id}");
    std::fs::create_dir_all(&a.worktree_root)?;
    worktree::add(&repo, &a.base, &wt, &branch)?;

    // Always tear the worktree down (unless asked to keep), even on error.
    let outcome = execute(
        &a,
        &repo,
        &target,
        &run_id,
        &wt,
        &base_commit,
        engine,
        &task,
        &manifest,
    );

    if a.keep_worktree {
        eprintln!("[o7] worktree kept at {}", wt.display());
    } else if let Err(e) = worktree::remove(&repo, &wt) {
        eprintln!("[o7] warning: worktree cleanup failed: {e}");
    }

    let verdict = outcome?;
    println!("[o7] {run_id}: verdict {verdict:?}");
    if verdict != Verdict::Pass {
        std::process::exit(1);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn execute(
    a: &RunArgs,
    repo: &Path,
    target: &str,
    run_id: &str,
    wt: &Path,
    base_commit: &str,
    engine: Engine,
    task: &str,
    manifest: &GateManifest,
) -> Result<Verdict> {
    println!(
        "[o7] {run_id}: {} ({}) full-auto in worktree",
        a.engine, a.model
    );
    let ar = agent::run(engine, wt, task, &a.model, a.max_turns)?;

    let rec = RunRecord::create(&a.runs_dir, target, run_id)?;
    rec.write_task(task)?;
    rec.write_agent_stdout(&ar.stdout)?;
    rec.write_diff(&worktree::diff_vs_base(wt, &a.base).unwrap_or_default())?;

    let steps = manifest.run(wt, &rec.gate_dir())?;
    let verdict = Verdict::reduce(&steps);

    let meta = RunMeta {
        schema: 1,
        kind: "run".to_string(),
        run_id: run_id.to_string(),
        target: target.to_string(),
        repo: repo.to_string_lossy().to_string(),
        base_commit: base_commit.to_string(),
        engine: a.engine.clone(),
        model: a.model.clone(),
        verdict,
        steps,
        agent_exit_code: ar.exit_code,
        session_id: None,
        cost_usd: None,
        started_at: None,
        finished_at: None,
    };
    rec.write_meta(&meta)?;
    println!("[o7] {run_id}: record at {}", rec.dir.display());
    Ok(verdict)
}
