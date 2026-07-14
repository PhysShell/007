"""Tests for acquisition_planner.py — AcquisitionPlanner (section 11)."""
import sys
import unittest
from pathlib import Path

MINER_DIR = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(MINER_DIR / "tools"))
import acquisition_planner as ap  # noqa: E402
from adapters import dotnet_adapter  # noqa: E402


class TestAcquisitionPlanner(unittest.TestCase):
    def setUp(self):
        self.candidate = {
            "candidate_id": "synthetic-dotnet-eligible",
            "repository": {"url": "https://github.com/example/foo", "owner": "example", "name": "foo"},
            "commit_sha": "a" * 40,
        }
        self.manifest = {"project": {"entry_point": "Foo/Foo.csproj"}}

    def test_plan_is_plan_only(self):
        plan = ap.plan_acquisition(self.candidate, self.manifest, dotnet_adapter)
        self.assertTrue(plan["plan_only"])
        self.assertIn("PLAN ONLY", plan["note"])

    def test_stages_separated(self):
        plan = ap.plan_acquisition(self.candidate, self.manifest, dotnet_adapter)
        for stage in ("trusted_acquisition", "trusted_dependency_realization",
                      "untrusted_execution", "artifact_collection"):
            self.assertIn(stage, plan["stages"])

    def test_trusted_acquisition_forbids_repository_script_execution(self):
        plan = ap.plan_acquisition(self.candidate, self.manifest, dotnet_adapter)
        forbidden = plan["stages"]["trusted_acquisition"]["forbidden_operations"]
        self.assertIn("execute_repository_script", forbidden)
        self.assertIn("build", forbidden)
        self.assertIn("test", forbidden)

    def test_persist_credentials_false(self):
        plan = ap.plan_acquisition(self.candidate, self.manifest, dotnet_adapter)
        self.assertFalse(plan["stages"]["trusted_acquisition"]["persist_credentials"])

    def test_immutable_commit_recorded(self):
        plan = ap.plan_acquisition(self.candidate, self.manifest, dotnet_adapter)
        self.assertEqual(plan["stages"]["trusted_acquisition"]["immutable_commit"], "a" * 40)

    def test_artifact_collection_forbids_re_execution(self):
        plan = ap.plan_acquisition(self.candidate, self.manifest, dotnet_adapter)
        forbidden = plan["stages"]["artifact_collection"]["forbidden_operations"]
        self.assertIn("re_execute_repository_code", forbidden)
        self.assertIn("re_fetch_source", forbidden)


if __name__ == "__main__":
    unittest.main()
