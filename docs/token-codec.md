# Token Codec (`qodec`) — lossless, tokenizer-aware context encoding

- **Status:** Experiment (working lab, measured; not wired into `o7`)
- **Code:** `qodec/` (standalone crate, own lockfile; deliberately not part of the `o7` binary)

## Summary

`qodec` is a lab for one question: **how much of an agent's context can be
re-encoded losslessly into fewer tokens, with a deterministic decoder and a
self-describing key?**

It is the "encryption key" version of context compression: the encoded
artifact carries a small legend (the key), the body is written in that
notation, and an exact `decode` inverts it. Unlike semantic compression
(summaries, reducers, repo maps), nothing is lost and nothing is trusted to
the model's paraphrasing — but unlike gzip-in-a-prompt, the alphabet is chosen
*by measuring the live tokenizer*, so the encoded form is genuinely cheaper
for the model to read.

## Where it sits among the sibling proposals

| layer | doc | loss model |
|---|---|---|
| output reducers (msbuild/test/rg) | `agents-outputs-budgeter.md` | lossy summary + raw artifact on disk |
| context briefs via local LLM | `fastcontext.md` | lossy, model-generated |
| **token codec** | this doc | **lossless, deterministic, measured** |

They compose: a reducer decides *what* enters context; the codec re-encodes
whatever enters it. The codec is universal — it knows nothing about MSBuild,
paths, or C#; every win is mined from repetition in the concrete payload.

## Why naive compression fails, and what survives

LLMs read tokens, not bytes. gzip/Huffman output tokenizes as high-entropy
soup (more tokens, not fewer), and a model asked to "decompress" is an
unreliable decoder. Three things survive that critique:

1. **Substitution with a measured alphabet.** Replacing a repeated exact span
   with an alias only helps if `tok(alias) < tok(span)` *under the actual
   tokenizer*, and only if the legend line pays for itself:
   `N·(tok(span) − tok(alias)) > tok(legend line)`. So measure; never assume.
2. **Structural re-encoding.** Uniform JSON repeats its keys N times; a table
   states them once. Consecutive identical lines RLE-fold. Both invert
   deterministically.
3. **Key amortization.** A stable legend can live in a cached prompt prefix
   (CLAUDE.md, system prompt, subagent preamble). Prompt caching bills cached
   prefixes at a fraction of fresh input, so the key is ~free after first
   send; each message pays only the (smaller) body. The bench reports both
   figures: **cold** (key travels in-message) and **warm** (key amortized).

## The container: the key travels with the payload

```text
%q1 mine n=9                  <- codec + params (header)
码=at System.Runtime.CompilerServices.TaskAwaiter.HandleNonSuccessAndDebuggerNotification(Task task)
类=in C:\build\src\Legacy.UI\ViewModels\UserEditorViewModel.cs:line
...                           <- legend lines: alias=exact phrase (the key)
%q1 body                      <- boundary
   at Legacy.UI.ViewModels.UserEditorViewModel.Validate() 类 96
   码
...                           <- body in the compact notation
```

Markers are ASCII on purpose (`%q1` ≈ 2 tokens; pretty brackets like `⟦` cost
3+ under o200k). Decoding is a total function: parse header → dispatch codec →
substitute back in reverse commit order. `encode` falls back to a `raw`
container whenever the measured artifact fails to beat the original, so the
pipeline can be applied blindly.

## Codecs

- **`mine`** — the core. LZ78 in spirit, but the cost function is the live
  tokenizer: repeated word-boundary spans are ranked, the top candidates are
  *exactly measured* (re-tokenize the actual replacement, subtract the legend
  line), and only positive-gain entries commit. Aliases come from a probed
  pool; chars are provably absent from the input, so decode is collision-free.
- **`deep`** — `mine` with the full-strength candidate miner: word candidates
  ∪ suffix-automaton candidates (`sam.rs`, every repeated substring at any
  boundary), half the probe budget each. Same container, same decode;
  ~15–20× the encode CPU — the BWT-lineage ratio-vs-time trade, live.
- **`fold`** — RLE for consecutive identical lines (`%q1 xN`), CRLF-safe,
  with escaping for hostile `%q1`-shaped input lines.
- **`toon`** — uniform JSON array → keys-once table with a probed separator;
  roundtrip is semantic (Value-equal canonical JSON), scope deliberately
  narrow (top-level array, identical flat keys) with honest fallback.
- **`squeeze`** — `toon` (JSON) or `fold` (text), then the better of the two
  miners over the result.

## Measured results (o200k, auto alphabet, corpus in `qodec/corpus/`)

With prefix-aware mining (see "BWT lineage" below) and nested dictionary
entries (a later phrase may contain an earlier alias; reverse-order decode
expands both):

| sample | best codec | cold Δ | warm Δ | roundtrip |
|---|---|---:|---:|---|
| build-log.txt (msbuild, repeated warnings) | squeeze | **+46.4%** | +71.6% (mine) | byte |
| stacktrace.txt (.NET async spam) | mine | +18.8% | **+54.9%** | byte |
| findings.json (12 uniform findings) | squeeze | +32.6% | +49.2% | semantic |
| rg-output.txt | mine | +27.0% | +52.0% | byte |
| git-diff.txt | mine | +8.1% | +25.7% | byte |
| prose.md (unique text — control) | any | −4.2% (raw fallback) | 0.0% | byte |

Cross-check under `cl100k` reproduces the ordering, so the effect is not one
tokenizer's quirk.

### Real payloads (gathered from the sibling repos and a live conversation)

| payload | tok in | cold Δ | warm Δ |
|---|---:|---:|---:|
| `find` file listing over Own.NET (200 paths) | 3070 | **+44.9%** | +49.4% |
| grep over Own.NET `rust/` (150 hits) | 3089 | +40.8% | +46.5% |
| verbose cargo build log (this crate) | 1323 | +20.6% | **+50.5%** |
| ChatGPT conversation transcript (30 KB slice) | 5861 | +16.1% | +38.4% |
| Own.NET `git diff --stat` (15 commits) | 1436 | +6.6% | +11.8% |
| Own.NET `git diff` (docs-heavy, mostly prose) | 4644 | +2.4% | +5.9% |
| OwnAudit oracle findings.json (tiny, nested) | 294 | raw fallback | 0.0% |

Pattern: tool output and transcripts (what agents actually exchange) sit in
the +20–50% band; unique prose and tiny payloads honestly fall back.

**The alphabet finding (corrected after PR #26 review).** Single CJK glyphs
(1 token under o200k) beat sigil-indexed aliases (`§05` — 2 tokens) on every
sample, but the gap is a steady few points, not the dramatic collapse first
measured: build log +46.4% (glyph-led auto) vs +43.6% (sigil-only) cold,
rg-output +27.0% vs +17.5%. The original "sigils barely work" numbers were an
artifact of a char-reservation bug that capped sigil mode at one dictionary
entry (caught by CodeRabbit). The refined finding: the alias alphabet is a
per-occurrence multiplier — 1-token aliases are strictly better and must be
*probed per tokenizer* (`码` is 1 token under o200k, but `堆` is 2;
`qodec aliases` shows the live table) — while fixed-width sigils are the
correct overflow once cheap glyphs run out.

Reading the two columns:

- **cold** is what a one-shot message saves. Only heavy repetition (logs,
  traces, uniform JSON) clears the ~13-token container tax plus legend.
- **warm** is the protocol case: orchestrator ↔ subagent traffic where the
  legend sits in the cached preamble and every message pays body-only.
  +30–50% on exactly the payloads agents exchange most (traces, diffs,
  findings, tool output).

## Comprehension A/B (`qodec ab`) — first measured result

The experiment the whole lab was waiting for: does a fresh model context
answer questions about an *encoded* payload as well as about the raw one?
`qodec ab emit` builds the paired prompts (payload + questions from
`qodec/ab/*.json`); `qodec ab grade` scores answers by distinctive accept
substrings — the model invocation stays outside, per the agent-language
discipline.

First run (2026-07-08, full record in `qodec/ab/results/`): 8 fresh-context
Claude subagents, 4 payloads × raw/encoded (`deep`, 12–16 legend entries),
6 questions each — **24/24 raw, 24/24 encoded**, answers near
byte-identical, including counting `suspect_fp=true` values scattered
across nested aliases in the findings table. Two honest observations ride
along: encoded QA cost ~3–5× the wall time on alias-dense payloads (the
model decodes in its head — on reasoning models some input savings shifts
into thinking tokens), and the scope is smoke-level (one model family, small
payloads, retrieval questions). The gate is open, not proven.

## What this deliberately does not claim

- **Comprehension is proven only at smoke level.** The A/B above covers
  retrieval questions over small payloads in one model family. Long-context
  behavior, deep reasoning over decoded content, and cheaper reader models
  are untested — the `o7 judge` FP-triage agreement run is the next rung.
- **Claude's tokenizer is not public.** o200k/cl100k are proxies; absolute
  numbers will shift, orderings should hold. A future meter can wrap an
  API-side count endpoint behind the same `TokenMeter` trait.
- **Code that will be edited must never travel mined.** An agent that pastes
  `码` into a patch ships garbage. The codec is for *evidence* payloads
  (logs, traces, listings, findings, briefs), not for spans the agent must
  reproduce exactly.

## BWT lineage — where the remaining headroom lives

Byte-level compressors put a ceiling on how much repetition exists at all:
on the real payloads above, `bzip2 -9` removes 65–81% of *bytes* while the
miner extracts 5–50% of *tokens*. The ceiling is not reachable in token
space — BPE already ate the easy entropy, the model must still read the
output, and every alias occurrence costs ≥1 token — but the *gap pattern*
says where to dig.

The BWT insight (group by context; repetition ignores human-visible
boundaries) transplants to token space as: **candidate discovery must not be
limited to word boundaries.** The ladder, as climbed:

1. **Separator prefixes** (`segment_prefixes`): prefixes *inside* words —
   `rust/src/`, `Legacy.UI.`, namespace chains. Together with nested
   dictionary entries this took the `find` listing from raw-fallback to
   +44.9% and real grep output from +5.6% to +40.8% cold.
2. **Suffix automaton** (`sam.rs`, the `deep` codec): every repeated
   substring, any boundary, O(n) states. Lab lesson learned the honest way:
   *pure* SAM ranking drowns the probe budget in nested variants of one
   giant repeat (stack traces fell from +18.8% to +6.8% cold; a 30 KB
   transcript fell back entirely). The fix is diversity, not depth — union
   the word-tally and SAM candidate families, half the budget each. Hybrid
   `deep` then wins or ties everywhere:

   | payload | `mine` cold | `deep` cold | `deep` warm | bzip2 byte ceiling |
   |---|---:|---:|---:|---:|
   | grep over Own.NET | +40.8% | **+51.3%** | +60.2% | 79% |
   | `find` listing | +44.9% | **+48.6%** | +54.5% | 81% |
   | cargo build log | +20.6% | **+36.1%** | +67.4% | 65% |
   | findings.json (synthetic) | +3.0% | **+34.4%** | +66.0% | — |
   | git-diff (synthetic) | +8.1% | **+17.9%** | +40.7% | — |

   JSON is the sleeper win: `","file":"src/` -style repeats straddle every
   word boundary, invisible to the word miner, trivial for the automaton.
3. What does *not* transplant: BWT's entropy-coding half (MTF/RLE/Huffman
   over the transform). Its output is byte soup the model cannot read; that
   family is transport-layer compression, where the decoder is a machine and
   prompt tokens are unaffected. The boundary rule stays: **the model reads
   substitution + structure; machines read entropy codes.**

## Perplexity gate (`qodec ppl`) — compression = prediction, inverted

The nncp/LLMZip end of the literature uses an LM's next-token predictions to
compress; flipped, the same quantity is a *comprehension proxy*: if a small
local LM finds the encoded body barely harder to predict than the raw text,
a frontier model will very likely read it fine.

`qodec ppl -i payload.txt --codec deep --url http://127.0.0.1:8000/v1/completions`
encodes the payload, scores raw and encoded under an OpenAI-compatible
legacy-completions endpoint (`echo=true, max_tokens=0, logprobs=0` — vLLM
implements this contract), and reports the perplexity ratio with a
three-band verdict (≤1.5 likely-readable / ≤3 borderline / else
model-hostile). This is where FastContext (`docs/fastcontext.md`) plugs in:
served locally, it makes the gate free, and only borderline artifacts spend
real `o7 judge` runs. The bands are heuristic seeds — calibrate them against
actual judge-run agreement before trusting them as a gate.

## Keeping the model unconfused (design rules for live use)

1. **One stable notation, taught once.** The container grammar and alias
   style live in the cached preamble with 2–3 worked decode examples;
   per-message novelty is limited to legend *entries*, never new syntax.
2. **Mnemonic aliases.** The glyph pool is not random: `警`=warning,
   `错`=error, `码`=code, `路`=path. Assigning meaning-adjacent glyphs to
   phrases (warning lines get `警`…) turns the alias from an opaque symbol
   into a hint. Unmeasured yet — candidate for the A/B.
3. **Never encode what the model must reproduce.** IDs, hashes, code spans
   to be edited/quoted travel raw. The codec is for evidence payloads.
4. **One-hop indirection only.** Alias → phrase, never alias → alias
   (enforced by the reserved-chars design).
5. **Read-side first.** The model only *reads* the notation; it never has
   to write it until the read side is proven.
6. **Cap the legend.** 64 entries is a lab bound; live use should probe the
   savings-vs-entries curve and likely stop near 16–24 per message.

## Next steps (in rough order of information gained per effort)

Done from previous editions of this list: suffix-automaton mining (`deep`),
the perplexity pre-gate (`qodec ppl`), and the first comprehension A/B
(`qodec ab`, 24/24 = 24/24 — section above).

1. **Judge-grade A/B** — encoded vs raw payloads through `o7 judge` on the
   FP-triage rubric; measure *verdict agreement* on a real task, not just
   retrieval QA. `qodec ppl` pre-filters; A/B results calibrate its bands.
2. **Wire as an output filter** — `o7` already harvests `agent.stdout` and
   feeds judge prompts; `qodec encode --codec squeeze` is a one-line insert
   at the prompt-assembly seam (and a PostToolUse hook candidate in Claude
   Code, where tool output is the dominant repetitive payload).
3. **Session dictionaries** — persist a per-repo legend (top paths, type
   names, frame prefixes) into the cached preamble once, so *every* message
   is warm-path; `mine` then only handles payload-local repetition.
4. **Output-side notation** — the reverse direction: let the subagent *reply*
   in the legend's notation and expand deterministically outside the model.
   Output tokens cost ~5× input; this is where the same trick pays most, and
   nothing about the container is input-specific.
