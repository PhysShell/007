"""R2 admission-plumbing tests: negative-control cardinality, --actual-only, and
registry override. Offline parts use synthetic state / the CLI arg layer; the
CAS-backed parts skip cleanly when the owner's private blobs are absent.

None of this changes selection, evaluation, schema, budget or report meaning.
"""
import os
import tempfile
import unittest

import project_case
import run_case
from o7b1 import registry as rg


def _cas_data_root():
    dr = os.environ.get("O7B1_DATA_ROOT", os.path.expanduser("~/.local/share/o7-research"))
    probe = os.path.join(dr, "cas", "sha256", "6a",
                         "86185402fd69d2fe4aad425a257b06c9bcfc4b28decc0b690d9b314f4066b9")
    return dr if os.path.isfile(probe) else None


class NegativeControlCardinalityTest(unittest.TestCase):
    def test_empty_is_none(self):
        self.assertIsNone(run_case._negative_control_entry({"negative_control": []}))

    def test_one_is_that_entry(self):
        e = {"id": "nc", "digest": "sha256:" + "a" * 64, "byte_length": 1}
        self.assertEqual(run_case._negative_control_entry({"negative_control": [e]}), e)

    def test_two_fail_closed(self):
        with self.assertRaises(run_case.FailClosed):
            run_case._negative_control_entry({"negative_control": [{"id": "a"}, {"id": "b"}]})

    def test_missing_fail_closed(self):
        with self.assertRaises(run_case.FailClosed):
            run_case._negative_control_entry({})

    def test_non_list_fail_closed(self):
        with self.assertRaises(run_case.FailClosed):
            run_case._negative_control_entry({"negative_control": {"id": "a"}})


class ActualOnlyArgTest(unittest.TestCase):
    def test_actual_only_and_update_are_mutually_exclusive(self):
        rc = run_case.main(["--fixture", "case-0001", "--out", "/tmp/never",
                            "--actual-only", "--update-expectations"])
        self.assertEqual(rc, 2)

    def test_default_registry_is_tasks_v0(self):
        self.assertEqual(rg.REGISTRY_FILENAME, "tasks-v0.yaml")


class RegistryOverrideSafetyTest(unittest.TestCase):
    """Path-safety of --registry, on a synthetic temp fixture (no CAS)."""

    def _fixture(self, tmp):
        import yaml
        d = os.path.join(tmp, "fx")
        os.makedirs(d)
        def w(name, obj):
            with open(os.path.join(d, name), "w") as fh:
                yaml.safe_dump(obj, fh)
        w("task-a.yaml", {"schema": "o7.b1.task/v0", "task_id": "t", "fixture_id": "fx",
                          "statement": "s", "selectors": {"required_topics": ["x"]}})
        w("q-a.yaml", {"schema": "o7.b1.questions/v0", "task_id": "t",
                       "questions": [{"id": "q", "required_observation_ids": ["o"]}]})
        reg = {"schema": rg.REGISTRY_SCHEMA, "fixture_id": "fx",
               "tasks": [{"task_file": "task-a.yaml", "questions_file": "q-a.yaml"}]}
        w("tasks-v0.yaml", reg)
        w("tasks-v1.yaml", reg)
        return d

    def test_default_and_override_both_load(self):
        with tempfile.TemporaryDirectory() as tmp:
            d = self._fixture(tmp)
            self.assertEqual(rg.load_task_registry(d, "fx")["registry_filename"], "tasks-v0.yaml")
            r = rg.load_task_registry(d, "fx", "tasks-v1.yaml")
            self.assertEqual(r["registry_filename"], "tasks-v1.yaml")

    def test_unsafe_registry_names_fail_closed(self):
        with tempfile.TemporaryDirectory() as tmp:
            d = self._fixture(tmp)
            for bad in ("/etc/passwd", "../tasks-v0.yaml", "sub/tasks-v0.yaml",
                        "nope.yaml", ".."):
                with self.assertRaises(rg.RegistryError):
                    rg.load_task_registry(d, "fx", bad)


class CasBackedR2Test(unittest.TestCase):
    """Preservation + actual-only + registry override on the real fixtures."""

    def setUp(self):
        self.dr = _cas_data_root()
        if not self.dr:
            self.skipTest("local CAS not present")
        self.fdir1 = os.path.normpath(os.path.join(
            os.path.dirname(__file__), "..", "..", "fixtures", "case-0001"))

    def _committed(self, rel):
        p = os.path.normpath(os.path.join(
            os.path.dirname(__file__), "..", "..", "results", "case-0001-v0", rel))
        with open(p, "rb") as fh:
            return fh.read()

    def test_case0001_actual_only_matches_committed_report(self):
        with tempfile.TemporaryDirectory() as td:
            res = run_case.run_actual_only("case-0001", self.dr, td)
            with open(os.path.join(td, "report.json"), "rb") as fh:
                self.assertEqual(fh.read(), self._committed("report.json"))
            self.assertEqual(res["development_result"], "PASS")
        # actual-only must not have created/changed a committed expectation
        exp = os.path.join(self.fdir1, run_case.EXPECTED_REPORT_FILENAME)
        self.assertTrue(os.path.isfile(exp))  # still the original committed one

    def test_case0002_actual_only_partial_deterministic(self):
        with tempfile.TemporaryDirectory() as td1, tempfile.TemporaryDirectory() as td2:
            r1 = run_case.run_actual_only("case-0002", self.dr, td1, "tasks-v1.yaml")
            run_case.run_actual_only("case-0002", self.dr, td2, "tasks-v1.yaml")
            self.assertEqual(r1["development_result"], "PARTIAL")
            self.assertTrue(all(t["evaluation"] is None for t in r1["report"]["tasks"]))
            self.assertEqual(len(r1["report"]["tasks"]), 3)
            self.assertEqual(r1["report"]["registry_filename"], "tasks-v1.yaml")
            with open(os.path.join(td1, "report.json"), "rb") as f1, \
                 open(os.path.join(td2, "report.json"), "rb") as f2:
                self.assertEqual(f1.read(), f2.read())
        # no expectation minted for case-0002
        fdir2 = os.path.normpath(os.path.join(
            os.path.dirname(__file__), "..", "..", "fixtures", "case-0002"))
        self.assertFalse(os.path.isfile(os.path.join(fdir2, run_case.EXPECTED_REPORT_FILENAME)))

    def test_both_clis_same_registry_same_projection(self):
        # project_case (offline) and run_case (actual-only) must emit byte-identical
        # projection artifacts for the same fixture+registry.
        with tempfile.TemporaryDirectory() as tp, tempfile.TemporaryDirectory() as tr:
            project_case.run("case-0002", tp, "tasks-v1.yaml")
            run_case.run_actual_only("case-0002", self.dr, tr, "tasks-v1.yaml")
            for rel in ("projection-comparison.json",
                        "tasks/case-0002-audit-r21-evidence-provenance/context.json"):
                with open(os.path.join(tp, rel), "rb") as a, open(os.path.join(tr, rel), "rb") as b:
                    self.assertEqual(a.read(), b.read(), rel)


if __name__ == "__main__":
    unittest.main()
