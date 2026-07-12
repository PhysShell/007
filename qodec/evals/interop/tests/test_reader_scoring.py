"""Level-2 scoring is deterministic and rule-based (no LLM judge).

Pure — no endpoint. Exercises count/files/symbols/facts scoring, invented-
identifier detection, alias leakage, JSON extraction, and the three arms.
"""

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from bench import reader_tasks as rt  # noqa: E402

SRC = ("20 matches in 10F:\n"
       "[file] src/Legacy.Core/Services/SessionService.cs (2):\n"
       "    33: public async Task OpenAsync(Credentials credentials)\n")


class Scoring(unittest.TestCase):
    def _score(self, q, ans, glyphs=frozenset()):
        return rt.score_question(q, ans, source_text=SRC, glyphs=set(glyphs))

    def test_count_exact(self):
        q = {"id": "c", "type": "count", "gold": {"answer": "20"}}
        self.assertTrue(self._score(q, {"answer": "20"}).correct)
        self.assertTrue(self._score(q, {"answer": 20}).correct)
        self.assertFalse(self._score(q, {"answer": "21"}).correct)

    def test_files_superset(self):
        q = {"id": "f", "type": "files", "gold": {"files": ["SessionService.cs"]}}
        self.assertTrue(self._score(q, {"files": ["SessionService.cs", "Other.cs"]}).correct)
        self.assertFalse(self._score(q, {"files": ["Nope.cs"]}).correct)

    def test_symbols_and_invented_identifier(self):
        q = {"id": "s", "type": "symbols", "gold": {"symbols": ["OpenAsync"]}}
        good = self._score(q, {"symbols": ["OpenAsync"]})
        self.assertTrue(good.correct)
        self.assertEqual(good.invalid_identifiers, [])
        # A symbol not present in the source is flagged as invented.
        bad = self._score(q, {"symbols": ["OpenAsync", "TotallyMadeUp"]})
        self.assertIn("TotallyMadeUp", bad.invalid_identifiers)

    def test_facts_substring(self):
        q = {"id": "x", "type": "facts", "gold": {"facts": ["OpenAsync"]}}
        self.assertTrue(self._score(q, {"facts": ["method OpenAsync"]}).correct)
        self.assertFalse(self._score(q, {"facts": ["nothing"]}).correct)

    def test_alias_leakage(self):
        q = {"id": "s", "type": "symbols", "gold": {"symbols": ["OpenAsync"]}}
        sc = self._score(q, {"symbols": ["OpenAsync"], "answer": "码路"}, glyphs={"码", "路"})
        self.assertEqual(sc.alias_leak, 2)


class Parsing(unittest.TestCase):
    def test_extract_json_amid_chatter(self):
        obj = rt.parse_answer('Sure! {"answer": "3", "files": []} hope that helps')
        self.assertEqual(obj["answer"], "3")

    def test_bad_json_is_empty(self):
        self.assertEqual(rt.parse_answer("no json here"), {})

    def test_legend_glyphs(self):
        artifact = "%q1 mine n=2\n码=src/foo\n路=bar/baz\n%q1 body\n码 路\n"
        self.assertEqual(rt.legend_glyphs(artifact), {"码", "路"})


class Arms(unittest.TestCase):
    def test_three_arms_differ_by_brief(self):
        raw = rt.build_messages("raw", "PAYLOAD", "BRIEF", "Q?")
        rb = rt.build_messages("raw+brief", "PAYLOAD", "BRIEF", "Q?")
        eb = rt.build_messages("encoded+brief", "ART", "BRIEF", "Q?")
        self.assertNotIn("BRIEF", raw[1]["content"])
        self.assertIn("BRIEF", rb[1]["content"])
        self.assertIn("BRIEF", eb[1]["content"])
        self.assertIn("PAYLOAD", rb[1]["content"])


if __name__ == "__main__":
    unittest.main()
