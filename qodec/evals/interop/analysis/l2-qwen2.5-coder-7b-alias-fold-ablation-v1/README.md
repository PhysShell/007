# Level-2 alias × structural ablation — Qwen2.5-Coder-7B

Same model/tokenizer/determinism as the canonical record (model `qwen2.5-coder-7b-instruct`, qodec `e82d5f3e3154`). Six arms in one run.

Arms: **R** raw+brief · **I** identity (framing only) · **M** mine over raw (alias only) · **F** structural fold/grep only · **MF** production squeeze · **VG** fold-grep-guarded (fold/grep shelf + guarded mine).

> VG is NOT "guarded squeeze": it also drops the diag/tmpl/toon shelf, so an `MF fail / VG pass` flip is candidate-policy evidence, not a lexical-guard attribution. VG's structural shelf (fold/grep) differs from MF's (toon/fold/grep/diag/tmpl), so no stage-1-matched arm pair exists in this run — the lexical guard's effect is NOT isolated here. Commit I's SM/SG arms (same frozen stage-1, guard on/off) isolate it.

**Conclusion: 2/5 losses have a confirmed factor (alias main effect); 3/5 are production-stage effects unresolved without a stage-matched S/SM/SG comparison (Commit I). Candidate policies that pass the gate: VG — candidate-policy evidence, NOT a causal claim that a lexical guard fixed production squeeze. Blind production squeeze remains rejected.**

## per-question truth tables

### build-log-rtk-log / n-warnings — LOSS
| arm | correct | stable | format | leaks | invalid | ptok |
|-----|---------|--------|--------|-------|---------|------|
| R | ✓ | stable | ok | 0 | 0 | 583 |
| I | ✓ | stable | ok | 0 | 0 | 593 |
| M | ✓ | stable | ok | 0 | 0 | 581 |
| F | ✓ | stable | ok | 0 | 0 | 593 |
| MF | ✗ | stable | ok | 0 | 0 | 582 |
| VG | ✓ | stable | ok | 0 | 0 | 593 |

causal verdict: production-stage effect unresolved (only MF fails; its outer codec is structural)
candidate-policy rescue (non-causal): F, VG

### clap-derive-explore / def-path — LOSS
| arm | correct | stable | format | leaks | invalid | ptok |
|-----|---------|--------|--------|-------|---------|------|
| R | ✓ | stable | ok | 0 | 0 | 4775 |
| I | ✓ | stable | ok | 0 | 0 | 4785 |
| M | ✗ | stable | ok | 0 | 1 | 4511 |
| F | ✓ | stable | ok | 0 | 0 | 4785 |
| MF | ✗ | stable | ok | 0 | 1 | 4230 |
| VG | ✓ | stable | ok | 0 | 0 | 4684 |

causal verdict: alias main effect confirmed (I pass, M fail)
candidate-policy rescue (non-causal): F, VG

### clap-derive-explore / top-symbol — LOSS
| arm | correct | stable | format | leaks | invalid | ptok |
|-----|---------|--------|--------|-------|---------|------|
| R | ✓ | stable | ok | 0 | 0 | 4773 |
| I | ✓ | stable | ok | 0 | 0 | 4783 |
| M | ✓ | stable | ok | 0 | 0 | 4509 |
| F | ✓ | stable | ok | 0 | 0 | 4783 |
| MF | ✗ | stable | ok | 1 | 1 | 4228 |
| VG | ✓ | stable | ok | 0 | 0 | 4682 |

causal verdict: production-stage / mine interaction unresolved (only MF fails; MF mined)
candidate-policy rescue (non-causal): F, VG

### rtk-rg-derive-clap / file — LOSS
| arm | correct | stable | format | leaks | invalid | ptok |
|-----|---------|--------|--------|-------|---------|------|
| R | ✓ | stable | ok | 0 | 0 | 3949 |
| I | ✓ | stable | ok | 0 | 0 | 3959 |
| M | ✓ | stable | ok | 0 | 0 | 2877 |
| F | ✓ | stable | ok | 0 | 0 | 3199 |
| MF | ✗ | stable | ok | 0 | 1 | 2315 |
| VG | ✓ | stable | ok | 0 | 0 | 3181 |

causal verdict: production-stage / mine interaction unresolved (only MF fails; MF mined)
candidate-policy rescue (non-causal): F, VG

### rtk-rg-parser-clap / file — LOSS
| arm | correct | stable | format | leaks | invalid | ptok |
|-----|---------|--------|--------|-------|---------|------|
| R | ✓ | stable | ok | 0 | 0 | 1244 |
| I | ✓ | stable | ok | 0 | 0 | 1254 |
| M | ✗ | stable | ok | 0 | 0 | 1087 |
| F | ✓ | stable | ok | 0 | 0 | 1116 |
| MF | ✗ | stable | ok | 0 | 0 | 1016 |
| VG | ✓ | stable | ok | 0 | 0 | 1116 |

causal verdict: alias main effect confirmed (I pass, M fail)
candidate-policy rescue (non-causal): F, VG

### build-log-rtk-log / n-errors — control
| arm | correct | stable | format | leaks | invalid | ptok |
|-----|---------|--------|--------|-------|---------|------|
| R | ✓ | n=1 | ok | 0 | 0 | 583 |
| I | ✓ | n=1 | ok | 0 | 0 | 593 |
| M | ✓ | n=1 | ok | 0 | 0 | 581 |
| F | ✓ | n=1 | ok | 0 | 0 | 593 |
| MF | ✓ | n=1 | ok | 0 | 0 | 582 |
| VG | ✓ | n=1 | ok | 0 | 0 | 593 |

### clap-derive-explore / trait — control
| arm | correct | stable | format | leaks | invalid | ptok |
|-----|---------|--------|--------|-------|---------|------|
| R | ✓ | n=1 | ok | 0 | 0 | 4775 |
| I | ✓ | n=1 | ok | 0 | 0 | 4785 |
| M | ✓ | n=1 | ok | 0 | 0 | 4511 |
| F | ✓ | n=1 | ok | 0 | 0 | 4785 |
| MF | ✓ | n=1 | ok | 0 | 0 | 4230 |
| VG | ✓ | n=1 | ok | 0 | 0 | 4684 |

### clap-derive-explore / trait-path — control
| arm | correct | stable | format | leaks | invalid | ptok |
|-----|---------|--------|--------|-------|---------|------|
| R | ✓ | n=1 | ok | 0 | 0 | 4779 |
| I | ✓ | n=1 | ok | 0 | 0 | 4789 |
| M | ✓ | n=1 | ok | 0 | 0 | 4515 |
| F | ✓ | n=1 | ok | 0 | 0 | 4789 |
| MF | ✓ | n=1 | ok | 0 | 0 | 4234 |
| VG | ✓ | n=1 | ok | 0 | 0 | 4688 |

### rtk-rg-parser-clap / symbol — control (weak match)
| arm | correct | stable | format | leaks | invalid | ptok |
|-----|---------|--------|--------|-------|---------|------|
| R | ✓ | n=1 | ok | 0 | 0 | 1239 |
| I | ✓ | n=1 | ok | 0 | 0 | 1249 |
| M | ✓ | n=1 | ok | 0 | 0 | 1082 |
| F | ✓ | n=1 | ok | 0 | 0 | 1111 |
| MF | ✓ | n=1 | ok | 0 | 0 | 1011 |
| VG | ✓ | n=1 | ok | 0 | 0 | 1111 |

### rg-output-rtk-grep / method — control (weak match)
| arm | correct | stable | format | leaks | invalid | ptok |
|-----|---------|--------|--------|-------|---------|------|
| R | ✓ | n=1 | ok | 0 | 0 | 869 |
| I | ✓ | n=1 | ok | 0 | 0 | 879 |
| M | ✓ | n=1 | ok | 0 | 0 | 837 |
| F | ✓ | n=1 | ok | 0 | 0 | 879 |
| MF | ✓ | n=1 | ok | 0 | 0 | 818 |
| VG | ✓ | n=1 | ok | 0 | 0 | 879 |

## candidate-policy gate (may a policy advance to a full rerun?)

Candidate-policy evidence only — passing this gate is NOT a causal claim.

| arm | rescued | control regr. | leaks | invalid Δ vs R | tok save total / median / % vs R | advances |
|-----|---------|---------------|-------|----------------|----------------------------------|----------|
| I | 5/5 | 0 | 0 | 0 | -100 / -10.0 / -0.4% | no |
| M | 3/5 | 0 | 0 | 1 | 2478 / 210.5 / 9.0% | no |
| F | 5/5 | 0 | 0 | 0 | 936 / -10.0 / 3.4% | no |
| MF | 0/5 | 0 | 1 | 3 | 4323 / 386.5 / 15.7% | no |
| VG | 5/5 | 0 | 0 | 0 | 1358 / 91.0 / 4.9% | YES |

## byte-identical arms (not independent results)

- build-log-rtk-log: F == VG (same artifact bytes)
- rg-output-rtk-grep: F == VG (same artifact bytes)
- rtk-rg-parser-clap: F == VG (same artifact bytes)

## priority ranking (lexicographic: quality → integrity → tokens → latency)

Not a Pareto frontier — a priority ordering of the arms.

ranked: VG > F > R > I > M > MF

```json
[
  {
    "arm": "R",
    "correct": 10,
    "alias_leaks": 0,
    "invalid_identifiers": 0,
    "mean_prompt_tokens": 2756.9,
    "mean_latency_ms": 73112.0
  },
  {
    "arm": "I",
    "correct": 10,
    "alias_leaks": 0,
    "invalid_identifiers": 0,
    "mean_prompt_tokens": 2766.9,
    "mean_latency_ms": 70800.0
  },
  {
    "arm": "M",
    "correct": 8,
    "alias_leaks": 0,
    "invalid_identifiers": 1,
    "mean_prompt_tokens": 2509.1,
    "mean_latency_ms": 60523.0
  },
  {
    "arm": "F",
    "correct": 10,
    "alias_leaks": 0,
    "invalid_identifiers": 0,
    "mean_prompt_tokens": 2663.3,
    "mean_latency_ms": 65250.0
  },
  {
    "arm": "MF",
    "correct": 5,
    "alias_leaks": 1,
    "invalid_identifiers": 3,
    "mean_prompt_tokens": 2324.6,
    "mean_latency_ms": 52838.0
  },
  {
    "arm": "VG",
    "correct": 10,
    "alias_leaks": 0,
    "invalid_identifiers": 0,
    "mean_prompt_tokens": 2621.1,
    "mean_latency_ms": 64495.0
  }
]
```
