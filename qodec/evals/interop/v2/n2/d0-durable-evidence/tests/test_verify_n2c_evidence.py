"""Unit tests (synthetic, no network) for verify_n2c_evidence.py."""
import sys
import unittest
from pathlib import Path

TOOLS = Path(__file__).resolve().parents[1] / "tools"
sys.path.insert(0, str(TOOLS))
import verify_n2c_evidence as v  # noqa: E402


def _fetch_report(fetched):
    return {"run_id": "29404265568", "missing_artifacts": [], "fetched": fetched}


def _fetched_entry(name, artifact_id, digest, contained_files, run_id="29404265568", head_sha="acb5737"):
    return {
        "artifact_name": name, "artifact_id": artifact_id, "api_reported_size_in_bytes": 100,
        "local_downloaded_size_bytes": 100, "api_reported_digest_sha256": digest,
        "locally_computed_zip_sha256": digest, "digest_match": True,
        "workflow_run_id_of_artifact": run_id, "head_sha_of_artifact_run": head_sha,
        "expired": False, "contained_files": contained_files,
    }


class TestPerArtifactFileHashes(unittest.TestCase):
    def test_matching_files_produce_no_problems(self):
        files = [{"path": "ci-log-nlog/acquisition-receipt.json", "sha256": "aaa", "size": 10}]
        fetch_report = _fetch_report([_fetched_entry("acquisition-ci-log-nlog", 1, "digest1", files)])
        index = [{"artifact_id": 1, "artifact_name": "acquisition-ci-log-nlog",
                  "archive_digest": "sha256:digest1", "contained_files": files}]
        problems = v.verify_per_artifact_file_hashes(fetch_report, index)
        self.assertEqual(problems, [])

    def test_file_hash_mismatch_detected(self):
        fetched_files = [{"path": "x/receipt.json", "sha256": "wrong", "size": 10}]
        indexed_files = [{"path": "x/receipt.json", "sha256": "right", "size": 10}]
        fetch_report = _fetch_report([_fetched_entry("acquisition-x", 1, "digest1", fetched_files)])
        index = [{"artifact_id": 1, "artifact_name": "acquisition-x",
                  "archive_digest": "sha256:digest1", "contained_files": indexed_files}]
        problems = v.verify_per_artifact_file_hashes(fetch_report, index)
        self.assertEqual(len(problems), 1)
        self.assertIn("x/receipt.json", problems[0]["mismatched_files"])

    def test_missing_artifact_id_in_index_flagged(self):
        fetch_report = _fetch_report([_fetched_entry("acquisition-x", 99, "digest1", [])])
        problems = v.verify_per_artifact_file_hashes(fetch_report, [])
        self.assertEqual(len(problems), 1)
        self.assertIn("not present", problems[0]["problem"])

    def test_non_acquisition_artifacts_are_skipped(self):
        fetch_report = _fetch_report([_fetched_entry("n2c-artifact-index", 5, "digest1", [])])
        problems = v.verify_per_artifact_file_hashes(fetch_report, [])
        self.assertEqual(problems, [])

    def test_extra_file_detected(self):
        fetched_files = [{"path": "a.json", "sha256": "h1", "size": 1}, {"path": "b.json", "sha256": "h2", "size": 1}]
        indexed_files = [{"path": "a.json", "sha256": "h1", "size": 1}]
        fetch_report = _fetch_report([_fetched_entry("acquisition-x", 1, "digest1", fetched_files)])
        index = [{"artifact_id": 1, "artifact_name": "acquisition-x",
                  "archive_digest": "sha256:digest1", "contained_files": indexed_files}]
        problems = v.verify_per_artifact_file_hashes(fetch_report, index)
        self.assertEqual(problems[0]["extra_files"], ["b.json"])

    def test_archive_digest_mismatch_detected(self):
        files = [{"path": "a.json", "sha256": "h1", "size": 1}]
        fetch_report = _fetch_report([_fetched_entry("acquisition-x", 1, "wrong-digest", files)])
        index = [{"artifact_id": 1, "artifact_name": "acquisition-x",
                  "archive_digest": "sha256:right-digest", "contained_files": files}]
        problems = v.verify_per_artifact_file_hashes(fetch_report, index)
        self.assertEqual(len(problems), 1)
        self.assertIn("archive_digest mismatch", problems[0]["problem"])


class TestCandidateRoles(unittest.TestCase):
    def test_exact_match_passes(self):
        fetch_report = _fetch_report([_fetched_entry("acquisition-a", 1, "d", []),
                                       _fetched_entry("acquisition-b", 2, "d", [])])
        quota = {"primary_case_ids": ["a"], "alternate_case_ids": ["b"]}
        result = v.verify_candidate_roles(fetch_report, quota)
        self.assertTrue(result["roles_match"])
        self.assertEqual(result["missing"], [])
        self.assertEqual(result["unexpected"], [])

    def test_missing_candidate_detected(self):
        fetch_report = _fetch_report([_fetched_entry("acquisition-a", 1, "d", [])])
        quota = {"primary_case_ids": ["a"], "alternate_case_ids": ["b"]}
        result = v.verify_candidate_roles(fetch_report, quota)
        self.assertFalse(result["roles_match"])
        self.assertEqual(result["missing"], ["b"])

    def test_unexpected_candidate_detected(self):
        fetch_report = _fetch_report([_fetched_entry("acquisition-a", 1, "d", []),
                                       _fetched_entry("acquisition-extra", 2, "d", [])])
        quota = {"primary_case_ids": ["a"], "alternate_case_ids": []}
        result = v.verify_candidate_roles(fetch_report, quota)
        self.assertFalse(result["roles_match"])
        self.assertEqual(result["unexpected"], ["extra"])


class TestReceiptFieldValues(unittest.TestCase):
    def test_matching_receipt_passes(self):
        import json
        import tempfile
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            case_dir = root / "acquisition-ci-log-nlog"
            case_dir.mkdir()
            (case_dir / "acquisition-receipt.json").write_text(json.dumps({
                "metadata_sha256": "m1", "source_content_sha256": "c1", "normalized_source_sha256": "n1",
            }))
            freeze = {
                "metadata_sha256": {"ci-log-nlog": "m1"},
                "source_content_sha256": {"ci-log-nlog": "c1"},
                "normalized_source_sha256": {"ci-log-nlog": "n1"},
            }
            problems = v.verify_receipt_field_values(root, freeze)
            self.assertEqual(problems, [])

    def test_mismatched_receipt_detected(self):
        import json
        import tempfile
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            case_dir = root / "acquisition-ci-log-nlog"
            case_dir.mkdir()
            (case_dir / "acquisition-receipt.json").write_text(json.dumps({
                "metadata_sha256": "WRONG", "source_content_sha256": "c1", "normalized_source_sha256": "n1",
            }))
            freeze = {
                "metadata_sha256": {"ci-log-nlog": "m1"},
                "source_content_sha256": {"ci-log-nlog": "c1"},
                "normalized_source_sha256": {"ci-log-nlog": "n1"},
            }
            problems = v.verify_receipt_field_values(root, freeze)
            self.assertEqual(len(problems), 1)
            self.assertEqual(problems[0]["field"], "metadata_sha256")

    def test_missing_receipt_file_detected(self):
        import tempfile
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "acquisition-ci-log-nlog").mkdir()
            problems = v.verify_receipt_field_values(root, {})
            self.assertEqual(len(problems), 1)
            self.assertIn("no acquisition-receipt.json", problems[0]["problem"])


if __name__ == "__main__":
    unittest.main()
