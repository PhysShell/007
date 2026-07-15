"""Section 23 tests: N2-A/N2-A.1/N2-B frozen paths unchanged, and the
section-19 sealing checks."""
import subprocess
import sys
import unittest
from pathlib import Path

TOOLS = Path(__file__).resolve().parents[1] / "tools"
sys.path.insert(0, str(TOOLS))
import frozen_base_check  # noqa: E402


def _repo_root() -> Path:
    r = subprocess.run(["git", "rev-parse", "--show-toplevel"], cwd=str(Path(__file__).resolve().parent),
                       capture_output=True, text=True, check=True)
    return Path(r.stdout.strip())


REPO_ROOT = _repo_root()


def _git(*args):
    return subprocess.run(["git", "-C", str(REPO_ROOT), *args], capture_output=True, text=True)


@unittest.skipUnless(_git("cat-file", "-e", frozen_base_check.BASE_MAIN_SHA).returncode == 0,
                      f"base commit {frozen_base_check.BASE_MAIN_SHA[:12]} not in history")
class TestFrozenPathsUnchanged(unittest.TestCase):
    def test_report_passes_for_current_tree(self):
        report = frozen_base_check.check(REPO_ROOT)
        self.assertEqual(report["drift"], [])
        self.assertTrue(report["pass"])

    def test_n2a_canary_path_is_checked(self):
        self.assertIn("qodec/evals/interop/v2/n2/canary", frozen_base_check.FROZEN_PATHS)

    def test_n2b_miner_path_is_checked(self):
        self.assertIn("qodec/evals/interop/v2/n2/miner", frozen_base_check.FROZEN_PATHS)

    def test_both_prior_workflow_files_are_checked(self):
        self.assertIn(".github/workflows/qodec-n2-miner-canary.yml", frozen_base_check.FROZEN_PATHS)
        self.assertIn(".github/workflows/qodec-n2-miner-framework.yml", frozen_base_check.FROZEN_PATHS)

    def test_sandboy_pin_unchanged(self):
        report = frozen_base_check.check(REPO_ROOT)
        self.assertTrue(report["sandboy_pin_unchanged"])


if __name__ == "__main__":
    unittest.main()
