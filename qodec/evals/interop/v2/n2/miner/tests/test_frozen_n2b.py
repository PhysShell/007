"""Frozen-base guard for Scope N2-B (section 2 + section 24 acceptance
contract): everything N2-A, N1, N0, M/M1 already froze must stay
byte-identical, and Scope N2-B's own diff must be purely additive
(qodec/evals/interop/v2/n2/miner/ + one new workflow file)."""
import subprocess
import sys
import unittest
from pathlib import Path

BASE = "d7fd03fdc6fcbf731de81d538ab0f7bca512a607"  # accepted N2-A merge into 007 main
ACCEPTED_SANDBOY_COMMIT_SHA = "e925058ddea405b5821fc0aed4882c76650dcbe9"

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
    ".github/workflows/qodec-n2-miner-canary.yml",
]


def _repo_root() -> Path:
    r = subprocess.run(["git", "rev-parse", "--show-toplevel"], cwd=str(Path(__file__).resolve().parent),
                       capture_output=True, text=True, check=True)
    return Path(r.stdout.strip())


REPO_ROOT = _repo_root()


def _git(*args):
    return subprocess.run(["git", "-C", str(REPO_ROOT), *args], capture_output=True, text=True)


@unittest.skipUnless(_git("cat-file", "-e", BASE).returncode == 0, f"base commit {BASE[:12]} not in history")
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


@unittest.skipUnless(_git("cat-file", "-e", BASE).returncode == 0, f"base commit {BASE[:12]} not in history")
class TestN2BDiffIsPurelyAdditive(unittest.TestCase):
    def test_no_deletions_relative_to_base(self):
        r = _git("diff", "--diff-filter=D", "--name-only", BASE, "HEAD")
        self.assertEqual(r.stdout.strip(), "", f"N2-B must never delete files present at {BASE[:12]}")

    def test_changed_files_confined_to_allowed_areas(self):
        r = _git("diff", "--name-only", BASE, "HEAD")
        changed = [line for line in r.stdout.splitlines() if line.strip()]
        allowed_prefixes = (
            "qodec/evals/interop/v2/n2/miner/",
            ".github/workflows/qodec-n2-miner-framework.yml",
        )
        offenders = [f for f in changed if not f.startswith(allowed_prefixes)]
        self.assertEqual(offenders, [], f"N2-B touched files outside its allowed additive scope: {offenders}")


class TestSandboyPinUnchanged(unittest.TestCase):
    def test_accepted_sandboy_commit_sha_is_still_the_s0_accepted_value(self):
        sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "tools"))
        import sandbox_planner  # noqa: E402
        self.assertEqual(sandbox_planner.ACCEPTED_SANDBOY_COMMIT_SHA, ACCEPTED_SANDBOY_COMMIT_SHA)


if __name__ == "__main__":
    unittest.main()
