"""Contract tests for the immutable N2-D1b Stage-1 pilot evidence record.

Rebuilt (2026-07-16) per the user's formal Stage-1 sign-off. This record
preserves the accepted, fully-green five-ecosystem pilot run (qodec-n2d1b-
miner-pilot run 29474805883, commit c51eacc) -- with repo-moshi in the
jvm-gradle slot and repo-spotless permanently excluded as
REJECTED_ACQUISITION_MODEL_INCOMPATIBLE -- as its own durable artifact.
Supersedes the prior record (run 29418422603, commit a68176b), which
included the since-rejected repo-spotless case and predates the
network-exception, deterministic-scheduling, and canonicalization-policy
work this record now reflects.
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

    def test_approving_decision_identity_is_the_formal_signoff(self):
        self.assertEqual(
            self.record["approving_decision_identity"],
            "n2d1b-stage1-acceptance-formal-signoff-2026-07-16",
        )

    def test_pr_state_recorded_as_draft_open_unmerged(self):
        self.assertEqual(self.record["pull_request"]["number"], 56)
        self.assertEqual(self.record["pull_request"]["state_at_acceptance"], "draft, open, unmerged")

    def test_exact_workflow_run_id_and_tested_head_sha(self):
        self.assertEqual(self.record["workflow"]["run_id"], 29474805883)
        self.assertEqual(self.record["tested_head_sha"], "c51eacca7edd9b73f58c740f5de31998304cf85c")
        self.assertEqual(self.record["workflow"]["conclusion"], "success")

    def test_superseded_record_identifies_the_stale_spotless_era_record(self):
        superseded = self.record["superseded_record"]
        self.assertEqual(superseded["workflow_run_id"], 29418422603)
        self.assertEqual(superseded["tested_head_sha"], "a68176bd1725ab46b9ff14c9694d4a622c95fe4d")

    def test_accepted_pilot_case_ids_include_moshi_not_spotless(self):
        case_ids = self.record["accepted_pilot_case_ids"]
        self.assertEqual(len(case_ids), 5)
        self.assertIn("repo-moshi", case_ids)
        self.assertNotIn("repo-spotless", case_ids)
        for expected in ("repo-hyperfine", "repo-docker-java-parser",
                         "repo-kubeops-generator", "repo-pyflakes"):
            self.assertIn(expected, case_ids)

    def test_exactly_ten_pilot_jobs_all_success(self):
        self.assertEqual(self.record["job_count"], 10)
        self.assertEqual(len(self.record["jobs"]), 10)
        self.assertTrue(self.record["all_job_conclusions_success"])
        for job in self.record["jobs"]:
            self.assertEqual(job["conclusion"], "success")

    def test_exactly_five_pair_verify_jobs_all_success(self):
        self.assertEqual(self.record["pair_verify_job_count"], 5)
        self.assertEqual(len(self.record["pair_verify_jobs"]), 5)
        self.assertTrue(self.record["all_pair_verify_job_conclusions_success"])
        for job in self.record["pair_verify_jobs"]:
            self.assertEqual(job["conclusion"], "success")

    def test_all_five_accepted_cases_have_pilot_capture_a_and_b_jobs(self):
        names = {j["name"] for j in self.record["jobs"]}
        for case_id in self.record["accepted_pilot_case_ids"]:
            self.assertIn(f"pilot-{case_id}-capture-a", names)
            self.assertIn(f"pilot-{case_id}-capture-b", names)

    def test_all_five_accepted_cases_have_a_pair_verify_job(self):
        names = {j["name"] for j in self.record["pair_verify_jobs"]}
        for case_id in self.record["accepted_pilot_case_ids"]:
            self.assertIn(f"pair-verify-{case_id}", names)

    def test_exactly_ten_capture_artifacts_with_sha256_digests(self):
        self.assertEqual(self.record["artifact_count"], 10)
        self.assertEqual(len(self.record["artifacts"]), 10)
        for artifact in self.record["artifacts"]:
            self.assertRegex(artifact["digest_sha256"], r"^[0-9a-f]{64}$")

    def test_exactly_five_pair_report_artifacts_with_sha256_digests(self):
        self.assertEqual(self.record["pair_report_artifact_count"], 5)
        self.assertEqual(len(self.record["pair_report_artifacts"]), 5)
        for artifact in self.record["pair_report_artifacts"]:
            self.assertRegex(artifact["digest_sha256"], r"^[0-9a-f]{64}$")

    def test_repo_spotless_rejection_recorded_and_excluded(self):
        rejection = self.record["repo_spotless_rejection"]
        self.assertEqual(rejection["classification"], "REJECTED_ACQUISITION_MODEL_INCOMPATIBLE")
        self.assertTrue(rejection["excluded_from_pilot_numerator_and_denominator"])

    def test_canonicalization_policy_hashes_recorded_for_maven_vstest_gradle(self):
        policies = self.record["canonicalization_policies"]
        self.assertEqual(policies["maven"]["applicable_case_ids"], ["repo-docker-java-parser"])
        self.assertEqual(policies["vstest"]["applicable_case_ids"], ["repo-kubeops-generator"])
        self.assertEqual(policies["vstest"]["policy_version"], 2)
        self.assertIn("msbuild_completion_pair_order", policies["vstest"]["structural_rules"][0])
        self.assertEqual(policies["gradle"]["applicable_case_ids"], ["repo-moshi"])
        for name in ("maven", "vstest", "gradle"):
            self.assertRegex(policies[name]["policy_sha256"], r"^[0-9a-f]{64}$")

    def test_network_enforcement_authorized_cases_excludes_spotless(self):
        authorized = self.record["network_enforcement_authorized_cases"]
        self.assertEqual(set(authorized), {"repo-kubeops-generator", "repo-moshi"})
        self.assertNotIn("repo-spotless", authorized)

    def test_moshi_deterministic_scheduling_profile_hash_recorded(self):
        self.assertRegex(
            self.record["moshi_deterministic_scheduling_profile_sha256"], r"^[0-9a-f]{64}$"
        )

    def test_local_test_count_recorded(self):
        self.assertEqual(self.record["local_test_suite_at_tested_head_sha"]["test_count"], 302)
        self.assertEqual(self.record["local_test_suite_at_tested_head_sha"]["result"], "OK")

    def test_prohibited_scopes_still_listed(self):
        for item in ("official Stage-2 leaderboard calculations pending full Stage-2 corpus, "
                     "RTK identity, and Nix identity",
                     "further argv errata beyond what is already authorized",
                     "modifications to frozen N2-C evidence"):
            self.assertIn(item, self.record["not_yet_authorized"])

    def test_builder_reproduces_identical_record(self):
        # The builder script is the single source of truth; regenerating it
        # must produce byte-identical output to the committed file.
        rebuilt = builder.build_record()
        self.assertEqual(rebuilt, self.record)


if __name__ == "__main__":
    unittest.main()
