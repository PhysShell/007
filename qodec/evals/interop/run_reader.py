#!/usr/bin/env python3
"""run_reader.py — Level 2: reader comprehension benchmark (needs a served model).

For each L1 real-tool case and question, ask the served model the same question
three ways — raw / raw+brief / encoded+brief — and score by rule. The encoded arm
is re-encoded under the TARGET tokenizer (QODEC_READER_TOKENIZER = hf:…), so
aliases and codec acceptance match what the model reads. Everything is recorded:
request, response, parsed answer, score, per-arm local tokens, server usage,
TTFT, latency, plus a preflight receipt and the tokenizer/model/qodec identities.

Two passes: pass 1 runs every (case, question) once; pass 2 re-runs (2 more
times) only the questions that were flagged — malformed JSON, raw/raw+brief
disagreement, a codec loss, alias leakage, or an invalid identifier.

    export QODEC_READER_URL=http://127.0.0.1:8000/v1
    export QODEC_READER_MODEL=<served-model-id>
    export QODEC_READER_TOKENIZER=hf:/abs/tokenizer.json
    python3 run_reader.py --l1-run results/rtk-codegraph-clap-v1 --name l2
"""

from __future__ import annotations

import argparse
import datetime as _dt
import json
from pathlib import Path

from bench import preflight, qodec, reader, reader_tasks

HERE = Path(__file__).resolve().parent
ARMS = ["raw", "raw+brief", "encoded+brief"]


def _tool_only_text(l1_run: Path, case: str) -> tuple[str, str]:
    matches = sorted(l1_run.glob(f"*/cases/{case}/*/transformed.txt")) \
        or sorted(l1_run.glob(f"cases/{case}/*/transformed.txt"))
    if not matches:
        raise FileNotFoundError(f"no transformed.txt for case {case!r} under {l1_run}")
    return matches[0].read_text(), matches[0].parent.name


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--l1-run", type=Path, required=True)
    ap.add_argument("--tasks", type=Path, default=HERE / "tasks" / "reader" / "tasks.json")
    ap.add_argument("--codec", default="squeeze")
    ap.add_argument("--name")
    ap.add_argument("--out", type=Path, default=HERE / "runs-l2")
    ap.add_argument("--repeats", type=int, default=3, help="repeats for flagged questions (pass 2)")
    args = ap.parse_args()

    try:
        cfg = reader.ReaderConfig.from_env()
    except reader.ReaderUnavailable as exc:
        print(f"reader unavailable: {exc}")
        print("Level 2 needs a served OpenAI-compatible model. Set QODEC_READER_* and re-run; "
              "the harness and scoring are otherwise ready (see tests).")
        return 3
    meter = cfg.tokenizer or "o200k"

    run_id = args.name or _dt.datetime.now(_dt.timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    run_dir = args.out / run_id
    run_dir.mkdir(parents=True, exist_ok=True)

    pf = preflight.run(cfg, meter)
    preflight.save(pf, run_dir / "preflight.json")
    if not pf["ready"]:
        print(f"preflight not ready: {json.dumps(pf.get('streaming', {}))[:200]}")
        print(f"saved {run_dir}/preflight.json — fix the endpoint and re-run.")
        return 4
    model_reported = None
    if pf["models"].get("ok") and pf["models"].get("ids"):
        model_reported = pf["models"]["ids"][0]

    tasks = reader_tasks.load_tasks(args.tasks)
    brief = qodec.notation()

    # Per-case context: encoded artifact under the target tokenizer, aliases,
    # per-arm content, and per-arm LOCAL content tokens (what the codec sees).
    ctx: dict[str, dict] = {}
    case_tokens: dict[str, dict] = {}
    for case in dict.fromkeys(t["case"] for t in tasks):
        tool_only, l1_arm = _tool_only_text(args.l1_run, case)
        env = qodec.encode(tool_only, codec=args.codec, meter=meter, passthrough=True)
        aliases = reader_tasks.used_aliases(env.content) if env.encoded else set()
        ctx[case] = {"tool_only": tool_only, "artifact": env.content, "aliases": aliases}
        # LOCAL content tokens = tokens of exactly what build_messages sends.
        local = {
            "raw": qodec.count(tool_only, meter=meter),
            "raw+brief": qodec.count(f"{brief}\n\n{tool_only}", meter=meter),
            "encoded+brief": qodec.count(f"{brief}\n\n{env.content}", meter=meter),
        }
        case_tokens[case] = {
            "raw_payload": local["raw"],
            "raw+brief_content": local["raw+brief"],
            "encoded+brief_content": local["encoded+brief"],  # = cold prompt
            "encoded_artifact_warm": env.tokens_out,           # amortization estimate only
            "encoded": env.encoded, "codec": env.codec,
        }

    # This endpoint streams content but not usage (recorded in preflight). Score
    # requests non-streaming so server prompt/completion tokens are REAL, not
    # local estimates; TTFT is the preflight streaming sample.
    stream_usage = bool(pf.get("streaming", {}).get("usage_supported"))

    def run_one(case: str, q: dict, arm: str, repeat: int) -> dict:
        c = ctx[case]
        payload = c["artifact"] if arm == "encoded+brief" else c["tool_only"]
        msgs = reader_tasks.build_messages(arm, payload, brief, q["q"])
        res = reader.chat(cfg, msgs, stream=stream_usage)
        ans = reader_tasks.parse_answer(res.text)
        sc = reader_tasks.score_question(q, ans, source_text=c["tool_only"], aliases=c["aliases"])
        return {
            "case": case, "question": q["id"], "category": q["category"], "arm": arm, "repeat": repeat,
            "correct": sc.correct, "malformed": ans is None,
            "invalid_identifiers": sc.invalid_identifiers, "alias_leaks": sc.alias_leaks,
            "local_content_tokens": {"raw": case_tokens[case]["raw_payload"],
                                     "raw+brief": case_tokens[case]["raw+brief_content"],
                                     "encoded+brief": case_tokens[case]["encoded+brief_content"]}[arm],
            "server_prompt_tokens": res.usage.get("prompt_tokens"),
            "completion_tokens": res.usage.get("completion_tokens"),
            "ttft_ms": res.ttft_ms, "total_ms": res.total_ms,
            "answer_raw": res.text, "answer_parsed": ans, "request": res.request,
        }

    # Cache-friendly order: group by (case, arm) so the server re-uses the
    # prefix KV across a case's questions (the content is identical; only the
    # short question suffix changes). Interleaving arms would evict it and
    # re-prefill the whole payload every request.
    by_case: dict[str, list[dict]] = {}
    for q in tasks:
        by_case.setdefault(q["case"], []).append(q)

    records = []
    # Pass 1 — everything once, content-grouped.
    for case, qs in by_case.items():
        for arm in ARMS:
            for q in qs:
                records.append(run_one(case, q, arm, 0))

    # Flag questions for pass 2.
    def flagged(case, qid) -> bool:
        rs = [r for r in records if r["case"] == case and r["question"] == qid]
        byarm = {r["arm"]: r for r in rs}
        if any(r["malformed"] for r in rs):
            return True
        if any(r["alias_leaks"] for r in rs) or any(r["invalid_identifiers"] for r in rs):
            return True
        raw, rb, eb = byarm.get("raw"), byarm.get("raw+brief"), byarm.get("encoded+brief")
        if raw and rb and raw["correct"] != rb["correct"]:
            return True
        if rb and eb and rb["correct"] and not eb["correct"]:  # codec loss
            return True
        return False

    to_repeat = [q for q in tasks if flagged(q["case"], q["id"])]
    repeat_by_case: dict[str, list[dict]] = {}
    for q in to_repeat:
        repeat_by_case.setdefault(q["case"], []).append(q)
    for repeat in range(1, max(1, args.repeats)):
        for case, qs in repeat_by_case.items():
            for arm in ARMS:
                for q in qs:
                    records.append(run_one(case, q, arm, repeat))

    meta = {
        "run_id": run_id, "level": 2, "kind": "cpu-calibration",
        "model_requested": cfg.model, "model_reported": model_reported,
        "tokenizer": pf["tokenizer"], "qodec_version": qodec.version(),
        "reader_url": cfg.url, "determinism": pf["determinism"],
        "preflight_ttft_ms": pf.get("streaming", {}).get("ttft_ms"),
        "streaming_usage_supported": bool(pf.get("streaming", {}).get("usage_supported")),
        "tasks": str(args.tasks), "l1_run": str(args.l1_run),
        "case_tokens": case_tokens, "n_records": len(records),
        "repeated_questions": [f"{q['case']}:{q['id']}" for q in to_repeat],
    }
    (run_dir / "meta.json").write_text(json.dumps(meta, indent=2) + "\n")
    with (run_dir / "records.jsonl").open("w") as fh:
        for r in records:
            fh.write(json.dumps(r, ensure_ascii=False) + "\n")
    print(f"wrote {run_dir}  ({len(records)} answers, {len(to_repeat)} questions repeated)")
    print(f"score with: python3 score_reader.py {run_dir}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
