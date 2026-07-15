"""Contract tests for the immutable N2-D1b Stage-1 pilot evidence record.

Preserves the accepted, fully-green five-ecosystem pilot run
(qodec-n2d1b-miner-pilot run 29418422603, commit a68176b) as its own durable
artifact -- these tests lock the facts the user required be recorded before
Stage 2 proceeds: exact run ID, tested head SHA, all 10 job names +
conclusions, artifact names + hashes, local test count, and the
all-five-ecosystem-lanes-passed statement.
"""
import hashlib
import json
import sys
import unittest
from pathlib import Path

TOOLS = Path(__file__).resolve().parents[1] / "tools"
sys.path.insert(0, str(TOOLS))
import build_stage1_pilot_evidence as builder  # noqa: E402

RECORD_PATH = Path(__file__).resolve().parents[1] / "stage1-pilot-evidence.json"


class TestStage1PilotEvidence(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.record = json.loads(RECORD_PATH.read_text())

    def test_self_hash_is_stable(self):
        without_hash = {k: v for k, v in self.record.items() if k != "record_sha256"}
        text = json.dumps(without_hash, indent=2, sort_keys=True) + "\n"
        recomputed = hashlib.sha256(text.encode()).hexdigest()
        self.assertEqual(recomputed, self.record["record_sha256"])

    def test_status_is_accepted_complete(self):
        self.assertEqual(self.record["status"], "STAGE_1_ACCEPTED_COMPLETE")

    def test_pr_state_recorded_as_draft_open_unmerged(self):
        self.assertEqual(self.record["pull_request"]["number"], 56)
        self.assertEqual(self.record["pull_request"]["state_at_acceptance"], "draft, open, unmerged")

    def test_exact_workflow_run_id_and_tested_head_sha(self):
        self.assertEqual(self.record["workflow"]["run_id"], 29418422603)
        self.assertEqual(self.record["tested_head_sha"], "a68176bd1725ab46b9ff14c9694d4a622c95fe4d")
        self.assertEqual(self.record["workflow"]["conclusion"], "success")

    def test_exactly_ten_jobs_all_success(self):
        self.assertEqual(self.record["job_count"], 10)
        self.assertEqual(len(self.record["jobs"]), 10)
        self.assertTrue(self.record["all_job_conclusions_success"])
        for job in self.record["jobs"]:
            self.assertEqual(job["conclusion"], "success")

    def test_all_five_ecosystem_lanes_represented_with_both_captures(self):
        names = {j["name"] for j in self.record["jobs"]}
        for ecosystem in ("rust", "jvm-maven", "jvm-gradle", "dotnet", "python"):
            self.assertIn(f"pilot-{ecosystem}-capture-a", names)
            self.assertIn(f"pilot-{ecosystem}-capture-b", names)

    def test_exactly_ten_artifacts_with_sha256_digests(self):
        self.assertEqual(self.record["artifact_count"], 10)
        self.assertEqual(len(self.record["artifacts"]), 10)
        for artifact in self.record["artifacts"]:
            self.assertRegex(artifact["digest_sha256"], r"^[0-9a-f]{64}$")

    def test_local_test_count_recorded(self):
        self.assertEqual(self.record["local_test_suite_at_tested_head_sha"]["test_count"], 67)
        self.assertEqual(self.record["local_test_suite_at_tested_head_sha"]["result"], "OK")

    def test_prohibited_scopes_still_listed(self):
        for item in ("N2-D2", "N2-D3", "token aggregation", "leaderboard calculations",
                     "case substitution", "further argv errata", "modifications to frozen N2-C evidence"):
            self.assertIn(item, self.record["not_yet_authorized"])

    def test_builder_reproduces_identical_record(self):
        # The builder script is the single source of truth; regenerating it
        # must produce byte-identical output to the committed file.
        rebuilt = builder.build_record()
        self.assertEqual(rebuilt, self.record)


if __name__ == "__main__":
    unittest.main()
