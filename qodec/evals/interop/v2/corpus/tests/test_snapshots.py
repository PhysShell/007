"""Snapshot integrity, qodec-leakage and evidence-span validation."""
import tempfile
import unittest
from pathlib import Path

import corpus_tool as ct
import corpus_testutil as U
import snapshots as snap


class TestSnapshots(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = U.make_temp_corpus(Path(self.tmp.name))
        self.bundle = self.root / "examples" / U.DEMO_ID

    def tearDown(self):
        self.tmp.cleanup()

    def _rebuild_manifest(self):
        ct.rebuild_snapshot_manifest(U.DEMO_ID, self.bundle)

    def test_snapshot_hash_mismatch_fails(self):
        (self.bundle / snap.RAW_STDOUT).write_bytes(b"tampered\n")
        v = U.run_validate()
        self.assertTrue(U.has_code(v, "hash"))

    def test_qodec_snapshot_leakage_fails(self):
        (self.bundle / "snapshots" / "qodec.stdout").write_bytes(b"derived\n")
        v = U.run_validate()
        self.assertTrue(U.has_code(v, "qodec-leak"))

    def test_evidence_span_outside_file_fails(self):
        ev_p = self.bundle / "evidence-map.json"
        ev = U.load(ev_p)
        ev["facts"][0]["evidence"] = [{"start_line": 999, "end_line": 1000}]
        U.dump(ev_p, ev)
        self._rebuild_manifest()
        v = U.run_validate()
        self.assertTrue(U.has_code(v, "evidence-span"))

    def test_evidence_literal_absent_fails(self):
        ev_p = self.bundle / "evidence-map.json"
        ev = U.load(ev_p)
        ev["facts"][0]["value"] = "NOT-IN-SPAN-XYZ"
        U.dump(ev_p, ev)
        self._rebuild_manifest()
        v = U.run_validate()
        self.assertTrue(U.has_code(v, "evidence-literal"))

    def test_rtk_empty_output_fails_for_nonempty_required(self):
        (self.bundle / snap.RTK_STDOUT).write_bytes(b"")
        self._rebuild_manifest()
        v = U.run_validate()
        self.assertTrue(U.has_code(v, "rtk-empty"))


if __name__ == "__main__":
    unittest.main()
