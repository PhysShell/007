"""Section 23/5 tests for identity_lock.py: fold currently-null registry
identity fields from real acquisition, then verify (and fail on drift) once
those fields are already committed (identity-locked)."""
import sys
import unittest
from pathlib import Path

TOOLS = Path(__file__).resolve().parents[1] / "tools"
sys.path.insert(0, str(TOOLS))
import identity_lock  # noqa: E402


def _repo_registry():
    return {"candidates": [{
        "candidate_id": "repo-c1", "source_kind": "repository-execution",
        "source_identity": {"commit_sha": None, "tree_sha": None,
                             "normalized_archive_sha256": None, "license_sha256": None},
    }]}


def _ci_log_registry():
    return {"candidates": [{
        "candidate_id": "ci-log-c1", "source_kind": "ci-run-artifact",
        "source_identity": {"metadata_sha256": None, "source_content_sha256": None,
                             "normalized_source_sha256": None, "source_commit_sha": None,
                             "selected_job_ids": [], "selected_job_names": [],
                             "log_acquisition_endpoint": None},
    }]}


class TestFoldBootstrapsNullFields(unittest.TestCase):
    def test_repository_fields_folded_in_from_null(self):
        reg = _repo_registry()
        results = [{"candidate_id": "repo-c1", "actual_head_sha": "abc", "git_tree_sha": "def",
                    "normalized_archive_sha256": "h1", "license_sha256": "h2"}]
        report = identity_lock.fold_or_verify(reg, results)
        self.assertEqual(report["updated"], ["repo-c1"])
        self.assertEqual(report["drift"], [])
        ident = reg["candidates"][0]["source_identity"]
        self.assertEqual(ident["commit_sha"], "abc")
        self.assertEqual(ident["tree_sha"], "def")
        self.assertEqual(ident["normalized_archive_sha256"], "h1")
        self.assertEqual(ident["license_sha256"], "h2")

    def test_ci_log_fields_folded_in_from_null(self):
        reg = _ci_log_registry()
        results = [{"candidate_id": "ci-log-c1", "metadata_sha256": "m1", "source_content_sha256": "c1",
                    "normalized_source_sha256": "n1", "source_commit_sha": "sha1",
                    "selected_job_ids": ["123"], "selected_job_names": ["build"],
                    "log_acquisition_endpoint": "https://api.github.com/.../logs"}]
        report = identity_lock.fold_or_verify(reg, results)
        self.assertEqual(report["updated"], ["ci-log-c1"])
        self.assertEqual(report["drift"], [])
        ident = reg["candidates"][0]["source_identity"]
        self.assertEqual(ident["selected_job_ids"], ["123"])
        self.assertEqual(ident["source_commit_sha"], "sha1")


class TestVerifyDetectsDrift(unittest.TestCase):
    def test_matching_reacquisition_is_clean_noop(self):
        reg = _repo_registry()
        results = [{"candidate_id": "repo-c1", "actual_head_sha": "abc", "git_tree_sha": "def",
                    "normalized_archive_sha256": "h1", "license_sha256": "h2"}]
        identity_lock.fold_or_verify(reg, results)  # bootstrap
        report = identity_lock.fold_or_verify(reg, results)  # re-verify, identical
        self.assertEqual(report["updated"], [])
        self.assertEqual(report["drift"], [])

    def test_differing_reacquisition_is_flagged_as_drift(self):
        reg = _repo_registry()
        first = [{"candidate_id": "repo-c1", "actual_head_sha": "abc", "git_tree_sha": "def",
                  "normalized_archive_sha256": "h1", "license_sha256": "h2"}]
        identity_lock.fold_or_verify(reg, first)  # bootstrap/lock
        second = [{"candidate_id": "repo-c1", "actual_head_sha": "abc", "git_tree_sha": "def",
                   "normalized_archive_sha256": "DIFFERENT", "license_sha256": "h2"}]
        report = identity_lock.fold_or_verify(reg, second)
        self.assertEqual(len(report["drift"]), 1)
        self.assertEqual(report["drift"][0]["field"], "normalized_archive_sha256")
        self.assertEqual(report["drift"][0]["committed"], "h1")
        self.assertEqual(report["drift"][0]["fresh"], "DIFFERENT")

    def test_unmatched_result_recorded_not_silently_dropped(self):
        reg = _repo_registry()
        results = [{"candidate_id": "does-not-exist", "actual_head_sha": "abc"}]
        report = identity_lock.fold_or_verify(reg, results)
        self.assertEqual(report["unmatched_results"], ["does-not-exist"])
        self.assertEqual(report["updated"], [])


if __name__ == "__main__":
    unittest.main()
