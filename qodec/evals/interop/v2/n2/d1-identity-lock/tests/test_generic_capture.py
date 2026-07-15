"""Unit tests for generic_capture.py's argv resolution: erratum lookup plus
the argv0_override mechanism that must be applied AFTER erratum resolution,
never before it.
"""
import json
import sys
import unittest
from pathlib import Path

TOOLS = Path(__file__).resolve().parents[1] / "tools"
REPO_ROOT = Path(__file__).resolve().parents[7]
sys.path.insert(0, str(TOOLS))
import generic_capture as gc  # noqa: E402

REAL_ERRATA_PATH = REPO_ROOT / "qodec/evals/interop/v2/n2/d1-identity-lock/execution-plan-errata.json"


class TestVerifyRelativeArgv0Exists(unittest.TestCase):
    def test_absolute_argv0_is_never_checked(self):
        # Only relative wrapper-style argv0s (./gradlew, ../foo) are checked;
        # an absolute path (e.g. a resolved venv python) is the caller's own
        # responsibility and must not be second-guessed here.
        gc.verify_relative_argv0_exists("/usr/bin/python3", Path("/does/not/exist"))  # must not raise

    def test_bare_command_name_is_never_checked(self):
        gc.verify_relative_argv0_exists("cargo", Path("/does/not/exist"))  # must not raise

    def test_existing_relative_wrapper_passes(self):
        import tempfile

        with tempfile.TemporaryDirectory() as tmp:
            cwd = Path(tmp)
            (cwd / "gradlew").write_text("#!/bin/sh\n")
            gc.verify_relative_argv0_exists("./gradlew", cwd)  # must not raise

    def test_missing_relative_wrapper_raises_with_clear_diagnostic(self):
        # This is exactly the real risk flagged for repo-moshi: its manifest
        # names project.entry_point "moshi" even though the frozen argv's
        # wrapper actually lives at the repository root, not under moshi/.
        # If cwd were ever wrongly derived from entry_point instead of the
        # true source_root, this must fail loudly here, not as an opaque
        # sandbox/ENOENT error deep inside the confined run.
        import tempfile

        with tempfile.TemporaryDirectory() as tmp:
            cwd = Path(tmp) / "moshi"  # wrong cwd: gradlew is NOT here
            cwd.mkdir()
            with self.assertRaises(gc.GenericCaptureFailure) as ctx:
                gc.verify_relative_argv0_exists("./gradlew", cwd)
            self.assertIn("./gradlew", str(ctx.exception))
            self.assertIn(str(cwd), str(ctx.exception))


class TestDedupeSanitizerRuleNames(unittest.TestCase):
    def test_empty_reports_produce_empty_list(self):
        self.assertEqual(gc.dedupe_sanitizer_rule_names({}, {}), [])

    def test_dict_shaped_rules_applied_does_not_raise(self):
        # A real repo-kubeops-generator/dotnet capture crashed with
        # "TypeError: unhashable type: 'dict'" because rules_applied is a
        # list of {"rule": ..., "replacements": ...} dicts (sanitizer.py's
        # real shape), not plain rule-name strings -- set()-ing them
        # directly blew up the moment any real rule matched (here:
        # dotnet_time_elapsed_line, from dotnet test's real "Time Elapsed
        # HH:MM:SS.ffffff" output line).
        stdout_report = {"rules_applied": [{"rule": "dotnet_time_elapsed_line", "replacements": 1}]}
        stderr_report = {"rules_applied": [{"rule": "ansi_csi", "replacements": 3}]}
        result = gc.dedupe_sanitizer_rule_names(stdout_report, stderr_report)
        self.assertEqual(result, ["ansi_csi", "dotnet_time_elapsed_line"])

    def test_duplicate_rule_names_across_streams_are_deduped(self):
        stdout_report = {"rules_applied": [{"rule": "cr", "replacements": 2}]}
        stderr_report = {"rules_applied": [{"rule": "cr", "replacements": 5}]}
        self.assertEqual(gc.dedupe_sanitizer_rule_names(stdout_report, stderr_report), ["cr"])


class TestResolveEffectiveArgv(unittest.TestCase):
    def _write_errata(self, tmp_path, case_id, original_argv, corrected_argv):
        entry = {
            "case_id": case_id,
            "status": "AUTHORIZED_ERRATUM",
            "original_frozen_argv": original_argv,
            "corrected_effective_argv": corrected_argv,
        }
        body = {"entries": [entry], "errata_sha256": "test-only-not-a-real-hash"}
        path = tmp_path / "errata.json"
        path.write_text(json.dumps(body))
        return path

    def test_no_erratum_returns_frozen_argv_unchanged(self, tmp_path=None):
        import tempfile
        with tempfile.TemporaryDirectory() as tmp:
            path = self._write_errata(Path(tmp), "some-other-case", ["a"], ["b"])
            argv, resolution, erratum_sha = gc.resolve_effective_argv("repo-hyperfine", ["cargo", "run"], path)
        self.assertEqual(argv, ["cargo", "run"])
        self.assertEqual(resolution, "frozen")
        self.assertIsNone(erratum_sha)

    def test_matching_erratum_applies_correction(self):
        import tempfile
        with tempfile.TemporaryDirectory() as tmp:
            path = self._write_errata(Path(tmp), "repo-pyflakes", ["python", "-m", "pyflakes", "src/"],
                                       ["python", "-m", "pyflakes", "pyflakes/"])
            argv, resolution, erratum_sha = gc.resolve_effective_argv(
                "repo-pyflakes", ["python", "-m", "pyflakes", "src/"], path
            )
        self.assertEqual(argv, ["python", "-m", "pyflakes", "pyflakes/"])
        self.assertEqual(resolution, "authorized-n2d1b-erratum")
        self.assertIsNotNone(erratum_sha)

    def test_stale_erratum_raises_when_frozen_argv_already_substituted(self):
        # This is exactly the real bug: a caller that pre-substitutes a venv
        # python path into argv[0] BEFORE calling resolve_effective_argv
        # breaks the exact-match comparison against the erratum's recorded
        # original_frozen_argv.
        import tempfile
        with tempfile.TemporaryDirectory() as tmp:
            path = self._write_errata(Path(tmp), "repo-pyflakes", ["python", "-m", "pyflakes", "src/"],
                                       ["python", "-m", "pyflakes", "pyflakes/"])
            already_substituted = ["/home/runner/work/_temp/venv-repo-pyflakes/bin/python", "-m", "pyflakes", "src/"]
            with self.assertRaises(gc.GenericCaptureFailure):
                gc.resolve_effective_argv("repo-pyflakes", already_substituted, path)

    def test_pure_frozen_argv_plus_post_hoc_override_reproduces_correct_final_argv(self):
        # The fixed calling convention: pass the PURE frozen argv in, get the
        # erratum-corrected argv out, THEN apply argv0_override -- matching
        # what run_one_capture now does internally.
        import tempfile
        with tempfile.TemporaryDirectory() as tmp:
            path = self._write_errata(Path(tmp), "repo-pyflakes", ["python", "-m", "pyflakes", "src/"],
                                       ["python", "-m", "pyflakes", "pyflakes/"])
            effective_argv, resolution, _ = gc.resolve_effective_argv(
                "repo-pyflakes", ["python", "-m", "pyflakes", "src/"], path
            )
            venv_python = "/home/runner/work/_temp/venv-repo-pyflakes/bin/python"
            final_argv = [venv_python, *effective_argv[1:]]
        self.assertEqual(final_argv, [venv_python, "-m", "pyflakes", "pyflakes/"])
        self.assertEqual(resolution, "authorized-n2d1b-erratum")

    def test_against_real_committed_errata_file(self):
        self.assertTrue(REAL_ERRATA_PATH.is_file())
        argv, resolution, erratum_sha = gc.resolve_effective_argv(
            "repo-pyflakes", ["python", "-m", "pyflakes", "src/"], REAL_ERRATA_PATH
        )
        self.assertEqual(argv, ["python", "-m", "pyflakes", "pyflakes/"])
        self.assertEqual(resolution, "authorized-n2d1b-erratum")
        self.assertIsNotNone(erratum_sha)


if __name__ == "__main__":
    unittest.main()
