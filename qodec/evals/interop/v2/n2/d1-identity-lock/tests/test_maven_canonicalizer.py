"""Unit tests for maven_canonicalizer.py -- the strict, case-specific
canonicalizer authorized (2026-07-16) for repo-docker-java-parser only, in
response to real CI evidence (run 29436883023) showing capture-a/capture-b
raw stdout differ in exactly five known wall-clock/timestamp lines.
"""
import sys
import unittest
from pathlib import Path

TOOLS = Path(__file__).resolve().parents[1] / "tools"
sys.path.insert(0, str(TOOLS))
import maven_canonicalizer as mc  # noqa: E402

ESC = "\x1b"


def _line(text: str) -> bytes:
    return (text + "\n").encode("utf-8")


class TestBuildnumberTimestampRule(unittest.TestCase):
    def test_replaces_only_the_integer(self):
        raw = _line(f"[{ESC}[1;34mINFO{ESC}[m] Storing buildNumber: null at timestamp: 1784136905453")
        out, report = mc.canonicalize_stream(raw)
        self.assertIn(b"<TIMESTAMP>", out)
        self.assertNotIn(b"1784136905453", out)
        self.assertEqual(report["rule_match_counts"]["buildnumber_timestamp"], 1)
        self.assertEqual(report["replacement_count"], 1)


class TestScalaCompileDurationRule(unittest.TestCase):
    def test_multiple_compile_lines_all_replaced_and_counted(self):
        raw = (
            _line(f"[{ESC}[1;34mINFO{ESC}[m] compile in 11.4 s")
            + _line(f"[{ESC}[1;34mINFO{ESC}[m] compile in 3.3 s")
        )
        out, report = mc.canonicalize_stream(raw)
        self.assertEqual(out.count(b"<ELAPSED>"), 2)
        self.assertNotIn(b"11.4", out)
        self.assertNotIn(b"3.3", out)
        self.assertEqual(report["rule_match_counts"]["scala_compile_duration"], 2)


class TestSurefireTimeElapsedRule(unittest.TestCase):
    def _line(self, elapsed: str) -> bytes:
        return _line(
            f"[{ESC}[1;34mINFO{ESC}[m] {ESC}[1;32mTests run: {ESC}[0;1;32m3{ESC}[m, Failures: 0, "
            f"Errors: 0, Skipped: 0, Time elapsed: {elapsed} s - in com.github.thstock.djp."
            f"{ESC}[1mScalaParserTest{ESC}[m"
        )

    def test_preserves_counts_and_suite_name_replaces_only_elapsed(self):
        raw = self._line("0.14")
        out, report = mc.canonicalize_stream(raw)
        text = out.decode()
        self.assertIn("Tests run: ", text)
        self.assertIn("3", text)
        self.assertIn("Failures: 0, Errors: 0, Skipped: 0", text)
        self.assertIn("ScalaParserTest", text)
        self.assertIn("<ELAPSED>", text)
        self.assertNotIn("0.14", text)
        self.assertEqual(report["rule_match_counts"]["surefire_time_elapsed"], 1)


class TestMavenTotalTimeRule(unittest.TestCase):
    def test_replaces_only_duration(self):
        raw = _line(f"[{ESC}[1;34mINFO{ESC}[m] Total time:  18.865 s")
        out, report = mc.canonicalize_stream(raw)
        self.assertIn(b"<ELAPSED>", out)
        self.assertNotIn(b"18.865", out)
        self.assertEqual(report["rule_match_counts"]["maven_total_time"], 1)


class TestMavenFinishedAtRule(unittest.TestCase):
    def test_replaces_only_timestamp(self):
        raw = _line(f"[{ESC}[1;34mINFO{ESC}[m] Finished at: 2026-07-15T17:35:22Z")
        out, report = mc.canonicalize_stream(raw)
        self.assertIn(b"<TIMESTAMP>", out)
        self.assertNotIn(b"2026-07-15T17:35:22Z", out)
        self.assertEqual(report["rule_match_counts"]["maven_finished_at"], 1)


class TestStructuralPreservation(unittest.TestCase):
    def _real_shaped_stream(self, *, buildnumber_ts="1784136905453", total_s="18.865",
                             finished_at="2026-07-15T17:35:22Z") -> bytes:
        lines = [
            f"[{ESC}[1;34mINFO{ESC}[m] Scanning for projects...",
            "",
            f"[{ESC}[1;34mINFO{ESC}[m] BUILD SUCCESS",
            f"[{ESC}[1;34mINFO{ESC}[m] Storing buildNumber: null at timestamp: {buildnumber_ts}",
            f"[{ESC}[1;34mINFO{ESC}[m] compile in 11.4 s",
            f"[{ESC}[1;34mINFO{ESC}[m] Total time:  {total_s} s",
            f"[{ESC}[1;34mINFO{ESC}[m] Finished at: {finished_at}",
        ]
        return ("\n".join(lines) + "\n").encode("utf-8")

    def test_line_count_preserved(self):
        raw = self._real_shaped_stream()
        out, report = mc.canonicalize_stream(raw)
        self.assertEqual(len(raw.splitlines()), len(out.splitlines()))
        self.assertEqual(report["line_count_in"], report["line_count_out"])

    def test_trailing_newline_preserved_when_present(self):
        raw = self._real_shaped_stream()
        self.assertTrue(raw.endswith(b"\n"))
        out, report = mc.canonicalize_stream(raw)
        self.assertTrue(out.endswith(b"\n"))
        self.assertTrue(report["trailing_newline_preserved"])

    def test_trailing_newline_preserved_when_absent(self):
        raw = self._real_shaped_stream().rstrip(b"\n")
        self.assertFalse(raw.endswith(b"\n"))
        out, report = mc.canonicalize_stream(raw)
        self.assertFalse(out.endswith(b"\n"))
        self.assertTrue(report["trailing_newline_preserved"])

    def test_line_order_preserved(self):
        raw = self._real_shaped_stream()
        out, _ = mc.canonicalize_stream(raw)
        raw_lines = raw.decode().splitlines()
        out_lines = out.decode().splitlines()
        # Every non-rule-matched line is byte-identical, in the same order.
        untouched_indices = [i for i, line in enumerate(raw_lines)
                              if "Storing buildNumber" not in line and "compile in" not in line
                              and "Total time:" not in line and "Finished at:" not in line]
        for i in untouched_indices:
            self.assertEqual(raw_lines[i], out_lines[i])

    def test_no_line_removed_including_blank_lines(self):
        raw = self._real_shaped_stream()
        out, _ = mc.canonicalize_stream(raw)
        self.assertIn("", out.decode().splitlines())


class TestIdempotence(unittest.TestCase):
    def test_applying_twice_changes_zero_bytes(self):
        raw = (
            _line(f"[{ESC}[1;34mINFO{ESC}[m] Storing buildNumber: null at timestamp: 1784136905453")
            + _line(f"[{ESC}[1;34mINFO{ESC}[m] compile in 11.4 s")
            + _line(f"[{ESC}[1;34mINFO{ESC}[m] Total time:  18.865 s")
            + _line(f"[{ESC}[1;34mINFO{ESC}[m] Finished at: 2026-07-15T17:35:22Z")
        )
        once, _ = mc.canonicalize_stream(raw)
        twice, report_twice = mc.canonicalize_stream(once)
        self.assertEqual(once, twice)
        self.assertEqual(report_twice["replacement_count"], 0)


class TestStrictUtf8(unittest.TestCase):
    def test_invalid_utf8_raises(self):
        raw = b"\xff\xfe not valid utf-8 \x00\x01"
        with self.assertRaises(mc.CanonicalizerError):
            mc.canonicalize_stream(raw)


class TestNonMatchingSimilarTextUnchanged(unittest.TestCase):
    def test_similar_but_non_triggering_text_passes_through_unchanged(self):
        # "recompiled in" does not contain the "compile in " trigger
        # substring (note the "d" before " in"), and "Compile input" differs
        # in case and wording -- neither should be touched.
        raw = _line(f"[{ESC}[1;34mINFO{ESC}[m] recompiled in 5.0 s, Compile input validated")
        out, report = mc.canonicalize_stream(raw)
        self.assertEqual(raw, out)
        self.assertEqual(report["replacement_count"], 0)


class TestUnexpectedGrammarFailsLoudly(unittest.TestCase):
    def test_trigger_present_but_grammar_mismatch_raises(self):
        # Trigger substring "Storing buildNumber:" is present, but the value
        # is not the expected `\d+` (or already-canonicalized placeholder)
        # shape -- must raise, never silently pass through un-canonicalized.
        raw = _line(f"[{ESC}[1;34mINFO{ESC}[m] Storing buildNumber: abc123 at timestamp: not-a-number")
        with self.assertRaises(mc.CanonicalizerError):
            mc.canonicalize_stream(raw)

    def test_malformed_total_time_line_raises(self):
        raw = _line(f"[{ESC}[1;34mINFO{ESC}[m] Total time:  1 min 23 s")
        with self.assertRaises(mc.CanonicalizerError):
            mc.canonicalize_stream(raw)


class TestRealEvidenceShapedCaptureAAndBCanonicalizeIdentically(unittest.TestCase):
    """Reproduces the real bounded diff (run 29436883023) that justified
    this policy -- five differing lines, byte-identical after
    canonicalization."""

    def _capture(self, *, buildnumber_ts, compile_1, compile_2, elapsed, total_s, finished_at) -> bytes:
        lines = [
            f"[{ESC}[1;34mINFO{ESC}[m] Storing buildNumber: null at timestamp: {buildnumber_ts}",
            f"[{ESC}[1;34mINFO{ESC}[m] compile in {compile_1} s",
            f"[{ESC}[1;34mINFO{ESC}[m] compile in {compile_2} s",
            (
                f"[{ESC}[1;34mINFO{ESC}[m] {ESC}[1;32mTests run: {ESC}[0;1;32m3{ESC}[m, Failures: 0, "
                f"Errors: 0, Skipped: 0, Time elapsed: {elapsed} s - in com.github.thstock.djp."
                f"{ESC}[1mScalaParserTest{ESC}[m"
            ),
            f"[{ESC}[1;34mINFO{ESC}[m] Total time:  {total_s} s",
            f"[{ESC}[1;34mINFO{ESC}[m] Finished at: {finished_at}",
        ]
        return ("\n".join(lines) + "\n").encode("utf-8")

    def test_five_line_real_diff_fully_canonicalizes_away(self):
        capture_a = self._capture(
            buildnumber_ts="1784136905453", compile_1="11.4", compile_2="3.3", elapsed="0.14",
            total_s="18.865", finished_at="2026-07-15T17:35:22Z",
        )
        capture_b = self._capture(
            buildnumber_ts="1784136890043", compile_1="10.5", compile_2="3.1", elapsed="0.134",
            total_s="17.597", finished_at="2026-07-15T17:35:06Z",
        )
        self.assertNotEqual(capture_a, capture_b)
        canon_a, report_a = mc.canonicalize_stream(capture_a)
        canon_b, report_b = mc.canonicalize_stream(capture_b)
        self.assertEqual(canon_a, canon_b)
        # 5 rules, but scala_compile_duration matches twice (two modules) --
        # 6 total replacements, matching real evidence (run 29436883023).
        self.assertEqual(report_a["replacement_count"], 6)
        self.assertEqual(report_b["replacement_count"], 6)


class TestReplacementRecordShape(unittest.TestCase):
    def test_replacement_records_rule_name_line_number_and_hashes(self):
        import hashlib

        raw = _line(f"[{ESC}[1;34mINFO{ESC}[m] Finished at: 2026-07-15T17:35:22Z")
        out, report = mc.canonicalize_stream(raw)
        self.assertEqual(len(report["replacements"]), 1)
        entry = report["replacements"][0]
        self.assertEqual(entry["rule_name"], "maven_finished_at")
        self.assertEqual(entry["line_number"], 1)
        before_line = raw.decode().splitlines()[0]
        after_line = out.decode().splitlines()[0]
        self.assertEqual(entry["before_line_sha256"], hashlib.sha256(before_line.encode()).hexdigest())
        self.assertEqual(entry["after_line_sha256"], hashlib.sha256(after_line.encode()).hexdigest())


if __name__ == "__main__":
    unittest.main()
