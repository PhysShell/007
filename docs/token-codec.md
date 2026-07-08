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
- **`fold`** — RLE for consecutive identical lines (`%q1 xN`), CRLF-safe,
  with escaping for hostile `%q1`-shaped input lines.
- **`toon`** — uniform JSON array → keys-once table with a probed separator;
  roundtrip is semantic (Value-equal canonical JSON), scope deliberately
  narrow (top-level array, identical flat keys) with honest fallback.
- **`squeeze`** — `toon` (JSON) or `fold` (text), then `mine` over the result.

## Measured results (o200k, auto alphabet, corpus in `qodec/corpus/`)

| sample | best codec | cold Δ | warm Δ | roundtrip |
|---|---|---:|---:|---|
| build-log.txt (msbuild, repeated warnings) | squeeze | **+40.4%** | +52.5% | byte |
| stacktrace.txt (.NET async spam) | mine | +16.9% | **+47.5%** | byte |
| findings.json (12 uniform findings) | squeeze | +24.6% | +37.8% | semantic |
| git-diff.txt | mine | +7.3% | +32.6% | byte |
| rg-output.txt | mine | +4.5% | +15.2% | byte |
| prose.md (unique text — control) | any | −4.2% (raw fallback) | 0.0% | byte |

Cross-check under `cl100k` reproduces the ordering (squeeze on build log
+39.1% cold / +51.3% warm), so the effect is not one tokenizer's quirk.

**The alphabet finding.** The same bench with `--alphabet sigil` (ASCII-style
`§0`, `§1` — 2 tokens each under o200k) collapses the wins: stacktrace drops
from +16.9% to +1.7% cold; git-diff and rg-output stop paying entirely.
Single CJK glyphs cost 1 token and nearly all of `mine`'s margin lives in
that difference. The "metalanguage over Unicode" intuition was right — with
the correction that the alphabet must be *probed per tokenizer* (`码` is
1 token under o200k, but `堆` is 2; `qodec aliases` shows the live table).

Reading the two columns:

- **cold** is what a one-shot message saves. Only heavy repetition (logs,
  traces, uniform JSON) clears the ~13-token container tax plus legend.
- **warm** is the protocol case: orchestrator ↔ subagent traffic where the
  legend sits in the cached preamble and every message pays body-only.
  +30–50% on exactly the payloads agents exchange most (traces, diffs,
  findings, tool output).

## What this deliberately does not claim

- **Comprehension is unproven.** Lossless-to-the-decoder ≠ legible-to-the-
  model. Whether Claude reasons as well over `类 96` with the legend in
  context as over the expanded line is the next experiment: `qodec probe`
  emits a paste-ready artifact (legend brief + encoded payload) so encoded
  vs raw can be A/B-judged with the existing `o7 judge` machinery.
- **Claude's tokenizer is not public.** o200k/cl100k are proxies; absolute
  numbers will shift, orderings should hold. A future meter can wrap an
  API-side count endpoint behind the same `TokenMeter` trait.
- **Code that will be edited must never travel mined.** An agent that pastes
  `码` into a patch ships garbage. The codec is for *evidence* payloads
  (logs, traces, listings, findings, briefs), not for spans the agent must
  reproduce exactly.

## Next steps (in rough order of information gained per effort)

1. **Comprehension A/B** — encoded vs raw payloads through `o7 judge` on the
   FP-triage rubric; measure verdict agreement. This is the go/no-go gate.
2. **Wire as an output filter** — `o7` already harvests `agent.stdout` and
   feeds judge prompts; `qodec encode --codec squeeze` is a one-line insert
   at the prompt-assembly seam (and a PostToolUse hook candidate in Claude
   Code, where tool output is the dominant repetitive payload).
3. **Prefix-aware mining** — word-boundary n-grams miss shared *prefixes*
   inside long path words (`…/ViewModels/UserEditorViewModel.cs` vs
   `…/ViewModels/ReportViewModel.cs`), visible as rg-output's modest +4.5%.
   A suffix-automaton miner over raw substrings should roughly double wins
   on path-heavy output.
4. **Session dictionaries** — persist a per-repo legend (top paths, type
   names, frame prefixes) into the cached preamble once, so *every* message
   is warm-path; `mine` then only handles payload-local repetition.
5. **Output-side notation** — the reverse direction: let the subagent *reply*
   in the legend's notation and expand deterministically outside the model.
   Output tokens cost ~5× input; this is where the same trick pays most, and
   nothing about the container is input-specific.
