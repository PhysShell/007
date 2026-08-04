"""Fixture task registry tests — synthetic temp fixtures, no private RAW.

Proves both CLIs consume the one registry and that malformed registries fail
closed rather than being approximated.
"""
import os
import tempfile
import unittest

import yaml

import project_case
import run_case
from o7b1 import registry as rg


def _write(path, obj):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w", encoding="utf-8") as fh:
        yaml.safe_dump(obj, fh, sort_keys=False)


def _task(tid):
    return {"schema": "o7.b1.task/v0", "task_id": tid, "fixture_id": "syn",
            "statement": "s", "selectors": {"required_topics": ["alpha"]}}


def _questions(tid):
    return {"schema": "o7.b1.questions/v0", "task_id": tid,
            "questions": [{"id": "q1", "required_observation_ids": ["obs-x"]}]}


class _Fixture:
    """A temp fixture directory with a couple of task/questions files."""

    def __init__(self, tmp):
        self.dir = os.path.join(tmp, "case-syn")
        os.makedirs(self.dir)
        _write(os.path.join(self.dir, "task-a.yaml"), _task("t-a"))
        _write(os.path.join(self.dir, "q-a.yaml"), _questions("t-a"))
        _write(os.path.join(self.dir, "task-b.yaml"), _task("t-b"))
        _write(os.path.join(self.dir, "q-b.yaml"), _questions("t-b"))
        # a second task file that declares the SAME task_id as task-a (dup id)
        _write(os.path.join(self.dir, "task-a-dup.yaml"), _task("t-a"))
        _write(os.path.join(self.dir, "q-a-dup.yaml"), _questions("t-a"))
        # a questions file whose task_id disagrees with its task
        _write(os.path.join(self.dir, "q-mismatch.yaml"), _questions("t-WRONG"))

    def registry(self, obj):
        _write(os.path.join(self.dir, rg.REGISTRY_FILENAME), obj)


def _base_registry():
    return {"schema": rg.REGISTRY_SCHEMA, "fixture_id": "case-syn",
            "tasks": [{"task_file": "task-a.yaml", "questions_file": "q-a.yaml"},
                      {"task_file": "task-b.yaml", "questions_file": "q-b.yaml"}]}


class RegistryValidTest(unittest.TestCase):
    def test_loads_in_order_with_digest(self):
        with tempfile.TemporaryDirectory() as tmp:
            fx = _Fixture(tmp)
            fx.registry(_base_registry())
            reg = rg.load_task_registry(fx.dir, "case-syn")
            self.assertEqual([e["task"]["task_id"] for e in reg["entries"]], ["t-a", "t-b"])
            self.assertTrue(reg["registry_digest"].startswith("sha256:"))

    def test_both_clis_have_no_hardcoded_task_list(self):
        self.assertFalse(hasattr(project_case, "TASKS"))
        self.assertFalse(hasattr(run_case, "TASKS"))

    def test_real_fixture_registry_matches_committed_task_files(self):
        fdir = os.path.normpath(os.path.join(
            os.path.dirname(__file__), "..", "..", "fixtures", "case-0001"))
        reg = rg.load_task_registry(fdir, "case-0001")
        self.assertEqual([e["task_file"] for e in reg["entries"]],
                         ["task-v0.yaml", "task-b-v0.yaml"])


class RegistryMalformedTest(unittest.TestCase):
    def _expect_fail(self, mutate):
        with tempfile.TemporaryDirectory() as tmp:
            fx = _Fixture(tmp)
            reg = _base_registry()
            mutate(fx, reg)
            fx.registry(reg)
            with self.assertRaises(rg.RegistryError):
                rg.load_task_registry(fx.dir, "case-syn")

    def test_unknown_top_field(self):
        self._expect_fail(lambda fx, r: r.__setitem__("extra", 1))

    def test_wrong_schema(self):
        self._expect_fail(lambda fx, r: r.__setitem__("schema", "o7.b1.wrong/v9"))

    def test_wrong_fixture_id(self):
        self._expect_fail(lambda fx, r: r.__setitem__("fixture_id", "other"))

    def test_empty_tasks(self):
        self._expect_fail(lambda fx, r: r.__setitem__("tasks", []))

    def test_unknown_entry_field(self):
        self._expect_fail(lambda fx, r: r["tasks"][0].__setitem__("weight", 3))

    def test_missing_task_file_key(self):
        self._expect_fail(lambda fx, r: r["tasks"][0].pop("task_file"))

    def test_duplicate_task_file(self):
        self._expect_fail(lambda fx, r: r["tasks"].append(
            {"task_file": "task-a.yaml", "questions_file": "q-b.yaml"}))

    def test_duplicate_task_id(self):
        self._expect_fail(lambda fx, r: r["tasks"].append(
            {"task_file": "task-a-dup.yaml", "questions_file": "q-a-dup.yaml"}))

    def test_missing_referenced_file(self):
        self._expect_fail(lambda fx, r: r["tasks"][0].__setitem__("task_file", "nope.yaml"))

    def test_mismatched_question_task_id(self):
        self._expect_fail(lambda fx, r: r["tasks"][0].__setitem__("questions_file", "q-mismatch.yaml"))

    def test_absolute_path(self):
        self._expect_fail(lambda fx, r: r["tasks"][0].__setitem__("task_file", "/etc/passwd"))

    def test_parent_traversal_path(self):
        self._expect_fail(lambda fx, r: r["tasks"][0].__setitem__("task_file", "../task-a.yaml"))

    def test_subdirectory_escape(self):
        self._expect_fail(lambda fx, r: r["tasks"][0].__setitem__("task_file", "sub/task-a.yaml"))


if __name__ == "__main__":
    unittest.main()
