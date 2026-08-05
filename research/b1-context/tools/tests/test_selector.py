"""Selector-contract tests: fail-closed behaviour and deterministic ranking.

`o7.b1.selector/v0` refuses anything it cannot interpret exactly, rather than
approximating a task it does not understand.
"""
import unittest

from o7b1 import projector as pj
from o7b1 import selector as sl

# `unittest discover -s tests` makes this directory the top-level package
# root, so the shared fixtures are imported as a plain sibling module.
import synth


def _task(**selectors):
    t = synth.task_a()
    t["selectors"].update(selectors)
    return t


class FailClosedTest(unittest.TestCase):
    def test_unknown_selector_field_rejected(self):
        t = _task(importance_weights={"alpha": 3})
        with self.assertRaises(sl.SelectorError) as cm:
            pj.select(synth.gold(), t, byte_budget=20000, record_budget=32)
        self.assertIn("unknown selector field", str(cm.exception))

    def test_unknown_topic_rejected(self):
        t = _task(required_topics=["alpha", "does-not-exist"])
        with self.assertRaises(sl.SelectorError) as cm:
            pj.select(synth.gold(), t, byte_budget=20000, record_budget=32)
        self.assertIn("topic_vocabulary", str(cm.exception))

    def test_unknown_required_kind_rejected(self):
        t = _task(required_kinds=["goal", "not-a-kind"])
        with self.assertRaises(sl.SelectorError):
            pj.select(synth.gold(), t, byte_budget=20000, record_budget=32)

    def test_missing_selectors_block_rejected(self):
        t = synth.task_a()
        del t["selectors"]
        with self.assertRaises(sl.SelectorError) as cm:
            pj.select(synth.gold(), t, byte_budget=20000, record_budget=32)
        self.assertIn("no `selectors` block", str(cm.exception))

    def test_empty_topic_selection_rejected(self):
        t = _task(required_topics=[], preferred_topics=[])
        with self.assertRaises(sl.SelectorError):
            pj.select(synth.gold(), t, byte_budget=20000, record_budget=32)

    def test_gold_without_topic_vocabulary_rejected(self):
        g = synth.gold()
        del g["topic_vocabulary"]
        with self.assertRaises(sl.SelectorError) as cm:
            pj.select(g, synth.task_a(), byte_budget=20000, record_budget=32)
        self.assertIn("topic_vocabulary", str(cm.exception))

    def test_observation_without_topics_rejected(self):
        g = synth.gold()
        for o in g["observations"]:
            if o["observation_id"] == "obs-goal":
                del o["topics"]
        with self.assertRaises(sl.SelectorError) as cm:
            pj.select(g, synth.task_a(), byte_budget=20000, record_budget=32)
        self.assertIn("carries no topics", str(cm.exception))


class AnchorTest(unittest.TestCase):
    def test_anchor_without_rationale_rejected(self):
        t = _task(anchor_observation_ids=["obs-beta-status"])
        with self.assertRaises(sl.SelectorError) as cm:
            pj.select(synth.gold(), t, byte_budget=20000, record_budget=32)
        self.assertIn("anchor_rationale", str(cm.exception))

    def test_anchor_naming_unknown_observation_rejected(self):
        t = _task(anchor_observation_ids=["obs-nope"],
                  anchor_rationale={"obs-nope": "because"})
        with self.assertRaises(sl.SelectorError):
            pj.select(synth.gold(), t, byte_budget=20000, record_budget=32)

    def test_anchor_may_not_resurrect_ineligible_state(self):
        t = _task(anchor_observation_ids=["obs-claim-separate"],
                  anchor_rationale={"obs-claim-separate": "I want the superseded belief"})
        with self.assertRaises(sl.SelectorError) as cm:
            pj.select(synth.gold(), t, byte_budget=20000, record_budget=32)
        self.assertIn("not eligible", str(cm.exception))

    def test_justified_single_anchor_is_accepted(self):
        t = _task(anchor_observation_ids=["obs-beta-status"],
                  anchor_rationale={"obs-beta-status":
                                    "alpha's next step is blocked on beta's capture status"})
        sel = pj.select(synth.gold(), t, byte_budget=20000, record_budget=32)
        self.assertIn("obs-beta-status", sel["selected_ids"])

    def test_answer_key_task_is_refused(self):
        """A task that names most of its result by id is not a selector."""
        t = _task(required_topics=["alpha"], preferred_topics=[], required_kinds=[],
                  anchor_observation_ids=["obs-beta-status", "obs-truth-topology"],
                  anchor_rationale={"obs-beta-status": "x", "obs-truth-topology": "y"})
        with self.assertRaises(sl.SelectorError) as cm:
            pj.select(synth.gold(), t, byte_budget=20000, record_budget=32)
        self.assertIn("answer key", str(cm.exception))


class RankingTest(unittest.TestCase):
    def test_required_topic_outranks_preferred(self):
        gold = synth.gold()
        spec = sl.parse_selectors(synth.task_b(), gold)
        by_id = {o["observation_id"]: o for o in gold["observations"]}
        # obs-truth-topology has both `beta` (required) and `shared` (preferred)
        # obs-beta-status has only `beta`
        self.assertGreater(sl.score(by_id["obs-truth-topology"], spec),
                           sl.score(by_id["obs-beta-status"], spec))

    def test_excluded_topic_beats_required(self):
        t = _task(required_topics=["alpha"], excluded_topics=["shared"])
        sel = pj.select(synth.gold(), t, byte_budget=20000, record_budget=32)
        self.assertNotIn("obs-alpha-constraint", sel["selected_ids"])
        reason = next(o["reason"] for o in sel["omitted"]
                      if o["observation_id"] == "obs-alpha-constraint")
        self.assertIn("excluded by task selector", reason)

    def test_budget_truncation_is_reported_not_silent(self):
        sel = pj.select(synth.gold(), synth.task_a(), byte_budget=200, record_budget=32)
        reasons = [o["reason"] for o in sel["omitted"]]
        self.assertTrue(any("budget exceeded" in r for r in reasons))

    def test_selector_ref_is_digest_bound(self):
        sel = pj.select(synth.gold(), synth.task_a(), byte_budget=20000, record_budget=32)
        ref = sel["selector_ref"]
        self.assertEqual(ref["selector_id"], "o7.b1.selector/v0")
        self.assertTrue(ref["selector_impl_digest"].startswith("sha256:"))
        self.assertTrue(ref["selectors_digest"].startswith("sha256:"))
        # a different task's selector block must produce a different digest
        other = pj.select(synth.gold(), synth.task_b(), byte_budget=20000, record_budget=32)
        self.assertNotEqual(ref["selectors_digest"], other["selector_ref"]["selectors_digest"])


if __name__ == "__main__":
    unittest.main()
