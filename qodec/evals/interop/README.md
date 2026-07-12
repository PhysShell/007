# qodec interop bench — layered context optimization, comprehension and actionability

A **separate** evaluation harness (not tool code stuffed into the Rust crate)
for one question:

> qodec does not replace Graphify, CodeGraph, RTK, Headroom or FastContext. It
> may be the last, tokenizer-aware, lossless layer over the context those tools
> already selected or shortened. Is there residual, tokenizer-visible
> redundancy left after each of them — and if qodec removes it, does
> comprehension or actionability survive?

Level 1 (artifacts, no model) is a **real-tool vertical slice**: it runs actual
RTK and CodeGraph against a pinned corpus repo, not stubs waiting for install.

## Producers vs transforms

The load-bearing distinction (and the correction this harness makes over the
first scaffold): Graphify/CodeGraph are **producers** — they create an artifact
from a repo + query. RTK's stdin filters and qodec are **transforms** — they
rewrite text. A case states its whole pipeline, so an adapter can never silently
ignore its input:

```json
{"id": "clap-derive-explore",
 "producer": {"type": "codegraph", "repo": "clap",
              "query": "How does clap parse arguments against a derived command definition?"},
 "transforms": ["qodec"]}

{"id": "build-log-rtk-log",
 "producer": {"type": "fixture", "path": "corpus/build-log.txt"},
 "transforms": [{"type": "rtk", "filter": "log"}, "qodec"]}
```

Producer types: `fixture`, `command`, `rtk-command`, `codegraph`. Transform
types: `rtk` (stdin filter only) and `qodec` (terminal). The arm is named by the
tool feeding qodec: `raw+qodec`, `rtk+qodec`, `codegraph+qodec`.

### The real RTK interface (`rtk pipe --filter`, confirmed against v0.42.4)

RTK 0.42.4 ships two modes, both used here:

- **`rtk pipe --filter <name>`** — "read stdin, apply filter, print filtered
  output (Unix pipe mode)". A genuine `text→text` transform. The harness pins
  the filters it uses: `log`, `grep`, `git-diff`, `cargo-test`.
- **command-runners** (`rtk rg PATTERN PATH`) proxy a native command. These
  cannot transform arbitrary text, so they are a `rtk-command` **producer** with
  a raw baseline (`rg …` without rtk). The manifest parser rejects a native
  command-runner name used as a `pipe` transform.

Provenance matters: the binary is built from the exact upstream tag
(`cargo install --git … --tag v0.42.4`), and `tools.lock.toml` pins its
SHA-256, which `doctor.py` verifies. A finding from that discipline: the
**tagged v0.42.4 `rtk rg` is a raw passthrough** (it does not filter — that was
added on `master` after the tag), so the `rtk-command` lane measures qodec over
raw ripgrep output. We report that rather than swap in a newer, unpinned binary.

## Cold vs warm (honest token metrics)

An encoded artifact is unreadable without the qodec **notation brief** (the
decoder instruction). So every arm reports two figures:

- **cold** — a one-shot message: `tokens(notation brief + artifact)`. A
  passthrough pays only its plaintext (no brief).
- **warm** — the protocol case: `tokens(artifact)` alone, brief amortized in a
  cached prefix.

`incremental_qodec_gain` is reported for both. A combination that wins warm but
loses cold — i.e. only after ignoring the mandatory decoder instruction — shows
up as exactly that, never sold as a flat win. On small one-shot payloads the
brief often makes cold negative while warm is strongly positive; that is the
truth, not a bug.

## Three rungs

**Level 1 — artifact benchmark, no model** (`run.py`, working today over real
RTK + CodeGraph). **Level 3 — agent** is deferred.

**Level 2 — reader comprehension** (`run_reader.py` / `score_reader.py`) runs a
served model; a first CPU calibration run is recorded (see below).

- **Target tokenizer, fail-closed (B1).** Level 2 is only honest if qodec picks
  aliases and accepts codecs under the tokenizer the served model actually reads
  — not an o200k proxy. `qodec --meter hf:/abs/tokenizer.json` is an in-process
  Rust `tokenizers` meter, proven bit-for-bit identical to the Python
  `tokenizers` library (`tests/test_hf_meter_parity.py`). It is **fail-closed**:
  a tokenizer that cannot encode aborts the qodec operation with no token result
  — never a char-count fallback (`test_hf_meter_parity.py::HfMeterFailsClosed`).
  Runs pin the tokenizer path + SHA-256 + size, requested vs reported model, and
  the qodec binary hash.
- **Three arms.** `raw`, `raw+brief` (control: the brief as distraction),
  `encoded+brief`. All three quality-comparable; the qodec effect is isolated by
  **encoded+brief vs raw+brief** (both carry the brief).
- **Conditional paired scoring.** Not a raw-vs-encoded accuracy race — per
  question+repeat it follows raw → raw+brief → encoded+brief and reports
  `raw_competence`, `brief_retention`, `codec_retention`, `codec_losses`,
  `codec_rescues`, broken out by facts/counts / locator / call-path /
  actionability. A weak reader is a valid **calibration reader**: questions it
  cannot answer in the control arms are ineligible and cannot move the verdict.
  Gates → **INCONCLUSIVE** (never PASS on `raw=0/encoded=0`): raw competence
  <60%, or eligible overall <10, or eligible locator <4.
- **Tasks + match modes (B4).** Five real-tool cases, gold grounded byte-for-byte
  in the L1 artifacts, fixed `{facts,files,symbols,call_path,answer}` schema.
  Rule-based, **no LLM judge**: `exact` / `exact-set` (no extra identifiers) /
  `one-of` / `contains-all` / `ordered-path`; exact file paths matched in full
  (no basename fallback); real `call_path` and `actionability` scoring. Integrity
  checks: invalid identifiers (absent from source) and alias leakage (a **whole**
  legend alias — matched as a full string, only those used in the encoded body,
  against structured answer values).
- **Preflight + client (B2, B7).** `preflight.json` records real endpoint
  capabilities (reachability, `/models`, a streaming request's content/TTFT/usage,
  seed acceptance, `stream_options.include_usage` support) and the tokenizer/qodec
  identities before the matrix. `temperature=0` + seed; unsupported seed is
  recorded as the effective determinism contract, not dropped. Token accounting
  shows **real server `prompt_tokens`** per arm (never the raw count in the
  raw+brief row, never the warm artifact as the encoded prompt); warm is an
  explicit amortization estimate. Two passes: everything once, then 3× only for
  flagged questions (malformed JSON, raw/brief disagreement, codec loss, alias
  leak, invalid id).

```bash
export QODEC_READER_URL=http://127.0.0.1:8000/v1
export QODEC_READER_MODEL=<served-model-id>
export QODEC_READER_TOKENIZER=hf:/abs/tokenizer.json     # MUST match the served model
python3 run_reader.py --l1-run results/rtk-codegraph-clap-v1 --name l2
python3 score_reader.py runs-l2/l2
```

Without an endpoint `run_reader.py` exits cleanly (code 3) — it never fabricates
answers. The full pipeline is also verified against a mock endpoint
(`tests/test_reader_mock.py`, `tests/test_reader_scoring.py`).

Decision (only after the raw gate + eligibility pass): blind qodec passes when
codec losses ≈ 0, no locator loss, zero alias leakage, and no rise in invalid
identifiers. A measured locator regression (raw+brief correct → encoded+brief
wrong) with facts/counts preserved is what makes **protected spans** the next
increment; a general comprehension drop means don't apply blind qodec here (or
change the notation), not protected spans.

Committed runs (`results/`): the 0.5B `l2-cpu-qwen0.5b-v1` came back
**INCONCLUSIVE** (raw 16% — the weak-reader guard). The decision-capable
`l2-cpu-qwen2.5-coder-7b-v1` (Qwen2.5-Coder-7B, raw 70%, exact tokenizer parity)
returns **DO NOT APPLY BLIND QODEC / change notation**: encoding saves tokens
(2769→2314) and latency (43→31 s) but drops codec_retention to 69% overall / 56%
locator with 5 stable losses — a *general* comprehension drop, so not a
protected-spans trigger.

## Go / no-go

Median `incremental_qodec_gain` ≥ 10% — reported for cold **and** warm. Quality
delta, invalid-ID rate and latency thresholds need L2/L3. Level-1 verdicts are
token-only: **win** / **marginal** / **loss** / **passthrough**;
harm / redundant / wrong-layer are comprehension verdicts for the model rungs.

## Layout

```
interop/
├── README.md
├── bench/              # importable package (unit-tested)
│   ├── qodec.py        # terminal transform + token meter (encode/decode/count/probe)
│   ├── lockfiles.py    # tools.lock.toml + repos.lock.toml
│   ├── manifest.py     # producer/transform model + validation
│   ├── producers.py    # fixture / command / rtk-command / codegraph
│   ├── transforms.py   # rtk stdin filter, qodec; headroom/fastcontext = unsupported
│   ├── execution.py    # run + capture full provenance
│   ├── artifacts.py    # save + sha256 every artifact
│   ├── metrics.py      # cold/warm token accounting
│   ├── doctor.py       # setup receipt + strict gate
│   └── runner.py       # execute a case's pipeline
├── doctor.py run.py score.py manage.py   # thin CLIs
├── manifests/          # corpus.json, rtk.json, codegraph.json
├── tests/              # unittest: manifest parsing, RTK invocation, receipt validation
├── tools.lock.toml     # pinned tool versions + exact invocations
├── repos.lock.toml     # corpus repos pinned by commit SHA
├── .cache/             # cloned repos + indexes (gitignored, built by manage.py)
└── runs/               # per-run outputs + hashed artifacts (gitignored)
```

## Running Level 1

```bash
cd qodec && cargo build --release              # the bench shells the release binary

# install the real tools (100% local, no API keys)
cargo install --git https://github.com/rtk-ai/rtk     # rtk 0.42.4
npm install -g @colbymchenry/codegraph                # codegraph 1.4.1

cd evals/interop
python3 manage.py sync                         # clone pinned repos + build codegraph index
python3 doctor.py --strict rtk codegraph       # must pass before the tool lanes run
python3 run.py --manifest manifests/rtk.json       --name rtk
python3 run.py --manifest manifests/codegraph.json --name cg
python3 run.py --manifest manifests/corpus.json    --name corpus   # fixtures, no tools
python3 score.py runs/rtk
```

No Python dependencies for Level 1 — the standard library plus the built
binaries. `doctor.py --strict` verifies actual==pinned versions, repo HEAD ==
pinned SHA, and CodeGraph index readiness, recording each check's exact command,
exit code and elapsed time; it exits non-zero on any drift. Binaries resolve via
`RTK_BIN` / `CODEGRAPH_BIN` env or PATH.

Every case/arm persists `producer.txt`, `transformed.txt`, `qodec-envelope.json`,
`qodec-content.txt`, `decoded.txt` and a `meta.json` with argv, cwd, versions,
repo SHA, exit codes, timings, and the SHA-256 + byte size of every artifact.
`decoded.txt`'s hash equalling `producer.txt`'s is the byte-exact roundtrip
proof.

## Not validated (do not treat as working)

- **Headroom** — the earlier adapter used an unverified return contract and
  side-effect flags. `doctor.py` reports it `unsupported`; a case that names it
  gets an explicit `unsupported` arm, not a skip.
- **FastContext** — a served model (OpenAI-compatible endpoint), not a
  `fastcontext.brief()` package. Also `unsupported`.
- **Graphify** — not yet integrated; a later increment.

## Deferred (unchanged)

Protected spans (`--protect …`, the `protected qodec` arm) and Level 2/3. The
CodeGraph lane is where protected mode matters most: explore output is verbatim
source + symbol names + paths, exactly the spans blind mining must not alias.
