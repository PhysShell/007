"""The acceptance invariant of the corrective round:

    same gold + different tasks -> different byte-stable projections

Proven twice: on synthetic state (always runnable), and on the REAL committed
case-0001 gold state (also always runnable — projection needs no CAS).
"""
import json
import os
import unittest

import yaml

from o7b1 import projector as pj
from o7b1 import selector as sl
from o7b1.canonical import canonical_file_bytes

# `unittest discover -s tests` makes this directory the top-level package
# root, so the shared fixtures are imported as a plain sibling module.
import synth

HERE = os.path.dirname(__file__)
FDIR = os.path.normpath(os.path.join(HERE, "..", "..", "fixtures", "case-0001"))


def _yaml(name):
    with open(os.path.join(FDIR, name), encoding="utf-8") as fh:
        return yaml.safe_load(fh)


def _gold():
    with open(os.path.join(FDIR, "gold-state-v0.json"), encoding="utf-8") as fh:
        return json.load(fh)


class SyntheticTaskDependenceTest(unittest.TestCase):
    def setUp(self):
        self.gold = synth.gold()
        self.a = pj.select(self.gold, synth.task_a(), byte_budget=20000, record_budget=32)
        self.b = pj.select(self.gold, synth.task_b(), byte_budget=20000, record_budget=32)

    def test_selections_differ(self):
        self.assertNotEqual(set(self.a["selected_ids"]), set(self.b["selected_ids"]))

    def test_neither_is_a_superset(self):
        sa, sb = set(self.a["selected_ids"]), set(self.b["selected_ids"])
        self.assertFalse(sa <= sb)
        self.assertFalse(sb <= sa)

    def test_each_task_gets_its_own_required_state(self):
        sa, sb = set(self.a["selected_ids"]), set(self.b["selected_ids"])
        for q in synth.QUESTIONS_A["questions"]:
            self.assertTrue(set(q["required_observation_ids"]) <= sa, q["id"])
        for q in synth.QUESTIONS_B["questions"]:
            self.assertTrue(set(q["required_observation_ids"]) <= sb, q["id"])

    def test_irrelevant_observations_omitted_with_reasons(self):
        omitted = {o["observation_id"]: o["reason"] for o in self.a["omitted"]}
        # beta-only state must not reach the alpha task
        self.assertIn("obs-beta-status", omitted)
        self.assertIn("not relevant to this task", omitted["obs-beta-status"])
        for reason in omitted.values():
            self.assertTrue(reason.strip())

    def test_context_bytes_differ(self):
        ja = canonical_file_bytes(pj.build_context_json(self.gold, synth.task_a(), self.a))
        jb = canonical_file_bytes(pj.build_context_json(self.gold, synth.task_b(), self.b))
        self.assertNotEqual(ja, jb)

    def test_deterministic_across_runs(self):
        again = pj.select(self.gold, synth.task_a(), byte_budget=20000, record_budget=32)
        j1 = canonical_file_bytes(pj.build_context_json(self.gold, synth.task_a(), self.a))
        j2 = canonical_file_bytes(pj.build_context_json(self.gold, synth.task_a(), again))
        self.assertEqual(j1, j2)


class RealFixtureTaskDependenceTest(unittest.TestCase):
    """Same invariant on the committed case-0001 gold state. No CAS needed."""

    def setUp(self):
        self.gold = _gold()
        self.task_a = _yaml("task-v0.yaml")
        self.task_b = _yaml("task-b-v0.yaml")
        self.qa = _yaml("questions-v0.yaml")
        self.qb = _yaml("questions-b-v0.yaml")
        self.a = pj.select(self.gold, self.task_a, byte_budget=20000, record_budget=32)
        self.b = pj.select(self.gold, self.task_b, byte_budget=20000, record_budget=32)

    def test_projections_differ_and_neither_is_a_superset(self):
        sa, sb = set(self.a["selected_ids"]), set(self.b["selected_ids"])
        self.assertNotEqual(sa, sb)
        self.assertFalse(sa <= sb)
        self.assertFalse(sb <= sa)
        self.assertGreater(len(sa ^ sb), 0)

    def test_not_every_eligible_observation_is_selected(self):
        """The pre-corrective projector selected all 16 eligible observations for
        any task; that tautology is what made recall meaningless."""
        eligible = [o for o in self.gold["observations"] if sl.eligible(o)]
        self.assertLess(len(self.a["selected"]), len(eligible))
        self.assertLess(len(self.b["selected"]), len(eligible))

    def test_each_task_answers_its_own_questions(self):
        sa, sb = set(self.a["selected_ids"]), set(self.b["selected_ids"])
        for q in self.qa["questions"]:
            self.assertTrue(set(q["required_observation_ids"]) <= sa, q["id"])
        for q in self.qb["questions"]:
            self.assertTrue(set(q["required_observation_ids"]) <= sb, q["id"])

    def test_task_b_does_not_receive_task_a_only_state(self):
        sb = set(self.b["selected_ids"])
        self.assertNotIn("obs-b1-hypothesis", sb)
        self.assertNotIn("obs-next-step-extractors-schema", sb)

    def test_task_a_does_not_receive_capture_topology_detail(self):
        sa = set(self.a["selected_ids"])
        self.assertNotIn("obs-bd-one-conversation", sa)
        self.assertNotIn("obs-byte-prefix-scope", sa)

    def test_both_projections_are_valid(self):
        for sel in (self.a, self.b):
            f = pj.validate(self.gold, sel, byte_budget=20000, record_budget=32)
            self.assertTrue(f["valid"], f)

    def test_questions_declare_their_task(self):
        self.assertEqual(self.qa["task_id"], self.task_a["task_id"])
        self.assertEqual(self.qb["task_id"], self.task_b["task_id"])


if __name__ == "__main__":
    unittest.main()
