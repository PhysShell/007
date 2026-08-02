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
use o7::record::{
    CommandBinding, LedgerBinding, ProviderSessionReceipt, RunMeta, RunRecord,
    CANDIDATE_PATCH_FILE, CANDIDATE_STATE_RECEIPT_FILE, PARENT_CANDIDATE_PATCH_FILE,
    PARENT_CANDIDATE_RECEIPT_FILE, PROVIDER_SESSION_RECEIPT_FILE,
};
use o7::verdict::{StepVerdict, Verdict};
use o7::worktree;
use o7_run::event::{ArtifactKind, CandidatePatchKind, CandidateStateContractV1};
use o7_run::ids::RunId as CanonicalRunId;
use o7_run::CandidateStateReceiptV1;

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
    /// Continue: Q-Deck R1's command-continuation executor
    /// (`docs/q-deck/r1-command.md` §9.5) — dispatch ONE durably accepted
    /// command as a brand new sealed child run, continuing the parent
    /// run's own provider session. Never invoked directly by an untrusted
    /// caller; `o7d`'s mutation endpoint is this subcommand's sole
    /// production caller (§9.4), always with a pre-minted, pre-bound
    /// `--run-id`/`--command-id` and its own fixed repo/worktree/runs-dir
    /// authority — never values taken from the HTTP request.
    Continue(ContinueArgs),
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
    /// Q-Deck R1 sixth corrective round: when given together with
    /// `--runs-dir`, additionally classify every "stuck" command's bound
    /// child run's own canonical record — read-only, exactly the same
    /// classifier `o7d`'s own redrive decision uses
    /// (`o7::recovery::classify_command_child`), never a second one — so
    /// an operator can see WHICH kind of "not automatically redriven" a
    /// command actually is: safe pre-dispatch, provider-outcome-ambiguous
    /// (with its last durable dispatch phase), sealed-but-not-yet-
    /// reconciled, or an invalid/tampered record. Never mutates anything
    /// and never re-invokes the provider — pure reporting.
    #[arg(long, requires = "runs_dir")]
    repo: Option<PathBuf>,
    /// Private run store root — required alongside `--repo` to resolve a
    /// stuck command's own record directory for classification.
    #[arg(long, requires = "repo")]
    runs_dir: Option<PathBuf>,
}

#[derive(Args)]
struct ContinueArgs {
    /// Target repo path — always `o7d`'s own fixed, server-configured
    /// value (`docs/q-deck/r1-command.md` §9.4); never taken from an HTTP
    /// request.
    #[arg(long)]
    repo: PathBuf,
    /// Label for the run store (default: repo folder name) — must match
    /// the value the conversation's earlier runs used, since the parent's
    /// flat record lives at `runs/<target>/<parent-run-id>/`.
    #[arg(long)]
    target: Option<String>,
    /// Base git ref for the child run's fresh worktree. This slice does
    /// NOT carry the parent run's own file changes forward into the
    /// child's worktree — only provider session continuity is proven
    /// here (see `docs/q-deck/r1-command.md`'s known-limitations note).
    #[arg(long, default_value = "HEAD")]
    base: String,
    /// Gate manifest (default: <repo>/.007/gate.toml).
    #[arg(long)]
    gate: Option<PathBuf>,
    /// Model alias or id — `o7d`'s own fixed value, never client-supplied.
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
    /// Ledger to project into — mandatory; a command's child run is always
    /// ledger-tracked, there is no legacy no-ledger continuation path.
    #[arg(long)]
    ledger: PathBuf,
    /// The conversation this command belongs to — must already exist.
    #[arg(long)]
    conversation_id: String,
    /// The run this command continues from — must exist, belong to
    /// `conversation_id`, and carry a durable provider session identity.
    #[arg(long)]
    parent_run_id: String,
    /// The exact command text — carried as inert text throughout; never
    /// interpreted by a shell (`agent::continue_session`'s own contract).
    #[arg(long)]
    command: String,
    /// The child run's canonical id, pre-minted by the caller (`o7d`)
    /// BEFORE spawning this process, so the durable command -> run binding
    /// (`docs/q-deck/r1-command.md` §3/§9.6) can be committed before the
    /// provider is ever invoked.
    #[arg(long)]
    run_id: String,
    /// The durably accepted command this run dispatches — its own
    /// bookkeeping status is updated to `completed`/`rejected` once this
    /// run's outcome is known.
    #[arg(long)]
    command_id: String,
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
        Cmd::Continue(a) => continue_run(&a),
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

    // Q-Deck R1 (`docs/q-deck/r1-command.md` §7): a command durably
    // accepted but never bound to a child run, or bound to one whose
    // `RunStarted` never reached the ledger, must stay DISCOVERABLE — never
    // silently dropped. Re-driving it is explicitly out of scope for this
    // slice; re-submitting the identical request under the SAME
    // idempotency key is the documented recovery path (safe per §4).
    let stuck = rt
        .block_on(ledger.stuck_commands())
        .context("scanning for stuck commands")?;
    // Q-Deck R1 sixth corrective round: when the operator gives enough
    // context to resolve a stuck command's own record directory, classify
    // it with the SAME read-only classifier `o7d`'s own redrive decision
    // uses — never a second, diverging one, and never anything that
    // mutates state or invokes the provider.
    let classify_stuck = |run_id: &str, command: &o7_ledger::Command| -> Option<String> {
        let (repo, runs_dir) = (a.repo.as_ref()?, a.runs_dir.as_ref()?);
        let target = repo
            .canonicalize()
            .ok()?
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "target".to_owned());
        let dir = runs_dir.join(target).join(run_id);
        Some(
            match o7::recovery::classify_command_child(&dir, run_id, command) {
                o7::recovery::ChildRecordState::Absent => "never actually started".to_owned(),
                o7::recovery::ChildRecordState::ValidUnsealedPreDispatch => {
                    "safe pre-dispatch redrive candidate (provider not yet invoked)".to_owned()
                }
                o7::recovery::ChildRecordState::ValidUnsealedDispatchAmbiguous { progress } => {
                    format!(
                        "PROVIDER-OUTCOME-AMBIGUOUS — past the durable dispatch boundary (last \
                         phase: {progress}), never sealed; automatic redrive is refused; manual \
                         resolution required"
                    )
                }
                o7::recovery::ChildRecordState::ValidSealed => {
                    "sealed — recoverable via a same-key retry or `o7 recover --run-dir`".to_owned()
                }
                o7::recovery::ChildRecordState::Invalid(reason) => {
                    format!("INVALID canonical record: {reason}")
                }
            },
        )
    };

    if stuck.is_empty() {
        println!("[o7] recover: 0 command(s) pending");
    } else {
        println!(
            "[o7] recover: {} command(s) pending — not re-driven automatically:",
            stuck.len()
        );
        for command in &stuck {
            match &command.child_run_id {
                None => println!(
                    "[o7]   command {} (conversation {}): accepted, never bound to a child run",
                    command.command_id, command.conversation_id
                ),
                Some(run_id) => {
                    let classification = classify_stuck(run_id.as_str(), command)
                        .unwrap_or_else(|| {
                            "that run's RunStarted never reached the ledger (pass --repo/--runs-dir \
                             to also classify its canonical record)"
                                .to_owned()
                        });
                    println!(
                        "[o7]   command {} (conversation {}): bound to run {run_id} — \
                         {classification}",
                        command.command_id, command.conversation_id
                    );
                }
            }
        }
    }

    // Q-Deck R1 corrective round: the mirror-image gap — a command whose
    // child run already sealed, but whose OWN status bookkeeping never
    // got updated (e.g. `o7 continue`'s post-seal write failed). Purely
    // reconcilable: no provider invocation needed, so `o7 recover` fixes
    // it directly rather than merely reporting it.
    let reconciled = rt
        .block_on(ledger.reconcile_completed_commands())
        .context("reconciling commands whose child run already sealed")?;
    if reconciled.is_empty() {
        println!("[o7] recover: 0 command(s) reconciled");
    } else {
        println!(
            "[o7] recover: {} command(s) reconciled to completed (child run already sealed):",
            reconciled.len()
        );
        for command in &reconciled {
            println!(
                "[o7]   command {} (conversation {})",
                command.command_id, command.conversation_id
            );
        }
    }
    Ok(())
}

/// Thin wrapper over the scoped `o7::recovery::catch_up_record` primitive —
/// this CLI's `--run-dir` handling and `o7d`'s own sealed-recovery path
/// call the SAME library function, never two divergent implementations.
/// Prints a summary; the plain-recovery caller (`recover`, below) never
/// sees anything beyond this run — no global scan happens here.
///
/// # Errors
/// See `o7::recovery::CatchUpError`.
fn catch_up(run_dir: &Path, ledger_path: &Path) -> Result<()> {
    let receipt = o7::recovery::catch_up_record(ledger_path, run_dir, None)
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    println!(
        "[o7] recover: caught up run {} ({}, session backfill: {:?})",
        receipt.run_id,
        if receipt.sealed {
            "sealed"
        } else {
            "still running"
        },
        receipt.session_backfill
    );
    if !receipt.projection_applied {
        eprintln!(
            "[o7] recover: warning — run {}'s live ledger projection was incomplete; the \
             canonical record itself is fully verified, but a sink write failed",
            receipt.run_id
        );
    }
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
    let mut contract = events::build_contract(&manifest)?;

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

    // Q-Deck A0 corrective round 1 (`docs/q-deck/a0-candidate-state.md`
    // §2.2): a fresh, ledger-backed top-level run FIXES its own
    // candidate-state contract obligation here, before RunStarted — its
    // own resolved base commit becomes the conversation's immutable base
    // for every future command continuation to inherit unchanged.
    if let Some(p) = &pending {
        contract.candidate_state = Some(CandidateStateContractV1 {
            schema: o7_run::CANDIDATE_STATE_CONTRACT_SCHEMA_V1,
            conversation_id: p.conversation_id().as_str().to_owned(),
            repository_id: worktree::repository_identity(&repo)?,
            base_commit: base_commit.clone(),
            patch_kind: CandidatePatchKind::GitBinaryCumulativePatchV1,
        });
    }

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
        session_id: ar.session_id.as_ref().map(|s| s.as_str().to_owned()),
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
/// Best-effort: open a fresh connection to `ledger_path` and durably record
/// `run_id`'s provider session identity. Callers must treat any error here
/// as non-fatal to the canonical run (see `execute_live`'s and
/// `continue_execute`'s own call sites) — this is a live-projection write
/// like any other, never a gate on the canonical record.
fn persist_provider_session(
    ledger_path: &Path,
    run_id: &CanonicalRunId,
    session: &agent::ProviderSessionId,
) -> Result<()> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("starting the session-persistence runtime")?;
    let ledger = o7_ledger::SqliteLedger::open(ledger_path)
        .with_context(|| format!("opening ledger at {}", ledger_path.display()))?;
    rt.block_on(ledger.set_run_provider_session(
        o7_ledger::RunId::from_raw(run_id.as_str().to_owned()),
        session.as_str().to_owned(),
    ))
    .context("calling set_run_provider_session")?;
    Ok(())
}

/// Q-Deck R1 fifth corrective round: durably write the canonical provider-
/// session receipt (`session_receipt.json`) and return the `ArtifactRef`
/// a caller appends into the canonical stream as a `ProviderSessionCaptured`
/// event, right after `AgentExited`. This — never `meta.json` — is what
/// recovery's session backfill trusts: digest-bound into the SAME
/// tamper-evident chain every other artifact is, so a clean replay
/// actually proves the receipt belongs to this run, rather than merely
/// asserting a matching `run_id` field inside an otherwise-uncorrelated
/// sidecar file.
///
/// # Errors
/// Any I/O failure durably writing the receipt file.
fn write_provider_session_receipt(
    rec: &RunRecord,
    run_id: &str,
    engine: &str,
    model: &str,
    session: &agent::ProviderSessionId,
) -> Result<o7_run::event::ArtifactRef> {
    let receipt = ProviderSessionReceipt {
        schema: 1,
        run_id: run_id.to_owned(),
        engine: engine.to_owned(),
        provider: engine.to_owned(),
        model: model.to_owned(),
        session_id: session.as_str().to_owned(),
    };
    let bytes = serde_json::to_vec(&receipt).context("serializing session_receipt.json")?;
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(rec.dir.join(PROVIDER_SESSION_RECEIPT_FILE))
        .context("opening session_receipt.json for a durable write")?;
    f.write_all(&bytes)
        .context("writing session_receipt.json")?;
    f.sync_data()
        .context("flushing session_receipt.json to disk")?;
    Ok(events::artifact(
        ArtifactKind::ProviderSession,
        PROVIDER_SESSION_RECEIPT_FILE,
        &bytes,
    ))
}

/// Durably write `bytes` at `name` inside `rec`'s own directory as a
/// canonical artifact, returning its `ArtifactRef`.
fn write_candidate_artifact(
    rec: &RunRecord,
    name: &str,
    kind: ArtifactKind,
    bytes: &[u8],
) -> Result<o7_run::event::ArtifactRef> {
    rec.write_bytes_durable(name, bytes)
        .with_context(|| format!("durably writing {name}"))?;
    Ok(events::artifact(kind, name, bytes))
}

/// Q-Deck A0 (`docs/q-deck/a0-candidate-state.md` §2/§4): capture this run's
/// own cumulative candidate state and durably write its receipt/patch pair,
/// returning the `ArtifactRef` a caller appends as `CandidateStateCaptured`
/// right after `PatchCaptured`. Shared by both `execute_live` and
/// `continue_execute` — the two ledger-backed paths that can become a
/// future command's parent.
///
/// `obligation` is this run's OWN canonical-bound contract obligation
/// (`docs/q-deck/a0-candidate-state.md` §2.2) — its `base_commit` is the
/// CONVERSATION's own immutable base (this run's own resolved `--base` for
/// a fresh conversation's first run; inherited UNCHANGED from the verified
/// parent's own contract for a command continuation), never re-resolved
/// from a live ref or a receipt's own self-claim.
///
/// # Errors
/// Any underlying `git` failure, or I/O failure durably writing the
/// receipt/patch files.
fn write_candidate_state_receipt(
    rec: &RunRecord,
    wt: &Path,
    obligation: &CandidateStateContractV1,
    run_id: &str,
    parent_run_id: Option<&str>,
) -> Result<o7_run::event::ArtifactRef> {
    let (patch, tree_oid) = worktree::capture_cumulative_candidate(wt, &obligation.base_commit)
        .context("capturing this run's cumulative candidate state")?;
    let patch_ref = write_candidate_artifact(
        rec,
        CANDIDATE_PATCH_FILE,
        ArtifactKind::CandidatePatch,
        &patch,
    )
    .context("durably writing candidate.patch")?;
    let receipt = CandidateStateReceiptV1 {
        schema: o7_run::CANDIDATE_STATE_RECEIPT_SCHEMA_V1,
        repository_id: obligation.repository_id.clone(),
        base_commit: obligation.base_commit.clone(),
        run_id: o7_run::ids::RunId::new(run_id.to_owned())
            .map_err(|e| anyhow::anyhow!("minting run id for the candidate receipt: {e}"))?,
        conversation_id: obligation.conversation_id.clone(),
        parent_run_id: parent_run_id
            .map(|p| o7_run::ids::RunId::new(p.to_owned()))
            .transpose()
            .map_err(|e| anyhow::anyhow!("minting parent run id for the candidate receipt: {e}"))?,
        candidate_tree_oid: tree_oid,
        patch_kind: obligation.patch_kind,
        patch: patch_ref,
    };
    let receipt_bytes =
        serde_json::to_vec(&receipt).context("serializing candidate_state_receipt.json")?;
    write_candidate_artifact(
        rec,
        CANDIDATE_STATE_RECEIPT_FILE,
        ArtifactKind::CandidateState,
        &receipt_bytes,
    )
    .context("durably writing candidate_state_receipt.json")
}

/// Q-Deck A0 corrective round 1: load, replay-verify, and semantically
/// verify a run directory's own candidate-state receipt against ITS OWN
/// contract — the one shared primitive both the early (pre-`RunStarted`)
/// contract-inheritance step and the late (post-`attach_run`) materialization
/// step build on, so the two never drift apart on what counts as "verified."
///
/// # Errors
/// The directory's canonical record fails to load/parse/chain/digest/
/// structurally verify, it has no candidate-state receipt, the receipt
/// fails to parse or disagrees with its own contract, or its nested patch
/// artifact fails to resolve/digest-check.
fn load_verified_candidate_receipt(
    dir: &Path,
) -> Result<(o7_run::state::RunState, CandidateStateReceiptV1)> {
    let events_text = std::fs::read_to_string(dir.join(events::EVENTS_FILE))
        .with_context(|| format!("reading the canonical events.jsonl at {}", dir.display()))?;
    let parsed = events::from_jsonl(&events_text).context("parsing canonical events.jsonl")?;
    let resolver = events::RecordDirResolver {
        base: dir.to_path_buf(),
    };
    let (state, _artifacts_verified) = o7_run::replay::verify_prefix(&parsed, &resolver)
        .map_err(|e| anyhow::anyhow!("canonical record failed verification: {e}"))?;
    let contract = state.contract.as_ref().ok_or_else(|| {
        anyhow::anyhow!("canonical record has no contract (unreachable for a valid stream)")
    })?;
    let receipt =
        o7_run::candidate::verify_candidate_state_captured(&state, contract, &resolver)
            .map_err(|e| anyhow::anyhow!("candidate-state semantic verification failed: {e}"))?;
    Ok((state, receipt))
}

/// Q-Deck A0 corrective round 1 (`docs/q-deck/a0-candidate-state.md` §2.2):
/// resolve the CHILD's own candidate-state contract obligation by
/// inheriting it UNCHANGED from the verified PARENT's own contract — never
/// re-derived from a mutable CLI argument or a receipt's own self-claim.
/// Called EARLY, before `RunStarted` is even minted (so it can be embedded
/// in the child's own contract) — a failure here leaves the command
/// exactly as `o7d` left it (no run directory for this child ever gets
/// created), which the existing redrive machinery already treats as safely
/// redrivable (`Absent`), mirroring the pre-existing "re-validate the
/// parent's provider session" check this same caller already performs at
/// the same point.
///
/// # Errors
/// The parent's own canonical record/receipt fails to load or semantically
/// verify, or its `conversation_id` disagrees with `conversation_id`.
fn resolve_inherited_candidate_obligation(
    repo: &Path,
    runs_dir: &Path,
    target: &str,
    parent_run_id: &str,
    conversation_id: &str,
) -> Result<CandidateStateContractV1> {
    let parent_dir = runs_dir.join(target).join(parent_run_id);
    let (_parent_state, receipt) =
        load_verified_candidate_receipt(&parent_dir).with_context(|| {
            "the parent run has no valid candidate-state receipt — command continuations require \
         one; a parent that predates Q-Deck A0, or whose receipt fails verification, cannot be \
         continued"
        })?;
    anyhow::ensure!(
        receipt.conversation_id == conversation_id,
        "the parent's candidate-state receipt belongs to a different conversation"
    );
    anyhow::ensure!(
        receipt.run_id.as_str() == parent_run_id,
        "the parent's candidate-state receipt's own run_id does not match the parent run"
    );
    // Q-Deck A0 corrective round 3 (Codex P1, Part 4, defense in depth):
    // the SAME repository-identity check `o7d`'s own admission preflight
    // performs, re-verified here for whatever reaches this code path
    // DESPITE that preflight — a direct CLI invocation, a race, or a row
    // accepted by an older deployment. Rejecting here happens strictly
    // before `RunStarted`/`attach_run`, so a foreign-repository parent
    // never gets a run directory created for this child at all.
    let this_repo_id = worktree::repository_identity(repo)?;
    anyhow::ensure!(
        receipt.repository_id == this_repo_id,
        "the parent's candidate-state receipt was captured against a different repository than \
         this server is currently configured for"
    );
    // The child's own contract always declares THIS build's own understood
    // candidate-state contract schema — never the parent's raw schema value
    // (which `load_verified_candidate_receipt`'s full `verify_prefix` call
    // above already proved equals this same constant, since Q-Deck A0
    // corrective round 4's reducer check refuses any other value at the
    // parent's own `RunStarted`). Naming this constant instead of borrowing
    // `CANDIDATE_STATE_RECEIPT_SCHEMA_V1` keeps the contract and receipt
    // schema spaces conceptually distinct even though both are `1` today.
    Ok(CandidateStateContractV1 {
        schema: o7_run::CANDIDATE_STATE_CONTRACT_SCHEMA_V1,
        conversation_id: receipt.conversation_id,
        repository_id: receipt.repository_id,
        base_commit: receipt.base_commit,
        patch_kind: receipt.patch_kind,
    })
}

/// Q-Deck A0 corrective round 1 (`docs/q-deck/a0-candidate-state.md` §5/§6):
/// locate, FULLY replay- and semantically verify, and materialize the
/// PARENT run's own cumulative candidate state into the child's fresh
/// worktree (created here, at the parent's own immutable `base_commit` —
/// never the child's `--base` CLI flag) — everything a command-continuation
/// child must prove BEFORE the durable dispatch boundary (`AgentStarted`).
/// Every failure here is a plain `anyhow::Error` returned to the caller; by
/// construction it always happens strictly before `AgentStarted` is ever
/// appended, so it resolves through R1's existing pre-dispatch-redrive
/// machinery with no new lifecycle status.
///
/// Requires the parent to be genuinely SEALED and its canonical record's
/// last event to be `RunSealed` — a truncated-after-capture parent (valid
/// prefix, but never actually sealed) is refused here, not merely "trusted
/// because a receipt happened to be present."
///
/// Returns the child's own `source_receipt`/`source_patch` `ArtifactRef`s
/// (both already durably copied) and `materialized_tree_oid` for the
/// caller to append as `CandidateStateMaterialized`.
///
/// # Errors
/// Any of: the parent's canonical record fails to load/parse/verify or is
/// not sealed; its candidate-state receipt fails semantic verification
/// against its own contract; its `run_id`/`conversation_id` disagree with
/// this continuation; the patch fails to apply cleanly; the resulting tree
/// OID disagrees with the receipt's own `candidate_tree_oid`; or the
/// resulting tree contains a gitlink.
#[allow(clippy::too_many_arguments)]
fn materialize_parent_candidate_state(
    rec: &RunRecord,
    repo: &Path,
    runs_dir: &Path,
    target: &str,
    wt: &Path,
    run_id: &str,
    conversation_id: &str,
    parent_run_id: &str,
) -> Result<(
    o7_run::event::ArtifactRef,
    o7_run::event::ArtifactRef,
    String,
)> {
    let parent_dir = runs_dir.join(target).join(parent_run_id);
    let (parent_state, receipt) = load_verified_candidate_receipt(&parent_dir)?;

    anyhow::ensure!(
        parent_state.verdict.is_some(),
        "the parent run is not sealed — a truncated-after-capture prefix must not be \
         materialized from"
    );
    let parent_events_text = std::fs::read_to_string(parent_dir.join(events::EVENTS_FILE))
        .context("re-reading the parent's own canonical events.jsonl for its final event")?;
    let last_kind_is_sealed = parent_events_text
        .lines()
        .rfind(|l| !l.trim().is_empty())
        .map(|l| l.contains("\"type\":\"run_sealed\""))
        .unwrap_or(false);
    anyhow::ensure!(
        last_kind_is_sealed,
        "the parent's own canonical record's last event is not RunSealed"
    );

    anyhow::ensure!(
        receipt.run_id.as_str() == parent_run_id,
        "the parent's candidate-state receipt's own run_id does not match the parent run"
    );
    anyhow::ensure!(
        receipt.conversation_id == conversation_id,
        "the parent's candidate-state receipt belongs to a different conversation"
    );
    let this_repo_id = worktree::repository_identity(repo)?;
    anyhow::ensure!(
        receipt.repository_id == this_repo_id,
        "the parent's candidate-state receipt was captured against a different repository"
    );
    worktree::verify_commit_exists(repo, &receipt.base_commit).with_context(|| {
        format!(
            "the parent's candidate-state receipt names a base_commit ({}) that does not exist \
             in this repository",
            receipt.base_commit
        )
    })?;

    // `load_verified_candidate_receipt` already proved the receipt's own
    // nested patch artifact resolves and its digest matches — safe to
    // re-read those exact bytes now.
    let patch_bytes = std::fs::read(parent_dir.join(&receipt.patch.locator))
        .context("re-reading the parent's own candidate.patch")?;

    // Durable copies of the parent's own receipt AND patch, before the
    // canonical event that references them — same crash-window discipline
    // as `ledger_binding.json`/`command_binding.json`. Raw-byte copies (not
    // re-serialized), so their digests trivially match the originals.
    let receipt_bytes = std::fs::read(parent_dir.join(CANDIDATE_STATE_RECEIPT_FILE))
        .context("re-reading the parent's own candidate_state_receipt.json bytes")?;
    let source_receipt_ref = write_candidate_artifact(
        rec,
        PARENT_CANDIDATE_RECEIPT_FILE,
        ArtifactKind::CandidateState,
        &receipt_bytes,
    )
    .context("durably writing parent_candidate_receipt.json")?;
    let source_patch_ref = write_candidate_artifact(
        rec,
        PARENT_CANDIDATE_PATCH_FILE,
        ArtifactKind::CandidatePatch,
        &patch_bytes,
    )
    .context("durably writing parent_candidate.patch")?;

    worktree::add(repo, &receipt.base_commit, wt, &format!("o7/{run_id}"))
        .context("creating the child run's worktree at the parent's own candidate base commit")?;
    let materialized_tree_oid =
        worktree::apply_candidate_patch(runs_dir, wt, &receipt.base_commit, &patch_bytes)
            .context("applying the parent's own cumulative candidate patch")?;
    anyhow::ensure!(
        materialized_tree_oid == receipt.candidate_tree_oid,
        "materializing the parent's candidate state produced tree {materialized_tree_oid}, but \
         its own receipt declares {}",
        receipt.candidate_tree_oid
    );

    Ok((source_receipt_ref, source_patch_ref, materialized_tree_oid))
}

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
        parent_run_id: None,
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

    // Extracted before `contract` moves into `RunStarted` below.
    let candidate_state_obligation = contract.candidate_state.clone();

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
            None,
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

    // Q-Deck R1 (`docs/q-deck/r1-command.md` §9.1): persist the provider
    // session identity durably whenever one came back — a forward-looking
    // side effect of the ORIGINAL `o7 run --ledger` path that does not
    // change this path's own observable behavior. Best-effort, like every
    // other live-projection write in this function (`project`, below) —
    // NEVER `?`. R0.7 spent five corrective rounds establishing that a
    // ledger-sink failure must never abort or gate the canonical record;
    // an earlier version of this exact block used `?` here and would have
    // silently reintroduced that regression, aborting the run before
    // AgentExited/gates/RunSealed ever got appended. A failure here only
    // means a future command can't continue from this run until `o7
    // recover` catches it up — never that the run itself didn't happen.
    if let Some(session) = &ar.session_id {
        if let Some(ledger_path) = &a.ledger {
            if let Err(e) = persist_provider_session(ledger_path, &canonical_run_id, session) {
                projection_incomplete = true;
                eprintln!(
                    "[o7] warning: persisting this run's provider session identity failed: \
                     {e:#} — continuing the run; a future command cannot continue from it \
                     until this is caught up"
                );
            }
        }
    }

    let agent_exited = append_only(
        &mut chain,
        o7_run::event::RunEventKind::AgentExited {
            outcome: events::agent_outcome(&ar),
        },
    )?;
    project(&projector, &agent_exited, &mut projection_incomplete);

    // Q-Deck R1 fifth corrective round: the canonical, digest-bound session
    // receipt -- the durable evidence recovery's session backfill now
    // trusts, never `meta.json`. Written and appended like any other
    // canonical artifact event (`?`, not best-effort): once a session
    // exists, failing to durably record it canonically is treated the same
    // as failing to write `diff.patch` below, not as a live-projection
    // sink issue.
    if let Some(session) = &ar.session_id {
        let receipt_ref = write_provider_session_receipt(rec, run_id, &a.engine, &a.model, session)
            .context("durably writing session_receipt.json")?;
        let session_captured = append_only(
            &mut chain,
            o7_run::event::RunEventKind::ProviderSessionCaptured {
                receipt: receipt_ref,
            },
        )?;
        project(&projector, &session_captured, &mut projection_incomplete);
    }

    rec.write_agent_stdout(&ar.stdout)?;
    let diff = worktree::diff_vs_base(wt, &a.base).unwrap_or_default();
    rec.write_diff(&diff)?;
    let diff_ref = events::artifact(ArtifactKind::Diff, "diff.patch", diff.as_bytes());
    let patch_captured = append_only(
        &mut chain,
        o7_run::event::RunEventKind::PatchCaptured { patch: diff_ref },
    )?;
    project(&projector, &patch_captured, &mut projection_incomplete);

    // Q-Deck A0 (`docs/q-deck/a0-candidate-state.md`): this run's own
    // cumulative candidate state, relative to the conversation's immutable
    // base commit (this run's own `--base`, since a fresh top-level `o7 run
    // --ledger` conversation has no parent to inherit one from) — so a
    // future command continuation can materialize from it. Digest-bound and
    // durable exactly like every other artifact event here.
    let candidate_obligation = candidate_state_obligation.as_ref().ok_or_else(|| {
        anyhow::anyhow!("this ledger-backed run's own contract has no candidate_state obligation")
    })?;
    let candidate_receipt_ref =
        write_candidate_state_receipt(rec, wt, candidate_obligation, run_id, None)
            .context("capturing this run's cumulative candidate state")?;
    let candidate_state_captured = append_only(
        &mut chain,
        o7_run::event::RunEventKind::CandidateStateCaptured {
            receipt: candidate_receipt_ref,
        },
    )?;
    project(
        &projector,
        &candidate_state_captured,
        &mut projection_incomplete,
    );

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

/// Q-Deck R1 second corrective round: the run-id-keyed exclusive lock this
/// process holds for its ENTIRE lifetime, mirrored exactly by `o7d`'s own
/// pre-redrive liveness check (`crates/o7d/src/routes.rs::no_live_process_holds`
/// — same path formula, deliberately duplicated rather than shared across
/// crates for a two-line helper). `flock` auto-releases on process death
/// regardless of how the process died (clean exit, panic, SIGKILL), so a
/// caller can prove this exact run id has no live owner by attempting the
/// SAME lock itself — a real liveness proof, not a wall-clock guess.
fn command_lock_path(runs_dir: &Path, run_id: &str) -> PathBuf {
    runs_dir.join(".locks").join(format!("{run_id}.lock"))
}

/// Q-Deck R1 third corrective round (blocker 2): closes the window between
/// a redrive deciding "this run id's process is dead" (its lock check
/// found nothing holding it) and this SAME process — merely slow to
/// start, not actually dead — finally getting scheduled and acquiring its
/// own lock. By the time THIS check runs (immediately after the lock
/// acquisition succeeds, before touching the ledger, the worktree, or the
/// agent), a redrive that raced this exact window has already durably
/// rebound the command elsewhere; reading the command's CURRENT binding
/// fresh catches that. `false` means this process has been superseded —
/// it must do NOTHING further (never invoke the agent, never touch
/// canonical state, never mutate command bookkeeping meant for whichever
/// run actually holds the binding now).
fn command_still_bound_to_this_run(
    ledger_path: &Path,
    command_id: &str,
    run_id: &str,
) -> Result<bool> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("starting the binding-check runtime")?;
    let ledger = o7_ledger::SqliteLedger::open(ledger_path)
        .with_context(|| format!("opening ledger at {}", ledger_path.display()))?;
    let command = rt
        .block_on(ledger.command(o7_ledger::CommandId::from_raw(command_id.to_owned())))
        .context("looking up the command's current binding")?;
    Ok(command
        .and_then(|c| c.child_run_id)
        .is_some_and(|bound| bound.as_str() == run_id))
}

/// Q-Deck R1's command-continuation executor (`docs/q-deck/r1-command.md`
/// §9.5) — dispatch ONE durably accepted command as a brand new sealed
/// child run, continuing the parent run's own provider session. Reuses the
/// SAME durability-ordering pipeline `execute_live` proves out for `o7 run
/// --ledger`: durable task/binding before `RunStarted`, per-event
/// append+sync, the two-phase `PendingProjection`/`attach_run` split, the
/// canonical reducer, and `LiveLedgerProjector::seal` — never a second
/// reducer, never a bypass of the ordinary live path.
///
/// # Errors
/// The parent run does not exist, does not belong to `--conversation-id`,
/// or has no durable provider session (defense-in-depth: `o7d`'s
/// `create_command` already checked this, but this subcommand re-checks
/// before spending anything, exactly like an invalid gate manifest);
/// any worktree, agent, gate, or ledger error.
fn continue_run(a: &ContinueArgs) -> Result<()> {
    // MUST be the very first thing this process does, before touching the
    // ledger, the worktree, or anything else: prove no other live process
    // already owns this exact run id. `o7d`'s own retry-redrive logic
    // performs the SAME check before ever deciding to spawn a second
    // process for this run id — if that check raced a genuinely slow (not
    // dead) instance of this process, THIS acquisition is the backstop
    // that turns a would-be double-write into a clean, silent no-op
    // instead of two processes corrupting the same `events.jsonl`.
    let lock_path = command_lock_path(&a.runs_dir, &a.run_id);
    if let Some(parent) = lock_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating lock directory {}", parent.display()))?;
    }
    // The lock file's own content never matters — only its existence and
    // flock state — so `truncate(false)` is explicit, not an oversight.
    let lock_file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(&lock_path)
        .with_context(|| format!("opening lock file {}", lock_path.display()))?;
    let _lock_guard =
        match nix::fcntl::Flock::lock(lock_file, nix::fcntl::FlockArg::LockExclusiveNonblock) {
            Ok(guard) => guard,
            Err((_file, _errno)) => {
                // Another process already, provably, holds this run id's
                // lock — it is the legitimate owner. Exit quietly WITHOUT
                // touching any durable command/run state: a rejection here
                // would incorrectly reject a command the other process may
                // still complete successfully.
                eprintln!(
                    "[o7] continue: run {} is already owned by a live process — exiting without \
                 touching any durable state",
                    a.run_id
                );
                return Ok(());
            }
        };

    // Closes the TOCTOU window a redrive's own pre-spawn lock check cannot:
    // this process may have been merely SLOW to reach the lock above (not
    // dead), and a redrive already decided otherwise and rebound the
    // command elsewhere before this process got here. A fresh read of the
    // command's CURRENT binding, taken immediately after winning the lock,
    // catches that — if the binding has already moved on, do NOTHING
    // further: never invoke the agent, never touch canonical state, never
    // mutate bookkeeping that now belongs to a different run.
    if !command_still_bound_to_this_run(&a.ledger, &a.command_id, &a.run_id)? {
        eprintln!(
            "[o7] continue: command {} no longer binds run {} — a redrive already superseded \
             it while this process was starting; exiting without touching any durable state",
            a.command_id, a.run_id
        );
        return Ok(());
    }

    let repo = a
        .repo
        .canonicalize()
        .with_context(|| format!("repo not found: {}", a.repo.display()))?;
    let target = a.target.clone().unwrap_or_else(|| {
        repo.file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "target".into())
    });
    let gate_path = a
        .gate
        .clone()
        .unwrap_or_else(|| repo.join(".007").join("gate.toml"));
    let manifest = GateManifest::load(&gate_path)?;
    let mut contract = events::build_contract(&manifest)?;

    let lookup_rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("starting the parent-lookup runtime")?;
    let ledger = o7_ledger::SqliteLedger::open(&a.ledger)
        .with_context(|| format!("opening ledger at {}", a.ledger.display()))?;
    let parent_run_id = o7_ledger::RunId::from_raw(a.parent_run_id.clone());
    let parent = lookup_rt
        .block_on(ledger.run(parent_run_id.clone()))
        .context("looking up the parent run")?
        .with_context(|| format!("parent run {} does not exist", a.parent_run_id))?;
    anyhow::ensure!(
        parent.conversation_id.as_str() == a.conversation_id,
        "parent run {} does not belong to conversation {}",
        a.parent_run_id,
        a.conversation_id
    );
    let parent_session_raw = parent.provider_session_id.clone().with_context(|| {
        format!(
            "parent run {} has no durable provider session to continue from — refusing to \
             start a fresh, uncontinued session and call it a continuation",
            a.parent_run_id
        )
    })?;
    let session_id = agent::ProviderSessionId::new(parent_session_raw)
        .context("parent run's stored provider session id is invalid")?;
    drop(lookup_rt);

    // Q-Deck A0 corrective round 2 (Part 6): `mark_rejected` is now defined
    // BEFORE the candidate-obligation resolution below, and used to guard
    // EVERY early failure — not just `continue_execute`'s own outcome. The
    // original round's own bug: this closure existed but was only wired up
    // to the LATE (post-`continue_execute`) error path; a failure inside
    // `resolve_inherited_candidate_obligation` propagated via a bare `?`
    // and returned straight out of `continue_run`, leaving an accepted
    // command permanently `started`/`accepted` with no terminal transition
    // ever attempted — invisible to `stuck_commands` (scoped to unbound
    // rows) and to `reconcile_completed_commands` (scoped to a sealed
    // child that never existed). Uses the atomic, single-transaction
    // conditional (`mark_command_rejected_if_unattached_and_bound`) rather
    // than the old check-then-act pair (`ledger_run_exists` followed by an
    // unconditional `mark_command_rejected`) — a concurrent `attach_run`
    // landing between a separate check and write can no longer be raced
    // into an incorrect rejection of a command whose child has, by now,
    // become real and durable.
    let command_id = o7_ledger::CommandId::from_raw(a.command_id.clone());
    let mark_rejected_if_unattached = {
        let command_id = command_id.clone();
        let run_id = a.run_id.clone();
        let ledger_path = a.ledger.clone();
        move || {
            let rt = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(rt) => rt,
                Err(e) => {
                    eprintln!("[o7] continue: starting the reject-marking runtime failed: {e}");
                    return;
                }
            };
            let ledger = match o7_ledger::SqliteLedger::open(&ledger_path) {
                Ok(ledger) => ledger,
                Err(e) => {
                    eprintln!("[o7] continue: opening the ledger for reject-marking failed: {e}");
                    return;
                }
            };
            let outcome = rt.block_on(ledger.mark_command_rejected_if_unattached_and_bound(
                command_id.clone(),
                o7_ledger::RunId::from_raw(run_id.clone()),
            ));
            match outcome {
                Ok(o7_ledger::RejectionOutcome::Rejected(_)) => {
                    eprintln!(
                        "[o7] continue: command {command_id} marked rejected (never attached)"
                    );
                }
                Ok(
                    o7_ledger::RejectionOutcome::NotEligible(_)
                    | o7_ledger::RejectionOutcome::NotFound,
                ) => {
                    // Already terminal, rebound elsewhere, or — the safe direction —
                    // the run id HAS attached a ledger row by now: leave it alone,
                    // exactly as the old `ledger_run_exists` check's own "assume
                    // attached on uncertainty" discipline required.
                }
                Err(e) => {
                    eprintln!("[o7] continue: reject-marking failed: {e:#}");
                }
            }
        }
    };

    // Q-Deck A0 corrective round 1 (`docs/q-deck/a0-candidate-state.md`
    // §2.2): resolve the child's OWN candidate-state contract obligation by
    // inheriting it unchanged from the verified parent's own contract —
    // before RunStarted, at the SAME defense-in-depth re-validation point
    // as the provider-session check just above. Q-Deck A0 corrective round
    // 2 (Part 6): a failure here now ALSO attempts the same conditional
    // reject-if-unattached transition every other early failure gets —
    // fixing the exact gap the independent re-gate found.
    match resolve_inherited_candidate_obligation(
        &repo,
        &a.runs_dir,
        &target,
        &a.parent_run_id,
        &a.conversation_id,
    ) {
        Ok(obligation) => contract.candidate_state = Some(obligation),
        Err(e) => {
            mark_rejected_if_unattached();
            return Err(e.context("resolving the parent's candidate-state contract obligation"));
        }
    }

    // Q-Deck A0 (`docs/q-deck/a0-candidate-state.md` §5): the child's
    // worktree is no longer created here, at the CLI's own `--base` flag —
    // it is created INSIDE `continue_execute`, at the parent's own
    // candidate-state receipt's immutable `base_commit`, only once that
    // receipt has been located and verified. This is deliberate: creating
    // it here (before the ledger row exists) would mean a materialization
    // failure has to go through the SAME "reject, needs a fresh idempotency
    // key" path an ordinary pre-attach worktree failure already does —
    // whereas A0's own materialization failures must stay safely,
    // same-key-redrivable, exactly like any other pre-`AgentStarted`
    // failure. Moving worktree creation to AFTER `attach_run` (still well
    // before `AgentStarted`) makes the EXISTING generic error handler below
    // (`if !ledger_run_exists { reject } else { leave started }`) do the
    // right thing for a materialization failure with zero special-casing.
    let wt = a.worktree_root.join(format!("{target}-{}", a.run_id));
    std::fs::create_dir_all(&a.worktree_root)?;

    let outcome = continue_execute(
        a,
        &repo,
        &target,
        &wt,
        &session_id,
        &contract,
        &manifest,
        parent_run_id,
    );

    if a.keep_worktree {
        eprintln!("[o7] worktree kept at {}", wt.display());
    } else if let Err(e) = worktree::remove(&repo, &wt) {
        eprintln!("[o7] warning: worktree cleanup failed: {e}");
    }

    let (verdict, projection_incomplete, child_session_id) = match outcome {
        Ok(v) => v,
        Err(e) => {
            // Q-Deck R1 third corrective round (blocker 1): marking the
            // COMMAND rejected here is only safe if this run never
            // durably attached to the ledger at all — once `attach_run`
            // (inside `continue_execute`) succeeds, the child is a real,
            // durable tail. Rejecting the command while leaving that
            // child non-terminal forever (an ordinary error well past
            // attach — a gate command failing to spawn, an artifact write
            // failing, anything) would permanently block the
            // conversation: neither the old (now-stale) parent nor this
            // non-terminal child could ever become a valid parent again,
            // and a `rejected` command is invisible to every existing
            // discovery/redrive mechanism (all scoped to
            // `accepted`/`started`). So: reject ONLY if unattached — leave
            // the command's bookkeeping exactly as `o7d` left it
            // (`started`) otherwise, so the EXISTING stuck-command
            // discovery/redrive machinery — already built to handle a
            // child stuck at any non-terminal status — is what resolves
            // it, not a premature rejection here. Q-Deck A0 corrective
            // round 2: the atomic conditional transition itself now
            // decides "unattached" (inside the SAME transaction as the
            // write), replacing the old separate `ledger_run_exists`
            // check-then-act.
            mark_rejected_if_unattached();
            return Err(e);
        }
    };

    // From here on the CANONICAL run has already sealed successfully — the
    // events.jsonl on disk is complete and correct regardless of what
    // happens below. Every remaining step is ledger bookkeeping, so a
    // failure here is reported the same way a live-projection failure is
    // (`projection_incomplete`, a warning, a non-zero exit) — never an
    // `anyhow` error that could read as "the run itself failed."
    let mut projection_incomplete = projection_incomplete;
    let finish = (|| -> Result<()> {
        let finish_rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .context("starting the command-completion runtime")?;
        let ledger = o7_ledger::SqliteLedger::open(&a.ledger)
            .with_context(|| format!("opening ledger at {}", a.ledger.display()))?;
        if let Some(child_session) = child_session_id {
            finish_rt
                .block_on(ledger.set_run_provider_session(
                    o7_ledger::RunId::from_raw(a.run_id.clone()),
                    child_session,
                ))
                .context("persisting the child run's own provider session")?;
        }
        // Q-Deck R1 fourth corrective round: `mark_command_completed_if_bound`
        // is ONE atomic conditional write (`status IN (...) AND
        // child_run_id = ?` in the SAME `UPDATE`), replacing the earlier
        // read-then-write pattern here (re-check the binding, THEN
        // unconditionally call `mark_command_completed`) — that pattern
        // left its own narrow window between the read and the write. The
        // lock (and the earlier post-acquisition binding check) already
        // make it extremely unlikely this run was ever a superseded/
        // orphaned attempt by the time it reaches its own seal, but this
        // run's own command_id-scoped write must never be able to stomp
        // bookkeeping that by now belongs to a different (later-redriven)
        // run — that guarantee now lives inside the ledger call itself,
        // not in a separate check at this call site.
        //
        // "completed" is command bookkeeping only — it means "dispatched
        // to a sealed run", not any specific verdict; the child run's own
        // ledger row stays the sole verdict authority
        // (docs/q-deck/r1-command.md §2).
        let outcome = finish_rt
            .block_on(ledger.mark_command_completed_if_bound(
                command_id,
                o7_ledger::RunId::from_raw(a.run_id.clone()),
            ))
            .context("marking the command completed")?;
        if matches!(outcome, o7_ledger::CompletionOutcome::NotBound(_)) {
            eprintln!(
                "[o7] {}: this run sealed successfully, but the command it was dispatching has \
                 since been rebound to a different run — not touching its bookkeeping",
                a.run_id
            );
        }
        Ok(())
    })();
    if let Err(e) = finish {
        projection_incomplete = true;
        eprintln!(
            "[o7] {}: warning — post-seal command bookkeeping failed: {e:#} — the child run \
             itself sealed successfully; run `o7 recover --ledger <path>` to catch this up",
            a.run_id
        );
    }

    println!("[o7] {}: verdict {verdict:?}", a.run_id);
    if projection_incomplete {
        eprintln!(
            "[o7] {}: WARNING — live ledger projection is incomplete; run \
             `o7 recover --ledger <path> --run-dir runs/{target}/{}` to catch up \
             before trusting Q-Deck's view of this run",
            a.run_id, a.run_id
        );
    }
    if verdict != Verdict::Pass || projection_incomplete {
        std::process::exit(1);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn continue_execute(
    a: &ContinueArgs,
    repo: &Path,
    target: &str,
    wt: &Path,
    session_id: &agent::ProviderSessionId,
    contract: &o7_run::event::RunContract,
    manifest: &GateManifest,
    parent_canonical_run_id: o7_ledger::RunId,
) -> Result<(Verdict, bool, Option<String>)> {
    println!(
        "[o7] {}: continuing session, dispatching command in worktree",
        a.run_id
    );

    let rec = RunRecord::create(&a.runs_dir, target, &a.run_id)?;
    let task_ref = events::artifact(ArtifactKind::Task, "task.md", a.command.as_bytes());
    let canonical_run_id = CanonicalRunId::new(a.run_id.clone())
        .map_err(|e| anyhow::anyhow!("minting run id: {e}"))?;
    let parent_canonical_run_id = CanonicalRunId::new(parent_canonical_run_id.as_str().to_owned())
        .map_err(|e| anyhow::anyhow!("re-minting parent run id: {e}"))?;

    // Durable before canonical RunStarted, exactly like `execute_live`.
    LedgerBinding {
        schema: 1,
        run_id: a.run_id.clone(),
        conversation_id: a.conversation_id.clone(),
        agent: "claude".to_string(),
        role: "implementer".to_string(),
        parent_run_id: Some(parent_canonical_run_id.as_str().to_owned()),
    }
    .write_durable(&rec)
    .context("durably writing ledger_binding.json before RunStarted")?;
    rec.write_task_durable(&a.command)
        .context("durably writing task.md before RunStarted")?;
    // Q-Deck R1 fifth corrective round: the exact-Command-identity receipt
    // — durable BEFORE RunStarted (same crash-window discipline as
    // `ledger_binding.json`), and made tamper-evident by the canonical
    // `CommandBindingCaptured` event appended right after RunStarted below
    // (a mutable sidecar alone, with no canonical reference, would not be
    // enough — see `docs/q-deck/r1-command.md` §7).
    let command_binding = CommandBinding {
        schema: 1,
        command_id: a.command_id.clone(),
        conversation_id: a.conversation_id.clone(),
        parent_run_id: parent_canonical_run_id.as_str().to_owned(),
        child_run_id: a.run_id.clone(),
        command_sha256: o7_run::event::Digest256::of_bytes(a.command.as_bytes())
            .as_str()
            .to_owned(),
    };
    command_binding
        .write_durable(&rec)
        .context("durably writing command_binding.json before RunStarted")?;

    let mut chain = events::EventChain::new(canonical_run_id.clone());
    let mut events_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(rec.dir.join(events::EVENTS_FILE))
        .context("opening events.jsonl for durable per-event append")?;
    let mut projection_incomplete = false;

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
            contract: contract.clone(),
            task: task_ref.clone(),
        },
    )?;

    let pending = PendingProjection::open(
        &a.ledger,
        ConversationSelector::Existing(a.conversation_id.clone()),
        &canonical_run_id,
    )
    .context("opening the live ledger projection for this child run")?;
    let projector = pending
        .attach_run(
            &canonical_run_id,
            "claude".to_string(),
            "implementer".to_string(),
            Some(&parent_canonical_run_id),
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

    let command_binding_bytes =
        serde_json::to_vec(&command_binding).context("re-serializing command_binding.json")?;
    let command_binding_ref = events::artifact(
        ArtifactKind::CommandBinding,
        "command_binding.json",
        &command_binding_bytes,
    );
    let command_binding_captured = append_only(
        &mut chain,
        o7_run::event::RunEventKind::CommandBindingCaptured {
            binding: command_binding_ref,
        },
    )?;
    project(
        &projector,
        &command_binding_captured,
        &mut projection_incomplete,
    );

    // Q-Deck A0 (`docs/q-deck/a0-candidate-state.md`): materialize the
    // parent's own cumulative candidate state into a fresh worktree at its
    // own immutable base commit — BEFORE the durable dispatch boundary
    // (`AgentStarted`), so any failure here stays safely, same-key
    // redrivable via R1's existing pre-dispatch machinery (this happens
    // after `attach_run` above, so this run id already has a ledger row).
    let (source_receipt_ref, source_patch_ref, materialized_tree_oid) =
        materialize_parent_candidate_state(
            &rec,
            repo,
            &a.runs_dir,
            target,
            wt,
            &a.run_id,
            &a.conversation_id,
            parent_canonical_run_id.as_str(),
        )
        .context("materializing the parent's own candidate state")?;
    let candidate_state_materialized = append_only(
        &mut chain,
        o7_run::event::RunEventKind::CandidateStateMaterialized {
            source_run_id: parent_canonical_run_id.clone(),
            source_receipt: source_receipt_ref,
            source_patch: source_patch_ref,
            materialized_tree_oid,
        },
    )?;
    project(
        &projector,
        &candidate_state_materialized,
        &mut projection_incomplete,
    );

    let agent_started = append_only(&mut chain, o7_run::event::RunEventKind::AgentStarted)?;
    project(&projector, &agent_started, &mut projection_incomplete);

    let ar = agent::continue_session(session_id, wt, &a.command, &a.model, a.max_turns)?;

    let agent_exited = append_only(
        &mut chain,
        o7_run::event::RunEventKind::AgentExited {
            outcome: events::agent_outcome(&ar),
        },
    )?;
    project(&projector, &agent_exited, &mut projection_incomplete);

    // Q-Deck R1 fifth corrective round: same canonical session-receipt
    // discipline as `execute_live` -- see its own comment there.
    if let Some(session) = &ar.session_id {
        let receipt_ref =
            write_provider_session_receipt(&rec, &a.run_id, "claude", &a.model, session)
                .context("durably writing session_receipt.json")?;
        let session_captured = append_only(
            &mut chain,
            o7_run::event::RunEventKind::ProviderSessionCaptured {
                receipt: receipt_ref,
            },
        )?;
        project(&projector, &session_captured, &mut projection_incomplete);
    }

    rec.write_agent_stdout(&ar.stdout)?;
    let diff = worktree::diff_vs_base(wt, &a.base).unwrap_or_default();
    rec.write_diff(&diff)?;
    let diff_ref = events::artifact(ArtifactKind::Diff, "diff.patch", diff.as_bytes());
    let patch_captured = append_only(
        &mut chain,
        o7_run::event::RunEventKind::PatchCaptured { patch: diff_ref },
    )?;
    project(&projector, &patch_captured, &mut projection_incomplete);

    // Q-Deck A0: this run's OWN cumulative candidate state, relative to the
    // conversation's immutable base commit inherited from the parent above
    // — so a FUTURE command continuing from THIS run can materialize from
    // it in turn.
    let candidate_obligation = contract.candidate_state.as_ref().ok_or_else(|| {
        anyhow::anyhow!("this continuation's own contract has no candidate_state obligation")
    })?;
    let candidate_receipt_ref = write_candidate_state_receipt(
        &rec,
        wt,
        candidate_obligation,
        &a.run_id,
        Some(parent_canonical_run_id.as_str()),
    )
    .context("capturing this run's own cumulative candidate state")?;
    let candidate_state_captured = append_only(
        &mut chain,
        o7_run::event::RunEventKind::CandidateStateCaptured {
            receipt: candidate_receipt_ref,
        },
    )?;
    project(
        &projector,
        &candidate_state_captured,
        &mut projection_incomplete,
    );

    manifest.validate()?;
    let gate_out = rec.gate_dir();
    std::fs::create_dir_all(&gate_out)?;
    let mut steps = Vec::new();
    for step in &manifest.gate {
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
             `o7 recover --ledger <path> --run-dir <run-dir>` before trusting Q-Deck's view \
             of it"
        );
    }

    let meta = RunMeta {
        schema: 1,
        kind: "run".to_string(),
        run_id: a.run_id.clone(),
        target: target.to_string(),
        repo: repo.to_string_lossy().to_string(),
        base_commit: candidate_obligation.base_commit.clone(),
        engine: "claude".to_string(),
        model: a.model.clone(),
        verdict,
        steps,
        agent_exit_code: ar.exit_code,
        session_id: ar.session_id.as_ref().map(|s| s.as_str().to_owned()),
        cost_usd: None,
        started_at: None,
        finished_at: None,
    };
    rec.write_meta(&meta)?;
    println!("[o7] {}: record at {}", a.run_id, rec.dir.display());

    Ok((
        verdict,
        projection_incomplete,
        ar.session_id.map(|s| s.as_str().to_owned()),
    ))
}
