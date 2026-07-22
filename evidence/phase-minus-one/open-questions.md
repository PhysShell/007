# Open questions / blockers

Ordered by how much they gate a go/no-go on the acquisition.

## Blockers to resolve before the first acquisition PR
1. **Codex real probe not run (BLOCKED).** The `codex` CLI is not installed locally and installing/login is disallowed by scope + stop-conditions. Stage D's Codex analysis is code-only. Before relying on the Codex adapter under Exact, run a minimal `codex exec --json` probe on a machine where it is authed and confirm whether any observed-model field exists (Stage D found `SessionStarted.model` is hard-`None`). **If Codex truly exposes no observed model, Exact cannot be honored for Codex — decide whether that means "Codex is Exact-ineligible" or "007 must derive the model another way."**
2. **Desktop/Tauri workspace never built (BLOCKED, environment).** `cargo build --workspace` fails on missing system `dbus-1`/`webkit2gtk`/`gtk3`/`libsoup`. The Tauri crate's Rust code was therefore not compiled or tested. Since the recommendation rejects the Tauri shell for android-first, this is low-impact — but if any desktop code is ever salvaged, build it on a host with the GUI dev libraries first.
3. **Single-owner worktree contract.** `git worktree prune --expire now` is repo-global and `PreparedWorktree` has no `Drop` cleanup (a dropped handle leaks). Before wiring the worktree crate, confirm o7d can guarantee it is the *only* creator/pruner of worktrees for a given repo, and decide RAII-vs-explicit cleanup.

## Design questions the acquisition forces
4. **Where does o7d's verdict live relative to `verify.rs`?** The recommendation moves the decision out of the (LLM) conductor into o7d. Confirm the exact contract: verify.rs computes `VerifyOutcome`; o7d combines it with artifact/diff-policy checks to decide accept/fail. What are 007's artifact checks beyond exit codes (007 today has none)?
5. **Trusted config location.** Consilium loads model + verify config from the target repo's `consilium.config.json` (untrusted). Where does 007 keep trusted config, and what is the trust-on-first-use UX for repo-supplied verify commands?
6. **Ledger schema.** Extending `quota.rs`'s append-only SQLite into a run-event ledger: what is the event row schema, the per-run monotonic `seq`, the cursor/`?since=` contract, and the fsync/recovery guarantee? (Consilium's transcript gives none of this.)
7. **Model-id normalization for drift detection.** Stage E showed init reports `claude-opus-4-8[1m]` while the assistant event reports `claude-opus-4-8`. Define the canonical model-id and the normalization rule so the Exact kill-switch does not false-positive on the context-window suffix — and does not false-negative on a real swap.
8. **Live permission-mode switching.** Consilium has no mechanism (operator control is between-subtasks only). Does the Claude SDK worker expose an in-session permission-mode change, and what is the requested-vs-effective display contract 007 needs?
9. **MCP bridge lifetime.** If the temporary MCP bridge is adopted, define the retirement trigger (which vendored worker capabilities must exist before it is removed) so it does not silently become load-bearing.

## Lower-priority / informational
10. **Upstream volatility.** Consilium is a fast-moving single-author MIT project (HEAD 2026-07-16). Vendored modules must be pinned to the studied SHA (`191f2d5…`) with a documented re-sync process; do not track a moving target.
11. **Prompt-injection hardening.** `prompts.rs` interpolates `feedback`/reviewer-findings bare and carries an explicit `TODO(M3)`. Any vendored prompt paths need 007's own injection hardening before trusting repo-derived text.
12. **Shallow clone.** Consilium was cloned `--depth=1`; no test needed history, but if a later step needs tags/history, unshallow first.
