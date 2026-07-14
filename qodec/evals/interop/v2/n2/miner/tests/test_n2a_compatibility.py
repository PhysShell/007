"""Tests for n2a_compatibility.py — the N2-A compatibility gate (section 17).
Reads the frozen N2-A source-manifest.json but never modifies or re-executes it."""
import sys
import unittest
from pathlib import Path

MINER_DIR = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(MINER_DIR / "tools"))
import n2a_compatibility as n2ac  # noqa: E402


class TestN2ACompatibility(unittest.TestCase):
    def setUp(self):
        self.report = n2ac.compute_compatibility_report()

    def test_zero_unexplained_incompatibilities(self):
        self.assertEqual(self.report["incompatible_fields"], [])
        self.assertTrue(self.report["zero_unexplained_incompatibilities"])

    def test_reference_case_id_matches_n2a(self):
        self.assertEqual(self.report["reference_case_id"], "miner-canary-dotnet-001")

    def test_matched_fields_include_core_identity(self):
        for field in ("source_repository", "source_commit_sha", "project_entry_point",
                      "ecosystem", "build_argv_semantics", "accepted_sandboy_commit_sha"):
            self.assertIn(field, self.report["matched_fields"])

    def test_generalizations_are_explained_not_silent(self):
        self.assertTrue(self.report["intentionally_generalized_fields"])
        for entry in self.report["intentionally_generalized_fields"]:
            self.assertIn("field", entry)
            self.assertIn("explanation", entry)
            self.assertTrue(entry["explanation"])

    def test_missing_generic_capabilities_disclosed(self):
        self.assertTrue(self.report["missing_generic_capabilities"])

    def test_report_is_deterministic(self):
        report_a = n2ac.compute_compatibility_report()
        report_b = n2ac.compute_compatibility_report()
        self.assertEqual(report_a, report_b)


if __name__ == "__main__":
    unittest.main()
