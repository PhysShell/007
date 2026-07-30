//! 007 (`o7`) — private harness. MVP = one isolated, gated agent run.
//!
//! loop: worktree at <base> -> agent full-auto -> gate manifest -> canonical
//!       events -> reducer verdict -> harvest run record into the private store.

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use o7::agent::{self, Engine};
use o7::events;
use o7::gate::GateManifest;
use o7::invoke;
use o7::judge;
use o7::ledger_projector::{ConversationSelector, LiveLedgerProjector};
use o7::record::{RunMeta, RunRecord};
use o7::verdict::{StepVerdict, Verdict};
use o7::worktree;
use o7_run::event::ArtifactKind;
use o7_run::ids::RunId as CanonicalRunId;

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
    /// Invoke: one narrow, read-only, schema-bound single-shot agent call.
    Invoke(invoke::InvokeArgs),
    /// Replay: independently re-verify a stored run record (chain, artifacts, verdict).
    Replay(ReplayArgs),
    /// Recover: mark ledger runs/attempts left `running` by a dead process
    /// as `Interrupted` (never `Error`) — a thin CLI over o7-ledger's
    /// existing `recover_scan`/`mark_interrupted` (Q-Deck R0.5), not a new
    /// mechanism.
    Recover(RecoverArgs),
}

#[derive(Args)]
struct ReplayArgs {
    /// Run record directory (`runs/<target>/<run-id>`).
    run_dir: PathBuf,
}

#[derive(Args)]
struct RecoverArgs {
    /// Ledger to scan and recover.
    #[arg(long)]
    ledger: PathBuf,
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
    /// Live-project this run's canonical events into this SQLite ledger as
    /// they happen (Q-Deck R0.7, `docs/q-deck/r07-live-ingress.md`).
    /// Omitted: today's flat-record-only behavior, byte/semantics-unchanged.
    /// Given: an unopenable ledger fails the run loudly before the worktree
    /// is even created, same discipline as an invalid gate manifest.
    #[arg(long)]
    ledger: Option<PathBuf>,
    /// Ledger conversation to project this run into (only meaningful with
    /// `--ledger`). Omitted: a new conversation is created. Given: must
    /// already exist — an unknown id fails loudly, never silently created
    /// under the caller-supplied id and never "the most recent one."
    #[arg(long)]
    conversation_id: Option<String>,
}

fn main() -> Result<()> {
    match Cli::parse().cmd {
        Cmd::Run(a) => run(a),
        Cmd::Judge(a) => judge::run(&a),
        Cmd::Invoke(a) => invoke::run(&a),
        Cmd::Replay(a) => replay(&a),
        Cmd::Recover(a) => recover(&a),
    }
}

/// Mark runs/attempts a dead process left `running` as `Interrupted`. Reuses
/// o7-ledger's existing recovery scan (Q-Deck R0.5) — no new mechanism.
fn recover(a: &RecoverArgs) -> Result<()> {
    let ledger = o7_ledger::SqliteLedger::open(&a.ledger)
        .with_context(|| format!("opening ledger at {}", a.ledger.display()))?;
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("starting the recovery runtime")?;
    let state = rt
        .block_on(ledger.recover_scan())
        .context("scanning for interrupted work")?;
    let runs = state.interrupted_runs.len();
    let attempts = state.interrupted_attempts.len();
    rt.block_on(ledger.mark_interrupted(state))
        .context("marking interrupted work")?;
    println!("[o7] recover: {runs} run(s), {attempts} attempt(s) marked interrupted");
    Ok(())
}

/// Independently re-verify a stored run record and report the anchors.
fn replay(a: &ReplayArgs) -> Result<()> {
    let report = events::replay_record(&a.run_dir)?;
    println!(
        "[o7] replay VERIFIED: verdict {:?}, {} events, {} artifacts",
        report.verdict, report.events_verified, report.artifacts_verified
    );
    println!(
        "[o7]   final event digest:      {}",
        report.final_event_digest.as_str()
    );
    println!(
        "[o7]   normalized state digest:  {}",
        report.normalized_state_digest.as_str()
    );
    Ok(())
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
    // Validate the manifest and fix the obligation contract BEFORE the
    // worktree, the agent, or any gate command spends anything — an invalid
    // manifest (blank/duplicate names, log collisions) must cost nothing and
    // must never abort a run after the fact without its canonical record.
    let contract = events::build_contract(&manifest)?;

    let secs = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    let run_id = format!("{secs}-{}", std::process::id());

    // A live ledger sink is opt-in and fails loudly BEFORE anything is
    // spent — the same discipline the contract build above already has
    // (Q-Deck R0.7, docs/q-deck/r07-live-ingress.md section 2.5). Built
    // before the worktree even exists.
    let projector = match &a.ledger {
        None => None,
        Some(path) => {
            let canonical_run_id = CanonicalRunId::new(run_id.clone())
                .map_err(|e| anyhow::anyhow!("minting run id for the ledger projector: {e}"))?;
            let conversation = match &a.conversation_id {
                Some(id) => ConversationSelector::Existing(id.clone()),
                None => ConversationSelector::New,
            };
            Some(
                LiveLedgerProjector::start(
                    path,
                    conversation,
                    &canonical_run_id,
                    a.engine.clone(),
                    "implementer".to_string(),
                )
                .with_context(|| {
                    format!("starting live ledger projection at {}", path.display())
                })?,
            )
        }
    };

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
        contract,
        projector.as_ref(),
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
    contract: o7_run::event::RunContract,
    projector: Option<&LiveLedgerProjector>,
) -> Result<Verdict> {
    println!(
        "[o7] {run_id}: {} ({}) full-auto in worktree",
        a.engine, a.model
    );

    let rec = RunRecord::create(&a.runs_dir, target, run_id)?;
    let task_ref = events::artifact(ArtifactKind::Task, "task.md", task.as_bytes());

    // No ledger: EXACTLY today's pre-R0.7 path, untouched — the agent and
    // every gate run to completion first, then the whole canonical stream
    // is synthesized in one call. Byte/semantics-identical to before this
    // slice (docs/q-deck/r07-live-ingress.md section 2.5's requirement).
    let (stream, steps, ar) = match projector {
        None => {
            let ar = agent::run(engine, wt, task, &a.model, a.max_turns)?;
            rec.write_task(task)?;
            rec.write_agent_stdout(&ar.stdout)?;
            let diff = worktree::diff_vs_base(wt, &a.base).unwrap_or_default();
            rec.write_diff(&diff)?;
            let steps = manifest.run(wt, &rec.gate_dir())?;
            let diff_ref = events::artifact(ArtifactKind::Diff, "diff.patch", diff.as_bytes());
            let stream = events::build_events(
                run_id, contract, &task_ref, &diff_ref, &ar, &steps, &rec.dir,
            )?;
            (stream, steps, ar)
        }
        Some(p) => execute_live(
            a, run_id, wt, engine, task, &task_ref, manifest, contract, &rec, p,
        )?,
    };

    rec.write_text(events::EVENTS_FILE, &events::to_jsonl(&stream)?)?;
    let verdict = events::canonical_verdict(&stream)?;

    if let Some(p) = projector {
        // The terminal ledger status comes only from the verdict the
        // canonical reducer above just produced — never recomputed by the
        // projector itself (docs/q-deck/r07-live-ingress.md section 2.1).
        // A sink failure here is reported, never allowed to change the
        // canonical verdict this function returns.
        if let Err(e) = events::to_canonical(verdict).and_then(|cv| p.seal(cv)) {
            eprintln!(
                "[o7] warning: canonical verdict is {verdict:?}, but sealing the live \
                 ledger projection failed: {e:#} — this run's durable projection is \
                 incomplete; run `o7 recover --ledger <path>` and re-apply the missing \
                 tail before trusting Q-Deck's view of it"
            );
        }
    }

    // The legacy per-step reduction stays as a cross-check surface: a
    // difference is expected exactly where the reducer is stricter (e.g. a
    // failed agent with green gates), and is surfaced, never hidden.
    let legacy = Verdict::reduce(&steps);
    if legacy != verdict {
        eprintln!(
            "[o7] note: canonical verdict {verdict:?} differs from legacy step reduction \
             {legacy:?} — the canonical reducer is the authority"
        );
    }

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

/// The `--ledger` path: mints each canonical event immediately after the
/// real thing it describes happens, and projects it to the ledger right
/// away — Q-Deck R0.7's live ingress
/// (`docs/q-deck/r07-live-ingress.md`). Produces the exact same
/// `(stream, steps, AgentRun)` shape [`execute`]'s no-ledger branch does
/// (proved equal for the same inputs by
/// `tests/live_ingress_matches_batch.rs`), just built incrementally with a
/// projection call after each event instead of in one call at the end.
///
/// A projection failure is reported but never aborts the run or changes
/// what gets returned — the canonical record is never held hostage to sink
/// health (docs/q-deck/r07-live-ingress.md section 2.5).
#[allow(clippy::too_many_arguments)]
fn execute_live(
    a: &RunArgs,
    run_id: &str,
    wt: &Path,
    engine: Engine,
    task: &str,
    task_ref: &o7_run::event::ArtifactRef,
    manifest: &GateManifest,
    contract: o7_run::event::RunContract,
    rec: &RunRecord,
    projector: &LiveLedgerProjector,
) -> Result<(
    Vec<o7_run::event::RunEvent>,
    Vec<StepVerdict>,
    agent::AgentRun,
)> {
    let canonical_run_id = CanonicalRunId::new(run_id.to_string())
        .map_err(|e| anyhow::anyhow!("minting run id: {e}"))?;
    let mut chain = events::EventChain::new(canonical_run_id);
    let project =
        |chain: &mut events::EventChain, kind: o7_run::event::RunEventKind| -> Result<()> {
            let event = chain.push(kind)?;
            if let Err(e) = projector.project(&event) {
                eprintln!(
                    "[o7] warning: live ledger projection of a canonical event failed: {e:#} — \
                 continuing the run; this event's durable projection is incomplete and \
                 needs `o7 recover --ledger <path>` afterward"
                );
            }
            Ok(())
        };

    project(
        &mut chain,
        o7_run::event::RunEventKind::RunStarted {
            contract,
            task: task_ref.clone(),
        },
    )?;

    project(&mut chain, o7_run::event::RunEventKind::AgentStarted)?;
    let ar = agent::run(engine, wt, task, &a.model, a.max_turns)?;
    project(
        &mut chain,
        o7_run::event::RunEventKind::AgentExited {
            outcome: events::agent_outcome(&ar),
        },
    )?;

    rec.write_task(task)?;
    rec.write_agent_stdout(&ar.stdout)?;
    let diff = worktree::diff_vs_base(wt, &a.base).unwrap_or_default();
    rec.write_diff(&diff)?;
    let diff_ref = events::artifact(ArtifactKind::Diff, "diff.patch", diff.as_bytes());
    project(
        &mut chain,
        o7_run::event::RunEventKind::PatchCaptured { patch: diff_ref },
    )?;

    manifest.validate()?;
    let gate_out = rec.gate_dir();
    std::fs::create_dir_all(&gate_out)?;
    let mut steps = Vec::new();
    for step in &manifest.gate {
        let verdict = manifest.run_one_step(step, wt, &gate_out)?;
        // Mirrors build_events' own skip condition exactly: a step that
        // never actually executed (windows-blocked or waived) emits no
        // gate events — the contract alone carries its obligation.
        if !matches!(verdict.verdict, Verdict::Blocked | Verdict::NotApplicable) {
            let gate = o7_run::ids::GateId::new(step.name.clone())
                .map_err(|e| anyhow::anyhow!("gate step name invalid as a gate id: {e}"))?;
            project(
                &mut chain,
                o7_run::event::RunEventKind::GateStarted { gate: gate.clone() },
            )?;
            let log = if verdict.log.is_empty() {
                None
            } else {
                let bytes = std::fs::read(rec.dir.join(&verdict.log))
                    .with_context(|| format!("reading back gate log {}", verdict.log))?;
                Some(events::artifact(
                    o7_run::event::ArtifactKind::GateLog,
                    &verdict.log,
                    &bytes,
                ))
            };
            project(
                &mut chain,
                o7_run::event::RunEventKind::GateFinished {
                    gate,
                    outcome: events::gate_outcome(verdict.verdict),
                    log,
                },
            )?;
        }
        steps.push(verdict);
    }

    project(&mut chain, o7_run::event::RunEventKind::RunSealed)?;

    Ok((chain.out, steps, ar))
}
