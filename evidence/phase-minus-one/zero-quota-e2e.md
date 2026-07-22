# Zero-quota end-to-end evidence

The Stage B claim was tightened per independent-gate feedback: a suite of 480 unit/integration tests is **coverage across all pipeline layers**, not by itself one reproducible end-to-end scenario. Below are **named, concrete, zero-quota E2E flows** that each traverse the full pipeline, run in isolation and captured in `raw/zero-quota-e2e.log`. All use scripted adapters (no vendor CLI, no network, no quota).

## E2E-1 — `conduct_test::happy_path_single_subtask` (`core/tests/conduct_test.rs:168`)
Path exercised: **scripted adapter → sessions::spawn → conduct (plan → route → worker → capture_changes → conductor eval) → terminal ConductOutcome**.
- Conductor = a `SequencedAdapter` returning `[plan_json, accept_json]` (`:173-182`); worker = a `ScriptedAdapter` whose `pre_script` writes `out.txt` (`:185-188`) — real child-process execution via `sessions::spawn`.
- `run_conduct(...)` (`:208-218`) drives the whole loop; asserts terminal `outcome.completed == [1]`, `halted/failed == None` (`:220-222`), that the worker actually created `out.txt` on disk (`:223-226`), and that the transcript has 1 subtask / 1 attempt with `decision == "accept"`, `worker == "codex-worker"`, `changes_chars > 0` (`:229-235`).
- Note: `verify: None` here — this flow proves adapter→session→conduct→terminal but not the verify stage. That stage is covered by E2E-3.
- Result: **PASS** (`test result: ok. 1 passed`).

## E2E-2 — `server_test::ws_streams_conduct_events_then_terminal_frame` (`core/tests/server_test.rs:57`)
Path exercised: **WebSocket front door → scripted adapter → sessions::spawn → conduct → normalized AgentEvent frames streamed to a real WS client → terminal ServerFrame**.
- A real `tokio-tungstenite` client connects to a scripted-adapter `ServerState`, sends a `conduct` first frame, drains live event frames, and asserts a terminal frame — i.e. the same pipeline as E2E-1 but through the Axum/WS server and its `ServerFrame::from(&ConductOutcome)` terminal path (`server.rs:415`).
- Result: **PASS** (`test result: ok. 1 passed`).

## E2E-3 — `conduct_test::failing_tests_force_rework_even_if_conductor_would_accept` (`core/tests/conduct_test.rs:2176`)
Path exercised: **scripted adapter → sessions::spawn → conduct → `verify::run_verify` (ran, failed) → grounding veto → terminal outcome**. This is the flow that includes the **verify** stage the gate asked for: the conductor would `Accept`, but because the verifier ran and failed, the grounding rule (`conduct.rs:669-680`) mechanically forces `Rework`, and the run reaches its terminal outcome accordingly.
- Result: **PASS** (`test result: ok. 1 passed`).

## Reproduce
```
nix shell nixpkgs#cargo nixpkgs#rustc nixpkgs#gcc nixpkgs#pkg-config -c bash -c '
  cd /path/to/consilium
  cargo test -p consilium --test conduct_test happy_path_single_subtask -- --exact --nocapture
  cargo test -p consilium --test server_test  ws_streams_conduct_events_then_terminal_frame -- --exact --nocapture
  cargo test -p consilium --test conduct_test failing_tests_force_rework_even_if_conductor_would_accept -- --exact --nocapture
'
```
Captured output: `raw/zero-quota-e2e.log`. Terminal outcome per test: `test result: ok. 1 passed; 0 failed`.

## Verdict
At least one zero-quota end-to-end flow (in fact three, one of which — E2E-3 — traverses the verify stage to a terminal outcome) is named, run, and captured. The broader "480 tests" figure remains true as **zero-quota coverage across all pipeline layers**, but the E2E acceptance gate is met by these specific, reproducible tests rather than by the aggregate.
