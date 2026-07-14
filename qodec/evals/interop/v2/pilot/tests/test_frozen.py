"""Frozen-base guard: Scope M / M1 / N0 artifacts must be byte-identical to the
accepted N0 head this pilot is based on. N1 adds only the pilot/ area, the pilot
CI workflow and the flake wiring — it never edits a frozen contract, gate,
schema, result, the production codec, the lockfile, the N0 demonstration bundle
or the RTK smoke fixtures.
"""
import subprocess
import sys
import unittest
from pathlib import Path

TOOLS = Path(__file__).resolve().parents[1] / "tools"
sys.path.insert(0, str(TOOLS))
import pilot_lib as pl  # noqa: E402

BASE = "529ca24557af9a45833025bf06e36e76800eb610"
FROZEN_PATHS = [
    "qodec/evals/interop/v2/coverage-matrix.json",
    "qodec/evals/interop/v2/benchmark-contract.json",
    "qodec/evals/interop/v2/heldout-policy.md",
    "qodec/evals/interop/v2/rtk-comparison-contract.json",
    "qodec/evals/interop/v2/schemas",
    "qodec/evals/interop/results",
    "qodec/src",
    "flake.lock",
    "qodec/evals/interop/v2/corpus/examples/deterministic-log-demo",
    "qodec/evals/interop/v2/corpus/tools",
    "qodec/evals/interop/v2/corpus/schemas",
    "qodec/evals/interop/v2/smoke",
]


def _git(*args):
    return subprocess.run(["git", "-C", str(pl.REPO_ROOT), *args],
                          capture_output=True, text=True)


@unittest.skipUnless(_git("cat-file", "-e", BASE).returncode == 0,
                     f"base commit {BASE[:12]} not in history")
class TestFrozenBase(unittest.TestCase):
    def test_frozen_paths_byte_identical_to_base(self):
        drift = []
        for p in FROZEN_PATHS:
            r = _git("diff", "--quiet", BASE, "--", p)
            if r.returncode != 0:
                drift.append(p)
        self.assertEqual(drift, [], f"frozen artifacts changed vs {BASE[:12]}: {drift}")

    def test_flake_lock_unchanged(self):
        self.assertEqual(_git("diff", "--quiet", BASE, "--", "flake.lock").returncode, 0)


if __name__ == "__main__":
    unittest.main()
