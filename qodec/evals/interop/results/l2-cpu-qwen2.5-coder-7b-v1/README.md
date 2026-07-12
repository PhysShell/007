# Canonical Level-2 record: l2-cpu-qwen2.5-coder-7b-v1

The first **decision-capable** reader run: a competent coder model clears the
raw gate, so qodec gets a real verdict (not INCONCLUSIVE).

## Setup

- **Reader:** Qwen2.5-Coder-7B-Instruct, Q4_K_M GGUF, served on **CPU** via
  `llama-cpp-python` 0.3.34 (OpenAI-compatible). Model GGUF SHA-256
  `509287f78cb4…`, source `Qwen/Qwen2.5-Coder-7B-Instruct-GGUF@main q4_k_m`,
  n_ctx 8192, n_threads 4, n_batch 512 (`meta.json` → `model_identity`).
- **Target tokenizer:** the model's own `tokenizer.json` (`hf:…`, SHA-256
  `c0382117ea32…` + tokenizer_config hash), so aliases/codec acceptance match
  what the model reads.
- **Negotiated contract (`preflight.json` → `effective`):** `stream=False` (the
  endpoint streams content but not usage, so scored requests are non-stream for
  real server tokens), `seed` accepted, **`response_format=json_object`**
  (structured JSON — malformed dropped from the 0.5B's 99 to 15/117).
- **Determinism:** temperature=0, seed=0.

## Result (decision-capable)

```
DO NOT APPLY BLIND QODEC / change notation (general comprehension drop)
```

23 unique questions, 117 executions, 8 repeated (all **stable**, `stability.txt`).
Paired transitions (primary = repeat 0, over unique questions):

| group | n | raw | brief_ret | codec_ret | elig | loss | stbl_loss | resc |
|---|--:|--:|--:|--:|--:|--:|--:|--:|
| all | 23 | 70% | 100% | 69% | 16 | 5 | 5 | 1 |
| facts/counts | 10 | 70% | 100% | 86% | 7 | 1 | 1 | 0 |
| locator | 11 | 82% | 100% | 56% | 9 | 4 | 4 | 1 |
| call_path | 1 | 0% | – | – | 0 | 0 | 0 | 0 |
| actionability | 1 | 0% | – | – | 0 | 0 | 0 | 0 |

Gates: raw competence 70% ≥ 60%, unique eligible overall 16 ≥ 10, unique
eligible locator 9 ≥ 4 → **not INCONCLUSIVE**.

- **Tokenizer parity: exact** — overhead (server − local) = 134 tokens on every
  arm, spread 0.0 → the hf: meter matches the server tokenizer, so token savings
  are trustworthy.
- **qodec does save cost:** encoded+brief vs raw+brief = 2314 vs 2769 server
  prompt tokens, 30.7 s vs 43.2 s latency.
- **…but comprehension drops:** codec_retention 69% overall / 56% locator, **5
  stable codec losses** (all consistent across repeats), 2 alias leaks, +4
  invalid identifiers on the encoded arm.

## Reading the verdict

Blind qodec, under the current notation, makes a 7B coder read the packed
CodeGraph/RTK evidence **worse** — and not only on exact locators. Because the
loss is *general* (overall retention 69%, below the 90% bar) rather than a
locator-only regression with facts/counts preserved, the indicated next step is
**not protected spans** — it is to stop applying blind qodec to this evidence,
or to change the notation. Protected spans remain deferred: they become the next
increment only on a *stable locator regression with facts/counts comprehension
preserved*, which this run does not show (facts/counts retention is 86%, and the
overall drop dominates).

The stable locator losses worth inspecting (`stability.txt`): `def-path`,
`top-symbol` (clap-derive-explore), `file` (rtk-rg-derive, rtk-rg-parser) — all
raw+brief correct → encoded+brief wrong on all three repeats.

## Caveats

- **CPU-served, single quant, one model.** A calibration-grade decision, not a
  universal claim; a GPU-served or larger reader may differ.
- `meta.model_identity.quantization` reads `None` here (the GGUF was renamed, so
  the filename lost `q4_k_m`); the quant is recorded in `model_source`. The
  parser was fixed post-run to also read the source.

## Contents

`meta.json` (identities, determinism, effective contract, per-case tokens),
`preflight.json` (negotiation + structured-json probe + identities),
`records.jsonl` (117 requests/responses/parsed-answers/scores),
`report.txt`, `stability.txt`, `snapshots/reader-tasks.json`, `SHA256SUMS`
(`sha256sum -c SHA256SUMS`).
