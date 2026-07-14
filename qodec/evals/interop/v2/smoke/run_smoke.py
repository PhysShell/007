#!/usr/bin/env python3
"""Non-scoring smoke runner for the RTK×qodec comparison substrate.

NON-BENCHMARK · NON-GATING · NOT part of the 48 base cases · NOT held-out.

Proves the plumbing: qodec is lossless over arbitrary (and RTK-shaped) input,
token accounting uses the real target tokenizer via qodec's `--json` envelope,
and every execution is recorded with an identity receipt. It runs **no model**.

RTK arms run only when a pinned RTK binary is supplied (`--rtk` / `$RTK_BIN`);
otherwise they are recorded as skipped. The report is written to an output
directory and must NOT be committed.

Usage:
    python run_smoke.py --qodec /path/to/qodec [--rtk /path/to/rtk] \
        --meter o200k --out /tmp/smoke-out
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import subprocess
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
FIXTURES = HERE / "fixtures"
CODEC = "fold-grep-guarded"  # the frozen VG policy codec


def sha256_bytes(b: bytes) -> str:
    return hashlib.sha256(b).hexdigest()


def run(cmd: list[str], stdin: bytes) -> dict:
    """Run a command, capturing stream digests and an execution receipt."""
    t0 = time.monotonic()
    p = subprocess.run(cmd, input=stdin, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    wall = time.monotonic() - t0
    return {
        "command": cmd,
        "cwd": os.getcwd(),
        "stdin_sha256": sha256_bytes(stdin),
        "stdout_sha256": sha256_bytes(p.stdout),
        "stderr_sha256": sha256_bytes(p.stderr),
        "exit_code": p.returncode,
        "wall_time_s": round(wall, 6),
        "stdout": p.stdout,
        "stderr": p.stderr,
    }


def qodec_envelope(qodec_bin: str, text: bytes, meter: str) -> tuple[dict, dict]:
    """Encode via the frozen VG codec, returning (envelope, receipt)."""
    cmd = [qodec_bin, "encode", "--codec", CODEC, "--meter", meter,
           "--passthrough-on-no-gain", "--json"]
    rec = run(cmd, text)
    if rec["exit_code"] != 0:
        raise RuntimeError(f"qodec encode failed: {rec['stderr'][:400]!r}")
    env = json.loads(rec["stdout"].decode("utf-8").strip())
    return env, rec


def qodec_decode(qodec_bin: str, content: str) -> tuple[bytes, dict]:
    cmd = [qodec_bin, "decode"]
    rec = run(cmd, content.encode("utf-8"))
    if rec["exit_code"] != 0:
        raise RuntimeError(f"qodec decode failed: {rec['stderr'][:400]!r}")
    return rec["stdout"], rec


def rtk_reduce(rtk_bin: str, text: bytes) -> tuple[bytes, dict]:
    """Run RTK as a filter (stdin -> reduced stdout). Best-effort invocation."""
    rec = run([rtk_bin], text)
    return rec["stdout"], rec


def token_count(qodec_bin: str, text: bytes, meter: str) -> int:
    env, _ = qodec_envelope(qodec_bin, text, meter)
    return int(env["tokens_in"])


def qodec_arm(qodec_bin: str, raw: bytes, meter: str) -> dict:
    """qodec over `raw`: measure tokens and verify lossless roundtrip."""
    env, enc_rec = qodec_envelope(qodec_bin, raw, meter)
    content = env["content"]
    if env["encoded"]:
        decoded, dec_rec = qodec_decode(qodec_bin, content)
    else:
        decoded, dec_rec = content.encode("utf-8"), None
    roundtrip_ok = decoded == raw
    return {
        "tokens_in": int(env["tokens_in"]),
        "tokens_out": int(env["tokens_out"]),
        "codec": env["codec"],
        "encoded": env["encoded"],
        "roundtrip_ok": roundtrip_ok,
        "encode_receipt": _strip(enc_rec),
        "decode_receipt": _strip(dec_rec) if dec_rec else None,
    }


def _strip(rec: dict | None) -> dict | None:
    if rec is None:
        return None
    return {k: v for k, v in rec.items() if k not in ("stdout", "stderr")}


def binary_identity(path: str | None, source_sha: str | None) -> dict:
    ident = {"binary_path": path, "source_sha": source_sha}
    if path and Path(path).exists():
        ident["binary_sha256"] = sha256_bytes(Path(path).read_bytes())
    else:
        ident["binary_sha256"] = None
    return ident


def smoke(qodec_bin: str, rtk_bin: str | None, meter: str,
          qodec_source_sha: str | None, rtk_source_sha: str) -> dict:
    fixtures = sorted(FIXTURES.glob("*"))
    results = []
    invariants = []

    def check(name: str, ok: bool, detail: str = ""):
        invariants.append({"invariant": name, "ok": bool(ok), "detail": detail})

    for fx in fixtures:
        raw = fx.read_bytes()
        entry = {"fixture": fx.name, "raw_sha256": sha256_bytes(raw)}

        # raw arm
        raw_tokens = token_count(qodec_bin, raw, meter)
        entry["raw"] = {"tokens": raw_tokens}

        # qodec arm
        q = qodec_arm(qodec_bin, raw, meter)
        entry["qodec"] = q
        check(f"decode(qodec(raw))==raw [{fx.name}]", q["roundtrip_ok"])
        check(f"qodec_tokens<=raw_tokens [{fx.name}]",
              q["tokens_out"] <= raw_tokens,
              f"{q['tokens_out']} <= {raw_tokens}")

        # rtk arms (only with a pinned RTK binary)
        if rtk_bin and Path(rtk_bin).exists():
            reduced, rtk_rec = rtk_reduce(rtk_bin, raw)
            rtk_tokens = token_count(qodec_bin, reduced, meter)
            entry["rtk"] = {"tokens": rtk_tokens, "receipt": _strip(rtk_rec),
                            "reduced_sha256": sha256_bytes(reduced)}
            h = qodec_arm(qodec_bin, reduced, meter)
            entry["rtk+qodec"] = h
            check(f"decode(qodec(rtk(raw)))==rtk(raw) [{fx.name}]", h["roundtrip_ok"])
            check(f"hybrid_tokens<=rtk_tokens [{fx.name}]",
                  h["tokens_out"] <= rtk_tokens,
                  f"{h['tokens_out']} <= {rtk_tokens}")
        else:
            entry["rtk"] = {"skipped": "rtk-binary-unavailable"}
            entry["rtk+qodec"] = {"skipped": "rtk-binary-unavailable"}

        results.append(entry)

    report = {
        "kind": "NON-BENCHMARK-SMOKE",
        "gating": False,
        "meter": meter,
        "codec": CODEC,
        "qodec_identity": binary_identity(qodec_bin, qodec_source_sha),
        "rtk_identity": binary_identity(rtk_bin, rtk_source_sha),
        "results": results,
        "invariants": invariants,
        "all_invariants_ok": all(i["ok"] for i in invariants),
    }
    return report


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description="Non-scoring RTK×qodec smoke runner.")
    ap.add_argument("--qodec", default=os.environ.get("QODEC_BIN", "qodec"))
    ap.add_argument("--rtk", default=os.environ.get("RTK_BIN"))
    ap.add_argument("--meter", default="o200k")
    ap.add_argument("--out", default=os.environ.get("SMOKE_OUT", "smoke-out"))
    ap.add_argument("--qodec-source-sha", default=os.environ.get("QODEC_SOURCE_SHA"))
    ap.add_argument("--rtk-source-sha",
                    default=os.environ.get("RTK_SOURCE_SHA",
                                           "5d32d0736f686b69d1e8b9dc45c007d4eb77a0a2"))
    args = ap.parse_args(argv)

    report = smoke(args.qodec, args.rtk, args.meter,
                   args.qodec_source_sha, args.rtk_source_sha)

    out_dir = Path(args.out)
    out_dir.mkdir(parents=True, exist_ok=True)
    (out_dir / "smoke-report.json").write_text(json.dumps(report, indent=2))

    for inv in report["invariants"]:
        flag = "ok " if inv["ok"] else "FAIL"
        print(f"[{flag}] {inv['invariant']} {inv['detail']}")
    print(f"\nreport -> {out_dir / 'smoke-report.json'}")
    print("ALL INVARIANTS OK" if report["all_invariants_ok"] else "SMOKE FAILED")
    return 0 if report["all_invariants_ok"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
