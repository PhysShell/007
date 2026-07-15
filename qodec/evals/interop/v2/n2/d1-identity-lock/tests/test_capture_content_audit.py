"""Contract tests for the capture-content audit and acceptance-revocation
records. These lock in the real finding that all 18 captures from CI run
#6 (the run previously treated as Stage-2 acceptance evidence) are
content-invalid, and that the Stage-1/Stage-2 acceptance claims are
revoked pending a fail-closed content-acceptance gate and a from-scratch
re-run.
"""
import hashlib
import json
import sys
import unittest
from pathlib import Path

TOOLS = Path(__file__).resolve().parents[1] / "tools"
sys.path.insert(0, str(TOOLS))
import build_capture_content_audit as audit_builder  # noqa: E402
import build_evidence_revocation as revocation_builder  # noqa: E402

AUDIT_PATH = Path(__file__).resolve().parents[1] / "capture-content-audit-run6.json"
REVOCATION_PATH = Path(__file__).resolve().parents[1] / "stage1-and-stage2-acceptance-revocation.json"
STAGE1_EVIDENCE_PATH = Path(__file__).resolve().parents[1] / "stage1-pilot-evidence.json"

EXPECTED_CASE_IDS = {
    "repo-hyperfine", "repo-docker-java-parser", "repo-kubeops-generator",
    "repo-pyflakes", "repo-spotless", "repo-rustlings",
    "repo-dockerfile-parser-rs", "repo-requests", "repo-moshi",
}


def _self_hash_ok(record: dict) -> bool:
    without_hash = {k: v for k, v in record.items() if k != "record_sha256"}
    text = json.dumps(without_hash, indent=2, sort_keys=True) + "\n"
    return hashlib.sha256(text.encode()).hexdigest() == record["record_sha256"]


class TestCaptureContentAudit(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.record = json.loads(AUDIT_PATH.read_text())

    def test_self_hash_is_stable(self):
        self.assertTrue(_self_hash_ok(self.record))

    def test_builder_reproduces_identical_record(self):
        self.assertEqual(audit_builder.build_record(), self.record)

    def test_exactly_eighteen_captures(self):
        self.assertEqual(self.record["capture_count"], 18)
        self.assertEqual(len(self.record["captures"]), 18)

    def test_all_nine_cases_represented_with_both_captures(self):
        seen = {(c["case_id"], c["capture_id"]) for c in self.record["captures"]}
        for case_id in EXPECTED_CASE_IDS:
            self.assertIn((case_id, "capture-a"), seen)
            self.assertIn((case_id, "capture-b"), seen)

    def test_every_capture_is_content_invalid(self):
        # This is the whole point of the audit: every one of the 18 captures
        # from run #6 is content-invalid, not a mix of valid/invalid.
        self.assertEqual(self.record["content_invalid_count"], 18)
        self.assertEqual(self.record["content_valid_count"], 0)
        for c in self.record["captures"]:
            self.assertNotEqual(c["content_validity"], "VALID")

    def test_artifact_ids_are_unique(self):
        ids = [c["artifact_id"] for c in self.record["captures"]]
        self.assertEqual(len(ids), len(set(ids)))

    def test_every_root_cause_category_is_a_real_known_class(self):
        allowed = {
            "rustup-default-toolchain-unresolved-in-sandbox",
            "dev-null-missing-from-sandbox-policy",
            "python-venv-root-not-in-sandbox-policy",
            "dotnet-trusted-restore-missing-nuget-restore-attempted-under-network-denial",
        }
        for c in self.record["captures"]:
            self.assertIn(c["root_cause_category"], allowed)

    def test_dotnet_case_is_the_only_nonempty_canonical_input(self):
        for c in self.record["captures"]:
            if c["case_id"] == "repo-kubeops-generator":
                self.assertGreater(c["canonical_input_byte_size"], 0)
            else:
                self.assertEqual(c["canonical_input_byte_size"], 0)


class TestAcceptanceRevocation(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.record = json.loads(REVOCATION_PATH.read_text())
        cls.stage1_evidence = json.loads(STAGE1_EVIDENCE_PATH.read_text())

    def test_self_hash_is_stable(self):
        self.assertTrue(_self_hash_ok(self.record))

    def test_builder_reproduces_identical_record(self):
        self.assertEqual(revocation_builder.build_record(), self.record)

    def test_stage1_evidence_file_is_named_as_revoked(self):
        revoked_paths = {r["path"] for r in self.record["revoked_records"]}
        self.assertIn("qodec/evals/interop/v2/n2/d1-identity-lock/stage1-pilot-evidence.json", revoked_paths)

    def test_revoked_record_hash_matches_the_real_committed_file(self):
        # The revocation must reference the actual, unmodified stage1 file's
        # own self-hash -- not a stale or invented value.
        entry = next(r for r in self.record["revoked_records"]
                     if r["path"] == "qodec/evals/interop/v2/n2/d1-identity-lock/stage1-pilot-evidence.json")
        self.assertEqual(entry["record_sha256"], self.stage1_evidence["record_sha256"])

    def test_stage1_pilot_evidence_json_itself_was_not_modified(self):
        # Revocation must not rewrite history -- the original file's self-hash
        # must still verify against its own committed content.
        self.assertTrue(_self_hash_ok(self.stage1_evidence))
        self.assertEqual(self.stage1_evidence["status"], "STAGE_1_ACCEPTED_COMPLETE")

    def test_stage_statuses_explicitly_not_accepted(self):
        statuses = self.record["effective_status_changes"]
        self.assertIn("NOT ACCEPTED", statuses["stage_1_five_ecosystem_pilot"])
        self.assertIn("NOT ACCEPTED", statuses["stage_2_full_nine_case_matrix"])

    def test_downstream_scopes_remain_blocked(self):
        for item in ("N2-D1b.3 (RTK filter applicability inventory + determinism probes)",
                     "N2-D1b.4 (canonical Nix build identity)", "N2-D2", "N2-D3"):
            self.assertIn(item, self.record["not_yet_authorized_pending_re_acceptance"])


if __name__ == "__main__":
    unittest.main()
