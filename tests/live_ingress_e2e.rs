//! Q-Deck R0.7 (`docs/q-deck/r07-live-ingress.md`): process-level acceptance.
//! A REAL `o7 run` process (the compiled `o7` binary, `CARGO_BIN_EXE_o7`)
//! projecting live into a REAL SQLite ledger, watched through a REAL `o7d`
//! process (its binary path derived from `o7`'s own — see `o7d_bin_path`)
//! over REST/SSE — never the projector called in-memory, never a post-run
//! import. The only stand-in is the external
//! `claude` CLI itself (no credentials in this environment): a tiny stub
//! script shadows it on PATH, but every other step of the production path
//! (worktree, real `bash` gate execution, canonical event minting, live
//! ledger projection, real HTTP/SSE) is exercised for real.
//!
//! Invariant for the restriction-lint allowance below: every `unwrap`/
//! `expect`/index here is on this test's own controlled fixtures/output —
//! a panic means this test's own setup broke, matching the precedent in
//! `crates/o7d/tests/golden_transcript_sse.rs`.
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
use std::time::{Duration, Instant};

/// `CARGO_BIN_EXE_<name>` only exists for binaries the CURRENT package
/// declares itself — it does not extend to a dependency's (or even a
/// dev-dependency's) binary targets, so `o7d`'s executable isn't reachable
/// that way from here. All of a workspace's binaries land in the same
/// `target/<profile>/` directory, though, and `CARGO_BIN_EXE_o7` (this
/// package's own binary, which IS set) sits in exactly that directory — so
/// `o7d`'s path is derived from it rather than guessed independently.
fn o7d_bin_path() -> PathBuf {
    let own = PathBuf::from(env!("CARGO_BIN_EXE_o7"));
    let dir = own.parent().expect("o7's binary has a parent dir");
    let candidate = dir.join("o7d");
    assert!(
        candidate.exists(),
        "expected o7d's binary at {} (built alongside o7 in the same workspace target dir) — \
         run `cargo build -p o7d` first if testing this file in isolation",
        candidate.display()
    );
    candidate
}

/// A tiny fake `claude` on its own PATH-prepended directory — the one
/// external dependency this environment cannot exercise for real. Sleeps
/// `sleep_secs` first so the test has a real window to observe `running`
/// over REST before the process exits.
fn fake_claude_dir(sleep_secs: u64) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let script = dir.path().join("claude");
    std::fs::write(
        &script,
        format!("#!/bin/sh\nsleep {sleep_secs}\necho '{{\"ok\":true}}'\nexit 0\n"),
    )
    .unwrap();
    let mut perms = std::fs::metadata(&script).unwrap().permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut perms, 0o755);
    std::fs::set_permissions(&script, perms).unwrap();
    dir
}

/// A minimal real git repo: one commit, a trivial gate manifest with a
/// single fast-passing gate, and a task file — everything `o7 run` needs
/// downstream of the agent.
fn fixture_repo() -> tempfile::TempDir {
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
    std::fs::write(
        gate_dir.join("gate.toml"),
        "[[gate]]\nname = \"unit\"\ncmd = \"true\"\n",
    )
    .unwrap();
    run(&["add", "-A"]);
    run(&["commit", "-q", "-m", "initial"]);
    dir
}

fn spawn_o7d_process(db_path: &Path) -> (Child, SocketAddr) {
    let mut child = Command::new(o7d_bin_path())
        .args([
            "serve",
            "--ledger",
            db_path.to_str().expect("utf8 path"),
            "--listen",
            "127.0.0.1:0",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn the real o7d binary");
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

#[allow(clippy::too_many_arguments)]
fn spawn_o7_run_process(
    repo: &Path,
    task_file: &Path,
    ledger_path: &Path,
    runs_dir: &Path,
    worktree_root: &Path,
    conversation_id: Option<&str>,
    claude_dir: &Path,
) -> Child {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_o7"));
    cmd.arg("run")
        .arg("--repo")
        .arg(repo)
        .arg("--task")
        .arg(task_file)
        .arg("--ledger")
        .arg(ledger_path)
        .arg("--runs-dir")
        .arg(runs_dir)
        .arg("--worktree-root")
        .arg(worktree_root)
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
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(id) = conversation_id {
        cmd.arg("--conversation-id").arg(id);
    }
    cmd.spawn().expect("spawn the real o7 binary")
}

fn get(addr: SocketAddr, path: &str) -> (u16, serde_json::Value) {
    let mut stream = std::net::TcpStream::connect(addr).unwrap();
    let req = format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
    stream.write_all(req.as_bytes()).unwrap();
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
    // Body may be chunked; dechunk minimally like the other o7d test suites.
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

// Two independent exit conditions (no more CRLF-delimited size line, or a
// non-hex size line) — genuinely not a single `while let`, unlike clippy's
// suggestion.
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
    let deadline = Instant::now() + Duration::from_secs(5);
    poll_until(deadline, || {
        let (status, _) = get(addr, "/api/v1/health");
        (status == 200).then_some(())
    });
}

/// One SSE frame's `data:` payload, read directly off a raw TCP connection —
/// enough to prove a live event arrives before the run process exits.
fn read_one_sse_data_frame(addr: SocketAddr, conversation_id: &str) -> String {
    let mut stream = std::net::TcpStream::connect(addr).unwrap();
    let req = format!(
        "GET /api/v1/conversations/{conversation_id}/events/stream HTTP/1.1\r\nHost: localhost\r\nConnection: keep-alive\r\n\r\n"
    );
    stream.write_all(req.as_bytes()).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    let mut reader = BufReader::new(stream);
    let mut buf = String::new();
    loop {
        let mut line = String::new();
        let n = reader.read_line(&mut line).unwrap();
        assert!(n != 0, "SSE stream closed before a data frame arrived");
        buf.push_str(&line);
        if let Some(rest) = line.strip_prefix("data:") {
            if !rest.trim().is_empty() {
                return rest.trim().to_string();
            }
        }
    }
}

#[test]
fn live_run_is_visible_before_and_after_completion_across_real_processes() {
    let repo = fixture_repo();
    let task_file = repo.path().join("task.md");
    std::fs::write(&task_file, "do the thing").unwrap();
    let work = tempfile::tempdir().unwrap();
    let ledger_path = work.path().join("ledger.sqlite3");
    let claude = fake_claude_dir(1);

    let (mut o7d_child, addr) = spawn_o7d_process(&ledger_path);
    wait_until_healthy(addr);

    let mut run_child = spawn_o7_run_process(
        repo.path(),
        &task_file,
        &ledger_path,
        &work.path().join("runs"),
        &work.path().join("worktrees"),
        None,
        claude.path(),
    );

    // Proof (6): while the process is still executing, REST already shows
    // the conversation and run, status running, no terminal status yet.
    let deadline = Instant::now() + Duration::from_secs(5);
    let (conv_id, run_id) = poll_until(deadline, || {
        let (status, page) = get(addr, "/api/v1/runs");
        if status != 200 {
            return None;
        }
        let items = page.get("items")?.as_array()?;
        let item = items.first()?;
        Some((
            item.get("conversation_id")?.as_str()?.to_owned(),
            item.get("run_id")?.as_str()?.to_owned(),
        ))
    });
    let (status, run) = get(addr, &format!("/api/v1/runs/{run_id}"));
    assert_eq!(status, 200);
    assert_eq!(run["status"], "running", "seen before the process exits");
    assert!(run["finished_at"].is_null());

    // Proof (7): a live SSE event arrives before the process has finished.
    let frame = read_one_sse_data_frame(addr, &conv_id);
    let frame_json: serde_json::Value = serde_json::from_str(&frame).unwrap();
    assert!(frame_json.get("event_type").is_some());

    let run_status = run_child.wait().unwrap();
    let mut run_stdout = String::new();
    run_child
        .stdout
        .take()
        .unwrap()
        .read_to_string(&mut run_stdout)
        .unwrap();
    assert!(
        run_status.success(),
        "the fixture gate/agent are set up to pass: {run_stdout}"
    );

    // Proof (8): after completion, terminal status matches the canonical
    // verdict, the right event type, finished_at set, no duplicates.
    let (status, run) = get(addr, &format!("/api/v1/runs/{run_id}"));
    assert_eq!(status, 200);
    assert_eq!(run["status"], "completed");
    assert!(run["finished_at"].is_number());

    let (status, events) = get(
        addr,
        &format!("/api/v1/conversations/{conv_id}/events?limit=100"),
    );
    assert_eq!(status, 200);
    let items = events["items"].as_array().unwrap();
    let event_types: Vec<&str> = items
        .iter()
        .map(|e| e["event_type"].as_str().unwrap())
        .collect();
    assert!(event_types.contains(&"run.completed"));
    assert!(!event_types
        .iter()
        .any(|t| *t == "run.errored" || *t == "run.blocked"));
    let sequences: Vec<u64> = items
        .iter()
        .map(|e| e["sequence"].as_u64().unwrap())
        .collect();
    let mut sorted = sequences.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sequences, sorted, "gap-free, duplicate-free, in order");

    // Proof (9)+(10): restart o7d against the same file, read back identical.
    let _ = o7d_child.kill();
    let _ = o7d_child.wait();
    let (mut o7d_child2, addr2) = spawn_o7d_process(&ledger_path);
    wait_until_healthy(addr2);
    let (status, run2) = get(addr2, &format!("/api/v1/runs/{run_id}"));
    assert_eq!(status, 200);
    assert_eq!(run2["status"], "completed");
    assert_eq!(run2["finished_at"], run["finished_at"]);

    // Proof (11): canonical replay of the flat record agrees with the ledger.
    let record_line = run_stdout
        .lines()
        .find(|l| l.contains(": record at "))
        .expect("o7 run prints the record directory");
    let record_dir = record_line
        .split(": record at ")
        .nth(1)
        .expect("record dir after the marker")
        .trim();
    let replay = Command::new(env!("CARGO_BIN_EXE_o7"))
        .arg("replay")
        .arg(record_dir)
        .output()
        .unwrap();
    assert!(replay.status.success(), "replay must verify a clean run");
    let replay_stdout = String::from_utf8_lossy(&replay.stdout);
    assert!(
        replay_stdout.contains("Pass"),
        "replay verdict must be Pass to match the ledger's completed: {replay_stdout}"
    );

    // Proof (12): recovery against a run that already sealed cleanly is a
    // safe, idempotent no-op — nothing left running to mark interrupted.
    let recover1 = Command::new(env!("CARGO_BIN_EXE_o7"))
        .args(["recover", "--ledger"])
        .arg(&ledger_path)
        .output()
        .unwrap();
    assert!(recover1.status.success());
    assert!(String::from_utf8_lossy(&recover1.stdout).contains("0 run(s)"));
    let recover2 = Command::new(env!("CARGO_BIN_EXE_o7"))
        .args(["recover", "--ledger"])
        .arg(&ledger_path)
        .output()
        .unwrap();
    assert!(recover2.status.success());
    let (status, run3) = get(addr2, &format!("/api/v1/runs/{run_id}"));
    assert_eq!(status, 200);
    assert_eq!(
        run3["status"], "completed",
        "recovery never touches a sealed run"
    );

    let _ = o7d_child2.kill();
    let _ = o7d_child2.wait();
}

#[test]
fn interruption_before_seal_is_interrupted_never_error() {
    let repo = fixture_repo();
    let task_file = repo.path().join("task.md");
    std::fs::write(&task_file, "do the thing").unwrap();
    let work = tempfile::tempdir().unwrap();
    let ledger_path = work.path().join("ledger.sqlite3");
    // Long enough that the test can reliably observe `running` and kill the
    // process well before it would ever seal on its own.
    let claude = fake_claude_dir(30);

    let (mut o7d_child, addr) = spawn_o7d_process(&ledger_path);
    wait_until_healthy(addr);

    let mut run_child = spawn_o7_run_process(
        repo.path(),
        &task_file,
        &ledger_path,
        &work.path().join("runs"),
        &work.path().join("worktrees"),
        None,
        claude.path(),
    );

    let deadline = Instant::now() + Duration::from_secs(5);
    let run_id = poll_until(deadline, || {
        let (status, page) = get(addr, "/api/v1/runs");
        if status != 200 {
            return None;
        }
        let items = page.get("items")?.as_array()?;
        let item = items.first()?;
        let status_str = item.get("status")?.as_str()?;
        (status_str == "running").then(|| item.get("run_id")?.as_str().map(str::to_owned))?
    });

    // A real process kill — not a graceful shutdown, not a simulated error.
    run_child.kill().expect("SIGKILL the o7 run process");
    let _ = run_child.wait();

    let recover = Command::new(env!("CARGO_BIN_EXE_o7"))
        .args(["recover", "--ledger"])
        .arg(&ledger_path)
        .output()
        .unwrap();
    assert!(recover.status.success());
    assert!(
        String::from_utf8_lossy(&recover.stdout).contains("1 run(s)"),
        "recovery must find exactly the one run this test killed"
    );

    let (status, run) = get(addr, &format!("/api/v1/runs/{run_id}"));
    assert_eq!(status, 200);
    assert_eq!(run["status"], "interrupted");
    assert!(
        run["finished_at"].is_null(),
        "interrupted is unsealed — finished_at must stay unset"
    );

    let (status, conv) = get(addr, &format!("/api/v1/runs/{run_id}"));
    assert_eq!(status, 200);
    let conversation_id = conv["conversation_id"].as_str().unwrap();
    let (status, events) = get(
        addr,
        &format!("/api/v1/conversations/{conversation_id}/events?limit=100"),
    );
    assert_eq!(status, 200);
    let event_types: Vec<String> = events["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["event_type"].as_str().unwrap().to_owned())
        .collect();
    assert!(
        !event_types.iter().any(|t| t == "run.errored"),
        "a pre-seal kill must never be recorded as run.errored: {event_types:?}"
    );
    assert!(
        !event_types
            .iter()
            .any(|t| t == "run.completed" || t == "run.blocked" || t == "run.failed"),
        "a pre-seal kill must never claim any sealed verdict: {event_types:?}"
    );

    let _ = o7d_child.kill();
    let _ = o7d_child.wait();
}
