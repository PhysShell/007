"""Receipt integrity, provenance and license checks."""
import tempfile
import unittest
from pathlib import Path

import corpus_tool as ct
import corpus_testutil as U
import snapshots as snap


class TestReceipts(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = U.make_temp_corpus(Path(self.tmp.name))
        self.bundle = self.root / "examples" / U.DEMO_ID

    def tearDown(self):
        self.tmp.cleanup()

    def test_receipt_hash_mismatch_fails(self):
        # tamper a receipt without updating the snapshot-manifest -> hash mismatch
        r = U.load(self.bundle / snap.NATIVE_RECEIPT)
        r["exit_code"] = 999
        U.dump(self.bundle / snap.NATIVE_RECEIPT, r)
        v = U.run_validate()
        self.assertTrue(U.has_code(v, "hash"))

    def test_missing_provenance_fails(self):
        (self.bundle / "provenance.json").unlink()
        v = U.run_validate()
        self.assertTrue(U.has_code(v, "missing-provenance"))

    def test_missing_license_fails(self):
        p = self.bundle / "provenance.json"
        prov = U.load(p)
        prov["license"] = ""
        U.dump(p, prov)
        v = U.run_validate()
        self.assertTrue(U.has_code(v, "schema") or U.has_code(v, "license"))

    def test_native_receipt_records_exact_argv(self):
        r = U.load(self.bundle / snap.NATIVE_RECEIPT)
        self.assertEqual(r["argv"], ["python3", "fixture/demo_tool.py"])
        self.assertEqual(r["phase"], "native")

    def test_rtk_receipt_records_pipe_and_source(self):
        r = U.load(self.bundle / snap.RTK_RECEIPT)
        self.assertIn("pipe", r["rtk_argv"])
        self.assertEqual(r["tool_identity"], "rtk")
        self.assertEqual(r["rtk_source_sha"], "5d32d0736f686b69d1e8b9dc45c007d4eb77a0a2")
        self.assertEqual(r["exit_code"], 0)


if __name__ == "__main__":
    unittest.main()
