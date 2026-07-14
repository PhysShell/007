"""Tests for sanitizer_contract.py — generic SanitizerContract (section 15)."""
import sys
import unittest
from pathlib import Path

MINER_DIR = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(MINER_DIR / "tools"))
import sanitizer_contract as sc  # noqa: E402
from adapters import dotnet_adapter, rust_adapter, python_adapter, maven_adapter, gradle_adapter  # noqa: E402


class TestValidProfileAccepted(unittest.TestCase):
    def test_generic_profile_is_valid(self):
        from adapters import base
        self.assertEqual(sc.validate_profile(base.generic_sanitizer_profile()), [])

    def test_every_adapter_profile_is_valid(self):
        for adapter in (dotnet_adapter, rust_adapter, python_adapter, maven_adapter, gradle_adapter):
            errors = sc.validate_profile(adapter.sanitizer_profile())
            self.assertEqual(errors, [], f"{adapter.__name__} sanitizer profile invalid: {errors}")


class TestForbiddenTransformationsRejected(unittest.TestCase):
    def test_dedup_transformation_rejected(self):
        profile = {"profile_version": "v1", "transformations": ["dedup_lines"]}
        errors = sc.validate_profile(profile)
        self.assertTrue(errors)

    def test_reorder_transformation_rejected(self):
        profile = {"profile_version": "v1", "transformations": ["reorder_by_severity"]}
        errors = sc.validate_profile(profile)
        self.assertTrue(errors)

    def test_truncate_transformation_rejected(self):
        profile = {"profile_version": "v1", "transformations": ["truncate_long_lines"]}
        errors = sc.validate_profile(profile)
        self.assertTrue(errors)

    def test_token_reduction_transformation_rejected(self):
        profile = {"profile_version": "v1", "transformations": ["token_reduction_pass"]}
        errors = sc.validate_profile(profile)
        self.assertTrue(errors)

    def test_strip_warning_transformation_rejected(self):
        profile = {"profile_version": "v1", "transformations": ["strip_warning_lines"]}
        errors = sc.validate_profile(profile)
        self.assertTrue(errors)

    def test_missing_transformations_list_rejected(self):
        profile = {"profile_version": "v1", "transformations": []}
        errors = sc.validate_profile(profile)
        self.assertTrue(errors)


class TestTransformationReceipt(unittest.TestCase):
    def test_receipt_flags_unknown_transformation(self):
        profile = {"profile_version": "v1", "transformations": ["iso_timestamp"]}
        receipt = sc.transformation_receipt(profile, ["iso_timestamp", "made_up_transform"])
        self.assertFalse(receipt["consistent_with_profile"])
        self.assertIn("made_up_transform", receipt["unknown_transformations"])

    def test_receipt_consistent_when_applied_matches_profile(self):
        profile = {"profile_version": "v1", "transformations": ["iso_timestamp", "pid_bracket"]}
        receipt = sc.transformation_receipt(profile, ["iso_timestamp"])
        self.assertTrue(receipt["consistent_with_profile"])


if __name__ == "__main__":
    unittest.main()
