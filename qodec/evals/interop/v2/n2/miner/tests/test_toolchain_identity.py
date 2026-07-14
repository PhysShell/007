"""Tests for toolchain_identity.py — requested/resolved/executed separation
and classification (section 10). Direct regression coverage for the N2-A
finding: a workflow requested 8.0.x but 10.0.301 actually executed."""
import sys
import unittest
from pathlib import Path

MINER_DIR = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(MINER_DIR / "tools"))
import toolchain_identity as ti  # noqa: E402


def _kwargs(**overrides):
    base = dict(
        requested_version_or_range="8.0.x",
        resolved_version="8.0.404",
        runtime_identifier="linux-x64",
        resolved_executable_path="/usr/share/dotnet/dotnet",
        executed_binary_absolute_path="/usr/share/dotnet/dotnet",
        executed_binary_sha256="deadbeef" * 8,
    )
    base.update(overrides)
    return base


class TestClassification(unittest.TestCase):
    def test_exact_match(self):
        self.assertEqual(ti.classify(**_kwargs(requested_version_or_range="8.0.404")), "exact-match")

    def test_compatible_resolution_within_range(self):
        self.assertEqual(ti.classify(**_kwargs()), "compatible-resolution")

    def test_unexpected_resolution_n2a_regression_case(self):
        # The actual N2-A finding: requested 8.0.x, but 10.0.301 executed.
        self.assertEqual(
            ti.classify(**_kwargs(requested_version_or_range="8.0.x", resolved_version="10.0.301")),
            "unexpected-resolution",
        )

    def test_identity_missing_when_resolved_version_absent(self):
        self.assertEqual(ti.classify(**_kwargs(resolved_version=None)), "identity-missing")

    def test_identity_missing_when_executed_binary_sha256_absent(self):
        self.assertEqual(ti.classify(**_kwargs(executed_binary_sha256=None)), "identity-missing")

    def test_identity_missing_when_all_fields_empty_string(self):
        self.assertEqual(
            ti.classify(**_kwargs(resolved_version="", runtime_identifier="", resolved_executable_path="",
                                   executed_binary_absolute_path="", executed_binary_sha256="")),
            "identity-missing",
        )

    def test_is_hard_failure_flags_only_identity_missing(self):
        self.assertTrue(ti.is_hard_failure("identity-missing"))
        for other in ("exact-match", "compatible-resolution", "unexpected-resolution"):
            self.assertFalse(ti.is_hard_failure(other))


class TestRequestedResolvedExecutedStayDistinct(unittest.TestCase):
    def test_build_toolchain_identity_keeps_three_sections_separate(self):
        identity = ti.build_toolchain_identity(
            requested_version_or_range="8.0.x", resolver_mechanism="global.json", **{
                k: v for k, v in _kwargs().items() if k != "requested_version_or_range"
            },
        )
        self.assertIn("toolchain_requested", identity)
        self.assertIn("toolchain_resolved", identity)
        self.assertIn("toolchain_executed", identity)
        self.assertNotIn("resolved_version", identity["toolchain_requested"])
        self.assertNotIn("requested_version_or_range", identity["toolchain_resolved"])
        self.assertNotIn("resolved_version", identity["toolchain_executed"])

    def test_classification_embedded_in_executed_section(self):
        identity = ti.build_toolchain_identity(
            requested_version_or_range="8.0.x", resolver_mechanism="global.json", **{
                k: v for k, v in _kwargs(resolved_version="10.0.301").items()
                if k != "requested_version_or_range"
            },
        )
        self.assertEqual(identity["toolchain_executed"]["classification"], "unexpected-resolution")


if __name__ == "__main__":
    unittest.main()
