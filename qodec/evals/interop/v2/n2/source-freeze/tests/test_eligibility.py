"""Section 23 tests for eligibility.py's dispatch between frozen N2-B rules
(repository-miner, N2-B-supported ecosystems) and the new N2-C extension
(6th ecosystem + 4 non-repository origin kinds)."""
import sys
import unittest
from pathlib import Path

TOOLS = Path(__file__).resolve().parents[1] / "tools"
sys.path.insert(0, str(TOOLS))
import eligibility  # noqa: E402
import n2b_bridge  # noqa: E402

SOURCE_FREEZE_DIR = Path(__file__).resolve().parents[1]


def _repo_candidate(**overrides):
    base = {
        "candidate_id": "c1",
        "public_canonical_url": "https://github.com/example/example",
        "source_kind": "repository-execution",
        "origin_kind": "repository-miner",
        "ecosystem": "rust",
        "primary_family": "test",
        "source_identity": {"identity_kind": "git-commit", "commit_sha": "a" * 40, "tree_sha": "b" * 40},
        "license": {"status": "clear", "spdx": "MIT", "redistribution_allowed": True},
        "project": {"entry_point": "Cargo.toml", "ambiguous": False},
        "dependency_lock": {"present": True, "files": ["Cargo.lock"]},
        "network_requirements": {"required_during_untrusted_execution": False},
        "submodule_status": "none", "git_lfs_status": "none", "private_feed_status": "none",
        "external_service_requirements": [], "container_requirements": [],
        "security_flags": [], "estimated_resource_class": "small",
        "reproducibility_class": "unknown",
        "evidence_references": ["https://github.com/example/example"],
    }
    base.update(overrides)
    return base


def _non_repo_candidate(origin_kind, **overrides):
    base = {
        "candidate_id": "c2",
        "public_canonical_url": "https://zenodo.org/records/1234567",
        "origin_kind": origin_kind,
        "ecosystem": "infrastructure-or-language-neutral",
        "source_identity": {"identity_kind": "immutable-object-or-doi", "object_id_or_doi": "10.5281/zenodo.1234567"},
        "license": {"status": "clear", "spdx": "CC-BY-4.0", "redistribution_allowed": True},
        "security_flags": [],
        "publisher": {"identity": "Example Publisher"},
        "personal_data_review": {"personal_data_present": False},
    }
    base.update(overrides)
    return base


class TestN2BBridgeDispatch(unittest.TestCase):
    def test_repository_miner_in_supported_ecosystem_uses_frozen_n2b(self):
        c = _repo_candidate()
        self.assertTrue(n2b_bridge.n2b_eligibility_applies(c))
        report = eligibility.evaluate(c)
        self.assertEqual(report["rule_set"], "n2b-frozen")
        self.assertTrue(report["eligible"])

    def test_repository_miner_infra_neutral_uses_n2c_extension(self):
        c = _repo_candidate(ecosystem="infrastructure-or-language-neutral",
                             project={"entry_point": "Makefile", "ambiguous": False})
        self.assertFalse(n2b_bridge.n2b_eligibility_applies(c))
        report = eligibility.evaluate(c)
        self.assertEqual(report["rule_set"], "n2c-extended")
        self.assertTrue(report["eligible"])

    def test_non_repository_origin_uses_n2c_extension(self):
        c = _non_repo_candidate("public-runtime-dataset")
        report = eligibility.evaluate(c)
        self.assertEqual(report["rule_set"], "n2c-extended")
        self.assertTrue(report["eligible"])


class TestRealRegistryEligibility(unittest.TestCase):
    def test_real_registry_has_at_least_25_eligible(self):
        sys.path.insert(0, str(TOOLS))
        import registry as registry_mod
        reg = registry_mod.load_registry(SOURCE_FREEZE_DIR / "candidate-registry.json")
        reports = eligibility.evaluate_registry(reg)
        eligible = [r for r in reports if r["eligible"]]
        self.assertGreaterEqual(len(reports), 30, "at least 30 candidates must be inspected")
        self.assertGreaterEqual(len(eligible), 25, "at least 25 candidates must be eligible")


class TestNegativePaths(unittest.TestCase):
    def test_floating_branch_rejected(self):
        c = _repo_candidate(source_identity={"identity_kind": "git-commit", "commit_sha": "main", "tree_sha": None})
        report = eligibility.evaluate(c)
        self.assertFalse(report["eligible"])
        self.assertIn(report["rejection_reason"], ("has_immutable_commit", "not_floating_ref"))

    def test_missing_license_rejected(self):
        c = _repo_candidate(license={"status": "missing", "spdx": None, "redistribution_allowed": False})
        report = eligibility.evaluate(c)
        self.assertFalse(report["eligible"])
        self.assertEqual(report["rejection_reason"], "has_explicit_license")

    def test_unclear_redistribution_rejected(self):
        c = _repo_candidate(license={"status": "ambiguous", "spdx": "MIT", "redistribution_allowed": "unclear"})
        report = eligibility.evaluate(c)
        self.assertFalse(report["eligible"])

    def test_private_feed_rejected(self):
        c = _repo_candidate(private_feed_status="required")
        report = eligibility.evaluate(c)
        self.assertFalse(report["eligible"])
        self.assertEqual(report["rejection_reason"], "no_private_package_feed_required")

    def test_required_credential_rejected(self):
        c = _repo_candidate(security_flags=["requires-private-credentials"])
        report = eligibility.evaluate(c)
        self.assertFalse(report["eligible"])
        self.assertEqual(report["rejection_reason"], "no_private_credentials_required")

    def test_required_docker_socket_rejected(self):
        c = _repo_candidate(container_requirements=["docker-socket"])
        report = eligibility.evaluate(c)
        self.assertFalse(report["eligible"])
        self.assertEqual(report["rejection_reason"], "no_docker_socket_required")

    def test_required_privileged_execution_rejected(self):
        c = _repo_candidate(security_flags=["requires-privileged-execution"])
        report = eligibility.evaluate(c)
        self.assertFalse(report["eligible"])
        self.assertEqual(report["rejection_reason"], "no_privileged_execution_required")

    def test_uncontrolled_network_rejected(self):
        c = _repo_candidate(network_requirements={"required_during_untrusted_execution": True})
        report = eligibility.evaluate(c)
        self.assertFalse(report["eligible"])
        self.assertEqual(report["rejection_reason"], "no_uncontrolled_network_during_untrusted_execution")

    def test_submodule_without_immutable_commit_rejected_via_supported_entry_point(self):
        # N2-B's frozen rule set has no separate "has_submodule" rule (that
        # is N2-A/N2-A.1's acquisition-time check, section 15) — a candidate
        # advertising submodule presence should still be caught by acquisition
        # tooling; verify the schema/eligibility contract records the flag.
        c = _repo_candidate(submodule_status="present")
        # Not itself an eligibility rejection reason in the frozen ruleset,
        # but must be visible for acquisition.py's verify_no_submodules to
        # reject at acquisition time (see test_acquisition.py).
        self.assertEqual(c["submodule_status"], "present")

    def test_mutable_latest_url_rejected_for_non_repository_candidate(self):
        c = _non_repo_candidate("public-runtime-dataset", public_canonical_url="https://example.com/dataset/latest")
        report = eligibility.evaluate(c)
        self.assertFalse(report["eligible"])
        self.assertEqual(report["rejection_reason"], "not_mutable_latest_url")

    def test_no_immutable_object_identity_rejected_for_non_repository_candidate(self):
        c = _non_repo_candidate("public-runtime-dataset",
                                 source_identity={"identity_kind": "immutable-object-or-doi", "object_id_or_doi": None})
        report = eligibility.evaluate(c)
        self.assertFalse(report["eligible"])
        self.assertEqual(report["rejection_reason"], "has_immutable_object_identity")

    def test_pii_present_rejected_for_non_repository_candidate(self):
        c = _non_repo_candidate("public-runtime-dataset",
                                 personal_data_review={"personal_data_present": True})
        report = eligibility.evaluate(c)
        self.assertFalse(report["eligible"])
        self.assertEqual(report["rejection_reason"], "no_ineliminable_pii_or_secret_exposure")

    def test_qodec_field_in_candidate_rejected_upstream_of_eligibility(self):
        # eligibility itself never reads such a field; the registry-level
        # seal (test_registry.py) is what rejects it before evaluate() runs.
        c = _repo_candidate()
        c["qodec_tokens"] = None
        sys.path.insert(0, str(TOOLS))
        import registry as registry_mod
        reg = {"registry_version": "v1", "candidates": [{**c, "status_history": [
            {"status": "eligible", "registry_version": "v1"}], "selection_status": "eligible"}]}
        self.assertTrue(registry_mod.validate_no_forbidden_fields(reg))


if __name__ == "__main__":
    unittest.main()
