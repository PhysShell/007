"""Tests for eligibility.py — hard rejection rules applied before scoring."""
import copy
import sys
import unittest
from pathlib import Path

MINER_DIR = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(MINER_DIR / "tools"))
import eligibility  # noqa: E402

BASE_CANDIDATE = {
    "candidate_id": "c-1",
    "commit_sha": "a" * 40,
    "ecosystem": "dotnet",
    "license": {"status": "clear", "spdx": "MIT", "file": "LICENSE"},
    "project": {"entry_point": "Foo/Foo.csproj", "ambiguous": False},
    "security_flags": [],
    "private_feed_status": "none",
    "container_requirements": [],
    "network_requirements": {"required_during_untrusted_execution": False},
    "external_service_requirements": [],
}


def _candidate(**overrides) -> dict:
    c = copy.deepcopy(BASE_CANDIDATE)
    c.update(overrides)
    return c


class TestEligiblePasses(unittest.TestCase):
    def test_fully_compliant_candidate_is_eligible(self):
        report = eligibility.evaluate(_candidate())
        self.assertTrue(report["eligible"])
        self.assertIsNone(report["rejection_reason"])


class TestHardRejections(unittest.TestCase):
    def test_missing_license_rejected(self):
        report = eligibility.evaluate(_candidate(license={"status": "missing", "spdx": None, "file": None}))
        self.assertFalse(report["eligible"])
        self.assertEqual(report["rejection_reason"], "has_explicit_license")

    def test_missing_commit_sha_rejected(self):
        report = eligibility.evaluate(_candidate(commit_sha=""))
        self.assertFalse(report["eligible"])
        self.assertEqual(report["rejection_reason"], "has_immutable_commit")

    def test_floating_branch_name_rejected(self):
        # A candidate whose commit_sha field actually holds a branch name
        # (not a 40-hex SHA) — the floating-ref case section 5 names.
        report = eligibility.evaluate(_candidate(commit_sha="main"))
        self.assertFalse(report["eligible"])
        self.assertIn(report["rejection_reason"], ("has_immutable_commit", "not_floating_ref"))

    def test_private_package_feed_required_rejected(self):
        report = eligibility.evaluate(_candidate(private_feed_status="required"))
        self.assertFalse(report["eligible"])
        self.assertEqual(report["rejection_reason"], "no_private_package_feed_required")

    def test_docker_socket_required_rejected(self):
        report = eligibility.evaluate(_candidate(container_requirements=["docker-socket"]))
        self.assertFalse(report["eligible"])
        self.assertEqual(report["rejection_reason"], "no_docker_socket_required")

    def test_uncontrolled_network_during_untrusted_execution_rejected(self):
        report = eligibility.evaluate(
            _candidate(network_requirements={"required_during_untrusted_execution": True})
        )
        self.assertFalse(report["eligible"])
        self.assertEqual(report["rejection_reason"], "no_uncontrolled_network_during_untrusted_execution")

    def test_mandatory_external_service_rejected(self):
        report = eligibility.evaluate(_candidate(external_service_requirements=["postgres"]))
        self.assertFalse(report["eligible"])
        self.assertEqual(report["rejection_reason"], "no_mandatory_external_service")

    def test_private_credentials_required_rejected(self):
        report = eligibility.evaluate(_candidate(security_flags=["requires-private-credentials"]))
        self.assertFalse(report["eligible"])
        self.assertEqual(report["rejection_reason"], "no_private_credentials_required")

    def test_ambiguous_entry_point_not_auto_selected_and_rejected(self):
        report = eligibility.evaluate(
            _candidate(project={"entry_point": None, "ambiguous": True})
        )
        self.assertFalse(report["eligible"])
        self.assertEqual(report["rejection_reason"], "supported_build_entry_point")

    def test_unsupported_ecosystem_rejected(self):
        report = eligibility.evaluate(_candidate(ecosystem="cobol"))
        self.assertFalse(report["eligible"])
        self.assertEqual(report["rejection_reason"], "supported_build_entry_point")


class TestNoBenchmarkSignalsConsidered(unittest.TestCase):
    def test_eligibility_report_never_mentions_qodec_or_rtk(self):
        report = eligibility.evaluate(_candidate())
        blob = str(report).lower()
        self.assertNotIn("qodec", blob)
        self.assertNotIn("rtk", blob)
        self.assertNotIn("token", blob)


if __name__ == "__main__":
    unittest.main()
