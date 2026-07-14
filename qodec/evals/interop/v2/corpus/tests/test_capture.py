"""Real capture behaviour: native/RTK phases, determinism, --write guard.

Capture tests execute the real RTK binary and are skipped when it is absent.
"""
import os
import shutil
import tempfile
import unittest
from pathlib import Path

import capture
import corpus_tool as ct
import corpus_testutil as U
import snapshots as snap
from hashing import sha256_file


def _rtk_available():
    return bool(os.environ.get("RTK_BIN") and Path(os.environ["RTK_BIN"]).exists()) or bool(shutil.which("rtk"))


@unittest.skipUnless(_rtk_available(), "pinned RTK binary required")
class TestCapture(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = U.make_temp_corpus(Path(self.tmp.name))
        self.bundle = self.root / "examples" / U.DEMO_ID

    def tearDown(self):
        self.tmp.cleanup()

    def test_native_capture_records_exact_argv(self):
        with tempfile.TemporaryDirectory() as d:
            out = Path(d)
            ct._copy_bundle_inputs(U.DEMO_ID, out)
            r = ct.capture_native(U.DEMO_ID, out, out)
            self.assertEqual(r["argv"], ["python3", "fixture/demo_tool.py"])

    def test_native_capture_records_stdout_stderr_separately(self):
        with tempfile.TemporaryDirectory() as d:
            out = Path(d)
            ct._copy_bundle_inputs(U.DEMO_ID, out)
            ct.capture_native(U.DEMO_ID, out, out)
            self.assertTrue((out / snap.RAW_STDOUT).exists())
            self.assertTrue((out / snap.RAW_STDERR).exists())
            self.assertGreater((out / snap.RAW_STDOUT).stat().st_size, 0)

    def test_rtk_capture_consumes_committed_raw_bytes(self):
        with tempfile.TemporaryDirectory() as d:
            out = Path(d)
            ct._copy_bundle_inputs(U.DEMO_ID, out)
            ct.capture_native(U.DEMO_ID, out, out)
            r = ct.capture_rtk(U.DEMO_ID, out, out)
            self.assertEqual(r["stdin_sha256"], sha256_file(out / snap.RAW_STDOUT))

    def test_rtk_capture_invokes_real_rtk_pipe(self):
        with tempfile.TemporaryDirectory() as d:
            out = Path(d)
            ct._copy_bundle_inputs(U.DEMO_ID, out)
            ct.capture_native(U.DEMO_ID, out, out)
            r = ct.capture_rtk(U.DEMO_ID, out, out)
            self.assertIn("pipe", r["rtk_argv"])
            self.assertEqual(r["exit_code"], 0)
            self.assertGreater((out / snap.RTK_STDOUT).stat().st_size, 0)

    def test_rtk_nonzero_exit_fails(self):
        with tempfile.TemporaryDirectory() as d:
            out = Path(d)
            ct._copy_bundle_inputs(U.DEMO_ID, out)
            recipe = U.load(out / "capture-recipe.json")
            recipe["rtk"]["argv"] = ["rtk", "pipe", "--filter", "totally-unknown-filter-xyz"]
            U.dump(out / "capture-recipe.json", recipe)
            ct.capture_native(U.DEMO_ID, out, out)
            with self.assertRaises(capture.CaptureError):
                ct.capture_rtk(U.DEMO_ID, out, out)

    def test_two_independent_captures_are_byte_identical(self):
        a = ct.capture_into_temp(U.DEMO_ID)
        b = ct.capture_into_temp(U.DEMO_ID)
        try:
            for rel in ct.SNAP_FILES:
                self.assertEqual(sha256_file(a / rel), sha256_file(b / rel), f"nondeterministic: {rel}")
        finally:
            shutil.rmtree(a, ignore_errors=True)
            shutil.rmtree(b, ignore_errors=True)

    def test_regenerate_without_write_cannot_modify_files(self):
        before = {p: sha256_file(p) for p in self.bundle.rglob("*") if p.is_file()}
        ns = type("NS", (), {"case": U.DEMO_ID, "write": False})
        rc = ct.cmd_regenerate(ns)
        after = {p: sha256_file(p) for p in self.bundle.rglob("*") if p.is_file()}
        self.assertEqual(before, after, "compare-only regenerate must not modify the working tree")
        self.assertEqual(rc, 0)


if __name__ == "__main__":
    unittest.main()
