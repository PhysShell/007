"""Level-2 scoring: deterministic, match-mode aware, fail-safe integrity checks.

Pure — no endpoint. Covers count/locator/call_path/actionability, the match
modes (exact-set rejects extras; one-of/contains-all allow them; ordered-path is
a subsequence), full-path matching (no basename), full-string alias leakage over
structured values only, and malformed-JSON handling.
"""

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from bench import reader_tasks as rt  # noqa: E402

SRC = (
    "Found 56 symbols across 3 files.\n"
    "`ValueParser` (clap_builder/src/builder/value_parser.rs:63) — 8 callers\n"
    "**`clap_builder/src/derive.rs`** pub trait Parser\n"
    "    let res = <Self as CommandFactory>::command().get_matches();\n"
    "    <Self as FromArgMatches>::from_arg_matches_mut(&mut matches)\n"
)


def q(**kw):
    base = {"id": "x", "category": "locator", "field": "files", "match": "one-of"}
    base.update(kw)
    base["gold"] = kw.get("gold", [])
    return base


class MatchModes(unittest.TestCase):
    def s(self, question, ans, aliases=frozenset()):
        return rt.score_question(question, ans, source_text=SRC, aliases=set(aliases))

    def test_count_exact(self):
        qq = q(category="count", field="answer", match="exact", gold=["56"])
        self.assertTrue(self.s(qq, {"answer": "56"}).correct)
        self.assertFalse(self.s(qq, {"answer": "57"}).correct)

    def test_one_of_allows_extras(self):
        qq = q(field="symbols", match="one-of", gold=["ValueParser"])
        self.assertTrue(self.s(qq, {"symbols": ["ValueParser", "Parser"]}).correct)

    def test_exact_set_rejects_extras(self):
        qq = q(field="files", match="exact-set", gold=["clap_builder/src/derive.rs"])
        self.assertTrue(self.s(qq, {"files": ["clap_builder/src/derive.rs"]}).correct)
        # gold + an extra identifier is NOT an exact answer.
        self.assertFalse(self.s(qq, {"files": ["clap_builder/src/derive.rs", "clap_builder/src/lib.rs"]}).correct)

    def test_exact_path_no_basename(self):
        qq = q(field="files", match="exact-set", gold=["clap_builder/src/derive.rs"])
        # basename alone must not satisfy an exact full-path question.
        self.assertFalse(self.s(qq, {"files": ["derive.rs"]}).correct)

    def test_ordered_path_subsequence(self):
        qq = q(category="call_path", field="call_path", match="ordered-path",
               gold=["CommandFactory::command", "get_matches", "FromArgMatches::from_arg_matches_mut"])
        good = {"call_path": ["CommandFactory::command", "get_matches", "FromArgMatches::from_arg_matches_mut"]}
        self.assertTrue(self.s(qq, good).correct)
        wrong_order = {"call_path": ["get_matches", "CommandFactory::command"]}
        self.assertFalse(self.s(qq, wrong_order).correct)

    def test_actionability_contains_all(self):
        qq = q(category="actionability", field="facts", match="contains-all",
               gold=["8 callers", "no covering tests"])
        self.assertTrue(self.s(qq, {"facts": ["ValueParser has 8 callers", "no covering tests found"]}).correct)
        self.assertFalse(self.s(qq, {"facts": ["8 callers"]}).correct)


class Integrity(unittest.TestCase):
    def s(self, question, ans, aliases=frozenset()):
        return rt.score_question(question, ans, source_text=SRC, aliases=set(aliases))

    def test_invalid_identifier_full_string(self):
        qq = q(field="symbols", match="one-of", gold=["ValueParser"])
        sc = self.s(qq, {"symbols": ["ValueParser", "MadeUpType"]})
        self.assertIn("MadeUpType", sc.invalid_identifiers)
        self.assertNotIn("ValueParser", sc.invalid_identifiers)

    def test_call_path_method_segment_not_invalid(self):
        qq = q(category="call_path", field="call_path", match="ordered-path",
               gold=["CommandFactory::command"])
        # 'CommandFactory::command' appears method-first in source; its final
        # segment `command` is present, so it must not be flagged invalid.
        sc = self.s(qq, {"call_path": ["CommandFactory::command"]})
        self.assertEqual(sc.invalid_identifiers, [])

    def test_alias_leak_full_strings_only(self):
        artifact = "%q1 mine n=2\n码=clap_builder/src\nΩ=derive\n%q1 body\n码/foo Ω\n"
        aliases = rt.used_aliases(artifact)
        self.assertEqual(aliases, {"码", "Ω"})
        qq = q(field="files", match="one-of", gold=["clap_builder/src/derive.rs"])
        sc = self.s(qq, {"files": ["码/derive.rs"], "answer": "Ω"}, aliases=aliases)
        self.assertEqual(sorted(sc.alias_leaks), ["Ω", "码"])

    def test_unused_alias_not_counted(self):
        # An alias declared but not in the body is not a leakage candidate.
        artifact = "%q1 mine n=1\n码=clap_builder/src\n%q1 body\nplain text no alias\n"
        self.assertEqual(rt.used_aliases(artifact), set())


class Parsing(unittest.TestCase):
    def test_malformed_json_is_none(self):
        self.assertIsNone(rt.parse_answer("no json here"))

    def test_extract_amid_chatter(self):
        self.assertEqual(rt.parse_answer('ok {"answer":"3"} done')["answer"], "3")


if __name__ == "__main__":
    unittest.main()
