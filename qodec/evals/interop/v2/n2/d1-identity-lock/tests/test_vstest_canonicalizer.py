"""Unit tests for vstest_canonicalizer.py -- the strict, case-specific
canonicalizer authorized (2026-07-16) for repo-kubeops-generator only, in
response to real CI evidence (run 29466573023, pair-verify artifact
8363205429) showing capture-a/capture-b raw stdout differ in exactly one
line -- VSTest's own completion banner's wall-clock "Duration: N s" field.
"""
import hashlib
import sys
import unittest
from pathlib import Path

TOOLS = Path(__file__).resolve().parents[1] / "tools"
sys.path.insert(0, str(TOOLS))
import vstest_canonicalizer as vc  # noqa: E402


def _banner(*, failed="0", passed="61", skipped="0", total="61", duration="2",
            tail="KubeOps.Generator.Test.dll (net10.0)") -> bytes:
    return (
        f"Passed!  - Failed:     {failed}, Passed:    {passed}, Skipped:     {skipped}, "
        f"Total:    {total}, Duration: {duration} s - {tail}\n"
    ).encode("utf-8")


class TestVstestDurationRule(unittest.TestCase):
    def test_replaces_only_the_duration(self):
        raw = _banner(duration="2")
        out, report = vc.canonicalize_stream(raw)
        self.assertIn(b"<ELAPSED>", out)
        self.assertNotIn(b"Duration: 2 s", out)
        self.assertIn(b"Failed:     0, Passed:    61, Skipped:     0, Total:    61", out)
        self.assertIn(b"KubeOps.Generator.Test.dll (net10.0)", out)
        self.assertEqual(report["rule_match_counts"]["vstest_duration"], 1)
        self.assertEqual(report["replacement_count"], 1)

    def test_duration_2s_and_1s_canonicalize_identically(self):
        # The exact real evidence from run 29466573023: identical 61/61 pass
        # results, differing only in wall-clock duration.
        capture_a = _banner(duration="2")
        capture_b = _banner(duration="1")
        self.assertNotEqual(capture_a, capture_b)
        canon_a, report_a = vc.canonicalize_stream(capture_a)
        canon_b, report_b = vc.canonicalize_stream(capture_b)
        self.assertEqual(canon_a, canon_b)
        self.assertEqual(report_a["replacement_count"], 1)
        self.assertEqual(report_b["replacement_count"], 1)

    def test_changed_pass_fail_total_count_does_not_canonicalize_identically(self):
        # A real regression (a genuinely different test result) must never
        # be masked -- only the duration field is ever touched.
        capture_a = _banner(passed="61", total="61", duration="2")
        capture_b = _banner(passed="60", total="61", duration="1")
        canon_a, _ = vc.canonicalize_stream(capture_a)
        canon_b, _ = vc.canonicalize_stream(capture_b)
        self.assertNotEqual(canon_a, canon_b)


class TestStructuralPreservation(unittest.TestCase):
    def _real_shaped_stream(self, *, duration="2") -> bytes:
        lines = [
            "Test run for /work/KubeOps.Generator.Test.dll (net10.0)",
            "",
            "Starting test execution, please wait...",
            "",
            _banner(duration=duration).decode().rstrip("\n"),
        ]
        return ("\n".join(lines) + "\n").encode("utf-8")

    def test_line_count_preserved(self):
        raw = self._real_shaped_stream()
        out, report = vc.canonicalize_stream(raw)
        self.assertEqual(len(raw.splitlines()), len(out.splitlines()))
        self.assertEqual(report["line_count_in"], report["line_count_out"])

    def test_trailing_newline_preserved_when_present(self):
        raw = self._real_shaped_stream()
        self.assertTrue(raw.endswith(b"\n"))
        out, report = vc.canonicalize_stream(raw)
        self.assertTrue(out.endswith(b"\n"))
        self.assertTrue(report["trailing_newline_preserved"])

    def test_trailing_newline_preserved_when_absent(self):
        raw = self._real_shaped_stream().rstrip(b"\n")
        self.assertFalse(raw.endswith(b"\n"))
        out, report = vc.canonicalize_stream(raw)
        self.assertFalse(out.endswith(b"\n"))
        self.assertTrue(report["trailing_newline_preserved"])

    def test_no_line_removed_including_blank_lines(self):
        raw = self._real_shaped_stream()
        out, _ = vc.canonicalize_stream(raw)
        self.assertIn("", out.decode().splitlines())

    def test_untouched_lines_are_byte_identical(self):
        raw = self._real_shaped_stream()
        out, _ = vc.canonicalize_stream(raw)
        raw_lines = raw.decode().splitlines()
        out_lines = out.decode().splitlines()
        for i, line in enumerate(raw_lines):
            if "Duration:" not in line:
                self.assertEqual(raw_lines[i], out_lines[i])


class TestIdempotence(unittest.TestCase):
    def test_applying_twice_changes_zero_bytes(self):
        raw = _banner(duration="2")
        once, _ = vc.canonicalize_stream(raw)
        twice, report_twice = vc.canonicalize_stream(once)
        self.assertEqual(once, twice)
        self.assertEqual(report_twice["replacement_count"], 0)


class TestStrictUtf8(unittest.TestCase):
    def test_invalid_utf8_raises(self):
        raw = b"\xff\xfe not valid utf-8 \x00\x01"
        with self.assertRaises(vc.CanonicalizerError):
            vc.canonicalize_stream(raw)


class TestNonMatchingSimilarTextUnchanged(unittest.TestCase):
    def test_line_without_trigger_substring_passes_through_unchanged(self):
        # "Duration without" has no "Duration: " (colon-space) trigger
        # substring, so it must never be touched.
        raw = b"Some unrelated line mentioning Duration without the banner grammar\n"
        out, report = vc.canonicalize_stream(raw)
        self.assertEqual(raw, out)
        self.assertEqual(report["replacement_count"], 0)


class TestUnexpectedGrammarFailsLoudly(unittest.TestCase):
    def test_malformed_duration_grammar_raises(self):
        # Trigger substring "Duration: " is present, but the banner's own
        # shape is malformed (missing the trailing " s - <suite>" tail) --
        # must raise, never silently pass through un-canonicalized.
        raw = b"Passed!  - Failed:     0, Passed:    61, Skipped:     0, Total:    61, Duration: 2\n"
        with self.assertRaises(vc.CanonicalizerError):
            vc.canonicalize_stream(raw)

    def test_non_numeric_duration_raises(self):
        raw = _banner().replace(b"Duration: 2 s", b"Duration: N/A s")
        with self.assertRaises(vc.CanonicalizerError):
            vc.canonicalize_stream(raw)


class TestReplacementRecordShape(unittest.TestCase):
    def test_replacement_records_rule_name_line_number_and_hashes(self):
        raw = _banner(duration="2")
        out, report = vc.canonicalize_stream(raw)
        self.assertEqual(len(report["replacements"]), 1)
        entry = report["replacements"][0]
        self.assertEqual(entry["rule_name"], "vstest_duration")
        self.assertEqual(entry["line_number"], 1)
        before_line = raw.decode().splitlines()[0]
        after_line = out.decode().splitlines()[0]
        self.assertEqual(entry["before_line_sha256"], hashlib.sha256(before_line.encode()).hexdigest())
        self.assertEqual(entry["after_line_sha256"], hashlib.sha256(after_line.encode()).hexdigest())


class TestRealEvidenceLineHashMatches(unittest.TestCase):
    def test_before_line_sha256_matches_real_ci_evidence(self):
        # The exact real capture-a raw line (run 29466573023) -- its
        # before_line_sha256 here must match the sha256 recorded against
        # "side: a" in the real pair-reproducibility-report.json
        # (artifact 8363205429)'s unmatched_raw_diff_lines.
        raw = _banner(duration="2")
        _, report = vc.canonicalize_stream(raw)
        self.assertEqual(
            report["replacements"][0]["before_line_sha256"],
            "8153ad7156fef14587d4dd3dfe21dc2c867c2ef4cd40149436b0e241676bb396",
        )


def _gen_line(project: str, dll_path: str) -> str:
    return f"  {project} -> {dll_path}"


class TestMsbuildCompletionPairOrderRule(unittest.TestCase):
    """Policy v2 (D1b, 2026-07-16): a second, structurally distinct
    canonicalization -- reordering exactly the two named MSBuild
    project-completion lines into a fixed declared order, bounded by hard
    preconditions that fail closed on anything else."""

    GEN = _gen_line("KubeOps.Generator", "/w/src/KubeOps.Generator/bin/Debug/netstandard2.0/KubeOps.Generator.dll")
    ENTITIES = _gen_line(
        "KubeOps.Generator.Test.Entities",
        "/w/test/KubeOps.Generator.Test.Entities/bin/Debug/net10.0/KubeOps.Generator.Test.Entities.dll",
    )
    TEST_DLL = _gen_line(
        "KubeOps.Generator.Test", "/w/test/KubeOps.Generator.Test/bin/Debug/net10.0/KubeOps.Generator.Test.dll"
    )

    def _stream(self, *lines: str) -> bytes:
        return ("\n".join(lines) + "\n").encode("utf-8")

    def test_swapped_pair_canonicalizes_identically(self):
        capture_a = self._stream(self.ENTITIES, self.GEN, self.TEST_DLL)
        capture_b = self._stream(self.GEN, self.ENTITIES, self.TEST_DLL)
        self.assertNotEqual(capture_a, capture_b)
        canon_a, report_a = vc.canonicalize_stream(capture_a)
        canon_b, report_b = vc.canonicalize_stream(capture_b)
        self.assertEqual(canon_a, canon_b)
        # Canonical order is KubeOps.Generator first.
        self.assertEqual(canon_a.decode().splitlines()[:2], [self.GEN, self.ENTITIES])
        self.assertFalse(report_a["structural_operations"][0]["already_canonical"])
        self.assertTrue(report_b["structural_operations"][0]["already_canonical"])

    def test_already_canonical_order_remains_unchanged(self):
        stream = self._stream(self.GEN, self.ENTITIES, self.TEST_DLL)
        out, report = vc.canonicalize_stream(stream)
        self.assertEqual(out, stream)
        self.assertTrue(report["structural_operations"][0]["already_canonical"])

    def test_changed_assembly_path_still_differs_after_canonicalization(self):
        # A genuinely different build output path must never be masked by
        # this rule -- it only reorders, never rewrites path content.
        gen_different_path = _gen_line("KubeOps.Generator", "/w/DIFFERENT/KubeOps.Generator.dll")
        capture_a = self._stream(self.ENTITIES, self.GEN, self.TEST_DLL)
        capture_b = self._stream(gen_different_path, self.ENTITIES, self.TEST_DLL)
        canon_a, _ = vc.canonicalize_stream(capture_a)
        canon_b, _ = vc.canonicalize_stream(capture_b)
        self.assertNotEqual(canon_a, canon_b)

    def test_missing_one_of_the_pair_fails_closed(self):
        # Only one of the two authorized lines present -- must raise, never
        # silently pass through un-reordered.
        stream = self._stream(self.GEN, self.TEST_DLL)
        with self.assertRaises(vc.CanonicalizerError):
            vc.canonicalize_stream(stream)

    def test_duplicated_project_line_fails_closed(self):
        stream = self._stream(self.GEN, self.GEN, self.ENTITIES, self.TEST_DLL)
        with self.assertRaises(vc.CanonicalizerError):
            vc.canonicalize_stream(stream)

    def test_third_project_line_between_the_pair_fails_closed(self):
        # An unrelated (or even authorized-adjacent) project's completion
        # line inserted BETWEEN the two authorized lines breaks contiguity.
        stream = self._stream(self.GEN, self.TEST_DLL, self.ENTITIES)
        with self.assertRaises(vc.CanonicalizerError):
            vc.canonicalize_stream(stream)

    def test_warning_line_between_the_pair_fails_closed(self):
        warning = (
            "/home/runner/.nuget/packages/microsoft.sourcelink.common/10.0.300/build/"
            "Microsoft.SourceLink.Common.targets(56,5): warning : some warning text"
        )
        stream = self._stream(self.ENTITIES, warning, self.GEN, self.TEST_DLL)
        with self.assertRaises(vc.CanonicalizerError):
            vc.canonicalize_stream(stream)

    def test_noncontiguous_pair_separated_by_blank_lines_is_allowed(self):
        # The authorization explicitly permits blank-line separation.
        stream = self._stream(self.ENTITIES, "", "", self.GEN, self.TEST_DLL)
        out, report = vc.canonicalize_stream(stream)
        lines = out.decode().splitlines()
        self.assertEqual(lines[0], self.GEN)
        self.assertFalse(report["structural_operations"][0]["already_canonical"])

    def test_another_case_module_has_no_such_rule(self):
        # maven_canonicalizer.py must never gain this rule -- independent
        # profiles, never merged.
        import maven_canonicalizer as mc

        self.assertFalse(hasattr(mc, "STRUCTURAL_RULES"))

    def test_arbitrary_other_msbuild_lines_are_never_sorted(self):
        # Unrelated "-> " lines outside the two authorized project names,
        # appearing in a scrambled order, must be left completely untouched.
        other_a = _gen_line("SomeOtherProject", "/w/SomeOtherProject.dll")
        other_b = _gen_line("AnotherProject", "/w/AnotherProject.dll")
        stream = self._stream(other_b, other_a, self.GEN, self.ENTITIES)
        out, _ = vc.canonicalize_stream(stream)
        lines = out.decode().splitlines()
        self.assertEqual(lines[0], other_b)
        self.assertEqual(lines[1], other_a)

    def test_neither_line_present_is_a_no_op(self):
        stream = self._stream(self.TEST_DLL)
        out, report = vc.canonicalize_stream(stream)
        self.assertEqual(out, stream)
        self.assertEqual(report["structural_operations"], [])

    def test_idempotent_on_its_own_reordered_output(self):
        capture = self._stream(self.ENTITIES, self.GEN, self.TEST_DLL)
        once, _ = vc.canonicalize_stream(capture)
        twice, report_twice = vc.canonicalize_stream(once)
        self.assertEqual(once, twice)
        self.assertTrue(report_twice["structural_operations"][0]["already_canonical"])

    def test_before_and_after_block_hashes_are_reproducible(self):
        capture = self._stream(self.ENTITIES, self.GEN, self.TEST_DLL)
        _, report = vc.canonicalize_stream(capture)
        op = report["structural_operations"][0]
        expected_before = hashlib.sha256((self.ENTITIES + "\n" + self.GEN + "\n").encode()).hexdigest()
        expected_after = hashlib.sha256((self.GEN + "\n" + self.ENTITIES + "\n").encode()).hexdigest()
        self.assertEqual(op["before_block_sha256"], expected_before)
        self.assertEqual(op["after_block_sha256"], expected_after)


class TestStructuralPolicyIntegrity(unittest.TestCase):
    def test_policy_file_verifies_against_code(self):
        policy_path = TOOLS.parent / "vstest-capture-canonicalization-policy.json"
        policy = vc.load_and_verify_policy(policy_path)
        self.assertEqual(policy["policy_version"], 2)
        self.assertIn("structural_rules", policy)
        self.assertEqual(
            policy["structural_rules"][0]["rule_name"], "msbuild_completion_pair_order"
        )


if __name__ == "__main__":
    unittest.main()
