# Consilium `conduct` vs 007 RunGraph — divergence analysis

**Scope:** `core/src/orchestrator/conduct.rs` (`run_conduct`) and collaborators (`topology.rs`, `routing.rs`, `verify.rs`, `changes.rs`, `stagnation.rs`, `resilience.rs`, `review.rs`, `prompts.rs`), plus wiring in `auto.rs`, `main.rs`, `server.rs`, and `safety/`.

**007 model (reference):** RunGraph = typed agent delegation; **only o7d owns the verdict**; an agent saying "DONE" ≠ acceptance; a result is accepted only after **independent gates + artifact checks**; Claude and Codex must execute in **separate runs and separate worktrees**. Sandboy is the process boundary; a worktree is **not** a sandbox.

**Headline:** Consilium runs the whole conduct pipeline **sequentially, in one shared, in-place cwd, with no per-worker worktree and no filesystem sandbox** — the `safety::create_detached_worktree` capability exists and is unit-tested (`core/tests/worktree_test.rs`) but is **never called from any run path** (`run_conduct` callers `auto.rs`, `server.rs:378`, `main.rs:620`, `eval.rs:399` all pass an ordinary cwd; `main.rs:585` uses `std::env::current_dir()`). This is the root of most divergences below.

## 1. Shared cwd across workers
`run_conduct` takes a single `cwd: PathBuf` (`conduct.rs:231`). Every worker, supervisor, reviewer, arbiter, verify call clones that same path (`conduct.rs:448/455`). Module doc is explicit: "Subtasks run SEQUENTIALLY in the shared `cwd` … Worktree-per-subtask isolation is deferred until real parallel workers land" (`conduct.rs:327-332`). Per-CLI "sandboxing" is only the provider's own permission flag over that shared directory — codex `--sandbox workspace-write` (`adapters/codex.rs:26`), claude `--permission-mode acceptEdits` (`adapters/claude.rs:30`), gemini `--dangerously-skip-permissions` (`adapters/gemini.rs:53`). **Divergence: fundamentally-divergent.**

## 2. Sequential vs parallel worker execution
Strictly sequential. `topology::plan_waves` computes dependency waves (`topology.rs:19-70`) and the doc claims waves run concurrently, but the actual loop is nested `for` with a full `await` per subtask: `'plan: for wave … { 'next_subtask: for &subtask_idx in wave { … }}` (`conduct.rs:354-355`). No `join_all` for subtasks (contrast council, which parallelizes answers at `council.rs:82`). Attempts within a subtask are also sequential (`for attempt_num in 0..=MAX_REWORKS`, `conduct.rs:437`). **Divergence: needs-adaptation** (sequential is race-free/safe, but not 007's parallel separate-worktree model; wave structure present but unused for concurrency).

## 3. Assumption that subtasks touch disjoint files
Explicit, unenforced assumption. Decompose prompt: "Design subtasks so they touch DISJOINT files" (`prompts.rs:113`); worker blackboard: "Your work must NOT overlap their files" (`prompts.rs:169-176`); loop comment justifies safety only under that assumption (`conduct.rs:328-331`). **No mechanical overlap detection anywhere.** **Divergence: fundamentally-divergent** (007's separate worktrees make disjointness unnecessary; Consilium's correctness rests on a prompt-level promise the conductor may break).

## 4. Behavior on a DIRTY repository
No worktree, so conduct runs **in-place over the dirty tree**. `capture_changes` uses `git diff HEAD` + untracked listing (`changes.rs:33-36,48-51`), including **pre-existing dirty changes**; that full diff is what the conductor evaluation sees (`conduct.rs:515,626-636`), so baseline dirt can be misattributed to the worker. Partial mitigation only for the *blackboard*: `run_start_files` snapshots pre-existing dirty files (`conduct.rs:325`) and per-subtask blackboard filters them (`conduct.rs:425-429`) — but the conductor-facing diff is **not** filtered. Safety preflight only *warns* (`preflight.rs:162-166`); `run_conduct` has no dirty-repo guard of its own. **Divergence: needs-adaptation.**

## 5. Can one worker overwrite another's changes?
**Yes, mechanically.** Shared cwd + sequential in-place writes (`write:true`, `conduct.rs:457`) + no isolation = a later subtask's worker has full write access to everything an earlier accepted subtask produced; no locks, no overlap detection, no worktree separation. Across subtasks it is an unguarded hazard. Only guards: prompt-level disjointness (§3) + read-only blackboard (`prompts.rs:169-176`). **Failed-subtask writes are never rolled back** (no `git reset`/revert on failure), so partial/broken files persist for later workers and verify. **Divergence: fundamentally-divergent.**

## 6. How do failed subtasks affect later subtasks?
DAG handling sound; shared-tree side effects not. **Skip cascade:** a subtask with any `depends_on` not in `completed` is skipped, and a skip never enters `completed`, so dependents transitively skip (`conduct.rs:387-412`; test `conduct_test.rs:3442`). **Isolation of independents:** a failed subtask doesn't stop independent siblings (`conduct.rs:355`; grounding of `failed` first-wins `conduct.rs:340-342`; test `conduct_test.rs:3361`). **Global aborts:** supervisor `Halt` + budget-exceeded `break 'plan` (`conduct.rs:583-592,370-381`). **Unrolled side effects:** a failed subtask's partial writes remain in the shared cwd (no revert). **Divergence: needs-adaptation** (DAG semantics 007-compatible; lack of rollback in a shared tree diverges).

## 7. Is verification INDEPENDENT of the worker (or self-reported)?
**Mixed.** **Independent:** `verify::run_verify` executes real build/test/lint **shell commands** in cwd (`verify.rs:75-166`), auto-detected by repo markers or config (`verify.rs:43-68`) — artifact execution, not self-report. **But optional**: no config + no detected ecosystem ⇒ `ran=false` (`verify.rs:83-89`; test `conduct_test.rs:2259`). **Agent gate:** the conductor evaluation is a *different* agent than the worker (never self-review), but it is model judgment consuming the worker's own `worker_report` (`conduct.rs:626-636`, `prompts.rs:197-209`). **Divergence: partially-compatible** (independent when a verifier runs; agent-opinion-only when absent).

## 8. Can an agent verdict OVERRIDE failed tests?
**No — grounding rule forbids it.** If `verify.ran && !verify.passed && decision == Accept`, decision is forcibly rewritten to `Rework` (`conduct.rs:669-680`); prompt: "Build/test results are AUTHORITATIVE" (`prompts.rs:199-201`); test `conduct_test.rs:2176`. **Caveat (agent-overrides-*agent*):** the **arbiter** can `Ship` over the **reviewer's** critical findings after rework exhaustion (`conduct.rs:1196-1202`) — but the reviewer is an LLM diff-opinion, not the test suite; grounding still gates the accept that precedes review, and only fires when `verify.ran`. **Divergence: compatible** for tests; flag the arbiter-overrides-reviewer path as an intentional agent-over-agent escape hatch.

## 9. Is an UNKNOWN verdict fail-closed?
**Parse-unknown: yes, consistently. Verification-absent: no (fail-open).** Unparseable conductor eval → `Rework` (`conduct.rs:64-71,657-662`); supervisor → `Concern` (`:97-104,570-575`); arbiter → `Fail` (`:151-159,1192-1195`); reviewer verdict → fail-closed `review_blocks=true` (`:112-115`); unknown triage → full pipeline (`:117-120`). *Minor exception:* unknown/missing/null **severity** → `Minor` (fail-open for that field, `review.rs:30-37`). **Absence of verification is fail-OPEN:** with no verifier a subtask can be accepted on the conductor's word (`verify_status="not_run"`, grounding skipped because `ran==false`, `conduct.rs:669`; test `conduct_test.rs:2259`). The prompt only *asks* the conductor to be conservative (`prompts.rs:201-202`). **Divergence: needs-adaptation.**

## 10. Are retries BOUNDED?
**Yes, at every level.** Rework per subtask `MAX_REWORKS=2` → 3 attempts (`conduct.rs:212,437`). Stagnation circuit-breaker on identical `(diff,verify)` fingerprint (`stagnation.rs:19-32`; `conduct.rs:527-532,813-842`). Replans bounded by `max_replans`, default 0 (`conduct.rs:860,193-194`). Per-rung failover `RetryConfig.max_retries`(prod 3) + exponential backoff + rate-limit "cold" breaker `cold_after=3` (`resilience.rs:47-70,122-136,250-270`); timeouts capped at 1 retry (`:260-262`). Optional global wall-clock budget (`conduct.rs:370-381,857-861`). Per-command verify timeout default 600s SIGKILL (`verify.rs:82-126`). **Divergence: compatible.**

## Summary table

| Concern | Consilium `conduct` behavior | 007 RunGraph requirement | Divergence |
|---|---|---|---|
| Shared cwd across workers | Single in-place cwd for all agents; worktree code never called from a run path (`conduct.rs:231,448`) | Separate worktree per run/agent | fundamentally-divergent |
| Sequential vs parallel | Strictly sequential nested `for`; no `join_all` (`conduct.rs:354-355,437`) | Parallel agents in separate runs/worktrees | needs-adaptation |
| Disjoint-files assumption | Prompt only, no enforcement (`prompts.rs:113`; `conduct.rs:328-331`) | Not required — worktrees isolate | fundamentally-divergent |
| Dirty repository | In-place; conductor-facing `git diff HEAD` includes baseline dirt; only preflight warns | Fresh worktree off clean base | needs-adaptation |
| Cross-worker overwrite | Possible; shared tree, no locks/rollback (`conduct.rs:457,330-331`) | Structurally impossible (separate worktrees) | fundamentally-divergent |
| Failed → later subtasks | DAG skip-cascade + independent isolation, but no rollback of failed writes (tests 3361/3442) | Isolated; failed work discarded with its worktree | needs-adaptation |
| Verification independence | Real build/test = independent but optional; conductor eval = separate agent reading worker self-report | Independent gates + artifact checks | partially-compatible |
| Agent verdict vs failed tests | Grounding forces Accept→Rework on test fail (`conduct.rs:669-680`; test 2176); arbiter may ship over *reviewer* (`:1196-1202`) | Tests/gates win; DONE ≠ acceptance | compatible (tests); note arbiter escape hatch |
| Unknown verdict fail-closed | Parse-unknowns fail-closed; **no-verifier accepts on agent word** (`conduct.rs:669`; test 2259) | Accept only after independent gates | needs-adaptation |
| Retries bounded | Bounded everywhere (`conduct.rs:212,860`; `resilience.rs:47-70`) | Bounded retries | compatible |

### Notes
- **Biggest structural gap for 007:** the worktree isolation library (`safety/git.rs`, `create_detached_worktree`) is fully built+tested but **not wired into `run_conduct`** (in-place shared-cwd). Adapting to RunGraph's "separate worktrees per run" is primarily a *wiring* job, not new isolation primitives.
- **Council is already advisor-shaped** (read-only, no verdict); packaging it as a Skill is low-risk, but its synthesis is *ungrounded* (no tests inside council).
- **Two fail-open spots to watch:** (1) no-verifier ⇒ accept on conductor's word (`conduct.rs:669` gated on `verify_outcome.ran`); (2) unknown/missing severity ⇒ `Minor` in the conduct reviewer (`review.rs:30-37`). Everything else on the parse path is fail-closed.
