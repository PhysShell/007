# qodec interop bench — layered context optimization, comprehension and actionability

A **separate** evaluation harness (not tool code stuffed into the Rust crate)
for one question:

> qodec does not replace Graphify, CodeGraph, RTK, Headroom or FastContext. It
> may be the last, tokenizer-aware, lossless layer over the context those tools
> already selected or shortened. Is there residual, tokenizer-visible
> redundancy left after each of them — and if qodec removes it, does
> comprehension or actionability survive?

The comparisons that matter are **compositions**, never `Graphify vs qodec`
(different floors). Each optimizer reduces something; qodec's reasonable place
is *after* it:

| tool | what it shortens | qodec's place |
|---|---|---|
| Graphify | file reads, via graph retrieval | after query/path/explain |
| CodeGraph | tool calls + code found | after `codegraph explore` |
| RTK | raw command stdout | command → RTK → qodec |
| Headroom | whole prompt, tool output, RAG, history | Headroom → qodec (protect control IDs) |
| FastContext | selected data → brief | brief plain + evidence via qodec |

## Three rungs (do them in order)

**Level 1 — artifact benchmark, no model** (`run.py`, working today). For each
producer artifact measure `A`, `qodec(A)`, `tool(A)`, `qodec(tool(A))` on tokens
and time only. Headline:

    incremental_qodec_gain = 1 - tokens(tool_then_qodec) / tokens(tool_only)

Answers the clean question — *did tokenizer-visible redundancy survive the
upstream optimizer?* — before a single model call is spent. Also records: bytes,
tool time, qodec encode/decode time, byte-exact roundtrip, chosen codec,
fallback fraction, cold cost. (Warm cost — the amortized-key figure — is a
follow-up: it needs the adapter envelope to expose the container's key
overhead.)

**Level 2 — reader benchmark, local model** (not yet built). Same data, same
model, same prompt; the only difference is raw vs qodec. Four task types — fact
retrieval, exact locator, relationship tracing, actionability — answered in a
fixed JSON shape so scoring is deterministic (rule-based, never an LLM judging
its own vibe).

**Level 3 — agent benchmark** (not yet built). The model picks tools, makes
several calls, gets output raw or through qodec, answers or patches; patches are
checked by tests. Only after L1+L2, so a quality drop is attributable to the
codec and not to the retriever, the model, or an agent that decided to explore
`node_modules`.

## Arms

Every payload runs through up to three arms:

- **no qodec** — the producer output as-is (the baseline).
- **blind qodec** — `qodec encode --json --passthrough-on-no-gain`. Applied
  without knowing the payload's structure; passthrough guarantees it never adds
  the container tax to already-dense output.
- **protected qodec** — *(not yet implemented; see "Next rung")* mining that
  refuses to touch code blocks, file paths, symbol names, finding IDs,
  tool-call arguments, Headroom retrieval handles, or JSON control fields. The
  hypothesis is that this is the production variant for CodeGraph and Headroom.

## Go / no-go for a combination

- median `incremental_qodec_gain` ≥ **10%**
- quality delta ≥ **−1 percentage point** *(needs L2/L3)*
- no rise in invalid exact IDs / paths *(needs L2/L3)*
- end-to-end latency not disproportionately worse *(the 24/24 A/B kept quality
  but alias-dense payloads cost 3–5× model wall time — a token win that
  quadruples latency is not a win)*

Classification: **win** (extra compression, no quality loss) · **neutral**
(qodec passes through) · **harm** (fewer tokens, worse comprehension) ·
**redundant** (optimizer already removed the redundancy) · **wrong layer**
(qodec applied to code/control data). Only *win / marginal / passthrough* are
decidable at Level 1.

## Layout

```
interop/
├── README.md          # this file — the plan of record
├── pyproject.toml     # Level 2/3 model deps (tokenizers/transformers); L1 is stdlib-only
├── tools.lock.toml    # pinned optimizer versions + exact invocations
├── repos.lock.toml    # neutral corpus repos, pinned by commit SHA
├── tasks/             # gold localization/patch tasks (L3) — TODO
├── prompts/           # reader prompt templates (L2) — TODO
├── adapters/          # one module per optimizer + the qodec adapter (always present)
├── doctor.py          # setup receipt: qodec healthy? which optimizers present?
├── run.py             # Level 1 runner
├── score.py           # Level 1 → go/no-go table
└── runs/              # per-run outputs (gitignored)
```

## Running Level 1

```bash
cd qodec && cargo build --release        # the bench shells out to the release binary
cd evals/interop
python3 doctor.py                        # confirm qodec is healthy
python3 run.py --name my-run             # corpus cases; tool lanes skip if tools absent
python3 score.py runs/my-run
```

No Python dependencies for Level 1 — the standard library plus the built
`qodec` binary. `doctor.py` reports which optimizers are installed; absent ones
skip their lanes and are named in the score, never silently counted as passing.

Custom cases: `python3 run.py --manifest cases.json`, where `cases.json` is
`{"cases": [{"id","lane","kind","path","optimizers":[...],"json":false}, ...]}`
with `path` relative to the crate root.

## Corpus

First (neutral) selection, from CodeGraph's public corpus + Graphify's
home-field ERPNext — pinned in `repos.lock.toml`. The MVP corpus lane uses the
crate's own `qodec/corpus/*` as stand-in producer output (msbuild log, .NET
stack trace, ripgrep hits, git diff, uniform findings, unique-prose control) so
Level 1 is runnable with zero external setup. Real optimizer lanes attach once
the tools in `tools.lock.toml` are installed.

## Next rung: protected spans (blocks the third arm)

Blind mining must never alias spans the model has to reproduce byte-for-byte.
The qodec side needs:

```
--protect markdown-code
--protect-json-pointer /tool_call/id
--protect-regex 'headroom:[A-Za-z0-9_-]+'
--protect-regex '(?:src|tests)/\S+'
```

with the semantics: protected byte ranges are excluded from candidate discovery
**and** from substitution, the surrounding text mines normally, the protected
content stays verbatim in place, and byte-exact roundtrip is preserved (decode
is unchanged — nothing inside a protected span was ever substituted). This is a
localized but careful change to the miner (`mine.rs`), so it is its own
increment; the `protected qodec` arm here is wired to report *not available*
until it lands.

The dependency this harness already unblocked is the adapter/passthrough
contract (`qodec/src/adapter.rs`, `encode --json --passthrough-on-no-gain`) —
without it, blind application taxes every dense payload it can no longer help.
