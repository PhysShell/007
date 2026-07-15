"""Static contract test for Scope N2-A.1 section 8: the canary workflow's
pull_request path trigger must be narrowed to qodec/evals/interop/v2/n2/canary/**
(+ the workflow file itself) so N2-B-only (miner/**) changes never
incidentally re-run this real, third-party-executing workflow — while
canary/** and workflow-file changes must still trigger it. Parsed with plain
text/regex, not PyYAML, since this repo carries no PyYAML dependency."""
import fnmatch
import re
import unittest
from pathlib import Path

WORKFLOW_PATH = (
    Path(__file__).resolve().parents[7] / ".github" / "workflows" / "qodec-n2-miner-canary.yml"
)

EXPECTED_PATHS = [
    "qodec/evals/interop/v2/n2/canary/**",
    ".github/workflows/qodec-n2-miner-canary.yml",
]


def _extract_pull_request_paths(text: str) -> list:
    m = re.search(r"pull_request:\s*\n\s*paths:\s*\n((?:\s*(?:#.*)?\n|\s*-\s*.+\n)+)", text)
    assert m, "could not locate on.pull_request.paths block in the workflow file"
    lines = [line.strip() for line in m.group(1).splitlines() if line.strip()]
    return [line.split("-", 1)[1].strip().strip('"') for line in lines if line.startswith("-")]


def _matches_any_pattern(rel_path: str, patterns: list) -> bool:
    return any(fnmatch.fnmatch(rel_path, pattern) for pattern in patterns)


class TestCanaryWorkflowTriggerContract(unittest.TestCase):
    def setUp(self):
        self.text = WORKFLOW_PATH.read_text()
        self.paths = _extract_pull_request_paths(self.text)

    def test_paths_are_narrowed_to_canary_and_workflow_file(self):
        self.assertEqual(sorted(self.paths), sorted(EXPECTED_PATHS))

    def test_old_broad_n2_glob_is_not_present(self):
        self.assertNotIn("qodec/evals/interop/v2/n2/**", self.paths)

    def test_workflow_dispatch_trigger_is_retained(self):
        self.assertIn("workflow_dispatch:", self.text)

    def test_canary_file_change_still_triggers(self):
        self.assertTrue(_matches_any_pattern(
            "qodec/evals/interop/v2/n2/canary/tools/determinism_probe.py", self.paths,
        ))

    def test_workflow_file_change_still_triggers(self):
        self.assertTrue(_matches_any_pattern(
            ".github/workflows/qodec-n2-miner-canary.yml", self.paths,
        ))

    def test_miner_only_change_does_not_trigger(self):
        self.assertFalse(_matches_any_pattern(
            "qodec/evals/interop/v2/n2/miner/tools/adapters/dotnet_adapter.py", self.paths,
        ))

    def test_miner_tests_only_change_does_not_trigger(self):
        self.assertFalse(_matches_any_pattern(
            "qodec/evals/interop/v2/n2/miner/tests/test_frozen_n2b.py", self.paths,
        ))


if __name__ == "__main__":
    unittest.main()
