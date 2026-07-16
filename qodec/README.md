# qodec — Q's codec lab

> **Migrated:** QODEC now lives in the standalone [`PhysShell/qodec`](https://github.com/PhysShell/qodec)
> repository. New development happens there; this copy is retained as
> historical evidence for `007`'s N2 benchmark work and is no longer
> actively maintained. See `PhysShell/qodec`'s `docs/migration/README.md`
> and `MIGRATION_PROVENANCE.json` for the extraction record.

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

## Learning across runs

The miners rediscover the same paths and sentence stems on every call; a
profile remembers them between runs:

```bash
# accumulate (counts merge; deterministic, reviewable JSON)
./target/release/qodec learn --corpus ~/logs --profile repo-profile.json
# probe the remembered phrases first
./target/release/qodec encode -i today.log --codec mine --profile repo-profile.json --report
```

`learn` harvests the miner's phrase candidates and `tmpl`'s learned template
parts. `encode --profile` probes them ahead of discovery — cross-file
knowledge a single-payload miner cannot have (a phrase longer than the word
window can only arrive this way). Measured on the real ownsharp pair: a
profile learned on the 3.7 KB broker log lifts `mine` on the 133 KB sectorts
log from −65.1% to −66.5% cold. Seeds only reorder the probe queue —
acceptance stays measured, so a stale profile costs probes, never bytes.

Profile templates seed `tmpl` clustering too: they enter each bucket first
as *sealed* clusters — exact on fixed words, wildcards free, never eroded —
so a matching line lands on the known template before any same-run cluster
can claim it. That rescues greedy first-fit's known failure: two same-shape
line families merging into one mongrel template that pays an extra slot
value on every row. The seeded pass competes with the plain one by
whole-artifact measurement (ties go to seeded), so when it wins, the legend
pins the profile's template bytes exactly — stable, diffable notation
across runs. Honest scope: on the real corpora tried so far (a 428 KB
MSBuild-style log, ownsharp slices) the plain pass already finds the same
templates or beats the seeds by fixing positions that agree by chance
inside the slice, and the gate returns the plain artifact byte-identically.
The mechanism is proven on the constructed misroute case in the tests, and
the byte-stable legend is the prerequisite for moving template legends into
a cached prefix the way `--extern-legend` already moves phrases.

The measured probes themselves are training data, and the profile can
keep them:

```bash
# accumulate (features -> measured gain) statistics from real probes
./target/release/qodec train --profile repo-profile.json -i build.log -i audit.txt
# rank probes by predicted gain, spend a quarter of the budget
./target/release/qodec encode -i today.log --codec deep --profile repo-profile.json --probe-budget 10 --report
```

`train` measures the real first-round gain of every pool candidate and
accumulates ridge-regression sufficient statistics (`XᵀX`/`Xᵀy` — constant
size, merged across runs by plain summation, deterministic) into the
profile; `encode --profile` solves them into linear weights and reorders
the probe queue by predicted gain over a 4× wider candidate pool. The
ranker never decides — acceptance stays measured — so a wrong model wastes
probes, never bytes. Measured on the real 133 KB ownsharp log (`deep`,
o200k): the full-budget baseline is −76.8% in 15.1 s; at a quarter of the
probes the naive cut drops to −75.0% in 5.6 s, and the in-domain-trained
ranker recovers 83% of that quality gap (−76.5% in 4.7 s — 3.2× faster at
99.6% of the reduction; training draws candidates from the same
words∪SAM pool deep ranks, so the model sees its real distribution). Cross-format transfer is honest-weak (a
MSBuild-trained ranker on the analyzer log recovers only 9%): train where
you encode — the profile is per-repo anyway.

The loop closes with a proposer that is *not* the lab:

```bash
# 1. what did the codecs leave on the table?
./target/release/qodec residual -i audit.txt --top 12
# 2. an LLM drafts parametric span rules from the brief (out of band)
# 3. keep only what inverts byte-exactly and wins measured tokens
./target/release/qodec rules verify --draft draft.txt -i audit.txt -i other.txt -o verified.txt
# 4. exploit: pre-pass before any codec, checksum-pinned like the legends
./target/release/qodec encode -i excerpt.txt --codec squeeze --rules verified.txt --report
./target/release/qodec decode -i artifact.q1 --rules verified.txt
```

A rule is a glob template applied to spans *inside* lines — fixed parts,
single-word wildcards, anchored both ends — rewritten to
`⌈alias|value|…⌉` with probed delimiters. The verifier is the whole
point: proposals are never trusted, only measured (byte-exact inversion on
every file touched, strict token win overall) — in the first live run it
kept 1 of 3 rules this session's own proposer drafted. Honest scope from
that run: against `squeeze` the k-hole rule costs ~2 more tokens per
occurrence than the miners' k+1-literal split, so in-artifact it only wins
at very low occurrence counts (measured: 2-finding excerpt 151 vs 153
cold; 4-finding 272 vs 246 — mine amortizes its legend fast). The rules
key's genuine edge is being *out-of-band and stable* where a mine legend
is per-artifact by construction; structures the miners cannot split into
literals (many short anchors) remain the open case for future proposers.

Freeze the profile into a stable dictionary for a *cached prompt prefix*
(CLAUDE.md / system prompt) — the warm story made real:

```bash
./target/release/qodec legend --profile repo-profile.json -o qodec-legend.txt
./target/release/qodec encode -i today.log --codec mine --extern-legend qodec-legend.txt --report
./target/release/qodec decode -i artifact.q1 --extern-legend qodec-legend.txt
```

The artifact pins the legend file's checksum (`%q1 ext sum=…`); decode
without the exact file fails closed instead of reconstructing wrong bytes.
An entry whose glyph already occurs in the payload is skipped and excluded
from the artifact's `used` list, so expansion can never touch pre-existing
bytes. Measured on the real ownsharp log: in-artifact key overhead drops
950 → 23 tokens — the 564-token key lives once in the cached prefix
instead of traveling in every message (an in-band `mine` legend differs
per artifact and could never be pre-cached).

The same move exists for whole *templates* — this is where the byte-stable
seeding above pays off:

```bash
./target/release/qodec legend --templates --profile repo-profile.json -o qodec-tmpl-legend.txt
./target/release/qodec encode -i today.log --codec tmpl --extern-templates qodec-tmpl-legend.txt --report
./target/release/qodec decode -i artifact.q1 --extern-templates qodec-tmpl-legend.txt
```

Lines matching a frozen template emit rows against the *file's* alias, no
in-artifact legend line; `ext=`/`used=` params on the tmpl container pin
the file by checksum and decode fails closed without it. Each used
template must beat the lines it replaces, the whole artifact must beat the
plain one strictly (a tie keeps the keyless artifact), and an alias
occurring naturally in the input skips its entry. With the legend cost
out of the artifact, cross-file templates stop losing to chance-agreement
ones — measured on the same slices where seeding changed nothing:
the 60-line MSBuild slices go −22.0% → −34.7% and −24.1% → −37.6% cold
(654-token key), and the ownsharp broker slice against a sectorts-learned
legend goes −9.0% → **−43.9%** cold (790 → 487 tokens, 547-token key),
byte-exact under `cmp`. Frozen templates (profile seeds and extern
entries alike) are matched by *glob* now — parts may start or end
mid-word — so `qodec learn` freezes each cluster in two shapes: bare
whole-word slots (general across files; its long parts also feed
`seed_phrases`) and sub-word refined (corpus-specific, far cheaper per
row), tried heaviest-first. With sub-word keys the extern story closes:
the MSBuild slices reach **−65.7%/−67.1%** cold (refined plain stops at
−50.3%/−51.0%) and the broker slice **−57.0%** (868 → 373 tokens, key
overhead 42), all byte-exact, all fail-closed without the exact file.

## Codecs

| codec | idea | roundtrip |
|---|---|---|
| `mine` | token-aware dictionary miner: repeated exact spans → probed 1-token aliases, legend in header; gain **measured** per commit against the live tokenizer | byte |
| `deep` | `mine` with the full miner: word candidates ∪ suffix-automaton candidates (every repeated substring, any boundary); ~15–20× encode CPU, best ratios | byte |
| `fold` | RLE for consecutive identical lines (`%q1 xN`) | byte |
| `toon` | uniform JSON array → keys-once table | semantic (Value-equal) |
| `grep` | `path:line[:col]:text` matcher output → path once per run of hits (the `rg --heading` shape) | byte |
| `diag` | template miner for diagnostic streams (`path:line: warning: …`, MSBuild `path(l,c): …`): repeated tails → legend once, quoted identifiers → slot values; one linear pass — the redundancy is *known*, not searched for | byte |
| `tmpl` | Drain-style template mining for *any* line-based log: lines cluster by skeleton (whitespace byte-equal, ≥60% of words equal), varying positions become slots; slots go *sub-word* — a cluster pulls its members' common prefix/suffix inside a varying word into the template when that measures cheaper, so a path that differs in one number costs one number per row | byte |
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
