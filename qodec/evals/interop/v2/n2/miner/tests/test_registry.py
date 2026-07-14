"""Tests for registry.py: schema validation + selection-status state machine."""
import sys
import unittest
from pathlib import Path

MINER_DIR = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(MINER_DIR / "tools"))
import registry  # noqa: E402

EXAMPLE_PATH = MINER_DIR / "candidate-registry.example.json"


def _minimal_candidate(**overrides) -> dict:
    base = {
        "candidate_id": "c-1",
        "repository": {"url": "https://github.com/example/foo", "owner": "example", "name": "foo"},
        "commit_sha": "a" * 40,
        "ecosystem": "dotnet",
        "expected_log_family": "build-success",
        "origin_kind": "synthetic-first-party",
        "license": {"status": "clear", "spdx": "MIT", "file": "LICENSE"},
        "project": {"entry_point": "Foo/Foo.csproj", "ambiguous": False},
        "toolchain_request": {"requested_version_or_range": "8.0.x", "resolver_mechanism": "global.json"},
        "dependency_lock": {"present": False, "files": []},
        "network_requirements": {"required_during_untrusted_execution": False},
        "submodule_status": "none",
        "git_lfs_status": "none",
        "private_feed_status": "none",
        "external_service_requirements": [],
        "container_requirements": [],
        "estimated_resource_class": "small",
        "expected_capture_command_class": "dotnet-build",
        "security_flags": [],
        "reproducibility_class": "expected-byte-reproducible",
        "selection_status": "discovered",
        "rejection_reason": None,
        "evidence_references": ["manual-review"],
        "status_history": [{"status": "discovered", "registry_version": "v1"}],
    }
    base.update(overrides)
    return base


class TestExampleRegistry(unittest.TestCase):
    def test_example_registry_is_valid(self):
        reg = registry.load_registry(EXAMPLE_PATH)
        self.assertEqual(registry.validate_registry(reg), [])

    def test_example_registry_has_n2a_reference_only_entry(self):
        reg = registry.load_registry(EXAMPLE_PATH)
        n2a = next(c for c in reg["candidates"] if c["candidate_id"] == "n2a-reference")
        self.assertEqual(n2a["origin_kind"], "n2a-reference-only")
        self.assertEqual(n2a["selection_status"], "frozen")


class TestValidRegistryAccepted(unittest.TestCase):
    def test_valid_registry_with_one_candidate_accepted(self):
        candidate = _minimal_candidate()
        reg = {"registry_version": "v1", "candidates": [candidate]}
        self.assertEqual(registry.validate_registry(reg), [])

    def test_valid_state_transition_chain_accepted(self):
        candidate = _minimal_candidate(
            selection_status="selected",
            status_history=[
                {"status": "discovered", "registry_version": "v1"},
                {"status": "inspected", "registry_version": "v1"},
                {"status": "eligible", "registry_version": "v1"},
                {"status": "selected", "registry_version": "v1"},
            ],
        )
        reg = {"registry_version": "v1", "candidates": [candidate]}
        self.assertEqual(registry.validate_registry(reg), [])


class TestInvalidTransitionsRejected(unittest.TestCase):
    def test_undefined_transition_rejected(self):
        candidate = _minimal_candidate(
            selection_status="frozen",
            status_history=[
                {"status": "discovered", "registry_version": "v1"},
                {"status": "frozen", "registry_version": "v1"},
            ],
        )
        errs = registry.validate_candidate_transitions(candidate)
        self.assertTrue(errs)

    def test_ineligible_to_selected_rejected_without_override(self):
        candidate = _minimal_candidate(
            selection_status="selected",
            status_history=[
                {"status": "discovered", "registry_version": "v1"},
                {"status": "inspected", "registry_version": "v1"},
                {"status": "ineligible", "registry_version": "v1"},
                {"status": "selected", "registry_version": "v1"},
            ],
        )
        errs = registry.validate_candidate_transitions(candidate)
        self.assertTrue(any("ineligible" in e and "selected" in e for e in errs))

    def test_ineligible_to_selected_accepted_with_override_and_new_version(self):
        candidate = _minimal_candidate(
            selection_status="selected",
            status_history=[
                {"status": "discovered", "registry_version": "v1"},
                {"status": "inspected", "registry_version": "v1"},
                {"status": "ineligible", "registry_version": "v1"},
                {"status": "selected", "registry_version": "v2",
                 "override": {"reason": "manual re-review found the rejection was a false positive"}},
            ],
        )
        errs = registry.validate_candidate_transitions(candidate)
        self.assertEqual(errs, [])

    def test_frozen_to_discovered_rejected_without_override(self):
        candidate = _minimal_candidate(
            selection_status="discovered",
            status_history=[
                {"status": "discovered", "registry_version": "v1"},
                {"status": "inspected", "registry_version": "v1"},
                {"status": "eligible", "registry_version": "v1"},
                {"status": "selected", "registry_version": "v1"},
                {"status": "frozen", "registry_version": "v1"},
                {"status": "discovered", "registry_version": "v1"},
            ],
        )
        errs = registry.validate_candidate_transitions(candidate)
        self.assertTrue(errs)

    def test_frozen_to_eligible_rejected_without_override(self):
        candidate = _minimal_candidate(
            selection_status="eligible",
            status_history=[
                {"status": "discovered", "registry_version": "v1"},
                {"status": "inspected", "registry_version": "v1"},
                {"status": "eligible", "registry_version": "v1"},
                {"status": "selected", "registry_version": "v1"},
                {"status": "frozen", "registry_version": "v1"},
                {"status": "eligible", "registry_version": "v1"},
            ],
        )
        errs = registry.validate_candidate_transitions(candidate)
        self.assertTrue(errs)

    def test_status_history_tail_must_match_selection_status(self):
        candidate = _minimal_candidate(
            selection_status="eligible",
            status_history=[{"status": "discovered", "registry_version": "v1"}],
        )
        errs = registry.validate_candidate_transitions(candidate)
        self.assertTrue(any("does not match selection_status" in e for e in errs))


class TestSchemaRejectsMissingFields(unittest.TestCase):
    def test_missing_commit_sha_rejected_by_schema(self):
        candidate = _minimal_candidate()
        del candidate["commit_sha"]
        reg = {"registry_version": "v1", "candidates": [candidate]}
        errs = registry.validate_registry(reg)
        self.assertTrue(any("commit_sha" in e for e in errs))

    def test_floating_ref_shaped_commit_sha_rejected_by_schema_pattern(self):
        candidate = _minimal_candidate(commit_sha="main")
        reg = {"registry_version": "v1", "candidates": [candidate]}
        errs = registry.validate_registry(reg)
        self.assertTrue(any("commit_sha" in e for e in errs))


if __name__ == "__main__":
    unittest.main()
