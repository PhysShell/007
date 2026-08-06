# Specula transplant — bounded model checking for the `o7-run` reducer

**Status: research note. The decision it proposes is autonomous, therefore PENDING** until the
maintainer ratifies or rejects it (`docs/evidence-and-decision-discipline.md` rule 3). No code is
introduced by this document. It fixes vocabulary and names one bounded transplant so a later slice
does not re-derive it — or smuggle in the weaker form.

Scope: `crates/o7-run` (the canonical event protocol, the pure reducer, replay). Not the sandbox,
not the ledger, not `o7 invoke`.

Adjacent, deliberately not duplicated: `docs/paper-transplant-map.md` §2.1 proposes a normalized
`trace.jsonl` + claim/evidence ledger for `o7 eval`, citing TriCEGAR for trace-driven abstraction.
That note is about turning agent *behavior* into checkable predicates. This one is about the
already-canonical run-event stream and the reducer that folds it. Same family, different subject.

---

## 0. Method, and what is checked versus asserted

Two external papers and one repository are the sources. Per rule 4 (revision-bound artifact
grounding) each is named with its revision, and every claim below is marked as one of
**artifact says** / **inference** / **decision**.

| Artifact | Revision anchor | Verified |
|---|---|---|
| Specula: *Scaling formal specifications for autonomous model checking of system code* | `arXiv:2607.25333`, submitted 2026-07-28, **v2 revised 2026-08-03** | abstract + full text fetched 2026-08-06 |
| SysMoBench: *Evaluating AI on Formally Modeling Complex Real-World Systems* | `arXiv:2509.23130` | abstract fetched 2026-08-06; the score figures below are quoted from the ACM SIGOPS write-up, **not** re-derived from the paper's own tables — treat them as second-hand until someone reads §5 of the PDF |
| This repository | `HEAD = 2e74c5106821541296e7e4807811edff450bde67` (`2e74c51`), branch `main` at note time | read directly |

Every 007 claim in §3 is a **property** of a named file at that SHA, not a line number. Line numbers
drift; the property is the claim, and the claim is STALE once the cited file changes.

---

## 1. Artifact says — what the two papers establish

**Specula** is an agentic system that reads real system code, has an LLM write a TLA+ model plus
invariants, instruments the code to collect real execution traces, replays those traces against the
model, and repairs the model where the two disagree — while model-checking the invariants to stop the
model from being repaired into something that admits everything.

Reported results, verbatim figures:

```text
48 open-source systems checked
249 bugs      — "207 were new bugs and 42 were known bugs"
89 reported   — "so far, 68 have been confirmed and 24 have been fixed"
cost          — "$19–$168 US dollars of token consumption, with a median of $57"
wall clock    — "1.43 to 9.86 hours", median 3.69 h
vs. baseline  — 4.8–37× the cost of Agent-Raw; 1.8–65× Agent-TLA+ (five systems)
counterexample length — median 9 steps, p90 18 steps; BFS found 93.5% of violations
```

The load-bearing sentence, and the reason this note exists:

> "trace validation and model checking — the former ensures that the model admits code-level
> behaviors and the latter rejects invalid states."

**SysMoBench** establishes why the naïve version fails. Syntax is solved — frontier models cluster
near 100% on writing *valid* TLA+. Meaning is not: conformance to the actual system code averages
~46%, invariant correctness ~41%, and on the hard artifacts (RedisRaft, CURP) a frontier model scores
~25% overall. The characteristic failure is a model that describes the textbook protocol rather than
the implementation in front of it.

**Inference.** The transplantable asset is not TLA+ and not the agent. It is the *two-sided pressure*:
a specification is squeezed between "must admit everything the code really does" and "must forbid
everything the property says is invalid". Either constraint alone is trivially satisfiable by a
degenerate specification. That is a general engineering shape, and it does not require a
specification language to apply.

---

## 2. Artifact says — where 007 already stands at `2e74c51`

007 is unusually well positioned, and the reason is worth stating precisely, because it changes the
cost of the transplant by orders of magnitude.

| Specula stage | 007 equivalent at `2e74c51` |
|---|---|
| instrument the code to emit a trace | **already there** — `src/events.rs` records a digest-chained `events.jsonl` in every run record |
| a formal model of the implementation | **already there, hand-written** — `crates/o7-run/src/state.rs` (`RunState`, a versioned byte-stable normal form) + `crates/o7-run/src/reduce.rs` (the transition relation) |
| replay a trace against the model | **already there** — `crates/o7-run/src/replay.rs` / `o7 replay <run-dir>`: chain continuity, per-event digests, artifact content digests, independent verdict recomputation |
| model-check invariants over the state space | **absent** |
| invariants stated independently of the model | **absent** |

The transition relation is not a sketch. `reduce.rs` carries **50 typed rejection rules**
(`ReduceError` variants — `ProtectedStartBeforePolicyAllowed`, `GateFinishedWithoutStart`,
`EventAfterSeal`, `BrokenChain`, `WaiverEnvironmentMismatch`, …) over a **14-symbol event alphabet**
(`RunEventKind`: `RunStarted`, `WorktreeCreated`, `AgentStarted`, `AgentExited`, `PatchCaptured`,
`PolicyChecked`, `GateStarted`, `GateFinished`, `SandboxEvidenceCaptured`, `ProviderSessionCaptured`,
`CommandBindingCaptured`, `CandidateStateCaptured`, `CandidateStateMaterialized`, `RunSealed`). The
reducer is pure and dependency-light on purpose — `crates/o7-run/src/lib.rs` says the semantics "can
be recomputed anywhere, by anyone, without the ledger, a runtime, or a database".

**Inference.** Specula spends its median $57 and 3.69 hours mostly on the stage 007 does not need:
deriving a formal model from code that has none, then repairing it. 007 wrote that model by hand,
froze its contract before implementation, and keeps it pure. The expensive half of the paper is
already paid for. Only the *checking* half is missing.

---

## 3. The correction — what "we already have this" gets wrong

The tempting reading is that 007's replay is Specula's trace conformance and the job is done. It is
half of it, and the missing half is the half that catches the failure mode the paper is actually
about.

**Gap 1 — the reducer is its own specification.** `reduce.rs` decides both which transitions are
legal and which verdict follows. `crates/o7-run/src/lib.rs` states the crate's first invariant in
prose — *a green verdict means every required obligation was actually discharged* — and nothing
checks it except the reducer whose behavior it describes. A future round that adds an event kind or
relaxes a guard can break that invariant while remaining internally consistent. This is precisely
what the paper's second constraint exists to prevent: without an independently stated property, a
model repaired for conformance drifts toward admitting everything.

**Gap 2 — coverage is example-based.** `crates/o7-run/tests/reducer_transitions.rs` is 1406 lines and
72 `#[test]` functions, each pinning one expected outcome for one hand-authored sequence. That is a good
transition table and a bad search. Specula's own counterexample distribution — median 9 steps, p90 18
— is the relevant yardstick: a defect that needs a 9-event interleaving over a 14-symbol alphabet is
not reachable by 72 authored sequences except by luck. `proptest` exists in this repo but only in
`src/judge.rs`, over pure string functions; the reducer has no property tests at all.

**Gap 3 — replay checks one observed trace, not the space of them.** `replay_verify` re-derives the
verdict for a stream that actually happened. That is trace validation, and it is genuinely the
paper's first constraint. It says nothing about streams that could happen and have not yet.

**Inference.** The three gaps are one gap seen from three sides: 007 has the model and the traces, and
no exploration and no independent property. The two-sided pressure is currently one-sided.

---

## 4. Decision proposed — the transplant worth doing

One bounded slice. Two pieces, deliberately separate.

### 4.1 Invariants stated outside the reducer

A new pure module — `crates/o7-run/src/invariant.rs` — holding `fn(&RunState) -> Result<(), Violation>`
predicates that **`reduce` never calls**. The separation is the entire point: an invariant the reducer
enforces is not a check on the reducer.

Starting set, each derived from something the repo already asserts in prose:

```text
INV-1  Sealed ∧ verdict = Pass  ⇒  every Required+Applicable gate reached GateFinished with a
       passing outcome, AND every Required agent obligation reached Exited with a clean outcome.
       (the crate's own stated invariant 1, restated so it is checkable)
INV-2  Sealed ⇔ verdict.is_some()                     (phase/verdict agreement)
INV-3  every captured sandbox-evidence key names a subject present in the contract
       (no orphan evidence; the typed-key injectivity claim, as a state property)
INV-4  no protected subject is Started before every protecting policy is Allowed
       (today a transition guard; restated as a property of every reachable state)
INV-5  folded sequence numbers are strictly monotone and the chain digest links
```

INV-4 and INV-5 duplicate existing guards on purpose. A property that restates a guard is exactly the
one that catches the guard being deleted.

### 4.2 Bounded exhaustive exploration

A test target that fixes a small contract — one required gate, one optional gate, one required agent,
one policy protecting the agent — enumerates **every** event sequence up to depth *D* over the 14-symbol
alphabet, folds each through `reduce`, and asserts that every *accepted* prefix satisfies every INV.
Breadth-first, matching the paper's finding that BFS located 93.5% of violations. Report the reachable
state count so a later change that collapses the space is visible.

The two-sided pressure, mapped concretely:

```text
conformance direction   every events.jsonl a real `o7 run` emitted must be ACCEPTED by the reducer.
                        A rejection is either a genuine harness bug or an over-strict guard — both
                        worth knowing. Corpus: the stored run records.
                        (Specula's trace validation — 007 already has this via replay.)

invariant  direction    every stream the reducer ACCEPTS must satisfy every INV.
                        A reducer weakened until it accepts anything fails INV-1 immediately.
                        (Specula's model checking — this is the new part.)
```

Neither alone is sufficient, and the reason is worth writing down once: conformance alone is satisfied
by a reducer that accepts everything; invariants alone are satisfied by a reducer that accepts nothing.
Only the pair pins a specification in place.

This slots into `docs/verification.md`'s existing ladder as a fourth rung: proptest → cargo-fuzz →
Kani → bounded reducer exploration. Kani is the wrong tool for this particular rung — `RunState` is
built from `BTreeMap`/`Vec`/`String`, which is where CBMC's symbolic execution stops being the sweet
spot that slice-boundary reasoning is. Concrete BFS over a fixed small contract is the pragmatic form.

---

## 5. Decision proposed — what is explicitly NOT worth doing

Stated so a later slice does not adopt the expensive half by default.

1. **Do not generate a TLA+ specification of `o7-run`.** The reducer already is the model. A second
   model in TLA+ creates exactly the conformance-drift problem Specula spends most of its budget
   repairing — 007 would be paying that cost to acquire a duplicate of something it already has in a
   language its CI already checks.
2. **Do not run Specula against 007 as a target system.** Its 48-system evaluation is concurrent and
   distributed system code. The reducer is a pure fold. If a Specula-shaped effort ever pays here it
   is at the `o7d`/worker concurrency boundary, not at the reducer — and that is a separate note, not
   this one.
3. **Do not adopt LLM-in-the-loop specification repair.** The repair loop exists because the model was
   generated and is therefore untrusted. 007's model is hand-written, contract-frozen, and reviewed.
4. **Do not treat "bugs found" as the acceptance metric.** The deliverable is the standing property,
   not a count. A run of this harness that finds nothing and reports its explored state count is a
   pass, and 007 already knows why a green signal from a lower layer is not a semantic fact of the
   upper one.

---

## 6. Cost

One test target plus one pure module. No new runtime dependency and no nightly toolchain — unlike
`cargo-fuzz` and Kani, both of which `docs/verification.md` records as nightly-only, with Kani
additionally never exercised because its setup bundle is unreachable under the session's egress
policy. It runs inside the existing workspace test job that AGENTS.md already declares mandatory
for every non-excluded member, and `o7-run` is not among the two excluded crates. The single tunable is the depth budget *D*, traded against CI wall-clock; the
paper's median counterexample length of 9 is the reference point for where *D* starts being
interesting.

**Inference.** The 4.8–37× cost multiplier the paper reports does not transfer, because the multiplier
is dominated by the model-generation and repair stages this transplant drops.

---

## 7. `qodec`

The analogous invariant there is `D(E(P)) = P`, and it is already a roundtrip property over the codec
corpus. `qodec` has no event protocol, no run state, and no reducer, so §4 has nothing to attach to.
The transplant is 007-only. Recorded here so the question is closed rather than re-asked.

---

## 8. Disposition

```text
Invariants stated outside the reducer     PROPOSED — pending maintainer ratification
Bounded exhaustive exploration            PROPOSED — pending maintainer ratification
TLA+ model of o7-run                      REJECTED (§5.1)
Specula run against 007                   REJECTED for the reducer; open at the o7d boundary (§5.2)
LLM specification repair                  REJECTED (§5.3)
qodec transplant                          NOT APPLICABLE (§7)
```

Per rule 3 this note is not normative authority and cannot be cited to justify the next autonomous
decision. Per rule 4 every §2 claim is bound to `2e74c51` and is STALE once `crates/o7-run/` changes.
