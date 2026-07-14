# RTK × qodec composition risks & the future VG-RTK-v1 boundary

This document is **design only**. It defines the boundary of a *future*
RTK-aware policy and does **not** implement it, does **not** implement protected
spans, and does **not** change the frozen VG policy or the frozen numeric gates.

## The baseline rule (non-negotiable)

1. The first RTK comparison runs with **unmodified frozen VG**. It is an
   *observational baseline*, nothing more.
2. Only after that baseline exists may a separate future scope propose
   **`VG-RTK-v1`**.
3. Any RTK-aware tuning gets a **new policy name and a new source SHA**. It does
   **not** inherit frozen VG's accepted results as its own evidence.

Frozen VG's verdicts stay exactly as accepted in Scope M:

```
VG FULL L2 CANDIDATE:      ACCEPTED
PRODUCTION SQUEEZE:        REJECTED
VG PRODUCTION PROMOTION:   NOT DECIDED
```

## Why composition is risky

RTK is a **lossy reducer**; qodec/VG is a **lossless notation layer**. Running
VG *after* RTK means VG operates on already-reduced text. Two failure modes:

- **Double-attribution of losses.** A reader miss on `RTK+QODEC` may be RTK's
  truncation, not VG's folding. The transparency leaderboard therefore excludes
  RTK entirely; only the end-to-end utility leaderboard mixes them, and it
  reports success and cost, never compression percentage as a verdict.
- **Destroying recovery affordances.** RTK sometimes leaves the only path back
  to raw as a textual hint. If a naive VG folded that hint away, the agent could
  no longer recover — turning a recoverable reduction into an unrecoverable
  loss.

## Spans a future VG-RTK-v1 MUST consider protecting

Documented here as requirements for the future scope — **not implemented now**:

- recovery paths;
- the commands `tail`, `cat`, `sed`;
- `[full output: …]` markers;
- `[see remaining: …]` markers;
- URLs;
- exit codes;
- error codes;
- exact full paths;
- line numbers;
- patchable identifiers;
- shell fragments.

Each of these is content an agent may need to *act on* or *re-fetch with*.
A lossless notation layer that renders any of them unrecoverable would fail the
integrity gates — which is precisely why protected spans are deferred to a named
future policy with its own evidence, rather than bolted onto frozen VG here.

## What this scope does instead

- Pins and audits the RTK source (`rtk-implementation-map.md`).
- Enumerates RTK output forms and their composition concerns
  (`rtk-output-grammar.json`).
- Defines the non-gating four-arm comparison (`rtk-comparison-contract.json`).
- Ships a non-scoring smoke suite that proves losslessness of frozen VG over
  arbitrary (including RTK-shaped) input, without changing VG.
