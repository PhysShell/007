# Agent-papers backlog (007-relevant slice) — reconciliation, not commitments

Source: an external digest of `github.com/VoltAgent/awesome-ai-agent-papers`,
filtered by a third party for what could feed `007` / Own.NET / Sandboy. This
note is the 007-side reconciliation — checked against the actual tree
(`README.md`, `TODO.md`, `docs/security-layers.md`,
`docs/agent-behavior-profiling.md`), not taken on faith, same discipline as
`docs/agent-behavior-profiling.md` itself. Everything below is **recorded for
consideration** — no code commitment, no scheduling, no priority change to
`TODO.md`'s "RESUME HERE."

## Already tracked — do not re-file

| Idea from the digest | Already tracked as | Where |
| --- | --- | --- |
| Deterministic multi-agent judge / consensus merge (claude+codex race, cross-family judge) | the #1 deferred item, already named | `README.md` ("deferred"), `TODO.md` Backlog |
| verify-before-commit / policy-mediated execution around `run`/gate | already identified as 007's sharpest present-day trust gap | `docs/security-layers.md` ("gate is attacker-controlled code execution... the real sandbox slot is here") — the enforcement design itself lives in Own.NET (`agent-capability-layer.md`, `sandboy-isolation-adr.md`); 007 wires it, doesn't design it |
| a real behavioral trace beyond the final-result blob | already identified as the actual prerequisite for *any* eval/profiling layer | `docs/agent-behavior-profiling.md` — "switch `agent.rs` to `--output-format stream-json --verbose`" |

Re-proposing any of these as new work would just duplicate what's already on
the record. The digest's framing ("trace → verifier → memory → eval") is
consistent with what's already deferred here; it doesn't add a new axis.

## New angles worth recording (not scheduled)

### 1. Claims ledger over the run record (`o7 eval`)

From the TrajAD / TriCEGAR / claim-level-eval corner of the digest: verify what
an agent *says* it did against what the artifacts actually show, mechanically.

**Hard dependency:** the same `stream-json` prerequisite as
`docs/agent-behavior-profiling.md`. This is effectively that proposal's
Phase-1 trace output consumed one step further, not a parallel track — build
it after, not alongside.

Sketch (illustrative, not a spec):

```
runs/<target>/<run-id>/
  trace.jsonl          # already-proposed, in agent-behavior-profiling.md
  claims.json          # statements the final result text makes about what it did
  verification.json    # which claims are backed by diff.patch / gate logs / trace.jsonl
  risk.json            # suspicious actions, missing evidence, flaky-gate flags
```

`verification.json` must be produced by mechanical checks against
`diff.patch`/gate output — **not** by asking the model again to confirm its
own claims; that would just be a second untrusted opinion, not evidence.

Non-goals: no LLM-as-judge standing in for the gate; no new behavior taxonomy
beyond what `agent-behavior-profiling.md` already scoped.

Open question, deliberately unresolved: whether this becomes a mode of the
eventual `o7 profile` or a separate command. Not decidable yet — there are
still zero real `o7 run` records to design either against (same blocker
`TODO.md:53` already names).

### 2. Replay + meta-tools

From SWE-Replay / "Optimizing Agentic Workflows using Meta-tools": reuse prior
trajectories, and fold repeated tool-call cascades into deterministic
meta-tools instead of re-deriving them via LLM reasoning every run.

Sketch: `o7 replay <run-id> --from step:gate-fail`; a named meta-tool like
`fmt -> clippy -> test -> patch-summary`.

Honest costs:

- `replay --from step:X` needs the `stream-json` trace too — you cannot resume
  mid-conversation from a single final-result blob, only re-run from scratch.
- The "meta-tool" half already exists structurally: `.007/gate.toml` **is**
  a named, deterministic step sequence. The only genuinely new part would be
  *auto-proposing* a meta-tool from observed repeated patterns across runs —
  which needs a corpus of run records to mine, and there are zero today.

Verdict: plausible later, but strictly behind "first real `o7 run` exercised
on a real coding task" — the same `TODO.md` blocker as section 1.

### 3. "Verify-before-harvest" ordering

The digest frames security work as "verify claims before exposing the patch to
the host." Checked against the actual `run` loop
(isolate → run → gate → harvest, `README.md`): 007 never auto-applies a patch
to the main checkout today — `harvest` only writes the record
(`diff.patch`, `meta.json`, `gate/*.log`) into the private store. So the
property the digest asks for ("nothing reaches the host unverified") already
holds by construction, for the narrow case that exists. It becomes a real
design question only if/when a command that auto-applies a run's diff to a
non-worktree checkout is added — there isn't one. Recorded so the concern
isn't lost, not filed as a gap to close now.

## What NOT to take

Mirrors the source digest's own exclusion list, and it's correct for 007 too:
RL training of agents, long-running self-evolving multi-agent colonies,
GUI/mobile agent training, social-simulation papers, medical-workflow agents,
"agents autonomously open PRs." None of these close 007's actual gap, which is
the `run`/gate trust boundary (`docs/security-layers.md`), not agent
creativity.

## Bottom line

Nothing here changes `TODO.md`'s "RESUME HERE" (the FP-direction gate + STS
run). Filed as backlog, cross-referenced from `TODO.md`'s Backlog section —
read when there are real run records to design against, not before.
