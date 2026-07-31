//! 007 (`o7`) — private harness. MVP = one isolated, gated agent run.
//!
//! loop: worktree at <base> -> agent full-auto -> gate manifest -> canonical
//!       events -> reducer verdict -> harvest run record into the private store.

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use o7::agent::{self, Engine};
use o7::events;
use o7::gate::GateManifest;
use o7::invoke;
use o7::judge;
use o7::ledger_projector::{ConversationSelector, LiveLedgerProjector, PendingProjection};
use o7::record::{LedgerBinding, RunMeta, RunRecord};
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
    /// existing `recover_scan`/`mark_interrupted` (Q-Deck R0.5). With
    /// `--run-dir`, ALSO re-verifies and re-applies one run's canonical
    /// `events.jsonl` to the ledger — catch-up for a sink that fell behind
    /// or a process that crashed mid-projection (Q-Deck R0.7).
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
    /// Re-verify and re-apply this run's canonical `events.jsonl` to the
    /// ledger (idempotent — safe even if some or all of it was already
    /// projected). Optional; the still-running -> `Interrupted` scan
    /// always runs regardless of whether this is given.
    #[arg(long)]
    run_dir: Option<PathBuf>,
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
    /// Ledger conversation to project this run into. Requires `--ledger` —
    /// rejected at parse time otherwise, rather than silently ignored.
    /// Omitted: a new conversation is created. Given: must already exist —
    /// an unknown id fails loudly, never silently created under the
    /// caller-supplied id and never "the most recent one."
    #[arg(long, requires = "ledger")]
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

/// Mark runs/attempts a dead process left `running` as `Interrupted`
/// (reuses o7-ledger's existing recovery scan, Q-Deck R0.5), and, if
/// `--run-dir` is given, first catch up that run's live projection from its
/// canonical record.
fn recover(a: &RecoverArgs) -> Result<()> {
    if let Some(run_dir) = &a.run_dir {
        catch_up(run_dir, &a.ledger)?;
    }

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

/// Re-verify a run's canonical `events.jsonl` (chain continuity, digests,
/// sequence order — the same structural check `o7 replay` does, via the
/// same `reduce_all`) and re-apply it to the ledger: attach to (or create)
/// the run/attempt idempotently, re-project every event (idempotent, so an
/// already-applied prefix is a safe no-op), and idempotently apply the
/// sealed verdict if the stream is sealed. Reuses `LiveLedgerProjector` —
/// not a second reducer, not a separate import format.
fn catch_up(run_dir: &Path, ledger_path: &Path) -> Result<()> {
    let events_text = std::fs::read_to_string(run_dir.join(events::EVENTS_FILE))
        .with_context(|| format!("reading canonical events.jsonl in {}", run_dir.display()))?;
    let stream = events::from_jsonl(&events_text)?;
    let state = o7_run::reduce::reduce_all(&stream)
        .map_err(|e| anyhow::anyhow!("canonical stream failed structural verification: {e}"))?;
    let canonical_run_id = stream
        .first()
        .map(|e| e.run_id.clone())
        .context("events.jsonl has no events to catch up")?;

    // Look up whether this run's ledger row already exists BEFORE deciding
    // how to resolve the conversation. `create_run_with_id`'s idempotency
    // digest includes `conversation_id` — if the original run was created
    // under an explicit `--conversation-id` and this call guessed `New`
    // instead, the digest would mismatch and fail as a conflict before ever
    // reaching any "existing row wins" replay logic. So: an existing run's
    // OWN stored conversation_id is used verbatim.
    let existing_conversation = {
        let ledger = o7_ledger::SqliteLedger::open(ledger_path)
            .with_context(|| format!("opening ledger at {}", ledger_path.display()))?;
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .context("starting the catch-up lookup runtime")?;
        rt.block_on(ledger.run(o7_ledger::RunId::from_raw(
            canonical_run_id.as_str().to_owned(),
        )))
        .context("looking up this run's existing ledger row")?
        .map(|run| run.conversation_id.as_str().to_owned())
    };

    // If no run row exists yet (the process crashed between durable
    // RunStarted and `attach_run`), `ConversationSelector::New` must never
    // be GUESSED — that would silently discard an explicit
    // `--conversation-id` the original run was actually given, and create a
    // phantom second conversation instead. The durable `ledger_binding.json`
    // (written before RunStarted — see `record::LedgerBinding`) records the
    // conversation the live run actually resolved to, so catch-up reads
    // the real answer instead of guessing. It is absent only for a run that
    // predates this fix or crashed before ITS OWN durable write, in which
    // case there's genuinely nothing more to go on than the pre-existing
    // `New` fallback.
    let binding =
        LedgerBinding::read(run_dir).context("reading this run's ledger_binding.json, if any")?;
    let conversation = match existing_conversation {
        Some(id) => ConversationSelector::Existing(id),
        None => match &binding {
            Some(b) => ConversationSelector::Existing(b.conversation_id.clone()),
            None => ConversationSelector::New,
        },
    };
    // Likewise, `agent`/`role` are part of `create_run_with_id`'s own
    // idempotency digest — reusing the run's actual original values (from
    // the same binding record) rather than a hardcoded guess keeps catch-up
    // correct once a second agent engine exists, not just for `claude`.
    let (agent, role) = match &binding {
        Some(b) => (b.agent.clone(), b.role.clone()),
        None => ("claude".to_string(), "implementer".to_string()),
    };

    let pending = PendingProjection::open(ledger_path, conversation, &canonical_run_id)
        .context("opening the ledger for catch-up")?;
    let projector = pending
        .attach_run(&canonical_run_id, agent, role)
        .context("attaching the catch-up projector to the existing (or new) run")?;

    for event in &stream {
        projector
            .project(event)
            .with_context(|| format!("re-projecting canonical event seq {}", event.sequence))?;
    }

    let sealed = state.verdict.is_some();
    if let Some(verdict) = state.verdict {
        projector
            .seal(verdict)
            .context("applying the canonical stream's sealed verdict during catch-up")?;
    }

    println!(
        "[o7] recover: caught up run {} ({} canonical event(s) re-applied, {})",
        canonical_run_id.as_str(),
        stream.len(),
        if sealed { "sealed" } else { "still running" }
    );
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

    // Phase 1 of the live-ledger sink, opt-in, fails loudly BEFORE anything
    // is spent (Q-Deck R0.7, docs/q-deck/r07-live-ingress.md section 2.5):
    // open the ledger and resolve the conversation. This does NOT create
    // the ledger run yet — that must wait until canonical RunStarted is
    // durably on disk (section 3's timing fix), which happens inside
    // `execute`/`execute_live`, after the worktree exists.
    let pending = match &a.ledger {
        None => None,
        Some(path) => {
            let canonical_run_id = CanonicalRunId::new(run_id.clone())
                .map_err(|e| anyhow::anyhow!("minting run id for the ledger projector: {e}"))?;
            let conversation = match &a.conversation_id {
                Some(id) => ConversationSelector::Existing(id.clone()),
                None => ConversationSelector::New,
            };
            Some(
                PendingProjection::open(path, conversation, &canonical_run_id).with_context(
                    || format!("opening live ledger projection at {}", path.display()),
                )?,
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
        pending,
    );

    if a.keep_worktree {
        eprintln!("[o7] worktree kept at {}", wt.display());
    } else if let Err(e) = worktree::remove(&repo, &wt) {
        eprintln!("[o7] warning: worktree cleanup failed: {e}");
    }

    let (verdict, projection_incomplete) = outcome?;
    println!("[o7] {run_id}: verdict {verdict:?}");
    if projection_incomplete {
        eprintln!(
            "[o7] {run_id}: WARNING — live ledger projection is incomplete; run \
             `o7 recover --ledger <path> --run-dir runs/{target}/{run_id}` to catch up \
             before trusting Q-Deck's view of this run"
        );
    }
    // A PASS whose explicitly requested ledger projection is incomplete is
    // never reported as a successful process exit — the canonical verdict
    // in meta.json/replay is unaffected, but the exit code must not lie
    // about durable Q-Deck visibility having actually been achieved.
    if verdict != Verdict::Pass || projection_incomplete {
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
    pending: Option<PendingProjection>,
) -> Result<(Verdict, bool)> {
    println!(
        "[o7] {run_id}: {} ({}) full-auto in worktree",
        a.engine, a.model
    );

    let rec = RunRecord::create(&a.runs_dir, target, run_id)?;
    let task_ref = events::artifact(ArtifactKind::Task, "task.md", task.as_bytes());
    let was_live = pending.is_some();

    // No ledger: EXACTLY today's pre-R0.7 path, untouched — the agent and
    // every gate run to completion first, then the whole canonical stream
    // is synthesized in one call. Byte/semantics-identical to before this
    // slice (docs/q-deck/r07-live-ingress.md section 2.5's requirement).
    let (stream, steps, ar, projection_incomplete) = match pending {
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
            (stream, steps, ar, false)
        }
        Some(p) => execute_live(
            a, run_id, wt, engine, task, &task_ref, manifest, contract, &rec, p,
        )?,
    };

    if was_live {
        // The live path already durably appended every event, one at a
        // time, with `sync_data()` after each (section 1's ordering fix).
        // Truncating and rewriting the SAME bytes here — even though they
        // are byte-identical — would still open a real window where the
        // file is empty or partial between the truncate and the rewrite
        // landing, destroying the durability guarantee the per-event
        // append path exists to provide. So the live path only ever reads
        // back and verifies; it never rewrites its own already-durable
        // journal.
        let on_disk = std::fs::read_to_string(rec.dir.join(events::EVENTS_FILE))
            .context("reading back the durably-appended events.jsonl for verification")?;
        let expected = events::to_jsonl(&stream)?;
        anyhow::ensure!(
            on_disk == expected,
            "durably-appended events.jsonl does not match the in-memory canonical stream \
             — this should never happen"
        );
    } else {
        rec.write_text(events::EVENTS_FILE, &events::to_jsonl(&stream)?)?;
    }
    let verdict = events::canonical_verdict(&stream)?;

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
    Ok((verdict, projection_incomplete))
}

/// The `--ledger` path: for every canonical event, in order —
///
/// 1. mint it;
/// 2. serialize it and durably append it to `events.jsonl` (flushed before
///    anything else happens — a canonical event must never be projected to
///    the ledger before it is part of the on-disk canonical record, Q-Deck
///    R0.7, docs/q-deck/r07-live-ingress.md section "Recovery and
///    idempotency");
/// 3. only then project it to the ledger.
///
/// `RunStarted` is durably appended BEFORE the ledger run is even created
/// (`pending.attach_run` runs after step 2 for that first event, never
/// before) — Q-Deck must never show a run `running` before its canonical
/// record says it started. Each `GateStarted` is minted/appended/projected
/// immediately before that gate's command actually runs, and each
/// `GateFinished` only after it completes and its log is captured — not
/// after the fact, the way the pre-corrective-review version of this
/// function did.
///
/// A ledger *projection* failure (as opposed to a canonical-journal write
/// failure, which is fatal and propagates) is reported but never aborts the
/// run — the canonical record is never held hostage to sink health — but it
/// IS tracked and returned, so the caller can report the run's exit
/// honestly instead of claiming a complete projection that didn't happen.
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
    pending: PendingProjection,
) -> Result<(
    Vec<o7_run::event::RunEvent>,
    Vec<StepVerdict>,
    agent::AgentRun,
    bool,
)> {
    let canonical_run_id = CanonicalRunId::new(run_id.to_string())
        .map_err(|e| anyhow::anyhow!("minting run id: {e}"))?;

    // Durable, in this order, BEFORE canonical RunStarted is even minted:
    //
    // 1. The ledger-binding record — which conversation this run's live
    //    projection resolved to (`pending.conversation_id()`), so a crash
    //    before the ledger run row exists still lets `o7 recover` find the
    //    real conversation instead of guessing `New` and losing an explicit
    //    `--conversation-id` (second corrective round, defect 3).
    // 2. `task.md` itself — RunStarted's own canonical `task` ArtifactRef
    //    durably references it by digest; writing it only after the agent
    //    finishes (as the no-ledger path still does, and as this path used
    //    to) left a window where a crash mid-agent-run produced a durable
    //    RunStarted whose referenced artifact didn't exist on disk yet
    //    (second corrective round, defect 1).
    LedgerBinding {
        schema: 1,
        run_id: run_id.to_string(),
        conversation_id: pending.conversation_id().as_str().to_string(),
        agent: a.engine.clone(),
        role: "implementer".to_string(),
    }
    .write_durable(rec)
    .context("durably writing ledger_binding.json before RunStarted")?;
    rec.write_task_durable(task)
        .context("durably writing task.md before RunStarted")?;

    let mut chain = events::EventChain::new(canonical_run_id.clone());
    let mut events_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(rec.dir.join(events::EVENTS_FILE))
        .context("opening events.jsonl for durable per-event append")?;
    let mut projection_incomplete = false;

    // Mint + durably append ONLY — no projector exists yet for the very
    // first event (RunStarted), since the ledger run must not be created
    // before this write lands on disk.
    let mut append_only = |chain: &mut events::EventChain,
                           kind: o7_run::event::RunEventKind|
     -> Result<o7_run::event::RunEvent> {
        let event = chain.push(kind)?;
        let mut line = serde_json::to_string(&event).context("serializing canonical event")?;
        line.push('\n');
        events_file
            .write_all(line.as_bytes())
            .context("appending canonical event to events.jsonl")?;
        events_file
            .sync_data()
            .context("flushing canonical event to disk")?;
        Ok(event)
    };

    let run_started = append_only(
        &mut chain,
        o7_run::event::RunEventKind::RunStarted {
            contract,
            task: task_ref.clone(),
        },
    )?;

    // Only NOW — after RunStarted is durably on disk — does the ledger run
    // get created/attached and reported `running`.
    let projector = pending
        .attach_run(
            &canonical_run_id,
            a.engine.clone(),
            "implementer".to_string(),
        )
        .context("attaching the live ledger projector after durable RunStarted")?;

    let project = |projector: &LiveLedgerProjector,
                   event: &o7_run::event::RunEvent,
                   incomplete: &mut bool| {
        if let Err(e) = projector.project(event) {
            *incomplete = true;
            eprintln!(
                "[o7] warning: live ledger projection of a canonical event failed: {e:#} — \
                 continuing the run; this event's durable projection is incomplete and \
                 needs `o7 recover --ledger <path> --run-dir <run-dir>` afterward"
            );
        }
    };

    project(&projector, &run_started, &mut projection_incomplete);

    let agent_started = append_only(&mut chain, o7_run::event::RunEventKind::AgentStarted)?;
    project(&projector, &agent_started, &mut projection_incomplete);

    let ar = agent::run(engine, wt, task, &a.model, a.max_turns)?;

    let agent_exited = append_only(
        &mut chain,
        o7_run::event::RunEventKind::AgentExited {
            outcome: events::agent_outcome(&ar),
        },
    )?;
    project(&projector, &agent_exited, &mut projection_incomplete);

    rec.write_agent_stdout(&ar.stdout)?;
    let diff = worktree::diff_vs_base(wt, &a.base).unwrap_or_default();
    rec.write_diff(&diff)?;
    let diff_ref = events::artifact(ArtifactKind::Diff, "diff.patch", diff.as_bytes());
    let patch_captured = append_only(
        &mut chain,
        o7_run::event::RunEventKind::PatchCaptured { patch: diff_ref },
    )?;
    project(&projector, &patch_captured, &mut projection_incomplete);

    manifest.validate()?;
    let gate_out = rec.gate_dir();
    std::fs::create_dir_all(&gate_out)?;
    let mut steps = Vec::new();
    for step in &manifest.gate {
        // Mirrors GateManifest::run_one_step's own skip predicate exactly —
        // a step that will not actually execute (windows-blocked or
        // waived) emits no gate events, the contract alone carries its
        // obligation. Checked HERE, before calling run_one_step, so that
        // for a step that DOES execute, GateStarted is minted/appended/
        // projected before the command runs, not after.
        if step.env.as_deref() == Some("windows") {
            steps.push(manifest.run_one_step(step, wt, &gate_out)?);
            continue;
        }

        let gate = o7_run::ids::GateId::new(step.name.clone())
            .map_err(|e| anyhow::anyhow!("gate step name invalid as a gate id: {e}"))?;
        let gate_started = append_only(
            &mut chain,
            o7_run::event::RunEventKind::GateStarted { gate: gate.clone() },
        )?;
        project(&projector, &gate_started, &mut projection_incomplete);

        let verdict = manifest.run_one_step(step, wt, &gate_out)?;
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
        let gate_finished = append_only(
            &mut chain,
            o7_run::event::RunEventKind::GateFinished {
                gate,
                outcome: events::gate_outcome(verdict.verdict),
                log,
            },
        )?;
        project(&projector, &gate_finished, &mut projection_incomplete);
        steps.push(verdict);
    }

    let run_sealed = append_only(&mut chain, o7_run::event::RunEventKind::RunSealed)?;
    project(&projector, &run_sealed, &mut projection_incomplete);

    let verdict = events::canonical_verdict(&chain.out)?;
    if let Err(e) = events::to_canonical(verdict).and_then(|cv| projector.seal(cv)) {
        projection_incomplete = true;
        eprintln!(
            "[o7] warning: canonical verdict is {verdict:?}, but sealing the live ledger \
             projection failed: {e:#} — this run's durable projection is incomplete; run \
             `o7 recover --ledger <path> --run-dir <run-dir>` before trusting Q-Deck's \
             view of it"
        );
    }

    Ok((chain.out, steps, ar, projection_incomplete))
}
