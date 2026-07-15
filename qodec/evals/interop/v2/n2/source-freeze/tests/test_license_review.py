"""Section 23 tests for license_review.py: hard-reject rules (section 11)."""
import sys
import unittest
from pathlib import Path

TOOLS = Path(__file__).resolve().parents[1] / "tools"
sys.path.insert(0, str(TOOLS))
import license_review  # noqa: E402


def _clear_candidate(**overrides):
    base = {
        "candidate_id": "c1",
        "public_canonical_url": "https://github.com/example/example",
        "license": {
            "status": "clear", "spdx": "MIT", "redistribution_allowed": True,
            "redistribution_basis": "MIT license permits redistribution",
            "attribution_requirements": "retain notice",
            "modification_requirements": "none",
        },
    }
    base.update(overrides)
    return base


class TestBuildAndValidate(unittest.TestCase):
    def test_clear_mit_record_passes(self):
        record = license_review.build_license_record(_clear_candidate(), ["https://github.com/example/example/blob/main/LICENSE"])
        self.assertEqual(license_review.validate_license_record(record), [])
        self.assertEqual(license_review.hard_reject_reasons(record), [])


class TestHardRejectRules(unittest.TestCase):
    def test_missing_license_rejected(self):
        c = _clear_candidate(license={"status": "missing", "spdx": None, "redistribution_allowed": False})
        record = license_review.build_license_record(c, [])
        self.assertIn("missing_license", license_review.hard_reject_reasons(record))

    def test_unclear_redistribution_basis_rejected(self):
        c = _clear_candidate(license={"status": "ambiguous", "spdx": "MIT", "redistribution_allowed": "unclear"})
        record = license_review.build_license_record(c, [])
        self.assertIn("unclear_redistribution_basis", license_review.hard_reject_reasons(record))

    def test_non_commercial_restriction_rejected(self):
        c = _clear_candidate(license={"status": "clear", "spdx": "CC-BY-NC-4.0", "redistribution_allowed": True})
        record = license_review.build_license_record(c, [])
        self.assertIn("non_commercial_restriction", license_review.hard_reject_reasons(record))

    def test_no_derivatives_restriction_rejected(self):
        c = _clear_candidate(license={"status": "clear", "spdx": "CC-BY-ND-4.0", "redistribution_allowed": True})
        record = license_review.build_license_record(c, [])
        self.assertIn("no_derivatives_restriction", license_review.hard_reject_reasons(record))

    def test_redistribution_explicitly_disallowed_rejected(self):
        c = _clear_candidate(license={"status": "clear", "spdx": "MIT", "redistribution_allowed": False})
        record = license_review.build_license_record(c, [])
        self.assertIn("unclear_redistribution_basis", license_review.hard_reject_reasons(record))


if __name__ == "__main__":
    unittest.main()
