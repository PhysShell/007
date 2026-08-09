# TencentDB Agent Memory as prior art for 007 — evaluation

- **Status:** prior-art evaluation · **Disposition `REFERENCE ONLY` is
  maintainer-ratified** under the rule 3 carve-out
  (`docs/evidence-and-decision-discipline.md`: decisions adjudicated in an
  interactive session with the maintainer are already human-agreed, so they are
  ratified, not pending). Adjudicated 2026-08-09.
- **Ratification binding:** per rule 4 the ratification is bound to **this
  file's own revision** — the anchor a versioned file already carries. If the
  disposition or the three retained ideas below are reworded, the new wording is
  not covered and returns to `pending`.
- **Scope:** the memory layer proposed in `docs/agent-memory-layer.md`. **No
  `o7` code and no roadmap change follow from this document.** A1-V0 remains
  next (`TODO.md`, "A-series status").
- **Subject:** `github.com/TencentCloud/TencentDB-Agent-Memory` (MIT), branch
  `feat/server_team` at `fe3230f` (2026-08-06) — the repository's current
  default branch. Latest release tag `v2.0.0` (2026-08-03).
- **Date:** 2026-08-09.
- **Method note:** an earlier pass of this evaluation was **withdrawn by its
  author** before it reached the tree; the failure class it demonstrated is
  recorded below under "Withdrawn: the first pass", because it is the more
  transferable result.

Per rule 4, every factual claim below about the subject is bound to `fe3230f`
and is stale the moment the upstream file changes. Claims are split into what the
artifact **says**, the **inference** drawn on top, and the **decision** for 007.

## Summary

TencentDB Agent Memory is a self-hosted memory product for agent teams. It
manages four asset kinds (Chat Memory, Skill, LLM-Wiki, Code-Graph) behind a
Memory Hub that carries owner, version, status, visibility and ACL, and binds
specific assets to specific agents before retrieval runs.

It is genuinely more than a vector store with a chat log in it, and several of
its primitives are recognisable relatives of things 007 is circling. That
recognition is precisely where an evaluation of it goes wrong, so the finding is
stated up front:

**Most of what looks borrowable is the solution to a product problem 007 does
not have.** Owner, visibility, ACL, team promotion and a Hub UI exist because
the subject serves multiple principals across an organisation. 007 is, by its
own README, a *personal harness* with exactly one operator, a default memory
scope of `private-local`, and an explicit precondition list that must be
satisfied before team memory is even considered
(`docs/agent-memory-layer.md`, "Memory scope"). Importing that governance
surface would be answering a question nobody asked, in a repository whose
`docs/workflow-scripting.md` already names that exact trap for a neighbouring
feature.

What survives the filter is small, structural, and local: two concrete gaps in
007's own record model, plus one invariant that the subject independently
demonstrates is worth stating explicitly.

## What it actually is (verified at `fe3230f`)

| Artifact says | Where |
|---|---|
| Four asset kinds: "turning conversations, docs, and code into four reusable memory assets (Chat Memory, Skill, LLM-Wiki, Code-Graph)" | `README.md` |
| Four-layer chat distillation: L0 Conversation (raw, full context) → L1 Atom (facts, preferences, constraints, events) → L2 Scenario (project-organised knowledge blocks) → L3 Core/Persona (long-term profile, stable patterns) | `README.md` |
| Retrieval bootstraps from the upper layers and falls back to "BM25 + vector retrieval + RRF" over L1/L0, under element/character/timeout limits | `README.md` |
| Access control is first-class: visibility `private` / `team` / `restricted`, with "precise access via User / Role / Agent ACL"; "Fixed Binding + ACL" determines the asset set an agent may see, *before* retrieval | `README.md` |
| A Skill is not a text file: it carries "versions, resource files, trigger boundaries, execution steps, and validation rules" | `README.md` |
| Wiki and Code-Graph are exposed to the agent as callable tools (`tools/list` / `tools/call`), not injected wholesale into the prompt | `README.md` |
| Deployment is a three-service self-hosted topology: "Start all three services in one go (`memory-core` + `memory-hub` + `proxy`)" | `README.md`, install section |
| Runtime is Node.js: badge requires `node >=22.16` | `README.md` |
| LLM credentials are mandatory to operate: "Fill in two sets of LLM parameters (memory group + proxy group)" | `README.md`, install section |
| One published benchmark: PersonaMem 48% → 76% ("+59%" relative), measuring "whether an Agent can correctly understand and apply user information after extended interactions" | `README.md` |
| License: MIT | `LICENSE` |
| Default branch is `feat/server_team`; that branch carries 8 commits, whose messages reference upstream pull requests numbered in the 600–800 range (`#618`, `#802`) | branch listing and commit log at `fe3230f` |

**Inference (marked as such):** the commit count on the default branch is a
branch-pointer artifact, not a project age. A published project whose default
branch is a feature branch makes the repository's apparent history misleading —
the low count is *not* evidence of immaturity, and the referenced PR numbers
contradict that reading outright. The weirdness of pointing `HEAD` at
`feat/server_team` is worth noting on its own; nothing about maturity may be
derived from it. (This paragraph replaces an inference the first pass got wrong;
see below.)

**Inference:** the published benchmark measures conversational persona recall.
It is silent on the correctness of Skills, Wiki, Code-Graph, ACL-scoped routing
and impact analysis — that is, on everything that distinguishes the current
architecture from a chat-memory library. A number that good on one axis is not
evidence on four others.

## Decision: `REFERENCE ONLY`

> TencentDB Agent Memory is not a candidate dependency or near-term
> implementation target for 007.
>
> Its operational model conflicts with 007's current constraints
> (Node/service topology, API-key-backed LLM use, and team-oriented
> governance), while 007's memory layer is explicitly deferred until
> real run data exists.
>
> Three ideas are retained as prior art:
>
> 1. a common envelope for heterogeneous memory records;
> 2. an explicit supersession relation between records;
> 3. mandatory provenance from derived memory back to source artifacts.
>
> No roadmap or milestone changes follow from this evaluation.
> A1-V0 remains next.

### Why the operational model is dispositive

The disqualifying argument is a boundary mismatch, not a maturity judgement.
Maturity is a moving target; a boundary is not.

```text
subject                          007
──────────────────────────────   ────────────────────────────────────────────
Node.js >= 22.16                 Rust / nix flake; "no Node dependency" is a
memory-core + memory-hub + proxy   listed advantage of the local-store option
                                   (docs/agent-memory-layer.md, Option B)

two sets of LLM API credentials  "subscription auth, no API keys" — README.md,
required to operate                first paragraph; auth is external, handled
                                   by `claude login` / `codex login` and never
                                   read or stored (docs/public-governance.md)

owner / visibility / ACL /       one operator; default memory scope
team promotion / Hub UI            `private-local`; team memory gated behind an
                                   explicit precondition list

generative extraction produces   memory is written by 007 from artifacts, never
new memory layers                  by the agent; trust levels separate
                                   `agent-claimed` from `gate-derived` and
                                   `human-confirmed`

backend chosen and shipped       backend deliberately undecided until a real
                                   consumer exists (D1', ratified 2026-08-07)
```

The credential line is the sharpest of these. "Memory extraction is largely
generative" is not only an epistemics objection — it is a *cost in API keys 007
declines to hold*, which is the repository's central operating claim and the
thing `AGENTS.md` rule 1 grades as P0.

### What is retained, and in what form

**1. A common envelope for heterogeneous memory records.** 007's record kinds —
`o7.run`, `o7.task`, `o7.gate`, `ownaudit.finding`, `o7.fix_pattern`,
`o7.failure_pattern`, `o7.decision` (`docs/agent-memory-layer.md`, "Memory item
types") — each repeat `kind`, `schema` and `provenance` by hand, with no shared
shape. The subject's asset envelope is the demonstration that this can be one
declared structure with a typed payload. This is a refactor of records that
already exist, not a new primitive, and it introduces no ownership,
no visibility and no ACL fields. Not scheduled here.

**2. An explicit supersession relation.** 007 expresses supersession as a
*state*: `superseded` is one of the trust levels, and the rule is that it is
hidden by default. What is absent is the edge — *which* record superseded this
one. The relation is already anticipated in the deferred graph sketch
(`Decision -> SupersedesDecision`, Option C of the same document) without being
available in the flat record model. A single resolvable `supersedes: <id>` field
closes a real local gap and is independent of any backend.

**3. Provenance from derived memory back to source artifacts.** Stated
independently of any layer hierarchy, so that it survives both the taxonomy and
the backend:

> Every derived memory record MUST retain a machine-resolvable provenance path
> to the artifact(s) from which it was derived.

**Artifact says:** the subject keeps intermediate levels human-readable and
traceable across Persona → Scenario → Atom → Conversation. **Inference:** the
value of that property does not depend on the levels being *those* levels — it
depends only on derivation existing at all. **Decision:** state it
hierarchy-independently, as above. This is a sharpening of a principle 007
already holds ("Memory is derived from artifacts. Artifacts are not derived from
memory", and "Every memory entry must have provenance"), not an import: what the
subject adds is the reminder to make the path *machine-resolvable* and to
require it at every derivation step, not only at the first.

### Everything else

```text
Agent Loadout          prior art, deferred
ACL / visibility       foreign problem, deferred
L0 / L1 / L2 hierarchy hypothesis, wait for data
Skills lifecycle       conflicts with a ratified decision
Vector / RRF           backend experiment only after a consumer exists
CodeGraph              separate capability, not memory-core
Persona (L3)           irrelevant until a demonstrated need
Hub                    no
```

Three of these carry a specific reason worth keeping:

- **Skills lifecycle** is not merely premature. `docs/workflow-scripting.md`
  already disposed of skills as **"Defer, likely reject"** for this repository,
  with the argument that a skill registry solves a multi-user distribution
  problem a single-user two-repo harness does not have — and named it as the
  same "shiny because it's modern" trap one layer up. Reopening it requires new
  evidence that the earlier decision became wrong, not a fresh enthusiasm.
- **The L0/L1/L2 hierarchy** is a hypothesis about data that does not exist yet.
  007 has not yet exercised `o7 run` on a real coding task (`TODO.md`, "Built &
  working"), and the memory layer sits in the deferred backlog explicitly marked
  *design with real data*. The same document's own verdict on building structure
  ahead of data — "a graph without real run data is architecture cosplay" —
  applies to a layer hierarchy exactly as it does to a graph.
- **Vector / RRF** is constrained by **D1'** (ratified 2026-08-07): 007 takes no
  backend dependency now and the choice waits for a real consumer. The
  permissible next step is therefore *not* "build the minimal one first" — it is
  *do not build*. **D2** constrains whatever is eventually chosen, ours
  included: rank, score, tier and similarity are candidate-selection signals,
  never evidence verdicts.

### On the MIT license

Correct, and it does lower the cost of adapting a specific implementation piece
later. It does not make adaptation free: substantial copied code carries the
copyright and permission notice with it, and this repository is public
(`docs/public-governance.md`). "Easier" is not "unencumbered".

## Withdrawn: the first pass

The first evaluation of this subject reached a superficially similar verdict —
do not integrate — through reasoning that does not survive rule 4, and was
withdrawn by its author. It is recorded because the failure is reusable, and
because a repository that keeps a `D1 → D1'` retraction in the tree should keep
this one too.

```text
substituted a convenient taxonomy for the real one
  claimed 007 splits memory into semantic / episodic / procedural. No such
  split exists anywhere in the tree. The normative model is the seven record
  kinds plus the trust-level ladder. The error mattered because it was
  load-bearing: the conclusion "the subject confirms our design is sound"
  was drawn by comparing the subject against a taxonomy that does not exist.

promoted a possible strategy to a ratified decision
  read "FTS5 first, vector only after a measured gap" as settled policy. D1'
  says the opposite and says it more strictly: no backend is chosen until a
  real consumer exists. The correction changes the admissible next step from
  "build minimally" to "do not build".

conflated execution least-privilege with memory authorization
  007's least-privilege is process confinement (sandboy, Landlock/seccomp,
  per-step policy). Sandboxing the executor is not an argument for ACLs on
  memory assets; the word "scope" appearing in both is not an inheritance.

reopened a ratified decision without addressing it
  recommended a Skill lifecycle without mentioning that this repository had
  already written "Defer, likely reject" with reasons.

ignored the timing constraint
  the memory layer is deferred until real run data exists, which by itself
  disposes of nearly every proposal about new memory architecture right now.

inferred maturity from a weak signal
  read "8 commits on the default branch" as project immaturity, when it is a
  branch-pointer artifact.
```

The common shape, and the reason this belongs in the tree rather than in a chat
log: **an external system's primitives were pattern-matched against a
remembered impression of 007 instead of against its ratified artifacts.** That
is rule 4's failure class in its purest form — a claim about existing
architecture asserted without naming the artifact, the revision, or the exact
property — and the recognition of familiar primitives is exactly what made it
feel like evidence.

## Not measured

- The subject was not deployed, installed, or run. Every claim above is a read
  of its published documentation at `fe3230f`; nothing here is a test of it.
- The storage backend is not stated in the current documentation. Claims
  elsewhere that it uses SQLite with `sqlite-vec` refer to an earlier branch and
  are **unverified**; the current branch names no backend.
- Retrieval quality, ACL enforcement, and Code-Graph impact analysis are
  undemonstrated by the one published benchmark and untested by us.
- Nothing here evaluates whether 007 should have a memory layer at all. That
  question is open and belongs to `docs/agent-memory-layer.md`.

## Final position

The subject is a competent product for a problem 007 does not have: many
principals, many agents, an organisation's worth of documents, and a governance
surface to keep them apart. Its interesting properties — assets before
retrieval, binding before search, tools instead of prompt dumps — are real, and
two of them happen to coincide with actual gaps in 007's record model.

That coincidence is the whole result. It is not confirmation that 007's design
is correct; an external system built for different constraints cannot supply
that, and reading it as though it could is how a survey becomes a mirror.
