//! Q-Deck R0.7 (`docs/q-deck/r07-live-ingress.md`): process-level acceptance.
//! A REAL `o7 run` process (the compiled `o7` binary, `CARGO_BIN_EXE_o7`)
//! projecting live into a REAL SQLite ledger, watched through a REAL `o7d`
//! process (its binary path derived from `o7`'s own — see `o7d_bin_path`)
//! over REST/SSE — never the projector called in-memory, never a post-run
//! import. The only stand-in is the external `claude` CLI itself (no
//! credentials in this environment): a tiny stub script shadows it on PATH,
//! but every other step of the production path (worktree, real `bash` gate
//! execution, canonical event minting, live ledger projection, real
//! HTTP/SSE) is exercised for real.
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
/// over REST before the process exits, then exits with `exit_code`. Writes
/// a sentinel file on invocation so a test can prove the stub was (or, for
/// the fail-fast cases, was NOT) ever actually run.
fn fake_claude_dir(sleep_secs: u64, exit_code: u32) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let sentinel = dir.path().join("invoked.sentinel");
    let script = dir.path().join("claude");
    std::fs::write(
        &script,
        format!(
            "#!/bin/sh\ntouch {}\nsleep {sleep_secs}\necho '{{\"ok\":true}}'\nexit {exit_code}\n",
            sentinel.display()
        ),
    )
    .unwrap();
    let mut perms = std::fs::metadata(&script).unwrap().permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut perms, 0o755);
    std::fs::set_permissions(&script, perms).unwrap();
    dir
}

fn claude_was_invoked(claude_dir: &Path) -> bool {
    claude_dir.join("invoked.sentinel").exists()
}

/// A minimal real git repo: one commit, a caller-supplied gate manifest,
/// and a task file — everything `o7 run` needs downstream of the agent.
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
const FAILING_GATE: &str = "[[gate]]\nname = \"unit\"\ncmd = \"false\"\n";
/// A required gate that can never run here (no waiver) — the reducer
/// scores an unwaived, unrun required obligation `Blocked`.
const BLOCKED_GATE: &str = "[[gate]]\nname = \"windows-only\"\ncmd = \"true\"\nenv = \"windows\"\n";

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

struct RunInvocation<'a> {
    repo: &'a Path,
    task_file: &'a Path,
    ledger_path: &'a Path,
    runs_dir: &'a Path,
    worktree_root: &'a Path,
    conversation_id: Option<&'a str>,
    claude_dir: &'a Path,
    no_ledger: bool,
}

fn spawn_o7_run_process(inv: &RunInvocation<'_>) -> Child {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_o7"));
    cmd.arg("run")
        .arg("--repo")
        .arg(inv.repo)
        .arg("--task")
        .arg(inv.task_file)
        .arg("--runs-dir")
        .arg(inv.runs_dir)
        .arg("--worktree-root")
        .arg(inv.worktree_root)
        .arg("--max-turns")
        .arg("1")
        .env(
            "PATH",
            format!(
                "{}:{}",
                inv.claude_dir.display(),
                std::env::var("PATH").unwrap_or_default()
            ),
        )
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if !inv.no_ledger {
        cmd.arg("--ledger").arg(inv.ledger_path);
    }
    if let Some(id) = inv.conversation_id {
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

/// Every `data:` line of every SSE frame on this connection, for up to
/// `want` frames or until `deadline` — enough to search for a specific
/// canonical kind rather than accepting the first frame of any shape.
fn read_sse_data_frames(addr: SocketAddr, conversation_id: &str, want: usize) -> Vec<String> {
    let mut stream = std::net::TcpStream::connect(addr).unwrap();
    let req = format!(
        "GET /api/v1/conversations/{conversation_id}/events/stream HTTP/1.1\r\nHost: localhost\r\nConnection: keep-alive\r\n\r\n"
    );
    stream.write_all(req.as_bytes()).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    let mut reader = BufReader::new(stream);
    let mut out = Vec::new();
    while out.len() < want {
        let mut line = String::new();
        let n = reader.read_line(&mut line).unwrap();
        assert!(
            n != 0,
            "SSE stream closed before {want} data frame(s) arrived"
        );
        if let Some(rest) = line.strip_prefix("data:") {
            if !rest.trim().is_empty() {
                out.push(rest.trim().to_string());
            }
        }
    }
    out
}

fn read_events_jsonl(run_dir: &Path) -> Vec<serde_json::Value> {
    let text = std::fs::read_to_string(run_dir.join("events.jsonl")).unwrap();
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

fn find_record_dir(stdout: &str) -> String {
    stdout
        .lines()
        .find(|l| l.contains(": record at "))
        .expect("o7 run prints the record directory")
        .split(": record at ")
        .nth(1)
        .expect("record dir after the marker")
        .trim()
        .to_string()
}

#[test]
fn live_run_is_visible_before_and_after_completion_across_real_processes() {
    let repo = fixture_repo(PASSING_GATE);
    let task_file = repo.path().join("task.md");
    std::fs::write(&task_file, "do the thing").unwrap();
    let work = tempfile::tempdir().unwrap();
    let ledger_path = work.path().join("ledger.sqlite3");
    let claude = fake_claude_dir(1, 0);

    let (mut o7d_child, addr) = spawn_o7d_process(&ledger_path);
    wait_until_healthy(addr);

    let mut run_child = spawn_o7_run_process(&RunInvocation {
        repo: repo.path(),
        task_file: &task_file,
        ledger_path: &ledger_path,
        runs_dir: &work.path().join("runs"),
        worktree_root: &work.path().join("worktrees"),
        conversation_id: None,
        claude_dir: claude.path(),
        no_ledger: false,
    });

    // Proof: while the process is still executing, REST already shows the
    // conversation and run, status running, no terminal status yet. The run
    // row is created `queued` (`create_run_with_id`) and only transitions to
    // `running` in a SEPARATE, following `start_run` call — polling only
    // until the row exists (not until it's specifically `running`) can
    // observe that brief `queued` window and flake, so the poll condition
    // itself checks for `running`, not mere existence.
    let deadline = Instant::now() + Duration::from_secs(5);
    let (conv_id, run_id, run) = poll_until(deadline, || {
        let (status, page) = get(addr, "/api/v1/runs");
        if status != 200 {
            return None;
        }
        let items = page.get("items")?.as_array()?;
        let item = items.first()?;
        if item.get("status")?.as_str()? != "running" {
            return None;
        }
        Some((
            item.get("conversation_id")?.as_str()?.to_owned(),
            item.get("run_id")?.as_str()?.to_owned(),
            item.clone(),
        ))
    });
    assert!(run["finished_at"].is_null());

    // Proof: a LIVE canonical event (not just any event_type — specifically
    // the projected AgentStarted, with matching provenance) arrives over
    // SSE before the process exits.
    // conversation.created, run.created, run.started, then the
    // system.note provenance for RunStarted, THEN AgentStarted's — comfortable
    // margin beyond that in case ordering shifts slightly.
    let frames = read_sse_data_frames(addr, &conv_id, 8);
    let agent_started_frame = frames
        .iter()
        .find_map(|f| {
            let v: serde_json::Value = serde_json::from_str(f).ok()?;
            (v["event_type"] == "system.note" && v["payload"]["canonical_kind"] == "agent_started")
                .then_some(v)
        })
        .expect("an agent_started system.note must arrive live over SSE before completion");
    assert_eq!(agent_started_frame["run_id"], run_id);

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

    let record_dir = PathBuf::from(find_record_dir(&run_stdout));

    // Proof: the SSE frame's provenance exactly matches the canonical
    // events.jsonl's own AgentStarted event — not merely "some event_type".
    let jsonl = read_events_jsonl(&record_dir);
    // RunEventKind is internally tagged: #[serde(tag = "type", rename_all
    // = "snake_case")] on o7_run::event::RunEventKind, nested under the
    // event's own "kind" field — so the discriminant lives at
    // kind.type, not at kind directly.
    let jsonl_agent_started = jsonl
        .iter()
        .find(|e| e["kind"]["type"] == "agent_started")
        .expect("AgentStarted must be present in the canonical record");
    assert_eq!(
        agent_started_frame["payload"]["canonical_sequence"],
        jsonl_agent_started["sequence"]
    );
    assert_eq!(
        agent_started_frame["payload"]["canonical_event_digest"],
        jsonl_agent_started["event_digest"]
    );
    assert_eq!(
        agent_started_frame["payload"]["canonical_run_id"],
        jsonl_agent_started["run_id"]
    );

    // Proof: after completion, terminal status matches the canonical
    // verdict, the right event type, finished_at set.
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

    // Proof: ledger sequences are truly CONSECUTIVE (next == previous + 1),
    // not merely sorted-and-unique — [1, 3, 8] would pass a sort+dedup
    // check but must fail this one.
    let sequences: Vec<u64> = items
        .iter()
        .map(|e| e["sequence"].as_u64().unwrap())
        .collect();
    for pair in sequences.windows(2) {
        assert_eq!(
            pair[1],
            pair[0] + 1,
            "ledger sequence must be gap-free and consecutive: {sequences:?}"
        );
    }

    // Proof: the SAME RunId bytes appear in the CLI's own stdout, the
    // record directory name, meta.json, every events.jsonl line, and the
    // REST DTO.
    assert!(
        run_stdout.contains(&run_id),
        "CLI stdout must echo the run id"
    );
    assert_eq!(
        record_dir.file_name().unwrap().to_str().unwrap(),
        run_id,
        "the run directory name must be exactly the run id"
    );
    let meta: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(record_dir.join("meta.json")).unwrap())
            .unwrap();
    assert_eq!(meta["run_id"], run_id);
    for event in &jsonl {
        assert_eq!(event["run_id"], run_id, "every canonical event: {event:?}");
    }
    assert_eq!(run["run_id"], run_id);

    // Proof: restart o7d against the same file — the FULL event transcript
    // (not just status/finished_at) reads back identically.
    let _ = o7d_child.kill();
    let _ = o7d_child.wait();
    let (mut o7d_child2, addr2) = spawn_o7d_process(&ledger_path);
    wait_until_healthy(addr2);
    let (status, run2) = get(addr2, &format!("/api/v1/runs/{run_id}"));
    assert_eq!(status, 200);
    assert_eq!(
        run2, run,
        "the whole run DTO must be byte-identical after restart"
    );
    let (status, events2) = get(
        addr2,
        &format!("/api/v1/conversations/{conv_id}/events?limit=100"),
    );
    assert_eq!(status, 200);
    assert_eq!(
        events2, events,
        "the FULL event transcript must be identical after restart, not just status"
    );

    // Proof: canonical replay of the flat record agrees with the ledger.
    let replay = Command::new(env!("CARGO_BIN_EXE_o7"))
        .arg("replay")
        .arg(&record_dir)
        .output()
        .unwrap();
    assert!(replay.status.success(), "replay must verify a clean run");
    let replay_stdout = String::from_utf8_lossy(&replay.stdout);
    assert!(
        replay_stdout.contains("Pass"),
        "replay verdict must be Pass to match the ledger's completed: {replay_stdout}"
    );

    // Proof: recovery against a run that already sealed cleanly is a safe,
    // idempotent no-op — nothing left running to mark interrupted.
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

/// Every real root-process verdict mapping, end to end: Pass->Completed,
/// Fail->Failed, Blocked->Blocked, plus (via a non-zero-exit fake claude)
/// Error->Error — proving the process boundary, not just the unit-level
/// projector mapping already covered in `src/ledger_projector.rs`.
#[test]
fn every_canonical_verdict_maps_correctly_across_real_processes() {
    for (label, gate_toml, agent_exit_code, expected_status) in [
        ("pass", PASSING_GATE, 0, "completed"),
        ("fail", FAILING_GATE, 0, "failed"),
        ("blocked", BLOCKED_GATE, 0, "blocked"),
        ("error", PASSING_GATE, 1, "error"),
    ] {
        let repo = fixture_repo(gate_toml);
        let task_file = repo.path().join("task.md");
        std::fs::write(&task_file, "do the thing").unwrap();
        let work = tempfile::tempdir().unwrap();
        let ledger_path = work.path().join("ledger.sqlite3");
        let claude = fake_claude_dir(0, agent_exit_code);

        let (mut o7d_child, addr) = spawn_o7d_process(&ledger_path);
        wait_until_healthy(addr);

        let mut run_child = spawn_o7_run_process(&RunInvocation {
            repo: repo.path(),
            task_file: &task_file,
            ledger_path: &ledger_path,
            runs_dir: &work.path().join("runs"),
            worktree_root: &work.path().join("worktrees"),
            conversation_id: None,
            claude_dir: claude.path(),
            no_ledger: false,
        });
        let _ = run_child.wait();

        let (status, page) = get(addr, "/api/v1/runs");
        assert_eq!(status, 200, "label={label}");
        let items = page["items"].as_array().unwrap();
        assert_eq!(items.len(), 1, "label={label}");
        assert_eq!(
            items[0]["status"], expected_status,
            "label={label}: {items:?}"
        );
        assert!(items[0]["finished_at"].is_number(), "label={label}");

        let _ = o7d_child.kill();
        let _ = o7d_child.wait();
    }
}

#[test]
fn interruption_before_seal_is_interrupted_never_error() {
    let repo = fixture_repo(PASSING_GATE);
    let task_file = repo.path().join("task.md");
    std::fs::write(&task_file, "do the thing").unwrap();
    let work = tempfile::tempdir().unwrap();
    let ledger_path = work.path().join("ledger.sqlite3");
    // Long enough that the test can reliably observe `running` and kill the
    // process well before it would ever seal on its own.
    let claude = fake_claude_dir(30, 0);

    let (mut o7d_child, addr) = spawn_o7d_process(&ledger_path);
    wait_until_healthy(addr);

    let mut run_child = spawn_o7_run_process(&RunInvocation {
        repo: repo.path(),
        task_file: &task_file,
        ledger_path: &ledger_path,
        runs_dir: &work.path().join("runs"),
        worktree_root: &work.path().join("worktrees"),
        conversation_id: None,
        claude_dir: claude.path(),
        no_ledger: false,
    });

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

    let conversation_id = run["conversation_id"].as_str().unwrap();
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

/// Q-Deck R0.7 third re-gate, defect #1: a durable `RunStarted` event, once
/// on disk, references `task.md` by digest — a SIGKILL during the agent run
/// (well after RunStarted lands, well before the run finishes) must never
/// be able to leave that reference dangling. Kills at exactly the same
/// point `interruption_before_seal_is_interrupted_never_error` does, but
/// asserts on `task.md`'s existence and content digest instead of the
/// ledger's interrupted status.
#[test]
fn sigkill_during_the_agent_leaves_a_durable_task_md_matching_run_started() {
    let repo = fixture_repo(PASSING_GATE);
    let task_file = repo.path().join("task.md");
    let task_contents = "do the durable thing";
    std::fs::write(&task_file, task_contents).unwrap();
    let work = tempfile::tempdir().unwrap();
    let ledger_path = work.path().join("ledger.sqlite3");
    let runs_dir = work.path().join("runs");
    // Long enough that the test can reliably observe `running` and kill the
    // process well before it would ever finish on its own.
    let claude = fake_claude_dir(30, 0);

    let (mut o7d_child, addr) = spawn_o7d_process(&ledger_path);
    wait_until_healthy(addr);

    let mut run_child = spawn_o7_run_process(&RunInvocation {
        repo: repo.path(),
        task_file: &task_file,
        ledger_path: &ledger_path,
        runs_dir: &runs_dir,
        worktree_root: &work.path().join("worktrees"),
        conversation_id: None,
        claude_dir: claude.path(),
        no_ledger: false,
    });

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

    // A real, uncatchable kill — the run's own tail-end writes (the OLD
    // `write_task` timing, after the agent finished) never get a chance to
    // run. `spawn_o7_run_process` never prints "record at" before this
    // point either (that only happens at the very end of `execute()`), so
    // the record dir is computed directly instead of parsed from stdout.
    run_child.kill().expect("SIGKILL the o7 run process");
    let _ = run_child.wait();

    let target_name = repo
        .path()
        .file_name()
        .expect("fixture repo has a name")
        .to_string_lossy()
        .to_string();
    let record_dir = runs_dir.join(&target_name).join(&run_id);

    let jsonl = read_events_jsonl(&record_dir);
    assert!(
        !jsonl.is_empty(),
        "a durable RunStarted must exist even after an early SIGKILL"
    );
    let run_started = &jsonl[0];
    assert_eq!(run_started["kind"]["type"], "run_started");
    let expected_digest = run_started["kind"]["task"]["digest"]
        .as_str()
        .expect("RunStarted's task ArtifactRef carries a digest")
        .to_owned();

    let task_md_path = record_dir.join("task.md");
    assert!(
        task_md_path.exists(),
        "task.md must exist durably even though the process was killed mid-agent, \
         well before the point the old (post-agent) write used to happen"
    );
    let task_md_bytes = std::fs::read(&task_md_path).unwrap();
    let actual_digest = o7_run::event::Digest256::of_bytes(&task_md_bytes);
    assert_eq!(
        actual_digest.as_str(),
        expected_digest,
        "task.md's actual content must hash to EXACTLY the digest RunStarted durably \
         committed to, not just happen to exist"
    );
    assert_eq!(std::str::from_utf8(&task_md_bytes).unwrap(), task_contents);

    let _ = o7d_child.kill();
    let _ = o7d_child.wait();
}

/// Q-Deck R0.7 fourth re-gate, defect #3: a run whose canonical stream is
/// genuinely SEALED (`RunSealed` durably in `events.jsonl`, a concrete
/// reducer verdict) but whose ledger `seal()` call never landed (a crash, or
/// a lost projection write) is legitimately still `running` in the ledger.
/// A plain `o7 recover` (no `--run-dir`) then classifies it `interrupted` —
/// correct AS FAR AS IT KNOWS, since it only scans for stale `running` rows,
/// never consults `events.jsonl`. But `interrupted` must never be allowed to
/// PERMANENTLY outrank the real, independently-verified canonical verdict:
/// catch-up (`--run-dir`) must repair it to the exact terminal status once
/// it has verified the sealed stream, and a repeated catch-up after that
/// must be a complete no-op.
#[test]
fn plain_recover_marking_interrupted_never_permanently_blocks_a_sealed_canonical_verdict() {
    let work = tempfile::tempdir().unwrap();
    let ledger_path = work.path().join("ledger.sqlite3");
    let record_dir = run_to_completion(work.path(), &ledger_path, None);

    let (mut o7d_child, addr) = spawn_o7d_process(&ledger_path);
    wait_until_healthy(addr);
    let (status, page) = get(addr, "/api/v1/runs");
    assert_eq!(status, 200);
    let run_id = page["items"][0]["run_id"].as_str().unwrap().to_owned();
    assert_eq!(
        page["items"][0]["status"], "completed",
        "sanity: the fixture run must have sealed cleanly the first time"
    );
    let _ = o7d_child.kill();
    let _ = o7d_child.wait();

    // Simulate "the ledger's own seal() call never landed" by directly
    // reverting the run row to `running` — leaving the (already-completed)
    // attempt row untouched, exactly like a real crash between the
    // durable, sealed events.jsonl and the ledger seal would: the run.
    // status column is what a plain `o7 recover` scan actually reads.
    {
        let conn = rusqlite::Connection::open(&ledger_path).unwrap();
        let updated = conn
            .execute(
                "UPDATE run SET status = 'running', finished_at = NULL WHERE run_id = ?1",
                [&run_id],
            )
            .unwrap();
        assert_eq!(updated, 1);
    }

    // Plain recovery (no --run-dir): classifies the stuck-running row
    // `interrupted`, exactly as R0.5 always has — it has no way to know
    // events.jsonl is actually sealed.
    let plain_recover = Command::new(env!("CARGO_BIN_EXE_o7"))
        .args(["recover", "--ledger"])
        .arg(&ledger_path)
        .output()
        .unwrap();
    assert!(plain_recover.status.success());
    assert!(String::from_utf8_lossy(&plain_recover.stdout).contains("1 run(s)"));

    let (mut o7d_child, addr) = spawn_o7d_process(&ledger_path);
    wait_until_healthy(addr);
    let (status, run) = get(addr, &format!("/api/v1/runs/{run_id}"));
    assert_eq!(status, 200);
    assert_eq!(
        run["status"], "interrupted",
        "plain recovery must classify the stuck-running row interrupted"
    );
    let _ = o7d_child.kill();
    let _ = o7d_child.wait();

    // Catch-up: verifies the canonical stream is genuinely sealed and
    // repairs `interrupted` to the exact terminal verdict — never blocked
    // by `interrupted` being an otherwise-settled dead-end.
    let catch_up = Command::new(env!("CARGO_BIN_EXE_o7"))
        .args(["recover", "--ledger"])
        .arg(&ledger_path)
        .args(["--run-dir"])
        .arg(&record_dir)
        .output()
        .unwrap();
    assert!(
        catch_up.status.success(),
        "catch-up must repair an interrupted-but-actually-sealed run: {}",
        String::from_utf8_lossy(&catch_up.stderr)
    );
    assert!(String::from_utf8_lossy(&catch_up.stdout).contains("sealed"));

    let (mut o7d_child, addr) = spawn_o7d_process(&ledger_path);
    wait_until_healthy(addr);
    let (status, repaired_run) = get(addr, &format!("/api/v1/runs/{run_id}"));
    assert_eq!(status, 200);
    assert_eq!(
        repaired_run["status"], "completed",
        "catch-up must repair the run to its true, independently-verified canonical verdict"
    );
    let _ = o7d_child.kill();
    let _ = o7d_child.wait();

    // A repeated catch-up must be a complete no-op — the run is already
    // `completed`, matching the verdict exactly.
    let catch_up_again = Command::new(env!("CARGO_BIN_EXE_o7"))
        .args(["recover", "--ledger"])
        .arg(&ledger_path)
        .args(["--run-dir"])
        .arg(&record_dir)
        .output()
        .unwrap();
    assert!(catch_up_again.status.success());

    let (mut o7d_child, addr) = spawn_o7d_process(&ledger_path);
    wait_until_healthy(addr);
    let (status, run_after) = get(addr, &format!("/api/v1/runs/{run_id}"));
    assert_eq!(status, 200);
    assert_eq!(
        run_after, repaired_run,
        "a repeated catch-up after the repair must be an exact no-op"
    );
    let _ = o7d_child.kill();
    let _ = o7d_child.wait();
}

#[test]
fn a_sink_that_lost_already_projected_events_is_fully_recovered_by_catch_up() {
    // A chmod-based "make the sink unwritable mid-run" was tried first and
    // does NOT work on Linux: a file descriptor already open for
    // read-write (the live process's own SQLite connection, opened during
    // Phase 1 before any chmod) keeps writing successfully regardless of
    // the file's mode bits changing afterward — permission checks happen
    // at open(), not at each write(). So this simulates the sink actually
    // LOSING previously-projected data instead (e.g. a lossy restore) by
    // deleting rows directly, bypassing o7-ledger's own API (which has no
    // delete method, by design, being append-only) — the observable
    // end-state a real sink failure would also produce: ledger rows that
    // should exist, per the canonical record, don't.
    let repo = fixture_repo(PASSING_GATE);
    let task_file = repo.path().join("task.md");
    std::fs::write(&task_file, "do the thing").unwrap();
    let work = tempfile::tempdir().unwrap();
    let ledger_path = work.path().join("ledger.sqlite3");
    let claude = fake_claude_dir(0, 0);

    let mut run_child = spawn_o7_run_process(&RunInvocation {
        repo: repo.path(),
        task_file: &task_file,
        ledger_path: &ledger_path,
        runs_dir: &work.path().join("runs"),
        worktree_root: &work.path().join("worktrees"),
        conversation_id: None,
        claude_dir: claude.path(),
        no_ledger: false,
    });
    let status = run_child.wait().unwrap();
    let mut run_stdout = String::new();
    run_child
        .stdout
        .take()
        .unwrap()
        .read_to_string(&mut run_stdout)
        .unwrap();
    assert!(
        status.success(),
        "the fixture is set up to pass cleanly: {run_stdout}"
    );
    let record_dir = PathBuf::from(find_record_dir(&run_stdout));
    let jsonl = read_events_jsonl(&record_dir);
    assert!(jsonl.len() >= 5, "sanity: {jsonl:?}");

    // Directly delete every system.note (the canonical provenance records —
    // the ones the LIVE projector, not a dedicated status-transition method,
    // is responsible for), simulating a sink that lost data it once
    // successfully wrote. Their idempotency_record rows are deleted too: a
    // REAL failure never leaves an idempotency record pointing at a
    // never-written event (both are written in the same transaction), so
    // leaving them behind here would be an unrealistic, inconsistent
    // simulation — catch-up's idempotency check would find a stale
    // "already done" record pointing at a row that no longer exists and
    // error, instead of correctly re-creating it. (`run.completed` is
    // deliberately left alone: it and the run's own `status` column are set
    // atomically in the same transaction by `complete_run`, so a state
    // where one exists without the other can never occur for real, and
    // isn't simulated here.)
    let deleted = {
        let conn = rusqlite::Connection::open(&ledger_path).unwrap();
        let mut ids_stmt = conn
            .prepare("SELECT event_id FROM event WHERE event_type = 'system.note'")
            .unwrap();
        let ids: Vec<String> = ids_stmt
            .query_map([], |row| row.get(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        drop(ids_stmt);
        for id in &ids {
            conn.execute(
                "DELETE FROM idempotency_record WHERE result_reference = ?1",
                [id],
            )
            .unwrap();
        }
        conn.execute("DELETE FROM event WHERE event_type = 'system.note'", [])
            .unwrap()
    };
    assert!(
        deleted >= 5,
        "expected to delete several rows, got {deleted}"
    );

    let recover = Command::new(env!("CARGO_BIN_EXE_o7"))
        .args(["recover", "--ledger"])
        .arg(&ledger_path)
        .args(["--run-dir"])
        .arg(&record_dir)
        .output()
        .unwrap();
    assert!(
        recover.status.success(),
        "catch-up failed: {}",
        String::from_utf8_lossy(&recover.stderr)
    );
    assert!(String::from_utf8_lossy(&recover.stdout).contains("sealed"));

    let (mut o7d_child, addr) = spawn_o7d_process(&ledger_path);
    wait_until_healthy(addr);
    let (status, page) = get(addr, "/api/v1/runs");
    assert_eq!(status, 200);
    let items = page["items"].as_array().unwrap();
    assert_eq!(items.len(), 1, "catch-up must not create a duplicate run");
    assert_eq!(items[0]["status"], "completed");
    let conv_id = items[0]["conversation_id"].as_str().unwrap();
    let (status, events) = get(
        addr,
        &format!("/api/v1/conversations/{conv_id}/events?limit=100"),
    );
    assert_eq!(status, 200);
    let event_types: Vec<&str> = events["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["event_type"].as_str().unwrap())
        .collect();
    assert!(event_types.contains(&"run.completed"));
    let notes_after = event_types.iter().filter(|t| **t == "system.note").count();
    assert!(
        notes_after >= 5,
        "catch-up must have restored the deleted system.note provenance records: {event_types:?}"
    );

    // Re-running catch-up again must be a safe, idempotent no-op.
    let recover2 = Command::new(env!("CARGO_BIN_EXE_o7"))
        .args(["recover", "--ledger"])
        .arg(&ledger_path)
        .args(["--run-dir"])
        .arg(&record_dir)
        .output()
        .unwrap();
    assert!(recover2.status.success());
    let (status, page2) = get(addr, "/api/v1/runs");
    assert_eq!(status, 200);
    assert_eq!(page2, page, "a repeated catch-up must be an exact no-op");

    let _ = o7d_child.kill();
    let _ = o7d_child.wait();
}

/// Runs `o7 run` with a passing gate through to completion, live-projecting
/// into a fresh ledger at `ledger_path` under `work`. Returns the run's
/// record directory. Shared by the catch-up/recovery tests below, which each
/// need a REAL, fully-completed run — every already-projected `system.note`
/// carrying its REAL (non-`None`) `attempt_id` — not a synthetic partial run.
fn run_to_completion(work: &Path, ledger_path: &Path, conversation_id: Option<&str>) -> PathBuf {
    let repo = fixture_repo(PASSING_GATE);
    let task_file = repo.path().join("task.md");
    std::fs::write(&task_file, "do the thing").unwrap();
    let claude = fake_claude_dir(0, 0);

    let mut run_child = spawn_o7_run_process(&RunInvocation {
        repo: repo.path(),
        task_file: &task_file,
        ledger_path,
        runs_dir: &work.join("runs"),
        worktree_root: &work.join("worktrees"),
        conversation_id,
        claude_dir: claude.path(),
        no_ledger: false,
    });
    let status = run_child.wait().unwrap();
    let mut run_stdout = String::new();
    run_child
        .stdout
        .take()
        .unwrap()
        .read_to_string(&mut run_stdout)
        .unwrap();
    assert!(
        status.success(),
        "fixture is set up to pass cleanly: {run_stdout}"
    );
    PathBuf::from(find_record_dir(&run_stdout))
}

/// Delete a `system.note` row (and its idempotency record, kept consistent
/// with the same the same-transaction invariant the other recovery tests
/// rely on) by ledger event id.
fn delete_system_note(ledger_path: &Path, event_id: &str) {
    let conn = rusqlite::Connection::open(ledger_path).unwrap();
    conn.execute(
        "DELETE FROM idempotency_record WHERE result_reference = ?1",
        [event_id],
    )
    .unwrap();
    let deleted = conn
        .execute("DELETE FROM event WHERE event_id = ?1", [event_id])
        .unwrap();
    assert_eq!(
        deleted, 1,
        "expected to delete exactly one row for {event_id}"
    );
}

fn system_note_ids_in_sequence_order(ledger_path: &Path) -> Vec<String> {
    let conn = rusqlite::Connection::open(ledger_path).unwrap();
    let mut stmt = conn
        .prepare("SELECT event_id FROM event WHERE event_type = 'system.note' ORDER BY sequence")
        .unwrap();
    stmt.query_map([], |row| row.get(0))
        .unwrap()
        .map(|r| r.unwrap())
        .collect()
}

/// `(canonical_sequence, event_id)` pairs, sorted by the CANONICAL sequence
/// carried in each note's own payload — not the ledger's own insertion-order
/// `event.sequence` column, which a re-inserted (idempotency-record-lost)
/// event always gets appended to the tail of, regardless of which canonical
/// position it represents (`docs/q-deck/r07-live-ingress.md` 2.3: ledger
/// sequence is distinct from canonical `RunEvent.sequence`). This is the
/// correct axis for verifying "restored in place."
fn system_notes_by_canonical_sequence(ledger_path: &Path) -> Vec<(i64, String)> {
    let conn = rusqlite::Connection::open(ledger_path).unwrap();
    let mut stmt = conn
        .prepare("SELECT event_id, payload_json FROM event WHERE event_type = 'system.note'")
        .unwrap();
    let mut pairs: Vec<(i64, String)> = stmt
        .query_map([], |row| {
            let event_id: String = row.get(0)?;
            let payload_json: String = row.get(1)?;
            Ok((event_id, payload_json))
        })
        .unwrap()
        .map(|r| r.unwrap())
        .map(|(event_id, payload_json)| {
            let payload: serde_json::Value = serde_json::from_str(&payload_json).unwrap();
            let canonical_sequence = payload["canonical_sequence"].as_i64().unwrap();
            (canonical_sequence, event_id)
        })
        .collect();
    pairs.sort_by_key(|(seq, _)| *seq);
    pairs
}

/// Q-Deck R0.7 second re-gate, blocker #2: a catch-up against a run that is
/// ALREADY fully, correctly projected — nothing missing, nothing deleted —
/// must be a genuine no-op. The prior version of this test file only proved
/// full reconstruction from scratch (everything deleted first, including
/// idempotency records), which never exercises the case where
/// `append_system_note`'s idempotency digest must match against a REAL,
/// already-recorded `attempt_id` rather than the `None` a naive "attach
/// read-only to a terminal run" would otherwise use.
#[test]
fn intact_replay_is_a_complete_no_op_for_catch_up() {
    let work = tempfile::tempdir().unwrap();
    let ledger_path = work.path().join("ledger.sqlite3");
    let record_dir = run_to_completion(work.path(), &ledger_path, None);

    let (mut o7d_child, addr) = spawn_o7d_process(&ledger_path);
    wait_until_healthy(addr);
    let (_, before_runs) = get(addr, "/api/v1/runs");
    let conv_id = before_runs["items"][0]["conversation_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let (_, before_events) = get(
        addr,
        &format!("/api/v1/conversations/{conv_id}/events?limit=100"),
    );
    let _ = o7d_child.kill();
    let _ = o7d_child.wait();

    let recover = Command::new(env!("CARGO_BIN_EXE_o7"))
        .args(["recover", "--ledger"])
        .arg(&ledger_path)
        .args(["--run-dir"])
        .arg(&record_dir)
        .output()
        .unwrap();
    assert!(
        recover.status.success(),
        "catch-up against an intact, fully-consistent ledger must succeed, not conflict: {}",
        String::from_utf8_lossy(&recover.stderr)
    );

    let (mut o7d_child, addr) = spawn_o7d_process(&ledger_path);
    wait_until_healthy(addr);
    let (_, after_runs) = get(addr, "/api/v1/runs");
    let (_, after_events) = get(
        addr,
        &format!("/api/v1/conversations/{conv_id}/events?limit=100"),
    );
    assert_eq!(
        after_runs, before_runs,
        "an intact replay must not change the run row at all"
    );
    assert_eq!(
        after_events, before_events,
        "an intact replay must not create, alter, or duplicate a single event"
    );
    let _ = o7d_child.kill();
    let _ = o7d_child.wait();
}

/// Q-Deck R0.7 second re-gate, blocker #2: a sink that fell behind near the
/// END of a run (not one that lost everything) — catch-up must fill in only
/// the missing suffix, leaving the earlier, already-correctly-projected
/// notes (with their REAL original `attempt_id`) completely undisturbed —
/// same `event_id`, not replaced.
#[test]
fn existing_prefix_plus_missing_suffix_is_filled_in_without_disturbing_the_prefix() {
    let work = tempfile::tempdir().unwrap();
    let ledger_path = work.path().join("ledger.sqlite3");
    let record_dir = run_to_completion(work.path(), &ledger_path, None);
    let canonical_len = read_events_jsonl(&record_dir).len();

    let all_ids = system_note_ids_in_sequence_order(&ledger_path);
    assert!(
        all_ids.len() >= 3,
        "need enough notes to leave a real prefix: {all_ids:?}"
    );
    let split = all_ids.len() - 2;
    let (kept_ids, missing_ids) = all_ids.split_at(split);
    for id in missing_ids {
        delete_system_note(&ledger_path, id);
    }

    let recover = Command::new(env!("CARGO_BIN_EXE_o7"))
        .args(["recover", "--ledger"])
        .arg(&ledger_path)
        .args(["--run-dir"])
        .arg(&record_dir)
        .output()
        .unwrap();
    assert!(
        recover.status.success(),
        "catch-up over an intact prefix + missing suffix must succeed: {}",
        String::from_utf8_lossy(&recover.stderr)
    );

    let (mut o7d_child, addr) = spawn_o7d_process(&ledger_path);
    wait_until_healthy(addr);
    let (status, page) = get(addr, "/api/v1/runs");
    assert_eq!(status, 200);
    let conv_id = page["items"][0]["conversation_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let (_, events) = get(
        addr,
        &format!("/api/v1/conversations/{conv_id}/events?limit=100"),
    );
    let restored_ids: Vec<&str> = events["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["event_type"] == "system.note")
        .map(|e| e["event_id"].as_str().unwrap())
        .collect();
    assert_eq!(
        restored_ids.len(),
        canonical_len,
        "the missing suffix must be filled back in: {restored_ids:?}"
    );
    for kept in kept_ids {
        assert!(
            restored_ids.contains(&kept.as_str()),
            "an untouched prefix event must keep its ORIGINAL event_id, not be replaced: \
             {kept} missing from {restored_ids:?}"
        );
    }
    let _ = o7d_child.kill();
    let _ = o7d_child.wait();
}

/// Q-Deck R0.7 second re-gate, blocker #2 (the third named case): a single
/// event missing from the MIDDLE of an otherwise fully-projected stream —
/// not a prefix, not a suffix — must be filled in without disturbing any
/// note before or after it.
#[test]
fn one_skipped_event_among_already_projected_is_restored_in_place() {
    let work = tempfile::tempdir().unwrap();
    let ledger_path = work.path().join("ledger.sqlite3");
    let record_dir = run_to_completion(work.path(), &ledger_path, None);

    let all_ids = system_note_ids_in_sequence_order(&ledger_path);
    assert!(
        all_ids.len() >= 3,
        "need a real middle to delete: {all_ids:?}"
    );
    let middle_index = all_ids.len() / 2;
    let missing_id = all_ids[middle_index].clone();
    // Captured BEFORE deleting: this is the full, intact set of (canonical
    // sequence, event_id) pairs to compare recovery's result against.
    let before = system_notes_by_canonical_sequence(&ledger_path);
    delete_system_note(&ledger_path, &missing_id);
    // The deleted note's OWN idempotency record was deleted along with it
    // (a realistic simulation — a real failure never leaves one without the
    // other), so catch-up legitimately mints a FRESH ledger event_id for it
    // when re-projecting that canonical sequence — there is no idempotency
    // record left to replay against, and a fresh insert always lands at the
    // ledger's own tail (insertion order), never back in the middle of
    // `event.sequence`. So "restored in place" is verified against the
    // CANONICAL sequence carried in each note's payload, not raw ledger
    // insertion order — see `system_notes_by_canonical_sequence`.

    let recover = Command::new(env!("CARGO_BIN_EXE_o7"))
        .args(["recover", "--ledger"])
        .arg(&ledger_path)
        .args(["--run-dir"])
        .arg(&record_dir)
        .output()
        .unwrap();
    assert!(
        recover.status.success(),
        "catch-up over a single missing middle event must succeed: {}",
        String::from_utf8_lossy(&recover.stderr)
    );

    let after = system_notes_by_canonical_sequence(&ledger_path);
    let after_sequences: Vec<i64> = after.iter().map(|(seq, _)| *seq).collect();
    let expected_sequences: Vec<i64> = (0..before.len() as i64).collect();
    assert_eq!(
        after_sequences, expected_sequences,
        "every canonical sequence, including the restored one, must be present exactly once: {after:?}"
    );
    for (before_seq, before_id) in &before {
        if *before_id == missing_id {
            continue;
        }
        let after_id = after
            .iter()
            .find(|(seq, _)| seq == before_seq)
            .map(|(_, id)| id);
        assert_eq!(
            after_id,
            Some(before_id),
            "an untouched neighbor (canonical seq {before_seq}) must keep its exact original id: {after:?}"
        );
    }
}

/// Q-Deck R0.7 second re-gate, blocker #1: catch-up must resolve the SAME
/// conversation an explicit `--conversation-id` run used — not guess "New"
/// and either fail (an idempotency-digest mismatch on the run's own
/// `create_run_with_id` key) or create a phantom second conversation.
#[test]
fn catch_up_resolves_the_same_explicit_conversation_the_original_run_used() {
    let work = tempfile::tempdir().unwrap();
    let ledger_path = work.path().join("ledger.sqlite3");

    let existing_conversation_id = {
        let ledger = o7_ledger::SqliteLedger::open(&ledger_path).unwrap();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(ledger.create_conversation(None))
            .unwrap()
            .conversation_id
            .as_str()
            .to_owned()
    };

    let record_dir = run_to_completion(work.path(), &ledger_path, Some(&existing_conversation_id));

    // Delete one note so catch-up has real work to do — proving this isn't
    // merely a no-op that happens to pass through unrelated code.
    let all_ids = system_note_ids_in_sequence_order(&ledger_path);
    let last_id = all_ids.last().expect("at least one note").clone();
    delete_system_note(&ledger_path, &last_id);

    let recover = Command::new(env!("CARGO_BIN_EXE_o7"))
        .args(["recover", "--ledger"])
        .arg(&ledger_path)
        .args(["--run-dir"])
        .arg(&record_dir)
        .output()
        .unwrap();
    assert!(
        recover.status.success(),
        "catch-up under an explicit --conversation-id must succeed: {}",
        String::from_utf8_lossy(&recover.stderr)
    );

    let (mut o7d_child, addr) = spawn_o7d_process(&ledger_path);
    wait_until_healthy(addr);
    let (status, convs) = get(addr, "/api/v1/conversations");
    assert_eq!(status, 200);
    assert_eq!(
        convs["items"].as_array().unwrap().len(),
        1,
        "catch-up must not create a phantom second conversation"
    );
    let (status, runs) = get(addr, "/api/v1/runs");
    assert_eq!(status, 200);
    assert_eq!(
        runs["items"][0]["conversation_id"], existing_conversation_id,
        "catch-up must resolve the SAME explicit conversation the original run used"
    );
    let _ = o7d_child.kill();
    let _ = o7d_child.wait();
}

#[test]
fn a_valid_existing_conversation_id_is_used_verbatim() {
    let repo = fixture_repo(PASSING_GATE);
    let task_file = repo.path().join("task.md");
    std::fs::write(&task_file, "do the thing").unwrap();
    let work = tempfile::tempdir().unwrap();
    let ledger_path = work.path().join("ledger.sqlite3");
    let claude = fake_claude_dir(0, 0);

    let existing_conversation_id = {
        let ledger = o7_ledger::SqliteLedger::open(&ledger_path).unwrap();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(ledger.create_conversation(None))
            .unwrap()
            .conversation_id
            .as_str()
            .to_owned()
    };

    let mut run_child = spawn_o7_run_process(&RunInvocation {
        repo: repo.path(),
        task_file: &task_file,
        ledger_path: &ledger_path,
        runs_dir: &work.path().join("runs"),
        worktree_root: &work.path().join("worktrees"),
        conversation_id: Some(&existing_conversation_id),
        claude_dir: claude.path(),
        no_ledger: false,
    });
    let status = run_child.wait().unwrap();
    assert!(status.success());

    let (mut o7d_child, addr) = spawn_o7d_process(&ledger_path);
    wait_until_healthy(addr);
    let (status, page) = get(
        addr,
        &format!("/api/v1/conversations/{existing_conversation_id}"),
    );
    assert_eq!(status, 200);
    let (status, runs) = get(addr, "/api/v1/runs");
    assert_eq!(status, 200);
    assert_eq!(
        runs["items"][0]["conversation_id"], existing_conversation_id,
        "the caller-supplied conversation must be used, not a new one"
    );
    let _ = page;
    let _ = o7d_child.kill();
    let _ = o7d_child.wait();
}

#[test]
fn an_unknown_conversation_id_fails_before_the_agent_ever_starts() {
    let repo = fixture_repo(PASSING_GATE);
    let task_file = repo.path().join("task.md");
    std::fs::write(&task_file, "do the thing").unwrap();
    let work = tempfile::tempdir().unwrap();
    let ledger_path = work.path().join("ledger.sqlite3");
    let claude = fake_claude_dir(0, 0);

    let mut run_child = spawn_o7_run_process(&RunInvocation {
        repo: repo.path(),
        task_file: &task_file,
        ledger_path: &ledger_path,
        runs_dir: &work.path().join("runs"),
        worktree_root: &work.path().join("worktrees"),
        conversation_id: Some("does-not-exist-anywhere"),
        claude_dir: claude.path(),
        no_ledger: false,
    });
    let status = run_child.wait().unwrap();
    assert!(
        !status.success(),
        "an unknown --conversation-id must fail the run"
    );
    assert!(
        !claude_was_invoked(claude.path()),
        "the agent must never start when --conversation-id doesn't resolve"
    );
}

#[test]
fn conversation_id_without_ledger_is_rejected_at_cli_parse_time() {
    let repo = fixture_repo(PASSING_GATE);
    let task_file = repo.path().join("task.md");
    std::fs::write(&task_file, "do the thing").unwrap();
    let work = tempfile::tempdir().unwrap();
    let claude = fake_claude_dir(0, 0);

    let mut run_child = spawn_o7_run_process(&RunInvocation {
        repo: repo.path(),
        task_file: &task_file,
        ledger_path: &work.path().join("unused-ledger.sqlite3"),
        runs_dir: &work.path().join("runs"),
        worktree_root: &work.path().join("worktrees"),
        conversation_id: Some("whatever"),
        claude_dir: claude.path(),
        no_ledger: true, // --conversation-id given, --ledger deliberately not
    });
    let status = run_child.wait().unwrap();
    assert!(
        !status.success(),
        "--conversation-id without --ledger must be rejected"
    );
    let mut stderr = String::new();
    run_child
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut stderr)
        .unwrap();
    assert!(
        stderr.contains("conversation-id") || stderr.contains("ledger"),
        "clap's error should mention the offending flags: {stderr}"
    );
    assert!(
        !claude_was_invoked(claude.path()),
        "a CLI parse rejection must never reach the agent"
    );
}

#[test]
fn no_ledger_flag_creates_no_sqlite_file_and_preserves_legacy_behavior() {
    let repo = fixture_repo(PASSING_GATE);
    let task_file = repo.path().join("task.md");
    std::fs::write(&task_file, "do the thing").unwrap();
    let work = tempfile::tempdir().unwrap();
    let would_be_ledger_path = work.path().join("never-created.sqlite3");
    let claude = fake_claude_dir(0, 0);

    let mut run_child = spawn_o7_run_process(&RunInvocation {
        repo: repo.path(),
        task_file: &task_file,
        ledger_path: &would_be_ledger_path,
        runs_dir: &work.path().join("runs"),
        worktree_root: &work.path().join("worktrees"),
        conversation_id: None,
        claude_dir: claude.path(),
        no_ledger: true,
    });
    let status = run_child.wait().unwrap();
    assert!(status.success());
    assert!(
        !would_be_ledger_path.exists(),
        "no --ledger must never create a SQLite file as a side effect"
    );

    let mut run_stdout = String::new();
    run_child
        .stdout
        .take()
        .unwrap()
        .read_to_string(&mut run_stdout)
        .unwrap();
    let record_dir = PathBuf::from(find_record_dir(&run_stdout));
    // The legacy flat record is still fully present and replayable.
    assert!(record_dir.join("events.jsonl").exists());
    assert!(record_dir.join("meta.json").exists());
    let replay = Command::new(env!("CARGO_BIN_EXE_o7"))
        .arg("replay")
        .arg(&record_dir)
        .output()
        .unwrap();
    assert!(replay.status.success());
}
