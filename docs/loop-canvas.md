# Loop Canvas: `o7 run` mapped to the nine fields

Status: design note · Scope: `o7 run` (the MVP unit) · Source:
[loop-engineering-canvas](https://github.com/alindnbrg/loop-engineering-canvas)

The [loop-engineering canvas](https://github.com/alindnbrg/loop-engineering-canvas)
is a **design vocabulary, not code** — nine fields you fill in *before* wiring an
agentic loop, in a deliberate order: name **Goal** and its **gate** first, then
**Trigger / Actions / State / Limits / Control / Observability**, and pick the
**Model & Prompt** *last* (the swappable engine). The point is to stop a harness
from drifting into "`claude` with a shotgun and `bash` access" — an agent whose
authority nobody wrote down.

`o7 run` is already almost a canvas; it just never had the label pinned on. This
note does that: it maps the MVP as it exists in the tree today, honestly marks
where each field is thin, and records where the missing loop parts attach. It is
descriptive, not a spec — nothing here changes behaviour.

## The flow, and what 007 implements

The canvas models a loop as:

```
trigger → state → agent → effect → gate → control → [cycle or stop]
```

`o7 run` today implements a **one-shot slice** of that — no cycle:

```
CLI invocation → worktree → agent (full-auto) → diff → gate (bash steps) → verdict → exit code
```

That is on purpose. The MVP is "one isolated, gated run (the unit)"; the loop
(retry / repair / escalate) is deferred until real run records justify it. The
table below marks the one-shot reality against each canvas field so the deferred
parts have a named home.

## The nine fields, mapped to the code

| # | Field | In `o7 run` today | Where it is thin / where the loop attaches |
|---|-------|-------------------|--------------------------------------------|
| 1 | **Goal** — the outcome, and how *done* is checked by something other than a human | Gate steps in `.007/gate.toml` reduce to `PASS`/`FAIL`/`ERROR` (`verdict.rs::Verdict::reduce`); exit `0` only on `PASS`, so CI can gate on it. | "Done" is *implicit* in whatever the target repo's gate asserts — there is no explicit per-target "definition of done" statement. A `[goal] done = "…"` note in the manifest (documentation-only) would make the intent legible. |
| 2 | **Problem** — the recurring work that justifies a loop, and what stays human | "One isolated, gated agent run" over the public repos (Own.NET first, OwnAudit Phase 2). | The recurring-work case (nightly, a task queue, PR-triggered) is not built — the unit is invoked by hand. One-shot is not yet a loop; keeping that line sharp is the whole reason for this doc. |
| 3 | **Trigger** — how work enters, and what one unit of work is | `o7 run --repo <path> --base <ref> --task ./task.md [--gate <toml>]` (`main.rs`). One unit = one worktree run at one base commit. | No scheduler / queue / event trigger; concurrency policy is "one run per invocation." A queue + `run-id` dedup is the natural first extension of **State**. |
| 4 | **Actions** — what the agent may read, write, call, decide; scope, network, isolation | Agent runs `claude` full-auto (`codex` is a Phase-2 engine flag, not wired yet — `agent.rs` bails) under `bypassPermissions` + a **deny-list** on irreversible ops (`agent.rs::DENY`). Gate steps run `bash -lc <cmd>` with `current_dir(worktree)` (`gate.rs`). | `current_dir(worktree)` sets **cwd, not a boundary** — a step can still write outside the tree via absolute/`..` paths, read anything the user can, and reach the network (see `security-layers.md`). There is **no** write-scope, forbidden-path, or per-step least-privilege enforcement. This is the sandbox slot — the enforcement tool now exists (`sandboy`, below). |
| 5 | **State** — what persists between runs, and who may write it | `runs/<target>/<run-id>/`: `task.md`, `meta.json` (`RunMeta`), `agent.stdout`, `diff.patch`, `gate/*.log` (`record.rs`). Append-only archive. | It is an **archive, not a state machine**: `RunMeta` has no `task_hash`, `diff_hash`, or normalized `failure_signature`, so there is no cross-run index for dedup / retry / "has this failed before?" queries. A `runs/index.*` ledger *over* the existing per-run dirs is new infra, and it is what unblocks **Control**. |
| 6 | **Limits** — token/cost budget, max iterations, circuit breaker, no-progress detection | `--max-turns` caps the agent (`main.rs` → `agent.rs`). | No gate/agent **timeout**, no cost cap, no max-diff-size, no per-step log-size cap, no max-retries (there is no retry yet). A hung gate step blocks the run and can produce an unbounded log. These are per-step manifest fields waiting to be added. |
| 7 | **Control** — after the gate: continue / retry-with-feedback / repair / escalate / pause / stop / ship | A single exit code from `Verdict::reduce`: `PASS` → `0`, else `1`. | The entire control layer is absent — one run, one verdict, done. `o7 loop` (feed gate logs + diff summary back, retry with a bound, stop on repeated `failure_signature`) lives here, and it **depends on** the ledger in **State** to detect no-progress. |
| 8 | **Observability** — can any run be reconstructed after the fact? | Strong forensic record: `meta.json` + `diff.patch` + `agent.stdout` + `gate/*.log`; per-step `StepVerdict` with exit codes. | Not yet machine-legible for a loop: no per-step **durations/cost** in the record, no `failure_signature`, no changed-files list, and **no sandbox-enforcement evidence** (whether confinement was fully/partially/not enforced). `sandboy --report` (not yet implemented) is the missing evidence source. |
| 9 | **Model & Prompt** — the swappable engine, chosen last | `--engine claude|codex`, `--model <id>`; the judge path already abstracts the provider (`judge.rs`). | Healthy: the engine is a flag, not baked into loop logic. **Keep it that way** — model choice must stay downstream of the harness, per the canvas. |

## The `run`/gate sandbox slot, and the `sandboy` contract

Fields **4 (Actions)** and **8 (Observability)** both point at the same gap: gate
steps are `bash -lc` under `bypassPermissions` with no confinement, and no
evidence that any confinement ran. `docs/security-layers.md` marks this as 007's
sharpest present-day trust boundary — the "sandbox slot" is here in `run`/gate,
not in `judge`. (The claude judge path is closed-world — read-only tools plus
no-network; the codex path is `--sandbox read-only`, which denies writes but
does not close network egress (`src/judge.rs`) — so for untrusted source the
codex-backed judge still has an exfiltration channel and belongs on the same
sandbox roadmap.)

The enforcement tool now exists as a sibling in Own.NET: **`sandboy`** — a
Landlock + seccomp *wrap-the-child* confinement (no root, no daemon), invoked as

```
sandboy run --policy <step-policy> -- bash -lc '<step.cmd>'
```

Landlock and seccomp both survive the `execve`, so the wrapped step and everything
it spawns inherit the cage. That is exactly the **Actions** boundary the canvas
asks for, per gate step (a `fmt` step gets RO toolchain + no network; a `test`
step gets a writable target dir + port 443). A machine-readable report (a
proposed `sandboy --report`, not yet implemented) is the **Observability**
evidence field 8 wants.

Wiring is a per-step `sandbox_policy` on `GateStep` — not yet added (`GateStep` is
`name`/`cmd`/`required`/`env` today). The manifest parser tolerates unknown
fields, which keeps *benign* additions forward-compatible — but a sandbox policy
is a **security control**, and there that same tolerance is a **fail-open** trap:
an older `o7` that predates the field would silently ignore a target repo's
`sandbox_policy` and run the step as bare `bash -lc` under `bypassPermissions` —
precisely the confinement the field exists to add. So it must **fail closed**:
gate it behind a manifest `schema` bump (or an explicit presence check) so a
runner that sees a `sandbox_policy` it cannot enforce **refuses the step** rather
than running it unconfined — per `security-layers.md`, "a deny is decoration if
we call the tool before asking." See `docs/security-layers.md` and
`Own.NET/sandboy/README.md` for the two sides of the contract.

## Roadmap, by cost (descriptive, not committed)

Ordered so no floor is built on an un-sandboxed one below it:

- **Floor 0 — docs (this).** Pin the canvas labels on `o7 run`; reconcile the
  `sandboy` model in `security-layers.md`/`README.md` (it is Landlock+seccomp
  today, not the earlier Wasmtime framing). Cheap; stabilizes the design.
- **Floor 1 — Actions/Limits/Observability at the gate.** Extend `GateStep` with
  `sandbox_policy` + `timeout_sec`; run steps through `sandboy` when a policy is
  set; emit a `sandboy --report` JSON into `gate/<step>.sandbox.json`. A bare
  legacy step (no `sandbox_policy`), loudly warned, is a *migration-window*
  allowance only — the end state is `docs/zero-trust-framework.md` §4's
  non-negotiable fail-closed rule: a gate step with no `sandbox_policy` does not
  run. Gate the security field behind a manifest `schema` bump so an older `o7`
  **fails closed** — refusing a step that carries a `sandbox_policy` it cannot
  enforce, never silently running it bare.
- **Floor 2 — State/Control.** A `runs/` ledger (`task_hash`, `diff_hash`,
  `failure_signature`) over the existing per-run dirs; then `o7 loop` with
  max-retries + no-progress detection keyed on the normalized signature.
- **Floor 3 — loops on top.** Test-factory (paired with a mutation gate, so it
  can't ship green-but-tautological tests), then false-positive / judge triage,
  then dependency-upgrade (deterministic tool does the edit, model only steers).

Floors 1–3 are **code**, tracked separately. This note is Floor 0.
