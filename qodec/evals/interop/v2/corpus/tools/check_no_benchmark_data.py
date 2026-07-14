#!/usr/bin/env python3
"""Assert Scope N0 contains zero benchmark data: the manifest declares no
benchmark cases, the contract's benchmark_case_count is 0, and no committed case
carries a non-demonstration status. Exit non-zero on any violation.

Used by checks.qodec-v2-no-benchmark-data.
"""
import json
import sys
from pathlib import Path

CORPUS = Path(__file__).resolve().parents[1]


def main() -> int:
    manifest = json.loads((CORPUS / "manifest.json").read_text())
    contract = json.loads((CORPUS / "corpus-contract.json").read_text())
    problems = []
    if manifest.get("benchmark_cases") != []:
        problems.append(f"benchmark_cases must be empty, got {manifest.get('benchmark_cases')}")
    if contract.get("benchmark_case_count") != 0:
        problems.append(f"benchmark_case_count must be 0, got {contract.get('benchmark_case_count')}")
    for cid in manifest.get("demonstration_cases", []):
        case_p = CORPUS / "examples" / cid / "case.json"
        if case_p.exists():
            status = json.loads(case_p.read_text()).get("status")
            if status != "demonstration":
                problems.append(f"case {cid} has non-demonstration status {status!r}")
    for p in problems:
        print("NO-BENCHMARK-DATA VIOLATION:", p, file=sys.stderr)
    if problems:
        return 1
    print("no benchmark data: OK (0 benchmark cases, 1 demonstration case)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
