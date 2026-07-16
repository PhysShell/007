"""Unit tests for gradle_canonicalizer.py -- the strict, case-specific
canonicalizer authorized (2026-07-16) for repo-moshi only, in response to
real CI evidence (run 29474204715, AFTER the deterministic scheduling
profile made every task-execution line byte-identical and same-order)
showing capture-a/capture-b raw stdout differ in exactly one line -- the
Gradle build-completion banner's wall-clock duration.
"""
import hashlib
import sys
import unittest
from pathlib import Path

TOOLS = Path(__file__).resolve().parents[1] / "tools"
sys.path.insert(0, str(TOOLS))
import gradle_canonicalizer as gc  # noqa: E402


def _line(text: str) -> bytes:
    return (text + "\n").encode("utf-8")


class TestGradleBuildDurationRule(unittest.TestCase):
    def test_replaces_only_the_duration(self):
        raw = _line("BUILD SUCCESSFUL in 1m 50s")
        out, report = gc.canonicalize_stream(raw)
        self.assertIn(b"<ELAPSED>", out)
        self.assertNotIn(b"1m 50s", out)
        self.assertEqual(report["rule_match_counts"]["gradle_build_duration"], 1)
        self.assertEqual(report["replacement_count"], 1)

    def test_duration_50s_and_11s_canonicalize_identically(self):
        capture_a = _line("BUILD SUCCESSFUL in 1m 50s") + _line("42 actionable tasks: 42 executed")
        capture_b = _line("BUILD SUCCESSFUL in 1m 11s") + _line("42 actionable tasks: 42 executed")
        self.assertNotEqual(capture_a, capture_b)
        canon_a, report_a = gc.canonicalize_stream(capture_a)
        canon_b, report_b = gc.canonicalize_stream(capture_b)
        self.assertEqual(canon_a, canon_b)
        self.assertEqual(report_a["replacement_count"], 1)
        self.assertEqual(report_b["replacement_count"], 1)

    def test_short_and_long_duration_formats_both_match(self):
        for duration in ("3s", "3m 12s", "1h 3m 12s"):
            with self.subTest(duration=duration):
                out, report = gc.canonicalize_stream(_line(f"BUILD SUCCESSFUL in {duration}"))
                self.assertIn(b"<ELAPSED>", out)
                self.assertEqual(report["replacement_count"], 1)

    def test_changed_task_count_does_not_canonicalize_identically(self):
        capture_a = _line("BUILD SUCCESSFUL in 1m 50s") + _line("42 actionable tasks: 42 executed")
        capture_b = _line("BUILD SUCCESSFUL in 1m 11s") + _line("41 actionable tasks: 41 executed")
        canon_a, _ = gc.canonicalize_stream(capture_a)
        canon_b, _ = gc.canonicalize_stream(capture_b)
        self.assertNotEqual(canon_a, canon_b)


class TestStructuralPreservation(unittest.TestCase):
    def _real_shaped_stream(self, *, duration="1m 50s") -> bytes:
        lines = [
            "> Task :moshi:compileJava",
            "> Task :moshi:test",
            "",
            f"BUILD SUCCESSFUL in {duration}",
            "42 actionable tasks: 42 executed",
        ]
        return ("\n".join(lines) + "\n").encode("utf-8")

    def test_line_count_preserved(self):
        raw = self._real_shaped_stream()
        out, report = gc.canonicalize_stream(raw)
        self.assertEqual(len(raw.splitlines()), len(out.splitlines()))
        self.assertEqual(report["line_count_in"], report["line_count_out"])

    def test_trailing_newline_preserved_when_present(self):
        raw = self._real_shaped_stream()
        self.assertTrue(raw.endswith(b"\n"))
        out, report = gc.canonicalize_stream(raw)
        self.assertTrue(out.endswith(b"\n"))
        self.assertTrue(report["trailing_newline_preserved"])

    def test_trailing_newline_preserved_when_absent(self):
        raw = self._real_shaped_stream().rstrip(b"\n")
        self.assertFalse(raw.endswith(b"\n"))
        out, report = gc.canonicalize_stream(raw)
        self.assertFalse(out.endswith(b"\n"))
        self.assertTrue(report["trailing_newline_preserved"])

    def test_untouched_lines_are_byte_identical(self):
        raw = self._real_shaped_stream()
        out, _ = gc.canonicalize_stream(raw)
        raw_lines = raw.decode().splitlines()
        out_lines = out.decode().splitlines()
        for i, line in enumerate(raw_lines):
            if "BUILD SUCCESSFUL" not in line:
                self.assertEqual(raw_lines[i], out_lines[i])

    def test_no_line_removed_including_blank_lines(self):
        raw = self._real_shaped_stream()
        out, _ = gc.canonicalize_stream(raw)
        self.assertIn("", out.decode().splitlines())


class TestIdempotence(unittest.TestCase):
    def test_applying_twice_changes_zero_bytes(self):
        raw = _line("BUILD SUCCESSFUL in 1m 50s")
        once, _ = gc.canonicalize_stream(raw)
        twice, report_twice = gc.canonicalize_stream(once)
        self.assertEqual(once, twice)
        self.assertEqual(report_twice["replacement_count"], 0)


class TestStrictUtf8(unittest.TestCase):
    def test_invalid_utf8_raises(self):
        raw = b"\xff\xfe not valid utf-8 \x00\x01"
        with self.assertRaises(gc.CanonicalizerError):
            gc.canonicalize_stream(raw)


class TestNonMatchingSimilarTextUnchanged(unittest.TestCase):
    def test_line_without_trigger_substring_passes_through_unchanged(self):
        raw = b"Some unrelated line mentioning BUILD without the completion grammar\n"
        out, report = gc.canonicalize_stream(raw)
        self.assertEqual(raw, out)
        self.assertEqual(report["replacement_count"], 0)


class TestUnexpectedGrammarFailsLoudly(unittest.TestCase):
    def test_malformed_duration_grammar_raises(self):
        raw = _line("BUILD SUCCESSFUL in a very long time")
        with self.assertRaises(gc.CanonicalizerError):
            gc.canonicalize_stream(raw)

    def test_build_failed_is_not_matched_or_touched(self):
        # This rule is scoped to the observed BUILD SUCCESSFUL grammar only
        # -- a genuine BUILD FAILED must never be silently canonicalized.
        raw = _line("BUILD FAILED in 1m 50s")
        out, report = gc.canonicalize_stream(raw)
        self.assertEqual(raw, out)
        self.assertEqual(report["replacement_count"], 0)


class TestReplacementRecordShape(unittest.TestCase):
    def test_replacement_records_rule_name_line_number_and_hashes(self):
        raw = _line("BUILD SUCCESSFUL in 1m 50s")
        out, report = gc.canonicalize_stream(raw)
        self.assertEqual(len(report["replacements"]), 1)
        entry = report["replacements"][0]
        self.assertEqual(entry["rule_name"], "gradle_build_duration")
        self.assertEqual(entry["line_number"], 1)
        before_line = raw.decode().splitlines()[0]
        after_line = out.decode().splitlines()[0]
        self.assertEqual(entry["before_line_sha256"], hashlib.sha256(before_line.encode()).hexdigest())
        self.assertEqual(entry["after_line_sha256"], hashlib.sha256(after_line.encode()).hexdigest())


class TestPolicyIntegrity(unittest.TestCase):
    def test_policy_file_verifies_against_code(self):
        policy_path = TOOLS.parent / "gradle-capture-canonicalization-policy.json"
        policy = gc.load_and_verify_policy(policy_path)
        self.assertEqual(policy["applicable_case_ids"], ["repo-moshi"])
        self.assertEqual(policy["canonicalizer_module"], "gradle_canonicalizer.py")


if __name__ == "__main__":
    unittest.main()
