//! Q-Deck R1 (`docs/q-deck/r1-command.md`): process-level acceptance for the
//! first real multi-turn Command vertical. REAL compiled `o7`/`o7d` binaries,
//! a REAL SQLite ledger, REAL REST/SSE over a REAL `o7d` process — the only
//! stand-in is the external `claude` CLI, replaced by a deterministic
//! fixture script that models its real contract (structured JSON output,
//! `--resume <session_id>` continuation) closely enough to prove the whole
//! vertical without ever touching a live provider.
//!
//! Mirrors `tests/live_ingress_e2e.rs`'s helper style deliberately (its own
//! doc comment already establishes the precedent of mirroring rather than
//! sharing test-only helpers across independently-compiled integration test
//! binaries).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// `cargo test` runs every `#[test]` in this binary concurrently by
/// default. Each test here spawns several REAL processes (git, `o7`,
/// `o7d`, the fake `claude`) — on a resource-constrained single-vCPU
/// development VPS, running all 8 of these at once caused a genuine,
/// reproducible flake: a spawned `o7d`'s very first stderr line failed to
/// parse as its own startup banner under heavy concurrent scheduling
/// pressure. Serializing every test in THIS file (never affects other
/// test binaries' own parallelism) trades a little wall-clock time for
/// reliability, which is the right trade for process-heavy integration
/// tests on constrained hardware.
static SERIAL: Mutex<()> = Mutex::new(());

fn o7d_bin_path() -> PathBuf {
    let own = PathBuf::from(env!("CARGO_BIN_EXE_o7"));
    let dir = own.parent().expect("o7's binary has a parent dir");
    let candidate = dir.join("o7d");
    assert!(
        candidate.exists(),
        "expected o7d's binary at {} — run `cargo build -p o7d` first if testing this file in \
         isolation",
        candidate.display()
    );
    candidate
}

/// A fake `claude` on its own PATH-prepended directory that models BOTH
/// calls this vertical makes: the initial `-p <task> --output-format json`
/// call (`agent::run`) and the continuation `--resume <session> -p
/// <command> --output-format json` call (`agent::continue_session`). Every
/// invocation: increments a durable counter, records its FULL argv
/// (NUL-delimited, so an argument containing an embedded newline round-
/// trips exactly — proving the command text was never shell-interpreted),
/// sleeps for however long `sleep_seconds` currently says (readable/
/// writable by the test between invocations, to open a deliberate
/// concurrency window), then answers the same structured JSON envelope
/// `judge.rs::call_claude` already parses in production.
struct FakeClaude {
    dir: tempfile::TempDir,
}

impl FakeClaude {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("claude");
        std::fs::write(
            &script,
            r#"#!/bin/sh
DIR="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
N=$(( $(cat "$DIR/count" 2>/dev/null || echo 0) + 1 ))
echo "$N" > "$DIR/count"
printf '%s\0' "$@" > "$DIR/argv.$N"
touch "$DIR/invoked.$N"
S=$(cat "$DIR/sleep_seconds" 2>/dev/null || echo 0)
sleep "$S"
printf '{"result":"synthetic ok","session_id":"fixed-session-1","total_cost_usd":0.001}\n'
exit 0
"#,
        )
        .unwrap();
        let mut perms = std::fs::metadata(&script).unwrap().permissions();
        std::os::unix::fs::PermissionsExt::set_mode(&mut perms, 0o755);
        std::fs::set_permissions(&script, perms).unwrap();
        std::fs::write(dir.path().join("sleep_seconds"), "0").unwrap();
        Self { dir }
    }

    fn path(&self) -> &Path {
        self.dir.path()
    }

    fn set_sleep_seconds(&self, secs: u64) {
        std::fs::write(self.dir.path().join("sleep_seconds"), secs.to_string()).unwrap();
    }

    fn invocation_count(&self) -> u64 {
        std::fs::read_to_string(self.dir.path().join("count"))
            .ok()
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(0)
    }

    /// Wait (bounded) until at least `n` invocations have been recorded.
    fn wait_for_invocation(&self, n: u64, deadline: Instant) {
        poll_until(deadline, || (self.invocation_count() >= n).then_some(()));
    }

    /// The exact argv (excluding argv[0]) of invocation number `n` (1-based).
    fn argv(&self, n: u64) -> Vec<String> {
        let bytes = std::fs::read(self.dir.path().join(format!("argv.{n}"))).unwrap();
        bytes
            .split(|b| *b == 0)
            .filter(|part| !part.is_empty())
            .map(|part| String::from_utf8_lossy(part).into_owned())
            .collect()
    }
}

fn fixture_repo(gate_toml: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let run = |args: &[&str]| {
        let status = Command::new("git")
            .args(args)
            .current_dir(dir.path())
            .status()
            .unwrap();
        assert!(status.success(), "git {args:?} failed");
    };
    run(&["init", "-q"]);
    run(&["config", "user.email", "test@example.com"]);
    run(&["config", "user.name", "test"]);
    std::fs::write(dir.path().join("README.md"), "fixture\n").unwrap();
    let gate_dir = dir.path().join(".007");
    std::fs::create_dir_all(&gate_dir).unwrap();
    std::fs::write(gate_dir.join("gate.toml"), gate_toml).unwrap();
    run(&["add", "-A"]);
    run(&["commit", "-q", "-m", "initial"]);
    dir
}

const PASSING_GATE: &str = "[[gate]]\nname = \"unit\"\ncmd = \"true\"\n";

/// A directory holding ONLY symlinks to the exact external tools the
/// worktree/gate machinery needs (`git`, `bash`, `sh`), resolved once via
/// the CURRENT ambient `PATH`. Exists so a test can construct an EXPLICIT,
/// minimal `PATH` for a spawned `o7d` that excludes every other directory
/// — specifically, whatever directory holds a REAL `claude` binary this
/// machine happens to have installed (this test suite itself runs inside
/// a machine with Claude Code, so `/usr/bin/claude` existing alongside
/// `/usr/bin/git`/`/usr/bin/bash` is the normal case, not an edge case —
/// a test that needs "the provider is genuinely unfindable" cannot rely on
/// deleting the fixture's own script alone, since PATH search would just
/// fall through to that real binary next).
fn safe_bin_dir() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for tool in ["git", "bash", "sh"] {
        let which = Command::new("which").arg(tool).output().unwrap();
        assert!(
            which.status.success(),
            "{tool} must be on PATH for this test to run at all"
        );
        let real_path = String::from_utf8(which.stdout).unwrap().trim().to_owned();
        std::os::unix::fs::symlink(&real_path, dir.path().join(tool)).unwrap();
    }
    dir
}

/// `o7d serve` with Q-Deck R1's execution authority configured — `--repo`/
/// `--worktree-root`/`--runs-dir`/`--o7-bin` fixed at startup, never from a
/// request (`docs/q-deck/r1-command.md` §9.4).
fn spawn_o7d_with_exec(
    db_path: &Path,
    repo: &Path,
    worktree_root: &Path,
    runs_dir: &Path,
    claude_dir: &Path,
) -> (Child, SocketAddr) {
    spawn_o7d_with_exec_and_redrive_ms(db_path, repo, worktree_root, runs_dir, claude_dir, None)
}

/// Same as [`spawn_o7d_with_exec`], with the stale-command-redrive
/// threshold overridden via `O7D_STALE_COMMAND_REDRIVE_MS` (production's
/// 60s default would make proving blocker-1's redrive path impractically
/// slow for a test).
fn spawn_o7d_with_exec_and_redrive_ms(
    db_path: &Path,
    repo: &Path,
    worktree_root: &Path,
    runs_dir: &Path,
    claude_dir: &Path,
    redrive_ms: Option<u64>,
) -> (Child, SocketAddr) {
    let mut cmd = Command::new(o7d_bin_path());
    cmd.args([
        "serve",
        "--ledger",
        db_path.to_str().expect("utf8 path"),
        "--listen",
        "127.0.0.1:0",
        "--repo",
    ])
    .arg(repo)
    .arg("--worktree-root")
    .arg(worktree_root)
    .arg("--runs-dir")
    .arg(runs_dir)
    .arg("--o7-bin")
    .arg(env!("CARGO_BIN_EXE_o7"))
    .arg("--max-turns")
    .arg("1")
    .env(
        "PATH",
        format!(
            "{}:{}",
            claude_dir.display(),
            std::env::var("PATH").unwrap_or_default()
        ),
    )
    .stdout(Stdio::null())
    .stderr(Stdio::piped());
    if let Some(ms) = redrive_ms {
        cmd.env("O7D_STALE_COMMAND_REDRIVE_MS", ms.to_string());
    }
    let mut child = cmd.spawn().expect("spawn the real o7d binary");
    let stderr = child.stderr.take().expect("piped stderr");
    let mut reader = BufReader::new(stderr);
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .expect("read o7d's startup line");
    let addr_str = line
        .trim()
        .strip_prefix("o7d: listening on http://")
        .expect("startup line reports the bound address");
    let addr: SocketAddr = addr_str.parse().expect("valid socket addr");
    std::thread::spawn(move || {
        let mut discarded = String::new();
        while reader.read_line(&mut discarded).unwrap_or(0) > 0 {
            discarded.clear();
        }
    });
    (child, addr)
}

fn spawn_o7_run_process(
    repo: &Path,
    task_file: &Path,
    ledger_path: &Path,
    runs_dir: &Path,
    worktree_root: &Path,
    claude_dir: &Path,
) -> Child {
    Command::new(env!("CARGO_BIN_EXE_o7"))
        .arg("run")
        .arg("--repo")
        .arg(repo)
        .arg("--task")
        .arg(task_file)
        .arg("--runs-dir")
        .arg(runs_dir)
        .arg("--worktree-root")
        .arg(worktree_root)
        .arg("--max-turns")
        .arg("1")
        .arg("--ledger")
        .arg(ledger_path)
        .env(
            "PATH",
            format!(
                "{}:{}",
                claude_dir.display(),
                std::env::var("PATH").unwrap_or_default()
            ),
        )
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn the real o7 binary")
}

fn http_roundtrip(addr: SocketAddr, request: &str) -> (u16, serde_json::Value) {
    let mut stream = std::net::TcpStream::connect(addr).unwrap();
    // Defense in depth against a malformed request in THIS test file (e.g.
    // a Content-Length that doesn't match the actual body) hanging the
    // whole suite instead of failing loudly — a real incident this file
    // already hit once.
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    stream.write_all(request.as_bytes()).unwrap();
    let mut resp = Vec::new();
    stream.read_to_end(&mut resp).unwrap();
    let text = String::from_utf8_lossy(&resp);
    let mut parts = text.splitn(2, "\r\n\r\n");
    let head = parts.next().unwrap_or_default();
    let body = parts.next().unwrap_or_default();
    let status: u16 = head
        .lines()
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let json_text = if head
        .to_ascii_lowercase()
        .contains("transfer-encoding: chunked")
    {
        dechunk(body)
    } else {
        body.to_string()
    };
    let value = if json_text.trim().is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_str(&json_text).unwrap_or(serde_json::Value::Null)
    };
    (status, value)
}

fn get(addr: SocketAddr, path: &str) -> (u16, serde_json::Value) {
    http_roundtrip(
        addr,
        &format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"),
    )
}

fn post(addr: SocketAddr, path: &str, body: &serde_json::Value) -> (u16, serde_json::Value) {
    let body_text = serde_json::to_string(body).unwrap();
    let request = format!(
        "POST {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nContent-Type: \
         application/json\r\nContent-Length: {}\r\n\r\n{body_text}",
        body_text.len()
    );
    http_roundtrip(addr, &request)
}

#[allow(clippy::while_let_loop)]
fn dechunk(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    loop {
        let Some(nl) = rest.find("\r\n") else { break };
        let size_line = &rest[..nl];
        let Ok(size) = usize::from_str_radix(size_line.trim(), 16) else {
            break;
        };
        if size == 0 {
            break;
        }
        let start = nl + 2;
        let end = (start + size).min(rest.len());
        out.push_str(&rest[start..end]);
        rest = &rest[end..];
        rest = rest.trim_start_matches("\r\n");
    }
    out
}

fn poll_until<T>(deadline: Instant, mut f: impl FnMut() -> Option<T>) -> T {
    loop {
        if let Some(v) = f() {
            return v;
        }
        assert!(Instant::now() <= deadline, "poll_until timed out");
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn wait_until_healthy(addr: SocketAddr) {
    let deadline = Instant::now() + Duration::from_secs(30);
    poll_until(deadline, || {
        let (status, _) = get(addr, "/api/v1/health");
        (status == 200).then_some(())
    });
}

fn wait_for_run_status(addr: SocketAddr, run_id: &str, status: &str, deadline: Instant) {
    poll_until(deadline, || {
        let (code, run) = get(addr, &format!("/api/v1/runs/{run_id}"));
        (code == 200 && run["status"] == status).then_some(())
    });
}

/// The run row's own `provider_session_id` — not exposed over REST by
/// design (`docs/q-deck/r1-command.md` §6: never echoed back to a
/// browser), so checked the same way other tests here check ledger-
/// internal state Q-Deck's own DTOs deliberately don't surface: a direct
/// read of the SQLite file.
fn provider_session_id(ledger_path: &Path, run_id: &str) -> Option<String> {
    let conn = rusqlite::Connection::open(ledger_path).unwrap();
    conn.query_row(
        "SELECT provider_session_id FROM run WHERE run_id = ?1",
        [run_id],
        |row| row.get(0),
    )
    .unwrap()
}

fn command_row_count(ledger_path: &Path) -> i64 {
    let conn = rusqlite::Connection::open(ledger_path).unwrap();
    conn.query_row("SELECT COUNT(*) FROM command", [], |row| row.get(0))
        .unwrap()
}

/// Behaviors (1)-(9), (11): the ordinary, successful multi-turn path —
/// initial run, durable session, one accepted command dispatching EXACTLY
/// one continuation, the child run's correct lineage and visibility over
/// REST/SSE, idempotent replay, a conflicting retry, and a concurrent
/// command rejected before ever touching the provider.
#[test]
fn command_vertical_end_to_end() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let repo = fixture_repo(PASSING_GATE);
    let task_file = repo.path().join("task.md");
    std::fs::write(&task_file, "do the initial thing").unwrap();
    let work = tempfile::tempdir().unwrap();
    let ledger_path = work.path().join("ledger.sqlite3");
    let runs_dir = work.path().join("runs");
    let worktree_root = work.path().join("worktrees");
    let claude = FakeClaude::new();

    // --- Phase 1: an ordinary initial run, exactly like R0.7. ---
    let mut run_child = spawn_o7_run_process(
        repo.path(),
        &task_file,
        &ledger_path,
        &runs_dir,
        &worktree_root,
        claude.path(),
    );
    let status = run_child.wait().unwrap();
    let mut run_stdout = String::new();
    run_child
        .stdout
        .take()
        .unwrap()
        .read_to_string(&mut run_stdout)
        .unwrap();
    assert!(status.success(), "initial run must pass: {run_stdout}");
    assert_eq!(
        claude.invocation_count(),
        1,
        "the initial run must invoke the provider exactly once"
    );

    let (mut o7d_child, addr) = spawn_o7d_with_exec(
        &ledger_path,
        repo.path(),
        &worktree_root,
        &runs_dir,
        claude.path(),
    );
    wait_until_healthy(addr);

    let (status, page) = get(addr, "/api/v1/runs");
    assert_eq!(status, 200);
    let items = page["items"].as_array().unwrap();
    assert_eq!(items.len(), 1, "sanity: exactly one run so far");
    let parent_run_id = items[0]["run_id"].as_str().unwrap().to_owned();
    let conversation_id = items[0]["conversation_id"].as_str().unwrap().to_owned();
    assert_eq!(items[0]["status"], "completed");

    // (1) initial run durably persisted a provider session identity.
    let session = provider_session_id(&ledger_path, &parent_run_id);
    assert_eq!(session.as_deref(), Some("fixed-session-1"));

    // --- Phase 2: one durable command, containing shell-hostile text. ---
    let hostile_command = "run `rm -rf /` && echo $(whoami) \\ \"quoted\"\nwith a newline too";
    let key1 = "idem-key-1";
    let (status, accepted) = post(
        addr,
        &format!("/api/v1/conversations/{conversation_id}/commands"),
        &serde_json::json!({
            "schema_version": 1,
            "parent_run_id": parent_run_id,
            "command": hostile_command,
            "idempotency_key": key1,
        }),
    );
    // (2) 202 with a CommandId and a NEW child RunId.
    assert_eq!(status, 202, "accepted: {accepted:?}");
    assert_eq!(accepted["schema_version"], 1);
    assert_eq!(accepted["conversation_id"], conversation_id);
    assert_eq!(accepted["parent_run_id"], parent_run_id);
    // The response's own status field reports "accepted" for THIS
    // request's own fresh acceptance — matching the frozen contract's
    // response example — even though binding+spawning ALSO happen to
    // succeed synchronously within the same request.
    assert_eq!(accepted["status"], "accepted");
    let command_id = accepted["command_id"].as_str().unwrap().to_owned();
    let child_run_id = accepted["run_id"].as_str().unwrap().to_owned();
    assert_ne!(child_run_id, parent_run_id);

    // (3) the continuation is invoked EXACTLY once for this command.
    let deadline = Instant::now() + Duration::from_secs(30);
    claude.wait_for_invocation(2, deadline);

    // (11) the hostile command text reached the fake provider as ONE
    // literal argv element, byte-for-byte — never shell-interpreted.
    let continuation_argv = claude.argv(2);
    assert!(
        continuation_argv.contains(&"--resume".to_string()),
        "continuation must pass --resume: {continuation_argv:?}"
    );
    assert!(
        continuation_argv.contains(&"fixed-session-1".to_string()),
        "must resume the PARENT run's own session id: {continuation_argv:?}"
    );
    assert!(
        continuation_argv.iter().any(|a| a == hostile_command),
        "the command text must arrive as one exact, unmangled argv element: {continuation_argv:?}"
    );

    // (4) + (5): the child run's lineage and terminal visibility over REST.
    let deadline = Instant::now() + Duration::from_secs(30);
    wait_for_run_status(addr, &child_run_id, "completed", deadline);
    let (status, child_run) = get(addr, &format!("/api/v1/runs/{child_run_id}"));
    assert_eq!(status, 200);
    assert_eq!(child_run["parent_run_id"], parent_run_id);
    assert_eq!(child_run["conversation_id"], conversation_id);

    // (5) visible over SSE too — the conversation's own unfiltered stream
    // carries the child run's run.started.
    let mut sse = std::net::TcpStream::connect(addr).unwrap();
    sse.write_all(
        format!(
            "GET /api/v1/conversations/{conversation_id}/events/stream HTTP/1.1\r\nHost: \
             localhost\r\nConnection: keep-alive\r\n\r\n"
        )
        .as_bytes(),
    )
    .unwrap();
    sse.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
    let mut sse_reader = BufReader::new(sse);
    let mut saw_child_run_started = false;
    // Bounded by BOTH a wall-clock deadline (a live SSE stream could keep
    // sending keep-alive comments forever, so a per-read timeout alone
    // never ends the loop if the target event genuinely never appears) and
    // the read timeout above — not a fixed line count, since the
    // conversation's full replayed history (initial run + command) is
    // several `event:`/`id:`/`data:`/blank lines each, comfortably more
    // than a small fixed cap would allow for.
    let sse_deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < sse_deadline {
        let mut line = String::new();
        if sse_reader.read_line(&mut line).unwrap_or(0) == 0 {
            break;
        }
        if let Some(rest) = line.strip_prefix("data:") {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(rest.trim()) {
                if v["event_type"] == "run.started" && v["run_id"] == child_run_id {
                    saw_child_run_started = true;
                    break;
                }
            }
        }
    }
    assert!(
        saw_child_run_started,
        "the child run's run.started must appear on the existing conversation SSE stream"
    );

    // (6) same key + IDENTICAL request replays the same identities, no
    // second invocation.
    let (status, replay) = post(
        addr,
        &format!("/api/v1/conversations/{conversation_id}/commands"),
        &serde_json::json!({
            "schema_version": 1,
            "parent_run_id": parent_run_id,
            "command": hostile_command,
            "idempotency_key": key1,
        }),
    );
    assert_eq!(status, 202);
    assert_eq!(replay["command_id"], command_id);
    assert_eq!(replay["run_id"], child_run_id);
    assert_eq!(
        claude.invocation_count(),
        2,
        "an idempotent replay must never invoke the provider a second time"
    );

    // (7) same key + DIFFERENT text conflicts, no invocation.
    let (status, conflict) = post(
        addr,
        &format!("/api/v1/conversations/{conversation_id}/commands"),
        &serde_json::json!({
            "schema_version": 1,
            "parent_run_id": parent_run_id,
            "command": "a completely different command",
            "idempotency_key": key1,
        }),
    );
    assert_eq!(status, 409, "conflict: {conflict:?}");
    assert_eq!(conflict["code"], "IDEMPOTENCY_CONFLICT");
    assert_eq!(
        claude.invocation_count(),
        2,
        "a rejected conflicting request must never invoke the provider"
    );
    assert_eq!(
        command_row_count(&ledger_path),
        1,
        "the conflicting request must not have created a second command row"
    );

    let _ = o7d_child.kill();
    let _ = o7d_child.wait();
}

/// (8): two commands racing for the SAME conversation — the second must be
/// rejected with a conflict BEFORE the provider is ever touched a second
/// time, and the two continuations never run concurrently.
#[test]
fn a_second_concurrent_command_is_rejected_before_provider_invocation() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let repo = fixture_repo(PASSING_GATE);
    let task_file = repo.path().join("task.md");
    std::fs::write(&task_file, "do the initial thing").unwrap();
    let work = tempfile::tempdir().unwrap();
    let ledger_path = work.path().join("ledger.sqlite3");
    let runs_dir = work.path().join("runs");
    let worktree_root = work.path().join("worktrees");
    let claude = FakeClaude::new();

    let mut run_child = spawn_o7_run_process(
        repo.path(),
        &task_file,
        &ledger_path,
        &runs_dir,
        &worktree_root,
        claude.path(),
    );
    assert!(run_child.wait().unwrap().success());

    let (mut o7d_child, addr) = spawn_o7d_with_exec(
        &ledger_path,
        repo.path(),
        &worktree_root,
        &runs_dir,
        claude.path(),
    );
    wait_until_healthy(addr);
    let (_, page) = get(addr, "/api/v1/runs");
    let parent_run_id = page["items"][0]["run_id"].as_str().unwrap().to_owned();
    let conversation_id = page["items"][0]["conversation_id"]
        .as_str()
        .unwrap()
        .to_owned();

    // Make the NEXT (continuation) invocation slow, opening a real window
    // where the command is durably `accepted`/`started` but not yet done.
    claude.set_sleep_seconds(5);
    let (status, first) = post(
        addr,
        &format!("/api/v1/conversations/{conversation_id}/commands"),
        &serde_json::json!({
            "schema_version": 1,
            "parent_run_id": parent_run_id,
            "command": "first command",
            "idempotency_key": "key-a",
        }),
    );
    assert_eq!(status, 202, "first: {first:?}");

    // While the first is still in flight, a second, genuinely different
    // submission must be rejected — before ever invoking the provider a
    // second time.
    let deadline = Instant::now() + Duration::from_secs(30);
    claude.wait_for_invocation(2, deadline); // the first continuation has started
    let (status, second) = post(
        addr,
        &format!("/api/v1/conversations/{conversation_id}/commands"),
        &serde_json::json!({
            "schema_version": 1,
            "parent_run_id": parent_run_id,
            "command": "second command",
            "idempotency_key": "key-b",
        }),
    );
    // Either code is a legitimate rejection here: by the time the first
    // command's continuation is actually running, its child run row (with
    // `parent_run_id = parent_run_id`) already exists, so BOTH "another
    // command is in flight" and "this parent already has a child" are true
    // simultaneously — which one `create_command` reports first is an
    // implementation-ordering detail, not something this test should pin
    // down. The one thing that MUST hold is: never a 202, never a second
    // provider invocation.
    assert_eq!(status, 409, "second: {second:?}");
    assert!(
        second["code"] == "COMMAND_CONFLICT" || second["code"] == "STALE_PARENT",
        "expected a conflict/stale-parent rejection, got: {second:?}"
    );
    assert_eq!(
        claude.invocation_count(),
        2,
        "the rejected second command must never reach the provider — only the \
         initial run + the first command's continuation may have run"
    );

    // Let the first finish; the child run must complete and free the
    // conversation up for a later command (not exercised further here).
    let (_, first_child) = get(
        addr,
        &format!("/api/v1/runs/{}", first["run_id"].as_str().unwrap()),
    );
    let child_run_id = first_child["run_id"]
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| first["run_id"].as_str().unwrap().to_owned());
    let deadline = Instant::now() + Duration::from_secs(30);
    wait_for_run_status(addr, &child_run_id, "completed", deadline);

    let _ = o7d_child.kill();
    let _ = o7d_child.wait();
}

/// (9): after an `o7d` restart, the accepted command and its child run
/// remain observable — durable, not process-memory-only.
#[test]
fn accepted_command_and_child_run_survive_an_o7d_restart() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let repo = fixture_repo(PASSING_GATE);
    let task_file = repo.path().join("task.md");
    std::fs::write(&task_file, "do the initial thing").unwrap();
    let work = tempfile::tempdir().unwrap();
    let ledger_path = work.path().join("ledger.sqlite3");
    let runs_dir = work.path().join("runs");
    let worktree_root = work.path().join("worktrees");
    let claude = FakeClaude::new();

    let mut run_child = spawn_o7_run_process(
        repo.path(),
        &task_file,
        &ledger_path,
        &runs_dir,
        &worktree_root,
        claude.path(),
    );
    assert!(run_child.wait().unwrap().success());

    let (mut o7d_child, addr) = spawn_o7d_with_exec(
        &ledger_path,
        repo.path(),
        &worktree_root,
        &runs_dir,
        claude.path(),
    );
    wait_until_healthy(addr);
    let (_, page) = get(addr, "/api/v1/runs");
    let parent_run_id = page["items"][0]["run_id"].as_str().unwrap().to_owned();
    let conversation_id = page["items"][0]["conversation_id"]
        .as_str()
        .unwrap()
        .to_owned();

    let (status, accepted) = post(
        addr,
        &format!("/api/v1/conversations/{conversation_id}/commands"),
        &serde_json::json!({
            "schema_version": 1,
            "parent_run_id": parent_run_id,
            "command": "restart durability check",
            "idempotency_key": "key-restart",
        }),
    );
    assert_eq!(status, 202, "{accepted:?}");
    let child_run_id = accepted["run_id"].as_str().unwrap().to_owned();
    let deadline = Instant::now() + Duration::from_secs(30);
    wait_for_run_status(addr, &child_run_id, "completed", deadline);
    let (_, before) = get(addr, &format!("/api/v1/runs/{child_run_id}"));

    let _ = o7d_child.kill();
    let _ = o7d_child.wait();

    let (mut o7d_child2, addr2) = spawn_o7d_with_exec(
        &ledger_path,
        repo.path(),
        &worktree_root,
        &runs_dir,
        claude.path(),
    );
    wait_until_healthy(addr2);
    let (status, after) = get(addr2, &format!("/api/v1/runs/{child_run_id}"));
    assert_eq!(status, 200);
    assert_eq!(
        after, before,
        "the child run's full DTO must be byte-identical after an o7d restart"
    );

    let _ = o7d_child2.kill();
    let _ = o7d_child2.wait();
}

/// (10): a command durably accepted (and, here, already bound to a child
/// run id) but whose process crashed before that child run's own
/// `RunStarted` ever reached the ledger must stay DISCOVERABLE via the
/// real `o7 recover` binary — never silently vanish
/// (`docs/q-deck/r1-command.md` §7). Modeled directly at the ledger level
/// (the same way `live_ingress_e2e.rs`'s own recovery tests simulate a
/// crash by editing rows directly, rather than racing a real kill signal
/// against a fast fixture) — the real `o7 recover` process is still what's
/// exercised here, only the crash itself is simulated.
#[test]
fn a_command_stuck_before_its_child_run_started_is_discoverable_via_recover() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let repo = fixture_repo(PASSING_GATE);
    let task_file = repo.path().join("task.md");
    std::fs::write(&task_file, "do the initial thing").unwrap();
    let work = tempfile::tempdir().unwrap();
    let ledger_path = work.path().join("ledger.sqlite3");
    let runs_dir = work.path().join("runs");
    let worktree_root = work.path().join("worktrees");
    let claude = FakeClaude::new();

    let mut run_child = spawn_o7_run_process(
        repo.path(),
        &task_file,
        &ledger_path,
        &runs_dir,
        &worktree_root,
        claude.path(),
    );
    assert!(run_child.wait().unwrap().success());

    let (parent_run_id, conversation_id) = {
        let conn = rusqlite::Connection::open(&ledger_path).unwrap();
        let row: (String, String) = conn
            .query_row("SELECT run_id, conversation_id FROM run LIMIT 1", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        row
    };

    // Simulate: `o7d` durably accepted a command AND bound it to a freshly
    // minted child run id, then the `o7 continue` process (or o7d itself)
    // was killed before that child run's own ledger row (RunStarted) ever
    // existed — exactly the gap `stuck_commands` exists to close.
    {
        let conn = rusqlite::Connection::open(&ledger_path).unwrap();
        conn.execute(
            "INSERT INTO command (command_id, conversation_id, parent_run_id, command_text, \
             status, child_run_id, created_at, updated_at) \
             VALUES ('cmd-stuck-1', ?1, ?2, 'stuck command', 'started', 'run-never-started', \
             1000, 1000)",
            [&conversation_id, &parent_run_id],
        )
        .unwrap();
    }

    let recover = Command::new(env!("CARGO_BIN_EXE_o7"))
        .args(["recover", "--ledger"])
        .arg(&ledger_path)
        .output()
        .unwrap();
    assert!(recover.status.success(), "{recover:?}");
    let stdout = String::from_utf8_lossy(&recover.stdout);
    assert!(
        stdout.contains("1 command(s) pending"),
        "the stuck command must be reported, not silently dropped: {stdout}"
    );
    assert!(stdout.contains("cmd-stuck-1"));

    // A repeated recover is a stable, safe no-op report (the command is
    // still not re-driven automatically — out of scope for this slice).
    let recover2 = Command::new(env!("CARGO_BIN_EXE_o7"))
        .args(["recover", "--ledger"])
        .arg(&ledger_path)
        .output()
        .unwrap();
    assert!(recover2.status.success());
    assert!(String::from_utf8_lossy(&recover2.stdout).contains("1 command(s) pending"));
}

/// Q-Deck R1 must not disturb R0.7's own initial-run behavior when
/// `o7d` is started WITHOUT execution authority — the mutation endpoint
/// must fail closed with a clear `500`, never a silent no-op, and every
/// pre-existing read-only route keeps working exactly as before.
#[test]
fn without_exec_config_the_command_endpoint_fails_closed_and_reads_still_work() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let repo = fixture_repo(PASSING_GATE);
    let task_file = repo.path().join("task.md");
    std::fs::write(&task_file, "do the initial thing").unwrap();
    let work = tempfile::tempdir().unwrap();
    let ledger_path = work.path().join("ledger.sqlite3");
    let claude = FakeClaude::new();

    let mut run_child = spawn_o7_run_process(
        repo.path(),
        &task_file,
        &ledger_path,
        &work.path().join("runs"),
        &work.path().join("worktrees"),
        claude.path(),
    );
    assert!(run_child.wait().unwrap().success());

    // Plain `o7d serve`, no --repo — R0's original read-only posture.
    let mut child = Command::new(o7d_bin_path())
        .args([
            "serve",
            "--ledger",
            ledger_path.to_str().unwrap(),
            "--listen",
            "127.0.0.1:0",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let stderr = child.stderr.take().unwrap();
    let mut reader = BufReader::new(stderr);
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    let addr: SocketAddr = line
        .trim()
        .strip_prefix("o7d: listening on http://")
        .unwrap()
        .parse()
        .unwrap();
    std::thread::spawn(move || {
        let mut discarded = String::new();
        while reader.read_line(&mut discarded).unwrap_or(0) > 0 {
            discarded.clear();
        }
    });
    wait_until_healthy(addr);

    let (status, page) = get(addr, "/api/v1/runs");
    assert_eq!(status, 200, "read routes must be unaffected");
    let conversation_id = page["items"][0]["conversation_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let parent_run_id = page["items"][0]["run_id"].as_str().unwrap().to_owned();

    let (status, body) = post(
        addr,
        &format!("/api/v1/conversations/{conversation_id}/commands"),
        &serde_json::json!({
            "schema_version": 1,
            "parent_run_id": parent_run_id,
            "command": "hi",
            "idempotency_key": "k",
        }),
    );
    assert_eq!(status, 500, "{body:?}");
    assert_eq!(body["code"], "EXEC_NOT_CONFIGURED");
    assert_eq!(
        claude.invocation_count(),
        1,
        "an unconfigured mutation attempt must never spawn a provider continuation"
    );

    let _ = child.kill();
    let _ = child.wait();
}

/// Q-Deck R1 corrective round, blocker 1 (independent re-gate on PR #90):
/// a command bound to a child run whose own `RunStarted` never reached the
/// ledger (the process that was going to spawn `o7 continue` died right
/// after the bind) must be safely re-drivable by a retry of the SAME
/// idempotent request — not permanently stuck, blocking the conversation
/// forever. Modeled directly at the ledger level (the same established
/// pattern `a_command_stuck_before_its_child_run_started_is_discoverable_via_recover`
/// and `live_ingress_e2e.rs`'s own crash-simulation tests use): hand-insert
/// a `command` row already `started`/bound to a run id that was never
/// created, plus a matching `idempotency_record` so a real HTTP retry
/// idempotently replays onto it — then prove the retry actually redrives
/// the REAL provider continuation through to a sealed child run.
#[test]
fn a_command_stuck_before_dispatch_is_redriven_by_a_retry_after_the_stale_bound() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let repo = fixture_repo(PASSING_GATE);
    let task_file = repo.path().join("task.md");
    std::fs::write(&task_file, "do the initial thing").unwrap();
    let work = tempfile::tempdir().unwrap();
    let ledger_path = work.path().join("ledger.sqlite3");
    let runs_dir = work.path().join("runs");
    let worktree_root = work.path().join("worktrees");
    let claude = FakeClaude::new();

    let mut run_child = spawn_o7_run_process(
        repo.path(),
        &task_file,
        &ledger_path,
        &runs_dir,
        &worktree_root,
        claude.path(),
    );
    assert!(run_child.wait().unwrap().success());
    assert_eq!(claude.invocation_count(), 1);

    let (parent_run_id, conversation_id) = {
        let conn = rusqlite::Connection::open(&ledger_path).unwrap();
        let row: (String, String) = conn
            .query_row("SELECT run_id, conversation_id FROM run LIMIT 1", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        row
    };

    let command_text = "redrive check";
    let idempotency_key = "key-redrive";
    let child_run_id_before_redrive = o7_ledger::RunId::generate();
    // A small positive epoch millis value — "1000ms after the epoch",
    // i.e. as far in the past as this test needs: `now_millis() -
    // stale_updated_at` is a huge number, comfortably past any sane
    // staleness threshold.
    let stale_updated_at = 1000i64;

    // Hand-build EXACTLY the durable state a real durable-bind-then-crash
    // would have left behind: a `command` row `started`/bound, plus the
    // matching idempotency_record a genuine `create_command` call would
    // have written (same scope, same digest formula) — so the HTTP retry
    // below takes the REAL idempotent-replay code path, not a fabricated
    // shortcut.
    {
        let conn = rusqlite::Connection::open(&ledger_path).unwrap();
        let digest_input = serde_json::json!({
            "conversation_id": conversation_id,
            "parent_run_id": parent_run_id,
            "command_text": command_text,
        });
        let digest =
            o7_ledger::idempotency::digest_bytes(&serde_json::to_vec(&digest_input).unwrap());
        conn.execute(
            "INSERT INTO command (command_id, conversation_id, parent_run_id, command_text, \
             status, child_run_id, created_at, updated_at) \
             VALUES ('cmd-stuck-redrive', ?1, ?2, ?3, 'started', ?4, 1000, ?5)",
            rusqlite::params![
                conversation_id,
                parent_run_id,
                command_text,
                child_run_id_before_redrive.as_str(),
                stale_updated_at,
            ],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO idempotency_record (scope, key, request_digest, result_reference, \
             created_at) VALUES ('create-command', ?1, ?2, 'cmd-stuck-redrive', 1000)",
            rusqlite::params![idempotency_key, digest],
        )
        .unwrap();
    }

    // A short redrive threshold isn't even needed here (`stale_updated_at`
    // is already far enough in the past to be stale under ANY sane
    // threshold), but set one anyway so this test does not silently start
    // depending on production's 60s default.
    let (mut o7d_child, addr) = spawn_o7d_with_exec_and_redrive_ms(
        &ledger_path,
        repo.path(),
        &worktree_root,
        &runs_dir,
        claude.path(),
        Some(200),
    );
    wait_until_healthy(addr);

    let (status, retried) = post(
        addr,
        &format!("/api/v1/conversations/{conversation_id}/commands"),
        &serde_json::json!({
            "schema_version": 1,
            "parent_run_id": parent_run_id,
            "command": command_text,
            "idempotency_key": idempotency_key,
        }),
    );
    assert_eq!(status, 202, "{retried:?}");
    assert_eq!(retried["command_id"], "cmd-stuck-redrive");
    // A redrive ALWAYS mints a FRESH child run id — never reuses the
    // originally-bound one, even though nothing durable was ever written
    // under it in THIS test's fixture: a real crash could have left a
    // PARTIAL events.jsonl already on disk for that id, and reusing it
    // would risk a second, conflicting RunStarted at sequence 1.
    let redriven_run_id = retried["run_id"].as_str().unwrap().to_owned();
    assert_ne!(
        redriven_run_id,
        child_run_id_before_redrive.as_str(),
        "a redrive must mint a fresh run id, never reuse the original binding"
    );

    let deadline = Instant::now() + Duration::from_secs(30);
    claude.wait_for_invocation(2, deadline);
    let deadline = Instant::now() + Duration::from_secs(30);
    wait_for_run_status(addr, &redriven_run_id, "completed", deadline);

    let (status, command_row) = get(addr, &format!("/api/v1/runs/{redriven_run_id}"));
    assert_eq!(status, 200);
    assert_eq!(command_row["parent_run_id"], parent_run_id);

    let _ = o7d_child.kill();
    let _ = o7d_child.wait();
}

/// Q-Deck R1 second corrective round, blocker 2: a wall-clock staleness
/// bound alone is NOT proof a stuck command's original `o7 continue`
/// process is dead — only slow (e.g. a large worktree checkout past the
/// threshold). Proves the real safety mechanism: while THIS TEST ITSELF
/// holds the exact same `flock` `o7 continue` would hold for its entire
/// lifetime, a stale retry must NOT redrive (never invoke the provider a
/// second time) — only once the lock is released does the SAME retry
/// succeed.
#[test]
fn a_redrive_never_races_a_lock_genuinely_still_held_by_a_live_process() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let repo = fixture_repo(PASSING_GATE);
    let task_file = repo.path().join("task.md");
    std::fs::write(&task_file, "do the initial thing").unwrap();
    let work = tempfile::tempdir().unwrap();
    let ledger_path = work.path().join("ledger.sqlite3");
    let runs_dir = work.path().join("runs");
    let worktree_root = work.path().join("worktrees");
    let claude = FakeClaude::new();

    let mut run_child = spawn_o7_run_process(
        repo.path(),
        &task_file,
        &ledger_path,
        &runs_dir,
        &worktree_root,
        claude.path(),
    );
    assert!(run_child.wait().unwrap().success());

    let (parent_run_id, conversation_id) = {
        let conn = rusqlite::Connection::open(&ledger_path).unwrap();
        let row: (String, String) = conn
            .query_row("SELECT run_id, conversation_id FROM run LIMIT 1", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        row
    };

    let command_text = "lock check";
    let idempotency_key = "key-lock";
    let stuck_child_run_id = o7_ledger::RunId::generate();
    let stale_updated_at = 1000i64;
    {
        let conn = rusqlite::Connection::open(&ledger_path).unwrap();
        let digest_input = serde_json::json!({
            "conversation_id": conversation_id,
            "parent_run_id": parent_run_id,
            "command_text": command_text,
        });
        let digest =
            o7_ledger::idempotency::digest_bytes(&serde_json::to_vec(&digest_input).unwrap());
        conn.execute(
            "INSERT INTO command (command_id, conversation_id, parent_run_id, command_text, \
             status, child_run_id, created_at, updated_at) \
             VALUES ('cmd-lock-check', ?1, ?2, ?3, 'started', ?4, 1000, ?5)",
            rusqlite::params![
                conversation_id,
                parent_run_id,
                command_text,
                stuck_child_run_id.as_str(),
                stale_updated_at,
            ],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO idempotency_record (scope, key, request_digest, result_reference, \
             created_at) VALUES ('create-command', ?1, ?2, 'cmd-lock-check', 1000)",
            rusqlite::params![idempotency_key, digest],
        )
        .unwrap();
    }

    // Hold the EXACT lock a live `o7 continue` for this run id would hold
    // — same path formula as both `src/main.rs::command_lock_path` and
    // `crates/o7d/src/routes.rs::command_lock_path`.
    let lock_path = runs_dir
        .join(".locks")
        .join(format!("{}.lock", stuck_child_run_id.as_str()));
    std::fs::create_dir_all(lock_path.parent().unwrap()).unwrap();
    let lock_file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(&lock_path)
        .unwrap();
    let held_lock = nix::fcntl::Flock::lock(lock_file, nix::fcntl::FlockArg::LockExclusiveNonblock)
        .expect("test must be the first to acquire this fresh lock file");

    let (mut o7d_child, addr) = spawn_o7d_with_exec_and_redrive_ms(
        &ledger_path,
        repo.path(),
        &worktree_root,
        &runs_dir,
        claude.path(),
        Some(200),
    );
    wait_until_healthy(addr);

    let (status, retried) = post(
        addr,
        &format!("/api/v1/conversations/{conversation_id}/commands"),
        &serde_json::json!({
            "schema_version": 1,
            "parent_run_id": parent_run_id,
            "command": command_text,
            "idempotency_key": idempotency_key,
        }),
    );
    assert_eq!(status, 202, "{retried:?}");
    // Give any (incorrect) redrive attempt a real chance to happen before
    // asserting it didn't.
    std::thread::sleep(Duration::from_millis(500));
    assert_eq!(
        claude.invocation_count(),
        1,
        "a retry must NEVER spawn a second process while the original run id's lock is \
         genuinely still held by a live process"
    );

    // Release — proving the original process finished (or died cleanly).
    drop(held_lock);

    let (status2, retried2) = post(
        addr,
        &format!("/api/v1/conversations/{conversation_id}/commands"),
        &serde_json::json!({
            "schema_version": 1,
            "parent_run_id": parent_run_id,
            "command": command_text,
            "idempotency_key": idempotency_key,
        }),
    );
    assert_eq!(status2, 202, "{retried2:?}");
    let redriven_run_id = retried2["run_id"].as_str().unwrap().to_owned();
    assert_ne!(redriven_run_id, stuck_child_run_id.as_str());

    let deadline = Instant::now() + Duration::from_secs(30);
    claude.wait_for_invocation(2, deadline);
    let deadline = Instant::now() + Duration::from_secs(30);
    wait_for_run_status(addr, &redriven_run_id, "completed", deadline);

    let _ = o7d_child.kill();
    let _ = o7d_child.wait();
}

/// Q-Deck R1 corrective round, blocker 2: a provider session can exist well
/// before a run has actually finished (it is persisted right after the
/// agent call, before gates/`RunSealed`) — a command must never be allowed
/// to continue a parent that has not sealed, even over the real HTTP path.
///
/// A REAL long-running agent process was tried first to produce a
/// genuinely `running` parent, and racing a `poll_until` against it proved
/// reliably flaky on this development VPS under memory/scheduling pressure
/// (a real, if environmental, timing issue — not what this test exists to
/// prove). `live_ingress_e2e.rs`'s own established pattern for simulating a
/// ledger state that would otherwise require a fragile real-time race
/// (`plain_recover_marking_interrupted_never_permanently_blocks_a_sealed_canonical_verdict`,
/// reverting `run.status` directly) is used here instead: run the initial
/// agent QUICKLY to a real seal, then revert its `run.status` back to
/// `running` directly — the exact state a genuinely-still-running parent
/// would have, deterministically, with no race.
#[test]
fn a_command_against_a_still_running_parent_is_rejected_over_http() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let repo = fixture_repo(PASSING_GATE);
    let task_file = repo.path().join("task.md");
    std::fs::write(&task_file, "do the initial thing").unwrap();
    let work = tempfile::tempdir().unwrap();
    let ledger_path = work.path().join("ledger.sqlite3");
    let runs_dir = work.path().join("runs");
    let worktree_root = work.path().join("worktrees");
    let claude = FakeClaude::new();

    let mut run_child = spawn_o7_run_process(
        repo.path(),
        &task_file,
        &ledger_path,
        &runs_dir,
        &worktree_root,
        claude.path(),
    );
    assert!(run_child.wait().unwrap().success());
    assert_eq!(claude.invocation_count(), 1);

    let (parent_run_id, conversation_id) = {
        let conn = rusqlite::Connection::open(&ledger_path).unwrap();
        let row: (String, String) = conn
            .query_row("SELECT run_id, conversation_id FROM run LIMIT 1", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        // A real crash never gets this far without both being `running` —
        // matches the fifth-re-gate precedent `live_ingress_e2e.rs` itself
        // established for this exact kind of fixture (revert run AND its
        // own attempt, never just one).
        conn.execute(
            "UPDATE run SET status = 'running', finished_at = NULL WHERE run_id = ?1",
            [&row.0],
        )
        .unwrap();
        conn.execute(
            "UPDATE run_attempt SET status = 'running', finished_at = NULL WHERE run_id = ?1",
            [&row.0],
        )
        .unwrap();
        row
    };

    let (mut o7d_child, addr) = spawn_o7d_with_exec(
        &ledger_path,
        repo.path(),
        &worktree_root,
        &runs_dir,
        claude.path(),
    );
    wait_until_healthy(addr);

    let (status, body) = post(
        addr,
        &format!("/api/v1/conversations/{conversation_id}/commands"),
        &serde_json::json!({
            "schema_version": 1,
            "parent_run_id": parent_run_id,
            "command": "hi",
            "idempotency_key": "key-still-running",
        }),
    );
    assert_eq!(status, 422, "{body:?}");
    assert_eq!(body["code"], "CONTINUATION_NOT_PERMITTED");
    assert_eq!(
        claude.invocation_count(),
        1,
        "a rejected command must never invoke the provider"
    );

    let _ = o7d_child.kill();
    let _ = o7d_child.wait();
}

/// Q-Deck R1 corrective round: malformed JSON, a field with the wrong
/// type, an unknown field, and an oversized body must ALL answer with the
/// SAME `ErrorDto` shape and `400` — never axum's own default rejection
/// format, never an undocumented `413`.
#[test]
fn malformed_requests_all_answer_with_the_uniform_error_dto() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let repo = fixture_repo(PASSING_GATE);
    let task_file = repo.path().join("task.md");
    std::fs::write(&task_file, "do the initial thing").unwrap();
    let work = tempfile::tempdir().unwrap();
    let ledger_path = work.path().join("ledger.sqlite3");
    let claude = FakeClaude::new();

    let mut run_child = spawn_o7_run_process(
        repo.path(),
        &task_file,
        &ledger_path,
        &work.path().join("runs"),
        &work.path().join("worktrees"),
        claude.path(),
    );
    assert!(run_child.wait().unwrap().success());

    let (mut o7d_child, addr) = spawn_o7d_with_exec(
        &ledger_path,
        repo.path(),
        &work.path().join("worktrees"),
        &work.path().join("runs"),
        claude.path(),
    );
    wait_until_healthy(addr);
    let (_, page) = get(addr, "/api/v1/runs");
    let conversation_id = page["items"][0]["conversation_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let path = format!("/api/v1/conversations/{conversation_id}/commands");

    // Invalid JSON syntax. Content-Length MUST match the body's actual
    // byte length exactly — an over-declared length makes the server keep
    // waiting for bytes that will never arrive, hanging the test instead
    // of failing it.
    let invalid_json_body = "{not json}";
    let (status, body) = http_roundtrip(
        addr,
        &format!(
            "POST {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nContent-Type: \
             application/json\r\nContent-Length: {}\r\n\r\n{invalid_json_body}",
            invalid_json_body.len()
        ),
    );
    assert_eq!(status, 400, "invalid JSON syntax: {body:?}");
    assert!(
        body.get("code").is_some(),
        "must be ErrorDto-shaped: {body:?}"
    );

    // A field with the wrong type.
    let (status, body) = post(
        addr,
        &path,
        &serde_json::json!({
            "schema_version": 1,
            "parent_run_id": 12345,
            "command": "hi",
            "idempotency_key": "k1",
        }),
    );
    assert_eq!(status, 400, "wrong field type: {body:?}");
    assert!(
        body.get("code").is_some(),
        "must be ErrorDto-shaped: {body:?}"
    );

    // An unknown field.
    let (status, body) = post(
        addr,
        &path,
        &serde_json::json!({
            "schema_version": 1,
            "parent_run_id": "whatever",
            "command": "hi",
            "idempotency_key": "k2",
            "worktree_path": "/etc",
        }),
    );
    assert_eq!(status, 400, "unknown field: {body:?}");
    assert!(
        body.get("code").is_some(),
        "must be ErrorDto-shaped: {body:?}"
    );

    // An oversized body — must not be an undocumented 413 with a
    // differently-shaped payload.
    let oversized = "x".repeat(32 * 1024);
    let big_body = serde_json::json!({
        "schema_version": 1,
        "parent_run_id": "whatever",
        "command": oversized,
        "idempotency_key": "k3",
    })
    .to_string();
    let (status, body) = http_roundtrip(
        addr,
        &format!(
            "POST {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nContent-Type: \
             application/json\r\nContent-Length: {}\r\n\r\n{big_body}",
            big_body.len()
        ),
    );
    assert_eq!(status, 400, "oversized body: {body:?}");
    assert!(
        body.get("code").is_some(),
        "must be ErrorDto-shaped: {body:?}"
    );
    assert_eq!(
        claude.invocation_count(),
        1,
        "not one of these malformed requests may ever reach the provider"
    );

    let _ = o7d_child.kill();
    let _ = o7d_child.wait();
}

/// Q-Deck R1 second corrective round, major finding: the live path's own
/// best-effort session persistence can fail independently of everything
/// else `o7 recover --run-dir` verifies — and that failure's own warning
/// explicitly promises catch-up will fix it. Proves it actually does:
/// `meta.json` (written regardless of whether the live ledger write
/// succeeded) backfills the ledger's `provider_session_id` once it's
/// missing there.
#[test]
fn recover_run_dir_backfills_a_missing_provider_session_from_meta_json() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let repo = fixture_repo(PASSING_GATE);
    let task_file = repo.path().join("task.md");
    std::fs::write(&task_file, "do the initial thing").unwrap();
    let work = tempfile::tempdir().unwrap();
    let ledger_path = work.path().join("ledger.sqlite3");
    let runs_dir = work.path().join("runs");
    let worktree_root = work.path().join("worktrees");
    let claude = FakeClaude::new();

    let mut run_child = spawn_o7_run_process(
        repo.path(),
        &task_file,
        &ledger_path,
        &runs_dir,
        &worktree_root,
        claude.path(),
    );
    assert!(run_child.wait().unwrap().success());

    let run_id = {
        let conn = rusqlite::Connection::open(&ledger_path).unwrap();
        let id: String = conn
            .query_row("SELECT run_id FROM run LIMIT 1", [], |r| r.get(0))
            .unwrap();
        id
    };
    assert!(
        provider_session_id(&ledger_path, &run_id).is_some(),
        "sanity: the live path must have persisted a session normally"
    );

    // Simulate "the live best-effort persist failed" — revert JUST the
    // ledger's own column, leaving meta.json (built independently, from
    // the same in-memory AgentRun) untouched.
    {
        let conn = rusqlite::Connection::open(&ledger_path).unwrap();
        conn.execute(
            "UPDATE run SET provider_session_id = NULL WHERE run_id = ?1",
            [&run_id],
        )
        .unwrap();
    }
    assert!(provider_session_id(&ledger_path, &run_id).is_none());

    let target_name = repo
        .path()
        .file_name()
        .unwrap()
        .to_string_lossy()
        .to_string();
    let record_dir = runs_dir.join(&target_name).join(&run_id);
    let meta_text = std::fs::read_to_string(record_dir.join("meta.json")).unwrap();
    let meta: serde_json::Value = serde_json::from_str(&meta_text).unwrap();
    let expected_session = meta["session_id"]
        .as_str()
        .expect("meta.json must carry the session independently of the ledger")
        .to_owned();

    let recover = Command::new(env!("CARGO_BIN_EXE_o7"))
        .args(["recover", "--ledger"])
        .arg(&ledger_path)
        .args(["--run-dir"])
        .arg(&record_dir)
        .output()
        .unwrap();
    assert!(
        recover.status.success(),
        "{}",
        String::from_utf8_lossy(&recover.stderr)
    );

    assert_eq!(
        provider_session_id(&ledger_path, &run_id),
        Some(expected_session),
        "catch-up must backfill the session from meta.json, exactly as its own warning promises"
    );
}

/// Q-Deck R1 third corrective round, blocker 1: an ORDINARY error well
/// after the child run's own ledger row exists (`attach_run` already
/// succeeded — this is not a SIGKILL, just the provider binary vanishing
/// out from under a real continuation attempt) must NEVER unconditionally
/// mark the command `rejected`. A `rejected` command is invisible to
/// every discovery/redrive mechanism (all scoped to `accepted`/`started`),
/// and its non-terminal child (a real, durable tail — neither the old
/// parent nor this child can ever be a valid parent again) would then
/// block the conversation forever, no SIGKILL required.
#[test]
fn an_ordinary_error_after_attach_never_rejects_a_command_and_stays_redrivable() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let repo = fixture_repo(PASSING_GATE);
    let task_file = repo.path().join("task.md");
    std::fs::write(&task_file, "do the initial thing").unwrap();
    let work = tempfile::tempdir().unwrap();
    let ledger_path = work.path().join("ledger.sqlite3");
    let runs_dir = work.path().join("runs");
    let worktree_root = work.path().join("worktrees");
    let claude = FakeClaude::new();

    let mut run_child = spawn_o7_run_process(
        repo.path(),
        &task_file,
        &ledger_path,
        &runs_dir,
        &worktree_root,
        claude.path(),
    );
    assert!(run_child.wait().unwrap().success());

    // This environment has a REAL `claude` binary on PATH (this test
    // suite runs inside a machine with Claude Code itself installed) —
    // sharing a directory with `git`/`bash`. Deleting the fixture's own
    // `claude` script would NOT simulate "the provider vanished": PATH
    // search would simply fall through past the now-empty fixture
    // directory to that real binary next in line, which would actually
    // run (and produce SOME sealed verdict, not the process-level spawn
    // failure this test needs). So `o7d` (and the `o7 continue` it
    // spawns) gets an EXPLICIT, minimal PATH here: the fixture's own
    // `claude` directory plus a dedicated directory holding ONLY symlinks
    // to the exact tools the worktree/gate machinery needs (`git`,
    // `bash`, `sh`) — deliberately excluding every other PATH entry,
    // so removing the fixture's `claude` script later genuinely leaves
    // NOTHING named `claude` anywhere this process can find.
    let safe_bin = safe_bin_dir();
    let safe_path = format!("{}:{}", claude.path().display(), safe_bin.path().display());
    let mut o7d_cmd = Command::new(o7d_bin_path());
    o7d_cmd
        .args([
            "serve",
            "--ledger",
            ledger_path.to_str().unwrap(),
            "--listen",
            "127.0.0.1:0",
            "--repo",
        ])
        .arg(repo.path())
        .arg("--worktree-root")
        .arg(&worktree_root)
        .arg("--runs-dir")
        .arg(&runs_dir)
        .arg("--o7-bin")
        .arg(env!("CARGO_BIN_EXE_o7"))
        .arg("--max-turns")
        .arg("1")
        .env("O7D_STALE_COMMAND_REDRIVE_MS", "200")
        .env("PATH", &safe_path)
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    let mut o7d_child = o7d_cmd.spawn().expect("spawn the real o7d binary");
    let stderr = o7d_child.stderr.take().unwrap();
    let mut reader = BufReader::new(stderr);
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    let addr: SocketAddr = line
        .trim()
        .strip_prefix("o7d: listening on http://")
        .unwrap()
        .parse()
        .unwrap();
    std::thread::spawn(move || {
        let mut discarded = String::new();
        while reader.read_line(&mut discarded).unwrap_or(0) > 0 {
            discarded.clear();
        }
    });
    wait_until_healthy(addr);
    let (_, page) = get(addr, "/api/v1/runs");
    let parent_run_id = page["items"][0]["run_id"].as_str().unwrap().to_owned();
    let conversation_id = page["items"][0]["conversation_id"]
        .as_str()
        .unwrap()
        .to_owned();

    // Break the provider AFTER the initial run succeeded, so the upcoming
    // command's own continuation attempt spawns for real, durably attaches
    // its child run (RunStarted, attach_run, AgentStarted), and THEN fails
    // trying to actually exec the (now-vanished) provider — a genuine
    // error well past attach, never a SIGKILL.
    std::fs::remove_file(claude.path().join("claude")).unwrap();

    let (status, accepted) = post(
        addr,
        &format!("/api/v1/conversations/{conversation_id}/commands"),
        &serde_json::json!({
            "schema_version": 1,
            "parent_run_id": parent_run_id,
            "command": "this will fail to even spawn the provider",
            "idempotency_key": "key-ordinary-error",
        }),
    );
    assert_eq!(status, 202, "{accepted:?}");
    let command_id = accepted["command_id"].as_str().unwrap().to_owned();
    let child_run_id = accepted["run_id"].as_str().unwrap().to_owned();

    // Wait for the child run to durably attach (become visible over
    // REST at all) — proving attach_run really did succeed before the
    // provider spawn then failed.
    let deadline = Instant::now() + Duration::from_secs(30);
    poll_until(deadline, || {
        let (code, _) = get(addr, &format!("/api/v1/runs/{child_run_id}"));
        (code == 200).then_some(())
    });

    // Give the doomed attempt time to actually fail and exit.
    std::thread::sleep(Duration::from_millis(800));

    let (command_status, run_status): (String, String) = {
        let conn = rusqlite::Connection::open(&ledger_path).unwrap();
        let cmd_status: String = conn
            .query_row(
                "SELECT status FROM command WHERE command_id = ?1",
                [&command_id],
                |r| r.get(0),
            )
            .unwrap();
        let run_status: String = conn
            .query_row(
                "SELECT status FROM run WHERE run_id = ?1",
                [&child_run_id],
                |r| r.get(0),
            )
            .unwrap();
        (cmd_status, run_status)
    };
    assert_ne!(
        command_status, "rejected",
        "an ordinary post-attach error must never mark the command rejected"
    );
    assert_eq!(
        command_status, "started",
        "the command must stay exactly as o7d's own bind left it"
    );
    assert_ne!(
        run_status, "completed",
        "the child run must NOT have sealed — the provider spawn genuinely failed"
    );

    // Restore the provider and prove the conversation actually recovers:
    // a retry of the SAME request, once past the staleness bound, must
    // redrive onto a FRESH run id and complete successfully.
    let restored = claude.path().join("claude");
    std::fs::write(
        &restored,
        r#"#!/bin/sh
DIR="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
N=$(( $(cat "$DIR/count" 2>/dev/null || echo 0) + 1 ))
echo "$N" > "$DIR/count"
printf '%s\0' "$@" > "$DIR/argv.$N"
touch "$DIR/invoked.$N"
printf '{"result":"synthetic ok","session_id":"fixed-session-1","total_cost_usd":0.001}\n'
exit 0
"#,
    )
    .unwrap();
    let mut perms = std::fs::metadata(&restored).unwrap().permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut perms, 0o755);
    std::fs::set_permissions(&restored, perms).unwrap();

    let deadline = Instant::now() + Duration::from_secs(30);
    let redriven_run_id = poll_until(deadline, || {
        let (status, retried) = post(
            addr,
            &format!("/api/v1/conversations/{conversation_id}/commands"),
            &serde_json::json!({
                "schema_version": 1,
                "parent_run_id": parent_run_id,
                "command": "this will fail to even spawn the provider",
                "idempotency_key": "key-ordinary-error",
            }),
        );
        if status != 202 {
            return None;
        }
        let run_id = retried["run_id"].as_str()?.to_owned();
        (run_id != child_run_id).then_some(run_id)
    });

    let deadline = Instant::now() + Duration::from_secs(30);
    wait_for_run_status(addr, &redriven_run_id, "completed", deadline);

    let _ = o7d_child.kill();
    let _ = o7d_child.wait();
}

/// Q-Deck R1 third corrective round, blocker 2: closes the TOCTOU window
/// between a redrive deciding a run id's process is dead and that SAME
/// process — merely slow to start — finally acquiring its own lock. Proves
/// the closing check directly: invoke the REAL `o7 continue` binary for a
/// run id the command has ALREADY been rebound away from (modeling
/// exactly what a genuinely-raced, merely-slow process would discover the
/// instant it gets scheduled) — it must exit cleanly and do NOTHING:
/// no canonical record, no worktree, no provider invocation.
#[test]
fn continue_exits_immediately_if_superseded_before_it_can_start_real_work() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let repo = fixture_repo(PASSING_GATE);
    let task_file = repo.path().join("task.md");
    std::fs::write(&task_file, "do the initial thing").unwrap();
    let work = tempfile::tempdir().unwrap();
    let ledger_path = work.path().join("ledger.sqlite3");
    let runs_dir = work.path().join("runs");
    let worktree_root = work.path().join("worktrees");
    let claude = FakeClaude::new();

    let mut run_child = spawn_o7_run_process(
        repo.path(),
        &task_file,
        &ledger_path,
        &runs_dir,
        &worktree_root,
        claude.path(),
    );
    assert!(run_child.wait().unwrap().success());
    assert_eq!(claude.invocation_count(), 1);

    let (parent_run_id, conversation_id) = {
        let conn = rusqlite::Connection::open(&ledger_path).unwrap();
        let row: (String, String) = conn
            .query_row("SELECT run_id, conversation_id FROM run LIMIT 1", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        row
    };

    // A command already bound to run id "superseded-run" — then REBOUND
    // to "current-run", modeling exactly the durable state a redrive
    // leaves behind while the ORIGINAL process for "superseded-run" was
    // merely slow to start.
    let superseded_run_id = "superseded-run";
    let current_run_id = "current-run";
    {
        let conn = rusqlite::Connection::open(&ledger_path).unwrap();
        conn.execute(
            "INSERT INTO command (command_id, conversation_id, parent_run_id, command_text, \
             status, child_run_id, created_at, updated_at) \
             VALUES ('cmd-superseded', ?1, ?2, 'irrelevant text', 'started', ?3, 1000, 1000)",
            rusqlite::params![conversation_id, parent_run_id, current_run_id],
        )
        .unwrap();
    }

    // Now run the REAL `o7 continue` binary for the SUPERSEDED run id —
    // exactly what the original, merely-slow process would do the moment
    // it finally gets scheduled and tries to acquire its own lock.
    let output = Command::new(env!("CARGO_BIN_EXE_o7"))
        .arg("continue")
        .arg("--repo")
        .arg(repo.path())
        .arg("--worktree-root")
        .arg(&worktree_root)
        .arg("--runs-dir")
        .arg(&runs_dir)
        .arg("--ledger")
        .arg(&ledger_path)
        .arg("--conversation-id")
        .arg(&conversation_id)
        .arg("--parent-run-id")
        .arg(&parent_run_id)
        .arg("--command")
        .arg("irrelevant text")
        .arg("--run-id")
        .arg(superseded_run_id)
        .arg("--command-id")
        .arg("cmd-superseded")
        .env(
            "PATH",
            format!(
                "{}:{}",
                claude.path().display(),
                std::env::var("PATH").unwrap_or_default()
            ),
        )
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "a superseded process must exit cleanly, not error: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // It must have done NOTHING: no run row for the superseded id, no
    // canonical record directory, no provider invocation, and the
    // command's bookkeeping (still pointing at current-run) untouched.
    assert_eq!(
        claude.invocation_count(),
        1,
        "the provider must never have been invoked again"
    );
    let conn = rusqlite::Connection::open(&ledger_path).unwrap();
    let superseded_run_exists: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM run WHERE run_id = ?1",
            [superseded_run_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        superseded_run_exists, 0,
        "a superseded process must never create a ledger row for its own (stale) run id"
    );
    let (command_status, command_child): (String, Option<String>) = conn
        .query_row(
            "SELECT status, child_run_id FROM command WHERE command_id = 'cmd-superseded'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(command_status, "started");
    assert_eq!(
        command_child.as_deref(),
        Some(current_run_id),
        "the command's binding must be untouched by the superseded process"
    );
}

/// Q-Deck R1 third corrective round, blocker 3: `meta.json` is not part of
/// the canonical event chain — a copy from a DIFFERENT run's directory
/// must never be trusted for session backfill, even though replay of the
/// canonical stream itself stays green regardless (replay never looks at
/// `meta.json` at all).
#[test]
fn recover_refuses_to_backfill_a_session_from_a_meta_json_with_the_wrong_run_id() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let repo = fixture_repo(PASSING_GATE);
    let task_file = repo.path().join("task.md");
    std::fs::write(&task_file, "do the initial thing").unwrap();
    let work = tempfile::tempdir().unwrap();
    let ledger_path = work.path().join("ledger.sqlite3");
    let runs_dir = work.path().join("runs");
    let worktree_root = work.path().join("worktrees");
    let claude = FakeClaude::new();

    let mut run_child = spawn_o7_run_process(
        repo.path(),
        &task_file,
        &ledger_path,
        &runs_dir,
        &worktree_root,
        claude.path(),
    );
    assert!(run_child.wait().unwrap().success());

    let run_id = {
        let conn = rusqlite::Connection::open(&ledger_path).unwrap();
        let id: String = conn
            .query_row("SELECT run_id FROM run LIMIT 1", [], |r| r.get(0))
            .unwrap();
        id
    };

    // Revert the ledger's own column (simulating "the live persist
    // failed") and REPLACE meta.json with one claiming a DIFFERENT run_id
    // — modeling a copy-paste mistake (or worse) rather than this run's
    // own genuine record.
    {
        let conn = rusqlite::Connection::open(&ledger_path).unwrap();
        conn.execute(
            "UPDATE run SET provider_session_id = NULL WHERE run_id = ?1",
            [&run_id],
        )
        .unwrap();
    }
    let target_name = repo
        .path()
        .file_name()
        .unwrap()
        .to_string_lossy()
        .to_string();
    let record_dir = runs_dir.join(&target_name).join(&run_id);
    let meta_path = record_dir.join("meta.json");
    let mut meta: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&meta_path).unwrap()).unwrap();
    let real_session = meta["session_id"].as_str().unwrap().to_owned();
    meta["run_id"] = serde_json::Value::String("some-other-run-entirely".to_owned());
    meta["session_id"] = serde_json::Value::String("attacker-controlled-session".to_owned());
    std::fs::write(&meta_path, serde_json::to_string(&meta).unwrap()).unwrap();

    let recover = Command::new(env!("CARGO_BIN_EXE_o7"))
        .args(["recover", "--ledger"])
        .arg(&ledger_path)
        .args(["--run-dir"])
        .arg(&record_dir)
        .output()
        .unwrap();
    assert!(
        recover.status.success(),
        "{}",
        String::from_utf8_lossy(&recover.stderr)
    );

    let after = provider_session_id(&ledger_path, &run_id);
    assert_ne!(
        after.as_deref(),
        Some("attacker-controlled-session"),
        "a meta.json claiming the WRONG run_id must never be trusted for backfill"
    );
    assert_ne!(after.as_deref(), Some(real_session.as_str()));
    assert!(
        after.is_none(),
        "backfill must be refused outright, not partially applied"
    );
}

/// Q-Deck R1 fourth corrective round: a `meta.json` whose `run_id` matches
/// exactly, but whose `schema` is one this build does not understand, must
/// be refused for session backfill on schema grounds alone — matching the
/// same "unsupported schema" refusal `ledger_binding.json` has always had.
/// Refusing backfill must NOT also refuse the canonical verdict catch-up
/// itself: the ledger's own stale (non-terminal) status must still be
/// reconciled to the record's real sealed verdict, proving the two checks
/// are independent, not one bundled all-or-nothing gate.
#[test]
fn recover_refuses_to_backfill_a_session_from_an_unsupported_meta_schema_but_still_catches_up_the_verdict(
) {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let repo = fixture_repo(PASSING_GATE);
    let task_file = repo.path().join("task.md");
    std::fs::write(&task_file, "do the initial thing").unwrap();
    let work = tempfile::tempdir().unwrap();
    let ledger_path = work.path().join("ledger.sqlite3");
    let runs_dir = work.path().join("runs");
    let worktree_root = work.path().join("worktrees");
    let claude = FakeClaude::new();

    let mut run_child = spawn_o7_run_process(
        repo.path(),
        &task_file,
        &ledger_path,
        &runs_dir,
        &worktree_root,
        claude.path(),
    );
    assert!(run_child.wait().unwrap().success());

    let run_id = {
        let conn = rusqlite::Connection::open(&ledger_path).unwrap();
        let id: String = conn
            .query_row("SELECT run_id FROM run LIMIT 1", [], |r| r.get(0))
            .unwrap();
        id
    };

    // Simulate the live path's own best-effort session persist failing
    // (`provider_session_id` still NULL) — same setup as the wrong-run-id
    // test above; the ledger's own status is untouched (already
    // `completed`, matching the canonical record), so a passing catch-up
    // here is a genuine no-op re-application, not a status transition —
    // proving the schema refusal and the (already-settled) verdict
    // catch-up are independent, neither blocking the other.
    {
        let conn = rusqlite::Connection::open(&ledger_path).unwrap();
        conn.execute(
            "UPDATE run SET provider_session_id = NULL WHERE run_id = ?1",
            [&run_id],
        )
        .unwrap();
    }
    let target_name = repo
        .path()
        .file_name()
        .unwrap()
        .to_string_lossy()
        .to_string();
    let record_dir = runs_dir.join(&target_name).join(&run_id);
    let meta_path = record_dir.join("meta.json");
    let mut meta: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&meta_path).unwrap()).unwrap();
    assert_eq!(
        meta["run_id"].as_str(),
        Some(run_id.as_str()),
        "sanity: run_id itself is correct — only schema is being corrupted"
    );
    meta["schema"] = serde_json::Value::Number(999.into());
    std::fs::write(&meta_path, serde_json::to_string(&meta).unwrap()).unwrap();

    let recover = Command::new(env!("CARGO_BIN_EXE_o7"))
        .args(["recover", "--ledger"])
        .arg(&ledger_path)
        .args(["--run-dir"])
        .arg(&record_dir)
        .output()
        .unwrap();
    assert!(
        recover.status.success(),
        "{}",
        String::from_utf8_lossy(&recover.stderr)
    );
    let stderr = String::from_utf8_lossy(&recover.stderr);
    assert!(
        stderr.contains("unsupported schema"),
        "expected an honest diagnostic naming the schema mismatch, got: {stderr}"
    );

    assert!(
        provider_session_id(&ledger_path, &run_id).is_none(),
        "an unsupported meta.json schema must never be trusted for session backfill"
    );

    let status: String = {
        let conn = rusqlite::Connection::open(&ledger_path).unwrap();
        conn.query_row("SELECT status FROM run WHERE run_id = ?1", [&run_id], |r| {
            r.get(0)
        })
        .unwrap()
    };
    assert_eq!(
        status, "completed",
        "the canonical verdict catch-up must still land even though session backfill was refused"
    );
}

/// Q-Deck R1 fourth corrective round — THE blocker-kill test: a child run
/// whose OWN canonical record is genuinely, fully sealed (the provider
/// really did run to completion), but whose ledger projection is stale
/// (still `running` — as if the live projector's own `seal()` call never
/// landed), must be RECOVERED by a same-key retry — never redriven under a
/// fresh id, never re-invoking the provider. Ledger status alone must
/// never be trusted to authorize re-invoking the provider a second time.
#[test]
fn a_sealed_canonical_record_with_a_stale_ledger_status_is_recovered_never_redriven() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let repo = fixture_repo(PASSING_GATE);
    let task_file = repo.path().join("task.md");
    std::fs::write(&task_file, "do the initial thing").unwrap();
    let work = tempfile::tempdir().unwrap();
    let ledger_path = work.path().join("ledger.sqlite3");
    let runs_dir = work.path().join("runs");
    let worktree_root = work.path().join("worktrees");
    let claude = FakeClaude::new();

    let mut run_child = spawn_o7_run_process(
        repo.path(),
        &task_file,
        &ledger_path,
        &runs_dir,
        &worktree_root,
        claude.path(),
    );
    assert!(run_child.wait().unwrap().success());
    assert_eq!(claude.invocation_count(), 1);

    let (parent_run_id, conversation_id) = {
        let conn = rusqlite::Connection::open(&ledger_path).unwrap();
        let row: (String, String) = conn
            .query_row("SELECT run_id, conversation_id FROM run LIMIT 1", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        row
    };

    // Produce a REAL, fully sealed child record for a run id this test
    // controls — invoke the real `o7 continue` binary directly, exactly
    // the pipeline `o7d` itself would have spawned, with a WORKING
    // provider. `continue_run`'s OWN binding-fence check
    // (`command_still_bound_to_this_run`) refuses to do any work at all
    // for a command id that doesn't durably exist yet, so the command row
    // — bound to this exact run id — must already be present.
    let old_run_id = "old-sealed-child";
    let command_text = "second turn";
    let idempotency_key = "key-sealed-stale";
    {
        let conn = rusqlite::Connection::open(&ledger_path).unwrap();
        let digest_input = serde_json::json!({
            "conversation_id": conversation_id,
            "parent_run_id": parent_run_id,
            "command_text": command_text,
        });
        let digest =
            o7_ledger::idempotency::digest_bytes(&serde_json::to_vec(&digest_input).unwrap());
        conn.execute(
            "INSERT INTO command (command_id, conversation_id, parent_run_id, command_text, \
             status, child_run_id, created_at, updated_at) \
             VALUES ('cmd-sealed-stale', ?1, ?2, ?3, 'started', ?4, 1000, 1000)",
            rusqlite::params![conversation_id, parent_run_id, command_text, old_run_id],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO idempotency_record (scope, key, request_digest, result_reference, \
             created_at) VALUES ('create-command', ?1, ?2, 'cmd-sealed-stale', 1000)",
            rusqlite::params![idempotency_key, digest],
        )
        .unwrap();
    }
    let output = Command::new(env!("CARGO_BIN_EXE_o7"))
        .arg("continue")
        .arg("--repo")
        .arg(repo.path())
        .arg("--worktree-root")
        .arg(&worktree_root)
        .arg("--runs-dir")
        .arg(&runs_dir)
        .arg("--ledger")
        .arg(&ledger_path)
        .arg("--conversation-id")
        .arg(&conversation_id)
        .arg("--parent-run-id")
        .arg(&parent_run_id)
        .arg("--command")
        .arg(command_text)
        .arg("--run-id")
        .arg(old_run_id)
        .arg("--command-id")
        .arg("cmd-sealed-stale")
        .env(
            "PATH",
            format!(
                "{}:{}",
                claude.path().display(),
                std::env::var("PATH").unwrap_or_default()
            ),
        )
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(claude.invocation_count(), 2);
    let status_before: String = {
        let conn = rusqlite::Connection::open(&ledger_path).unwrap();
        conn.query_row(
            "SELECT status FROM run WHERE run_id = ?1",
            [old_run_id],
            |r| r.get(0),
        )
        .unwrap()
    };
    assert_eq!(
        status_before, "completed",
        "sanity: the child run really did seal"
    );
    let command_status_before: String = {
        let conn = rusqlite::Connection::open(&ledger_path).unwrap();
        conn.query_row(
            "SELECT status FROM command WHERE command_id = 'cmd-sealed-stale'",
            [],
            |r| r.get(0),
        )
        .unwrap()
    };
    assert_eq!(
        command_status_before, "completed",
        "sanity: `o7 continue`'s own post-seal bookkeeping landed"
    );

    // Now revert the run's status, its OWN attempt's status (a real crash
    // in `projector.seal()` itself never marks either terminal — it never
    // reaches that write at all), AND the command's own bookkeeping, all
    // back to non-terminal — modeling a crash in the live projection AND
    // the post-seal bookkeeping write AFTER the canonical stream itself
    // fully sealed on disk (the flat record is never touched). Reverting
    // the run's status WITHOUT its attempt would leave an inconsistent
    // state no real crash produces: `attach_run` would find no `running`
    // attempt for a `running` run and mint a SECOND one, and re-projecting
    // `RunStarted` under that new attempt id would itself hit an
    // idempotency conflict against the original attempt's own projection.
    {
        let conn = rusqlite::Connection::open(&ledger_path).unwrap();
        conn.execute(
            "UPDATE run SET status = 'running' WHERE run_id = ?1",
            [old_run_id],
        )
        .unwrap();
        conn.execute(
            "UPDATE run_attempt SET status = 'running', finished_at = NULL \
             WHERE run_id = ?1",
            [old_run_id],
        )
        .unwrap();
        conn.execute(
            "UPDATE command SET status = 'started', updated_at = 1000 \
             WHERE command_id = 'cmd-sealed-stale'",
            [],
        )
        .unwrap();
    }

    let (mut o7d_child, addr) = spawn_o7d_with_exec_and_redrive_ms(
        &ledger_path,
        repo.path(),
        &worktree_root,
        &runs_dir,
        claude.path(),
        Some(200),
    );
    wait_until_healthy(addr);

    let (status, retried) = post(
        addr,
        &format!("/api/v1/conversations/{conversation_id}/commands"),
        &serde_json::json!({
            "schema_version": 1,
            "parent_run_id": parent_run_id,
            "command": command_text,
            "idempotency_key": idempotency_key,
        }),
    );
    assert_eq!(status, 202, "{retried:?}");
    assert_eq!(
        retried["run_id"], old_run_id,
        "a sealed canonical record must be RECOVERED under its OWN run id, never redriven under \
         a fresh one"
    );

    let deadline = Instant::now() + Duration::from_secs(15);
    poll_until(deadline, || {
        let conn = rusqlite::Connection::open(&ledger_path).unwrap();
        let s: String = conn
            .query_row(
                "SELECT status FROM run WHERE run_id = ?1",
                [old_run_id],
                |r| r.get(0),
            )
            .unwrap();
        (s == "completed").then_some(())
    });

    assert_eq!(
        claude.invocation_count(),
        2,
        "the provider must NEVER be invoked again for an already-sealed child record"
    );

    let (command_status, command_child): (String, Option<String>) = {
        let conn = rusqlite::Connection::open(&ledger_path).unwrap();
        conn.query_row(
            "SELECT status, child_run_id FROM command WHERE command_id = 'cmd-sealed-stale'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap()
    };
    assert_eq!(command_status, "completed");
    assert_eq!(command_child.as_deref(), Some(old_run_id));

    let _ = o7d_child.kill();
    let _ = o7d_child.wait();
}

/// Q-Deck R1 fourth corrective round: a child run's canonical record that
/// LOOKS sealed but fails independent verification (its `diff.patch`
/// tampered with after the fact) must fail closed — a stable, typed error,
/// never silently redriven (which would double-invoke the provider) and
/// never silently recovered (which would trust a corrupted verdict). The
/// command's own binding/status must be left exactly as found, for manual
/// investigation.
#[test]
fn a_tampered_sealed_record_fails_closed_never_redriven_never_recovered() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let repo = fixture_repo(PASSING_GATE);
    let task_file = repo.path().join("task.md");
    std::fs::write(&task_file, "do the initial thing").unwrap();
    let work = tempfile::tempdir().unwrap();
    let ledger_path = work.path().join("ledger.sqlite3");
    let runs_dir = work.path().join("runs");
    let worktree_root = work.path().join("worktrees");
    let claude = FakeClaude::new();

    let mut run_child = spawn_o7_run_process(
        repo.path(),
        &task_file,
        &ledger_path,
        &runs_dir,
        &worktree_root,
        claude.path(),
    );
    assert!(run_child.wait().unwrap().success());
    assert_eq!(claude.invocation_count(), 1);

    let (parent_run_id, conversation_id) = {
        let conn = rusqlite::Connection::open(&ledger_path).unwrap();
        let row: (String, String) = conn
            .query_row("SELECT run_id, conversation_id FROM run LIMIT 1", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        row
    };

    let old_run_id = "old-tampered-child";
    let command_text = "second turn";
    let idempotency_key = "key-tampered";
    {
        let conn = rusqlite::Connection::open(&ledger_path).unwrap();
        let digest_input = serde_json::json!({
            "conversation_id": conversation_id,
            "parent_run_id": parent_run_id,
            "command_text": command_text,
        });
        let digest =
            o7_ledger::idempotency::digest_bytes(&serde_json::to_vec(&digest_input).unwrap());
        conn.execute(
            "INSERT INTO command (command_id, conversation_id, parent_run_id, command_text, \
             status, child_run_id, created_at, updated_at) \
             VALUES ('cmd-tampered', ?1, ?2, ?3, 'started', ?4, 1000, 1000)",
            rusqlite::params![conversation_id, parent_run_id, command_text, old_run_id],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO idempotency_record (scope, key, request_digest, result_reference, \
             created_at) VALUES ('create-command', ?1, ?2, 'cmd-tampered', 1000)",
            rusqlite::params![idempotency_key, digest],
        )
        .unwrap();
    }
    let output = Command::new(env!("CARGO_BIN_EXE_o7"))
        .arg("continue")
        .arg("--repo")
        .arg(repo.path())
        .arg("--worktree-root")
        .arg(&worktree_root)
        .arg("--runs-dir")
        .arg(&runs_dir)
        .arg("--ledger")
        .arg(&ledger_path)
        .arg("--conversation-id")
        .arg(&conversation_id)
        .arg("--parent-run-id")
        .arg(&parent_run_id)
        .arg("--command")
        .arg(command_text)
        .arg("--run-id")
        .arg(old_run_id)
        .arg("--command-id")
        .arg("cmd-tampered")
        .env(
            "PATH",
            format!(
                "{}:{}",
                claude.path().display(),
                std::env::var("PATH").unwrap_or_default()
            ),
        )
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(claude.invocation_count(), 2);

    // Tamper an artifact AFTER the record sealed — the same class of
    // post-seal edit `o7-run`'s own replay-acceptance suite proves `replay`
    // itself rejects.
    let target_name = repo
        .path()
        .file_name()
        .unwrap()
        .to_string_lossy()
        .to_string();
    let record_dir = runs_dir.join(&target_name).join(old_run_id);
    std::fs::write(
        record_dir.join("diff.patch"),
        b"totally different patch bytes",
    )
    .unwrap();

    // Revert to stale bookkeeping, exactly like the sealed-recovery test —
    // this run's own record LOOKS like the blocker-kill scenario, except
    // its evidence no longer verifies.
    {
        let conn = rusqlite::Connection::open(&ledger_path).unwrap();
        conn.execute(
            "UPDATE run SET status = 'running' WHERE run_id = ?1",
            [old_run_id],
        )
        .unwrap();
        conn.execute(
            "UPDATE run_attempt SET status = 'running', finished_at = NULL \
             WHERE run_id = ?1",
            [old_run_id],
        )
        .unwrap();
        conn.execute(
            "UPDATE command SET status = 'started', updated_at = 1000 \
             WHERE command_id = 'cmd-tampered'",
            [],
        )
        .unwrap();
    }

    let (mut o7d_child, addr) = spawn_o7d_with_exec_and_redrive_ms(
        &ledger_path,
        repo.path(),
        &worktree_root,
        &runs_dir,
        claude.path(),
        Some(200),
    );
    wait_until_healthy(addr);

    let (status, retried) = post(
        addr,
        &format!("/api/v1/conversations/{conversation_id}/commands"),
        &serde_json::json!({
            "schema_version": 1,
            "parent_run_id": parent_run_id,
            "command": command_text,
            "idempotency_key": idempotency_key,
        }),
    );
    assert_eq!(
        status, 500,
        "a tampered sealed record must fail closed with a stable error, not a 202: {retried:?}"
    );
    assert_eq!(retried["code"], "COMMAND_CANONICAL_RECORD_INVALID");

    // Give any (incorrect) redrive/recovery attempt a real chance to
    // happen before asserting it didn't.
    std::thread::sleep(Duration::from_millis(500));
    assert_eq!(
        claude.invocation_count(),
        2,
        "the provider must never be invoked for a tampered record"
    );

    let (command_status, command_child): (String, Option<String>) = {
        let conn = rusqlite::Connection::open(&ledger_path).unwrap();
        conn.query_row(
            "SELECT status, child_run_id FROM command WHERE command_id = 'cmd-tampered'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap()
    };
    assert_eq!(
        command_status, "started",
        "the command must be left discoverable, never silently rejected or completed"
    );
    assert_eq!(command_child.as_deref(), Some(old_run_id));

    let _ = o7d_child.kill();
    let _ = o7d_child.wait();
}

/// Q-Deck R1 fourth corrective round: a child run whose canonical record is
/// a genuinely valid but UNSEALED prefix (the process crashed before ever
/// reaching the agent — a real, verified `RunStarted` on disk, nothing
/// more) must be redriven exactly once, under a FRESH child run id, and the
/// old partial record must be left byte-identical (never resumed, never
/// rewritten).
#[test]
fn a_valid_unsealed_prefix_is_redriven_exactly_once_leaving_the_old_record_untouched() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let repo = fixture_repo(PASSING_GATE);
    let task_file = repo.path().join("task.md");
    std::fs::write(&task_file, "do the initial thing").unwrap();
    let work = tempfile::tempdir().unwrap();
    let ledger_path = work.path().join("ledger.sqlite3");
    let runs_dir = work.path().join("runs");
    let worktree_root = work.path().join("worktrees");
    let claude = FakeClaude::new();
    let safe_bin = safe_bin_dir();

    let mut run_child = spawn_o7_run_process(
        repo.path(),
        &task_file,
        &ledger_path,
        &runs_dir,
        &worktree_root,
        claude.path(),
    );
    assert!(run_child.wait().unwrap().success());
    assert_eq!(claude.invocation_count(), 1);

    let (parent_run_id, conversation_id) = {
        let conn = rusqlite::Connection::open(&ledger_path).unwrap();
        let row: (String, String) = conn
            .query_row("SELECT run_id, conversation_id FROM run LIMIT 1", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        row
    };

    let old_run_id = "old-unsealed-child";
    let command_text = "second turn";
    let idempotency_key = "key-unsealed";
    {
        let conn = rusqlite::Connection::open(&ledger_path).unwrap();
        let digest_input = serde_json::json!({
            "conversation_id": conversation_id,
            "parent_run_id": parent_run_id,
            "command_text": command_text,
        });
        let digest =
            o7_ledger::idempotency::digest_bytes(&serde_json::to_vec(&digest_input).unwrap());
        conn.execute(
            "INSERT INTO command (command_id, conversation_id, parent_run_id, command_text, \
             status, child_run_id, created_at, updated_at) \
             VALUES ('cmd-unsealed', ?1, ?2, ?3, 'started', ?4, 1000, 1000)",
            rusqlite::params![conversation_id, parent_run_id, command_text, old_run_id],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO idempotency_record (scope, key, request_digest, result_reference, \
             created_at) VALUES ('create-command', ?1, ?2, 'cmd-unsealed', 1000)",
            rusqlite::params![idempotency_key, digest],
        )
        .unwrap();
    }

    // A genuinely unavailable provider (PATH excludes any `claude`
    // whatsoever — see `safe_bin_dir`'s own doc comment on why deleting
    // the fixture script alone isn't enough) makes `continue_execute`'s
    // own agent call fail right after durably appending `RunStarted` (and
    // `AgentStarted`) — a real, verified, chain-valid, UNSEALED prefix.
    let output = Command::new(env!("CARGO_BIN_EXE_o7"))
        .arg("continue")
        .arg("--repo")
        .arg(repo.path())
        .arg("--worktree-root")
        .arg(&worktree_root)
        .arg("--runs-dir")
        .arg(&runs_dir)
        .arg("--ledger")
        .arg(&ledger_path)
        .arg("--conversation-id")
        .arg(&conversation_id)
        .arg("--parent-run-id")
        .arg(&parent_run_id)
        .arg("--command")
        .arg(command_text)
        .arg("--run-id")
        .arg(old_run_id)
        .arg("--command-id")
        .arg("cmd-unsealed")
        .env("PATH", safe_bin.path())
        .output()
        .unwrap();
    assert!(
        !output.status.success(),
        "the provider being unavailable must fail this continuation, not silently succeed"
    );
    assert_eq!(
        claude.invocation_count(),
        1,
        "the unavailable-provider attempt must never have reached the real fixture"
    );

    let target_name = repo
        .path()
        .file_name()
        .unwrap()
        .to_string_lossy()
        .to_string();
    let record_dir = runs_dir.join(&target_name).join(old_run_id);
    let events_before = std::fs::read_to_string(record_dir.join("events.jsonl")).unwrap();
    assert!(
        !events_before.contains("run_sealed"),
        "sanity: the old record must genuinely be UNSEALED"
    );
    let (old_status, old_command_status): (String, String) = {
        let conn = rusqlite::Connection::open(&ledger_path).unwrap();
        let s: String = conn
            .query_row(
                "SELECT status FROM run WHERE run_id = ?1",
                [old_run_id],
                |r| r.get(0),
            )
            .unwrap();
        let cs: String = conn
            .query_row(
                "SELECT status FROM command WHERE command_id = 'cmd-unsealed'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        (s, cs)
    };
    assert_ne!(
        old_status, "completed",
        "sanity: really not sealed in the ledger either"
    );
    assert_eq!(old_command_status, "started");

    let (mut o7d_child, addr) = spawn_o7d_with_exec_and_redrive_ms(
        &ledger_path,
        repo.path(),
        &worktree_root,
        &runs_dir,
        claude.path(),
        Some(200),
    );
    wait_until_healthy(addr);

    let (status, retried) = post(
        addr,
        &format!("/api/v1/conversations/{conversation_id}/commands"),
        &serde_json::json!({
            "schema_version": 1,
            "parent_run_id": parent_run_id,
            "command": command_text,
            "idempotency_key": idempotency_key,
        }),
    );
    assert_eq!(status, 202, "{retried:?}");
    let redriven_run_id = retried["run_id"].as_str().unwrap().to_owned();
    assert_ne!(
        redriven_run_id, old_run_id,
        "a genuinely unsealed prefix must be redriven under a FRESH child run id"
    );

    let deadline = Instant::now() + Duration::from_secs(30);
    claude.wait_for_invocation(2, deadline);
    let deadline = Instant::now() + Duration::from_secs(30);
    wait_for_run_status(addr, &redriven_run_id, "completed", deadline);

    assert_eq!(
        claude.invocation_count(),
        2,
        "the provider must be invoked exactly once for the fresh child, on top of the one \
         failed attempt against the old (unsealed) child"
    );

    let events_after = std::fs::read_to_string(record_dir.join("events.jsonl")).unwrap();
    assert_eq!(
        events_before, events_after,
        "the old, superseded partial record must be left byte-identical — never resumed, \
         never rewritten"
    );

    let _ = o7d_child.kill();
    let _ = o7d_child.wait();
}

/// Q-Deck R1 fourth corrective round (Part 5): a spawn failure right after
/// a SUCCESSFUL atomic CAS rebind (the fresh child run id is durably
/// bound, but `o7 continue` itself never even started) must NOT reject the
/// command — the fresh binding stays `started`, and the NEXT retry (once
/// stale again) classifies that SAME fresh run id (still `Absent` — it
/// never got a canonical record) and redrives it again. The conversation
/// must never become permanently wedged by an ordinary spawn failure.
#[test]
fn a_spawn_failure_right_after_rebind_leaves_the_command_recoverable() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let repo = fixture_repo(PASSING_GATE);
    let task_file = repo.path().join("task.md");
    std::fs::write(&task_file, "do the initial thing").unwrap();
    let work = tempfile::tempdir().unwrap();
    let ledger_path = work.path().join("ledger.sqlite3");
    let runs_dir = work.path().join("runs");
    let worktree_root = work.path().join("worktrees");
    let claude = FakeClaude::new();

    let mut run_child = spawn_o7_run_process(
        repo.path(),
        &task_file,
        &ledger_path,
        &runs_dir,
        &worktree_root,
        claude.path(),
    );
    assert!(run_child.wait().unwrap().success());
    assert_eq!(claude.invocation_count(), 1);

    let (parent_run_id, conversation_id) = {
        let conn = rusqlite::Connection::open(&ledger_path).unwrap();
        let row: (String, String) = conn
            .query_row("SELECT run_id, conversation_id FROM run LIMIT 1", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        row
    };

    // A command already durably `started`/bound, its child run id's own
    // ledger row never having landed — the exact same "stuck before
    // dispatch" state the existing redrive test builds, reused here so
    // this test is ONLY about what happens after the spawn itself fails.
    let command_text = "spawn failure check";
    let idempotency_key = "key-spawn-failure";
    let child_run_id_before_redrive = o7_ledger::RunId::generate();
    let stale_updated_at = 1000i64;
    {
        let conn = rusqlite::Connection::open(&ledger_path).unwrap();
        let digest_input = serde_json::json!({
            "conversation_id": conversation_id,
            "parent_run_id": parent_run_id,
            "command_text": command_text,
        });
        let digest =
            o7_ledger::idempotency::digest_bytes(&serde_json::to_vec(&digest_input).unwrap());
        conn.execute(
            "INSERT INTO command (command_id, conversation_id, parent_run_id, command_text, \
             status, child_run_id, created_at, updated_at) \
             VALUES ('cmd-spawn-failure', ?1, ?2, ?3, 'started', ?4, 1000, ?5)",
            rusqlite::params![
                conversation_id,
                parent_run_id,
                command_text,
                child_run_id_before_redrive.as_str(),
                stale_updated_at,
            ],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO idempotency_record (scope, key, request_digest, result_reference, \
             created_at) VALUES ('create-command', ?1, ?2, 'cmd-spawn-failure', 1000)",
            rusqlite::params![idempotency_key, digest],
        )
        .unwrap();
    }

    // First `o7d`: a deliberately BOGUS `--o7-bin` — the CAS rebind itself
    // (a pure ledger write) must still succeed; only the subsequent
    // `std::process::Command::spawn()` for `o7 continue` fails (`ENOENT`).
    let mut o7d_cmd = Command::new(o7d_bin_path());
    o7d_cmd
        .args([
            "serve",
            "--ledger",
            ledger_path.to_str().unwrap(),
            "--listen",
            "127.0.0.1:0",
            "--repo",
        ])
        .arg(repo.path())
        .arg("--worktree-root")
        .arg(&worktree_root)
        .arg("--runs-dir")
        .arg(&runs_dir)
        .arg("--o7-bin")
        .arg("/nonexistent/o7-binary-does-not-exist")
        .arg("--max-turns")
        .arg("1")
        .env("O7D_STALE_COMMAND_REDRIVE_MS", "200")
        .env(
            "PATH",
            format!(
                "{}:{}",
                claude.path().display(),
                std::env::var("PATH").unwrap_or_default()
            ),
        )
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    let mut o7d_child = o7d_cmd.spawn().expect("spawn the real o7d binary");
    let stderr = o7d_child.stderr.take().unwrap();
    let mut reader = BufReader::new(stderr);
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    let addr: SocketAddr = line
        .trim()
        .strip_prefix("o7d: listening on http://")
        .unwrap()
        .parse()
        .unwrap();
    std::thread::spawn(move || {
        let mut discarded = String::new();
        while reader.read_line(&mut discarded).unwrap_or(0) > 0 {
            discarded.clear();
        }
    });
    wait_until_healthy(addr);

    let (status, first_retry) = post(
        addr,
        &format!("/api/v1/conversations/{conversation_id}/commands"),
        &serde_json::json!({
            "schema_version": 1,
            "parent_run_id": parent_run_id,
            "command": command_text,
            "idempotency_key": idempotency_key,
        }),
    );
    // The rebind itself is a pure ledger write and succeeds regardless of
    // whether the subsequent spawn does — this request still durably
    // accepts/rebinds and answers normally.
    assert_eq!(status, 202, "{first_retry:?}");
    let first_fresh_run_id = first_retry["run_id"].as_str().unwrap().to_owned();
    assert_ne!(first_fresh_run_id, child_run_id_before_redrive.as_str());

    let (command_status, command_child): (String, Option<String>) = {
        let conn = rusqlite::Connection::open(&ledger_path).unwrap();
        conn.query_row(
            "SELECT status, child_run_id FROM command WHERE command_id = 'cmd-spawn-failure'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap()
    };
    assert_eq!(
        command_status, "started",
        "a spawn failure right after a successful rebind must NOT reject the command"
    );
    assert_eq!(command_child.as_deref(), Some(first_fresh_run_id.as_str()));

    let _ = o7d_child.kill();
    let _ = o7d_child.wait();

    // Give this fresh (never-actually-started) attempt time to become
    // stale again, then retry against a SECOND `o7d` with a WORKING
    // `--o7-bin` — proving the conversation is not permanently wedged.
    std::thread::sleep(Duration::from_millis(400));
    let (mut o7d_child2, addr2) = spawn_o7d_with_exec_and_redrive_ms(
        &ledger_path,
        repo.path(),
        &worktree_root,
        &runs_dir,
        claude.path(),
        Some(200),
    );
    wait_until_healthy(addr2);

    let (status2, second_retry) = post(
        addr2,
        &format!("/api/v1/conversations/{conversation_id}/commands"),
        &serde_json::json!({
            "schema_version": 1,
            "parent_run_id": parent_run_id,
            "command": command_text,
            "idempotency_key": idempotency_key,
        }),
    );
    assert_eq!(status2, 202, "{second_retry:?}");
    let second_fresh_run_id = second_retry["run_id"].as_str().unwrap().to_owned();
    assert_ne!(
        second_fresh_run_id, first_fresh_run_id,
        "the never-started fresh attempt must itself be redriven, under ANOTHER fresh id"
    );

    let deadline = Instant::now() + Duration::from_secs(30);
    claude.wait_for_invocation(2, deadline);
    let deadline = Instant::now() + Duration::from_secs(30);
    wait_for_run_status(addr2, &second_fresh_run_id, "completed", deadline);
    assert_eq!(
        claude.invocation_count(),
        2,
        "the provider must be invoked exactly once for THIS command's own continuation, on top \
         of the one earlier invocation for the initial setup run — the first (bogus-o7-bin) \
         spawn attempt never even started a process"
    );

    let _ = o7d_child2.kill();
    let _ = o7d_child2.wait();
}

/// Q-Deck R1 fourth corrective round (Part 3): two concurrent same-key
/// retries against a stuck command must produce AT MOST one successful
/// atomic CAS rebind — never two fresh child run ids, never two provider
/// invocations. The CAS loser must report the WINNER's authoritative fresh
/// run id, never a stale value of its own.
#[test]
fn two_concurrent_same_key_retries_produce_exactly_one_cas_winner() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let repo = fixture_repo(PASSING_GATE);
    let task_file = repo.path().join("task.md");
    std::fs::write(&task_file, "do the initial thing").unwrap();
    let work = tempfile::tempdir().unwrap();
    let ledger_path = work.path().join("ledger.sqlite3");
    let runs_dir = work.path().join("runs");
    let worktree_root = work.path().join("worktrees");
    let claude = FakeClaude::new();

    let mut run_child = spawn_o7_run_process(
        repo.path(),
        &task_file,
        &ledger_path,
        &runs_dir,
        &worktree_root,
        claude.path(),
    );
    assert!(run_child.wait().unwrap().success());
    assert_eq!(claude.invocation_count(), 1);

    let (parent_run_id, conversation_id) = {
        let conn = rusqlite::Connection::open(&ledger_path).unwrap();
        let row: (String, String) = conn
            .query_row("SELECT run_id, conversation_id FROM run LIMIT 1", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        row
    };

    let command_text = "concurrent redrive check";
    let idempotency_key = "key-concurrent";
    let child_run_id_before_redrive = o7_ledger::RunId::generate();
    let stale_updated_at = 1000i64;
    {
        let conn = rusqlite::Connection::open(&ledger_path).unwrap();
        let digest_input = serde_json::json!({
            "conversation_id": conversation_id,
            "parent_run_id": parent_run_id,
            "command_text": command_text,
        });
        let digest =
            o7_ledger::idempotency::digest_bytes(&serde_json::to_vec(&digest_input).unwrap());
        conn.execute(
            "INSERT INTO command (command_id, conversation_id, parent_run_id, command_text, \
             status, child_run_id, created_at, updated_at) \
             VALUES ('cmd-concurrent', ?1, ?2, ?3, 'started', ?4, 1000, ?5)",
            rusqlite::params![
                conversation_id,
                parent_run_id,
                command_text,
                child_run_id_before_redrive.as_str(),
                stale_updated_at,
            ],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO idempotency_record (scope, key, request_digest, result_reference, \
             created_at) VALUES ('create-command', ?1, ?2, 'cmd-concurrent', 1000)",
            rusqlite::params![idempotency_key, digest],
        )
        .unwrap();
    }

    // A deliberately GENEROUS staleness bound — unlike every other test
    // here, this one needs the freshly-rebound attempt to NOT look stale
    // again by the time the second concurrent request's own processing
    // reaches the server (real subprocess-spawn/scheduling latency on a
    // loaded single-core VPS can easily exceed a tight 200ms window),
    // which would otherwise legitimately trigger a SECOND, independent
    // redrive of the first request's own (still-attaching) fresh attempt —
    // a different, already-covered scenario, not the one this test proves.
    let (mut o7d_child, addr) = spawn_o7d_with_exec_and_redrive_ms(
        &ledger_path,
        repo.path(),
        &worktree_root,
        &runs_dir,
        claude.path(),
        Some(5000),
    );
    wait_until_healthy(addr);

    let body = serde_json::json!({
        "schema_version": 1,
        "parent_run_id": parent_run_id,
        "command": command_text,
        "idempotency_key": idempotency_key,
    });
    let body_a = body.clone();
    let conversation_id_a = conversation_id.clone();
    let handle_a = std::thread::spawn(move || {
        post(
            addr,
            &format!("/api/v1/conversations/{conversation_id_a}/commands"),
            &body_a,
        )
    });
    let handle_b = std::thread::spawn(move || {
        post(
            addr,
            &format!("/api/v1/conversations/{conversation_id}/commands"),
            &body,
        )
    });
    let (status_a, retried_a) = handle_a.join().unwrap();
    let (status_b, retried_b) = handle_b.join().unwrap();

    assert_eq!(status_a, 202, "{retried_a:?}");
    assert_eq!(status_b, 202, "{retried_b:?}");
    let run_id_a = retried_a["run_id"].as_str().unwrap().to_owned();
    let run_id_b = retried_b["run_id"].as_str().unwrap().to_owned();
    assert_eq!(
        run_id_a, run_id_b,
        "both concurrent retries must report the SAME authoritative fresh child run id — no \
         stale response from the CAS loser"
    );
    assert_ne!(run_id_a, child_run_id_before_redrive.as_str());

    let deadline = Instant::now() + Duration::from_secs(30);
    claude.wait_for_invocation(2, deadline);
    let deadline = Instant::now() + Duration::from_secs(30);
    wait_for_run_status(addr, &run_id_a, "completed", deadline);

    // Give any (incorrect) second redrive a real chance to happen before
    // asserting it didn't.
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(
        claude.invocation_count(),
        2,
        "exactly one provider invocation for the redrive, never two"
    );

    let (command_status, command_child): (String, Option<String>) = {
        let conn = rusqlite::Connection::open(&ledger_path).unwrap();
        conn.query_row(
            "SELECT status, child_run_id FROM command WHERE command_id = 'cmd-concurrent'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap()
    };
    assert_eq!(command_status, "completed");
    assert_eq!(command_child.as_deref(), Some(run_id_a.as_str()));

    let _ = o7d_child.kill();
    let _ = o7d_child.wait();
}

/// Q-Deck R1 fourth corrective round: "sealed canonical record with a
/// missing ledger row" cannot actually arise. Ordering proof: a canonical
/// stream only reaches `RunSealed` if `continue_execute` got PAST
/// `attach_run` (it happens right after the durable `RunStarted` append,
/// via `?` — any earlier failure returns before appending anything
/// further, including `RunSealed`), and `attach_run`'s own
/// `create_run_with_id` call is what creates the ledger row in the first
/// place. So the state this test instead builds — a durable, chain-valid
/// `RunStarted`-only prefix with NO ledger row at all — is the adjacent
/// crash window the proof depends on: crashed between the durable
/// `RunStarted` append and `attach_run` ever running. It must classify as
/// `ValidUnsealed` (never `Absent`, never `Invalid`), and a same-key retry
/// must redrive it normally under a fresh id, leaving this hand-crafted
/// prefix byte-identical.
#[test]
fn a_run_started_only_prefix_with_no_ledger_row_is_valid_unsealed_and_redriven() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let repo = fixture_repo(PASSING_GATE);
    let task_file = repo.path().join("task.md");
    std::fs::write(&task_file, "do the initial thing").unwrap();
    let work = tempfile::tempdir().unwrap();
    let ledger_path = work.path().join("ledger.sqlite3");
    let runs_dir = work.path().join("runs");
    let worktree_root = work.path().join("worktrees");
    let claude = FakeClaude::new();

    let mut run_child = spawn_o7_run_process(
        repo.path(),
        &task_file,
        &ledger_path,
        &runs_dir,
        &worktree_root,
        claude.path(),
    );
    assert!(run_child.wait().unwrap().success());
    assert_eq!(claude.invocation_count(), 1);

    let (parent_run_id, conversation_id) = {
        let conn = rusqlite::Connection::open(&ledger_path).unwrap();
        let row: (String, String) = conn
            .query_row("SELECT run_id, conversation_id FROM run LIMIT 1", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        row
    };

    // Hand-construct EXACTLY the durable state a crash between
    // `continue_execute`'s durable `RunStarted` append and its very next
    // line (`attach_run`) would leave behind: `ledger_binding.json`,
    // `task.md`, and a one-event, chain-valid `events.jsonl` — using the
    // SAME public helpers `continue_execute` itself calls, never a
    // hand-rolled shortcut format.
    let old_run_id = "old-preattach-crash";
    let command_text = "second turn";
    let target_name = repo
        .path()
        .file_name()
        .unwrap()
        .to_string_lossy()
        .to_string();
    let rec = o7::record::RunRecord::create(&runs_dir, &target_name, old_run_id).unwrap();
    o7::record::LedgerBinding {
        schema: 1,
        run_id: old_run_id.to_owned(),
        conversation_id: conversation_id.clone(),
        agent: "claude".to_owned(),
        role: "implementer".to_owned(),
        parent_run_id: Some(parent_run_id.clone()),
    }
    .write_durable(&rec)
    .unwrap();
    rec.write_task_durable(command_text).unwrap();
    let manifest = o7::gate::GateManifest::load(&repo.path().join(".007/gate.toml")).unwrap();
    let contract = o7::events::build_contract(&manifest).unwrap();
    let task_ref = o7::events::artifact(
        o7_run::event::ArtifactKind::Task,
        "task.md",
        command_text.as_bytes(),
    );
    let canonical_run_id = o7_run::ids::RunId::new(old_run_id).unwrap();
    let mut chain = o7::events::EventChain::new(canonical_run_id);
    let run_started = chain
        .push(o7_run::event::RunEventKind::RunStarted {
            contract,
            task: task_ref,
        })
        .unwrap();
    let events_before = o7::events::to_jsonl(&[run_started]).unwrap();
    std::fs::write(rec.dir.join("events.jsonl"), &events_before).unwrap();

    // Sanity: genuinely no ledger row for this run id — never touched.
    let no_ledger_row: i64 = {
        let conn = rusqlite::Connection::open(&ledger_path).unwrap();
        conn.query_row(
            "SELECT COUNT(*) FROM run WHERE run_id = ?1",
            [old_run_id],
            |r| r.get(0),
        )
        .unwrap()
    };
    assert_eq!(no_ledger_row, 0);

    let idempotency_key = "key-preattach-crash";
    {
        let conn = rusqlite::Connection::open(&ledger_path).unwrap();
        let digest_input = serde_json::json!({
            "conversation_id": conversation_id,
            "parent_run_id": parent_run_id,
            "command_text": command_text,
        });
        let digest =
            o7_ledger::idempotency::digest_bytes(&serde_json::to_vec(&digest_input).unwrap());
        conn.execute(
            "INSERT INTO command (command_id, conversation_id, parent_run_id, command_text, \
             status, child_run_id, created_at, updated_at) \
             VALUES ('cmd-preattach', ?1, ?2, ?3, 'started', ?4, 1000, 1000)",
            rusqlite::params![conversation_id, parent_run_id, command_text, old_run_id],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO idempotency_record (scope, key, request_digest, result_reference, \
             created_at) VALUES ('create-command', ?1, ?2, 'cmd-preattach', 1000)",
            rusqlite::params![idempotency_key, digest],
        )
        .unwrap();
    }

    let (mut o7d_child, addr) = spawn_o7d_with_exec_and_redrive_ms(
        &ledger_path,
        repo.path(),
        &worktree_root,
        &runs_dir,
        claude.path(),
        Some(200),
    );
    wait_until_healthy(addr);

    let (status, retried) = post(
        addr,
        &format!("/api/v1/conversations/{conversation_id}/commands"),
        &serde_json::json!({
            "schema_version": 1,
            "parent_run_id": parent_run_id,
            "command": command_text,
            "idempotency_key": idempotency_key,
        }),
    );
    assert_eq!(status, 202, "{retried:?}");
    let redriven_run_id = retried["run_id"].as_str().unwrap().to_owned();
    assert_ne!(
        redriven_run_id, old_run_id,
        "a valid unsealed prefix with no ledger row must STILL be redriven under a fresh id, \
         exactly like the missing-ledger-row-and-no-record case"
    );

    let deadline = Instant::now() + Duration::from_secs(30);
    claude.wait_for_invocation(2, deadline);
    let deadline = Instant::now() + Duration::from_secs(30);
    wait_for_run_status(addr, &redriven_run_id, "completed", deadline);
    assert_eq!(claude.invocation_count(), 2);

    let events_after = std::fs::read_to_string(rec.dir.join("events.jsonl")).unwrap();
    assert_eq!(
        events_before, events_after,
        "the hand-crafted pre-attach prefix must be left byte-identical"
    );
    let still_no_ledger_row: i64 = {
        let conn = rusqlite::Connection::open(&ledger_path).unwrap();
        conn.query_row(
            "SELECT COUNT(*) FROM run WHERE run_id = ?1",
            [old_run_id],
            |r| r.get(0),
        )
        .unwrap()
    };
    assert_eq!(
        still_no_ledger_row, 0,
        "the old (superseded) run id must never retroactively gain a ledger row either"
    );

    let _ = o7d_child.kill();
    let _ = o7d_child.wait();
}

/// Q-Deck R1 fourth corrective round (Part 2, "delayed old process during
/// held-lock rebind"): a genuinely redriven command's OLD child run id must
/// stay permanently harmless — even if the original (merely slow, not
/// dead) `o7 continue` process for it finally wakes up and runs AFTER the
/// redrive has already completed and the fresh child has already sealed.
/// Ties the redrive path and `continue_run`'s own child-side binding fence
/// together in one full round trip (each already proven in isolation by
/// `a_command_stuck_before_dispatch_is_redriven_by_a_retry_after_the_stale_bound`
/// and `continue_exits_immediately_if_superseded_before_it_can_start_real_work`
/// respectively).
#[test]
fn a_delayed_original_process_is_harmless_after_its_run_id_is_redriven_and_completes() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let repo = fixture_repo(PASSING_GATE);
    let task_file = repo.path().join("task.md");
    std::fs::write(&task_file, "do the initial thing").unwrap();
    let work = tempfile::tempdir().unwrap();
    let ledger_path = work.path().join("ledger.sqlite3");
    let runs_dir = work.path().join("runs");
    let worktree_root = work.path().join("worktrees");
    let claude = FakeClaude::new();

    let mut run_child = spawn_o7_run_process(
        repo.path(),
        &task_file,
        &ledger_path,
        &runs_dir,
        &worktree_root,
        claude.path(),
    );
    assert!(run_child.wait().unwrap().success());
    assert_eq!(claude.invocation_count(), 1);

    let (parent_run_id, conversation_id) = {
        let conn = rusqlite::Connection::open(&ledger_path).unwrap();
        let row: (String, String) = conn
            .query_row("SELECT run_id, conversation_id FROM run LIMIT 1", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        row
    };

    let command_text = "delayed original check";
    let idempotency_key = "key-delayed-original";
    let old_run_id = o7_ledger::RunId::generate();
    let stale_updated_at = 1000i64;
    {
        let conn = rusqlite::Connection::open(&ledger_path).unwrap();
        let digest_input = serde_json::json!({
            "conversation_id": conversation_id,
            "parent_run_id": parent_run_id,
            "command_text": command_text,
        });
        let digest =
            o7_ledger::idempotency::digest_bytes(&serde_json::to_vec(&digest_input).unwrap());
        conn.execute(
            "INSERT INTO command (command_id, conversation_id, parent_run_id, command_text, \
             status, child_run_id, created_at, updated_at) \
             VALUES ('cmd-delayed-original', ?1, ?2, ?3, 'started', ?4, 1000, ?5)",
            rusqlite::params![
                conversation_id,
                parent_run_id,
                command_text,
                old_run_id.as_str(),
                stale_updated_at,
            ],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO idempotency_record (scope, key, request_digest, result_reference, \
             created_at) VALUES ('create-command', ?1, ?2, 'cmd-delayed-original', 1000)",
            rusqlite::params![idempotency_key, digest],
        )
        .unwrap();
    }

    let (mut o7d_child, addr) = spawn_o7d_with_exec_and_redrive_ms(
        &ledger_path,
        repo.path(),
        &worktree_root,
        &runs_dir,
        claude.path(),
        Some(200),
    );
    wait_until_healthy(addr);

    let (status, retried) = post(
        addr,
        &format!("/api/v1/conversations/{conversation_id}/commands"),
        &serde_json::json!({
            "schema_version": 1,
            "parent_run_id": parent_run_id,
            "command": command_text,
            "idempotency_key": idempotency_key,
        }),
    );
    assert_eq!(status, 202, "{retried:?}");
    let fresh_run_id = retried["run_id"].as_str().unwrap().to_owned();
    assert_ne!(fresh_run_id, old_run_id.as_str());

    let deadline = Instant::now() + Duration::from_secs(30);
    claude.wait_for_invocation(2, deadline);
    let deadline = Instant::now() + Duration::from_secs(30);
    wait_for_run_status(addr, &fresh_run_id, "completed", deadline);
    assert_eq!(claude.invocation_count(), 2);

    let _ = o7d_child.kill();
    let _ = o7d_child.wait();

    // NOW the delayed original process for the OLD (superseded) run id
    // finally gets scheduled and tries to do its own work — exactly what
    // a genuinely slow (not dead) `o7 continue` would do the moment it
    // reaches its own lock, long after the redrive already won.
    let output = Command::new(env!("CARGO_BIN_EXE_o7"))
        .arg("continue")
        .arg("--repo")
        .arg(repo.path())
        .arg("--worktree-root")
        .arg(&worktree_root)
        .arg("--runs-dir")
        .arg(&runs_dir)
        .arg("--ledger")
        .arg(&ledger_path)
        .arg("--conversation-id")
        .arg(&conversation_id)
        .arg("--parent-run-id")
        .arg(&parent_run_id)
        .arg("--command")
        .arg(command_text)
        .arg("--run-id")
        .arg(old_run_id.as_str())
        .arg("--command-id")
        .arg("cmd-delayed-original")
        .env(
            "PATH",
            format!(
                "{}:{}",
                claude.path().display(),
                std::env::var("PATH").unwrap_or_default()
            ),
        )
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "the delayed original process must exit cleanly, not error: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        claude.invocation_count(),
        2,
        "the delayed original must NEVER invoke the provider — the command was already \
         redriven and sealed under the fresh run id"
    );

    let conn = rusqlite::Connection::open(&ledger_path).unwrap();
    let old_run_exists: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM run WHERE run_id = ?1",
            [old_run_id.as_str()],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        old_run_exists, 0,
        "the delayed original must never create a ledger row for its own (superseded) run id"
    );
    let (command_status, command_child): (String, Option<String>) = conn
        .query_row(
            "SELECT status, child_run_id FROM command WHERE command_id = 'cmd-delayed-original'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(command_status, "completed");
    assert_eq!(
        command_child.as_deref(),
        Some(fresh_run_id.as_str()),
        "the command's binding must still point at the fresh (redriven) child — untouched by \
         the delayed original"
    );
}
