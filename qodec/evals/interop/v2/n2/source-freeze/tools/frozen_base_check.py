#!/usr/bin/env python3
"""N2-C frozen-base guard: verifies every N2-A/N2-A.1/N2-B frozen path is
byte-identical to BASE_MAIN_SHA (the accepted N2-A.1 merge commit N2-C
branched from). A NEW, wider path list than N2-B's own frozen-base check
(qodec/evals/interop/v2/n2/miner/tools/generate_ci_artifacts.py,
frozen/unchanged) — that check verified N2-A's state relative to its own
predecessor base; this one additionally freezes N2-B's own tree plus both
prior workflow files, since N2-C's job is to build strictly beside all of
them, never edit them.
"""
from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

BASE_MAIN_SHA = "7d4dc3aabf760c4df272cf13a7e17ea437c81490"  # accepted N2-A.1 merge into main

FROZEN_PATHS = [
    "qodec/evals/interop/v2/coverage-matrix.json",
    "qodec/evals/interop/v2/benchmark-contract.json",
    "qodec/evals/interop/v2/heldout-policy.md",
    "qodec/evals/interop/v2/rtk-comparison-contract.json",
    "qodec/evals/interop/v2/schemas",
    "qodec/evals/interop/results",
    "qodec/src",
    "flake.lock",
    "qodec/evals/interop/v2/corpus",
    "qodec/evals/interop/v2/pilot",
    "qodec/evals/interop/v2/n2/canary",
    "qodec/evals/interop/v2/n2/miner",
    ".github/workflows/qodec-n2-miner-canary.yml",
    ".github/workflows/qodec-n2-miner-framework.yml",
]

ACCEPTED_SANDBOY_COMMIT_SHA = "e925058ddea405b5821fc0aed4882c76650dcbe9"


def _git(repo_root: Path, *args: str) -> subprocess.CompletedProcess:
    return subprocess.run(["git", "-C", str(repo_root), *args], capture_output=True, text=True)


def check(repo_root: Path) -> dict:
    drift = [p for p in FROZEN_PATHS if _git(repo_root, "diff", "--quiet", BASE_MAIN_SHA, "--", p).returncode != 0]
    sys.path.insert(0, str(repo_root / "qodec" / "evals" / "interop" / "v2" / "n2" / "miner" / "tools"))
    import sandbox_planner  # noqa: E402
    sandboy_pin_ok = sandbox_planner.ACCEPTED_SANDBOY_COMMIT_SHA == ACCEPTED_SANDBOY_COMMIT_SHA
    return {
        "base_main_sha": BASE_MAIN_SHA,
        "frozen_paths_checked": FROZEN_PATHS,
        "drift": drift,
        "sandboy_pin_unchanged": sandboy_pin_ok,
        "pass": not drift and sandboy_pin_ok,
    }


if __name__ == "__main__":
    import argparse

    ap = argparse.ArgumentParser()
    ap.add_argument("--repo-root", required=True)
    ap.add_argument("--out", required=True)
    args = ap.parse_args()

    report = check(Path(args.repo_root))
    Path(args.out).write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    if not report["pass"]:
        print(f"::error::N2-C frozen-base check failed: {report}", file=sys.stderr)
        sys.exit(1)
    print(f"N2-C frozen-base check passed: {report}")
