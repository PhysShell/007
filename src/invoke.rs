//! `o7 invoke` — a narrow, read-only, schema-bound single-shot agent call.
//!
//! `o7 invoke --engine claude|codex --prompt-file <f> --schema <f>
//! --capability-profile read-only-data --out <dir>`. Not a workflow, not a
//! DAG, not a provider framework: one prompt in, one schema-checked JSON
//! object out, closed-world by construction. This is `judge.rs`'s
//! closed-world call pattern generalized to an arbitrary caller-supplied
//! prompt + schema instead of judge's own hardcoded per-file verdict shape —
//! see `docs/o7-invoke.md` for why this exists and what it deliberately does
//! not do. Zero changes to `o7 run` or `o7 judge`'s own domain behavior:
//! this module is additive only.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::agent::Engine;

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
    /// Backend agent CLI: claude | codex.
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InvokeStatus {
    Pass,
    BlockedAuth,
    BlockedUsage,
    BlockedTimeout,
    BlockedNotInstalled,
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
            InvokeStatus::FailInvalidOutput => "FAIL_INVALID_OUTPUT",
            InvokeStatus::FailSchema => "FAIL_SCHEMA",
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

pub fn run(a: &InvokeArgs) -> Result<()> {
    if a.capability_profile != READ_ONLY_DATA_PROFILE {
        anyhow::bail!(
            "unknown --capability-profile '{}' (only '{READ_ONLY_DATA_PROFILE}' is \
             implemented) -- refusing to run rather than silently narrow or widen it",
            a.capability_profile
        );
    }
    let engine: Engine = a.engine.parse()?;

    // Run-dir integrity: the out dir must be absent or an existing EMPTY dir,
    // checked BEFORE the version probe or any backend spawn. A non-empty --out
    // is refused, never partially overwritten -- otherwise a stale result.json
    // from a previous PASS could be mistaken for the output of this run (which,
    // if it FAILs, writes no result.json of its own).
    ensure_empty_out(&a.out)?;

    let command_version = detect_version(engine.label());

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
            match extract_final_json(&call, engine) {
                None => (InvokeStatus::FailInvalidOutput, None, Some("invalid_json")),
                Some(v) => {
                    if validator.is_valid(&v) {
                        (InvokeStatus::Pass, Some(v), None)
                    } else {
                        (InvokeStatus::FailSchema, Some(v), Some("schema_violation"))
                    }
                }
            }
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
fn strip_provider_api_keys(cmd: &mut Command) {
    for key in [
        "ANTHROPIC_API_KEY",
        "CLAUDE_API_KEY",
        "OPENAI_API_KEY",
        "CODEX_API_KEY",
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
/// field. Deliberately STRICT — narrower than the codex fallback's
/// bracket-slice: the trimmed payload must be EITHER a bare JSON value, OR
/// exactly one complete ```-fenced block that occupies the WHOLE trimmed
/// payload (no prose before/after, no second block). claude 2.1.162's `result`
/// comes back fence-wrapped (```json\n{...}\n```) even with `--json-schema`,
/// while 2.1.210 returned bare JSON — both must parse, and nothing looser may.
/// `call_claude`'s argv is unchanged; this only relaxes how its `result` text
/// is read. Never panics: `trim`/`strip_prefix`/`strip_suffix`/`find`/
/// `split_at` on char-boundary-safe ASCII delimiters plus `serde_json`.
fn parse_claude_result_payload(result_text: &str) -> Option<serde_json::Value> {
    let trimmed = result_text.trim();
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(trimmed) {
        return Some(v);
    }
    let inner = strip_single_full_fence(trimmed)?;
    serde_json::from_str(inner).ok()
}

/// If `s` (already trimmed) is EXACTLY one complete fenced block — opens with
/// ``` plus an optional single-token language tag on the first line, and the
/// closing ``` terminates the whole payload — return the inner content. Any
/// text outside the one fence, or a second fence within, yields `None`.
fn strip_single_full_fence(s: &str) -> Option<&str> {
    let after_open = s.strip_prefix("```")?;
    let nl = after_open.find('\n')?;
    let (tag, rest) = after_open.split_at(nl); // `rest` starts with the '\n'
    if tag.contains('`') {
        return None; // a stray ``` inside prose, not a real opening fence
    }
    let body = rest.strip_prefix('\n')?;
    let inner = body.strip_suffix("```")?; // closing fence must end the payload
    if inner.contains("```") {
        return None; // more than one block
    }
    Some(inner.trim())
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
    fn claude_result_payload_accepts_bare_and_one_full_fence_only() {
        // Shape 1: bare JSON (2.1.210).
        assert_eq!(
            parse_claude_result_payload("  {\"ok\": true}  "),
            Some(serde_json::json!({"ok": true}))
        );
        // Shape 2a: one full fenced block WITH a language tag (2.1.162).
        assert_eq!(
            parse_claude_result_payload("```json\n{\"ok\": true}\n```"),
            Some(serde_json::json!({"ok": true}))
        );
        // Shape 2b: one full fenced block WITHOUT a language tag.
        assert_eq!(
            parse_claude_result_payload("```\n{\"ok\": true}\n```"),
            Some(serde_json::json!({"ok": true}))
        );
    }

    #[test]
    fn claude_result_payload_rejects_anything_looser() {
        // Prose before the fence -> not "the entire payload".
        assert!(parse_claude_result_payload("sure:\n```json\n{\"ok\": true}\n```").is_none());
        // Prose after the closing fence.
        assert!(
            parse_claude_result_payload("```json\n{\"ok\": true}\n```\nhope that helps").is_none()
        );
        // Two fenced blocks.
        assert!(parse_claude_result_payload("```json\n{}\n```\n```json\n{}\n```").is_none());
        // Fenced, but the inner content is not valid JSON.
        assert!(parse_claude_result_payload("```json\nnot json at all\n```").is_none());
        // Bare-ish but with trailing prose -> not a bare JSON value.
        assert!(parse_claude_result_payload("{\"ok\": true} then some words").is_none());
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
