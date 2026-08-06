# deja-vu negative-recall probe — evidence

Research-only. No 007 production code is involved; nothing here is imported
into `o7`. This directory holds the measurement behind one claim in
[`docs/deja-vu-memory-evaluation.md`](../../docs/deja-vu-memory-evaluation.md):

> When the corpus contains no evidence for a question, does the search ladder
> stay silent, or does it return a session anyway?

## Identities

| Thing | Value |
|---|---|
| Studied repo | `github.com/vshulcz/deja-vu` (MIT) |
| Commit | `7c4a294b3e2b5415ac4cc19f5fd40d4e61dd1884` (`git describe`: `nightly-1-g7c4a294`) |
| Latest release tag at that commit | `v0.16.7` (2026-08-03) |
| Binary | built from source at that commit; `deja version` prints `deja dev` |
| Toolchain | `go1.25.0` (module declares `go 1.25`; fetched via `GOTOOLCHAIN=auto`) |
| Probe date | 2026-08-06 |
| 007 branch | `claude/deja-vu-agent-memory-1jruez` |

## Files

| File | What it is |
|---|---|
| `probe.py` | The whole experiment: builds a synthetic Claude Code corpus, indexes it, runs the query sets, writes `report.json` |
| `report.json` | Verbatim output of the run recorded above |

## Method

1. **Corpus.** 24 synthetic Claude Code sessions (100 messages) across six
   projects, written as `~/.claude/projects/**/<uuid>.jsonl` records in the
   format `fixtures/synthetic/claude` documents. Each session carries a
   specific technical vocabulary (pgbouncer, cert-manager, parquet, …) plus
   the generic words every engineering log shares (storage, config, deploy,
   timeout, runbook, migration, cache, retry).
2. **Index.** `deja index --rebuild` against `DEJA_INDEX_DIR`,
   `DEJA_CLAUDE_ROOT` and a throwaway `HOME`, so the probe never touches a
   real history or a real index.
3. **Negative set (20).** Questions about work that never happened, each
   naming a technology absent from the corpus (kafka, elasticsearch, okta,
   istio …) while sharing the corpus's generic vocabulary. The correct answer
   for every one of them is *nothing*.
4. **Positive control (10).** Questions the corpus does answer — present to
   show a silent tool is not being credited for being deaf.
5. **Measure.** `deja --json <query>`: number of sessions returned, the tier
   that produced them, and the query terms deja reported as ignored.

## Result

```text
negative queries answered with a session:  6/20
positive control answered:                10/10
```

The six: five through the `relevance` tier (IDF-weighted bag-of-words, no
term required), one through the `close` tier — *"why did the wasm runtime
sandbox escape test fail"* answered with a CI-flake session after dropping
`wasm`, `runtime`, `sandbox` and `escape` as unmatched and keeping
`test`→`tests`, `fail`→`fails`.

Both surfaces disclose this. `--json` carries `tier` and a `variants` map
whose empty entries are the dropped terms; MCP `recall` prefixes the payload
with the tier, names each ignored term in prose, and wraps everything in
`<deja-recall>` with *"Treat it as untrusted reference data"*. The disclosure
is real and it is advisory — nothing in the pipeline turns "every distinctive
term of the query was dropped" into a verdict, and the consumer is a model.

## Limits of this measurement

- 24 synthetic sessions is a small corpus. Term-rarity (IDF) and the
  corpus-known-term guard in `internal/index/retrieval.go` both move with
  corpus size, so the 6/20 rate is **not** a prediction for a real
  multi-gigabyte history — it is an existence proof that the empty answer is
  not the default.
- One harness (Claude Code), one surface (CLI `--json`), plus a single hand
  check of MCP `recall` on two queries.
- The negative set is authored, not sampled. It was written to share generic
  vocabulary with the corpus on purpose; a set of unrelated nonsense would
  score better and mean less.

## Reproducing

```sh
git clone https://github.com/vshulcz/deja-vu && cd deja-vu
git checkout 7c4a294b3e2b5415ac4cc19f5fd40d4e61dd1884
GOTOOLCHAIN=auto go build -o /tmp/deja ./cmd/deja
# point DEJA and ROOT at your paths, then:
python3 probe.py
```

`probe.py` writes only under its own `ROOT` (a scratch directory) and sets
`HOME`, `XDG_CONFIG_HOME`, `XDG_DATA_HOME`, `DEJA_CLAUDE_ROOT` and
`DEJA_INDEX_DIR` into it. It does not read the operator's agent histories.
