#!/usr/bin/env python3
"""run.py — Level 1: the artifact benchmark (no model).

The first of the three rungs in docs/token-codec.md's "qodec interop bench".
For each producer artifact it measures, purely on tokens and time:

    A            raw producer output
    qodec(A)     A run blindly through qodec (the raw home-stadium lane)
    tool(A)      an optimizer's output, when that optimizer is installed
    qodec(tool)  qodec layered after the optimizer

and reports the question that actually matters for interop:

    incremental_qodec_gain = 1 - tokens(tool_then_qodec) / tokens(tool_only)

i.e. did any tokenizer-visible redundancy survive the upstream optimizer? No
model is involved, so nothing here speaks to comprehension — that is Level 2
(reader) and Level 3 (agent). This rung exists to cheaply kill combinations
that do not even save tokens before a single model call is spent.

With no optimizer installed the tool lanes skip and the run still produces the
raw-vs-qodec numbers over the corpus — the reproducible baseline every later
rung is compared against.

Usage:
    python3 run.py                       # corpus cases, ./runs/<id>/
    python3 run.py --manifest cases.json --name my-run --codec deep
"""

from __future__ import annotations

import argparse
import datetime as _dt
import json
from pathlib import Path

from adapters import OPTIMIZERS, ToolUnavailable
from adapters import qodec as qodec_adapter
from doctor import build_receipt

HERE = Path(__file__).resolve().parent
CRATE_ROOT = HERE.parents[1]  # qodec/


def _default_cases() -> list[dict]:
    """The corpus is a stand-in for real producer output: msbuild logs, .NET
    stack traces, ripgrep hits, a git diff, a uniform findings array, and one
    unique-prose control. Each declares which optimizers *would* precede qodec
    on its lane, so the tool arms light up the moment those tools are installed.
    """
    return [
        {"id": "build-log", "lane": "command-output", "kind": "cargo",
         "path": "corpus/build-log.txt", "optimizers": ["rtk", "headroom"]},
        {"id": "stacktrace", "lane": "command-output", "kind": "test",
         "path": "corpus/stacktrace.txt", "optimizers": ["rtk", "headroom"]},
        {"id": "rg-output", "lane": "retrieval", "kind": "grep",
         "path": "corpus/rg-output.txt", "optimizers": ["rtk", "headroom"]},
        {"id": "git-diff", "lane": "command-output", "kind": "git",
         "path": "corpus/git-diff.txt", "optimizers": ["rtk", "headroom"]},
        {"id": "findings", "lane": "findings", "kind": "json", "json": True,
         "path": "corpus/findings.json", "optimizers": ["headroom", "fastcontext"]},
        {"id": "prose", "lane": "control", "kind": "prose",
         "path": "corpus/prose.md", "optimizers": ["headroom"]},
    ]


def _resolve(path: str) -> Path:
    p = Path(path)
    return p if p.is_absolute() else CRATE_ROOT / p


def _roundtrip_ok(original: str, env: qodec_adapter.Encoded, is_json: bool) -> bool:
    back, _ = qodec_adapter.decode(env.content)
    if back == original:
        return True
    if is_json:
        try:
            return json.loads(back) == json.loads(original)
        except json.JSONDecodeError:
            return False
    return False


def _classify(env: qodec_adapter.Encoded) -> str:
    """Token-level verdict only. harm / wrong-layer need comprehension (L2+)."""
    if env.is_fallback:
        return "passthrough"
    if env.gain >= 0.10:
        return "win"
    return "marginal"


def _measure(input_text: str, *, codec: str, meter: str, is_json: bool) -> dict:
    env = qodec_adapter.encode(input_text, codec=codec, meter=meter, passthrough=True)
    _, decode_ms = qodec_adapter.decode(env.content)
    return {
        "tokens_in": env.tokens_in,
        "tokens_out": env.tokens_out,
        "bytes_in": len(input_text.encode("utf-8")),
        "bytes_out": len(env.content.encode("utf-8")),
        "incremental_qodec_gain": round(env.gain, 4),
        "codec": env.codec,
        "is_fallback": env.is_fallback,
        "encode_ms": round(env.encode_ms, 2),
        "decode_ms": round(decode_ms, 2),
        "roundtrip_ok": _roundtrip_ok(input_text, env, is_json),
        "verdict": _classify(env),
    }


def run_case(case: dict, *, codec: str, meter: str) -> dict:
    text = _resolve(case["path"]).read_text()
    is_json = bool(case.get("json", False))
    record: dict = {
        "id": case["id"],
        "lane": case["lane"],
        "kind": case.get("kind", "text"),
        "arms": {},
        "skipped": [],
    }

    # Raw home-stadium lane: qodec applied directly to the producer output.
    record["arms"]["raw+qodec"] = _measure(text, codec=codec, meter=meter, is_json=is_json)

    # Tool lanes: optimizer first, then qodec over its output.
    for name in case.get("optimizers", []):
        mod = OPTIMIZERS.get(name)
        if mod is None:
            record["skipped"].append({"optimizer": name, "reason": "unknown optimizer"})
            continue
        avail = mod.available()
        if not avail.ok:
            record["skipped"].append({"optimizer": name, "reason": avail.detail})
            continue
        try:
            optimized = mod.optimize(
                text, kind=case.get("kind", "text"),
                repo=case.get("repo"), query=case.get("query"),
                target=case.get("target"),
            )
        except ToolUnavailable as exc:
            record["skipped"].append({"optimizer": name, "reason": str(exc)})
            continue
        arm = _measure(optimized, codec=codec, meter=meter, is_json=is_json)
        # tokens the optimizer alone produced == the qodec arm's input.
        arm["tool_tokens"] = arm["tokens_in"]
        record["arms"][f"{name}+qodec"] = arm
    return record


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--manifest", type=Path, help="cases JSON (default: the corpus)")
    ap.add_argument("--codec", default="squeeze", help="qodec codec (default squeeze)")
    ap.add_argument("--meter", default="o200k", help="tokenizer meter (default o200k)")
    ap.add_argument("--name", help="run id (default: UTC timestamp)")
    ap.add_argument("--out", type=Path, default=HERE / "runs", help="runs directory")
    args = ap.parse_args()

    receipt = build_receipt()
    if not receipt["healthy"]:
        print("qodec is unhealthy — run `python3 doctor.py` and fix it first.")
        return 1

    if args.manifest:
        cases = json.loads(args.manifest.read_text())["cases"]
    else:
        cases = _default_cases()

    run_id = args.name or _dt.datetime.now(_dt.timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    run_dir = args.out / run_id
    run_dir.mkdir(parents=True, exist_ok=True)

    records = [run_case(c, codec=args.codec, meter=args.meter) for c in cases]

    meta = {
        "run_id": run_id,
        "level": 1,
        "codec": args.codec,
        "meter": args.meter,
        "n_cases": len(records),
        "setup": receipt,
    }
    (run_dir / "meta.json").write_text(json.dumps(meta, indent=2) + "\n")
    with (run_dir / "metrics.jsonl").open("w") as fh:
        for rec in records:
            fh.write(json.dumps(rec) + "\n")

    # Human summary + fail-loud on any roundtrip break (that is a codec bug).
    broke = []
    print(f"run {run_id}  (codec={args.codec}, meter={args.meter})")
    print(f"{'case':<12}{'lane':<15}{'arm':<18}{'tok_in':>7}{'tok_out':>8}{'gain':>8}  codec")
    for rec in records:
        for arm, m in rec["arms"].items():
            gain = f"{m['incremental_qodec_gain']*100:+.1f}%"
            print(f"{rec['id']:<12}{rec['lane']:<15}{arm:<18}"
                  f"{m['tokens_in']:>7}{m['tokens_out']:>8}{gain:>8}  {m['codec']}")
            if not m["roundtrip_ok"]:
                broke.append((rec["id"], arm))
        for sk in rec["skipped"]:
            print(f"{rec['id']:<12}{rec['lane']:<15}{sk['optimizer']+' (skip)':<18}"
                  f"{'':>7}{'':>8}{'':>8}  {sk['reason']}")
    print(f"\nwrote {run_dir}/metrics.jsonl + meta.json")
    if broke:
        print(f"ROUNDTRIP FAILURES (codec bug): {broke}")
        return 2
    print("all roundtrips byte/Value-exact. Score with: python3 score.py " + str(run_dir))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
