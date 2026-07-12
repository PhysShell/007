#!/usr/bin/env python3
"""run_reader.py — Level 2: reader comprehension benchmark (needs a served model).

For each of the L1 real-tool cases, and each question, ask the served model the
same question three ways — raw / raw+brief / encoded+brief — and score the
answers by rule. The encoded arm is re-encoded under the TARGET tokenizer
(`--meter hf:<tokenizer.json>`), so aliases and codec acceptance match what the
model actually reads (B1). Everything (request, response, TTFT, latency, tokens)
is recorded.

    export QODEC_READER_URL=http://127.0.0.1:8000/v1
    export QODEC_READER_MODEL=<served-model-id>
    export QODEC_READER_TOKENIZER=hf:/abs/tokenizer.json
    python3 run_reader.py --l1-run results/rtk-codegraph-clap-v1 --name l2-smoke

Without an endpoint this exits with a clear message — it never fabricates
answers.
"""

from __future__ import annotations

import argparse
import datetime as _dt
import json
from pathlib import Path

from bench import qodec, reader, reader_tasks

HERE = Path(__file__).resolve().parent


def _tool_only_text(l1_run: Path, case: str) -> tuple[str, str]:
    """Return (tool_only_text, arm) for a case from an L1 run/record."""
    matches = sorted((l1_run).glob(f"*/cases/{case}/*/transformed.txt")) \
        or sorted((l1_run).glob(f"cases/{case}/*/transformed.txt"))
    if not matches:
        raise FileNotFoundError(f"no transformed.txt for case {case!r} under {l1_run}")
    p = matches[0]
    return p.read_text(), p.parent.name


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--l1-run", type=Path, required=True, help="an L1 run/record dir with cases/*/transformed.txt")
    ap.add_argument("--tasks", type=Path, default=HERE / "tasks" / "reader" / "tasks.json")
    ap.add_argument("--codec", default="squeeze")
    ap.add_argument("--name")
    ap.add_argument("--out", type=Path, default=HERE / "runs-l2")
    ap.add_argument("--repeats", type=int, default=1, help="runs per (case,arm,question); >1 for disagreement cases")
    args = ap.parse_args()

    try:
        cfg = reader.ReaderConfig.from_env()
    except reader.ReaderUnavailable as exc:
        print(f"reader unavailable: {exc}")
        print("Level 2 needs a served OpenAI-compatible model. Set the QODEC_READER_* env "
              "and re-run; the harness and scoring are otherwise ready (see tests).")
        return 3
    meter = cfg.tokenizer or "o200k"
    if not meter.startswith("hf:"):
        print("WARNING: QODEC_READER_TOKENIZER is not hf:<tokenizer.json> — Level 2 is only "
              "honest under the served model's own tokenizer.")

    tasks = reader_tasks.load_tasks(args.tasks)
    brief = qodec.notation()
    run_id = args.name or _dt.datetime.now(_dt.timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    run_dir = args.out / run_id
    run_dir.mkdir(parents=True, exist_ok=True)

    records = []
    case_tokens: dict[str, dict] = {}
    for case, questions in tasks.items():
        tool_only, l1_arm = _tool_only_text(args.l1_run, case)
        env = qodec.encode(tool_only, codec=args.codec, meter=meter, passthrough=True)
        glyphs = reader_tasks.legend_glyphs(env.content) if env.encoded else set()
        payloads = {
            "raw": (tool_only, ""),
            "raw+brief": (tool_only, brief),
            "encoded+brief": (env.content, brief),
        }
        tokens = {
            "raw": qodec.count(tool_only, meter=meter),
            "cold_encoded": qodec.count(f"{brief}\n\n{env.content}", meter=meter),
            "warm_encoded": env.tokens_out,
            "encoded": env.encoded, "codec": env.codec,
        }
        case_tokens[case] = tokens
        for q in questions:
            for arm, (payload, br) in payloads.items():
                for rep in range(args.repeats):
                    msgs = reader_tasks.build_messages(arm, payload, br, q["q"])
                    res = reader.chat(cfg, msgs)
                    ans = reader_tasks.parse_answer(res.text)
                    sc = reader_tasks.score_question(q, ans, source_text=tool_only, glyphs=glyphs)
                    records.append({
                        "case": case, "l1_arm": l1_arm, "question": q["id"], "qtype": q["type"],
                        "arm": arm, "repeat": rep, "correct": sc.correct,
                        "invalid_identifiers": sc.invalid_identifiers, "alias_leak": sc.alias_leak,
                        "answer_raw": res.text, "answer_parsed": ans,
                        "ttft_ms": res.ttft_ms, "total_ms": res.total_ms,
                        "usage": res.usage, "request": res.request,
                    })

    meta = {
        "run_id": run_id, "level": 2, "model": cfg.model, "tokenizer": meter,
        "reader_url": cfg.url, "tasks": str(args.tasks), "l1_run": str(args.l1_run),
        "case_tokens": case_tokens,
    }
    (run_dir / "meta.json").write_text(json.dumps(meta, indent=2) + "\n")
    with (run_dir / "records.jsonl").open("w") as fh:
        for r in records:
            fh.write(json.dumps(r, ensure_ascii=False) + "\n")
    print(f"wrote {run_dir}  ({len(records)} answers)  score with: python3 score_reader.py {run_dir}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
