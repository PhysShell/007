//! `o7 invoke` — a narrow, read-only, schema-bound single-shot agent call.
//!
//! `o7 invoke --engine claude|codex|arliai --prompt-file <f> --schema <f>
//! --capability-profile read-only-data --out <dir>`. Not a workflow, not a
//! DAG, not a provider framework: one prompt in, one schema-checked JSON
//! object out, closed-world by construction. This is `judge.rs`'s
//! closed-world call pattern generalized to an arbitrary caller-supplied
//! prompt + schema instead of judge's own hardcoded per-file verdict shape —
//! see `docs/o7-invoke.md` for why this exists and what it deliberately does
//! not do. Zero changes to `o7 run` or `o7 judge`'s own domain behavior:
//! this module is additive only.
//!
//! Two backend families share one contract: the CLI engines (`claude`,
//! `codex` — subprocesses) and the `arliai` HTTPS API backend
//! (`invoke_arliai.rs` — no subprocess at all). The status vocabulary,
//! artifact layout, and independent schema re-validation are identical
//! across all three; classification inputs differ (exit codes + stderr
//! markers vs. HTTP statuses).

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::agent::Engine;
use crate::invoke_arliai::{self, ArliOutcome};

/// The one capability bundle this MVP implements. A named label mapped to a
/// fixed, hardcoded flag set — not something a caller's string can widen.
/// Any other value is refused before any process spawns (fail closed, per
/// `docs/security-layers.md`'s "a capability-profile claim that isn't
/// actually enforceable must refuse, not silently downgrade").
const READ_ONLY_DATA_PROFILE: &str = "read-only-data";

/// Backend subprocess timed out and had to be killed before finishing.
const DEFAULT_TIMEOUT_SECS: u64 = 120;

/// Bounded wait for the best-effort `<binary> --version` probe. A hung or
/// pathologically slow `--version` must never stall the real call; on timeout
/// the probe yields `None` (`command_version` stays null), exactly like any
/// other probe failure.
const VERSION_PROBE_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(clap::Args)]
pub struct InvokeArgs {
    /// Backend: claude | codex (agent CLIs) | arliai (direct HTTPS API).
    #[arg(long)]
    pub engine: String,
    /// Prompt text, read as-is and piped to the backend over stdin (never
    /// argv — see `judge.rs`'s own rationale: size limits, `ps`/`/proc` leak).
    #[arg(long)]
    pub prompt_file: PathBuf,
    /// JSON manifest of input file paths this call's output should be
    /// considered reproducible from: `{"input_paths": ["a", "b"]}`. Hashed
    /// here for provenance only — `o7` never reads their content into the
    /// prompt itself; the caller already built `--prompt-file`'s full text.
    #[arg(long)]
    pub input_manifest: Option<PathBuf>,
    /// JSON Schema (Draft 2020-12) the structured output must satisfy.
    /// Re-validated by `o7` itself after the call — never just trusted from
    /// a backend's own claim of schema-conformance.
    #[arg(long)]
    pub schema: PathBuf,
    /// Named capability restriction bundle. Only "read-only-data" exists
    /// today.
    #[arg(long)]
    pub capability_profile: String,
    /// Model id/alias, forwarded to the backend only if given. Neither
    /// engine gets a default pinned here — see `call_claude`/`call_codex`.
    #[arg(long)]
    pub model: Option<String>,
    /// Run directory: prompt.txt, stdout.raw, stderr.log, result.json (if
    /// any), meta.json are all written here.
    #[arg(long)]
    pub out: PathBuf,
    /// Kill the backend if it hasn't finished after this many seconds.
    #[arg(long, default_value_t = DEFAULT_TIMEOUT_SECS)]
    pub timeout_secs: u64,
}

/// spec vocabulary shared with Demand Radar's `AgentRunStatus` — the
/// cross-repo conformance gate (`docs/o7-invoke.md`) compares these strings
/// directly, so they must not drift from `demand_radar.models.AgentRunStatus`.
/// One deliberate exception: `BlockedProvider` is 007-side only (the arliai
/// backend's provider-fault bucket — 3xx/5xx/transport). The gate runs
/// `claude`/`codex` only, so the shared-vocabulary invariant it checks is
/// untouched; Demand Radar must adopt `BLOCKED_PROVIDER` before ever growing
/// an arliai path of its own (docs/o7-invoke.md, classification matrix).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InvokeStatus {
    Pass,
    BlockedAuth,
    BlockedUsage,
    BlockedTimeout,
    BlockedNotInstalled,
    BlockedProvider,
    FailInvalidOutput,
    FailSchema,
}

impl InvokeStatus {
    fn label(self) -> &'static str {
        match self {
            InvokeStatus::Pass => "PASS",
            InvokeStatus::BlockedAuth => "BLOCKED_AUTH",
            InvokeStatus::BlockedUsage => "BLOCKED_USAGE",
            InvokeStatus::BlockedTimeout => "BLOCKED_TIMEOUT",
            InvokeStatus::BlockedNotInstalled => "BLOCKED_NOT_INSTALLED",
            InvokeStatus::BlockedProvider => "BLOCKED_PROVIDER",
            InvokeStatus::FailInvalidOutput => "FAIL_INVALID_OUTPUT",
            InvokeStatus::FailSchema => "FAIL_SCHEMA",
        }
    }
}

/// Backend selector local to `o7 invoke`. Deliberately NOT `agent::Engine`:
/// that enum means "CLI agent engine" everywhere else in the repo (`o7 run`,
/// `judge`, consensus plans), and the arliai HTTPS backend must not leak
/// into those call sites through a widened shared type. `Engine` stays
/// exactly what it is; this maps onto it only for the two CLI arms.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InvokeBackend {
    Claude,
    Codex,
    ArliAi,
}

impl std::str::FromStr for InvokeBackend {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "claude" => Ok(InvokeBackend::Claude),
            "codex" => Ok(InvokeBackend::Codex),
            "arliai" => Ok(InvokeBackend::ArliAi),
            other => anyhow::bail!("unknown --engine '{other}' (claude | codex | arliai)"),
        }
    }
}

/// Mirrors `demand_radar.models.AgentResult` field-for-field (same names,
/// same shapes) so the conformance gate's "equivalent normalized
/// AgentResult" check is a direct structural comparison, not a translation.
#[derive(Serialize)]
struct InvokeMeta {
    schema: u32,
    provider: &'static str,
    command_version: Option<String>,
    model: Option<String>,
    started_at: String,
    finished_at: String,
    exit_code: Option<i32>,
    status: &'static str,
    stdout_path: String,
    stderr_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    structured_output_path: Option<String>,
    schema_valid: bool,
    prompt_hash: String,
    input_hashes: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error_kind: Option<&'static str>,
}

#[derive(Deserialize, Default)]
struct InputManifest {
    #[serde(default)]
    input_paths: Vec<PathBuf>,
}

/// Everything both backend families need before any network/subprocess
/// activity: run-dir integrity, prompt/schema/manifest reads, the
/// independent validator, provenance hashes, and the `prompt.txt` artifact.
/// One shared function so the two paths cannot drift on refusal order or
/// hashing behavior.
struct PreparedCall {
    prompt: String,
    schema: serde_json::Value,
    validator: jsonschema::Validator,
    input_hashes: Vec<String>,
    prompt_hash: String,
}

fn prepare(a: &InvokeArgs) -> Result<PreparedCall> {
    // Run-dir integrity: the out dir must be absent or an existing EMPTY dir,
    // checked BEFORE the version probe or any backend spawn/connection. A
    // non-empty --out is refused, never partially overwritten -- otherwise a
    // stale result.json from a previous PASS could be mistaken for the output
    // of this run (which, if it FAILs, writes no result.json of its own).
    ensure_empty_out(&a.out)?;

    let prompt = std::fs::read_to_string(&a.prompt_file)
        .with_context(|| format!("reading prompt file {}", a.prompt_file.display()))?;
    let schema_text = std::fs::read_to_string(&a.schema)
        .with_context(|| format!("reading schema {}", a.schema.display()))?;
    let schema: serde_json::Value = serde_json::from_str(&schema_text)
        .with_context(|| format!("parsing schema {}", a.schema.display()))?;
    let validator = jsonschema::validator_for(&schema)
        .with_context(|| format!("{} is not a valid JSON Schema", a.schema.display()))?;

    let manifest: InputManifest = match &a.input_manifest {
        Some(p) => {
            let text = std::fs::read_to_string(p)
                .with_context(|| format!("reading input manifest {}", p.display()))?;
            serde_json::from_str(&text)
                .with_context(|| format!("parsing input manifest {}", p.display()))?
        }
        None => InputManifest::default(),
    };
    let mut input_hashes = Vec::with_capacity(manifest.input_paths.len());
    for p in &manifest.input_paths {
        input_hashes
            .push(sha256_hex_file(p).with_context(|| format!("hashing input {}", p.display()))?);
    }

    // `ensure_empty_out` above already created (or validated-empty) the dir.
    std::fs::write(a.out.join("prompt.txt"), &prompt)
        .with_context(|| format!("writing {}/prompt.txt", a.out.display()))?;
    let prompt_hash = sha256_hex_text(&prompt);

    Ok(PreparedCall {
        prompt,
        schema,
        validator,
        input_hashes,
        prompt_hash,
    })
}

pub fn run(a: &InvokeArgs) -> Result<()> {
    if a.capability_profile != READ_ONLY_DATA_PROFILE {
        anyhow::bail!(
            "unknown --capability-profile '{}' (only '{READ_ONLY_DATA_PROFILE}' is \
             implemented) -- refusing to run rather than silently narrow or widen it",
            a.capability_profile
        );
    }
    let backend: InvokeBackend = a.engine.parse()?;
    match backend {
        InvokeBackend::Claude => run_cli(a, Engine::Claude),
        InvokeBackend::Codex => run_cli(a, Engine::Codex),
        InvokeBackend::ArliAi => run_arliai(a),
    }
}

fn run_cli(a: &InvokeArgs, engine: Engine) -> Result<()> {
    let prepared = prepare(a)?;
    let PreparedCall {
        prompt,
        schema,
        validator,
        input_hashes,
        prompt_hash,
    } = prepared;

    let command_version = detect_version(engine.label());

    let timeout = Duration::from_secs(a.timeout_secs);
    let started_at = now_epoch_tag();
    let outcome = match engine {
        Engine::Claude => call_claude(&prompt, &schema, a.model.as_deref(), timeout),
        Engine::Codex => call_codex(&prompt, &schema, a.model.as_deref(), timeout),
    }?;
    let finished_at = now_epoch_tag();

    let stdout_path = a.out.join("stdout.raw");
    let stderr_path = a.out.join("stderr.log");
    let result_path = a.out.join("result.json");

    let call = match outcome {
        Err(NotInstalled) => {
            std::fs::write(&stdout_path, b"")?;
            std::fs::write(
                &stderr_path,
                format!("{} not found on PATH\n", engine.label()),
            )?;
            write_meta(
                a,
                &InvokeMeta {
                    schema: 1,
                    provider: provider_label(engine),
                    command_version: None,
                    model: a.model.clone(),
                    started_at,
                    finished_at,
                    exit_code: None,
                    status: InvokeStatus::BlockedNotInstalled.label(),
                    stdout_path: display(&stdout_path),
                    stderr_path: display(&stderr_path),
                    structured_output_path: None,
                    schema_valid: false,
                    prompt_hash,
                    input_hashes,
                    error_kind: Some("not_installed"),
                },
            )?;
            println!("[o7 invoke] {}: BLOCKED_NOT_INSTALLED", engine.label());
            std::process::exit(1);
        }
        Ok(v) => v,
    };

    std::fs::write(&stdout_path, &call.stdout)?;
    std::fs::write(&stderr_path, &call.stderr)?;
    let combined_lower = format!(
        "{}{}",
        String::from_utf8_lossy(&call.stdout),
        String::from_utf8_lossy(&call.stderr)
    )
    .to_ascii_lowercase();

    let (status, structured, error_kind): (InvokeStatus, Option<serde_json::Value>, Option<&str>) =
        if call.timed_out {
            (InvokeStatus::BlockedTimeout, None, Some("timeout"))
        } else if call.exit_code != Some(0) && is_auth_failure(&combined_lower, engine) {
            (InvokeStatus::BlockedAuth, None, Some("auth"))
        } else if call.exit_code != Some(0) && any_marker(&combined_lower, USAGE_LIMIT_MARKERS) {
            (InvokeStatus::BlockedUsage, None, Some("usage_limit"))
        } else if call.exit_code != Some(0) {
            (InvokeStatus::FailInvalidOutput, None, Some("nonzero_exit"))
        } else {
            classify_extracted(extract_final_json(&call, engine), &validator)
        };

    let structured_output_path = if structured.is_some() {
        Some(result_path.clone())
    } else {
        None
    };
    if let Some(v) = &structured {
        std::fs::write(&result_path, serde_json::to_string_pretty(v)?)?;
    }

    let schema_valid = status == InvokeStatus::Pass;
    write_meta(
        a,
        &InvokeMeta {
            schema: 1,
            provider: provider_label(engine),
            command_version: command_version.clone(),
            model: a.model.clone(),
            started_at,
            finished_at,
            exit_code: call.exit_code,
            status: status.label(),
            stdout_path: display(&stdout_path),
            stderr_path: display(&stderr_path),
            structured_output_path: structured_output_path.as_deref().map(display),
            schema_valid,
            prompt_hash,
            input_hashes,
            error_kind,
        },
    )?;

    println!(
        "[o7 invoke] {}: {} -> {}",
        engine.label(),
        status.label(),
        a.out.display()
    );
    if status != InvokeStatus::Pass {
        std::process::exit(1);
    }
    Ok(())
}

/// The arliai path: one HTTPS POST to the pinned endpoint, then the same
/// artifact/meta contract as the CLI engines. Contract: `docs/o7-invoke.md`,
/// "Arli AI backend".
fn run_arliai(a: &InvokeArgs) -> Result<()> {
    // Fail-closed wire-logging preflight, before ANY other step (run dir,
    // artifacts, connection). ureq logs through the `log` facade and its
    // wire-level TRACE is unredacted; "o7 installs no logger" is a fact
    // about today's binary, not a boundary. If the global max level admits
    // TRACE — because a logger was added to o7 later, or an embedder of
    // this crate installed one — the run degrades to a refusal, never to a
    // Bearer token on a log sink (docs/o7-invoke.md, key handling).
    if log::max_level() >= log::LevelFilter::Trace {
        anyhow::bail!(
            "refusing --engine arliai: the global `log` max level admits TRACE, \
             and HTTP wire tracing could expose the Authorization header -- \
             lower the log level (or remove the logger) to use this backend"
        );
    }

    // Pre-network configuration refusals — plain errors, not run outcomes:
    // nothing is written, no connection is opened (same class as an unknown
    // --capability-profile).
    let Some(model) = a.model.as_deref() else {
        anyhow::bail!(
            "--engine arliai requires --model: Arli AI would otherwise fall back \
             to a provider-selected 'default served model', and 007 refuses that \
             implicit model identity -- the request must carry the explicit \
             requested model (o7 itself pins none, consistent with the CLI engines)"
        );
    };
    // The key lives in one function-local owned value; only a borrowed,
    // trimmed view of it goes to the HTTP layer. The properties that
    // matter: it is never stored in any struct, never formatted into any
    // artifact/error text, and (via `strip_provider_api_keys`) never
    // inherited by a claude/codex subprocess — the fifth security boundary
    // in docs/o7-invoke.md.
    let api_key_raw = std::env::var("ARLIAI_API_KEY").unwrap_or_default();
    // Trimmed once here: a stray trailing newline (an `echo`-built env var)
    // would otherwise ride into the Authorization header as a malformed
    // value and surface as a confusing transport failure.
    let api_key = api_key_raw.trim();
    if api_key.is_empty() {
        anyhow::bail!(
            "ARLIAI_API_KEY is not set (or empty) -- refusing before any artifact \
             is written or any connection is opened"
        );
    }

    let PreparedCall {
        prompt,
        schema,
        validator,
        input_hashes,
        prompt_hash,
    } = prepare(a)?;

    let started_at = now_epoch_tag();
    let outcome = invoke_arliai::call(
        &prompt,
        &strip_dollar_schema(&schema),
        model,
        api_key,
        Duration::from_secs(a.timeout_secs),
    );
    let finished_at = now_epoch_tag();

    let stdout_path = a.out.join("stdout.raw");
    let stderr_path = a.out.join("stderr.log");
    let result_path = a.out.join("result.json");

    // Defense in depth on the one path that still relays provider bytes:
    // a 2xx body is written to `stdout.raw` verbatim, so if the provider
    // echoed the `Authorization` value back, that write would put the key
    // in a run record — the exact P0 of AGENTS.md rule 1. The key is still
    // in hand here, so the verbatim case is checkable and is refused rather
    // than accepted as residual. Transformed echoes (base64, hex, a
    // per-character split) are out of reach of any byte comparison and stay
    // named as residual in docs/o7-invoke.md; this is not a DLP engine.
    let credential_reflected = matches!(
        &outcome,
        ArliOutcome::Http { status, body }
            if (200..300).contains(status) && contains_subslice(body, api_key.as_bytes())
    );

    // Classification first: the run-dir evidence split below needs the
    // `error_kind` so a non-2xx `stderr.log` can name the classification
    // without carrying the provider's diagnostic body.
    let (status, structured, error_kind) = if credential_reflected {
        (
            InvokeStatus::BlockedProvider,
            None,
            Some("credential_reflected"),
        )
    } else {
        classify_arli(&outcome, &validator)
    };

    let (raw_body, stderr_text) = if credential_reflected {
        // No body, no extracted value, and a message that names the
        // condition without quoting either the body or the key.
        (
            &[][..],
            "arliai response contained the request credential verbatim; the body is not \
             persisted and the call is refused (docs/o7-invoke.md, key handling)\n"
                .to_owned(),
        )
    } else {
        arli_run_dir_evidence(&outcome, error_kind, a.timeout_secs)
    };
    std::fs::write(&stdout_path, raw_body)?;
    std::fs::write(&stderr_path, stderr_text)?;

    let structured_output_path = if structured.is_some() {
        Some(result_path.clone())
    } else {
        None
    };
    if let Some(v) = &structured {
        std::fs::write(&result_path, serde_json::to_string_pretty(v)?)?;
    }

    let schema_valid = status == InvokeStatus::Pass;
    write_meta(
        a,
        &InvokeMeta {
            schema: 1,
            provider: "arliai-api",
            command_version: None, // no CLI to probe
            model: a.model.clone(),
            started_at,
            finished_at,
            exit_code: None, // no subprocess
            status: status.label(),
            stdout_path: display(&stdout_path),
            stderr_path: display(&stderr_path),
            structured_output_path: structured_output_path.as_deref().map(display),
            schema_valid,
            prompt_hash,
            input_hashes,
            error_kind,
        },
    )?;

    println!(
        "[o7 invoke] arliai: {} -> {}",
        status.label(),
        a.out.display()
    );
    if status != InvokeStatus::Pass {
        std::process::exit(1);
    }
    Ok(())
}

/// Exact byte-substring search — the containment check that keeps a
/// verbatim credential echo out of `stdout.raw`.
///
/// Deliberately the least clever thing that closes the realistic case: no
/// normalization, no decoding, no entropy heuristics. An empty needle
/// returns `false` rather than matching everything, so a misconfiguration
/// upstream of this call can never turn every response into a refusal.
fn contains_subslice(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() || needle.len() > haystack.len() {
        return false;
    }
    haystack.windows(needle.len()).any(|w| w == needle)
}

/// What one arliai outcome contributes to the run dir: the `stdout.raw`
/// bytes and the `stderr.log` text. Pure over the outcome so the split is
/// unit-testable without a socket or a run dir.
///
/// The load-bearing rule is the `docs/o7-invoke.md` amendment, "a non-2xx
/// body is not canonical run evidence":
///
/// ```text
/// 2xx      stdout.raw = the exact provider body, byte-for-byte
/// non-2xx  stdout.raw is empty; the status and its classification go to
///          stderr.log, the diagnostic body is not persisted anywhere
/// ```
///
/// A non-2xx body is the provider's diagnostic channel, and diagnostics are
/// where a server echoes request context back at the caller — this endpoint
/// is on record doing exactly that (the `400` echoing
/// `body.response_format…json_schema.name`). This backend sends
/// `Authorization` on every call, and `AGENTS.md` rule 1 counts writing
/// provider output into a run record without considering what it may have
/// echoed as a P0. Nothing needed that body either: `classify_arli` decides
/// every non-2xx case from the status alone, so persisting it bought
/// convenience at the price of one more place an echoed credential could
/// land.
fn arli_run_dir_evidence<'a>(
    outcome: &'a ArliOutcome,
    error_kind: Option<&'static str>,
    timeout_secs: u64,
) -> (&'a [u8], String) {
    const EMPTY: &[u8] = &[];
    match outcome {
        // The only path on which a provider body enters the run dir.
        ArliOutcome::Http { status, body } if (200..300).contains(status) => {
            (body.as_slice(), String::new())
        }
        // Non-2xx: status + classification only. The body is deliberately
        // dropped, and no part of it is formatted into this text.
        ArliOutcome::Http { status, .. } => (
            EMPTY,
            format!(
                "arliai responded {status} ({}); the diagnostic body is not canonical run \
                 evidence and is not persisted (docs/o7-invoke.md)\n",
                error_kind.unwrap_or("unclassified")
            ),
        ),
        ArliOutcome::TimedOut => (
            EMPTY,
            format!("arliai call timed out after {timeout_secs}s\n"),
        ),
        ArliOutcome::Redirect { detail } | ArliOutcome::Transport { detail } => {
            (EMPTY, format!("{detail}\n"))
        }
        ArliOutcome::TooLarge { limit } => (
            EMPTY,
            format!("arliai response body exceeded the {limit}-byte bound\n"),
        ),
    }
}

/// Terminal status for an arliai call — the code form of the normative
/// classification matrix in `docs/o7-invoke.md`. Pure over the outcome +
/// validator so the whole matrix is unit-testable without a socket. The
/// FAIL/BLOCKED boundary is exact: FAIL_* only when the provider claimed
/// success (2xx) and the payload was unusable; every non-2xx means no
/// trustworthy answer was produced and classifies as a BLOCKED_* — an
/// unknown cause does not turn ERROR into FAIL.
fn classify_arli(
    outcome: &ArliOutcome,
    validator: &jsonschema::Validator,
) -> (
    InvokeStatus,
    Option<serde_json::Value>,
    Option<&'static str>,
) {
    match outcome {
        ArliOutcome::TimedOut => (InvokeStatus::BlockedTimeout, None, Some("timeout")),
        ArliOutcome::Redirect { .. } => (InvokeStatus::BlockedProvider, None, Some("redirect")),
        ArliOutcome::Transport { .. } => (InvokeStatus::BlockedProvider, None, Some("transport")),
        ArliOutcome::TooLarge { .. } => (
            InvokeStatus::BlockedProvider,
            None,
            Some("response_too_large"),
        ),
        ArliOutcome::Http { status, body } => match *status {
            200..=299 => match invoke_arliai::extract_content(body) {
                // Reuses the CLI paths' Pass/FailSchema split verbatim: a
                // schema-violating value reached the validator, so it is
                // FAIL_SCHEMA with the offending value recorded.
                Ok(v) => classify_extracted(Some(v), validator),
                Err(e) => (InvokeStatus::FailInvalidOutput, None, Some(e.error_kind())),
            },
            // Every non-2xx arm below is a BLOCKED_*: the provider did not
            // claim success, so no model output existed and there is nothing
            // to FAIL on. FAIL_* is reserved for the 2xx path above, where
            // the provider claimed success but the payload was unusable
            // (docs/o7-invoke.md, "The FAIL/BLOCKED boundary").
            300..=399 => (InvokeStatus::BlockedProvider, None, Some("redirect")),
            401 | 403 => (InvokeStatus::BlockedAuth, None, Some("auth")),
            402 | 429 => (InvokeStatus::BlockedUsage, None, Some("usage_limit")),
            408 => (InvokeStatus::BlockedTimeout, None, Some("timeout")),
            400..=499 => (
                InvokeStatus::BlockedProvider,
                None,
                Some("http_request_rejected"),
            ),
            500..=599 => (InvokeStatus::BlockedProvider, None, Some("http_5xx")),
            _ => (
                InvokeStatus::BlockedProvider,
                None,
                Some("http_status_unclassified"),
            ),
        },
    }
}

fn provider_label(engine: Engine) -> &'static str {
    match engine {
        Engine::Claude => "claude-cli",
        Engine::Codex => "codex-cli",
    }
}

fn write_meta(a: &InvokeArgs, meta: &InvokeMeta) -> Result<()> {
    std::fs::write(a.out.join("meta.json"), serde_json::to_string_pretty(meta)?)
        .with_context(|| format!("writing {}/meta.json", a.out.display()))
}

fn display(p: &Path) -> String {
    p.display().to_string()
}

/// The run dir must be absent or an existing EMPTY directory. A non-empty
/// `--out` is refused up front — this module writes only the files a given
/// outcome produces (a FAIL leaves no `result.json`), so a stale `result.json`
/// from a previous PASS reused into a later FAILED run would masquerade as that
/// run's output. No selective per-file cleanup: the whole dir is required
/// clean, which one `read_dir` verifies. Refusal happens before the version
/// probe or any backend spawn (see `run`).
fn ensure_empty_out(out: &Path) -> Result<()> {
    match std::fs::read_dir(out) {
        Ok(mut entries) => {
            if entries.next().is_some() {
                anyhow::bail!(
                    "--out {} is not empty; refusing to run into a dir that may already \
                     hold a previous run's result.json/meta.json -- use a fresh or empty dir",
                    out.display()
                );
            }
            Ok(())
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => std::fs::create_dir_all(out)
            .with_context(|| format!("creating run dir {}", out.display())),
        Err(e) => Err(e).with_context(|| format!("inspecting run dir {}", out.display())),
    }
}

/// NOT RFC3339 -- deliberately `"epoch:<seconds>"` instead. No chrono
/// dependency for one timestamp: seconds-since-epoch is exact, sortable, and
/// unambiguous; a caller that wants a `datetime` (Demand Radar's
/// `O7InvokeRunner` does — `AgentResult.started_at`/`finished_at` are typed
/// `datetime`) parses the epoch integer after the `epoch:` tag itself,
/// rather than this module claiming an RFC3339 format it doesn't produce.
fn now_epoch_tag() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    format!("epoch:{secs}")
}

/// Strip provider API keys from the child's environment before it spawns.
/// Neither `claude` nor `codex` needs one here (both auth via their own
/// subscription login state, external to this process); a key present in
/// the parent environment for an unrelated reason must never silently
/// substitute for that subscription auth. Applied to both engines
/// regardless of which is being called, since `o7 invoke` is a shared
/// primitive, not two independent code paths that could drift.
/// `ARLIAI_API_KEY` is stripped too: it IS a real credential for the arliai
/// backend (which spawns no children), and precisely for that reason must
/// never leak into a claude/codex subprocess's environment.
fn strip_provider_api_keys(cmd: &mut Command) {
    for key in [
        "ANTHROPIC_API_KEY",
        "CLAUDE_API_KEY",
        "OPENAI_API_KEY",
        "CODEX_API_KEY",
        "ARLIAI_API_KEY",
    ] {
        cmd.env_remove(key);
    }
}

/// Best-effort `<binary> --version`, mirroring Demand Radar's own
/// `_detect_version` so the conformance gate's meta.json comparison isn't
/// comparing a populated field on one side against always-null on the other.
/// `None` on any failure (including "not installed" -- `run` already
/// classifies that case from the real call, not from this probe).
///
/// Provider API keys are stripped here too (`strip_provider_api_keys`): the
/// probe is a provider subprocess like any other, so the docs' claim "keys
/// stripped before every provider subprocess" stays literally true rather than
/// true-for-the-call-but-not-the-probe. Bounded by `VERSION_PROBE_TIMEOUT` so a
/// hung `--version` degrades to `None` instead of stalling the whole invoke;
/// `stdin` is closed and `stderr` discarded so neither can block the probe.
fn detect_version(binary: &str) -> Option<String> {
    let mut cmd = Command::new(binary);
    cmd.arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    strip_provider_api_keys(&mut cmd);
    let mut child = cmd.spawn().ok()?;

    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break Some(s),
            Ok(None) => {}
            Err(_) => return None,
        }
        if start.elapsed() >= VERSION_PROBE_TIMEOUT {
            break None;
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    if status.is_none() {
        let _ = child.kill();
        let _ = child.wait();
        return None;
    }

    // `--version` output is tiny, so reading after exit cannot deadlock on a
    // full pipe buffer the way a chatty backend would.
    let mut buf = String::new();
    child.stdout.take()?.read_to_string(&mut buf).ok()?;
    let text = buf.trim().to_string();
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

fn sha256_hex_bytes(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    let hex: String = h.finalize().iter().map(|b| format!("{b:02x}")).collect();
    format!("sha256:{hex}")
}

fn sha256_hex_text(text: &str) -> String {
    sha256_hex_bytes(text.as_bytes())
}

fn sha256_hex_file(path: &Path) -> Result<String> {
    let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    Ok(sha256_hex_bytes(&bytes))
}

/// Drop a top-level `$schema` meta-key before handing a schema to `claude
/// --json-schema`. Verified live (this environment, `claude` v2.1.210):
/// passing a schema that declares `$schema` fails every time with `Error:
/// --json-schema is not a valid JSON Schema: no schema with key or ref
/// "https://json-schema.org/draft/2020-12/schema"`; the same schema with
/// only `$schema` removed succeeds, and `$id` alone does not trigger it —
/// see `docs/o7-invoke.md`. Applied only to the copy sent to `claude`; the
/// schema file on disk and the `Validator` built from it above both keep
/// `$schema` (harmless there — only claude's own flag parser rejects it).
fn strip_dollar_schema(schema: &serde_json::Value) -> serde_json::Value {
    let mut v = schema.clone();
    if let Some(obj) = v.as_object_mut() {
        obj.remove("$schema");
    }
    v
}

/// Engine-agnostic auth-failure phrases. Deliberately specific: bare "login"
/// and bare "please run" were REMOVED — both fire on unrelated diagnostics
/// ("failed to login to the database", "please run cargo test") and a
/// false BLOCKED_AUTH hides the real error. What stays here reads as an
/// auth problem in isolation.
const AUTH_MARKERS_SHARED: &[&str] = &[
    "not logged in",
    "please log in",
    "no active session",
    "unauthorized",
    "authentication",
];
/// Claude-specific auth phrases. `"claude login"` also covers the longer
/// "please run `claude login`" the CLI prints (substring match).
const AUTH_MARKERS_CLAUDE: &[&str] = &["claude login", "/login"];
/// Codex-specific auth phrases.
const AUTH_MARKERS_CODEX: &[&str] = &["codex login"];

/// Is this (already lowercased) combined stdout+stderr an auth failure for
/// `engine`? Shared phrases plus that engine's own — never the other engine's
/// (a codex-login hint in a claude run is not a claude auth failure).
fn is_auth_failure(haystack: &str, engine: Engine) -> bool {
    let engine_markers = match engine {
        Engine::Claude => AUTH_MARKERS_CLAUDE,
        Engine::Codex => AUTH_MARKERS_CODEX,
    };
    any_marker(haystack, AUTH_MARKERS_SHARED) || any_marker(haystack, engine_markers)
}
const USAGE_LIMIT_MARKERS: &[&str] = &[
    "usage limit",
    "rate limit",
    "quota",
    "exceeded your",
    "upgrade your plan",
    "resets at",
];

fn any_marker(haystack: &str, markers: &[&str]) -> bool {
    markers.iter().any(|m| haystack.contains(m))
}

/// One subprocess call's raw outcome — engine-agnostic. `stdout` is empty
/// when the caller configured `Stdio::null()` for it (the codex path: the
/// real answer comes from a side-channel file, not stdout — see
/// `call_codex`).
struct RawCall {
    timed_out: bool,
    exit_code: Option<i32>,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

/// Marker error: the backend binary isn't on PATH. A distinct type (not a
/// plain `anyhow::Error` string) so `run` can match it precisely instead of
/// re-parsing an error message to decide BLOCKED_NOT_INSTALLED.
struct NotInstalled;

impl std::fmt::Debug for NotInstalled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "backend binary not found on PATH")
    }
}
impl std::fmt::Display for NotInstalled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "backend binary not found on PATH")
    }
}
impl std::error::Error for NotInstalled {}

/// Spawn `cmd`, write `prompt` to its stdin, drain whichever of stdout/
/// stderr the caller configured as `Stdio::piped()` on dedicated threads
/// (skipped for a stream set to `Stdio::null()`), and poll for completion
/// with a hard timeout. Draining on separate threads — not reading after
/// `wait()` — is load-bearing: a chatty child can fill an OS pipe buffer
/// before exiting, and an un-drained pipe would deadlock a poll loop that
/// only calls `try_wait()`.
fn spawn_with_timeout(
    mut cmd: Command,
    prompt: &str,
    timeout: Duration,
) -> Result<std::result::Result<RawCall, NotInstalled>> {
    cmd.stdin(Stdio::piped());
    // Put the backend in its own process group (leader = the child itself) so
    // the timeout path can SIGKILL the WHOLE group — child plus any descendant
    // it spawned — not just the direct child. A descendant that inherited and
    // still holds the stdout/stderr pipe would otherwise keep the reader
    // threads blocked on `read_to_end` after we killed only the parent, so the
    // join below (and thus the "timeout") would hang forever. Unix-only; 007
    // runs on WSL2/Linux and a Windows Job Object equivalent is out of scope.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Err(NotInstalled)),
        Err(e) => return Err(e).context("spawning backend process"),
    };

    let mut stdin = child.stdin.take().context("child stdin unavailable")?;
    let prompt_owned = prompt.to_string();
    let stdin_writer = std::thread::spawn(move || {
        let res = stdin.write_all(prompt_owned.as_bytes());
        drop(stdin);
        res
    });

    let stdout_reader = child.stdout.take().map(|mut pipe| {
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = pipe.read_to_end(&mut buf);
            buf
        })
    });
    let stderr_reader = child.stderr.take().map(|mut pipe| {
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = pipe.read_to_end(&mut buf);
            buf
        })
    });

    let start = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().context("polling child")? {
            break Some(status);
        }
        if start.elapsed() >= timeout {
            break None;
        }
        std::thread::sleep(Duration::from_millis(50));
    };

    let timed_out = status.is_none();
    if timed_out {
        // Kill the whole group BEFORE joining the reader/writer threads: on
        // unix, SIGKILL the process group (pgid == child pid, from
        // process_group(0) above) so a pipe-holding descendant dies too and the
        // readers see EOF. `child.kill()` alone would leave that descendant
        // holding the pipe and hang the joins. `nix::killpg` wraps the syscall
        // safely (the tree forbids `unsafe`). The direct-child kill still runs
        // as a belt-and-braces reap (and is the only step on non-unix).
        #[cfg(unix)]
        {
            use nix::sys::signal::{killpg, Signal};
            use nix::unistd::Pid;
            let _ = killpg(Pid::from_raw(child.id() as i32), Signal::SIGKILL);
        }
        let _ = child.kill();
        let _ = child.wait();
    }
    let _ = stdin_writer.join();
    let stdout = stdout_reader
        .map(|h| h.join().unwrap_or_default())
        .unwrap_or_default();
    let stderr = stderr_reader
        .map(|h| h.join().unwrap_or_default())
        .unwrap_or_default();

    Ok(Ok(RawCall {
        timed_out,
        exit_code: status.and_then(|s| s.code()),
        stdout,
        stderr,
    }))
}

/// Read-only claude call. `--tools ""` + `--strict-mcp-config` disables
/// every built-in tool and refuses any ambient MCP server (closed world —
/// mirrors `judge.rs::call_claude`'s proven rationale exactly); `--setting-
/// sources ""` additionally refuses any ambient project CLAUDE.md/hooks,
/// which `judge`'s narrower per-file call never needed to consider but this
/// more general, arbitrary-caller primitive should; `--permission-mode
/// default` so a `bypassPermissions` ambient default is never silently
/// inherited (with no tools there is nothing to prompt for, so `default`
/// never hangs); `--max-budget-usd` bounds a single call's spend.
/// `--json-schema` gets the `$schema`-stripped copy (see
/// `strip_dollar_schema`); the *un*-stripped schema still drives this
/// module's own independent re-validation in `run`.
fn call_claude(
    prompt: &str,
    schema: &serde_json::Value,
    model: Option<&str>,
    timeout: Duration,
) -> Result<std::result::Result<RawCall, NotInstalled>> {
    let schema_for_cli = serde_json::to_string(&strip_dollar_schema(schema))
        .context("serializing stripped schema for --json-schema")?;
    let mut cmd = Command::new("claude");
    strip_provider_api_keys(&mut cmd);
    cmd.arg("-p")
        .arg("--output-format")
        .arg("json")
        .arg("--input-format")
        .arg("text")
        .arg("--json-schema")
        .arg(&schema_for_cli)
        .arg("--tools")
        .arg("")
        .arg("--strict-mcp-config")
        .arg("--setting-sources")
        .arg("")
        .arg("--permission-mode")
        .arg("default")
        .arg("--no-session-persistence")
        .arg("--max-budget-usd")
        .arg("0.50")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(m) = model {
        cmd.arg("--model").arg(m);
    }
    spawn_with_timeout(cmd, prompt, timeout)
}

/// Build the read-only `codex exec` argv, WITHOUT spawning — so an argv test
/// can assert the isolation flags and cwd without a live binary. Base flags
/// match `judge.rs::call_codex`'s proven set (`--sandbox read-only`,
/// `--skip-git-repo-check`, `--ephemeral`, `--color never`,
/// `--output-last-message <file>`, stdout discarded), plus the ambient-context
/// isolation this general primitive needs and `judge`'s narrower call did not:
///
/// - `-c features.shell_tool=false` — defense in depth (see the caveat below);
/// - `--ignore-user-config` — refuse the user-level `~/.codex/config.toml`;
/// - `--ignore-rules` — refuse ambient project/user rule files;
/// - `current_dir(cwd)` — the caller sets a FRESH EMPTY temp dir, so codex
///   cannot discover a project `.codex/config.toml` / `AGENTS.md` by walking up
///   from wherever `o7` happened to be invoked.
///
/// Together these are the codex-side analogue of claude's `--setting-sources
/// ""` + `--strict-mcp-config`: no ambient user/project context leaks into a
/// closed-world call.
///
/// **Caveat, unchanged:** neither `--sandbox read-only` nor
/// `features.shell_tool=false` is verified against a live `codex` install
/// (none in this build environment). `--sandbox read-only` denies writes but
/// does not disable network, and whether `features.shell_tool=false` actually
/// removes the shell tool (vs. restricting it inside the sandbox) has never
/// been observed. Docs/callers must **not** describe codex's posture as "no
/// shell" the way claude's `--tools ""` earns that claim structurally (see
/// `docs/o7-invoke.md`); a caller processing untrusted external content must
/// not select `--engine codex` until this is live-verified. Unlike claude, no
/// `--json-schema`-equivalent is assumed; the schema is appended to the prompt
/// and `run`'s independent `jsonschema` validation is what enforces it.
fn codex_command(model: Option<&str>, cwd: &Path, last_msg: &Path) -> Command {
    let mut cmd = Command::new("codex");
    strip_provider_api_keys(&mut cmd);
    cmd.current_dir(cwd)
        .arg("exec")
        .arg("--ignore-user-config")
        .arg("--ignore-rules")
        .arg("--sandbox")
        .arg("read-only")
        .arg("--skip-git-repo-check")
        .arg("--ephemeral")
        .arg("--color")
        .arg("never")
        .arg("-c")
        .arg("features.shell_tool=false")
        .arg("--output-last-message")
        .arg(last_msg)
        .arg("-")
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    if let Some(m) = model {
        cmd.arg("--model").arg(m);
    }
    cmd
}

fn call_codex(
    prompt: &str,
    schema: &serde_json::Value,
    model: Option<&str>,
    timeout: Duration,
) -> Result<std::result::Result<RawCall, NotInstalled>> {
    let augmented = format!(
        "{prompt}\n\n---\nRespond with EXACTLY one JSON object and nothing else -- no prose, \
         no markdown code fences, no explanation before or after. It must validate against \
         this JSON Schema:\n{}\n",
        serde_json::to_string_pretty(schema).unwrap_or_default()
    );

    // A fresh, EMPTY per-call working directory: codex is launched from here so
    // it cannot inherit a project `.codex/config.toml` / `AGENTS.md` / other
    // cwd-context. Removed unconditionally after the call (it also holds the
    // `--output-last-message` side-channel file).
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let cwd = std::env::temp_dir().join(format!(
        "o7-invoke-codex-cwd-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&cwd)
        .with_context(|| format!("creating codex isolated cwd {}", cwd.display()))?;
    let last_msg = cwd.join("last-message.txt");

    let cmd = codex_command(model, &cwd, &last_msg);
    let outcome = spawn_with_timeout(cmd, &augmented, timeout);

    // The answer lives in `last_msg`, never in stdout (judge.rs's own
    // rationale: codex's stdout carries a session preamble whose stray `[` or
    // `{` could fool a bracket-slicing extractor into a bogus "answer"). Remove
    // the isolated dir (side-channel file included) whatever the outcome.
    let result = match outcome {
        Ok(Ok(mut call)) => {
            if !call.timed_out {
                call.stdout = std::fs::read_to_string(&last_msg)
                    .unwrap_or_default()
                    .into_bytes();
            }
            Ok(Ok(call))
        }
        other => other,
    };
    let _ = std::fs::remove_dir_all(&cwd);
    result
}

/// Parse the text claude carries in its `--output-format json` `result`
/// field, accepting EXACTLY the two shapes observed in the wild and nothing
/// else:
///
/// 1. a **bare JSON value** after surrounding whitespace is trimmed (claude
///    2.1.210); or
/// 2. exactly one ```` ```json ```` fenced block spanning the WHOLE trimmed
///    payload — literally ```` ```json\n<JSON>\n``` ```` (claude 2.1.162's
///    shape even with `--json-schema`).
///
/// The grammar is deliberately not widened past that: only the literal
/// lowercase `json` tag with a newline is a fence here — an absent tag, a
/// different tag (`text`, `JSON`, `json extra`), prose outside the one block, a
/// second block, or unbalanced fences all yield `None` (→ FAIL_INVALID_OUTPUT).
/// `call_claude`'s argv is unchanged; this only relaxes how its `result` text
/// is read. Never panics: `trim`/`strip_prefix`/`strip_suffix`/`contains` plus
/// `serde_json` (whose `from_str` itself rejects a payload with trailing
/// content, so two JSON values in one fence do not parse).
fn parse_claude_result_payload(result_text: &str) -> Option<serde_json::Value> {
    let trimmed = result_text.trim();
    // Shape 1: bare JSON value.
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(trimmed) {
        return Some(v);
    }
    // Shape 2: one full ```json block, and only that.
    let body = trimmed.strip_prefix("```json\n")?.strip_suffix("```")?;
    if body.contains("```") {
        return None; // a second fence -> not a single block
    }
    serde_json::from_str(body.trim()).ok()
}

/// Terminal status for a clean call (exit 0, not timed out) from its extracted
/// JSON. Split out of `run` so the distinction is unit-testable: a value that
/// is syntactically valid JSON but violates the schema is **FAIL_SCHEMA** (it
/// reached the validator), never FAIL_INVALID_OUTPUT (which is only for output
/// that did not yield a JSON value at all).
fn classify_extracted(
    extracted: Option<serde_json::Value>,
    validator: &jsonschema::Validator,
) -> (
    InvokeStatus,
    Option<serde_json::Value>,
    Option<&'static str>,
) {
    match extracted {
        None => (InvokeStatus::FailInvalidOutput, None, Some("invalid_json")),
        Some(v) if validator.is_valid(&v) => (InvokeStatus::Pass, Some(v), None),
        Some(v) => (InvokeStatus::FailSchema, Some(v), Some("schema_violation")),
    }
}

/// Turn one call's raw bytes into a JSON `Value` to schema-check, per
/// engine's own envelope shape. Never panics: `find`/`rfind`/`strip_*` on
/// ASCII delimiters (always char-boundary-safe) plus `serde_json::from_str`.
fn extract_final_json(call: &RawCall, engine: Engine) -> Option<serde_json::Value> {
    match engine {
        Engine::Claude => {
            // `--output-format json` envelope: {"result": "<json-encoded text>", ...}.
            let stdout = String::from_utf8_lossy(&call.stdout);
            let envelope: serde_json::Value = serde_json::from_str(&stdout).ok()?;
            let result_text = envelope.get("result")?.as_str()?;
            parse_claude_result_payload(result_text)
        }
        Engine::Codex => {
            let text = String::from_utf8_lossy(&call.stdout);
            let trimmed = text.trim();
            if trimmed.is_empty() {
                return None;
            }
            // Fast path: the model behaved and returned bare JSON.
            if let Ok(v) = serde_json::from_str(trimmed) {
                return Some(v);
            }
            // Fallback: tolerate ```json fences / stray prose around the
            // object, mirroring judge.rs::extract_json_array's approach for
            // the array case (see its Kani proof for why this never panics).
            let start = text.find('{')?;
            let end = text.rfind('}')?;
            if end <= start {
                return None;
            }
            serde_json::from_str(&text[start..=end]).ok()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_dollar_schema_removes_only_that_key() {
        let schema = serde_json::json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "$id": "https://example.test/x",
            "type": "object"
        });
        let stripped = strip_dollar_schema(&schema);
        assert_eq!(
            stripped,
            serde_json::json!({"$id": "https://example.test/x", "type": "object"})
        );
    }

    #[test]
    fn strip_dollar_schema_is_noop_without_the_key() {
        let schema = serde_json::json!({"type": "object"});
        assert_eq!(strip_dollar_schema(&schema), schema);
    }

    #[test]
    fn sha256_hex_text_is_stable_and_prefixed() {
        let a = sha256_hex_text("hello");
        let b = sha256_hex_text("hello");
        assert_eq!(a, b);
        assert!(a.starts_with("sha256:"));
        assert_eq!(a.len(), "sha256:".len() + 64);
    }

    #[test]
    fn extract_final_json_claude_envelope() {
        let call = RawCall {
            timed_out: false,
            exit_code: Some(0),
            stdout: br#"{"result": "{\"acknowledged\": true}", "session_id": "x"}"#.to_vec(),
            stderr: Vec::new(),
        };
        let parsed = extract_final_json(&call, Engine::Claude);
        assert_eq!(parsed, Some(serde_json::json!({"acknowledged": true})));
    }

    #[test]
    fn extract_final_json_claude_envelope_fenced_result() {
        // claude 2.1.162: the `result` field is a ```json fenced block, not
        // bare JSON. The envelope still parses; the strict payload parser peels
        // the single full-payload fence.
        let inner = "```json\n{\"ok\": true}\n```";
        let stdout = serde_json::to_vec(&serde_json::json!({"result": inner, "session_id": "x"}))
            .unwrap_or_default();
        let call = RawCall {
            timed_out: false,
            exit_code: Some(0),
            stdout,
            stderr: Vec::new(),
        };
        assert_eq!(
            extract_final_json(&call, Engine::Claude),
            Some(serde_json::json!({"ok": true}))
        );
    }

    #[test]
    fn claude_result_payload_accepts_only_bare_or_json_fence() {
        // Shape 1: bare JSON (2.1.210).
        assert_eq!(
            parse_claude_result_payload("  {\"ok\": true}  "),
            Some(serde_json::json!({"ok": true}))
        );
        // Shape 2: one full ```json block (2.1.162) -- the ONLY accepted fence.
        assert_eq!(
            parse_claude_result_payload("```json\n{\"ok\": true}\n```"),
            Some(serde_json::json!({"ok": true}))
        );
    }

    #[test]
    fn claude_result_payload_rejects_anything_but_the_two_shapes() {
        // Wrong language tag.
        assert!(parse_claude_result_payload("```text\n{\"ok\": true}\n```").is_none());
        // Absent language tag (an untagged fence is NOT accepted).
        assert!(parse_claude_result_payload("```\n{\"ok\": true}\n```").is_none());
        // Uppercase tag -- only lowercase `json`.
        assert!(parse_claude_result_payload("```JSON\n{\"ok\": true}\n```").is_none());
        // Extra text after the tag on the fence line.
        assert!(parse_claude_result_payload("```json extra\n{\"ok\": true}\n```").is_none());
        // Prose before the fence.
        assert!(parse_claude_result_payload("sure:\n```json\n{\"ok\": true}\n```").is_none());
        // Prose after the closing fence.
        assert!(
            parse_claude_result_payload("```json\n{\"ok\": true}\n```\nhope that helps").is_none()
        );
        // Unclosed fence.
        assert!(parse_claude_result_payload("```json\n{\"ok\": true}").is_none());
        // Two JSON values inside one fence (serde rejects trailing content).
        assert!(
            parse_claude_result_payload("```json\n{\"ok\": true}\n{\"ok\": false}\n```").is_none()
        );
        // Two fenced blocks.
        assert!(parse_claude_result_payload("```json\n{}\n```\n```json\n{}\n```").is_none());
        // Fenced, but the inner content is not valid JSON.
        assert!(parse_claude_result_payload("```json\nnot json at all\n```").is_none());
        // Bare-ish but with trailing prose -> not a bare JSON value.
        assert!(parse_claude_result_payload("{\"ok\": true} then some words").is_none());
    }

    #[test]
    fn schema_invalid_object_is_fail_schema_not_invalid_output() {
        // A value that is valid JSON syntax but violates the schema must be
        // FAIL_SCHEMA (it reached the validator), never FAIL_INVALID_OUTPUT.
        let schema = serde_json::json!({
            "type": "object",
            "properties": {"ok": {"type": "boolean"}},
            "required": ["ok"],
            "additionalProperties": false
        });
        let validator = match jsonschema::validator_for(&schema) {
            Ok(v) => v,
            Err(_) => return, // schema build failure is environmental, not under test
        };

        // {"ok": 123}: parses as JSON, violates {ok: boolean} -> FAIL_SCHEMA.
        let wrong_type = serde_json::json!({"ok": 123});
        let (status, structured, error_kind) =
            classify_extracted(Some(wrong_type.clone()), &validator);
        assert_eq!(status, InvokeStatus::FailSchema);
        assert_eq!(error_kind, Some("schema_violation"));
        assert_eq!(structured, Some(wrong_type)); // the offending value is still recorded

        // A schema-valid value -> PASS.
        let (ok_status, _, ok_kind) =
            classify_extracted(Some(serde_json::json!({"ok": true})), &validator);
        assert_eq!(ok_status, InvokeStatus::Pass);
        assert_eq!(ok_kind, None);

        // No value extracted at all -> FAIL_INVALID_OUTPUT (the other branch).
        let (none_status, _, none_kind) = classify_extracted(None, &validator);
        assert_eq!(none_status, InvokeStatus::FailInvalidOutput);
        assert_eq!(none_kind, Some("invalid_json"));
    }

    #[test]
    fn extract_final_json_codex_bare() {
        let call = RawCall {
            timed_out: false,
            exit_code: Some(0),
            stdout: br#"{"acknowledged": true}"#.to_vec(),
            stderr: Vec::new(),
        };
        let parsed = extract_final_json(&call, Engine::Codex);
        assert_eq!(parsed, Some(serde_json::json!({"acknowledged": true})));
    }

    #[test]
    fn extract_final_json_codex_tolerates_fence_and_prose() {
        let call = RawCall {
            timed_out: false,
            exit_code: Some(0),
            stdout: b"sure, here you go:\n```json\n{\"acknowledged\": true}\n```\nhope that helps!"
                .to_vec(),
            stderr: Vec::new(),
        };
        let parsed = extract_final_json(&call, Engine::Codex);
        assert_eq!(parsed, Some(serde_json::json!({"acknowledged": true})));
    }

    #[test]
    fn extract_final_json_empty_codex_output_is_none() {
        let call = RawCall {
            timed_out: false,
            exit_code: Some(0),
            stdout: Vec::new(),
            stderr: Vec::new(),
        };
        assert!(extract_final_json(&call, Engine::Codex).is_none());
    }

    #[test]
    fn usage_markers_match_case_folded_input() {
        assert!(any_marker(
            "you have hit your usage limit",
            USAGE_LIMIT_MARKERS
        ));
        assert!(!any_marker("everything is fine", USAGE_LIMIT_MARKERS));
    }

    #[test]
    fn real_auth_markers_still_classify() {
        // The specific phrases each engine actually prints on an auth failure.
        assert!(is_auth_failure(
            "please run `claude login` first",
            Engine::Claude
        ));
        assert!(is_auth_failure("error: not logged in", Engine::Claude));
        assert!(is_auth_failure(
            "run `codex login` to authenticate",
            Engine::Codex
        ));
        assert!(is_auth_failure("no active session", Engine::Codex));
    }

    #[test]
    fn unrelated_login_and_please_run_text_is_not_auth_failure() {
        // Negative controls: bare "login" / "please run" were removed precisely
        // because they fire on ordinary diagnostics that have nothing to do with
        // auth. A false BLOCKED_AUTH here would bury the real error.
        assert!(!is_auth_failure(
            "failed to login to the postgres database at db:5432",
            Engine::Claude
        ));
        assert!(!is_auth_failure(
            "please run cargo test to reproduce this failure",
            Engine::Claude
        ));
        assert!(!is_auth_failure(
            "please run cargo test to reproduce this failure",
            Engine::Codex
        ));
        // Cross-engine: a codex-login hint is not a claude auth failure.
        assert!(!is_auth_failure("hint: try `codex login`", Engine::Claude));
    }

    #[test]
    fn codex_command_is_ambient_isolated() {
        // The argv MUST carry both ambient-config refusals and run from the
        // caller-supplied fresh cwd (change 1). No live codex binary needed.
        let cwd = Path::new("/tmp/o7-invoke-codex-cwd-test");
        let last = cwd.join("last-message.txt");
        let cmd = codex_command(None, cwd, &last);
        let args: Vec<String> = cmd
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert!(
            args.iter().any(|a| a == "--ignore-user-config"),
            "missing --ignore-user-config: {args:?}"
        );
        assert!(
            args.iter().any(|a| a == "--ignore-rules"),
            "missing --ignore-rules: {args:?}"
        );
        // Still closed-world on the sandbox/shell axis.
        assert!(args.iter().any(|a| a == "read-only"));
        assert!(args.iter().any(|a| a == "features.shell_tool=false"));
        // Launched from the isolated cwd, not wherever o7 was invoked.
        assert_eq!(cmd.get_current_dir(), Some(cwd));
    }

    #[test]
    fn ensure_empty_out_contract() {
        // absent -> created; existing-empty -> ok; non-empty -> refused.
        // The non-empty case is the regression: a stale result.json from a
        // previous PASS must NOT be reusable as the next run's dir (change 3).
        let base =
            std::env::temp_dir().join(format!("o7-invoke-out-contract-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);

        let target = base.join("run");
        assert!(
            ensure_empty_out(&target).is_ok(),
            "absent dir must be created"
        );
        assert!(target.is_dir());
        assert!(
            ensure_empty_out(&target).is_ok(),
            "existing empty dir must be accepted"
        );

        assert!(
            std::fs::write(target.join("result.json"), "{}").is_ok(),
            "test setup: writing the stale result.json failed"
        );
        assert!(
            ensure_empty_out(&target).is_err(),
            "a dir holding a stale result.json must be refused"
        );

        let _ = std::fs::remove_dir_all(&base);
    }

    /// The verbatim-credential containment check that guards the one
    /// remaining path where provider bytes reach an artifact.
    #[test]
    fn credential_containment_is_exact_and_never_matches_on_empty() {
        let key = "sk-live-DEADBEEF";
        let echoed = br#"{"choices":[{"message":{"content":"you sent Bearer sk-live-DEADBEEF"}}]}"#;
        assert!(
            contains_subslice(echoed, key.as_bytes()),
            "a verbatim echo must be detected"
        );

        let clean = br#"{"choices":[{"message":{"content":"{\"ok\": true}"}}]}"#;
        assert!(
            !contains_subslice(clean, key.as_bytes()),
            "an ordinary body must not trip the check"
        );

        // An empty needle must not match everything: a misconfiguration
        // upstream would otherwise refuse every single response.
        assert!(!contains_subslice(clean, b""));
        // A needle longer than the body is simply absent, not a panic.
        assert!(!contains_subslice(b"short", key.as_bytes()));
        // Boundary positions still match.
        assert!(contains_subslice(b"sk-live-DEADBEEF tail", key.as_bytes()));
        assert!(contains_subslice(b"head sk-live-DEADBEEF", key.as_bytes()));
        // A transformed echo is out of scope by construction, and the test
        // pins that so nobody mistakes this for a DLP check.
        assert!(
            !contains_subslice(b"c2stbGl2ZS1ERUFEQkVFRg==", key.as_bytes()),
            "base64 is deliberately not detected; see docs/o7-invoke.md"
        );
    }

    /// The run-dir evidence split (docs/o7-invoke.md, "a non-2xx body is
    /// not canonical run evidence"): a 2xx body is persisted byte-for-byte,
    /// every non-2xx body is dropped entirely.
    ///
    /// The negative half is the point. A provider diagnostic is where an
    /// echoed `Authorization` would surface, so these cases assert that no
    /// byte of the diagnostic reaches either artifact — not merely that
    /// `stdout.raw` is empty, since a "helpful" detail string quoting the
    /// body would reopen the same surface through `stderr.log`.
    #[test]
    fn arliai_non_2xx_body_never_enters_the_run_dir() {
        // A body shaped like the leak this rule exists to prevent.
        let secretish =
            b"{\"detail\":\"upstream rejected Authorization: Bearer sk-live-DEADBEEF\"}";

        for (status, kind) in [
            (400u16, Some("http_request_rejected")),
            (401, Some("auth")),
            (403, Some("auth")),
            (429, Some("usage_limit")),
            (500, Some("http_5xx")),
            (599, Some("http_status_unclassified")),
        ] {
            let outcome = ArliOutcome::Http {
                status,
                body: secretish.to_vec(),
            };
            let (raw, stderr) = arli_run_dir_evidence(&outcome, kind, 120);

            assert!(
                raw.is_empty(),
                "status {status}: a non-2xx diagnostic body must not reach stdout.raw"
            );
            assert!(
                !stderr.contains("sk-live-DEADBEEF") && !stderr.contains("Bearer"),
                "status {status}: the diagnostic body must not be quoted into stderr.log \
                 (got: {stderr})"
            );
            assert!(
                stderr.contains(&status.to_string()),
                "status {status}: stderr.log must still name the status"
            );
        }

        // 2xx keeps the exact bytes — this is the path the local schema
        // re-validation judges, so it must stay byte-for-byte.
        let body = b"{\"choices\":[{\"message\":{\"content\":\"{}\"}}]}".to_vec();
        let outcome = ArliOutcome::Http {
            status: 200,
            body: body.clone(),
        };
        let (raw, stderr) = arli_run_dir_evidence(&outcome, None, 120);
        assert_eq!(
            raw,
            body.as_slice(),
            "a 2xx body must be persisted verbatim"
        );
        assert!(stderr.is_empty(), "a 2xx response leaves stderr.log empty");

        // A genuinely empty 2xx body stays distinguishable from "no body":
        // both write nothing, but only the 2xx path is the body channel.
        let empty_2xx = ArliOutcome::Http {
            status: 204,
            body: Vec::new(),
        };
        let (raw, stderr) = arli_run_dir_evidence(&empty_2xx, None, 120);
        assert!(raw.is_empty());
        assert!(stderr.is_empty());
    }

    /// The non-HTTP outcomes are untouched by the amendment: they never had
    /// a body to persist, and each keeps its own bounded `stderr.log` line.
    #[test]
    fn arliai_non_http_outcomes_do_not_regress() {
        let (raw, stderr) = arli_run_dir_evidence(&ArliOutcome::TimedOut, Some("timeout"), 42);
        assert!(raw.is_empty());
        assert!(stderr.contains("42"), "timeout detail names the deadline");

        // `let` bindings, not temporaries: the returned slice borrows the
        // outcome, which is exactly the property that keeps a 2xx body from
        // being copied on its way to disk.
        let transport = ArliOutcome::Transport {
            detail: "dns failure".to_owned(),
        };
        let (raw, stderr) = arli_run_dir_evidence(&transport, Some("transport"), 120);
        assert!(raw.is_empty());
        assert!(stderr.contains("dns failure"));

        let redirect = ArliOutcome::Redirect {
            detail: "refused redirect".to_owned(),
        };
        let (raw, stderr) = arli_run_dir_evidence(&redirect, Some("redirect"), 120);
        assert!(raw.is_empty());
        assert!(stderr.contains("refused redirect"));

        let too_large = ArliOutcome::TooLarge {
            limit: invoke_arliai::MAX_RESPONSE_BYTES,
        };
        let (raw, stderr) = arli_run_dir_evidence(&too_large, Some("response_too_large"), 120);
        assert!(
            raw.is_empty(),
            "an over-limit body is deliberately not kept"
        );
        assert!(stderr.contains(&invoke_arliai::MAX_RESPONSE_BYTES.to_string()));
    }

    /// The full normative classification matrix for the arliai backend
    /// (docs/o7-invoke.md), driven through the pure classifier — no socket.
    #[test]
    fn arliai_classification_matrix() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {"ok": {"type": "boolean"}},
            "required": ["ok"],
            "additionalProperties": false
        });
        let validator = match jsonschema::validator_for(&schema) {
            Ok(v) => v,
            Err(_) => return, // environmental, not under test
        };
        let ok_body = |content: &str| -> Vec<u8> {
            serde_json::json!({
                "choices": [{"message": {"role": "assistant", "content": content}}]
            })
            .to_string()
            .into_bytes()
        };
        let http = |status: u16, body: Vec<u8>| ArliOutcome::Http { status, body };

        // 2xx + valid content + schema-valid -> PASS.
        let (s, v, k) = classify_arli(&http(200, ok_body("{\"ok\": true}")), &validator);
        assert_eq!(
            (s, k),
            (InvokeStatus::Pass, None),
            "200 + schema-valid must PASS"
        );
        assert_eq!(v, Some(serde_json::json!({"ok": true})));

        // 2xx + valid JSON content violating the schema -> FAIL_SCHEMA,
        // offending value recorded.
        let (s, v, k) = classify_arli(&http(200, ok_body("{\"ok\": 123}")), &validator);
        assert_eq!((s, k), (InvokeStatus::FailSchema, Some("schema_violation")));
        assert_eq!(v, Some(serde_json::json!({"ok": 123})));

        // 2xx + non-JSON body -> FAIL_INVALID_OUTPUT / bad_envelope.
        let (s, v, k) = classify_arli(&http(200, b"<html>oops</html>".to_vec()), &validator);
        assert_eq!(
            (s, v, k),
            (InvokeStatus::FailInvalidOutput, None, Some("bad_envelope"))
        );

        // 2xx + empty choices -> FAIL_INVALID_OUTPUT / empty_choices.
        let (s, _, k) = classify_arli(&http(200, br#"{"choices": []}"#.to_vec()), &validator);
        assert_eq!(
            (s, k),
            (InvokeStatus::FailInvalidOutput, Some("empty_choices"))
        );

        // 2xx + null content -> FAIL_INVALID_OUTPUT / null_content.
        let null_content = serde_json::json!({
            "choices": [{"message": {"role": "assistant", "content": null}}]
        })
        .to_string()
        .into_bytes();
        let (s, _, k) = classify_arli(&http(200, null_content), &validator);
        assert_eq!(
            (s, k),
            (InvokeStatus::FailInvalidOutput, Some("null_content"))
        );

        // 2xx + tool_calls present -> FAIL_INVALID_OUTPUT / tool_calls_present.
        let tool_calls = serde_json::json!({
            "choices": [{"message": {"role": "assistant", "content": "{\"ok\": true}",
                "tool_calls": [{"id": "t", "type": "function",
                                "function": {"name": "x", "arguments": "{}"}}]}}]
        })
        .to_string()
        .into_bytes();
        let (s, _, k) = classify_arli(&http(200, tool_calls), &validator);
        assert_eq!(
            (s, k),
            (InvokeStatus::FailInvalidOutput, Some("tool_calls_present"))
        );

        // 2xx + content string that is not JSON -> invalid_json.
        let (s, _, k) = classify_arli(&http(200, ok_body("sure, here it is")), &validator);
        assert_eq!(
            (s, k),
            (InvokeStatus::FailInvalidOutput, Some("invalid_json"))
        );

        // 3xx -> BLOCKED_PROVIDER / redirect (endpoint binding), both via a
        // returned response and via ureq's refused-redirect error path.
        let (s, _, k) = classify_arli(&http(302, Vec::new()), &validator);
        assert_eq!((s, k), (InvokeStatus::BlockedProvider, Some("redirect")));
        let (s, _, k) = classify_arli(
            &ArliOutcome::Redirect {
                detail: "redirect refused".into(),
            },
            &validator,
        );
        assert_eq!((s, k), (InvokeStatus::BlockedProvider, Some("redirect")));

        // 401/403 -> BLOCKED_AUTH; 402/429 -> BLOCKED_USAGE; 408 (server-side
        // request timeout) -> BLOCKED_TIMEOUT.
        for auth_status in [401, 403] {
            let (s, _, k) = classify_arli(&http(auth_status, Vec::new()), &validator);
            assert_eq!((s, k), (InvokeStatus::BlockedAuth, Some("auth")));
        }
        for usage_status in [402, 429] {
            let (s, _, k) = classify_arli(&http(usage_status, Vec::new()), &validator);
            assert_eq!((s, k), (InvokeStatus::BlockedUsage, Some("usage_limit")));
        }
        let (s, _, k) = classify_arli(&http(408, Vec::new()), &validator);
        assert_eq!((s, k), (InvokeStatus::BlockedTimeout, Some("timeout")));

        // 5xx -> BLOCKED_PROVIDER / http_5xx.
        for server_err in [500, 502, 503] {
            let (s, _, k) = classify_arli(&http(server_err, Vec::new()), &validator);
            assert_eq!((s, k), (InvokeStatus::BlockedProvider, Some("http_5xx")));
        }

        // Every remaining 4xx -> BLOCKED_PROVIDER / http_request_rejected:
        // the server states the request was not processed, so no model
        // output existed to FAIL on.
        for rejected in [400, 404, 405, 418, 422, 451] {
            let (s, _, k) = classify_arli(&http(rejected, Vec::new()), &validator);
            assert_eq!(
                (s, k),
                (InvokeStatus::BlockedProvider, Some("http_request_rejected"))
            );
        }

        // Anything else non-2xx (1xx, non-standard codes) -> still BLOCKED,
        // never FAIL: an unknown cause does not turn ERROR into FAIL.
        for other in [100, 101, 999] {
            let (s, _, k) = classify_arli(&http(other, Vec::new()), &validator);
            assert_eq!(
                (s, k),
                (
                    InvokeStatus::BlockedProvider,
                    Some("http_status_unclassified")
                )
            );
        }

        // Transport / timeout / over-limit body.
        let (s, _, k) = classify_arli(
            &ArliOutcome::Transport {
                detail: "connection refused".into(),
            },
            &validator,
        );
        assert_eq!((s, k), (InvokeStatus::BlockedProvider, Some("transport")));
        let (s, _, k) = classify_arli(&ArliOutcome::TimedOut, &validator);
        assert_eq!((s, k), (InvokeStatus::BlockedTimeout, Some("timeout")));
        let (s, _, k) = classify_arli(&ArliOutcome::TooLarge { limit: 64 }, &validator);
        assert_eq!(
            (s, k),
            (InvokeStatus::BlockedProvider, Some("response_too_large"))
        );
    }

    /// Live smoke against the real Arli endpoint, driven through the
    /// PRODUCTION classifier: it must prove the schema-bound structured
    /// output property (`PASS` + the exact normalized value), not merely
    /// that the server can emit braces. Ignored by default; both env vars
    /// are REQUIRED — no fallback model id, because a stale hardcoded
    /// model would make the smoke test the author's memory, not the
    /// protocol. Run:
    /// `ARLIAI_API_KEY=... ARLIAI_SMOKE_MODEL=<exact id> \
    ///    cargo test --lib live_smoke_arliai -- --ignored --nocapture`
    /// On success it prints the evidence line (model id, HTTP status,
    /// SHA-256 of the raw response body) — never the key, never the body.
    #[test]
    #[ignore = "live network + ARLIAI_API_KEY + ARLIAI_SMOKE_MODEL required"]
    fn live_smoke_arliai() {
        let key = std::env::var("ARLIAI_API_KEY").unwrap_or_default();
        assert!(
            !key.trim().is_empty(),
            "live smoke needs ARLIAI_API_KEY set"
        );
        let model = std::env::var("ARLIAI_SMOKE_MODEL").unwrap_or_default();
        assert!(
            !model.trim().is_empty(),
            "live smoke needs ARLIAI_SMOKE_MODEL set to an exact, current model id \
             (deliberately no fallback)"
        );
        let schema = serde_json::json!({
            "type": "object",
            "properties": { "ok": { "type": "boolean" } },
            "required": ["ok"],
            "additionalProperties": false
        });
        let validator = jsonschema::validator_for(&schema);
        assert!(validator.is_ok(), "smoke schema failed to build");
        let Ok(validator) = validator else { return };

        let outcome = invoke_arliai::call(
            "Return exactly this JSON object and nothing else: {\"ok\": true}",
            &strip_dollar_schema(&schema),
            &model,
            &key,
            Duration::from_secs(90),
        );
        if let ArliOutcome::Http { status, body } = &outcome {
            println!(
                "[live-smoke] model={model} http={status} raw_sha256={}",
                sha256_hex_bytes(body)
            );
        }

        let (status, structured, error_kind) = classify_arli(&outcome, &validator);
        assert_eq!(
            status,
            InvokeStatus::Pass,
            "live classification was {} (error_kind: {error_kind:?}), not PASS",
            status.label()
        );
        assert_eq!(
            structured,
            Some(serde_json::json!({"ok": true})),
            "normalized result must be exactly {{\"ok\": true}}"
        );
        println!("[live-smoke] classification=PASS normalized={{\"ok\":true}}");
    }

    #[test]
    fn invoke_backend_parses_all_three_and_refuses_the_rest() {
        assert_eq!(
            "claude".parse::<InvokeBackend>().ok(),
            Some(InvokeBackend::Claude)
        );
        assert_eq!(
            "codex".parse::<InvokeBackend>().ok(),
            Some(InvokeBackend::Codex)
        );
        assert_eq!(
            "arliai".parse::<InvokeBackend>().ok(),
            Some(InvokeBackend::ArliAi)
        );
        assert!("gpt4all".parse::<InvokeBackend>().is_err());
        assert!("ArliAI".parse::<InvokeBackend>().is_err()); // exact, lowercase
    }

    #[test]
    fn blocked_provider_label_matches_docs() {
        assert_eq!(InvokeStatus::BlockedProvider.label(), "BLOCKED_PROVIDER");
    }

    #[cfg(unix)]
    #[test]
    fn timeout_kills_descendants_holding_pipe() {
        // Regression (change 2): the direct child (bash) stays alive on `wait`
        // while a background `sleep` descendant inherits and HOLDS the stdout
        // pipe. Killing only bash would leave `sleep` holding the pipe and hang
        // the stdout reader forever; the process-group SIGKILL must reap the
        // descendant so the call returns (timed_out) in bounded time. A channel
        // + recv_timeout converts any regression-induced hang into a clean
        // failure instead of an infinite test.
        use std::sync::mpsc;
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let mut cmd = Command::new("bash");
            cmd.arg("-c")
                .arg("sleep 300 & wait")
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
            let outcome = spawn_with_timeout(cmd, "", Duration::from_secs(1))
                .map(|inner| inner.map(|c| c.timed_out).map_err(|_| "not_installed"));
            let _ = tx.send(outcome);
        });
        // recv_timeout returning Err (a timeout on OUR side) is the regression
        // signature: spawn_with_timeout hung on the reader join because the
        // descendant kept the pipe open. Assert on a bool so no `panic!` is
        // needed (the tree denies `clippy::panic`, even in tests).
        let returned = rx.recv_timeout(Duration::from_secs(30));
        assert!(
            returned.is_ok(),
            "spawn_with_timeout did not return within 30s -- process-group kill \
             regressed (a descendant kept the stdout pipe open)"
        );
        // When it did return with a real call (bash present), it must be a
        // timeout; an Err inner ("not_installed") means bash is absent on this
        // runner, which leaves nothing to assert.
        if let Ok(Ok(Ok(timed_out))) = returned {
            assert!(timed_out, "expected a timeout");
        }
    }
}
