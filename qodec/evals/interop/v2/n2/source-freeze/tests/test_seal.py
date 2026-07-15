"""Section 23/19 tests: pre-QODEC seal contains no benchmark metrics, no
QODEC/RTK/model-call markers anywhere in the registry, manifests, or
workflow file."""
import sys
import unittest
from pathlib import Path

TOOLS = Path(__file__).resolve().parents[1] / "tools"
sys.path.insert(0, str(TOOLS))
import generate_ci_reports  # noqa: E402
import registry as registry_mod  # noqa: E402

SOURCE_FREEZE_DIR = Path(__file__).resolve().parents[1]


def _repo_root() -> Path:
    import subprocess
    r = subprocess.run(["git", "rev-parse", "--show-toplevel"], cwd=str(Path(__file__).resolve().parent),
                       capture_output=True, text=True, check=True)
    return Path(r.stdout.strip())


class TestSealCheck(unittest.TestCase):
    def test_real_repo_is_sealed(self):
        import argparse
        args = argparse.Namespace(repo_root=str(_repo_root()), out="/tmp/n2c-seal-test.json")
        rc = generate_ci_reports.cmd_seal_check(args)
        self.assertEqual(rc, 0)

    def test_candidate_registry_has_no_benchmark_output_fields(self):
        reg = registry_mod.load_registry(SOURCE_FREEZE_DIR / "candidate-registry.json")
        self.assertEqual(registry_mod.validate_no_forbidden_fields(reg), [])

    def test_workflow_imports_no_qodec_or_rtk_runner(self):
        workflow_text = (_repo_root() / ".github" / "workflows" / "qodec-n2-source-freeze.yml").read_text()
        for marker in ("qodec_runner", "rtk_runner", "run-qodec", "run-rtk"):
            self.assertNotIn(marker, workflow_text.lower())

    def test_workflow_performs_no_model_call(self):
        workflow_text = (_repo_root() / ".github" / "workflows" / "qodec-n2-source-freeze.yml").read_text()
        for marker in ("anthropic", "openai", "model_call"):
            self.assertNotIn(marker, workflow_text.lower())


if __name__ == "__main__":
    unittest.main()
