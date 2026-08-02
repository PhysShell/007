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
//!
//! Invariant for the restriction-lint allowance below (Q-Deck A0
//! corrective round 4, Codex P1): every `unwrap`/`expect`/index here
//! operates on this test's own controlled fixtures and output — a
//! throwaway repo/ledger/worktree this test itself created, a REST
//! response body this test itself just parsed and is asserting the shape
//! of, a child process this test itself spawned and is waiting on — a
//! panic here is this test's own setup or assertion failing loudly, not a
//! runtime condition in the production code under test. Matches the
//! precedent in `crates/o7d/tests/golden_transcript_sse.rs`.
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
    fixture_repo_at(dir.path(), gate_toml);
    dir
}

/// Same as [`fixture_repo`], but targeting an explicit, caller-chosen directory —
/// Q-Deck A0 corrective round 3 (Codex P1, Part 4): lets a test control a repository's own
/// final path COMPONENT (basename), which is what `child_target`/`o7d`'s own default
/// `--target` derivation uses, so two genuinely DIFFERENT repositories can share the same
/// `target` label on purpose (exercising a real target-name collision).
fn fixture_repo_at(dir: &Path, gate_toml: &str) {
    std::fs::create_dir_all(dir).unwrap();
    let run = |args: &[&str]| {
        let status = Command::new("git")
            .args(args)
            .current_dir(dir)
            .status()
            .unwrap();
        assert!(status.success(), "git {args:?} failed");
    };
    run(&["init", "-q"]);
    run(&["config", "user.email", "test@example.com"]);
    run(&["config", "user.name", "test"]);
    std::fs::write(dir.join("README.md"), "fixture\n").unwrap();
    let gate_dir = dir.join(".007");
    std::fs::create_dir_all(&gate_dir).unwrap();
    std::fs::write(gate_dir.join("gate.toml"), gate_toml).unwrap();
    run(&["add", "-A"]);
    run(&["commit", "-q", "-m", "initial"]);
}

const PASSING_GATE: &str = "[[gate]]\nname = \"unit\"\ncmd = \"true\"\n";
/// Q-Deck A0 corrective round 2 (Part 8): a required gate that always
/// reports a domain failure — produces a genuinely SEALED, replay-valid
/// parent run whose own verdict is `Fail`, not `Pass`, while candidate-state
/// capture (which happens unconditionally, before the gate loop) still
/// succeeds normally.
const FAILING_GATE: &str = "[[gate]]\nname = \"unit\"\ncmd = \"false\"\n";

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

/// Overwrite a run's own `candidate_state_receipt.json` with `new_receipt_bytes`, then
/// re-point its `CandidateStateCaptured` event's own `receipt.digest` at the new bytes and
/// recompute the digest chain FORWARD from there — so the record stays otherwise fully
/// chain/digest-consistent (a genuinely tamper-evident record with exactly ONE deliberately
/// altered fact, never a record that merely fails on an incidental stale digest). Used for
/// negative cases that need the receipt's OWN content to disagree with something specific
/// (e.g. a patch that will fail to apply) without ALSO tripping the unrelated "this file's
/// bytes don't match what the canonical stream declared" check first.
fn overwrite_captured_receipt_and_recompute_chain(dir: &Path, new_receipt_bytes: &[u8]) {
    std::fs::write(dir.join("candidate_state_receipt.json"), new_receipt_bytes).unwrap();
    let text = std::fs::read_to_string(dir.join("events.jsonl")).unwrap();
    let mut events: Vec<o7_run::RunEvent> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let mutated_index = events
        .iter()
        .position(|e| matches!(e.kind, o7_run::RunEventKind::CandidateStateCaptured { .. }))
        .expect("this run must have a CandidateStateCaptured event to overwrite");
    let o7_run::RunEventKind::CandidateStateCaptured { receipt } = &mut events[mutated_index].kind
    else {
        unreachable!()
    };
    receipt.digest = o7_run::event::Digest256::of_bytes(new_receipt_bytes);
    for i in mutated_index..events.len() {
        if i > 0 {
            events[i].previous_event_digest = events[i - 1].event_digest.clone();
        }
        events[i].event_digest = events[i].compute_digest();
    }
    let rewritten: String = events
        .iter()
        .map(|e| serde_json::to_string(e).unwrap())
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(dir.join("events.jsonl"), rewritten + "\n").unwrap();
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

/// Post a command and assert the provider is never invoked and the child
/// run never completes, within a bounded observation window — the
/// required proof for every pre-provider negative case
/// (`docs/q-deck/a0-candidate-state.md` §6).
///
/// Q-Deck A0 corrective round 2 (Part 5): admission-preflight now catches
/// many of these cases BEFORE any command row is ever created — a `409
/// COMMAND_PARENT_CANDIDATE_UNAVAILABLE` (no command row, no child run, no
/// worktree, provider count unchanged) is an equally valid, and now
/// EARLIER, proof as the original round's `202` (accepted, but never
/// completes; command stays `started`) — which cases that predates this
/// round for a defect materialization alone can only catch (patch-apply
/// conflicts, which never touch the parent's own admission-time validity)
/// still produce. Both outcomes prove the same underlying safety property;
/// which one a given negative case hits depends on whether its defect is
/// visible from the parent's OWN record alone (caught at admission) or only
/// by actually attempting materialization against a live worktree (caught
/// later, same as corrective round 1).
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

    if status == 409 {
        assert_eq!(
            accepted["code"], "COMMAND_PARENT_CANDIDATE_UNAVAILABLE",
            "a 409 for a candidate-state negative case must carry the stable admission code: \
             {accepted:?}"
        );
        assert_eq!(
            claude.invocation_count(),
            1,
            "admission-preflight rejection must never invoke the provider"
        );
        let conn = rusqlite::Connection::open(ledger_path).unwrap();
        let rows: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM command WHERE parent_run_id = ?1 AND command_text = ?2",
                [parent_run_id, "this must never reach the provider"],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            rows, 0,
            "an admission-preflight rejection must never create a command row at all"
        );
        return;
    }

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
    // Q-Deck A0 corrective round 1: some negative cases (e.g. the parent's
    // own candidate contract failing its EARLY, pre-RunStarted inheritance
    // check) never create a ledger row for the child run id at all — a 404
    // here is just as valid a "never completed" proof as a 200 with a
    // non-terminal status; both mean `Absent`/safely pre-dispatch-
    // redrivable, never a completed run.
    let (status, run) = get(addr, &format!("/api/v1/runs/{child_run_id}"));
    assert!(
        status == 404 || (status == 200 && run["status"] != "completed"),
        "a failed materialization must never complete the child run (got status {status}, \
         run {run:?})"
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
    // The receipt's own nested `patch: ArtifactRef` digest must match the
    // conflicting patch bytes just written, and the CANONICAL EVENT
    // referencing the receipt must in turn be re-chained to match the
    // receipt's own new bytes — so this record stays otherwise FULLY
    // chain/digest/semantically valid (a legitimate parent, a real
    // receipt, real digests throughout, admission-preflight-usable). The
    // ONLY thing wrong is that the patch does not actually apply cleanly
    // against the base commit — a fact that only surfaces at
    // MATERIALIZATION time (a real `git apply`), never at admission-
    // preflight time (which never touches Git at all).
    let mut receipt: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(dir.join("candidate_state_receipt.json")).unwrap(),
    )
    .unwrap();
    receipt["patch"]["digest"] = serde_json::Value::String(
        o7_run::event::Digest256::of_bytes(conflicting_patch.as_bytes())
            .as_str()
            .to_owned(),
    );
    overwrite_captured_receipt_and_recompute_chain(
        &dir,
        serde_json::to_string(&receipt).unwrap().as_bytes(),
    );

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

    // Q-Deck A0 corrective round 2: uses a genuine PATCH-APPLY conflict
    // (same technique as `a_patch_apply_conflict_never_invokes_the_provider`),
    // not a missing/tampered receipt — the parent's own record stays fully
    // admission-preflight-usable (a real, semantically valid, chain-
    // consistent receipt), so this exercises the SAME concurrent-CAS
    // convergence property the original round proved, now for a defect
    // that is only reachable at MATERIALIZATION time, past admission.
    let dir = run_dir(&runs_dir, &target, &run_a_id);
    let conflicting_patch = "diff --git a/base.txt b/base.txt\n\
         index 0000000000000000000000000000000000000000..2222222222222222222222222222222222222222 100644\n\
         --- a/base.txt\n\
         +++ b/base.txt\n\
         @@ -1,50 +1,50 @@\n\
         -this context line cannot possibly match either\n\
         +neither can this one\n";
    std::fs::write(dir.join("candidate.patch"), conflicting_patch).unwrap();
    let mut receipt: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(dir.join("candidate_state_receipt.json")).unwrap(),
    )
    .unwrap();
    receipt["patch"]["digest"] = serde_json::Value::String(
        o7_run::event::Digest256::of_bytes(conflicting_patch.as_bytes())
            .as_str()
            .to_owned(),
    );
    overwrite_captured_receipt_and_recompute_chain(
        &dir,
        serde_json::to_string(&receipt).unwrap().as_bytes(),
    );

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

    // Q-Deck A0 corrective round 2: the receipt is deliberately kept fully
    // chain/digest/schema-consistent (via `overwrite_captured_receipt_and_
    // recompute_chain`, which re-points the canonical event at the new
    // bytes' own digest) — admission-preflight has no opinion on
    // `candidate_tree_oid` at all, so this stays accepted (`202`) and the
    // mismatch is caught ONLY where it always was: materialization's own
    // explicit tree-OID comparison against an independently resolved
    // source receipt.
    let dir = run_dir(&runs_dir, &target, &run_a_id);
    let receipt_path = dir.join("candidate_state_receipt.json");
    let original = std::fs::read_to_string(&receipt_path).unwrap();
    let mut broken: serde_json::Value = serde_json::from_str(&original).unwrap();
    broken["candidate_tree_oid"] = serde_json::Value::String("0".repeat(40));
    overwrite_captured_receipt_and_recompute_chain(
        &dir,
        serde_json::to_string(&broken).unwrap().as_bytes(),
    );

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

    // Fix the cause — restore the ORIGINAL receipt bytes AND re-chain the
    // canonical event back to reference them (the same recompute as the
    // break above, just in reverse).
    overwrite_captured_receipt_and_recompute_chain(&dir, original.as_bytes());

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

/// Q-Deck A0 corrective round 1 (P6 regression, `docs/q-deck/a0-candidate-state.md`
/// §1): the conversation's own base commit contains a symlink at the EXACT
/// name the old, fixed-name temp file used
/// (`.o7-candidate-patch.tmp`, inside the checkout) pointing OUTSIDE the
/// repository at a sentinel file. A command continuation's own real patch
/// application must NEVER write through that symlink — it must go to the
/// confined private temp store instead. Proves: the outside sentinel's
/// bytes are untouched, the checked-out symlink itself is byte-identical to
/// what the base commit committed (never followed, read, or overwritten),
/// the continuation still succeeds normally, and the provider is invoked
/// exactly once for it (the presence of a hostile symlink in the base must
/// not itself cause a false-closed failure for an otherwise-legitimate
/// continuation).
#[test]
fn a_symlink_at_the_old_temp_files_exact_name_never_escapes_the_confined_temp_store() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let repo = fixture_repo(PASSING_GATE);

    // The outside sentinel: a file the candidate-controlled base commit's
    // own symlink points at, OUTSIDE the repo and outside any o7-owned
    // directory.
    let sentinel_dir = tempfile::tempdir().unwrap();
    let sentinel = sentinel_dir.path().join("sentinel.txt");
    std::fs::write(&sentinel, "untouched-sentinel-content\n").unwrap();

    // Bake the hostile symlink into the repo's OWN base commit — the git
    // commands here mirror `fixture_repo`'s own style.
    let git = |args: &[&str]| {
        let status = Command::new("git")
            .args(args)
            .current_dir(repo.path())
            .status()
            .unwrap();
        assert!(status.success(), "git {args:?} failed");
    };
    std::os::unix::fs::symlink(&sentinel, repo.path().join(".o7-candidate-patch.tmp")).unwrap();
    git(&["add", "-A"]);
    git(&[
        "commit",
        "-q",
        "-m",
        "hostile symlink at the old temp file's exact name",
    ]);
    let hostile_base = worktree_rev_parse(repo.path(), "HEAD");

    let task_file = repo.path().join("task.md");
    std::fs::write(&task_file, "do the initial thing").unwrap();
    let work = tempfile::tempdir().unwrap();
    let ledger_path = work.path().join("ledger.sqlite3");
    let runs_dir = work.path().join("runs");
    let worktree_root = work.path().join("worktrees");
    let claude = ActionClaude::new();

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

    let (mut o7d_child, addr) = spawn_o7d_with_exec_and_redrive_ms(
        &ledger_path,
        repo.path(),
        &worktree_root,
        &runs_dir,
        claude.path(),
        None,
    );
    wait_until_healthy(addr);
    let (_, page) = get(addr, "/api/v1/runs");
    let run_a_id = page["items"][0]["run_id"].as_str().unwrap().to_owned();
    let conversation_id = page["items"][0]["conversation_id"]
        .as_str()
        .unwrap()
        .to_owned();

    // Run A's own conversation base really is the hostile commit.
    let target = repo
        .path()
        .file_name()
        .unwrap()
        .to_string_lossy()
        .to_string();
    let receipt_a = read_candidate_receipt(&run_dir(&runs_dir, &target, &run_a_id));
    assert_eq!(receipt_a["base_commit"], hostile_base);

    // A legitimate continuation, with a REAL (non-empty) patch to apply —
    // the case the old, fixed-name temp file inside the worktree was
    // actually needed for.
    claude.set_actions(2, "echo base-v2 > base.txt\necho from-b > b.txt\n");
    let (status, accepted) = post(
        addr,
        &format!("/api/v1/conversations/{conversation_id}/commands"),
        &serde_json::json!({
            "schema_version": 1,
            "parent_run_id": run_a_id,
            "command": "continue despite the hostile symlink",
            "idempotency_key": "key-symlink-escape",
        }),
    );
    assert_eq!(status, 202, "{accepted:?}");
    let run_b_id = accepted["run_id"].as_str().unwrap().to_owned();

    let deadline = Instant::now() + Duration::from_secs(30);
    claude.wait_for_invocation(2, deadline);
    let deadline = Instant::now() + Duration::from_secs(30);
    wait_for_run_status(addr, &run_b_id, "completed", deadline);
    assert_eq!(
        claude.invocation_count(),
        2,
        "a hostile symlink in the base commit must not prevent a legitimate continuation \
         from succeeding, and must never cause a second, spurious provider invocation"
    );

    // THE proof: the outside sentinel is byte-identical to what it started
    // as — nothing ever wrote through the symlink.
    assert_eq!(
        std::fs::read_to_string(&sentinel).unwrap(),
        "untouched-sentinel-content\n",
        "the outside sentinel must be completely untouched by patch application"
    );

    // The materialized child's own candidate state still faithfully
    // preserves the symlink itself as a real repository entry (never
    // followed, dereferenced, or replaced) — verified via an independent
    // scratch clone, exactly like the main cumulative-state test does.
    let run_b_dir = run_dir(&runs_dir, &target, &run_b_id);
    let verify_dir = tempfile::tempdir().unwrap();
    let git_in = |args: &[&str], cwd: &Path| {
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
    };
    git_in(
        &[
            "clone",
            "-q",
            repo.path().to_str().unwrap(),
            verify_dir.path().to_str().unwrap(),
        ],
        Path::new("/"),
    );
    git_in(&["checkout", "-q", &hostile_base], verify_dir.path());
    let patch_b = std::fs::read_to_string(run_b_dir.join("candidate.patch")).unwrap();
    if !patch_b.trim().is_empty() {
        let patch_file = verify_dir.path().join("verify.patch");
        std::fs::write(&patch_file, &patch_b).unwrap();
        git_in(
            &["apply", "--binary", patch_file.to_str().unwrap()],
            verify_dir.path(),
        );
    }
    let symlink_meta =
        std::fs::symlink_metadata(verify_dir.path().join(".o7-candidate-patch.tmp")).unwrap();
    assert!(
        symlink_meta.file_type().is_symlink(),
        "the committed symlink must still be a symlink after materialization, never followed \
         or replaced"
    );
    assert_eq!(
        std::fs::read_link(verify_dir.path().join(".o7-candidate-patch.tmp")).unwrap(),
        sentinel
    );

    let _ = o7d_child.kill();
    let _ = o7d_child.wait();
}

fn worktree_rev_parse(dir: &Path, refname: &str) -> String {
    let out = Command::new("git")
        .args(["rev-parse", refname])
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(out.status.success());
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

// ==================== Q-Deck A0 corrective round 2: legacy-parent admission (Part 7) ====================
//
// Every test below uses a REAL o7d, REAL o7, a REAL SQLite ledger, and a REAL Git
// repository — the only stand-in is the deterministic `claude` fixture, exactly like
// every other test in this file.

/// Simulates a genuinely pre-Q-Deck-A0 sealed parent run: strips its OWN
/// `candidate_state` contract obligation and `CandidateStateCaptured` evidence entirely
/// from its canonical record, re-sequences and re-chains every remaining event from
/// genesis, and removes the now-orphaned candidate artifacts from disk — producing a
/// record indistinguishable, from replay's point of view, from one an R1-only build
/// (before Q-Deck A0 existed at all) would have produced: fully valid, sealed, a real
/// provider session, simply never captured any A0 evidence.
fn strip_candidate_state_and_make_pre_a0(dir: &Path) {
    let text = std::fs::read_to_string(dir.join("events.jsonl")).unwrap();
    let mut events: Vec<o7_run::RunEvent> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    events.retain(|e| !matches!(e.kind, o7_run::RunEventKind::CandidateStateCaptured { .. }));
    for e in &mut events {
        if let o7_run::RunEventKind::RunStarted { contract, .. } = &mut e.kind {
            contract.candidate_state = None;
        }
    }
    let mut prev = o7_run::event::Digest256::genesis();
    for (i, e) in events.iter_mut().enumerate() {
        e.sequence = i as u64;
        e.previous_event_digest = prev.clone();
        e.event_digest = e.compute_digest();
        prev = e.event_digest.clone();
    }
    let rewritten: String = events
        .iter()
        .map(|e| serde_json::to_string(e).unwrap())
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(dir.join("events.jsonl"), rewritten + "\n").unwrap();
    let _ = std::fs::remove_file(dir.join("candidate_state_receipt.json"));
    let _ = std::fs::remove_file(dir.join("candidate.patch"));
}

fn conversation_run_count(ledger_path: &Path, conversation_id: &str) -> i64 {
    let conn = rusqlite::Connection::open(ledger_path).unwrap();
    conn.query_row(
        "SELECT COUNT(*) FROM run WHERE conversation_id = ?1",
        [conversation_id],
        |r| r.get(0),
    )
    .unwrap()
}

fn command_row_count_for_parent(ledger_path: &Path, parent_run_id: &str) -> i64 {
    let conn = rusqlite::Connection::open(ledger_path).unwrap();
    conn.query_row(
        "SELECT COUNT(*) FROM command WHERE parent_run_id = ?1",
        [parent_run_id],
        |r| r.get(0),
    )
    .unwrap()
}

/// Part 7 Case A: a FRESH command against a genuinely pre-A0 (R1-only) sealed parent
/// must be rejected at admission — `409 COMMAND_PARENT_CANDIDATE_UNAVAILABLE`, no command
/// row ever created, no child run, no worktree, the provider never invoked again, and the
/// conversation's own run count (its "tail") unchanged.
#[test]
fn a_fresh_command_against_a_pre_a0_parent_is_rejected_at_admission() {
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
    assert_eq!(claude.invocation_count(), 1);

    let (mut o7d_child, addr) = spawn_o7d_with_exec_and_redrive_ms(
        &ledger_path,
        repo.path(),
        &worktree_root,
        &runs_dir,
        claude.path(),
        Some(3_600_000),
    );
    wait_until_healthy(addr);
    let (_, page) = get(addr, "/api/v1/runs");
    let run_a_id = page["items"][0]["run_id"].as_str().unwrap().to_owned();
    let conversation_id = page["items"][0]["conversation_id"]
        .as_str()
        .unwrap()
        .to_owned();

    strip_candidate_state_and_make_pre_a0(&run_dir(&runs_dir, &target, &run_a_id));
    let runs_before = conversation_run_count(&ledger_path, &conversation_id);

    let (status, accepted) = post(
        addr,
        &format!("/api/v1/conversations/{conversation_id}/commands"),
        &serde_json::json!({
            "schema_version": 1,
            "parent_run_id": run_a_id,
            "command": "should never be accepted against a legacy parent",
            "idempotency_key": "key-legacy-parent-fresh",
        }),
    );
    assert_eq!(status, 409, "{accepted:?}");
    assert_eq!(
        accepted["code"], "COMMAND_PARENT_CANDIDATE_UNAVAILABLE",
        "{accepted:?}"
    );

    assert_eq!(
        command_row_count_for_parent(&ledger_path, &run_a_id),
        0,
        "a pre-A0 parent must never get a command row created against it at all"
    );
    assert_eq!(
        conversation_run_count(&ledger_path, &conversation_id),
        runs_before,
        "no child run/worktree was ever created — the conversation's own tail is unchanged"
    );
    std::thread::sleep(Duration::from_millis(500));
    assert_eq!(
        claude.invocation_count(),
        1,
        "the provider must never be invoked for a rejected legacy-parent command"
    );

    let _ = o7d_child.kill();
    let _ = o7d_child.wait();
    drop(repo);
}

/// Part 7 Case D: a command row created by an OLDER version — `accepted`, bound to a
/// fresh `child_run_id` that has NEVER attached a ledger row, against a pre-A0 parent —
/// simulated with a direct SQL insert (bypassing `o7d`'s own accept endpoint and its new
/// admission preflight entirely, exactly as a real pre-round-2 deployment's already-
/// accepted row would look). Running `o7 continue` directly against it (mirroring a
/// direct CLI invocation, or a stale row a redrive resurrects) must: invoke the provider
/// zero times, transition the command to `rejected` via the new conditional transition,
/// and leave the parent conversation able to accept a fresh, valid command afterward —
/// never permanently wedged behind an eternally-`accepted` row.
#[test]
fn an_already_accepted_legacy_command_against_a_pre_a0_parent_self_rejects_via_o7_continue() {
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
    assert_eq!(claude.invocation_count(), 1);

    let (mut o7d_child, addr) = spawn_o7d_with_exec_and_redrive_ms(
        &ledger_path,
        repo.path(),
        &worktree_root,
        &runs_dir,
        claude.path(),
        Some(3_600_000),
    );
    wait_until_healthy(addr);
    let (_, page) = get(addr, "/api/v1/runs");
    let run_a_id = page["items"][0]["run_id"].as_str().unwrap().to_owned();
    let conversation_id = page["items"][0]["conversation_id"]
        .as_str()
        .unwrap()
        .to_owned();
    strip_candidate_state_and_make_pre_a0(&run_dir(&runs_dir, &target, &run_a_id));
    let _ = o7d_child.kill();
    let _ = o7d_child.wait();

    // A legacy accepted-and-bound row, created directly in SQL — exactly what an
    // OLDER `o7d` (predating this round's own admission preflight) would have left
    // behind: `accepted`, bound to a fresh run id, that run id never attached.
    let command_id = format!("cmd-legacy-{}", run_a_id);
    let child_run_id = format!("legacy-child-{}", run_a_id);
    {
        let conn = rusqlite::Connection::open(&ledger_path).unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        conn.execute(
            "INSERT INTO command (command_id, conversation_id, parent_run_id, command_text, \
             status, child_run_id, created_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, 'accepted', ?5, ?6, ?6)",
            rusqlite::params![
                command_id,
                conversation_id,
                run_a_id,
                "a legacy pre-round-2 command",
                child_run_id,
                now,
            ],
        )
        .unwrap();
    }

    // Direct CLI invocation of `o7 continue` — mirroring `o7d`'s own `spawn_continue`
    // argv shape exactly, but invoked here directly (as a real stale/redriven process,
    // or a direct CLI call, would) rather than through the HTTP accept path this
    // round's new preflight gates.
    let gate_path = repo.path().join(".007").join("gate.toml");
    let status = Command::new(env!("CARGO_BIN_EXE_o7"))
        .arg("continue")
        .arg("--repo")
        .arg(repo.path())
        .arg("--worktree-root")
        .arg(&worktree_root)
        .arg("--runs-dir")
        .arg(&runs_dir)
        .arg("--gate")
        .arg(&gate_path)
        .arg("--model")
        .arg("opus")
        .arg("--max-turns")
        .arg("1")
        .arg("--ledger")
        .arg(&ledger_path)
        .arg("--conversation-id")
        .arg(&conversation_id)
        .arg("--parent-run-id")
        .arg(&run_a_id)
        .arg("--command")
        .arg("a legacy pre-round-2 command")
        .arg("--run-id")
        .arg(&child_run_id)
        .arg("--command-id")
        .arg(&command_id)
        .env(
            "PATH",
            format!(
                "{}:{}",
                claude.path().display(),
                std::env::var("PATH").unwrap_or_default()
            ),
        )
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("spawn the real o7 continue process directly");
    assert!(
        !status.success(),
        "o7 continue must exit non-zero for an unusable legacy parent"
    );

    assert_eq!(
        claude.invocation_count(),
        1,
        "the provider must never be invoked for a legacy pre-A0 parent, even via direct CLI \
         invocation past o7d's own admission preflight"
    );
    let conn = rusqlite::Connection::open(&ledger_path).unwrap();
    let final_status: String = conn
        .query_row(
            "SELECT status FROM command WHERE command_id = ?1",
            [&command_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        final_status, "rejected",
        "the conditional reject-if-unattached-and-bound transition must have fired — this is \
         the exact defense-in-depth gap corrective round 2 closes"
    );
    let attached: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM run WHERE run_id = ?1",
            [&child_run_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        attached, 0,
        "the legacy child run id must never have attached a ledger row"
    );

    // The conversation is NOT wedged: a fresh, valid command against the SAME
    // (still pre-A0) parent still correctly gets the admission-preflight 409, proving
    // the conversation's own in-flight-command slot was freed by the rejection above,
    // not left stuck behind the now-rejected legacy row.
    let (mut o7d_child2, addr2) = spawn_o7d_with_exec_and_redrive_ms(
        &ledger_path,
        repo.path(),
        &worktree_root,
        &runs_dir,
        claude.path(),
        Some(3_600_000),
    );
    wait_until_healthy(addr2);
    let (status2, accepted2) = post(
        addr2,
        &format!("/api/v1/conversations/{conversation_id}/commands"),
        &serde_json::json!({
            "schema_version": 1,
            "parent_run_id": run_a_id,
            "command": "a fresh follow-up after the legacy row was rejected",
            "idempotency_key": "key-after-legacy-reject",
        }),
    );
    assert_eq!(
        status2, 409,
        "the conversation must not be wedged behind the rejected legacy command: {accepted2:?}"
    );
    assert_eq!(accepted2["code"], "COMMAND_PARENT_CANDIDATE_UNAVAILABLE");

    let _ = o7d_child2.kill();
    let _ = o7d_child2.wait();
    drop(repo);
}

/// Part 7 Case E: TWO processes simultaneously run `o7 continue` directly against the
/// SAME legacy accepted-and-bound row (same construction as the previous test). The
/// existing per-run-id `flock` (the very first thing `continue_run` acquires, unchanged
/// by this round) already guarantees only one ever proceeds; this proves that guarantee
/// composes correctly with THIS round's new early-failure path: at most one terminal
/// rejection ever lands, the loser touches no durable state at all, no child run ever
/// attaches, and the provider is invoked zero times by either.
#[test]
fn two_processes_racing_the_same_legacy_accepted_command_produce_at_most_one_rejection() {
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

    let (mut o7d_child, addr) = spawn_o7d_with_exec_and_redrive_ms(
        &ledger_path,
        repo.path(),
        &worktree_root,
        &runs_dir,
        claude.path(),
        Some(3_600_000),
    );
    wait_until_healthy(addr);
    let (_, page) = get(addr, "/api/v1/runs");
    let run_a_id = page["items"][0]["run_id"].as_str().unwrap().to_owned();
    let conversation_id = page["items"][0]["conversation_id"]
        .as_str()
        .unwrap()
        .to_owned();
    strip_candidate_state_and_make_pre_a0(&run_dir(&runs_dir, &target, &run_a_id));
    let _ = o7d_child.kill();
    let _ = o7d_child.wait();

    let command_id = format!("cmd-legacy-race-{}", run_a_id);
    let child_run_id = format!("legacy-child-race-{}", run_a_id);
    {
        let conn = rusqlite::Connection::open(&ledger_path).unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        conn.execute(
            "INSERT INTO command (command_id, conversation_id, parent_run_id, command_text, \
             status, child_run_id, created_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, 'accepted', ?5, ?6, ?6)",
            rusqlite::params![
                command_id,
                conversation_id,
                run_a_id,
                "a racing legacy command",
                child_run_id,
                now,
            ],
        )
        .unwrap();
    }

    let gate_path = repo.path().join(".007").join("gate.toml");
    let spawn_continue_directly = || {
        Command::new(env!("CARGO_BIN_EXE_o7"))
            .arg("continue")
            .arg("--repo")
            .arg(repo.path())
            .arg("--worktree-root")
            .arg(&worktree_root)
            .arg("--runs-dir")
            .arg(&runs_dir)
            .arg("--gate")
            .arg(&gate_path)
            .arg("--model")
            .arg("opus")
            .arg("--max-turns")
            .arg("1")
            .arg("--ledger")
            .arg(&ledger_path)
            .arg("--conversation-id")
            .arg(&conversation_id)
            .arg("--parent-run-id")
            .arg(&run_a_id)
            .arg("--command")
            .arg("a racing legacy command")
            .arg("--run-id")
            .arg(&child_run_id)
            .arg("--command-id")
            .arg(&command_id)
            .env(
                "PATH",
                format!(
                    "{}:{}",
                    claude.path().display(),
                    std::env::var("PATH").unwrap_or_default()
                ),
            )
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .expect("spawn a real o7 continue process directly")
    };
    let (s1, s2) = std::thread::scope(|scope| {
        let h1 = scope.spawn(spawn_continue_directly);
        let h2 = scope.spawn(spawn_continue_directly);
        (h1.join().unwrap(), h2.join().unwrap())
    });
    // Exactly one of the two either wins the flock and rejects the command (exit
    // non-zero), or loses the flock and exits cleanly (exit zero, "already owned by a
    // live process") without touching anything — never a crash on either side.
    assert!(
        s1.success() != s2.success(),
        "exactly one of the two racing processes must be the lock winner: s1={s1:?} s2={s2:?}"
    );

    assert_eq!(
        claude.invocation_count(),
        1,
        "neither racing process may ever invoke the provider"
    );
    let conn = rusqlite::Connection::open(&ledger_path).unwrap();
    let final_status: String = conn
        .query_row(
            "SELECT status FROM command WHERE command_id = ?1",
            [&command_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        final_status, "rejected",
        "the lock winner must have rejected the command exactly once"
    );
    let attached: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM run WHERE run_id = ?1",
            [&child_run_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        attached, 0,
        "neither racing process may ever attach a child run"
    );

    drop(repo);
}

/// Part 8: canonical replay validity is NOT the same question as
/// continuation eligibility. A sealed `Fail` (or, symmetrically, `Blocked`)
/// parent is an honest record of an unsuccessful run — its own verdict is
/// untouched by candidate-state admission, and a follow-up command against
/// it must be accepted normally as long as its candidate evidence is
/// itself valid. This is the DIRECT proof that `parent_candidate_state_usable`
/// checks `state.verdict.is_none()` (not sealed) — never the verdict's
/// VALUE — as its sealed/unsealed predicate.
#[test]
fn a_sealed_non_pass_verdict_parent_still_accepts_a_valid_command() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let claude = ActionClaude::new();
    let repo = fixture_repo(FAILING_GATE);
    let task_file = repo.path().join("task.md");
    std::fs::write(&task_file, "do the initial thing").unwrap();
    let work = tempfile::tempdir().unwrap();
    let ledger_path = work.path().join("ledger.sqlite3");
    let runs_dir = work.path().join("runs");
    let worktree_root = work.path().join("worktrees");

    claude.set_actions(1, "echo base-v1 > base.txt\n");
    let mut run_child = spawn_o7_run_process(
        repo.path(),
        &task_file,
        &ledger_path,
        &runs_dir,
        &worktree_root,
        claude.path(),
    );
    // Deliberately NOT asserting success: the required gate always fails, so this run
    // seals with verdict `Fail`, not `Pass` — exit code non-zero by this project's own
    // "non-Pass verdict exits non-zero" convention.
    let _ = run_child.wait().unwrap();
    assert_eq!(claude.invocation_count(), 1);

    let (mut o7d_child, addr) = spawn_o7d_with_exec_and_redrive_ms(
        &ledger_path,
        repo.path(),
        &worktree_root,
        &runs_dir,
        claude.path(),
        Some(3_600_000),
    );
    wait_until_healthy(addr);
    let (_, page) = get(addr, "/api/v1/runs");
    let run_a_id = page["items"][0]["run_id"].as_str().unwrap().to_owned();
    let conversation_id = page["items"][0]["conversation_id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(
        page["items"][0]["status"], "failed",
        "the parent's own sealed status must genuinely be non-Pass for this test to prove \
         anything: {:?}",
        page["items"][0]
    );

    let (status, accepted) = post(
        addr,
        &format!("/api/v1/conversations/{conversation_id}/commands"),
        &serde_json::json!({
            "schema_version": 1,
            "parent_run_id": run_a_id,
            "command": "continue from a failed-but-candidate-valid parent",
            "idempotency_key": "key-non-pass-parent",
        }),
    );
    assert_eq!(
        status, 202,
        "a sealed non-Pass verdict must not, by itself, make a parent unusable: {accepted:?}"
    );

    let _ = o7d_child.kill();
    let _ = o7d_child.wait();
    drop(repo);
}

// ==================== Q-Deck A0 corrective round 3 (Codex P1, Part 1) ====================
//
// Path confinement and admission ordering, all real o7d/o7/SQLite processes.

fn command_row_count(ledger_path: &Path) -> i64 {
    let conn = rusqlite::Connection::open(ledger_path).unwrap();
    conn.query_row("SELECT COUNT(*) FROM command", [], |r| r.get(0))
        .unwrap()
}

/// Case: a structurally invalid `parent_run_id` (path traversal, absolute path, a nested
/// path, `.`, `..`) must be rejected with a stable `400` — BEFORE the ledger is even
/// consulted, and before any candidate-record filesystem I/O is attempted. Every one of
/// these previously reached `child_record_dir`'s own path join unconfined.
#[test]
fn a_malformed_parent_run_id_is_rejected_with_400_before_any_filesystem_access() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let claude = ActionClaude::new();
    let (
        repo,
        _work,
        ledger_path,
        _runs_dir,
        _worktree_root,
        mut o7d_child,
        addr,
        _run_a_id,
        conversation_id,
        _target,
    ) = setup_real_parent(&claude);

    let malformed_ids = ["../../outside", "/etc/passwd", "a/b", ".", ".."];
    for (i, bad_id) in malformed_ids.iter().enumerate() {
        let (status, body) = post(
            addr,
            &format!("/api/v1/conversations/{conversation_id}/commands"),
            &serde_json::json!({
                "schema_version": 1,
                "parent_run_id": bad_id,
                "command": "should never reach the ledger",
                "idempotency_key": format!("key-malformed-{i}"),
            }),
        );
        assert_eq!(status, 400, "parent_run_id {bad_id:?}: {body:?}");
    }

    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(
        claude.invocation_count(),
        1,
        "the provider must never be invoked for any malformed parent_run_id"
    );
    assert_eq!(
        command_row_count(&ledger_path),
        0,
        "no command row must ever be created for a malformed parent_run_id"
    );

    let _ = o7d_child.kill();
    let _ = o7d_child.wait();
    drop(repo);
}

/// Case: even when a traversal-shaped `parent_run_id` WOULD resolve to a real, existing
/// file outside the run store (a regular file, a FIFO that blocks forever on open, or a
/// large file that could exhaust memory if read), the path-confinement check rejects it
/// BEFORE any filesystem access is attempted at all — proven by a fast (non-hanging)
/// response and the sentinel's own bytes/existence staying completely untouched.
#[test]
fn a_malformed_parent_run_id_never_reads_an_outside_sentinel() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let claude = ActionClaude::new();
    let (
        repo,
        work,
        ledger_path,
        _runs_dir,
        _worktree_root,
        mut o7d_child,
        addr,
        _run_a_id,
        conversation_id,
        _target,
    ) = setup_real_parent(&claude);

    // `runs_dir.join(target).join("../../<name>")` resolves to `work.path().join(<name>)`
    // — one level above `runs_dir` itself, exactly what "../../" from
    // `runs_dir/<target>/` traverses to.
    let outside_dir = work.path();
    let regular_sentinel = outside_dir.join("outside-regular.txt");
    std::fs::write(&regular_sentinel, "sentinel content — must never change").unwrap();
    let before_regular = std::fs::read(&regular_sentinel).unwrap();

    let fifo_sentinel = outside_dir.join("outside.fifo");
    assert!(Command::new("mkfifo")
        .arg(&fifo_sentinel)
        .status()
        .unwrap()
        .success());

    let large_sentinel = outside_dir.join("outside-large.bin");
    {
        let f = std::fs::File::create(&large_sentinel).unwrap();
        f.set_len(64 * 1024 * 1024).unwrap(); // 64 MiB sparse file — cheap to create, expensive to read whole.
    }

    for (label, sentinel_name) in [
        ("regular", "outside-regular.txt"),
        ("fifo", "outside.fifo"),
        ("large", "outside-large.bin"),
    ] {
        let traversal = format!("../../{sentinel_name}");
        let start = Instant::now();
        let (status, body) = post(
            addr,
            &format!("/api/v1/conversations/{conversation_id}/commands"),
            &serde_json::json!({
                "schema_version": 1,
                "parent_run_id": traversal,
                "command": "must never read outside the run store",
                "idempotency_key": format!("key-outside-{label}"),
            }),
        );
        let elapsed = start.elapsed();
        assert_eq!(status, 400, "{label}: {body:?}");
        assert!(
            elapsed < Duration::from_secs(5),
            "{label}: request must return quickly ({elapsed:?}) — a hang would mean the \
             FIFO/large file was actually opened/read"
        );
    }

    assert_eq!(
        std::fs::read(&regular_sentinel).unwrap(),
        before_regular,
        "the outside regular file's bytes must be completely unchanged"
    );
    assert!(
        fifo_sentinel.exists(),
        "the FIFO itself must still exist, untouched"
    );
    assert_eq!(
        std::fs::metadata(&large_sentinel).unwrap().len(),
        64 * 1024 * 1024,
        "the large sentinel file must be completely unchanged"
    );

    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(claude.invocation_count(), 1);
    assert_eq!(command_row_count(&ledger_path), 0);

    let _ = o7d_child.kill();
    let _ = o7d_child.wait();
    drop(repo);
}

/// Case: a WELL-FORMED (single path component) but genuinely nonexistent `parent_run_id`
/// must preserve the ESTABLISHED R1 wire contract — `404 NOT_FOUND` — never the newer
/// `409` this round's own admission preflight introduces. This is exactly the case that
/// used to reach the filesystem preflight FIRST and get the wrong status/code.
#[test]
fn a_well_formed_but_nonexistent_parent_gets_the_established_404() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let claude = ActionClaude::new();
    let (
        repo,
        _work,
        ledger_path,
        _runs_dir,
        _worktree_root,
        mut o7d_child,
        addr,
        _run_a_id,
        conversation_id,
        _target,
    ) = setup_real_parent(&claude);

    let (status, body) = post(
        addr,
        &format!("/api/v1/conversations/{conversation_id}/commands"),
        &serde_json::json!({
            "schema_version": 1,
            "parent_run_id": "genuinely-does-not-exist-anywhere",
            "command": "must 404, not 409",
            "idempotency_key": "key-404-nonexistent-parent",
        }),
    );
    assert_eq!(status, 404, "{body:?}");

    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(claude.invocation_count(), 1);
    assert_eq!(command_row_count(&ledger_path), 0);

    let _ = o7d_child.kill();
    let _ = o7d_child.wait();
    drop(repo);
}

/// Case: a well-formed, EXISTING run id that belongs to a DIFFERENT conversation than the
/// one in the URL must also 404 — never leak cross-conversation existence as a 409.
#[test]
fn a_parent_from_a_different_conversation_gets_404_not_409() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let claude = ActionClaude::new();
    let repo = fixture_repo(PASSING_GATE);
    let task_file = repo.path().join("task.md");
    std::fs::write(&task_file, "do the initial thing").unwrap();
    let work = tempfile::tempdir().unwrap();
    let ledger_path = work.path().join("ledger.sqlite3");
    let runs_dir = work.path().join("runs");
    let worktree_root = work.path().join("worktrees");

    // Two independent top-level runs against the SAME ledger/runs_dir/repo — each
    // `o7 run --ledger` mints its OWN fresh conversation, since there is no
    // `--conversation-id` flag for a top-level run.
    claude.set_actions(2, "echo v1 > f.txt\n");
    for _ in 0..2 {
        let mut run_child = spawn_o7_run_process(
            repo.path(),
            &task_file,
            &ledger_path,
            &runs_dir,
            &worktree_root,
            claude.path(),
        );
        assert!(run_child.wait().unwrap().success());
    }
    assert_eq!(claude.invocation_count(), 2);

    let (mut o7d_child, addr) = spawn_o7d_with_exec_and_redrive_ms(
        &ledger_path,
        repo.path(),
        &worktree_root,
        &runs_dir,
        claude.path(),
        Some(3_600_000),
    );
    wait_until_healthy(addr);
    let (_, page) = get(addr, "/api/v1/runs");
    let items = page["items"].as_array().unwrap();
    assert_eq!(
        items.len(),
        2,
        "two independent conversations, one run each"
    );
    let run_a = items[0]["run_id"].as_str().unwrap().to_owned();
    let conv_a = items[0]["conversation_id"].as_str().unwrap().to_owned();
    let conv_b = items[1]["conversation_id"].as_str().unwrap().to_owned();
    assert_ne!(
        conv_a, conv_b,
        "these really must be two different conversations"
    );

    // Post against conversation B's URL, but naming conversation A's OWN run as parent.
    let (status, body) = post(
        addr,
        &format!("/api/v1/conversations/{conv_b}/commands"),
        &serde_json::json!({
            "schema_version": 1,
            "parent_run_id": run_a,
            "command": "must 404, not leak cross-conversation existence as 409",
            "idempotency_key": "key-cross-conversation-parent",
        }),
    );
    assert_eq!(status, 404, "{body:?}");

    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(
        claude.invocation_count(),
        2,
        "no additional provider invocation"
    );
    assert_eq!(command_row_count(&ledger_path), 0);

    let _ = o7d_child.kill();
    let _ = o7d_child.wait();
    drop(repo);
}

// ==================== Q-Deck A0 corrective round 3 (Codex P1, Part 4) ====================
//
// Repository-bound admission — two GENUINELY DIFFERENT repositories sharing the SAME
// final directory basename (`target` label), so a real target-name collision is actually
// exercised, not merely asserted about in prose.

/// Case: a FRESH command against a parent whose own candidate evidence was captured
/// against a DIFFERENT repository than the one `o7d` is currently configured for (same
/// `target` basename, genuinely different canonical identity) must be rejected at
/// admission — `409 COMMAND_PARENT_CANDIDATE_UNAVAILABLE`, zero command rows, zero child
/// run ids, zero worktrees, zero additional provider invocations.
#[test]
fn a_fresh_command_against_foreign_repository_evidence_is_rejected_at_admission() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let claude = ActionClaude::new();
    let repos_root = tempfile::tempdir().unwrap();
    let repo_a = repos_root.path().join("repo-a").join("myrepo");
    let repo_b = repos_root.path().join("repo-b").join("myrepo");
    fixture_repo_at(&repo_a, PASSING_GATE);
    fixture_repo_at(&repo_b, PASSING_GATE);
    // Both share the SAME final basename ("myrepo"), so `child_target`'s own
    // "canonicalized repo's final path component" derivation collides on purpose.
    assert_eq!(repo_a.file_name(), repo_b.file_name());

    let task_file = repo_a.join("task.md");
    std::fs::write(&task_file, "do the initial thing").unwrap();
    let work = tempfile::tempdir().unwrap();
    let ledger_path = work.path().join("ledger.sqlite3");
    let runs_dir = work.path().join("runs");
    let worktree_root = work.path().join("worktrees");

    // The parent run is captured against REPO A.
    claude.set_actions(1, "echo base-v1 > base.txt\n");
    let mut run_child = spawn_o7_run_process(
        &repo_a,
        &task_file,
        &ledger_path,
        &runs_dir,
        &worktree_root,
        claude.path(),
    );
    assert!(run_child.wait().unwrap().success());
    assert_eq!(claude.invocation_count(), 1);

    // `o7d` itself is configured against REPO B — same runs_dir/ledger, same `target`
    // basename, a GENUINELY different repository underneath.
    let (mut o7d_child, addr) = spawn_o7d_with_exec_and_redrive_ms(
        &ledger_path,
        &repo_b,
        &worktree_root,
        &runs_dir,
        claude.path(),
        Some(3_600_000),
    );
    wait_until_healthy(addr);
    let (_, page) = get(addr, "/api/v1/runs");
    let run_a_id = page["items"][0]["run_id"].as_str().unwrap().to_owned();
    let conversation_id = page["items"][0]["conversation_id"]
        .as_str()
        .unwrap()
        .to_owned();

    let (status, accepted) = post(
        addr,
        &format!("/api/v1/conversations/{conversation_id}/commands"),
        &serde_json::json!({
            "schema_version": 1,
            "parent_run_id": run_a_id,
            "command": "must never be accepted against foreign-repository evidence",
            "idempotency_key": "key-foreign-repo-fresh",
        }),
    );
    assert_eq!(status, 409, "{accepted:?}");
    assert_eq!(
        accepted["code"], "COMMAND_PARENT_CANDIDATE_UNAVAILABLE",
        "{accepted:?}"
    );
    assert_eq!(
        command_row_count(&ledger_path),
        0,
        "foreign-repository evidence must never create a command row"
    );

    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(
        claude.invocation_count(),
        1,
        "the provider must never be invoked for foreign-repository evidence"
    );

    let _ = o7d_child.kill();
    let _ = o7d_child.wait();
}

/// Case (defense in depth): a legacy `accepted`-and-bound command row (direct SQL insert,
/// bypassing `o7d`'s own admission preflight entirely — exactly what an older deployment,
/// or a direct CLI invocation, could produce) naming a parent whose candidate evidence was
/// captured against a DIFFERENT repository than `o7 continue` is invoked with. Must:
/// invoke the provider zero times; never attach a child run; atomically transition to
/// `rejected`; and NOT wedge — a subsequent fresh command against the same (still-foreign)
/// parent must fail the SAME way, not loop or hang.
#[test]
fn an_old_accepted_row_against_foreign_repository_self_rejects_via_o7_continue() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let claude = ActionClaude::new();
    let repos_root = tempfile::tempdir().unwrap();
    let repo_a = repos_root.path().join("repo-a").join("myrepo");
    let repo_b = repos_root.path().join("repo-b").join("myrepo");
    fixture_repo_at(&repo_a, PASSING_GATE);
    fixture_repo_at(&repo_b, PASSING_GATE);

    let task_file = repo_a.join("task.md");
    std::fs::write(&task_file, "do the initial thing").unwrap();
    let work = tempfile::tempdir().unwrap();
    let ledger_path = work.path().join("ledger.sqlite3");
    let runs_dir = work.path().join("runs");
    let worktree_root = work.path().join("worktrees");

    claude.set_actions(1, "echo base-v1 > base.txt\n");
    let mut run_child = spawn_o7_run_process(
        &repo_a,
        &task_file,
        &ledger_path,
        &runs_dir,
        &worktree_root,
        claude.path(),
    );
    assert!(run_child.wait().unwrap().success());
    assert_eq!(claude.invocation_count(), 1);

    // Discover the parent's own ids via a throwaway o7d against REPO A itself (just to
    // read `/api/v1/runs`; never used to accept the command under test).
    let (mut discover_child, discover_addr) = spawn_o7d_with_exec_and_redrive_ms(
        &ledger_path,
        &repo_a,
        &worktree_root,
        &runs_dir,
        claude.path(),
        Some(3_600_000),
    );
    wait_until_healthy(discover_addr);
    let (_, page) = get(discover_addr, "/api/v1/runs");
    let run_a_id = page["items"][0]["run_id"].as_str().unwrap().to_owned();
    let conversation_id = page["items"][0]["conversation_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let _ = discover_child.kill();
    let _ = discover_child.wait();

    // A legacy accepted-and-bound row, direct SQL insert.
    let command_id = format!("cmd-legacy-foreign-{run_a_id}");
    let child_run_id = format!("legacy-child-foreign-{run_a_id}");
    {
        let conn = rusqlite::Connection::open(&ledger_path).unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        conn.execute(
            "INSERT INTO command (command_id, conversation_id, parent_run_id, command_text, \
             status, child_run_id, created_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, 'accepted', ?5, ?6, ?6)",
            rusqlite::params![
                command_id,
                conversation_id,
                run_a_id,
                "a legacy command against foreign-repository evidence",
                child_run_id,
                now,
            ],
        )
        .unwrap();
    }

    // Direct `o7 continue`, configured against REPO B — the SAME repository mismatch
    // `parent_candidate_state_usable` would have caught, now hit via the CLI path
    // directly, past any `o7d` preflight entirely.
    let gate_path = repo_b.join(".007").join("gate.toml");
    let run_continue = || {
        Command::new(env!("CARGO_BIN_EXE_o7"))
            .arg("continue")
            .arg("--repo")
            .arg(&repo_b)
            .arg("--worktree-root")
            .arg(&worktree_root)
            .arg("--runs-dir")
            .arg(&runs_dir)
            .arg("--gate")
            .arg(&gate_path)
            .arg("--model")
            .arg("opus")
            .arg("--max-turns")
            .arg("1")
            .arg("--ledger")
            .arg(&ledger_path)
            .arg("--conversation-id")
            .arg(&conversation_id)
            .arg("--parent-run-id")
            .arg(&run_a_id)
            .arg("--command")
            .arg("a legacy command against foreign-repository evidence")
            .arg("--run-id")
            .arg(&child_run_id)
            .arg("--command-id")
            .arg(&command_id)
            .env(
                "PATH",
                format!(
                    "{}:{}",
                    claude.path().display(),
                    std::env::var("PATH").unwrap_or_default()
                ),
            )
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .expect("spawn o7 continue directly")
    };

    let status = run_continue();
    assert!(
        !status.success(),
        "o7 continue must exit non-zero for foreign-repository evidence"
    );

    assert_eq!(
        claude.invocation_count(),
        1,
        "the provider must never be invoked for foreign-repository evidence, even via direct \
         CLI invocation"
    );
    {
        let conn = rusqlite::Connection::open(&ledger_path).unwrap();
        let final_status: String = conn
            .query_row(
                "SELECT status FROM command WHERE command_id = ?1",
                [&command_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            final_status, "rejected",
            "the conditional reject-if-unattached-and-bound transition must have fired"
        );
        let attached: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM run WHERE run_id = ?1",
                [&child_run_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            attached, 0,
            "the legacy child run id must never have attached"
        );
    }

    // Not wedged: retrying `o7 continue` again for the SAME already-rejected command must
    // not loop, hang, crash, or invoke the provider — it must observe the command is still
    // `rejected` (never reverted, never double-processed) and exit harmlessly.
    let _second_status = run_continue();
    assert_eq!(
        claude.invocation_count(),
        1,
        "a repeat invocation against an already-rejected legacy row must still never invoke \
         the provider"
    );
    let conn = rusqlite::Connection::open(&ledger_path).unwrap();
    let still_rejected: String = conn
        .query_row(
            "SELECT status FROM command WHERE command_id = ?1",
            [&command_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        still_rejected, "rejected",
        "a repeat invocation must not revert or corrupt the already-rejected status — no \
         perpetual redrive loop"
    );
}
