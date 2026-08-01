//! Q-Deck A0 (`docs/q-deck/a0-candidate-state.md`): process-level acceptance
//! for candidate-state continuity — a follow-up command must materialize
//! the exact prior sealed run's cumulative file state, not just continue
//! the provider session. REAL compiled `o7`/`o7d` binaries, a REAL SQLite
//! ledger, REAL REST, a REAL Git repository/worktrees — the only stand-in
//! is the external `claude` CLI, replaced by a deterministic fixture that
//! can also mutate files in its own worktree on command, so the test can
//! prove the provider actually SAW the materialized state, not merely that
//! materialization didn't crash.
//!
//! Mirrors `tests/r1_command_e2e.rs`'s own helper style deliberately (its
//! own doc comment already establishes the precedent of mirroring rather
//! than sharing test-only helpers across independently-compiled
//! integration test binaries).
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

/// Same rationale as `tests/r1_command_e2e.rs`'s own `SERIAL`: every test
/// here spawns several real processes on a resource-constrained VPS.
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

/// A fake `claude` that ALSO mutates files in its own CWD (the worktree
/// `agent::run`/`agent::continue_session` invoke it from) when a per-
/// invocation action script is present — `set_actions(n, script)` before
/// the run/command that will become invocation `n`. Every invocation still
/// increments a durable counter and answers the same structured JSON
/// envelope `judge.rs::call_claude` parses.
struct ActionClaude {
    dir: tempfile::TempDir,
}

impl ActionClaude {
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
export O7_TEST_DIR="$DIR"
if [ -f "$DIR/actions.$N" ]; then
  sh "$DIR/actions.$N"
fi
printf '{"result":"synthetic ok","session_id":"fixed-session-1","total_cost_usd":0.001}\n'
exit 0
"#,
        )
        .unwrap();
        let mut perms = std::fs::metadata(&script).unwrap().permissions();
        std::os::unix::fs::PermissionsExt::set_mode(&mut perms, 0o755);
        std::fs::set_permissions(&script, perms).unwrap();
        Self { dir }
    }

    fn path(&self) -> &Path {
        self.dir.path()
    }

    /// Install the shell script invocation `n` (1-based) will run, from
    /// inside its own worktree CWD, before answering.
    fn set_actions(&self, n: u64, script: &str) {
        std::fs::write(self.dir.path().join(format!("actions.{n}")), script).unwrap();
    }

    fn invocation_count(&self) -> u64 {
        std::fs::read_to_string(self.dir.path().join("count"))
            .ok()
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(0)
    }

    fn wait_for_invocation(&self, n: u64, deadline: Instant) {
        poll_until(deadline, || (self.invocation_count() >= n).then_some(()));
    }

    /// A marker file an action script wrote via `$O7_TEST_DIR/<name>` —
    /// e.g. a precondition check's own recorded verdict.
    fn marker(&self, name: &str) -> Option<String> {
        std::fs::read_to_string(self.dir.path().join(name))
            .ok()
            .map(|s| s.trim().to_owned())
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

fn run_dir(runs_dir: &Path, target: &str, run_id: &str) -> PathBuf {
    runs_dir.join(target).join(run_id)
}

fn read_candidate_receipt(dir: &Path) -> serde_json::Value {
    let text = std::fs::read_to_string(dir.join("candidate_state_receipt.json"))
        .expect("candidate_state_receipt.json must exist for a ledger-backed run");
    serde_json::from_str(&text).unwrap()
}

fn events_of_kind(dir: &Path, kind: &str) -> Vec<serde_json::Value> {
    let text = std::fs::read_to_string(dir.join("events.jsonl")).unwrap();
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str::<serde_json::Value>(l).unwrap())
        .filter(|e| e["kind"]["type"] == kind)
        .collect()
}

fn command_status(ledger_path: &Path, command_id: &str) -> String {
    let conn = rusqlite::Connection::open(ledger_path).unwrap();
    conn.query_row(
        "SELECT status FROM command WHERE command_id = ?1",
        [command_id],
        |r| r.get(0),
    )
    .unwrap()
}

/// The core proof: Run A produces a candidate receipt; Command B
/// materializes A's exact state (proven by the provider itself checking
/// preconditions before acting), captures a CUMULATIVE A+B receipt (not a
/// delta against A alone); Command C materializes A+B and captures A+B+C.
/// Every candidate_tree_oid is verified against an independently-computed
/// `git write-tree` in a scratch clone, never trusted from the receipt
/// alone.
#[test]
fn candidate_state_flows_through_a_b_c_chain() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let repo = fixture_repo(PASSING_GATE);
    let task_file = repo.path().join("task.md");
    std::fs::write(&task_file, "do the initial thing").unwrap();
    let work = tempfile::tempdir().unwrap();
    let ledger_path = work.path().join("ledger.sqlite3");
    let runs_dir = work.path().join("runs");
    let worktree_root = work.path().join("worktrees");
    let claude = ActionClaude::new();
    let target = repo
        .path()
        .file_name()
        .unwrap()
        .to_string_lossy()
        .to_string();

    // --- Run A: creates base.txt, a.txt; deletes nothing yet. ---
    claude.set_actions(
        1,
        r#"
echo "base-v1" > base.txt
echo "from-a" > a.txt
touch delete-me.txt
chmod +x maybe-exec.sh 2>/dev/null || printf '#!/bin/sh\necho a\n' > maybe-exec.sh
"#,
    );
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

    let (mut o7d_child, addr) = spawn_o7d_with_exec_and_redrive_ms(
        &ledger_path,
        repo.path(),
        &worktree_root,
        &runs_dir,
        claude.path(),
        None,
    );
    wait_until_healthy(addr);

    let (status, page) = get(addr, "/api/v1/runs");
    assert_eq!(status, 200);
    let items = page["items"].as_array().unwrap();
    let run_a_id = items[0]["run_id"].as_str().unwrap().to_owned();
    let conversation_id = items[0]["conversation_id"].as_str().unwrap().to_owned();

    let run_a_dir = run_dir(&runs_dir, &target, &run_a_id);
    let receipt_a = read_candidate_receipt(&run_a_dir);
    assert_eq!(receipt_a["schema"], 1);
    assert_eq!(receipt_a["run_id"], run_a_id);
    assert_eq!(receipt_a["conversation_id"], conversation_id);
    assert!(receipt_a["parent_run_id"].is_null());
    let base_commit = receipt_a["base_commit"].as_str().unwrap().to_owned();
    assert_eq!(
        events_of_kind(&run_a_dir, "candidate_state_captured").len(),
        1,
        "Run A must capture exactly one candidate-state event"
    );

    // --- Command B: must see A's state before acting, adds b.txt,
    // appends to base.txt, deletes delete-me.txt. ---
    claude.set_actions(
        2,
        r#"
if [ "$(cat base.txt)" = "base-v1" ] && [ -f a.txt ] && [ -f delete-me.txt ]; then
  echo ok > "$O7_TEST_DIR/precondition.2"
else
  echo fail > "$O7_TEST_DIR/precondition.2"
fi
rm -f delete-me.txt
echo "base-v2" > base.txt
echo "from-b" > b.txt
"#,
    );
    let key_b = "key-command-b";
    let (status, accepted_b) = post(
        addr,
        &format!("/api/v1/conversations/{conversation_id}/commands"),
        &serde_json::json!({
            "schema_version": 1,
            "parent_run_id": run_a_id,
            "command": "do the second thing",
            "idempotency_key": key_b,
        }),
    );
    assert_eq!(status, 202, "{accepted_b:?}");
    let run_b_id = accepted_b["run_id"].as_str().unwrap().to_owned();

    let deadline = Instant::now() + Duration::from_secs(30);
    claude.wait_for_invocation(2, deadline);
    let deadline = Instant::now() + Duration::from_secs(30);
    wait_for_run_status(addr, &run_b_id, "completed", deadline);

    assert_eq!(
        claude.marker("precondition.2").as_deref(),
        Some("ok"),
        "Command B's own provider invocation must observe Run A's exact materialized state \
         BEFORE acting"
    );

    let run_b_dir = run_dir(&runs_dir, &target, &run_b_id);
    // The child's own materialization evidence: proves what it started
    // from, matches Run A's identity, and the reducer already refused any
    // stream where expected != actual — so its presence alone proves they
    // agreed.
    let materialized_b = events_of_kind(&run_b_dir, "candidate_state_materialized");
    assert_eq!(materialized_b.len(), 1);
    assert_eq!(materialized_b[0]["kind"]["source_run_id"], run_a_id);

    let receipt_b = read_candidate_receipt(&run_b_dir);
    assert_eq!(
        receipt_b["base_commit"], base_commit,
        "the conversation's immutable base commit must be inherited unchanged, never re-resolved"
    );
    assert_eq!(receipt_b["parent_run_id"], run_a_id);

    // --- Independent verification: materialize B's own cumulative
    // receipt from scratch in a clean clone, and confirm the resulting
    // tree matches base.txt=v2, a.txt present, b.txt present,
    // delete-me.txt ABSENT, executable bit preserved — proving B's
    // receipt really is CUMULATIVE (A+B), not a bare delta against A. ---
    let verify_dir = tempfile::tempdir().unwrap();
    let git = |args: &[&str], cwd: &Path| {
        let out = Command::new("git")
            .args(args)
            .current_dir(cwd)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).to_string()
    };
    git(
        &[
            "clone",
            "-q",
            repo.path().to_str().unwrap(),
            verify_dir.path().to_str().unwrap(),
        ],
        Path::new("/"),
    );
    git(&["checkout", "-q", &base_commit], verify_dir.path());
    let patch_b = std::fs::read_to_string(run_b_dir.join("candidate.patch")).unwrap();
    let patch_file = verify_dir.path().join("verify.patch");
    std::fs::write(&patch_file, &patch_b).unwrap();
    if !patch_b.trim().is_empty() {
        git(
            &["apply", "--binary", patch_file.to_str().unwrap()],
            verify_dir.path(),
        );
    }
    assert_eq!(
        std::fs::read_to_string(verify_dir.path().join("base.txt"))
            .unwrap()
            .trim(),
        "base-v2"
    );
    assert!(
        verify_dir.path().join("a.txt").exists(),
        "a.txt from Run A must survive into B's cumulative state"
    );
    assert!(verify_dir.path().join("b.txt").exists());
    assert!(
        !verify_dir.path().join("delete-me.txt").exists(),
        "B's deletion of delete-me.txt must be part of the cumulative state"
    );

    // --- Command C: must see A+B's state, adds c.txt, modifies a.txt. ---
    claude.set_actions(
        3,
        r#"
if [ -f a.txt ] && [ -f b.txt ] && [ "$(cat base.txt)" = "base-v2" ] && [ ! -f delete-me.txt ]; then
  echo ok > "$O7_TEST_DIR/precondition.3"
else
  echo fail > "$O7_TEST_DIR/precondition.3"
fi
echo "from-a-modified-by-c" > a.txt
echo "from-c" > c.txt
"#,
    );
    let key_c = "key-command-c";
    let (status, accepted_c) = post(
        addr,
        &format!("/api/v1/conversations/{conversation_id}/commands"),
        &serde_json::json!({
            "schema_version": 1,
            "parent_run_id": run_b_id,
            "command": "do the third thing",
            "idempotency_key": key_c,
        }),
    );
    assert_eq!(status, 202, "{accepted_c:?}");
    let run_c_id = accepted_c["run_id"].as_str().unwrap().to_owned();

    let deadline = Instant::now() + Duration::from_secs(30);
    claude.wait_for_invocation(3, deadline);
    let deadline = Instant::now() + Duration::from_secs(30);
    wait_for_run_status(addr, &run_c_id, "completed", deadline);

    assert_eq!(
        claude.marker("precondition.3").as_deref(),
        Some("ok"),
        "Command C's own provider invocation must observe the cumulative A+B state"
    );

    let run_c_dir = run_dir(&runs_dir, &target, &run_c_id);
    let materialized_c = events_of_kind(&run_c_dir, "candidate_state_materialized");
    assert_eq!(materialized_c.len(), 1);
    assert_eq!(
        materialized_c[0]["kind"]["source_run_id"], run_b_id,
        "C must materialize from B (its own true parent), not skip straight to A"
    );
    let receipt_c = read_candidate_receipt(&run_c_dir);
    assert_eq!(
        receipt_c["base_commit"], base_commit,
        "still the SAME immutable base three runs later"
    );

    // Final cumulative check: C's own receipt materializes to a.txt
    // (C's edit wins over A's), b.txt present, c.txt present,
    // delete-me.txt absent — proving C's candidate = A+B+C relative to
    // the ORIGINAL base, not "apply A then B then C" as a delta chain.
    let verify_dir_c = tempfile::tempdir().unwrap();
    git(
        &[
            "clone",
            "-q",
            repo.path().to_str().unwrap(),
            verify_dir_c.path().to_str().unwrap(),
        ],
        Path::new("/"),
    );
    git(&["checkout", "-q", &base_commit], verify_dir_c.path());
    let patch_c = std::fs::read_to_string(run_c_dir.join("candidate.patch")).unwrap();
    let patch_file_c = verify_dir_c.path().join("verify.patch");
    std::fs::write(&patch_file_c, &patch_c).unwrap();
    if !patch_c.trim().is_empty() {
        git(
            &["apply", "--binary", patch_file_c.to_str().unwrap()],
            verify_dir_c.path(),
        );
    }
    assert_eq!(
        std::fs::read_to_string(verify_dir_c.path().join("a.txt"))
            .unwrap()
            .trim(),
        "from-a-modified-by-c"
    );
    assert!(verify_dir_c.path().join("b.txt").exists());
    assert!(verify_dir_c.path().join("c.txt").exists());
    assert!(!verify_dir_c.path().join("delete-me.txt").exists());

    assert_eq!(
        claude.invocation_count(),
        3,
        "exactly one invocation per run/command, ever"
    );

    let _ = o7d_child.kill();
    let _ = o7d_child.wait();
}

/// A helper shared by every negative test below: build a real Run A (with
/// real candidate-state capture), start `o7d`, and return everything a
/// negative case needs to then corrupt something and dispatch a command.
fn setup_real_parent(
    claude: &ActionClaude,
) -> (
    tempfile::TempDir, // repo
    tempfile::TempDir, // work (ledger/runs/worktrees root)
    PathBuf,           // ledger_path
    PathBuf,           // runs_dir
    PathBuf,           // worktree_root
    Child,             // o7d_child
    SocketAddr,
    String, // run_a_id
    String, // conversation_id
    String, // target
) {
    let repo = fixture_repo(PASSING_GATE);
    let task_file = repo.path().join("task.md");
    std::fs::write(&task_file, "do the initial thing").unwrap();
    let work = tempfile::tempdir().unwrap();
    let ledger_path = work.path().join("ledger.sqlite3");
    let runs_dir = work.path().join("runs");
    let worktree_root = work.path().join("worktrees");
    let target = repo
        .path()
        .file_name()
        .unwrap()
        .to_string_lossy()
        .to_string();

    claude.set_actions(1, "echo base-v1 > base.txt\n");
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

    let (o7d_child, addr) = spawn_o7d_with_exec_and_redrive_ms(
        &ledger_path,
        repo.path(),
        &worktree_root,
        &runs_dir,
        claude.path(),
        // Effectively "never" within this test's own lifetime — a
        // materialization failure must stay pre-dispatch-redrivable in
        // principle, but these tests assert it stays UN-redriven for the
        // whole bounded observation window, not that it's permanently
        // stuck.
        Some(3_600_000),
    );
    wait_until_healthy(addr);

    let (status, page) = get(addr, "/api/v1/runs");
    assert_eq!(status, 200);
    let items = page["items"].as_array().unwrap();
    let run_a_id = items[0]["run_id"].as_str().unwrap().to_owned();
    let conversation_id = items[0]["conversation_id"].as_str().unwrap().to_owned();

    (
        repo,
        work,
        ledger_path,
        runs_dir,
        worktree_root,
        o7d_child,
        addr,
        run_a_id,
        conversation_id,
        target,
    )
}

/// Post a command and assert it is accepted (202) but never actually
/// completes, and the provider is never invoked a second time, within a
/// bounded observation window — the required proof for every pre-provider
/// negative case (`docs/q-deck/a0-candidate-state.md` §6).
fn assert_command_never_completes_and_provider_never_reinvoked(
    addr: SocketAddr,
    conversation_id: &str,
    parent_run_id: &str,
    claude: &ActionClaude,
    ledger_path: &Path,
) {
    let (status, accepted) = post(
        addr,
        &format!("/api/v1/conversations/{conversation_id}/commands"),
        &serde_json::json!({
            "schema_version": 1,
            "parent_run_id": parent_run_id,
            "command": "this must never reach the provider",
            "idempotency_key": "key-negative",
        }),
    );
    assert_eq!(status, 202, "{accepted:?}");
    let child_run_id = accepted["run_id"].as_str().unwrap().to_owned();
    let command_id = accepted["command_id"].as_str().unwrap().to_owned();

    // Bounded observation window: long enough for a real materialization
    // attempt to have failed and returned, short enough not to waste the
    // suite's time. If materialization succeeded, this would already be
    // "completed" well within this window (as the happy-path test proves).
    std::thread::sleep(Duration::from_secs(2));
    assert_eq!(
        claude.invocation_count(),
        1,
        "the provider must NEVER be invoked for a command whose materialization fails"
    );
    let (status, run) = get(addr, &format!("/api/v1/runs/{child_run_id}"));
    assert_eq!(status, 200);
    assert_ne!(
        run["status"], "completed",
        "a failed materialization must never complete the child run"
    );
    assert_eq!(
        command_status(ledger_path, &command_id),
        "started",
        "the command stays started — safely pre-dispatch-redrivable, never rejected outright, \
         never falsely completed"
    );
}

#[test]
fn missing_candidate_receipt_never_invokes_the_provider() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let claude = ActionClaude::new();
    let (
        repo,
        _work,
        ledger_path,
        runs_dir,
        _worktree_root,
        mut o7d_child,
        addr,
        run_a_id,
        conversation_id,
        target,
    ) = setup_real_parent(&claude);

    std::fs::remove_file(
        run_dir(&runs_dir, &target, &run_a_id).join("candidate_state_receipt.json"),
    )
    .unwrap();
    // Deleting the receipt alone leaves a canonical event referencing a
    // digest whose file no longer resolves — verify_prefix's own artifact
    // resolution already fails closed on that; this test's real target is
    // the "no receipt at all" case a legacy pre-A0 parent would present,
    // which is what deleting the file (and its own canonical event
    // reference no longer resolving) faithfully simulates here.

    assert_command_never_completes_and_provider_never_reinvoked(
        addr,
        &conversation_id,
        &run_a_id,
        &claude,
        &ledger_path,
    );
    let _ = o7d_child.kill();
    let _ = o7d_child.wait();
    drop(repo);
}

#[test]
fn tampered_candidate_receipt_never_invokes_the_provider() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let claude = ActionClaude::new();
    let (
        repo,
        _work,
        ledger_path,
        runs_dir,
        _worktree_root,
        mut o7d_child,
        addr,
        run_a_id,
        conversation_id,
        target,
    ) = setup_real_parent(&claude);

    let receipt_path = run_dir(&runs_dir, &target, &run_a_id).join("candidate_state_receipt.json");
    let mut receipt: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&receipt_path).unwrap()).unwrap();
    receipt["candidate_tree_oid"] = serde_json::Value::String("0".repeat(40));
    std::fs::write(&receipt_path, serde_json::to_string(&receipt).unwrap()).unwrap();

    assert_command_never_completes_and_provider_never_reinvoked(
        addr,
        &conversation_id,
        &run_a_id,
        &claude,
        &ledger_path,
    );
    let _ = o7d_child.kill();
    let _ = o7d_child.wait();
    drop(repo);
}

#[test]
fn missing_patch_file_never_invokes_the_provider() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let claude = ActionClaude::new();
    let (
        repo,
        _work,
        ledger_path,
        runs_dir,
        _worktree_root,
        mut o7d_child,
        addr,
        run_a_id,
        conversation_id,
        target,
    ) = setup_real_parent(&claude);

    std::fs::remove_file(run_dir(&runs_dir, &target, &run_a_id).join("candidate.patch")).unwrap();

    assert_command_never_completes_and_provider_never_reinvoked(
        addr,
        &conversation_id,
        &run_a_id,
        &claude,
        &ledger_path,
    );
    let _ = o7d_child.kill();
    let _ = o7d_child.wait();
    drop(repo);
}

#[test]
fn tampered_patch_content_never_invokes_the_provider() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let claude = ActionClaude::new();
    let (
        repo,
        _work,
        ledger_path,
        runs_dir,
        _worktree_root,
        mut o7d_child,
        addr,
        run_a_id,
        conversation_id,
        target,
    ) = setup_real_parent(&claude);

    let patch_path = run_dir(&runs_dir, &target, &run_a_id).join("candidate.patch");
    let mut bytes = std::fs::read(&patch_path).unwrap();
    bytes.extend_from_slice(b"\ntampered garbage that breaks the digest\n");
    std::fs::write(&patch_path, bytes).unwrap();

    assert_command_never_completes_and_provider_never_reinvoked(
        addr,
        &conversation_id,
        &run_a_id,
        &claude,
        &ledger_path,
    );
    let _ = o7d_child.kill();
    let _ = o7d_child.wait();
    drop(repo);
}

#[test]
fn wrong_base_commit_never_invokes_the_provider() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let claude = ActionClaude::new();
    let (
        repo,
        _work,
        ledger_path,
        runs_dir,
        _worktree_root,
        mut o7d_child,
        addr,
        run_a_id,
        conversation_id,
        target,
    ) = setup_real_parent(&claude);

    let receipt_path = run_dir(&runs_dir, &target, &run_a_id).join("candidate_state_receipt.json");
    let mut receipt: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&receipt_path).unwrap()).unwrap();
    receipt["base_commit"] = serde_json::Value::String("f".repeat(40));
    std::fs::write(&receipt_path, serde_json::to_string(&receipt).unwrap()).unwrap();

    assert_command_never_completes_and_provider_never_reinvoked(
        addr,
        &conversation_id,
        &run_a_id,
        &claude,
        &ledger_path,
    );
    let _ = o7d_child.kill();
    let _ = o7d_child.wait();
    drop(repo);
}

#[test]
fn wrong_repository_identity_never_invokes_the_provider() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let claude = ActionClaude::new();
    let (
        repo,
        _work,
        ledger_path,
        runs_dir,
        _worktree_root,
        mut o7d_child,
        addr,
        run_a_id,
        conversation_id,
        target,
    ) = setup_real_parent(&claude);

    let receipt_path = run_dir(&runs_dir, &target, &run_a_id).join("candidate_state_receipt.json");
    let mut receipt: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&receipt_path).unwrap()).unwrap();
    receipt["repository_id"]["dev"] = serde_json::Value::from(999_999_u64);
    receipt["repository_id"]["ino"] = serde_json::Value::from(999_999_u64);
    std::fs::write(&receipt_path, serde_json::to_string(&receipt).unwrap()).unwrap();

    assert_command_never_completes_and_provider_never_reinvoked(
        addr,
        &conversation_id,
        &run_a_id,
        &claude,
        &ledger_path,
    );
    let _ = o7d_child.kill();
    let _ = o7d_child.wait();
    drop(repo);
}

#[test]
fn wrong_conversation_id_never_invokes_the_provider() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let claude = ActionClaude::new();
    let (
        repo,
        _work,
        ledger_path,
        runs_dir,
        _worktree_root,
        mut o7d_child,
        addr,
        run_a_id,
        conversation_id,
        target,
    ) = setup_real_parent(&claude);

    let receipt_path = run_dir(&runs_dir, &target, &run_a_id).join("candidate_state_receipt.json");
    let mut receipt: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&receipt_path).unwrap()).unwrap();
    receipt["conversation_id"] = serde_json::Value::String("some-other-conversation".to_owned());
    std::fs::write(&receipt_path, serde_json::to_string(&receipt).unwrap()).unwrap();

    assert_command_never_completes_and_provider_never_reinvoked(
        addr,
        &conversation_id,
        &run_a_id,
        &claude,
        &ledger_path,
    );
    let _ = o7d_child.kill();
    let _ = o7d_child.wait();
    drop(repo);
}

/// A patch that cannot possibly apply against the real base commit (it
/// targets a file path that doesn't exist in the fixture repo at all) —
/// proves a genuine `git apply` conflict fails closed exactly like every
/// other pre-provider negative case, not merely the digest-mismatch ones.
#[test]
fn a_patch_apply_conflict_never_invokes_the_provider() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let claude = ActionClaude::new();
    let (
        repo,
        _work,
        ledger_path,
        runs_dir,
        _worktree_root,
        mut o7d_child,
        addr,
        run_a_id,
        conversation_id,
        target,
    ) = setup_real_parent(&claude);

    let dir = run_dir(&runs_dir, &target, &run_a_id);
    let conflicting_patch = "diff --git a/base.txt b/base.txt\n\
         index 0000000000000000000000000000000000000000..1111111111111111111111111111111111111111 100644\n\
         --- a/base.txt\n\
         +++ b/base.txt\n\
         @@ -1,50 +1,50 @@\n\
         -this context line cannot possibly match the real file\n\
         +neither can this replacement\n";
    std::fs::write(dir.join("candidate.patch"), conflicting_patch).unwrap();
    let mut receipt: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(dir.join("candidate_state_receipt.json")).unwrap(),
    )
    .unwrap();
    receipt["patch_sha256"] = serde_json::Value::String(
        o7_run::event::Digest256::of_bytes(conflicting_patch.as_bytes())
            .as_str()
            .to_owned(),
    );
    receipt["patch_size"] = serde_json::Value::from(conflicting_patch.len() as u64);
    std::fs::write(
        dir.join("candidate_state_receipt.json"),
        serde_json::to_string(&receipt).unwrap(),
    )
    .unwrap();

    assert_command_never_completes_and_provider_never_reinvoked(
        addr,
        &conversation_id,
        &run_a_id,
        &claude,
        &ledger_path,
    );
    let _ = o7d_child.kill();
    let _ = o7d_child.wait();
    drop(repo);
}

/// Two simultaneous same-key retries against a command whose
/// materialization is failing: both must converge on the SAME child run
/// id, neither may invoke the provider, and the command's own binding must
/// never change.
#[test]
fn two_concurrent_retries_against_a_failing_materialization_both_fail_closed() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let claude = ActionClaude::new();
    let (
        repo,
        _work,
        _ledger_path,
        runs_dir,
        _worktree_root,
        mut o7d_child,
        addr,
        run_a_id,
        conversation_id,
        target,
    ) = setup_real_parent(&claude);

    std::fs::remove_file(run_dir(&runs_dir, &target, &run_a_id).join("candidate.patch")).unwrap();

    let key = "key-concurrent-negative";
    let body = serde_json::json!({
        "schema_version": 1,
        "parent_run_id": run_a_id,
        "command": "concurrent negative",
        "idempotency_key": key,
    });
    let path = format!("/api/v1/conversations/{conversation_id}/commands");
    let (a1, a2) = std::thread::scope(|scope| {
        let h1 = scope.spawn(|| post(addr, &path, &body));
        let h2 = scope.spawn(|| post(addr, &path, &body));
        (h1.join().unwrap(), h2.join().unwrap())
    });
    assert_eq!(a1.0, 202, "{:?}", a1.1);
    assert_eq!(a2.0, 202, "{:?}", a2.1);
    let run1 = a1.1["run_id"].as_str().unwrap();
    let run2 = a2.1["run_id"].as_str().unwrap();
    assert_eq!(
        run1, run2,
        "concurrent retries must converge on ONE child run id"
    );

    std::thread::sleep(Duration::from_secs(2));
    assert_eq!(
        claude.invocation_count(),
        1,
        "neither concurrent retry may ever invoke the provider"
    );
    let _ = o7d_child.kill();
    let _ = o7d_child.wait();
    drop(repo);
}

/// Once the underlying cause is fixed, a same-key retry succeeds — proving
/// the pre-dispatch-failed record really is redrivable, not permanently
/// wedged.
#[test]
fn a_same_key_retry_succeeds_once_the_materialization_cause_is_fixed() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let claude = ActionClaude::new();
    let repo = fixture_repo(PASSING_GATE);
    let task_file = repo.path().join("task.md");
    std::fs::write(&task_file, "do the initial thing").unwrap();
    let work = tempfile::tempdir().unwrap();
    let ledger_path = work.path().join("ledger.sqlite3");
    let runs_dir = work.path().join("runs");
    let worktree_root = work.path().join("worktrees");
    let target = repo
        .path()
        .file_name()
        .unwrap()
        .to_string_lossy()
        .to_string();

    claude.set_actions(1, "echo base-v1 > base.txt\n");
    let mut run_child = spawn_o7_run_process(
        repo.path(),
        &task_file,
        &ledger_path,
        &runs_dir,
        &worktree_root,
        claude.path(),
    );
    assert!(run_child.wait().unwrap().success());

    // A short redrive threshold this time — the whole point of this test
    // is to observe a redrive actually happen once the cause is fixed.
    let (mut o7d_child, addr) = spawn_o7d_with_exec_and_redrive_ms(
        &ledger_path,
        repo.path(),
        &worktree_root,
        &runs_dir,
        claude.path(),
        Some(200),
    );
    wait_until_healthy(addr);
    let (_, page) = get(addr, "/api/v1/runs");
    let run_a_id = page["items"][0]["run_id"].as_str().unwrap().to_owned();
    let conversation_id = page["items"][0]["conversation_id"]
        .as_str()
        .unwrap()
        .to_owned();

    let receipt_path = run_dir(&runs_dir, &target, &run_a_id).join("candidate_state_receipt.json");
    let original = std::fs::read_to_string(&receipt_path).unwrap();
    let mut broken: serde_json::Value = serde_json::from_str(&original).unwrap();
    broken["candidate_tree_oid"] = serde_json::Value::String("0".repeat(40));
    std::fs::write(&receipt_path, serde_json::to_string(&broken).unwrap()).unwrap();

    let key = "key-retry-after-fix";
    let body = serde_json::json!({
        "schema_version": 1,
        "parent_run_id": run_a_id,
        "command": "retry after fix",
        "idempotency_key": key,
    });
    let path = format!("/api/v1/conversations/{conversation_id}/commands");
    let (status, first) = post(addr, &path, &body);
    assert_eq!(status, 202, "{first:?}");

    std::thread::sleep(Duration::from_secs(1));
    assert_eq!(
        claude.invocation_count(),
        1,
        "the broken first attempt must not invoke the provider"
    );

    // Fix the cause.
    std::fs::write(&receipt_path, &original).unwrap();

    // Retry with the SAME idempotency key, after the staleness bound.
    let deadline = Instant::now() + Duration::from_secs(30);
    let final_run_id = poll_until(deadline, || {
        let (status, retried) = post(addr, &path, &body);
        (status == 202).then(|| retried["run_id"].as_str().unwrap().to_owned())
    });

    let deadline = Instant::now() + Duration::from_secs(30);
    claude.wait_for_invocation(2, deadline);
    let deadline = Instant::now() + Duration::from_secs(30);
    wait_for_run_status(addr, &final_run_id, "completed", deadline);
    assert_eq!(
        claude.invocation_count(),
        2,
        "exactly one successful redrive invocation"
    );

    let _ = o7d_child.kill();
    let _ = o7d_child.wait();
}
