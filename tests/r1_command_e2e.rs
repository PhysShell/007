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
    // The redrive reuses the ALREADY-bound child run id — it must never
    // mint a new one (that would orphan the original binding).
    assert_eq!(retried["run_id"], child_run_id_before_redrive.as_str());

    let deadline = Instant::now() + Duration::from_secs(30);
    claude.wait_for_invocation(2, deadline);
    let deadline = Instant::now() + Duration::from_secs(30);
    wait_for_run_status(
        addr,
        child_run_id_before_redrive.as_str(),
        "completed",
        deadline,
    );

    let (status, command_row) = get(
        addr,
        &format!("/api/v1/runs/{}", child_run_id_before_redrive.as_str()),
    );
    assert_eq!(status, 200);
    assert_eq!(command_row["parent_run_id"], parent_run_id);

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
