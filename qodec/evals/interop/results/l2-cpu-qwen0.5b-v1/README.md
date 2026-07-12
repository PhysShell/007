# Canonical Level-2 record: l2-cpu-qwen0.5b-v1 (CPU calibration)

The first real reader run — a **calibration** run, not a quality publication.
It exists to exercise the whole Level-2 path end-to-end against a served model
and to prove the scorer withholds judgement when the reader is too weak.

## Setup

- **Reader:** Qwen2.5-0.5B-Instruct (Q4_K_M GGUF) served on **CPU** via
  `llama-cpp-python` (OpenAI-compatible). No GPU in this environment.
- **Target tokenizer:** the model's own `tokenizer.json`
  (`hf:…`, SHA-256 `c0382117ea32…`, recorded in `meta.json`). The encoded arm is
  re-encoded under it, so aliases/codec acceptance match what the model reads.
- **qodec:** binary hash in `meta.json`. **Determinism:** temperature=0, seed=0
  (seed accepted by the endpoint).
- **Endpoint capability (preflight.json):** streams content but **not** usage —
  so scored requests are non-streaming (real server `prompt_tokens`), and TTFT is
  the preflight streaming sample (~202 ms).

## Result

```
INCONCLUSIVE: reader too weak for this task set (raw competence 16% < 60%).
```

Paired transitions (raw → raw+brief → encoded+brief), 61 complete pairs:

| group | n | raw | brief_ret | codec_ret | elig | loss | resc |
|---|--:|--:|--:|--:|--:|--:|--:|
| all | 61 | 16% | 70% | 54% | 13 | 6 | 7 |
| facts/counts | 30 | 20% | 50% | 100% | 3 | 0 | 6 |
| locator | 25 | 16% | 100% | 40% | 10 | 6 | 1 |
| call_path | 3 | 0% | – | – | 0 | 0 | 0 |
| actionability | 3 | 0% | – | – | 0 | 0 | 0 |

Integrity: alias leaks (encoded) = 9; invalid ids raw+brief = 6, encoded = 30;
malformed JSON = 99 (drove 19 flagged questions into the 3× repeat pass).

## What this run *does* establish

This is not a qodec verdict — the raw gate (60%) fails, so the codec comparison
is withheld exactly as designed. What it validates is the harness under a real
model:

- **The weak-reader guard works.** raw competence 16% → INCONCLUSIVE, never a
  false "blind qodec passes" on near-zero scores.
- **Real server token accounting.** The table shows local content tokens vs the
  endpoint's actual `prompt_tokens` per arm (raw+brief shows its own content, not
  the raw count; warm is a labelled amortization estimate). Encoding cuts the
  server prompt on the large cases — rtk-rg-derive raw+brief 3947 → encoded+brief
  2313, clap-derive 4775 → 4230 server tokens.
- **The integrity checks fire.** The alias detector caught 9 whole-alias leaks;
  the invalid-identifier check flagged hallucinated names; malformed JSON is
  counted, not silently scored as wrong.

## Reproduce

```bash
# serve the model (CPU)
python3 -m llama_cpp.server --model qwen2.5-0.5b-instruct-q4_k_m.gguf \
  --host 127.0.0.1 --port 8080 --n_ctx 8192 --model_alias qwen2.5-0.5b-instruct
# run
export QODEC_READER_URL=http://127.0.0.1:8080/v1
export QODEC_READER_MODEL=qwen2.5-0.5b-instruct
export QODEC_READER_TOKENIZER=hf:/abs/qwen2.5-0.5b/tokenizer.json
export QODEC_READER_MAX_TOKENS=128
python3 run_reader.py --l1-run results/rtk-codegraph-clap-v1 --name l2
python3 score_reader.py runs-l2/l2
```

## Contents

`meta.json` (identities + determinism + per-case tokens), `preflight.json`,
`records.jsonl` (every request/response/parsed-answer/score, 183 answers),
`report.txt` (the score output above), `snapshots/reader-tasks.json`,
`SHA256SUMS` (`sha256sum -c SHA256SUMS` to verify).

## Next

A stronger reader (a 7–30B coder, or GLM-5.2 on the contested cases) is needed
to move past INCONCLUSIVE. Protected spans stay deferred until a real run shows a
locator regression (raw+brief locator correct → encoded+brief wrong) with
facts/counts comprehension preserved — which requires a reader that clears the
raw gate first.
