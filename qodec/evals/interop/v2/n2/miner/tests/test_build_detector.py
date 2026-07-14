"""Tests for build_detector.py — ecosystem-agnostic dispatch across adapters."""
import sys
import unittest
from pathlib import Path

MINER_DIR = Path(__file__).resolve().parents[1]
FIXTURES = MINER_DIR / "fixtures"
sys.path.insert(0, str(MINER_DIR / "tools"))
import build_detector  # noqa: E402


class TestBuildDetector(unittest.TestCase):
    def test_dotnet_simple_detected_as_only_dotnet(self):
        report = build_detector.detect_all(FIXTURES / "dotnet_simple")
        self.assertEqual(report["ecosystems_detected"], ["dotnet"])
        self.assertFalse(report["ambiguous_ecosystem_selection"])

    def test_rust_workspace_detected_as_only_rust(self):
        report = build_detector.detect_all(FIXTURES / "rust_workspace")
        self.assertEqual(report["ecosystems_detected"], ["rust"])

    def test_python_project_detected_as_only_python(self):
        report = build_detector.detect_all(FIXTURES / "python_pytest_lock")
        self.assertEqual(report["ecosystems_detected"], ["python"])

    def test_maven_project_detected_as_only_jvm_maven(self):
        report = build_detector.detect_all(FIXTURES / "maven_multimodule")
        self.assertEqual(report["ecosystems_detected"], ["jvm-maven"])

    def test_gradle_project_detected_as_only_jvm_gradle(self):
        report = build_detector.detect_all(FIXTURES / "gradle_wrapper_custom_repo")
        self.assertEqual(report["ecosystems_detected"], ["jvm-gradle"])

    def test_ambiguous_within_ecosystem_still_reports_single_ecosystem(self):
        report = build_detector.detect_all(FIXTURES / "dotnet_ambiguous")
        self.assertEqual(report["ecosystems_detected"], ["dotnet"])
        self.assertTrue(report["per_ecosystem"]["dotnet"]["ambiguous"])


if __name__ == "__main__":
    unittest.main()
