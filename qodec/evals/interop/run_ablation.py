#!/usr/bin/env python3
"""run_ablation.py — targeted factorial alias × structural ablation on 7B.

Ten questions (five canonical stable losses + five matched controls) × six arms
(R / I / M / F / MF / GF, see bench.ablation_policies) in ONE run, so runtime
drift is never confounded with the treatment. Same served model, tokenizer,
determinism and negotiated contract as the canonical 7B record. Crash-durable:
records are journaled per request; --resume skips completed keys.

    export QODEC_READER_URL=http://127.0.0.1:8000/v1
    export QODEC_READER_MODEL=qwen2.5-coder-7b-instruct
    export QODEC_READER_TOKENIZER=hf:/abs/tokenizer.json
    python3 run_ablation.py --l1-run results/rtk-codegraph-clap-v1 --name l2-ablation
"""

from __future__ import annotations

import argparse
import datetime as _dt
import json
from pathlib import Path

from bench import ablation_policies as ap
from bench import durability, preflight, qodec, reader, reader_tasks

HERE = Path(__file__).resolve().parent

# The diagnostic target: the five canonical stable losses, then the five
# deterministically-selected matched controls (last two pairs weakly matched).
LOSSES = [("build-log-rtk-log", "n-warnings"), ("clap-derive-explore", "def-path"),
          ("clap-derive-explore", "top-symbol"), ("rtk-rg-derive-clap", "file"),
          ("rtk-rg-parser-clap", "file")]
CONTROLS = [("build-log-rtk-log", "n-errors"), ("clap-derive-explore", "trait"),
            ("clap-derive-explore", "trait-path"), ("rtk-rg-parser-clap", "symbol"),
            ("rg-output-rtk-grep", "method")]
WEAKLY_MATCHED = {("rtk-rg-parser-clap", "symbol"), ("rg-output-rtk-grep", "method")}
ARMS = ap.ARM_NAMES  # R, I, M, F, MF, GF


def _tool_only_text(l1_run: Path, case: str) -> str:
    matches = sorted(l1_run.glob(f"*/cases/{case}/*/transformed.txt")) \
        or sorted(l1_run.glob(f"cases/{case}/*/transformed.txt"))
    if not matches:
        raise FileNotFoundError(f"no transformed.txt for case {case!r} under {l1_run}")
    return matches[0].read_text(encoding="utf-8")


def main() -> int:
    ap_ = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap_.add_argument("--l1-run", type=Path, required=True)
    ap_.add_argument("--tasks", type=Path,
                     default=HERE / "results" / "l2-cpu-qwen2.5-coder-7b-v1" / "snapshots" / "reader-tasks.json")
    ap_.add_argument("--name")
    ap_.add_argument("--out", type=Path, default=HERE / "runs-l2")
    ap_.add_argument("--repeats", type=int, default=3)
    ap_.add_argument("--resume", action="store_true")
    args = ap_.parse_args()

    try:
        cfg = reader.ReaderConfig.from_env()
    except reader.ReaderUnavailable as exc:
        print(f"reader unavailable: {exc}")
        return 3
    meter = cfg.tokenizer or "o200k"
    qbin = str(qodec.binary())

    run_id = args.name or _dt.datetime.now(_dt.timezone.utc).strftime("ablation-%Y%m%dT%H%M%SZ")
    run_dir = args.out / run_id
    run_dir.mkdir(parents=True, exist_ok=True)

    pf = preflight.run(cfg, meter)
    preflight.save(pf, run_dir / ("preflight.json" if not args.resume else "preflight-resume.json"))
    if not pf["ready"]:
        print(f"preflight not ready — see {run_dir}/preflight.json")
        return 4
    eff = preflight.effective_from(pf)

    tasks = reader_tasks.load_tasks(args.tasks)
    task_by = {(t["case"], t["id"]): t for t in tasks}
    brief = qodec.notation()
    targets = LOSSES + CONTROLS

    # Per (case, arm): build the arm payload ONCE (deterministic), with a stage
    # receipt + the aliases used (for leak scoring). Cache by case+arm.
    ctx: dict[str, dict] = {}
    arm_art: dict[tuple, ap.ArmResult] = {}
    for case in dict.fromkeys(c for c, _ in targets):
        tool_only = _tool_only_text(args.l1_run, case)
        ctx[case] = tool_only
        arms = ap.encode_all_arms(tool_only, meter, qbin)
        for name, res in arms.items():
            arm_art[(case, name)] = res

    def build_payload(case: str, arm: str) -> tuple[str, str, set]:
        res = arm_art[(case, arm)]
        if arm == "R":
            return "raw+brief", ctx[case], set()          # brief + raw
        aliases = set(ap.legend_of(res.artifact))
        used = {a for a in aliases if a in res.artifact}
        return "encoded+brief", res.artifact, used         # brief + arm artifact

    def run_one(case: str, q: dict, arm: str, repeat: int) -> dict:
        framing, payload, aliases = build_payload(case, arm)
        res_arm = arm_art[(case, arm)]
        msgs = reader_tasks.build_messages(framing, payload, brief, q["q"])
        r = reader.chat(cfg, msgs, eff)
        ans = reader_tasks.parse_answer(r.text)
        sc = reader_tasks.score_question(q, ans, source_text=ctx[case], aliases=aliases)
        return {
            "case": case, "question": q["id"], "category": q["category"], "arm": arm, "repeat": repeat,
            "correct": sc.correct, "format_compliant": ans is not None, "malformed": ans is None,
            "invalid_identifiers": sc.invalid_identifiers, "alias_leaks": sc.alias_leaks,
            "local_content_tokens": res_arm.tokens, "arm_receipt": res_arm.receipt,
            "server_prompt_tokens": r.usage.get("prompt_tokens"),
            "completion_tokens": r.usage.get("completion_tokens"),
            "ttft_ms": r.ttft_ms, "total_ms": r.total_ms, "http_error": r.http_error,
            "finish_reason": (r.response_meta or {}).get("finish_reason"),
            "answer_raw": r.text, "answer_parsed": ans, "request": r.request,
        }

    log = durability.RecordLog(run_dir / "records.jsonl")
    if args.resume and (run_dir / "records.jsonl").exists():
        log.load_existing()
        print(f"resume: {len(log.completed)} records present")
    log.open()

    def do(case, q, arm, repeat):
        key = (case, q["id"], arm, repeat)
        if not log.has(key):
            log.append(run_one(case, q, arm, repeat))
            durability.atomic_write(run_dir / "run-state.json", json.dumps(
                {"completed": len(log.records), "last": list(key)}, indent=2) + "\n", fsync=False)

    # Cache-friendly order: case → arm → question. All questions of a case share
    # ONE arm artifact (identical prefix; only the short question suffix differs),
    # so the server reuses the KV cache across them instead of re-evaluating a
    # ~4k-token prefix every request.
    qs_by_case: dict[str, list] = {}
    for case, qid in targets:
        qs_by_case.setdefault(case, []).append(task_by[(case, qid)])

    # Pass 1 — all 10 × 6 primary.
    for case, qlist in qs_by_case.items():
        for arm in ARMS:
            for q in qlist:
                do(case, q, arm, 0)

    # Pass 2 — repeats for all five losses × six arms, plus any control arm that
    # was flagged in primary (incorrect / malformed / leaked / invalid id).
    def flagged_control(case, qid) -> bool:
        for arm in ARMS:
            r = next((x for x in log.records if x["case"] == case and x["question"] == qid
                      and x["arm"] == arm and x["repeat"] == 0), None)
            if r and (not r["correct"] or r["malformed"] or r["alias_leaks"] or r["invalid_identifiers"]):
                return True
        return False

    repeat_targets = list(LOSSES) + [c for c in CONTROLS if flagged_control(*c)]
    rq_by_case: dict[str, list] = {}
    for case, qid in repeat_targets:
        rq_by_case.setdefault(case, []).append(task_by[(case, qid)])
    for repeat in range(1, max(1, args.repeats)):
        for case, qlist in rq_by_case.items():
            for arm in ARMS:
                for q in qlist:
                    do(case, q, arm, repeat)
    log.close()

    manifest = {
        "run_id": run_id, "kind": "alias-fold-ablation", "arms": ARMS,
        "losses": [f"{c}:{q}" for c, q in LOSSES],
        "controls": [f"{c}:{q}" for c, q in CONTROLS],
        "weakly_matched": [f"{c}:{q}" for c, q in WEAKLY_MATCHED],
        "model_requested": cfg.model, "model_reported": pf["models"].get("model_reported"),
        "model_identity": pf["model_identity"], "tokenizer": pf["tokenizer"],
        "qodec_version": qodec.version(), "qodec_binary_sha256": qodec.binary_sha256(),
        "effective": pf["effective"], "determinism": pf["determinism"],
        "reader_url": cfg.url, "max_tokens": cfg.max_tokens, "l1_run": str(args.l1_run),
        "arm_receipts": {f"{c}|{a}": arm_art[(c, a)].receipt for (c, a) in arm_art},
        "n_records": len(log.records),
    }
    durability.atomic_write(run_dir / "manifest.json", json.dumps(manifest, indent=2) + "\n")
    print(f"wrote {run_dir}  ({len(log.records)} records)")
    print(f"analyze with: python3 analyze_ablation.py --run {run_dir}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
