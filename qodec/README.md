# qodec — Q's codec lab

Token-aware **lossless** encode/decode experiments for agent context. Q hands
the agents compact gear; this is the workbench where the gear is measured.

Standalone crate (not part of the `o7` workspace/binary — it's a lab). Design
record with theory and measured results: [`../docs/token-codec.md`](../docs/token-codec.md).

**Prereqs:** Rust ≥ 1.82 (`rustup` is enough; no nix required). The first
`cargo build` fetches dependencies from crates.io like any Rust project;
after that everything is offline — tokenizer data is bundled in the binary
and nothing phones home at runtime (`ppl` being the explicit exception: it
talks to the local LM endpoint you point it at).

## Quickstart

```bash
cd qodec
./demo.sh                 # guided tour: aliases → encode → roundtrip →
                          # honest fallback → full bench → probe artifact
./demo.sh my-big.log      # same tour + your own file at the end
```

## The pieces, one by one

```bash
cargo build --release

# what does an encoded artifact look like?
./target/release/qodec encode --codec deep -i corpus/stacktrace.txt --report
./target/release/qodec encode --codec squeeze -i corpus/findings.json --report | ./target/release/qodec decode

# the full measured table (every codec × every sample, roundtrip-verified)
./target/release/qodec bench --corpus corpus --meter o200k

# play: what do alias candidates cost under a given tokenizer?
./target/release/qodec aliases --meter o200k --top 30

# emit a paste-ready comprehension probe for a live model
./target/release/qodec probe -i corpus/stacktrace.txt --codec deep

# comprehension A/B: emit paired prompts, run them through any model, grade
./target/release/qodec ab emit -i corpus/stacktrace.txt --questions ab/stacktrace.json --out-dir /tmp/ab
./target/release/qodec ab grade --questions ab/stacktrace.json --answers answers.json
# (recorded runs + methodology: ab/results/)

# perplexity gate: score raw vs encoded under a local LM (vLLM-style
# echo+logprobs endpoint, e.g. FastContext) — cheap comprehension proxy
QODEC_PPL_URL=http://127.0.0.1:8000/v1/completions \
  ./target/release/qodec ppl -i corpus/stacktrace.txt --codec deep
```

## On your own data

Everything reads stdin when `-i` is omitted:

```bash
git diff | ./target/release/qodec encode --codec deep --report >/dev/null   # just the numbers
rg "TODO" ~/src/repo | ./target/release/qodec encode --codec deep --report | less
./target/release/qodec bench --corpus /path/to/dir/of/logs
./target/release/qodec probe -i huge.log --codec deep > probe.txt           # paste into a chat, ask questions
```

Reading the report: **cold** = the legend travels in-message; **warm** = the
legend lives in a cached prompt prefix (CLAUDE.md / system prompt) and is
amortized. Result `raw` = the codec refused — the artifact would not beat
the original, and that honesty is a feature.

## Big files (bigger than the miner wants to chew)

The `mine`/`deep`/`squeeze` miner is single-threaded and superlinear — a 26 MB
`findings.json` took ~28 min in one shot. Three rungs, in order of leverage:

```bash
# 1. Don't send records you don't need — the biggest lever needs no codec.
#    Descend to the array, filter, get a compact JSON array; kept/total on stderr.
./target/release/qodec slice -i findings.json --key findings \
  --where tool=codeql --where 'message~injection' > slice.json

# 2. Still sending many same-shaped records? toon the slice — keys-once
#    table, seconds instead of minutes (-49% on that 26 MB file in ~18 s).
./target/release/qodec slice -i findings.json --key findings \
  | ./target/release/qodec encode --codec toon --report > slice.toon

# 3. Unstructured text (build logs, traces): byte chunks, deep-encoded in
#    parallel across all cores, token reports summed.
./qodec-bulk.sh huge.log 150k deep              # -52% on the 26 MB audit JSON
```

`slice` ANDs its `--where` clauses (`key=value`, `key!=value`,
`key~substring`; the operator is the first `=`/`~`) and emits canonical
compact JSON — value-equal, like `toon`'s decode, not byte-exact.
`qodec-bulk.sh` is byte-lossless per chunk and its summed gain is a lower
bound (cross-chunk repetition is lost); a failed chunk fails the totals
instead of skewing them. It takes `$QODEC` to override the binary path.

## Codecs

| codec | idea | roundtrip |
|---|---|---|
| `mine` | token-aware dictionary miner: repeated exact spans → probed 1-token aliases, legend in header; gain **measured** per commit against the live tokenizer | byte |
| `deep` | `mine` with the full miner: word candidates ∪ suffix-automaton candidates (every repeated substring, any boundary); ~15–20× encode CPU, best ratios | byte |
| `fold` | RLE for consecutive identical lines (`%q1 xN`) | byte |
| `toon` | uniform JSON array → keys-once table | semantic (Value-equal) |
| `grep` | `path:line[:col]:text` matcher output → path once per run of hits (the `rg --heading` shape) | byte |
| `diag` | template miner for diagnostic streams (`path:line: warning: …`, MSBuild `path(l,c): …`): repeated tails → legend once, quoted identifiers → slot values; one linear pass — the redundancy is *known*, not searched for | byte |
| `tmpl` | Drain-style template mining for *any* line-based log: lines cluster by skeleton (whitespace byte-equal, ≥60% of words equal), varying positions become slots — `diag` without the format rules | byte |
| `squeeze` | `toon` (JSON) or measured best of `fold`/`grep`/`diag`/`tmpl` (text), then the better miner over the result | byte / semantic |

Format specialization is the speed lever: on the real 133 KB ownsharp audit
log, `diag` takes −52% in 0.4 s where `deep` takes −77% in 20 s — the miner
*searches* for redundancy (superlinear, re-tokenizing candidates), a format
codec already knows where it lives.

Every encode is self-describing (`%q1` container: header + legend = the
decryption key) and falls back to `raw` whenever the measured artifact does
not beat the original. `decode` is exact and deterministic — the model never
has to decompress anything.

## Rules the lab lives by

1. **Tokens, not bytes.** Gains are measured by a real BPE (`o200k` bundled
   offline; `cl100k` for cross-checks). Claude's tokenizer is not public —
   treat absolute numbers as proxy, relative ordering transfers.
2. **Measured, not modeled.** A dictionary entry is committed only if
   re-tokenizing the actual replacement beats the legend line it adds.
3. **Never lie to the decoder.** Aliases are chars provably absent from the
   input; decoding is reverse-order substitution, property-tested.
4. **Fallback is a feature.** Unique prose doesn't compress — the honest
   answer is `raw`, not a worse artifact.
