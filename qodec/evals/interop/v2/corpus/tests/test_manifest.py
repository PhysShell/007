"""Manifest membership, demonstration-leakage and changed-file mapping."""
import io
import tempfile
import unittest
from contextlib import redirect_stdout, redirect_stderr
from pathlib import Path

import corpus_tool as ct
import corpus_testutil as U


class TestManifest(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = U.make_temp_corpus(Path(self.tmp.name))

    def tearDown(self):
        self.tmp.cleanup()

    def test_demonstration_cannot_enter_benchmark_cases(self):
        m = U.load(ct.MANIFEST_PATH)
        m["benchmark_cases"] = ["deterministic-log-demo"]
        U.dump(ct.MANIFEST_PATH, m)
        v = U.run_validate()
        self.assertTrue(U.has_code(v, "benchmark-data") or U.has_code(v, "benchmark-leak"))

    def test_duplicate_case_id_fails(self):
        m = U.load(ct.MANIFEST_PATH)
        m["demonstration_cases"] = ["deterministic-log-demo", "deterministic-log-demo"]
        U.dump(ct.MANIFEST_PATH, m)
        v = U.run_validate()
        self.assertTrue(U.has_code(v, "schema"))  # uniqueItems on demonstration_cases

    def test_demo_promoted_by_retagging_fails(self):
        case_p = self.root / "examples" / U.DEMO_ID / "case.json"
        case = U.load(case_p)
        case["status"] = "frozen-public-development"
        U.dump(case_p, case)
        v = U.run_validate()
        self.assertTrue(U.has_code(v, "status"))

    def _changed(self, argv):
        out = io.StringIO()
        with redirect_stdout(out), redirect_stderr(io.StringIO()):
            ns = type("NS", (), {"files": argv, "base": None, "head": None})
            ct.cmd_changed(ns)
        return [l for l in out.getvalue().splitlines() if l and not l.startswith("#")]

    def test_changed_files_maps_to_correct_case_id(self):
        changed = self._changed("qodec/evals/interop/v2/corpus/examples/deterministic-log-demo/fixture/demo_tool.py")
        self.assertEqual(changed, ["deterministic-log-demo"])

    def test_documentation_only_change_selects_no_case(self):
        changed = self._changed("qodec/evals/interop/v2/corpus/README.md,qodec/evals/interop/v2/README.md")
        self.assertEqual(changed, [])


if __name__ == "__main__":
    unittest.main()
