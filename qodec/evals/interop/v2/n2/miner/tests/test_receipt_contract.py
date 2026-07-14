"""Tests for receipt_contract.py — generic receipt schema + reproducibility
comparison gate. Direct regression coverage for the N2-A finding that let
`None == None` count as reproducibility agreement for toolchain identity."""
import sys
import unittest
from pathlib import Path

MINER_DIR = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(MINER_DIR / "tools"))
import receipt_contract as rc  # noqa: E402


def _valid_receipt() -> dict:
    return {
        "receipt_contract_version": "n2b-receipt-contract-v1",
        "source_identity": {"commit_sha": "a" * 40, "archive_sha256": "b" * 64},
        "license_identity": {"spdx": "MIT", "sha256": "c" * 64},
        "acquisition_identity": {"checkout_action_identity": "actions/checkout@deadbeef", "persist_credentials": False},
        "adapter_identity": {"name": "dotnet_adapter", "version": "n2b-v1"},
        "toolchain_requested": {"version_or_range": "8.0.x", "resolver_mechanism": "global.json"},
        "toolchain_resolved": {"resolved_version": "8.0.404", "runtime_identifier": "linux-x64",
                                "resolved_executable_path": "/usr/share/dotnet/dotnet"},
        "toolchain_executed": {"executed_argv0": "/usr/share/dotnet/dotnet",
                                "executed_binary_absolute_path": "/usr/share/dotnet/dotnet",
                                "executed_binary_sha256": "d" * 64, "classification": "exact-match"},
        "sandbox_identity": {"sandboy_commit_sha": "e" * 40, "policy_sha256": "f" * 64},
        "outer_isolation": {"network_isolation": True, "wall_clock_timeout_s": 900},
        "resource_limits": {"cpu_time_limit_s": 600, "process_count_limit": 512,
                             "memory_enforcement_mechanism": "outer-runner-enforced"},
        "command_argv": ["dotnet", "build"],
        "environment_variable_names": ["PATH", "HOME"],
        "stdout_identity": {"sha256": "1" * 64},
        "stderr_identity": {"sha256": "2" * 64},
        "termination": {"exit_code": 0},
        "sanitization": {"profile_version": "n2b-sanitizer-profile-v1", "transformations": ["iso_timestamp"]},
        "reproducibility": {"class": "expected-byte-reproducible"},
    }


class TestValidReceiptAccepted(unittest.TestCase):
    def test_valid_receipt_has_no_schema_errors(self):
        self.assertEqual(rc.validate_receipt(_valid_receipt()), [])


class TestNonEmptyIdentityFieldsEnforced(unittest.TestCase):
    def test_missing_commit_sha_rejected(self):
        receipt = _valid_receipt()
        del receipt["source_identity"]["commit_sha"]
        errs = rc.validate_receipt(receipt)
        self.assertTrue(any("commit_sha" in e for e in errs))

    def test_empty_string_commit_sha_rejected(self):
        receipt = _valid_receipt()
        receipt["source_identity"]["commit_sha"] = ""
        errs = rc.validate_receipt(receipt)
        self.assertTrue(any("commit_sha" in e for e in errs))

    def test_identity_missing_classification_still_schema_valid_but_flagged_elsewhere(self):
        # The schema alone can't forbid identity-missing outright (that's the
        # toolchain_identity module's job); it just requires the classification
        # field be present and one of the enum values.
        receipt = _valid_receipt()
        receipt["toolchain_executed"]["classification"] = "identity-missing"
        self.assertEqual(rc.validate_receipt(receipt), [])


class TestCompareReceiptsNullEqualityGate(unittest.TestCase):
    def test_null_equals_null_is_rejected_as_agreement(self):
        a = _valid_receipt()
        b = _valid_receipt()
        a["toolchain_resolved"]["resolved_version"] = None
        b["toolchain_resolved"]["resolved_version"] = None
        rows = rc.compare_receipts(a, b, semantic_paths=["toolchain_resolved.resolved_version"])
        self.assertFalse(rows[0]["equal"])

    def test_empty_equals_empty_is_rejected_as_agreement(self):
        a = _valid_receipt()
        b = _valid_receipt()
        a["toolchain_resolved"]["runtime_identifier"] = ""
        b["toolchain_resolved"]["runtime_identifier"] = ""
        rows = rc.compare_receipts(a, b, semantic_paths=["toolchain_resolved.runtime_identifier"])
        self.assertFalse(rows[0]["equal"])

    def test_same_real_value_is_accepted_as_agreement(self):
        a = _valid_receipt()
        b = _valid_receipt()
        rows = rc.compare_receipts(a, b, semantic_paths=["toolchain_resolved.resolved_version"])
        self.assertTrue(rows[0]["equal"])

    def test_different_real_values_rejected(self):
        a = _valid_receipt()
        b = _valid_receipt()
        b["toolchain_resolved"]["resolved_version"] = "10.0.301"
        rows = rc.compare_receipts(a, b, semantic_paths=["toolchain_resolved.resolved_version"])
        self.assertFalse(rows[0]["equal"])

    def test_one_side_missing_rejected(self):
        a = _valid_receipt()
        b = _valid_receipt()
        del b["toolchain_resolved"]["resolved_version"]
        rows = rc.compare_receipts(a, b, semantic_paths=["toolchain_resolved.resolved_version"])
        self.assertFalse(rows[0]["equal"])

    def test_binary_hash_comparison_not_loosened(self):
        a = _valid_receipt()
        b = _valid_receipt()
        b["toolchain_executed"]["executed_binary_sha256"] = "9" * 64
        rows = rc.compare_receipts(a, b, semantic_paths=["toolchain_executed.executed_binary_sha256"])
        self.assertFalse(rows[0]["equal"])

    def test_identically_failing_receipts_are_field_reproducible_but_that_is_not_this_modules_job(self):
        # compare_receipts only reports field agreement; whether exit_code==0
        # gates overall N2-A/N2-B acceptance is a caller concern (mirrors the
        # N2-A build_succeeded fix) — both receipts sharing exit_code=137
        # agrees on the field without this module claiming overall success.
        a = _valid_receipt()
        b = _valid_receipt()
        a["termination"]["exit_code"] = 137
        b["termination"]["exit_code"] = 137
        rows = rc.compare_receipts(a, b, semantic_paths=["termination.exit_code"])
        self.assertTrue(rows[0]["equal"])
        self.assertNotEqual(a["termination"]["exit_code"], 0)


if __name__ == "__main__":
    unittest.main()
