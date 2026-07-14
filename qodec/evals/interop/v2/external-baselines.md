# External baselines — what we borrow, what we refuse

Interop Benchmark v2 borrows *methods* from external benchmarks but keeps its
own gates clean. The recurring failure of token-reduction benchmarks is
collapsing distinct things into one number — savings without comprehension, or
lossy compression mixed into a transparency claim. This document separates the
useful signal from the inadmissible verdict for each external source.

VG is scored as a **lossless notation layer**. Any *lossy* reducer (RTK,
prompt-compression) is kept on the opposite side of the transparency gate.

---

## RTK smoke / output benchmark

**Useful:**
- broad command matrix,
- real tool outputs,
- per-command savings,
- negative and no-gain cases (kept, not hidden).

**Not accepted as semantic evidence:**
- character-count token approximation (must use the real target tokenizer),
- a savings-only verdict,
- absence of a downstream comprehension scorer.

## RTK session benchmark

**Useful:**
- RTK ON / OFF environments,
- paired agent sessions,
- separate VM setup,
- pass-rate comparison,
- captured manifests and outputs.

These inform the future agent A/B design; they do not substitute for the reader
retention gate.

## Terminal-style task benchmarks

For the future agent A/B we take:
- isolated environments,
- verifiable tasks,
- comprehensive tests,
- fixed budgets,
- final artifact validation.

## Prompt-compression benchmarks

We take:
- task accuracy,
- compression ratio,
- preprocessing cost,
- inference cost,
- break-even analysis,
- model × hardware interaction.

**Refused:** mixing lossy compression methods into the VG transparency gate.
Lossy methods trade comprehension for size; VG's transparency gate asserts
comprehension is *retained*. They are measured, but never averaged together.

---

## How this maps onto v2

| External method | Where it lands in v2 |
|---|---|
| Broad command matrix, real outputs | 12 families × real captures, hazard coverage |
| Per-command savings, no-gain cases | L1 savings distribution + passthrough accounting |
| Downstream comprehension scorer | L2 reader retention gate (rule-scored, no LLM judge) |
| Paired ON/OFF sessions, separate VMs | Agent A/B pilot (documented in `coverage-matrix.json`) |
| Break-even, model × hardware | L1 runtime observation (required, not advertised) |
| Lossy reduction (RTK, prompt-compression) | Interoperability experiment arm — **outside** the transparency gate |

Two numbers that must never merge: **token savings** (L1) and **comprehension
retention** (L2). A method that shrinks output while quietly losing answers
fails v2 even if its savings look excellent, because the reader gate scores what
survived — not what was removed.
