"""Unit tests for generic_capture.py's argv resolution: erratum lookup plus
the argv0_override mechanism that must be applied AFTER erratum resolution,
never before it.
"""
import json
import sys
import unittest
import unittest.mock as mock
from pathlib import Path

TOOLS = Path(__file__).resolve().parents[1] / "tools"
REPO_ROOT = Path(__file__).resolve().parents[7]
sys.path.insert(0, str(TOOLS))
import generic_capture as gc  # noqa: E402

REAL_ERRATA_PATH = REPO_ROOT / "qodec/evals/interop/v2/n2/d1-identity-lock/execution-plan-errata.json"


class TestRunOneCaptureContentGate(unittest.TestCase):
    """Integration-level tests proving the fail-closed content-acceptance
    gate (content_acceptance.py) actually gates run_one_capture -- a
    schema-valid receipt alone can no longer make a capture report success."""

    def _make_source_artifact_dir(self, tmp_path):
        import hashlib
        import tarfile

        source_artifact_dir = tmp_path / "source-artifact"
        source_artifact_dir.mkdir()
        tar_path = source_artifact_dir / "source.tar"
        src_file = tmp_path / "hello.txt"
        src_file.write_text("hello\n")
        with tarfile.open(tar_path, "w") as tar:
            tar.add(src_file, arcname="hello.txt")
        archive_sha256 = hashlib.sha256(tar_path.read_bytes()).hexdigest()
        (source_artifact_dir / "acquisition-receipt.json").write_text(json.dumps({
            "actual_head_sha": "deadbeef" * 5,
            "normalized_archive_sha256": archive_sha256,
            "license_sha256": "cafe" * 16,
        }))
        return source_artifact_dir

    def _fake_toolchain_capture_fn(self, source_root):
        return {
            "resolved_version": "1.97.0",
            "runtime_identifier": "x86_64-unknown-linux-gnu",
            "rustc_binary_path": "/usr/bin/true",
            "rustc_binary_sha256": "a" * 64,
        }

    def _run(self, tmp_path, raw_stdout: bytes, raw_stderr: bytes, exit_code: int):
        import unittest.mock as mock

        source_artifact_dir = self._make_source_artifact_dir(tmp_path)
        work_dir = tmp_path / "work"
        out_dir = tmp_path / "out"
        fake_result = {
            "raw_stdout": raw_stdout, "raw_stderr": raw_stderr, "exit_code": exit_code,
            "wall_time_s": 1.0, "peak_rss_kb": 1024,
        }
        with mock.patch.object(gc.capture_build, "run_real_build", return_value=fake_result):
            return gc.run_one_capture(
                case_id="repo-rustlings", ecosystem="rust", job_name="capture-a",
                source_artifact_dir=source_artifact_dir, work_dir=work_dir, out_dir=out_dir,
                frozen_argv=["cargo", "test"], errata_path=REAL_ERRATA_PATH,
                sandboy_bin=Path("/nonexistent/sandboy"), sandboy_commit_sha="e" * 40,
                toolchain_capture_fn=self._fake_toolchain_capture_fn,
                toolchain_env_values={"CARGO_HOME": str(tmp_path / "cargo-home")},
                canonical_stream="stdout", primary_stream_rationale="test",
                project_writable_dirs_relative=[],
                requested_version_or_range="stable", resolver_mechanism="test",
            ), out_dir

    def test_infrastructure_failure_output_is_rejected_not_promoted_to_a_receipt(self):
        import tempfile

        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            real_rustup_stderr = (
                b"error: rustup could not choose a version of cargo to run, because one wasn't "
                b"specified explicitly, and no default is configured.\n"
            )
            with self.assertRaises(gc.GenericCaptureFailure):
                self._run(tmp_path, raw_stdout=b"", raw_stderr=real_rustup_stderr, exit_code=1)
            out_dir = tmp_path / "out"
            # The content-validation-report.json must exist and record the
            # rejection EVEN THOUGH the capture failed -- per spec, failed
            # captures must still get a structured report.
            report = json.loads((out_dir / "content-validation-report.json").read_text())
            self.assertFalse(report["accepted"])
            self.assertEqual(report["infrastructure_failure_detected"], "rustup-no-default-toolchain")
            # No receipt.json should exist for a rejected capture -- a
            # schema-valid receipt must never be produced without a passing
            # content-validation report.
            self.assertFalse((out_dir / "receipt.json").exists())

    def test_genuinely_valid_capture_produces_a_receipt_referencing_its_validation_report(self):
        import tempfile

        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            valid_stdout = b"running 3 tests\ntest result: ok. 3 passed; 0 failed; 0 ignored\n"
            receipt, out_dir = self._run(tmp_path, raw_stdout=valid_stdout, raw_stderr=b"", exit_code=0)
            self.assertTrue((out_dir / "receipt.json").exists())
            report = json.loads((out_dir / "content-validation-report.json").read_text())
            self.assertTrue(report["accepted"])
            self.assertEqual(receipt["content_validation_report_sha256"], hashlib_sha256_of_report(report))

    def test_project_writable_files_relative_is_pre_touched_and_granted_fs_rw(self):
        # A real capture (CI run #8) showed repo-docker-java-parser's own
        # pom.xml write "dependency.tree" directly into the (otherwise
        # read-only) project root -- a FILE, not one of the known
        # build-output directories already covered by
        # project_writable_dirs_relative.
        import tempfile
        import unittest.mock as mock

        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            source_artifact_dir = self._make_source_artifact_dir(tmp_path)
            work_dir = tmp_path / "work"
            out_dir = tmp_path / "out"
            fake_result = {
                "raw_stdout": b"[INFO] BUILD SUCCESS\n", "raw_stderr": b"", "exit_code": 0,
                "wall_time_s": 1.0, "peak_rss_kb": 1024,
            }
            with mock.patch.object(gc.capture_build, "run_real_build", return_value=fake_result):
                gc.run_one_capture(
                    case_id="repo-docker-java-parser", ecosystem="jvm-maven", job_name="capture-a",
                    source_artifact_dir=source_artifact_dir, work_dir=work_dir, out_dir=out_dir,
                    frozen_argv=["mvn", "test"], errata_path=REAL_ERRATA_PATH,
                    sandboy_bin=Path("/nonexistent/sandboy"), sandboy_commit_sha="e" * 40,
                    toolchain_capture_fn=lambda source_root: {
                        "resolved_version": "3.8.7", "runtime_identifier": "11",
                        "mvn_binary_path": "/usr/bin/true", "mvn_binary_sha256": "a" * 64,
                    },
                    toolchain_env_values={"JAVA_HOME": "/usr/lib/jvm/java-11"},
                    canonical_stream="stdout", primary_stream_rationale="test",
                    project_writable_dirs_relative=["target"],
                    project_writable_files_relative=["dependency.tree"],
                    requested_version_or_range="11", resolver_mechanism="test",
                )
            source_root = work_dir / "source"
            self.assertTrue((source_root / "dependency.tree").is_file())
            policy_text = (work_dir / "policy.toml").read_text()
            fs_rw_line = next(line for line in policy_text.splitlines() if line.startswith("fs_rw"))
            self.assertIn(str(source_root / "dependency.tree"), fs_rw_line)


def hashlib_sha256_of_report(report: dict) -> str:
    import hashlib
    return hashlib.sha256((json.dumps(report, indent=2, sort_keys=True) + "\n").encode()).hexdigest()


class TestGradleNetworkEnforcementWiring(unittest.TestCase):
    """jvm-gradle's network_enforcement_mode = "outer-netns-loopback-only"
    must be LIVE-VERIFIED, in the same envelope, before every jvm-gradle
    capture -- never merely declared in the policy and trusted."""

    def _make_source_artifact_dir(self, tmp_path):
        import hashlib
        import tarfile

        source_artifact_dir = tmp_path / "source-artifact"
        source_artifact_dir.mkdir()
        tar_path = source_artifact_dir / "source.tar"
        src_file = tmp_path / "hello.txt"
        src_file.write_text("hello\n")
        gradlew_file = tmp_path / "gradlew"
        gradlew_file.write_text("#!/bin/sh\n")
        with tarfile.open(tar_path, "w") as tar:
            tar.add(src_file, arcname="hello.txt")
            tar.add(gradlew_file, arcname="gradlew")
        archive_sha256 = hashlib.sha256(tar_path.read_bytes()).hexdigest()
        (source_artifact_dir / "acquisition-receipt.json").write_text(json.dumps({
            "actual_head_sha": "deadbeef" * 5,
            "normalized_archive_sha256": archive_sha256,
            "license_sha256": "cafe" * 16,
        }))
        return source_artifact_dir

    def _run(self, tmp_path, *, probe_verified: bool, gradle_stdout: bytes):
        probe_report = {
            "report_type": "n2d1b-gradle-network-enforcement-probe-v1",
            "network_enforcement_mode": "outer-netns-loopback-only",
            "negative_external_connectivity_probe": {"exit_code": 0 if probe_verified else 1},
            "positive_loopback_bind_connect_probe": {"exit_code": 0},
            "external_connectivity_confirmed_blocked": probe_verified,
            "loopback_bind_connect_confirmed_allowed": True,
            "enforcement_exception_verified": probe_verified,
        }
        source_artifact_dir = self._make_source_artifact_dir(tmp_path)
        work_dir = tmp_path / "work"
        out_dir = tmp_path / "out"
        fake_result = {
            "raw_stdout": gradle_stdout, "raw_stderr": b"", "exit_code": 0,
            "wall_time_s": 1.0, "peak_rss_kb": 1024,
        }
        with mock.patch.object(gc, "network_enforcement_probe") as fake_probe_module, \
                mock.patch.object(gc.capture_build, "run_real_build", return_value=fake_result):
            fake_probe_module.run_gradle_network_enforcement_checks.return_value = probe_report
            return gc.run_one_capture(
                case_id="repo-spotless", ecosystem="jvm-gradle", job_name="capture-a",
                source_artifact_dir=source_artifact_dir, work_dir=work_dir, out_dir=out_dir,
                frozen_argv=["./gradlew", "spotlessCheck"], errata_path=REAL_ERRATA_PATH,
                sandboy_bin=Path("/nonexistent/sandboy"), sandboy_commit_sha="e" * 40,
                toolchain_capture_fn=lambda source_root: {
                    "resolved_version": "9.4.1", "runtime_identifier": "21",
                    "gradle_binary_path": "/usr/bin/true", "gradle_binary_sha256": "a" * 64,
                },
                toolchain_env_values={"JAVA_HOME": "/usr/lib/jvm/java-21"},
                canonical_stream="stdout", primary_stream_rationale="test",
                project_writable_dirs_relative=[],
                requested_version_or_range="9.4.1", resolver_mechanism="test",
            ), out_dir

    def test_verified_enforcement_lets_the_real_capture_proceed(self):
        import tempfile

        with tempfile.TemporaryDirectory() as tmp:
            receipt, out_dir = self._run(
                Path(tmp), probe_verified=True, gradle_stdout=b"BUILD SUCCESSFUL in 3s\n",
            )
            self.assertIsNotNone(receipt["network_enforcement_exception"])
            self.assertTrue(receipt["network_enforcement_exception"]["enforcement_exception_verified"])
            self.assertTrue((out_dir / "network-enforcement-probe-report.json").exists())
            self.assertTrue((out_dir / "receipt.json").exists())

    def test_unverified_enforcement_fails_closed_before_the_real_gradle_run(self):
        import tempfile

        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            with self.assertRaises(gc.GenericCaptureFailure):
                self._run(tmp_path, probe_verified=False, gradle_stdout=b"BUILD SUCCESSFUL in 3s\n")
            out_dir = tmp_path / "out"
            report = json.loads((out_dir / "network-enforcement-probe-report.json").read_text())
            self.assertFalse(report["enforcement_exception_verified"])
            # The real gradle capture must never have been promoted to a receipt.
            self.assertFalse((out_dir / "receipt.json").exists())


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
