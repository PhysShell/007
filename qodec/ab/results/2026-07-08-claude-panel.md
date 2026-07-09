# Comprehension A/B — run record, 2026-07-08

- **Panel:** 8 fresh-context Claude subagents (Claude Code session model),
  one per (payload × side), tools forbidden, payload inline. Fresh contexts
  had no access to the raw originals on the encoded side.
- **Payloads:** the 4 lab corpus files that survive encoding
  (`stacktrace`, `build-log`, `findings`, `rg-output`), encoded with
  `--codec deep --alphabet auto` (12–16 dictionary entries each).
- **Questions:** `ab/*.json` (6 per payload, distinctive accept substrings).
- **Answers:** `ab/results/*.answers.json`, graded by `qodec ab grade`.

| payload | raw | encoded | notes |
|---|---:|---:|---|
| stacktrace | 6/6 | 6/6 | answers byte-identical bar formatting |
| build-log | 6/6 | 6/6 | answers byte-identical |
| findings | 6/6 | 6/6 | incl. counting `suspect_fp=true` across nested aliases |
| rg-output | 6/6 | 6/6 | answers byte-identical |
| **total** | **24/24** | **24/24** | |

Observed asymmetry worth tracking: encoded QA took ~3–5× the wall time on
the alias-dense payloads (24–26 s vs 5–8 s) — the model spends thinking
effort decoding. On reasoning models some of the input-token savings shifts
into thinking tokens; on non-reasoning flows the savings is undiluted.

Scope honesty: one model family, one run, 4 small payloads (300–660 tokens),
retrieval-style questions. Not tested: long-context payloads, deep reasoning
over decoded content, weaker/cheaper reader models. The gate is *open*, not
*proven* — extend with `o7 judge` FP-triage agreement next.

Post-run grader hardening (Codex review on PR #28): purely numeric accepts
now match only at digit boundaries (`"2"` no longer passes `"20 warnings"`),
and `ab emit` fails closed when the codec falls back to raw. The recorded
answers above were re-graded under the hardened rules: **24/24 = 24/24
stands** (every numeric answer was the exact digits). All four payloads had
encoded as `mine` with 12–16 legend entries — no raw fallbacks in this run.
