"""Projection + schema tests — use only committed fixtures (no private RAW)."""
import json
import os
import unittest

import yaml

from o7b1 import projector as pj
from o7b1.schema import SchemaError, validate_gold_state

HERE = os.path.dirname(__file__)
FDIR = os.path.normpath(os.path.join(HERE, "..", "..", "fixtures", "case-0001"))


def _load_gold():
    with open(os.path.join(FDIR, "gold-state-v0.json"), encoding="utf-8") as fh:
        return json.load(fh)


class SchemaTest(unittest.TestCase):
    def test_gold_state_is_valid(self):
        validate_gold_state(_load_gold())

    def test_agent_claim_current_rejected(self):
        gold = _load_gold()
        # flip a superseded agent_claim to current -> must fail closed
        for o in gold["observations"]:
            if o["authority"] == "agent_claim":
                o["status"] = "current"
                o.pop("superseded_by", None)
                break
        with self.assertRaises(SchemaError):
            validate_gold_state(gold)

    def test_superseded_requires_superseded_by(self):
        gold = _load_gold()
        for o in gold["observations"]:
            if o["status"] == "superseded":
                o.pop("superseded_by", None)
                break
        with self.assertRaises(SchemaError):
            validate_gold_state(gold)


class ProjectionTest(unittest.TestCase):
    def setUp(self):
        self.gold = _load_gold()
        with open(os.path.join(FDIR, "task-v0.yaml"), encoding="utf-8") as fh:
            self.task = yaml.safe_load(fh)

    def _select(self):
        return pj.select(self.gold, byte_budget=20000, record_budget=32)

    def test_no_agent_claim_or_superseded_selected(self):
        sel = self._select()
        for o in sel["selected"]:
            self.assertNotEqual(o["authority"], "agent_claim")
            self.assertIn(o["status"], ("current", "pending"))

    def test_superseded_claims_omitted_with_reason(self):
        sel = self._select()
        omitted_ids = {o["observation_id"] for o in sel["omitted"]}
        self.assertIn("obs-nc-claim-bd-separate", omitted_ids)
        self.assertIn("obs-nc-claim-no-session-e", omitted_ids)
        for om in sel["omitted"]:
            self.assertTrue(om["reason"])

    def test_context_build_is_deterministic(self):
        from o7b1.canonical import canonical_file_bytes
        sel = self._select()
        j1 = canonical_file_bytes(pj.build_context_json(self.gold, self.task, sel))
        j2 = canonical_file_bytes(pj.build_context_json(self.gold, self.task, self._select()))
        self.assertEqual(j1, j2)
        m1 = pj.build_context_md(self.gold, self.task, sel)
        m2 = pj.build_context_md(self.gold, self.task, self._select())
        self.assertEqual(m1, m2)

    def test_context_md_has_no_superseded_text(self):
        sel = self._select()
        md = pj.build_context_md(self.gold, self.task, sel)
        # the negative control's superseded beliefs must not be rendered as state
        self.assertNotIn("obs-nc-claim-bd-separate", md.split("## Omitted")[0])

    def test_budget_omits_when_too_small(self):
        sel = pj.select(self.gold, byte_budget=500, record_budget=32)
        self.assertTrue(any("budget exceeded" in o["reason"] for o in sel["omitted"]))


if __name__ == "__main__":
    unittest.main()
