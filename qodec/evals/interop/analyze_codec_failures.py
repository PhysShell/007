#!/usr/bin/env python3
"""analyze_codec_failures.py — offline decomposition of stable codec losses.

Reads an immutable canonical Level-2 run, verifies its SHA256SUMS, reproduces
the stable codec losses from the scorer's own criterion (not a hardcoded list),
and writes a hash-verified dossier: per-loss and matched-control evidence, gold
span fate, evidence-linked mechanism labels, and an aggregate summary with an
honest conclusion. No model endpoint, no qodec, no network.

    python3 analyze_codec_failures.py \
        --run results/l2-cpu-qwen2.5-coder-7b-v1 \
        --out analysis/l2-qwen2.5-coder-7b-failure-decomp-v1 \
        --source-commit 0b76e64

The analysis directory is separate from the canonical result — nothing is ever
written into the canonical run directory.
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path

from bench import failure_decomp as fd

HERE = Path(__file__).resolve().parent


def _dump(obj) -> str:
    return json.dumps(obj, indent=2, sort_keys=True, ensure_ascii=False) + "\n"


def _slug(case: str, qid: str) -> str:
    return f"{case}__{qid}"


def _render_case_md(d: dict) -> str:
    idn = d["identity"]
    out = [f"# {d['kind'].upper()}: {idn['case']} / {idn['question_id']}", ""]
    out.append(f"- category: **{idn['category']}**  field: `{idn['field']}`  match: `{idn['match_mode']}`")
    out.append(f"- question: {idn['question_text']}")
    out.append(f"- gold: `{idn['gold']}`")
    out.append(f"- source: run `{idn['source_run']}` commit `{idn['source_commit']}`  "
               f"records_sha256 `{(idn['records_sha256'] or '')[:12]}`")
    out.append(f"- model `{idn['model']}`  qodec `{idn['qodec']}`  tokenizer `{(idn['tokenizer_sha256'] or '')[:12]}`")
    if d["kind"] == "loss":
        m = d["mechanism"]
        out += ["", f"## mechanism: **{m['primary_mechanism']}**"
                + (f"  (+ {', '.join(m['secondary_mechanisms'])})" if m["secondary_mechanisms"] else "")]
        out.append("```json")
        out.append(json.dumps(m["evidence"], indent=2, ensure_ascii=False))
        out.append("```")

    out += ["", "## answers (all arms, all repeats)", "",
            "| arm | rep | correct | fmt | malformed | leaks | invalid | ptok | ctok | answer_sha |",
            "|-----|-----|---------|-----|-----------|-------|---------|------|------|------------|"]
    for a in d["answers"]:
        out.append(f"| {a['arm']} | {a['repeat']} | {a['correct']} | {a['format_compliant']} | "
                   f"{a['malformed']} | {len(a['alias_leaks'])} | {len(a['invalid_identifiers'])} | "
                   f"{a['prompt_tokens']} | {a['completion_tokens']} | {a['answer_sha256'][:10]} |")

    out += ["", "## gold span fate"]
    for s in d["span_analysis"]["gold_spans"]:
        out.append(f"- `{s['span']}` → **{s['fate']}**"
                   + (f"  aliases={s['aliases']}" if s.get("aliases") else ""))
    if "locator_checks" in d["span_analysis"]:
        out.append("")
        out.append("locator checks: " + json.dumps(d["span_analysis"]["locator_checks"], ensure_ascii=False))
    if "count_checks" in d["span_analysis"]:
        out.append("")
        out.append("count checks: " + json.dumps(d["span_analysis"]["count_checks"], ensure_ascii=False))

    ev = d["prompt_evidence"]
    out += ["", "## alias dictionary (used)", "```"]
    for a in ev["used_aliases"]:
        out.append(f"{a} = {ev['alias_dictionary'].get(a)}")
    out.append("```")
    out += ["", f"## raw→encoded diff (+{d['diff_stats']['added']} / -{d['diff_stats']['removed']}), full diff in `{_slug(idn['case'], idn['question_id'])}.diff`",
            "", "gold-touching hunks:", "```diff"]
    out += d["gold_diff_hunks"]
    out.append("```")
    return "\n".join(out) + "\n"


def _render_readme(summary: dict, controls_map: list, loss_dossiers: list) -> str:
    out = ["# Level-2 failure decomposition — five stable codec losses", ""]
    out.append(f"Offline decomposition of the canonical run `{summary['source_run']}`. "
               "No model/qodec/network; source verified against its own SHA256SUMS.")
    out += ["", f"**Conclusion: {summary['conclusion']}**", "",
            "This decomposes *why* the five stable codec losses happen; it does not "
            "revisit the canonical verdict (general comprehension drop) and does not "
            "propose protected spans.", ""]
    out += ["## losses", "",
            "| case | question | category | primary mechanism | secondary |",
            "|------|----------|----------|-------------------|-----------|"]
    for d in loss_dossiers:
        i, m = d["identity"], d["mechanism"]
        out.append(f"| {i['case']} | {i['question_id']} | {i['category']} | "
                   f"**{m['primary_mechanism']}** | {', '.join(m['secondary_mechanisms']) or '—'} |")
    out += ["", "## matched controls (deterministic selection)", "",
            "| loss | control | same cat | same case | Δtokens | Δalias# | Δdensity |",
            "|------|---------|----------|-----------|---------|---------|----------|"]
    for c in controls_map:
        s = c["selection_score"]
        out.append(f"| {c['loss']['case']}/{c['loss']['question_id']} | "
                   f"{c['control']['case']}/{c['control']['question_id']} | {s['same_category']} | "
                   f"{s['same_case']} | {s['encoded_token_diff']} | {s['alias_count_diff']} | {s['alias_density_diff']} |")
    out += ["", "## summary", "```json", json.dumps(summary, indent=2, ensure_ascii=False), "```", ""]
    out.append("Files: `summary.json`, `losses.json`, `controls.json`, `cases/`. "
               "Integrity: `SHA256SUMS`.")
    return "\n".join(out) + "\n"


def build_files(run_dir: Path, source_commit: str | None = None) -> dict:
    """Pure: verify the canonical run, decompose, and return {relpath: text} for
    every output file including SHA256SUMS. No writes, no timestamps — so a
    regeneration is byte-identical. Raises on canonical drift or loss-set mismatch."""
    expected = fd.verify_sha256sums(run_dir)   # raises CanonicalMismatch on any drift
    canon = fd.load_canonical(run_dir)
    records, tasks, meta = canon["records"], canon["tasks"], canon["meta"]

    losses = fd.stable_codec_losses(records)
    fd.crosscheck_losses(losses, tasks, canon["report"])   # raises if != canonical report

    source = {"commit": source_commit,
              "records_sha256": expected.get("records.jsonl"),
              "tasks_snapshot_sha256": canon["tasks_snapshot_sha256"]}

    loss_dossiers = [fd.build_dossier("loss", c, q, records, tasks, meta, source) for (c, q) in losses]
    controls_map = fd.select_controls(losses, records, tasks)
    control_dossiers = [fd.build_dossier("control", cm["control"]["case"], cm["control"]["question_id"],
                                         records, tasks, meta, source) for cm in controls_map]
    summary = fd.summarize(loss_dossiers, control_dossiers, controls_map, meta)

    def strip(d):
        d = dict(d)
        d.pop("_full_diff", None)
        return d

    files: dict[str, str] = {}
    files["summary.json"] = _dump(summary)
    files["losses.json"] = _dump([strip(d) for d in loss_dossiers])
    files["controls.json"] = _dump({"selection": controls_map,
                                    "dossiers": [strip(d) for d in control_dossiers]})
    for d in loss_dossiers + control_dossiers:
        slug = _slug(d["identity"]["case"], d["identity"]["question_id"])
        files[f"cases/{slug}.md"] = _render_case_md(d)
        files[f"cases/{slug}.diff"] = d["_full_diff"] + "\n"
    files["README.md"] = _render_readme(summary, controls_map, loss_dossiers)

    sha_lines = [f"{fd.sha256_text(text)}  ./{rel}" for rel, text in sorted(files.items())]
    files["SHA256SUMS"] = "\n".join(sha_lines) + "\n"
    files["_meta"] = {"losses": loss_dossiers, "controls_map": controls_map, "summary": summary}
    return files


def write_files(out_dir: Path, files: dict) -> None:
    out_dir.mkdir(parents=True, exist_ok=True)
    (out_dir / "cases").mkdir(exist_ok=True)
    for rel, text in files.items():
        if rel == "_meta":
            continue
        (out_dir / rel).write_text(text, encoding="utf-8")


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--run", type=Path, required=True, help="canonical Level-2 run directory")
    ap.add_argument("--out", type=Path, required=True, help="analysis output directory (kept separate)")
    ap.add_argument("--source-commit", default=None, help="commit of the canonical record, for provenance")
    args = ap.parse_args()

    run_dir = args.run if args.run.is_absolute() else (HERE / args.run)
    out_dir = args.out if args.out.is_absolute() else (HERE / args.out)
    if run_dir.resolve() in out_dir.resolve().parents or run_dir.resolve() == out_dir.resolve():
        print("refusing to write analysis inside the canonical run directory")
        return 2

    files = build_files(run_dir, args.source_commit)
    write_files(out_dir, files)

    meta = files["_meta"]
    print(f"wrote {out_dir}  ({len(meta['losses'])} losses, {len(meta['controls_map'])} controls)")
    print(f"conclusion: {meta['summary']['conclusion']}")
    for d in meta["losses"]:
        i, m = d["identity"], d["mechanism"]
        print(f"  {i['case']}:{i['question_id']}  → {m['primary_mechanism']}"
              + (f" (+{', '.join(m['secondary_mechanisms'])})" if m["secondary_mechanisms"] else ""))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
