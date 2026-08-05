"""Adversarial projection-validity tests.

Corrective round `#issuecomment-5182632187` §3/§6: the previous evaluator could
not report a non-zero `unsupported_claim_count` / `contradiction_count` /
`supersession_error_count` for the projection arm at all — the helper that
produced them returned `[]` for every arm except the negative control. The
projection's zeroes were control flow, not measurement.

Each test here builds a DELIBERATELY MALFORMED projection and asserts the
corresponding counter rises and `valid` goes false. If any of these ever passes
silently, the metric has stopped measuring again.
"""
import unittest

from o7b1 import projector as pj
from o7b1 import validity as vl

# `unittest discover -s tests` makes this directory the top-level package
# root, so the shared fixtures are imported as a plain sibling module.
import synth


def _check(gold, spec, selected, used_bytes=100, budget=None):
    return vl.check_projection(gold, spec, selected, used_bytes,
                               budget or {"byte_budget": 20000, "record_budget": 32})


def _by_id(gold, oid):
    return next(o for o in gold["observations"] if o["observation_id"] == oid)


class BaselineTest(unittest.TestCase):
    def test_honest_projection_is_valid(self):
        gold = synth.gold()
        sel = pj.select(gold, synth.task_a(), byte_budget=20000, record_budget=32)
        f = pj.validate(gold, sel, byte_budget=20000, record_budget=32)
        self.assertTrue(f["valid"], f)
        self.assertEqual(f["agent_claim_presented_count"], 0)
        self.assertEqual(f["superseded_presented_count"], 0)
        self.assertEqual(f["contradiction_pair_count"], 0)
        self.assertEqual(f["incomplete_provenance_count"], 0)


class AdversarialTest(unittest.TestCase):
    """Six malformed projections; each must be caught."""

    def setUp(self):
        self.gold = synth.gold()
        self.sel = pj.select(self.gold, synth.task_a(), byte_budget=20000, record_budget=32)
        self.spec = self.sel["selector_spec"]
        self.honest = list(self.sel["selected"])

    # 1
    def test_agent_claim_presented_as_authoritative_is_caught(self):
        bad = self.honest + [_by_id(self.gold, "obs-claim-separate")]
        f = _check(self.gold, self.spec, bad)
        self.assertEqual(f["agent_claim_presented_count"], 1)
        self.assertIn("obs-claim-separate", f["agent_claim_presented_ids"])
        self.assertFalse(f["valid"])

    # 2
    def test_superseded_observation_presented_is_caught(self):
        bad = self.honest + [_by_id(self.gold, "obs-claim-separate")]
        f = _check(self.gold, self.spec, bad)
        self.assertEqual(f["superseded_presented_count"], 1)
        self.assertEqual(f["not_in_force_presented_count"], 1)
        self.assertFalse(f["valid"])

    # 3
    def test_both_sides_of_a_contradiction_is_caught(self):
        bad = self.honest + [_by_id(self.gold, "obs-claim-separate"),
                             _by_id(self.gold, "obs-truth-topology")]
        f = _check(self.gold, self.spec, bad)
        self.assertEqual(f["contradiction_pair_count"], 1)
        self.assertEqual(f["contradiction_pairs"],
                         [["obs-claim-separate", "obs-truth-topology"]])
        self.assertFalse(f["valid"])

    # 4
    def test_incomplete_provenance_is_caught(self):
        broken = dict(_by_id(self.gold, "obs-goal"))
        broken["provenance"] = [{"source_id": "readme"}]  # no digest, no pointer
        bad = [broken] + [o for o in self.honest if o["observation_id"] != "obs-goal"]
        f = _check(self.gold, self.spec, bad)
        self.assertEqual(f["incomplete_provenance_count"], 1)
        self.assertIn("obs-goal", f["incomplete_provenance_ids"])
        self.assertFalse(f["valid"])
        self.assertLess(vl.presented_provenance_ratio(bad), 1.0)

    def test_empty_provenance_is_caught(self):
        broken = dict(_by_id(self.gold, "obs-goal"))
        broken["provenance"] = []
        f = _check(self.gold, self.spec, [broken])
        self.assertEqual(f["incomplete_provenance_count"], 1)
        self.assertFalse(f["valid"])

    # 5
    def test_missing_required_kind_is_caught(self):
        bad = [o for o in self.honest if o["kind"] != "next_action"]
        f = _check(self.gold, self.spec, bad)
        self.assertIn("next_action", f["missing_required_kinds"])
        self.assertFalse(f["valid"])

    def test_missing_required_topic_is_caught(self):
        bad = [o for o in self.honest if "alpha" not in (o.get("topics") or [])]
        f = _check(self.gold, self.spec, bad)
        self.assertIn("alpha", f["missing_required_topics"])
        self.assertFalse(f["valid"])

    # 6
    def test_record_budget_overflow_is_caught(self):
        f = _check(self.gold, self.spec, self.honest, used_bytes=100,
                   budget={"byte_budget": 20000, "record_budget": 1})
        self.assertTrue(f["budget_record_overflow"])
        self.assertFalse(f["valid"])

    def test_byte_budget_overflow_is_caught(self):
        f = _check(self.gold, self.spec, self.honest, used_bytes=999999,
                   budget={"byte_budget": 10, "record_budget": 32})
        self.assertTrue(f["budget_byte_overflow"])
        self.assertFalse(f["valid"])


class ProvenanceRatioTest(unittest.TestCase):
    """The old field was `1.0 if carried else 0.0` — a boolean wearing a ratio's
    name. These pin zero / partial / full."""

    def test_zero(self):
        o = {"observation_id": "obs-x", "provenance": []}
        self.assertEqual(vl.presented_provenance_ratio([o]), 0.0)

    def test_partial(self):
        good = {"observation_id": "obs-a",
                "provenance": [{"source_id": "s", "digest": "sha256:" + "a" * 64}]}
        bad = {"observation_id": "obs-b", "provenance": [{"source_id": "s"}]}
        self.assertEqual(vl.presented_provenance_ratio([good, bad]), 0.5)

    def test_full(self):
        good = {"observation_id": "obs-a",
                "provenance": [{"source_id": "s", "source_pointer": "p"}]}
        self.assertEqual(vl.presented_provenance_ratio([good]), 1.0)

    def test_empty_projection_is_undefined_not_one(self):
        self.assertIsNone(vl.presented_provenance_ratio([]))


if __name__ == "__main__":
    unittest.main()
