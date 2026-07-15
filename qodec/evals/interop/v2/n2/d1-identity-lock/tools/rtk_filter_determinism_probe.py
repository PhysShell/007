#!/usr/bin/env python3
"""N2-D1b part 3: RTK filter determinism probe.

Mirrors the proven pattern in qodec/evals/interop/v2/n2/canary/tools/
determinism_probe.py (N2-A.1's MSBuild producer-ordering probe): run the
SAME frozen invocation N independent times over the SAME frozen input,
record exit code + stdout/stderr SHA256 + o200k token count for each
repetition, and require exact agreement across all N before a filter may be
selected for any N2-D case of the matching content shape.

Never used to justify `--filter log` (prohibited outright by the accepted
N1 pilot nondeterminism evidence, regardless of what this probe would show
if run against it) and never run against a case shape without first
confirming, from the real per-case applicability map (Part 3's filter
inventory), that the filter's documented input grammar matches that case.
"""
from __future__ import annotations

import hashlib
import json
import subprocess


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def run_one(rtk_bin: str, argv_tail: list[str], input_bytes: bytes, timeout_s: int) -> dict:
    proc = subprocess.run(
        [rtk_bin, *argv_tail], input=input_bytes,
        stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=timeout_s,
    )
    return {
        "exit_code": proc.returncode,
        "stdout_sha256": sha256_bytes(proc.stdout),
        "stderr_sha256": sha256_bytes(proc.stderr),
        "stdout_bytes": len(proc.stdout),
        "stdout": proc.stdout,
    }


def count_tokens(qodec_bin: str, data: bytes, timeout_s: int) -> int | None:
    """Reuses qodec's own o200k meter (N2-D1's uniform tokenizer, per
    n2d1-contract.json section_4) as the token counter for each repetition's
    output -- never a separate/second tokenizer."""
    proc = subprocess.run(
        [qodec_bin, "encode", "--codec", "fold-grep-guarded", "--meter", "o200k",
         "--passthrough-on-no-gain", "--json"],
        input=data, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=timeout_s,
    )
    if proc.returncode != 0:
        return None
    return json.loads(proc.stdout.decode("utf-8"))["tokens_in"]


def probe_filter(*, rtk_bin: str, qodec_bin: str, filter_argv_tail: list[str], input_bytes: bytes,
                  repeats: int, timeout_s: int = 60) -> dict:
    repetitions = []
    for _ in range(repeats):
        rep = run_one(rtk_bin, filter_argv_tail, input_bytes, timeout_s)
        token_count = count_tokens(qodec_bin, rep["stdout"], timeout_s) if rep["exit_code"] == 0 else None
        repetitions.append({
            "exit_code": rep["exit_code"], "stdout_sha256": rep["stdout_sha256"],
            "stderr_sha256": rep["stderr_sha256"], "stdout_bytes": rep["stdout_bytes"],
            "o200k_token_count": token_count,
        })

    exit_codes = {r["exit_code"] for r in repetitions}
    stdout_hashes = {r["stdout_sha256"] for r in repetitions}
    token_counts = {r["o200k_token_count"] for r in repetitions}
    deterministic = (
        len(exit_codes) == 1 and 0 in exit_codes
        and len(stdout_hashes) == 1
        and len(token_counts) == 1 and None not in token_counts
    )
    return {
        "filter_argv_tail": filter_argv_tail,
        "repeats": repeats,
        "distinct_exit_codes": sorted(exit_codes),
        "distinct_stdout_sha256": sorted(stdout_hashes),
        "distinct_o200k_token_counts": sorted(c for c in token_counts if c is not None),
        "deterministic": deterministic,
        "repetitions": repetitions,
    }


def main() -> int:
    import argparse
    from pathlib import Path

    ap = argparse.ArgumentParser()
    ap.add_argument("--rtk-bin", required=True)
    ap.add_argument("--qodec-bin", required=True)
    ap.add_argument("--input-file", required=True)
    ap.add_argument("--filter-argv-tail", required=True, help="comma-separated, e.g. 'pipe,--filter,cargo-test'")
    ap.add_argument("--repeats", type=int, default=20)
    ap.add_argument("--out", required=True)
    args = ap.parse_args()

    input_bytes = Path(args.input_file).read_bytes()
    result = probe_filter(
        rtk_bin=args.rtk_bin, qodec_bin=args.qodec_bin,
        filter_argv_tail=args.filter_argv_tail.split(","),
        input_bytes=input_bytes, repeats=args.repeats,
    )
    Path(args.out).write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
    print(json.dumps({"deterministic": result["deterministic"],
                       "distinct_stdout_sha256": result["distinct_stdout_sha256"]}, indent=2))
    return 0 if result["deterministic"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
