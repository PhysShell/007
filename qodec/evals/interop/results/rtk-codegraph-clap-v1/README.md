# Canonical Level-1 record: rtk-codegraph-clap-v1

A committed, hash-verified snapshot of one Level-1 run so the numbers in the
docs are reproducible from evidence, not memory. Normal `runs/` are gitignored;
this one record is kept.

## What produced it

- **qodec**: built from this crate (`cargo build --release`); binary hash in
  `doctor.json` / each `meta.json`.
- **RTK**: `0.42.4`, built from the exact tag
  (`cargo install --git https://github.com/rtk-ai/rtk --tag v0.42.4`), binary
  SHA-256 `61683c0a…` (pinned in `snapshots/tools.lock.toml`, verified by
  doctor). Transforms use `rtk pipe --filter {log,grep,git-diff,cargo-test}`;
  the `rtk rg` command-runner is a separate producer.
- **CodeGraph**: `1.4.1` (`@colbymchenry/codegraph`), 100% local.
- **Repo**: clap `v4.5.61`, SHA `8b41d0b8497ccaa0fb0d1d8a51f91ea2f62b3aa8`
  (`snapshots/repos.lock.toml`), CodeGraph index complete, no pending sync.
- **Meter**: o200k (a proxy — Level 2 introduces the served model's tokenizer).

`doctor.py --strict rtk codegraph` passed (`doctor.json`: `strict_ok: true`,
`rtk.sha256_match: true`).

## Contents

```
doctor.json                 strict setup receipt (versions, SHAs, index, explore smoke)
score-rtk.txt               RTK lane score (median by pipeline_id)
score-codegraph.txt         CodeGraph lane score
rtk/ codegraph/             meta.json + metrics.jsonl + cases/<case>/<arm>/…
  cases/*/*/producer.txt        raw producer output
  cases/*/*/transformed.txt     tool-only text (post-rtk-pipe) fed to qodec
  cases/*/*/qodec-envelope.json adapter envelope
  cases/*/*/qodec-content.txt   what the reader receives (artifact or passthrough)
  cases/*/*/cold-prompt.txt     notation brief + artifact (the cold prompt)
  cases/*/*/decoded.txt         round-trip; its hash == producer.txt's hash
  cases/*/*/meta.json           argv/cwd/version/SHA/exit/wall-time + per-file sha256
snapshots/                  tools.lock.toml, repos.lock.toml, the two manifests
SHA256SUMS                  sha256 of every file above
```

Verify integrity: `cd results/rtk-codegraph-clap-v1 && sha256sum -c SHA256SUMS`.

## Results (o200k, clap v4.5.61)

RTK lane — median incremental_qodec_gain by pipeline_id:

| pipeline_id | n | med cold | med warm | note |
|---|--:|--:|--:|---|
| rtk:command:rg | 2 | +18.9% | +39.2% | `rtk rg` (v0.42.4) is raw passthrough; qodec mines the raw rg output |
| rtk:pipe:grep | 1 | −39.3% | +14.1% | grep leaves residual redundancy; brief tax dominates cold |
| rtk:pipe:log | 2 | 0.0% | 0.0% | qodec passthrough — RTK already compressed the log |
| rtk:pipe:git-diff | 1 | 0.0% | 0.0% | passthrough |
| rtk:pipe:cargo-test | 1 | 0.0% | 0.0% | passthrough (RTK cut 1451→16 tok) |

CodeGraph lane:

| pipeline_id | n | med cold | med warm |
|---|--:|--:|--:|
| codegraph:explore | 1 | +6.0% | +12.9% |

Reading it: RTK's `pipe` filters compress hard (upstream −73% log, −99% cargo-test,
−41% git-diff), so qodec correctly **passes through** — the *redundant-layer*
case. Raw tool output that RTK v0.42.4 does not filter (`rtk rg` passthrough) and
CodeGraph `explore` still carry residual redundancy qodec mines (warm +13…+50%).
All roundtrips byte-exact. Cold is lower than warm throughout because the one-shot
notation brief is only amortized in the warm (cached-prefix) case.
