#!/usr/bin/env python3
"""run_reader.py — Level 2: reader comprehension benchmark (needs a served model).

For each L1 real-tool case and question, ask the served model the same question
three ways — raw / raw+brief / encoded+brief — and score by rule. The encoded arm
is re-encoded under the TARGET tokenizer (QODEC_READER_TOKENIZER = hf:…), so
aliases and codec acceptance match what the model reads. Everything is recorded:
request, response, parsed answer, score, per-arm local tokens, server usage,
TTFT, latency, plus a preflight receipt and the tokenizer/model/qodec identities.

Before the first request a `run-manifest.json` pins the run's identity (model,
tokenizer, qodec, codec, tasks, L1 inputs, brief/system prompt, effective
contract, seed, arms). On `--resume` the current environment is re-derived and
compared field-by-field against that manifest — any mismatch aborts before a
single request, and the canonical `preflight.json` is never overwritten (a
resume writes `preflight-resume-N.json`). The matrix itself is crash-durable:
records are journaled + flushed per request, so a pass-2 crash keeps pass 1 and
`--resume` skips completed (case,question,arm,repeat) keys without duplicates.

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

from bench import durability, matrix, preflight, qodec, reader, reader_tasks

HERE = Path(__file__).resolve().parent
ARMS = matrix.ARMS


def _tool_only_text(l1_run: Path, case: str) -> tuple[str, str]:
    matches = sorted(l1_run.glob(f"*/cases/{case}/*/transformed.txt")) \
        or sorted(l1_run.glob(f"cases/{case}/*/transformed.txt"))
    if not matches:
        raise FileNotFoundError(f"no transformed.txt for case {case!r} under {l1_run}")
    return matches[0].read_text(), matches[0].parent.name


def _run_manifest(cfg, pf: dict, args, tasks: list[dict], brief: str, ctx: dict) -> dict:
    """The immutable run identity: everything whose change would make a resumed
    run a different experiment. Hashes, not paths, are load-bearing."""
    ti, mi = pf["tokenizer"], pf["model_identity"]
    chat_template_sha = None
    if ti.get("path"):
        tok_dir = Path(ti["path"]).parent
        for name in ("chat_template.jinja", "chat_template.json"):
            ct = tok_dir / name
            if ct.exists():
                chat_template_sha = matrix.sha256_bytes(ct.read_bytes())
                break
    return {
        "manifest_version": 1,
        "model_requested": cfg.model,
        "model_reported": pf["models"].get("model_reported"),
        "model_gguf_sha256": mi.get("model_file_sha256"),
        "tokenizer_sha256": ti.get("sha256"),
        "tokenizer_config_sha256": ti.get("tokenizer_config_sha256"),
        "chat_template_sha256": chat_template_sha,
        "qodec_binary_sha256": qodec.binary_sha256(),
        "codec": args.codec,
        "tasks_snapshot_sha256": matrix.sha256_bytes(Path(args.tasks).read_bytes()),
        "l1_run": str(args.l1_run),
        "l1_tool_only_sha256": {case: matrix.sha256_text(ctx[case]["tool_only"]) for case in sorted(ctx)},
        "notation_brief_sha256": matrix.sha256_text(brief),
        "system_prompt_sha256": matrix.sha256_text(reader_tasks.SYSTEM),
        "effective_contract": pf["effective"],
        "determinism": {"temperature": 0, "seed": pf["determinism"].get("seed_sent")},
        "arms": list(ARMS),
    }


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--l1-run", type=Path, required=True)
    ap.add_argument("--tasks", type=Path, default=HERE / "tasks" / "reader" / "tasks.json")
    ap.add_argument("--codec", default="squeeze")
    ap.add_argument("--name")
    ap.add_argument("--out", type=Path, default=HERE / "runs-l2")
    ap.add_argument("--repeats", type=int, default=3, help="repeats for flagged questions (pass 2)")
    ap.add_argument("--resume", action="store_true",
                    help="continue an existing run dir, skipping already-completed keys")
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

    # Fail fast on the directory policy before touching the endpoint.
    try:
        matrix.assert_dir_policy(run_dir, args.resume)
    except matrix.DirectoryPolicyError as exc:
        print(f"directory policy: {exc}")
        return 5
    if not args.resume:
        run_dir.mkdir(parents=True, exist_ok=True)

    pf = preflight.run(cfg, meter)
    if args.resume:
        # Never overwrite the canonical preflight.json on a resume; keep the
        # current environment's receipt alongside it for the audit trail.
        n = len(list(run_dir.glob("preflight-resume-*.json"))) + 1
        preflight.save(pf, run_dir / f"preflight-resume-{n}.json")
    else:
        preflight.save(pf, run_dir / "preflight.json")
    if not pf["ready"]:
        print(f"preflight not ready: {json.dumps(pf.get('streaming_sample', {}))[:200]}")
        print(f"saved preflight receipt under {run_dir} — fix the endpoint and re-run.")
        return 4
    eff = preflight.effective_from(pf)     # the negotiated contract the matrix uses
    model_reported = pf["models"].get("model_reported")

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

    def run_one(case: str, q: dict, arm: str, repeat: int) -> dict:
        c = ctx[case]
        payload = c["artifact"] if arm == "encoded+brief" else c["tool_only"]
        msgs = reader_tasks.build_messages(arm, payload, brief, q["q"])
        res = reader.chat(cfg, msgs, eff)
        ans = reader_tasks.parse_answer(res.text)
        sc = reader_tasks.score_question(q, ans, source_text=c["tool_only"], aliases=c["aliases"])
        return {
            "case": case, "question": q["id"], "category": q["category"], "arm": arm, "repeat": repeat,
            # semantic_correct vs format: malformed is a FORMAT failure separated
            # from wrong-content, but both count as an overall failure.
            "correct": sc.correct, "format_compliant": ans is not None, "malformed": ans is None,
            "invalid_identifiers": sc.invalid_identifiers, "alias_leaks": sc.alias_leaks,
            "local_content_tokens": {"raw": case_tokens[case]["raw_payload"],
                                     "raw+brief": case_tokens[case]["raw+brief_content"],
                                     "encoded+brief": case_tokens[case]["encoded+brief_content"]}[arm],
            "server_prompt_tokens": res.usage.get("prompt_tokens"),
            "completion_tokens": res.usage.get("completion_tokens"),
            "ttft_ms": res.ttft_ms, "total_ms": res.total_ms, "http_error": res.http_error,
            "answer_raw": res.text, "answer_parsed": ans, "request": res.request,
        }

    manifest = _run_manifest(cfg, pf, args, tasks, brief, ctx)
    try:
        result = matrix.run_matrix(run_dir, manifest=manifest, tasks=tasks, run_one=run_one,
                                   resume=args.resume, repeats=args.repeats)
    except matrix.ManifestMismatch as exc:
        print(f"resume refused — {exc}")
        print("The endpoint/inputs changed since this run started; no request was sent. "
              "Start a fresh run (new --name) for the changed configuration.")
        return 6
    except matrix.DirectoryPolicyError as exc:
        print(f"directory policy: {exc}")
        return 5

    records, to_repeat = result["records"], result["to_repeat"]
    meta = {
        "run_id": run_id, "level": 2, "kind": "cpu-calibration",
        "model_requested": cfg.model, "model_reported": model_reported,
        "model_identity": pf["model_identity"], "tokenizer": pf["tokenizer"],
        "qodec_version": qodec.version(), "reader_url": cfg.url,
        "effective": pf["effective"], "structured_json": pf.get("structured_json"),
        "determinism": pf["determinism"],
        "preflight_ttft_ms": pf.get("streaming_sample", {}).get("ttft_ms"),
        "matrix_streamed": eff.stream,
        "tasks": str(args.tasks), "l1_run": str(args.l1_run),
        "manifest_sha256": matrix.sha256_bytes((run_dir / matrix.MANIFEST).read_bytes()),
        "case_tokens": case_tokens, "n_records": len(records),
        "unique_questions": len(tasks),
        "repeated_questions": [f"{q['case']}:{q['id']}" for q in to_repeat],
    }
    durability.atomic_write(run_dir / "meta.json", json.dumps(meta, indent=2) + "\n")
    print(f"wrote {run_dir}  ({len(records)} answers, {len(to_repeat)} questions repeated)")
    print(f"score with: python3 score_reader.py {run_dir}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
