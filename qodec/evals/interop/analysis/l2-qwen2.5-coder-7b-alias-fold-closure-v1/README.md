# Level-2 stage-matched closure ablation — Qwen2.5-Coder-7B

Same model/tokenizer/determinism as the canonical record (model `qwen2.5-coder-7b-instruct`, qodec `a18f2fec1e21`). Six arms in one run.

Arms: **R** raw+brief · **S** production stage-1 only · **SM** production squeeze · **SG** stage-1 + guarded mine (SM/SG share stage-1, differ only in the guard) · **V** fold/grep structural · **VG** fold/grep + guarded mine.

> SM and SG share production's exact stage-1 (squeeze_stage1) and differ only in the mine guard, so SM-fail / SG-pass IS a clean stage-2 lexical-mining attribution.

**Conclusion: lexical guard alone is insufficient; the viable candidate is the simplified structural shelf (VG: fold/grep + guarded mine), which passes the gate. By stage: 4/5 losses are a confirmed stage-2 lexical-mining effect (S pass, SM fail, SG pass); 1/5 fail already at the production structural stage (S fails), where the stage-2 guard cannot help. Gate winners: VG. Full rerun is a separate decision.**

## per-question truth tables

### build-log-rtk-log / n-warnings — LOSS
| arm | correct | stable | format | leaks | invalid | ptok |
|-----|---------|--------|--------|-------|---------|------|
| R | ✓ | stable | ok | 0 | 0 | 583 |
| S | ✗ | stable | ok | 0 | 0 | 582 |
| SM | ✗ | stable | ok | 0 | 0 | 582 |
| SG | ✗ | stable | ok | 0 | 0 | 582 |
| V | ✓ | stable | ok | 0 | 0 | 593 |
| VG | ✓ | stable | ok | 0 | 0 | 593 |

causal verdict: production structural stage itself causes the loss (S fails); V/VG pass → rescue comes from dropping the diag/tmpl/toon shelf, not the guard
candidate-policy rescue (non-causal): V, VG

### clap-derive-explore / def-path — LOSS
| arm | correct | stable | format | leaks | invalid | ptok |
|-----|---------|--------|--------|-------|---------|------|
| R | ✓ | stable | ok | 0 | 0 | 4775 |
| S | ✓ | stable | ok | 0 | 0 | 4785 |
| SM | ✗ | stable | ok | 0 | 1 | 4230 |
| SG | ✓ | stable | ok | 0 | 0 | 4684 |
| V | ✓ | stable | ok | 0 | 0 | 4785 |
| VG | ✓ | stable | ok | 0 | 0 | 4684 |

causal verdict: stage-2 lexical mining effect confirmed (S pass, SM fail, SG pass; stage-1 matched)
candidate-policy rescue (non-causal): SG, V, VG

### clap-derive-explore / top-symbol — LOSS
| arm | correct | stable | format | leaks | invalid | ptok |
|-----|---------|--------|--------|-------|---------|------|
| R | ✓ | stable | ok | 0 | 0 | 4773 |
| S | ✓ | stable | ok | 0 | 0 | 4783 |
| SM | ✗ | stable | ok | 1 | 1 | 4228 |
| SG | ✓ | stable | ok | 0 | 0 | 4682 |
| V | ✓ | stable | ok | 0 | 0 | 4783 |
| VG | ✓ | stable | ok | 0 | 0 | 4682 |

causal verdict: stage-2 lexical mining effect confirmed (S pass, SM fail, SG pass; stage-1 matched)
candidate-policy rescue (non-causal): SG, V, VG

### rtk-rg-derive-clap / file — LOSS
| arm | correct | stable | format | leaks | invalid | ptok |
|-----|---------|--------|--------|-------|---------|------|
| R | ✓ | stable | ok | 0 | 0 | 3949 |
| S | ✓ | stable | ok | 0 | 0 | 3199 |
| SM | ✗ | stable | ok | 0 | 1 | 2315 |
| SG | ✓ | stable | ok | 0 | 0 | 3181 |
| V | ✓ | stable | ok | 0 | 0 | 3199 |
| VG | ✓ | stable | ok | 0 | 0 | 3181 |

causal verdict: stage-2 lexical mining effect confirmed (S pass, SM fail, SG pass; stage-1 matched)
candidate-policy rescue (non-causal): SG, V, VG

### rtk-rg-parser-clap / file — LOSS
| arm | correct | stable | format | leaks | invalid | ptok |
|-----|---------|--------|--------|-------|---------|------|
| R | ✓ | stable | ok | 0 | 0 | 1244 |
| S | ✓ | stable | ok | 0 | 0 | 1116 |
| SM | ✗ | stable | ok | 0 | 0 | 1016 |
| SG | ✓ | stable | ok | 0 | 0 | 1116 |
| V | ✓ | stable | ok | 0 | 0 | 1116 |
| VG | ✓ | stable | ok | 0 | 0 | 1116 |

causal verdict: stage-2 lexical mining effect confirmed (S pass, SM fail, SG pass; stage-1 matched)
candidate-policy rescue (non-causal): SG, V, VG

### build-log-rtk-log / n-errors — control
| arm | correct | stable | format | leaks | invalid | ptok |
|-----|---------|--------|--------|-------|---------|------|
| R | ✓ | n=1 | ok | 0 | 0 | 583 |
| S | ✓ | n=1 | ok | 0 | 0 | 582 |
| SM | ✓ | n=1 | ok | 0 | 0 | 582 |
| SG | ✓ | n=1 | ok | 0 | 0 | 582 |
| V | ✓ | n=1 | ok | 0 | 0 | 593 |
| VG | ✓ | n=1 | ok | 0 | 0 | 593 |

### clap-derive-explore / trait — control
| arm | correct | stable | format | leaks | invalid | ptok |
|-----|---------|--------|--------|-------|---------|------|
| R | ✓ | n=1 | ok | 0 | 0 | 4775 |
| S | ✓ | n=1 | ok | 0 | 0 | 4785 |
| SM | ✓ | n=1 | ok | 0 | 0 | 4230 |
| SG | ✓ | n=1 | ok | 0 | 0 | 4684 |
| V | ✓ | n=1 | ok | 0 | 0 | 4785 |
| VG | ✓ | n=1 | ok | 0 | 0 | 4684 |

### clap-derive-explore / trait-path — control
| arm | correct | stable | format | leaks | invalid | ptok |
|-----|---------|--------|--------|-------|---------|------|
| R | ✓ | n=1 | ok | 0 | 0 | 4779 |
| S | ✓ | n=1 | ok | 0 | 0 | 4789 |
| SM | ✓ | n=1 | ok | 0 | 0 | 4234 |
| SG | ✓ | n=1 | ok | 0 | 0 | 4688 |
| V | ✓ | n=1 | ok | 0 | 0 | 4789 |
| VG | ✓ | n=1 | ok | 0 | 0 | 4688 |

### rtk-rg-parser-clap / symbol — control (weak match)
| arm | correct | stable | format | leaks | invalid | ptok |
|-----|---------|--------|--------|-------|---------|------|
| R | ✓ | n=1 | ok | 0 | 0 | 1239 |
| S | ✓ | n=1 | ok | 0 | 0 | 1111 |
| SM | ✓ | n=1 | ok | 0 | 0 | 1011 |
| SG | ✓ | n=1 | ok | 0 | 0 | 1111 |
| V | ✓ | n=1 | ok | 0 | 0 | 1111 |
| VG | ✓ | n=1 | ok | 0 | 0 | 1111 |

### rg-output-rtk-grep / method — control (weak match)
| arm | correct | stable | format | leaks | invalid | ptok |
|-----|---------|--------|--------|-------|---------|------|
| R | ✓ | n=1 | ok | 0 | 0 | 869 |
| S | ✓ | n=1 | ok | 0 | 0 | 879 |
| SM | ✓ | n=1 | ok | 0 | 0 | 818 |
| SG | ✓ | n=1 | ok | 0 | 0 | 879 |
| V | ✓ | n=1 | ok | 0 | 0 | 879 |
| VG | ✓ | n=1 | ok | 0 | 0 | 879 |

## candidate-policy gate (may a policy advance to a full rerun?)

Candidate-policy evidence only — passing this gate is NOT a causal claim.

| arm | rescued | control regr. | leaks | invalid Δ vs R | tok save total / median / % vs R | advances |
|-----|---------|---------------|-------|----------------|----------------------------------|----------|
| S | 4/5 | 0 | 0 | 0 | 958 / -4.5 / 3.5% | no |
| SM | 0/5 | 0 | 1 | 3 | 4323 / 386.5 / 15.7% | no |
| SG | 4/5 | 0 | 0 | 0 | 1380 / 91.0 / 5.0% | no |
| V | 5/5 | 0 | 0 | 0 | 936 / -10.0 / 3.4% | no |
| VG | 5/5 | 0 | 0 | 0 | 1358 / 91.0 / 4.9% | YES |

## priority ranking (lexicographic: quality → integrity → tokens → latency)

Not a Pareto frontier — a priority ordering of the arms.

ranked: VG > V > R > SG > S > SM

```json
[
  {
    "arm": "R",
    "correct": 10,
    "alias_leaks": 0,
    "invalid_identifiers": 0,
    "mean_prompt_tokens": 2756.9,
    "mean_latency_ms": 98667.0
  },
  {
    "arm": "S",
    "correct": 9,
    "alias_leaks": 0,
    "invalid_identifiers": 0,
    "mean_prompt_tokens": 2661.1,
    "mean_latency_ms": 85384.0
  },
  {
    "arm": "SM",
    "correct": 5,
    "alias_leaks": 1,
    "invalid_identifiers": 3,
    "mean_prompt_tokens": 2324.6,
    "mean_latency_ms": 67409.0
  },
  {
    "arm": "SG",
    "correct": 9,
    "alias_leaks": 0,
    "invalid_identifiers": 0,
    "mean_prompt_tokens": 2618.9,
    "mean_latency_ms": 82079.0
  },
  {
    "arm": "V",
    "correct": 10,
    "alias_leaks": 0,
    "invalid_identifiers": 0,
    "mean_prompt_tokens": 2663.3,
    "mean_latency_ms": 75618.0
  },
  {
    "arm": "VG",
    "correct": 10,
    "alias_leaks": 0,
    "invalid_identifiers": 0,
    "mean_prompt_tokens": 2621.1,
    "mean_latency_ms": 73093.0
  }
]
```
