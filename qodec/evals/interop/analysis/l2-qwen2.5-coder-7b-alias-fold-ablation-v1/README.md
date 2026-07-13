# Level-2 alias × structural ablation — Qwen2.5-Coder-7B

Same model/tokenizer/determinism as the canonical record (model `qwen2.5-coder-7b-instruct`, qodec `e82d5f3e3154`). Six arms in one run.

Arms: **R** raw+brief · **I** identity (framing only) · **M** alias-only · **F** structural-only · **MF** squeeze · **GF** guarded squeeze.

**Conclusion: candidate policy for full 23-question rerun: F, GF**

## per-question truth tables

### build-log-rtk-log / n-warnings — LOSS
| arm | correct | stable | format | leaks | invalid | ptok |
|-----|---------|--------|--------|-------|---------|------|
| R | ✓ | stable | ok | 0 | 0 | 583 |
| I | ✓ | stable | ok | 0 | 0 | 593 |
| M | ✓ | stable | ok | 0 | 0 | 581 |
| F | ✓ | stable | ok | 0 | 0 | 593 |
| MF | ✗ | stable | ok | 0 | 0 | 582 |
| GF | ✓ | stable | ok | 0 | 0 | 593 |

verdict: alias × structural interaction (I/M/F pass, MF fail); lexical aliasing implicated (MF fail, GF pass)

### clap-derive-explore / def-path — LOSS
| arm | correct | stable | format | leaks | invalid | ptok |
|-----|---------|--------|--------|-------|---------|------|
| R | ✓ | stable | ok | 0 | 0 | 4775 |
| I | ✓ | stable | ok | 0 | 0 | 4785 |
| M | ✗ | stable | ok | 0 | 1 | 4511 |
| F | ✓ | stable | ok | 0 | 0 | 4785 |
| MF | ✗ | stable | ok | 0 | 1 | 4230 |
| GF | ✓ | stable | ok | 0 | 0 | 4684 |

verdict: alias main effect (I pass, M fail); lexical aliasing implicated (MF fail, GF pass)

### clap-derive-explore / top-symbol — LOSS
| arm | correct | stable | format | leaks | invalid | ptok |
|-----|---------|--------|--------|-------|---------|------|
| R | ✓ | stable | ok | 0 | 0 | 4773 |
| I | ✓ | stable | ok | 0 | 0 | 4783 |
| M | ✓ | stable | ok | 0 | 0 | 4509 |
| F | ✓ | stable | ok | 0 | 0 | 4783 |
| MF | ✗ | stable | ok | 1 | 1 | 4228 |
| GF | ✓ | stable | ok | 0 | 0 | 4682 |

verdict: alias × structural interaction (I/M/F pass, MF fail); lexical aliasing implicated (MF fail, GF pass)

### rtk-rg-derive-clap / file — LOSS
| arm | correct | stable | format | leaks | invalid | ptok |
|-----|---------|--------|--------|-------|---------|------|
| R | ✓ | stable | ok | 0 | 0 | 3949 |
| I | ✓ | stable | ok | 0 | 0 | 3959 |
| M | ✓ | stable | ok | 0 | 0 | 2877 |
| F | ✓ | stable | ok | 0 | 0 | 3199 |
| MF | ✗ | stable | ok | 0 | 1 | 2315 |
| GF | ✓ | stable | ok | 0 | 0 | 3181 |

verdict: alias × structural interaction (I/M/F pass, MF fail); lexical aliasing implicated (MF fail, GF pass)

### rtk-rg-parser-clap / file — LOSS
| arm | correct | stable | format | leaks | invalid | ptok |
|-----|---------|--------|--------|-------|---------|------|
| R | ✓ | stable | ok | 0 | 0 | 1244 |
| I | ✓ | stable | ok | 0 | 0 | 1254 |
| M | ✗ | stable | ok | 0 | 0 | 1087 |
| F | ✓ | stable | ok | 0 | 0 | 1116 |
| MF | ✗ | stable | ok | 0 | 0 | 1016 |
| GF | ✓ | stable | ok | 0 | 0 | 1116 |

verdict: alias main effect (I pass, M fail); lexical aliasing implicated (MF fail, GF pass)

### build-log-rtk-log / n-errors — control
| arm | correct | stable | format | leaks | invalid | ptok |
|-----|---------|--------|--------|-------|---------|------|
| R | ✓ | n=1 | ok | 0 | 0 | 583 |
| I | ✓ | n=1 | ok | 0 | 0 | 593 |
| M | ✓ | n=1 | ok | 0 | 0 | 581 |
| F | ✓ | n=1 | ok | 0 | 0 | 593 |
| MF | ✓ | n=1 | ok | 0 | 0 | 582 |
| GF | ✓ | n=1 | ok | 0 | 0 | 593 |

### clap-derive-explore / trait — control
| arm | correct | stable | format | leaks | invalid | ptok |
|-----|---------|--------|--------|-------|---------|------|
| R | ✓ | n=1 | ok | 0 | 0 | 4775 |
| I | ✓ | n=1 | ok | 0 | 0 | 4785 |
| M | ✓ | n=1 | ok | 0 | 0 | 4511 |
| F | ✓ | n=1 | ok | 0 | 0 | 4785 |
| MF | ✓ | n=1 | ok | 0 | 0 | 4230 |
| GF | ✓ | n=1 | ok | 0 | 0 | 4684 |

### clap-derive-explore / trait-path — control
| arm | correct | stable | format | leaks | invalid | ptok |
|-----|---------|--------|--------|-------|---------|------|
| R | ✓ | n=1 | ok | 0 | 0 | 4779 |
| I | ✓ | n=1 | ok | 0 | 0 | 4789 |
| M | ✓ | n=1 | ok | 0 | 0 | 4515 |
| F | ✓ | n=1 | ok | 0 | 0 | 4789 |
| MF | ✓ | n=1 | ok | 0 | 0 | 4234 |
| GF | ✓ | n=1 | ok | 0 | 0 | 4688 |

### rtk-rg-parser-clap / symbol — control (weak match)
| arm | correct | stable | format | leaks | invalid | ptok |
|-----|---------|--------|--------|-------|---------|------|
| R | ✓ | n=1 | ok | 0 | 0 | 1239 |
| I | ✓ | n=1 | ok | 0 | 0 | 1249 |
| M | ✓ | n=1 | ok | 0 | 0 | 1082 |
| F | ✓ | n=1 | ok | 0 | 0 | 1111 |
| MF | ✓ | n=1 | ok | 0 | 0 | 1011 |
| GF | ✓ | n=1 | ok | 0 | 0 | 1111 |

### rg-output-rtk-grep / method — control (weak match)
| arm | correct | stable | format | leaks | invalid | ptok |
|-----|---------|--------|--------|-------|---------|------|
| R | ✓ | n=1 | ok | 0 | 0 | 869 |
| I | ✓ | n=1 | ok | 0 | 0 | 879 |
| M | ✓ | n=1 | ok | 0 | 0 | 837 |
| F | ✓ | n=1 | ok | 0 | 0 | 879 |
| MF | ✓ | n=1 | ok | 0 | 0 | 818 |
| GF | ✓ | n=1 | ok | 0 | 0 | 879 |

## candidate-policy gate (advance to full rerun?)

| arm | rescued | control regressions | leaks | invalid Δ vs R | tok savings vs R | advances |
|-----|---------|---------------------|-------|----------------|------------------|----------|
| I | 5/5 | 0 | 0 | 0 | -100 | no |
| M | 3/5 | 0 | 0 | 1 | 2478 | no |
| F | 5/5 | 0 | 0 | 0 | 936 | YES |
| MF | 0/5 | 0 | 1 | 3 | 4323 | no |
| GF | 5/5 | 0 | 0 | 0 | 1358 | YES |

## Pareto (quality → integrity → tokens → latency)

ranked: GF > F > R > I > M > MF

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
    "arm": "GF",
    "correct": 10,
    "alias_leaks": 0,
    "invalid_identifiers": 0,
    "mean_prompt_tokens": 2621.1,
    "mean_latency_ms": 64495.0
  }
]
```
