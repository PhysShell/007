"""Section 23 tests for registry.py: schema validation, transition state
machine, and the section-19 sealing check (no QODEC/RTK/token fields)."""
import sys
import unittest
from pathlib import Path

TOOLS = Path(__file__).resolve().parents[1] / "tools"
sys.path.insert(0, str(TOOLS))
import registry  # noqa: E402

SOURCE_FREEZE_DIR = Path(__file__).resolve().parents[1]


def _minimal_candidate(**overrides):
    base = {
        "candidate_id": "test-candidate",
        "public_canonical_url": "https://github.com/example/example",
        "source_kind": "repository-execution",
        "origin_kind": "repository-miner",
        "ecosystem": "rust",
        "primary_family": "test",
        "secondary_tags": [],
        "publisher": {"identity": "example"},
        "discovery": {"timestamp": "2026-01-01T00:00:00Z", "mechanism": "manual"},
        "source_identity": {"identity_kind": "git-commit", "commit_sha": "a" * 40, "tree_sha": "b" * 40},
        "license": {"status": "clear", "spdx": "MIT", "redistribution_allowed": True},
        "project": {"entry_point": "Cargo.toml", "ambiguous": False},
        "network_requirements": {"required_during_untrusted_execution": False},
        "external_service_requirements": [],
        "container_requirements": [],
        "security_flags": [],
        "estimated_resource_class": "small",
        "expected_size_bucket": "small",
        "expected_size_estimation_basis": "test",
        "expected_size_confidence": "low",
        "reproducibility_class": "unknown",
        "offline_feasibility": "offline-ready-by-inspection",
        "selection_status": "eligible",
        "evidence_references": ["https://github.com/example/example"],
        "status_history": [
            {"status": "discovered", "registry_version": "v1"},
            {"status": "inspected", "registry_version": "v1"},
            {"status": "eligible", "registry_version": "v1"},
        ],
    }
    base.update(overrides)
    return base


class TestRegistrySchemaValidation(unittest.TestCase):
    def test_real_registry_is_valid(self):
        reg = registry.load_registry(SOURCE_FREEZE_DIR / "candidate-registry.json")
        self.assertEqual(registry.validate_registry(reg), [])

    def test_minimal_valid_candidate_passes(self):
        reg = {"registry_version": "v1", "candidates": [_minimal_candidate()]}
        self.assertEqual(registry.validate_registry(reg), [])

    def test_missing_required_field_rejected(self):
        candidate = _minimal_candidate()
        del candidate["license"]
        reg = {"registry_version": "v1", "candidates": [candidate]}
        self.assertNotEqual(registry.validate_registry(reg), [])

    def test_floating_ref_rejected_by_pattern(self):
        candidate = _minimal_candidate()
        candidate["source_identity"]["commit_sha"] = "main"
        reg = {"registry_version": "v1", "candidates": [candidate]}
        errors = registry.validate_schema(reg)
        # source_identity.commit_sha is schema-pattern-constrained to
        # exactly 40 lowercase hex chars (or empty/null for non-repository
        # candidates) — a branch name like "main" fails at the schema layer
        # already, before eligibility.py's has_immutable_commit rule would
        # even run.
        self.assertTrue(errors)


class TestStatusTransitions(unittest.TestCase):
    def test_valid_transition_chain_accepted(self):
        candidate = _minimal_candidate(status_history=[
            {"status": "discovered", "registry_version": "v1"},
            {"status": "inspected", "registry_version": "v1"},
            {"status": "eligible", "registry_version": "v1"},
            {"status": "selected-primary", "registry_version": "v1"},
            {"status": "frozen", "registry_version": "v1"},
        ], selection_status="frozen")
        self.assertEqual(registry.validate_candidate_transitions(candidate), [])

    def test_ineligible_to_selected_without_override_rejected(self):
        candidate = _minimal_candidate(status_history=[
            {"status": "discovered", "registry_version": "v1"},
            {"status": "inspected", "registry_version": "v1"},
            {"status": "ineligible", "registry_version": "v1"},
            {"status": "selected-primary", "registry_version": "v1"},
        ], selection_status="selected-primary")
        errors = registry.validate_candidate_transitions(candidate)
        self.assertTrue(errors)

    def test_frozen_to_discovered_regression_rejected(self):
        candidate = _minimal_candidate(status_history=[
            {"status": "discovered", "registry_version": "v1"},
            {"status": "inspected", "registry_version": "v1"},
            {"status": "eligible", "registry_version": "v1"},
            {"status": "selected-primary", "registry_version": "v1"},
            {"status": "frozen", "registry_version": "v1"},
            {"status": "discovered", "registry_version": "v1"},
        ], selection_status="discovered")
        errors = registry.validate_candidate_transitions(candidate)
        self.assertTrue(errors)

    def test_status_history_last_entry_must_match_selection_status(self):
        candidate = _minimal_candidate(selection_status="eligible", status_history=[
            {"status": "discovered", "registry_version": "v1"},
            {"status": "inspected", "registry_version": "v1"},
        ])
        errors = registry.validate_candidate_transitions(candidate)
        self.assertTrue(errors)


class TestSealingCheck(unittest.TestCase):
    def test_real_registry_has_no_forbidden_fields(self):
        reg = registry.load_registry(SOURCE_FREEZE_DIR / "candidate-registry.json")
        self.assertEqual(registry.validate_no_forbidden_fields(reg), [])

    def test_qodec_field_in_candidate_rejected(self):
        candidate = _minimal_candidate()
        candidate["qodec_tokens"] = None
        reg = {"registry_version": "v1", "candidates": [candidate]}
        self.assertTrue(registry.validate_no_forbidden_fields(reg))

    def test_rtk_field_in_candidate_rejected(self):
        candidate = _minimal_candidate()
        candidate["rtk_tokens"] = 123
        reg = {"registry_version": "v1", "candidates": [candidate]}
        self.assertTrue(registry.validate_no_forbidden_fields(reg))

    def test_token_metric_field_rejected(self):
        candidate = _minimal_candidate()
        candidate["token_savings"] = 0.5
        reg = {"registry_version": "v1", "candidates": [candidate]}
        self.assertTrue(registry.validate_no_forbidden_fields(reg))

    def test_nested_forbidden_field_rejected(self):
        candidate = _minimal_candidate()
        candidate["project"]["winner"] = "qodec"
        reg = {"registry_version": "v1", "candidates": [candidate]}
        self.assertTrue(registry.validate_no_forbidden_fields(reg))

    def test_mutable_latest_url_is_not_rejected_by_schema_alone(self):
        # Schema permits any https URL; rejecting "latest" URLs as canonical
        # identity is eligibility_extended.py's job for non-repository
        # candidates (see test_eligibility.py).
        candidate = _minimal_candidate(public_canonical_url="https://example.com/dataset/latest")
        reg = {"registry_version": "v1", "candidates": [candidate]}
        self.assertEqual(registry.validate_schema(reg), [])


if __name__ == "__main__":
    unittest.main()
