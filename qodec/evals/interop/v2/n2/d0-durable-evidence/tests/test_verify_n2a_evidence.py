"""Unit tests (synthetic, no network) for verify_n2a_evidence.py."""
import json
import sys
import tempfile
import unittest
from pathlib import Path

TOOLS = Path(__file__).resolve().parents[1] / "tools"
sys.path.insert(0, str(TOOLS))
import verify_n2a_evidence as v  # noqa: E402


def _fetched(name, run_id="29384147131", head_sha=v.ACCEPTED_HEAD_SHA, digest_match=True):
    return {"artifact_name": name, "artifact_id": 1, "api_reported_digest_sha256": "d",
            "digest_match": digest_match, "workflow_run_id_of_artifact": run_id, "head_sha_of_artifact_run": head_sha}


class TestRunIdentity(unittest.TestCase):
    def test_all_present_and_matching_head(self):
        fetch_report = {"fetched": [_fetched(n) for n in v.REQUIRED_ARTIFACT_NAMES]}
        result = v.verify_run_identity(fetch_report)
        self.assertEqual(result["missing_required_artifacts"], [])
        self.assertTrue(result["head_sha_matches_accepted"])
        self.assertTrue(result["all_digests_verified"])

    def test_missing_artifact_detected(self):
        fetch_report = {"fetched": [_fetched(n) for n in v.REQUIRED_ARTIFACT_NAMES[:-1]]}
        result = v.verify_run_identity(fetch_report)
        self.assertEqual(result["missing_required_artifacts"], [v.REQUIRED_ARTIFACT_NAMES[-1]])

    def test_wrong_head_sha_detected(self):
        fetch_report = {"fetched": [_fetched(n, head_sha="deadbeef") for n in v.REQUIRED_ARTIFACT_NAMES]}
        result = v.verify_run_identity(fetch_report)
        self.assertFalse(result["head_sha_matches_accepted"])

    def test_digest_mismatch_detected(self):
        fetch_report = {"fetched": [_fetched(n, digest_match=False) for n in v.REQUIRED_ARTIFACT_NAMES]}
        result = v.verify_run_identity(fetch_report)
        self.assertFalse(result["all_digests_verified"])


class TestCaptureAgreement(unittest.TestCase):
    def test_agreeing_reports_pass(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            reports_dir = root / "miner-canary-reports"
            reports_dir.mkdir()
            (reports_dir / "reproducibility-report.json").write_text(json.dumps({"overall_reproducible": True}))
            result = v.verify_capture_agreement(root)
            self.assertTrue(result["agrees"])

    def test_disagreeing_reports_fail(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            reports_dir = root / "miner-canary-reports"
            reports_dir.mkdir()
            (reports_dir / "reproducibility-report.json").write_text(json.dumps({"overall_reproducible": False}))
            result = v.verify_capture_agreement(root)
            self.assertFalse(result["agrees"])

    def test_missing_report_fails(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "miner-canary-reports").mkdir()
            result = v.verify_capture_agreement(root)
            self.assertFalse(result["agrees"])
            self.assertIn("problem", result)


if __name__ == "__main__":
    unittest.main()
