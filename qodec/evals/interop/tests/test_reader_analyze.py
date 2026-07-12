"""Level-2 analysis: decision units are UNIQUE questions, not (question, repeat).

Pure — no endpoint. The load-bearing property: repeats (which only ever exist for
flagged questions) must not inflate n, eligibility, or the gates. A locator
question run 100 times is one eligible locator unit.
"""

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import score_reader as sr  # noqa: E402


def rec(case, q, cat, arm, *, repeat=0, correct=True, malformed=False,
        inv=None, leaks=None, local=100, server=130):
    return {
        "case": case, "question": q, "category": cat, "arm": arm, "repeat": repeat,
        "correct": correct, "format_compliant": not malformed, "malformed": malformed,
        "invalid_identifiers": inv or [], "alias_leaks": leaks or [],
        "local_content_tokens": local, "server_prompt_tokens": server,
        "completion_tokens": 10, "total_ms": 100.0,
    }


def three_arms(case, q, cat, *, raw=True, rb=True, eb=True, **kw):
    return [rec(case, q, cat, "raw", correct=raw, **kw),
            rec(case, q, cat, "raw+brief", correct=rb, **kw),
            rec(case, q, cat, "encoded+brief", correct=eb, **kw)]


class UniqueQuestionGate(unittest.TestCase):
    def test_one_locator_question_x100_is_one_eligible_unit(self):
        recs = []
        for r in range(100):
            recs += three_arms("c", "locq", "locator", repeat=r)
        a = sr.analyze({}, recs)
        self.assertEqual(a["unique_questions"], 1)
        self.assertEqual(a["groups"]["locator"]["eligible"], 1)  # NOT 100
        self.assertEqual(a["groups"]["locator"]["n"], 1)
        # 1 unique question can never satisfy locator>=4 / overall>=10.
        self.assertTrue(a["decision"]["inconclusive"])

    def test_repeats_do_not_inflate_eligible_or_n(self):
        recs = []
        for i in range(12):
            recs += three_arms("c", f"q{i}", "locator")
        # Repeat one question 100 more times — must not change unique counts.
        for r in range(1, 100):
            recs += three_arms("c", "q0", "locator", repeat=r)
        a = sr.analyze({}, recs)
        self.assertEqual(a["unique_questions"], 12)
        self.assertEqual(a["groups"]["all"]["n"], 12)
        self.assertEqual(a["groups"]["all"]["eligible"], 12)
        self.assertEqual(a["groups"]["locator"]["eligible"], 12)


class Verdicts(unittest.TestCase):
    def _many(self, **kw):
        recs = []
        for i in range(12):
            recs += three_arms("c", f"q{i}", "locator", **kw)
        return recs

    def test_blind_pass_when_clean(self):
        a = sr.analyze({}, self._many())
        self.assertFalse(a["decision"]["inconclusive"])
        self.assertEqual(a["decision"]["verdict"], "BLIND QODEC PASSES")

    def test_inconclusive_on_weak_raw(self):
        a = sr.analyze({}, self._many(raw=False))
        self.assertTrue(a["decision"]["inconclusive"])
        self.assertIn("raw competence", a["decision"]["verdict"])

    def test_protected_spans_on_stable_locator_loss(self):
        # 12 locator questions correct in raw+brief; encoded wrong on 3 → stable
        # locator loss. Add facts/counts that qodec preserves.
        recs = self._many()
        for i in range(3):
            for r in recs:
                if r["question"] == f"q{i}" and r["arm"] == "encoded+brief":
                    r["correct"] = False
        for i in range(10):
            recs += three_arms("c", f"f{i}", "count")  # facts/counts all preserved
        a = sr.analyze({}, recs)
        self.assertGreater(a["groups"]["locator"]["stable_codec_losses"], 0)
        self.assertIn("PROTECTED SPANS NEXT", a["decision"]["verdict"])


class Stability(unittest.TestCase):
    def test_unstable_question_excluded_from_stable_loss(self):
        # A codec loss whose repeats DISAGREE is not a stable loss.
        recs = []
        for i in range(12):
            recs += three_arms("c", f"q{i}", "locator")
        # q0 encoded: repeat0 wrong, repeat1 correct → unstable, not a stable loss.
        recs += [rec("c", "q0", "locator", "encoded+brief", repeat=0, correct=False)]
        recs = [r for r in recs if not (r["question"] == "q0" and r["arm"] == "encoded+brief" and r["repeat"] == 0)] + \
               [rec("c", "q0", "locator", "encoded+brief", repeat=0, correct=False),
                rec("c", "q0", "locator", "encoded+brief", repeat=1, correct=True)]
        a = sr.analyze({}, recs)
        self.assertGreaterEqual(a["unstable_questions"], 1)
        self.assertEqual(a["groups"]["locator"]["stable_codec_losses"], 0)

    def test_signature_flip_beyond_correct_is_unstable(self):
        # correct is stable across repeats, but alias_leaks appears in one → the
        # extended signature (correct, format, leaks, invalid) marks it unstable.
        recs = []
        for i in range(12):
            recs += three_arms("c", f"q{i}", "locator")
        recs += [rec("c", "q0", "locator", "encoded+brief", repeat=1, correct=True, leaks=["码"])]
        a = sr.analyze({}, recs)
        self.assertGreaterEqual(a["unstable_questions"], 1)

    def _repeat_eb(self, r0, r1):
        # 12 clean questions for eligibility, with q0's encoded+brief replaced by
        # two explicit repeats so the signature comparison is exercised directly.
        recs = []
        for i in range(12):
            recs += three_arms("c", f"q{i}", "locator")
        recs = [r for r in recs if not (r["question"] == "q0" and r["arm"] == "encoded+brief")]
        return recs + [r0, r1]

    def test_alias_change_same_count_is_unstable(self):
        # One leaked alias becomes a *different* alias — same count, so a bool
        # flag would call it stable; the normalized set makes it unstable.
        recs = self._repeat_eb(
            rec("c", "q0", "locator", "encoded+brief", repeat=0, leaks=["码A"]),
            rec("c", "q0", "locator", "encoded+brief", repeat=1, leaks=["码B"]))
        self.assertGreaterEqual(sr.analyze({}, recs)["unstable_questions"], 1)

    def test_invalid_identifier_change_is_unstable(self):
        # invalid Foo → invalid Bar across repeats.
        recs = self._repeat_eb(
            rec("c", "q0", "locator", "encoded+brief", repeat=0, inv=["Foo"]),
            rec("c", "q0", "locator", "encoded+brief", repeat=1, inv=["Bar"]))
        self.assertGreaterEqual(sr.analyze({}, recs)["unstable_questions"], 1)

    def test_reordered_same_set_is_stable(self):
        # Same set of invalid ids in a different order → NOT unstable.
        recs = self._repeat_eb(
            rec("c", "q0", "locator", "encoded+brief", repeat=0, inv=["a", "b"]),
            rec("c", "q0", "locator", "encoded+brief", repeat=1, inv=["b", "a"]))
        self.assertEqual(sr.analyze({}, recs)["unstable_questions"], 0)


class Parity(unittest.TestCase):
    def test_constant_overhead_is_ok(self):
        recs = []
        for i in range(12):
            recs += three_arms("c", f"q{i}", "locator", local=100, server=130)  # overhead 30 all arms
        a = sr.analyze({}, recs)
        self.assertFalse(a["parity"]["mismatch"])

    def test_divergent_overhead_flags_mismatch(self):
        recs = []
        for i in range(12):
            recs.append(rec("c", f"q{i}", "locator", "raw", local=100, server=130))       # oh 30
            recs.append(rec("c", f"q{i}", "locator", "raw+brief", local=100, server=130))  # oh 30
            recs.append(rec("c", f"q{i}", "locator", "encoded+brief", local=100, server=200))  # oh 100
        a = sr.analyze({}, recs)
        self.assertTrue(a["parity"]["mismatch"])


if __name__ == "__main__":
    unittest.main()
