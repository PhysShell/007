"""Tests for the committed synthetic positive receipt fixture
(fixtures/receipts/valid-receipt.json) and unit-generated negative cases
mutated from it. This is the fixture the schema-validation CI job actually
validates against receipt-contract.schema.json (closure item 2)."""
import json
import sys
import unittest
from pathlib import Path

MINER_DIR = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(MINER_DIR / "tools"))
import receipt_contract as rc  # noqa: E402

FIXTURE_PATH = MINER_DIR / "fixtures" / "receipts" / "valid-receipt.json"


def _load_fixture() -> dict:
    return json.loads(FIXTURE_PATH.read_text())


class TestValidReceiptFixture(unittest.TestCase):
    def test_fixture_conforms_to_receipt_contract_schema(self):
        self.assertEqual(rc.validate_receipt(_load_fixture()), [])

    def test_fixture_exit_code_is_zero(self):
        self.assertEqual(_load_fixture()["termination"]["exit_code"], 0)

    def test_fixture_carries_no_external_source_or_secret(self):
        blob = json.dumps(_load_fixture()).lower()
        for forbidden in ("github.com", "http://", "https://", "password", "token", "secret", "api_key"):
            self.assertNotIn(forbidden, blob)

    def test_fixture_every_mandatory_identity_field_is_non_empty(self):
        receipt = _load_fixture()
        for path in rc.REQUIRE_NON_EMPTY_PATHS:
            value = rc._get_path(receipt, path)
            self.assertTrue(rc._is_present(path, value), f"{path} is not present: {value!r}")


class TestNegativeReceiptCases(unittest.TestCase):
    """Each case mutates a copy of the valid fixture to violate exactly one
    mandatory-field requirement and asserts the schema validator rejects it."""

    def setUp(self):
        self.receipt = _load_fixture()

    def test_missing_source_commit_sha_rejected(self):
        del self.receipt["source_identity"]["commit_sha"]
        errs = rc.validate_receipt(self.receipt)
        self.assertTrue(any("commit_sha" in e for e in errs))

    def test_empty_archive_sha256_rejected(self):
        self.receipt["source_identity"]["archive_sha256"] = ""
        errs = rc.validate_receipt(self.receipt)
        self.assertTrue(any("archive_sha256" in e for e in errs))

    def test_empty_adapter_version_rejected(self):
        self.receipt["adapter_identity"]["version"] = ""
        errs = rc.validate_receipt(self.receipt)
        self.assertTrue(any("version" in e for e in errs))

    def test_missing_resolved_toolchain_version_rejected(self):
        del self.receipt["toolchain_resolved"]["resolved_version"]
        errs = rc.validate_receipt(self.receipt)
        self.assertTrue(any("resolved_version" in e for e in errs))

    def test_empty_executed_binary_path_rejected(self):
        self.receipt["toolchain_executed"]["executed_binary_absolute_path"] = ""
        errs = rc.validate_receipt(self.receipt)
        self.assertTrue(any("executed_binary_absolute_path" in e for e in errs))

    def test_missing_stdout_hash_rejected(self):
        del self.receipt["stdout_identity"]["sha256"]
        errs = rc.validate_receipt(self.receipt)
        self.assertTrue(any("stdout_identity" in e for e in errs))

    def test_missing_stderr_hash_rejected(self):
        del self.receipt["stderr_identity"]["sha256"]
        errs = rc.validate_receipt(self.receipt)
        self.assertTrue(any("stderr_identity" in e for e in errs))

    def test_missing_exit_code_rejected(self):
        del self.receipt["termination"]["exit_code"]
        errs = rc.validate_receipt(self.receipt)
        self.assertTrue(any("exit_code" in e for e in errs))

    def test_boolean_exit_code_rejected_by_presence_check(self):
        # The schema alone can't reject a bool (it's a valid "integer" in
        # Python's type system); the type-aware _is_present() gate is what
        # actually excludes it — verified directly here since this fixture
        # is exactly the receipt shape that gate is meant to guard.
        self.receipt["termination"]["exit_code"] = False
        self.assertFalse(rc._is_present("termination.exit_code", self.receipt["termination"]["exit_code"]))


if __name__ == "__main__":
    unittest.main()
