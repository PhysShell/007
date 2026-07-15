"""Unit tests for content_acceptance.py -- the fail-closed content-
acceptance gate added after real inspection of CI run #6 found all 18
"successful" captures were actually infrastructure/sandbox failures that
nothing had ever validated against.
"""
import sys
import unittest
from pathlib import Path

TOOLS = Path(__file__).resolve().parents[1] / "tools"
sys.path.insert(0, str(TOOLS))
import content_acceptance as ca  # noqa: E402

# Real stderr bytes captured from CI run #6 (see capture-content-audit-run6.json)
REAL_RUSTUP_FAILURE_STDERR = (
    b"sandboy: warning: Landlock only PARTIALLY enforced (kernel too old for some access rights)\n"
    b"error: rustup could not choose a version of cargo to run, because one wasn't specified explicitly, "
    b"and no default is configured.\n"
    b"help: run 'rustup default stable' to download the latest stable release of Rust and set it as your "
    b"default toolchain.\n"
    b"Command exited with non-zero status 1\n"
)
REAL_DEV_NULL_FAILURE_STDERR = (
    b"sandboy: warning: Landlock only PARTIALLY enforced (kernel too old for some access rights)\n"
    b"/usr/bin/mvn: 61: cannot create /dev/null: Permission denied\n"
    b"/usr/bin/mvn: 1: cd: can't cd to /usr/bin//etc/alternatives/..\n"
    b"Command exited with non-zero status 1\n"
)
REAL_VENV_FAILURE_STDERR = (
    b"sandboy: warning: Landlock only PARTIALLY enforced (kernel too old for some access rights)\n"
    b"Fatal Python error: init_import_site: Failed to import the site module\n"
    b"PermissionError: [Errno 13] Permission denied: '/home/runner/work/_temp/venv-repo-pyflakes/pyvenv.cfg'\n"
)
REAL_NU1301_STDOUT = (
    b"  Determining projects to restore...\n"
    b"error NU1301: Unable to load the service index for source https://api.nuget.org/v3/index.json.\n"
)
HARMLESS_LANDLOCK_WARNING_STDERR = b"sandboy: warning: Landlock only PARTIALLY enforced (kernel too old for some access rights)\n"


class TestDetectInfrastructureFailure(unittest.TestCase):
    def test_rustup_no_default_toolchain_detected(self):
        self.assertEqual(ca.detect_infrastructure_failure(b"", REAL_RUSTUP_FAILURE_STDERR), "rustup-no-default-toolchain")

    def test_dev_null_permission_denied_detected(self):
        self.assertEqual(ca.detect_infrastructure_failure(b"", REAL_DEV_NULL_FAILURE_STDERR), "dev-null-permission-denied")

    def test_python_venv_permission_denied_detected(self):
        self.assertEqual(ca.detect_infrastructure_failure(b"", REAL_VENV_FAILURE_STDERR), "python-venv-permission-denied")

    def test_nuget_nu1301_detected(self):
        self.assertEqual(ca.detect_infrastructure_failure(REAL_NU1301_STDOUT, b""), "nuget-restore-failure-nu1301")

    def test_harmless_landlock_warning_alone_is_not_a_failure(self):
        # The partial-enforcement warning line is expected on every real run
        # (including eventually-valid ones) -- it must never itself trigger
        # a false-positive rejection.
        self.assertIsNone(ca.detect_infrastructure_failure(b"real test output here", HARMLESS_LANDLOCK_WARNING_STDERR))

    def test_clean_output_has_no_detected_failure(self):
        self.assertIsNone(ca.detect_infrastructure_failure(b"test result: ok. 3 passed; 0 failed\n", b""))


class TestTerminationAllowed(unittest.TestCase):
    def test_zero_is_allowed(self):
        self.assertTrue(ca.termination_allowed(0))

    def test_pytest_style_nonzero_is_allowed(self):
        self.assertTrue(ca.termination_allowed(1))

    def test_command_not_found_style_codes_rejected(self):
        self.assertFalse(ca.termination_allowed(126))
        self.assertFalse(ca.termination_allowed(127))


class TestCaseSemanticMarkers(unittest.TestCase):
    def test_cargo_test_marker(self):
        ok, _ = ca.CASE_SEMANTIC_VALIDATORS["repo-rustlings"]("running 3 tests\ntest result: ok. 3 passed; 0 failed\n")
        self.assertTrue(ok)
        ok, _ = ca.CASE_SEMANTIC_VALIDATORS["repo-rustlings"]("")
        self.assertFalse(ok)

    def test_hyperfine_marker(self):
        ok, _ = ca.CASE_SEMANTIC_VALIDATORS["repo-hyperfine"]("hyperfine 1.19.0\n")
        self.assertTrue(ok)
        ok, _ = ca.CASE_SEMANTIC_VALIDATORS["repo-hyperfine"]("")
        self.assertFalse(ok)

    def test_maven_marker(self):
        ok, _ = ca.CASE_SEMANTIC_VALIDATORS["repo-docker-java-parser"]("[INFO] BUILD SUCCESS\n")
        self.assertTrue(ok)
        ok, _ = ca.CASE_SEMANTIC_VALIDATORS["repo-docker-java-parser"]("Tests run: 42, Failures: 0\n")
        self.assertTrue(ok)

    def test_dotnet_test_marker(self):
        ok, _ = ca.CASE_SEMANTIC_VALIDATORS["repo-kubeops-generator"]("Passed!  - Failed: 0, Passed: 61, Total: 61\n")
        self.assertTrue(ok)

    def test_pytest_marker(self):
        ok, _ = ca.CASE_SEMANTIC_VALIDATORS["repo-requests"]("========== 614 passed, 5 failed in 12.34s ==========\n")
        self.assertTrue(ok)

    def test_gradle_marker(self):
        ok, _ = ca.CASE_SEMANTIC_VALIDATORS["repo-spotless"]("BUILD SUCCESSFUL in 3s\n")
        self.assertTrue(ok)
        ok, _ = ca.CASE_SEMANTIC_VALIDATORS["repo-moshi"]("BUILD FAILED in 3s\n")
        self.assertTrue(ok)

    def test_pyflakes_marker_requires_nonempty(self):
        ok, _ = ca.CASE_SEMANTIC_VALIDATORS["repo-pyflakes"]("foo.py:1:1 'os' imported but unused\n")
        self.assertTrue(ok)
        ok, _ = ca.CASE_SEMANTIC_VALIDATORS["repo-pyflakes"]("")
        self.assertFalse(ok)


class TestValidateCaptureContent(unittest.TestCase):
    def test_real_rustup_failure_is_rejected(self):
        report = ca.validate_capture_content(
            case_id="repo-hyperfine", canonical_stream_bytes=b"",
            raw_stdout=b"", raw_stderr=REAL_RUSTUP_FAILURE_STDERR, exit_code=1,
        )
        self.assertFalse(report["accepted"])
        self.assertEqual(report["infrastructure_failure_detected"], "rustup-no-default-toolchain")
        self.assertFalse(report["checks"]["canonical_is_nonempty"])

    def test_real_dotnet_nu1301_is_rejected_even_though_nonempty(self):
        # This is exactly the repo-kubeops-generator finding: real, nonempty
        # stdout that is NOT genuine workload output.
        report = ca.validate_capture_content(
            case_id="repo-kubeops-generator", canonical_stream_bytes=REAL_NU1301_STDOUT,
            raw_stdout=REAL_NU1301_STDOUT, raw_stderr=b"", exit_code=1,
        )
        self.assertFalse(report["accepted"])
        self.assertEqual(report["infrastructure_failure_detected"], "nuget-restore-failure-nu1301")
        self.assertTrue(report["checks"]["canonical_is_nonempty"])  # nonempty, but still rejected

    def test_genuinely_valid_capture_is_accepted(self):
        stdout = b"running 3 tests\ntest result: ok. 3 passed; 0 failed; 0 ignored\n"
        report = ca.validate_capture_content(
            case_id="repo-rustlings", canonical_stream_bytes=stdout,
            raw_stdout=stdout, raw_stderr=b"", exit_code=0,
        )
        self.assertTrue(report["accepted"])
        self.assertEqual(report["rejection_reasons"], [])

    def test_pytest_test_failures_permitted_with_valid_summary(self):
        # repo-requests: a completion with real test FAILURES and a nonzero
        # exit code must still be accepted -- exit code alone is not the gate.
        stdout = b"===== 614 passed, 5 failed, 15 skipped in 42.1s =====\n"
        report = ca.validate_capture_content(
            case_id="repo-requests", canonical_stream_bytes=stdout,
            raw_stdout=stdout, raw_stderr=b"", exit_code=1,
        )
        self.assertTrue(report["accepted"])

    def test_genuinely_empty_pyflakes_after_clean_run_is_rejected_not_synthesized(self):
        # Without the authorized erratum resolution signal (e.g. a caller
        # that never passes execution_argv_resolution), a genuinely empty
        # pyflakes result still fails closed -- the bypass below requires
        # ALL of its conditions, not just emptiness+exit-0.
        report = ca.validate_capture_content(
            case_id="repo-pyflakes", canonical_stream_bytes=b"",
            raw_stdout=b"", raw_stderr=b"", exit_code=0,
        )
        self.assertFalse(report["accepted"])
        self.assertIn("canonical stream is empty", report["rejection_reasons"])

    def test_invalid_utf8_canonical_stream_is_rejected(self):
        report = ca.validate_capture_content(
            case_id="repo-hyperfine", canonical_stream_bytes=b"hyperfine \xff\xfe garbage",
            raw_stdout=b"hyperfine \xff\xfe garbage", raw_stderr=b"", exit_code=0,
        )
        self.assertFalse(report["accepted"])
        self.assertFalse(report["checks"]["canonical_is_valid_utf8"])

    def test_abnormal_exit_code_is_rejected(self):
        report = ca.validate_capture_content(
            case_id="repo-hyperfine", canonical_stream_bytes=b"hyperfine 1.19.0\n",
            raw_stdout=b"hyperfine 1.19.0\n", raw_stderr=b"", exit_code=127,
        )
        self.assertFalse(report["accepted"])
        self.assertFalse(report["checks"]["termination_allowed"])

    def test_report_written_even_when_rejected_has_all_expected_keys(self):
        report = ca.validate_capture_content(
            case_id="repo-hyperfine", canonical_stream_bytes=b"",
            raw_stdout=b"", raw_stderr=REAL_RUSTUP_FAILURE_STDERR, exit_code=1,
        )
        for key in ("report_type", "case_id", "exit_code", "checks", "infrastructure_failure_detected",
                    "case_semantic_marker_description", "accepted", "rejection_reasons",
                    "content_classification", "empty_output_authorized", "approving_decision_identity"):
            self.assertIn(key, report)


class TestAuthorizedPyflakesEmptyOutput(unittest.TestCase):
    """D1b decision (2026-07-15): repo-pyflakes, and ONLY repo-pyflakes, may
    pass with a genuinely empty canonical stream when every one of the
    authorized conditions holds. Every other case keeps the unconditional
    nonempty-canonical-stream requirement."""

    def test_empty_pyflakes_exit_0_clean_stderr_authorized_erratum_passes(self):
        report = ca.validate_capture_content(
            case_id="repo-pyflakes", canonical_stream_bytes=b"",
            raw_stdout=b"", raw_stderr=HARMLESS_LANDLOCK_WARNING_STDERR, exit_code=0,
            execution_argv_resolution="authorized-n2d1b-erratum",
        )
        self.assertTrue(report["accepted"])
        self.assertEqual(report["rejection_reasons"], [])
        self.assertEqual(report["content_classification"], "successful-empty-domain-result")
        self.assertTrue(report["empty_output_authorized"])
        self.assertEqual(report["approving_decision_identity"], ca.PYFLAKES_EMPTY_OUTPUT_AUTHORIZATION_ID)
        self.assertFalse(report["checks"]["canonical_is_nonempty"])

    def test_empty_pyflakes_with_nonzero_exit_fails(self):
        report = ca.validate_capture_content(
            case_id="repo-pyflakes", canonical_stream_bytes=b"",
            raw_stdout=b"", raw_stderr=HARMLESS_LANDLOCK_WARNING_STDERR, exit_code=1,
            execution_argv_resolution="authorized-n2d1b-erratum",
        )
        self.assertFalse(report["accepted"])
        self.assertFalse(report["empty_output_authorized"])

    def test_empty_pyflakes_with_infrastructure_signature_fails(self):
        report = ca.validate_capture_content(
            case_id="repo-pyflakes", canonical_stream_bytes=b"",
            raw_stdout=b"", raw_stderr=REAL_VENV_FAILURE_STDERR, exit_code=0,
            execution_argv_resolution="authorized-n2d1b-erratum",
        )
        self.assertFalse(report["accepted"])
        self.assertEqual(report["infrastructure_failure_detected"], "python-venv-permission-denied")
        self.assertFalse(report["empty_output_authorized"])

    def test_empty_pyflakes_without_the_authorized_erratum_resolution_fails(self):
        # Exit 0 and clean stderr alone are not sufficient -- the effective
        # argv must specifically be the authorized pyflakes erratum.
        report = ca.validate_capture_content(
            case_id="repo-pyflakes", canonical_stream_bytes=b"",
            raw_stdout=b"", raw_stderr=HARMLESS_LANDLOCK_WARNING_STDERR, exit_code=0,
            execution_argv_resolution="frozen",
        )
        self.assertFalse(report["accepted"])
        self.assertFalse(report["empty_output_authorized"])

    def test_empty_pyflakes_with_traceback_in_stderr_fails(self):
        stderr = HARMLESS_LANDLOCK_WARNING_STDERR + b"Traceback (most recent call last):\n  File \"x.py\"\nValueError\n"
        report = ca.validate_capture_content(
            case_id="repo-pyflakes", canonical_stream_bytes=b"",
            raw_stdout=b"", raw_stderr=stderr, exit_code=0,
            execution_argv_resolution="authorized-n2d1b-erratum",
        )
        self.assertFalse(report["accepted"])
        self.assertFalse(report["empty_output_authorized"])

    def test_empty_output_for_any_other_case_still_fails(self):
        # The exact same otherwise-qualifying conditions, for a DIFFERENT
        # case_id, must never trigger the bypass -- it is repo-pyflakes-only.
        report = ca.validate_capture_content(
            case_id="repo-hyperfine", canonical_stream_bytes=b"",
            raw_stdout=b"", raw_stderr=HARMLESS_LANDLOCK_WARNING_STDERR, exit_code=0,
            execution_argv_resolution="authorized-n2d1b-erratum",
        )
        self.assertFalse(report["accepted"])
        self.assertFalse(report["empty_output_authorized"])
        self.assertIsNone(report["approving_decision_identity"])

    def test_nonempty_pyflakes_violations_remain_accepted_when_semantically_valid(self):
        stdout = b"foo.py:1:1 'os' imported but unused\n"
        report = ca.validate_capture_content(
            case_id="repo-pyflakes", canonical_stream_bytes=stdout,
            raw_stdout=stdout, raw_stderr=HARMLESS_LANDLOCK_WARNING_STDERR, exit_code=1,
            execution_argv_resolution="authorized-n2d1b-erratum",
        )
        self.assertTrue(report["accepted"])
        self.assertFalse(report["empty_output_authorized"])
        self.assertEqual(report["content_classification"], "genuine-workload-output")

    def test_empty_string_sha256_is_the_well_known_constant(self):
        # Documents the invariant referenced by the D1b authorization: a
        # genuinely empty canonical stream always hashes to the same
        # well-known SHA-256 of the empty byte string, for both captures.
        import hashlib
        self.assertEqual(
            hashlib.sha256(b"").hexdigest(),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        )


if __name__ == "__main__":
    unittest.main()
