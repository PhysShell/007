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
            fake_probe_module.run_network_enforcement_checks.return_value = probe_report
            return gc.run_one_capture(
                # repo-moshi (D1b, 2026-07-16): repo-spotless's own network
                # exception was revoked when the case itself was rejected
                # for an unrelated reason (REJECTED_ACQUISITION_MODEL_
                # INCOMPATIBLE) and repo-moshi separately authorized in its
                # place -- see NETWORK_ENFORCEMENT_AUTHORIZED_CASES.
                case_id="repo-moshi", ecosystem="jvm-gradle", job_name="capture-a",
                source_artifact_dir=source_artifact_dir, work_dir=work_dir, out_dir=out_dir,
                frozen_argv=["./gradlew", "test"], errata_path=REAL_ERRATA_PATH,
                sandboy_bin=Path("/nonexistent/sandboy"), sandboy_commit_sha="e" * 40,
                toolchain_capture_fn=lambda source_root: {
                    "resolved_version": "9.5.1", "runtime_identifier": "21",
                    "gradle_binary_path": "/usr/bin/true", "gradle_binary_sha256": "a" * 64,
                },
                toolchain_env_values={"JAVA_HOME": "/usr/lib/jvm/java-21"},
                canonical_stream="stdout", primary_stream_rationale="test",
                project_writable_dirs_relative=[],
                requested_version_or_range="9.5.1", resolver_mechanism="test",
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

    def test_receipt_binds_case_scoped_authorization_and_probe_report(self):
        import tempfile

        with tempfile.TemporaryDirectory() as tmp:
            receipt, _out_dir = self._run(
                Path(tmp), probe_verified=True, gradle_stdout=b"BUILD SUCCESSFUL in 3s\n",
            )
            exc = receipt["network_enforcement_exception"]
            self.assertEqual(exc["network_enforcement_mode"], "outer-netns-loopback-only")
            self.assertIn("negative_external_connectivity_probe", exc)
            self.assertIn("positive_loopback_bind_connect_probe", exc)

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


class TestKubeopsGeneratorNetworkEnforcementWiring(unittest.TestCase):
    """D1b authorization (2026-07-16): repo-kubeops-generator's VSTest
    test-host hit the identical class of loopback-bind failure as Gradle's
    daemon -- network_enforcement_mode is authorized for this EXACT case_id
    (gsp.NETWORK_ENFORCEMENT_AUTHORIZED_CASES), not the whole dotnet
    ecosystem, and must be LIVE-VERIFIED before the real capture, exactly
    like repo-spotless."""

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

    def _run(self, tmp_path, *, case_id: str, probe_verified: bool, dotnet_stdout: bytes):
        probe_report = {
            "report_type": "n2d1b-network-enforcement-probe-v1",
            "authorized_case_id": case_id,
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
            "raw_stdout": dotnet_stdout, "raw_stderr": b"", "exit_code": 0,
            "wall_time_s": 1.0, "peak_rss_kb": 1024,
        }
        with mock.patch.object(gc, "network_enforcement_probe") as fake_probe_module, \
                mock.patch.object(gc.capture_build, "run_real_build", return_value=fake_result):
            fake_probe_module.run_network_enforcement_checks.return_value = probe_report
            receipt = gc.run_one_capture(
                case_id=case_id, ecosystem="dotnet", job_name="capture-a",
                source_artifact_dir=source_artifact_dir, work_dir=work_dir, out_dir=out_dir,
                frozen_argv=["dotnet", "test", "test/KubeOps.Generator.Test/KubeOps.Generator.Test.csproj"],
                errata_path=REAL_ERRATA_PATH,
                sandboy_bin=Path("/nonexistent/sandboy"), sandboy_commit_sha="e" * 40,
                toolchain_capture_fn=lambda source_root: {
                    "resolved_version": "10.0.100", "runtime_identifier": "linux-x64",
                    "rustc_binary_path": "/usr/bin/true", "rustc_binary_sha256": "a" * 64,
                },
                toolchain_env_values={"DOTNET_ROOT": "/usr/share/dotnet"},
                canonical_stream="stdout", primary_stream_rationale="test",
                project_writable_dirs_relative=[],
                requested_version_or_range="10.0.x", resolver_mechanism="test",
            )
            return receipt, out_dir, fake_probe_module

    def test_repo_kubeops_generator_receives_the_exception_and_is_live_verified(self):
        import tempfile

        with tempfile.TemporaryDirectory() as tmp:
            receipt, out_dir, _ = self._run(
                Path(tmp), case_id="repo-kubeops-generator", probe_verified=True,
                dotnet_stdout=b"Passed!  - Failed: 0, Passed: 3, Skipped: 0, Total: 3\n",
            )
            self.assertIsNotNone(receipt["network_enforcement_exception"])
            self.assertEqual(receipt["network_enforcement_exception"]["authorized_case_id"], "repo-kubeops-generator")
            self.assertTrue(receipt["network_enforcement_exception"]["enforcement_exception_verified"])
            self.assertTrue((out_dir / "network-enforcement-probe-report.json").exists())
            self.assertTrue((out_dir / "receipt.json").exists())

    def test_failed_probe_stops_the_capture_before_the_real_workload_runs(self):
        import tempfile

        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            with self.assertRaises(gc.GenericCaptureFailure):
                self._run(
                    tmp_path, case_id="repo-kubeops-generator", probe_verified=False,
                    dotnet_stdout=b"Passed!  - Failed: 0, Passed: 3, Skipped: 0, Total: 3\n",
                )
            out_dir = tmp_path / "out"
            report = json.loads((out_dir / "network-enforcement-probe-report.json").read_text())
            self.assertFalse(report["enforcement_exception_verified"])
            self.assertFalse((out_dir / "receipt.json").exists())

    def test_an_unauthorized_dotnet_case_never_invokes_the_probe_module(self):
        # A dotnet case OTHER than repo-kubeops-generator must never even
        # call into network_enforcement_probe -- the gate is
        # `case_id in NETWORK_ENFORCEMENT_AUTHORIZED_CASES`, checked before
        # any probe invocation. This case_id has no registered
        # content-acceptance semantic validator, so the capture still fails
        # overall (an unrelated, expected reason) -- what matters here is
        # that the probe module itself was never called before that point.
        import tempfile

        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            source_artifact_dir = self._make_source_artifact_dir(tmp_path)
            work_dir = tmp_path / "work"
            out_dir = tmp_path / "out"
            fake_result = {
                "raw_stdout": b"Passed!  - Failed: 0, Passed: 3, Skipped: 0, Total: 3\n", "raw_stderr": b"",
                "exit_code": 0, "wall_time_s": 1.0, "peak_rss_kb": 1024,
            }
            with mock.patch.object(gc, "network_enforcement_probe") as fake_probe_module, \
                    mock.patch.object(gc.capture_build, "run_real_build", return_value=fake_result):
                with self.assertRaises(gc.GenericCaptureFailure):
                    gc.run_one_capture(
                        case_id="repo-some-other-dotnet-case", ecosystem="dotnet", job_name="capture-a",
                        source_artifact_dir=source_artifact_dir, work_dir=work_dir, out_dir=out_dir,
                        frozen_argv=["dotnet", "test"], errata_path=REAL_ERRATA_PATH,
                        sandboy_bin=Path("/nonexistent/sandboy"), sandboy_commit_sha="e" * 40,
                        toolchain_capture_fn=lambda source_root: {
                            "resolved_version": "10.0.100", "runtime_identifier": "linux-x64",
                            "rustc_binary_path": "/usr/bin/true", "rustc_binary_sha256": "a" * 64,
                        },
                        toolchain_env_values={"DOTNET_ROOT": "/usr/share/dotnet"},
                        canonical_stream="stdout", primary_stream_rationale="test",
                        project_writable_dirs_relative=[],
                        requested_version_or_range="10.0.x", resolver_mechanism="test",
                    )
                fake_probe_module.run_network_enforcement_checks.assert_not_called()
            self.assertFalse((out_dir / "network-enforcement-probe-report.json").exists())

    def test_receipt_binds_exception_mode_case_identity_and_full_probe_report(self):
        import tempfile

        with tempfile.TemporaryDirectory() as tmp:
            receipt, _, _ = self._run(
                Path(tmp), case_id="repo-kubeops-generator", probe_verified=True,
                dotnet_stdout=b"Passed!  - Failed: 0, Passed: 3, Skipped: 0, Total: 3\n",
            )
            exc = receipt["network_enforcement_exception"]
            self.assertEqual(exc["network_enforcement_mode"], "outer-netns-loopback-only")
            self.assertEqual(exc["authorized_case_id"], "repo-kubeops-generator")
            self.assertIn("negative_external_connectivity_probe", exc)
            self.assertIn("positive_loopback_bind_connect_probe", exc)


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


class TestMavenCanonicalizationWiring(unittest.TestCase):
    """repo-docker-java-parser's canonical benchmark input must be the
    CANONICALIZED stream (case-specific-deterministic-canonicalization), not
    the raw stream verbatim -- while raw.stdout/raw.stderr and content
    acceptance must still see the real, untouched raw bytes."""

    ESC = "\x1b"

    def _maven_stdout(self, *, buildnumber_ts: str, compile_s: list[str], elapsed_s: str,
                       total_s: str, finished_at: str) -> bytes:
        esc = self.ESC
        lines = [
            f"[{esc}[1;34mINFO{esc}[m] Scanning for projects...",
            f"[{esc}[1;34mINFO{esc}[m] BUILD SUCCESS",
            f"[{esc}[1;34mINFO{esc}[m] Storing buildNumber: null at timestamp: {buildnumber_ts}",
        ]
        for s in compile_s:
            lines.append(f"[{esc}[1;34mINFO{esc}[m] compile in {s} s")
        lines.append(
            f"[{esc}[1;34mINFO{esc}[m] {esc}[1;32mTests run: {esc}[0;1;32m3{esc}[m, Failures: 0, "
            f"Errors: 0, Skipped: 0, Time elapsed: {elapsed_s} s - in com.example.FooTest"
        )
        lines.append(f"[{esc}[1;34mINFO{esc}[m] Total time:  {total_s} s")
        lines.append(f"[{esc}[1;34mINFO{esc}[m] Finished at: {finished_at}")
        return ("\n".join(lines) + "\n").encode("utf-8")

    def _run(self, tmp_path, *, job_name: str, stdout: bytes):
        import unittest.mock as mock

        source_artifact_dir = self._make_source_artifact_dir(tmp_path)
        work_dir = tmp_path / f"work-{job_name}"
        out_dir = tmp_path / f"out-{job_name}"
        fake_result = {
            "raw_stdout": stdout, "raw_stderr": b"", "exit_code": 0,
            "wall_time_s": 1.0, "peak_rss_kb": 1024,
        }
        with mock.patch.object(gc.capture_build, "run_real_build", return_value=fake_result):
            receipt = gc.run_one_capture(
                case_id="repo-docker-java-parser", ecosystem="jvm-maven", job_name=job_name,
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
        return receipt, out_dir

    def _make_source_artifact_dir(self, tmp_path):
        import hashlib
        import tarfile

        source_artifact_dir = tmp_path / "source-artifact"
        source_artifact_dir.mkdir(exist_ok=True)
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

    def test_canonical_benchmark_input_is_the_canonicalized_stream(self):
        import tempfile

        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            stdout = self._maven_stdout(
                buildnumber_ts="1784136905453", compile_s=["11.4", "3.3"], elapsed_s="0.14",
                total_s="18.865", finished_at="2026-07-15T17:35:22Z",
            )
            receipt, out_dir = self._run(tmp_path, job_name="capture-a", stdout=stdout)
            self.assertEqual(receipt["canonical_input_derivation"], "case-specific-deterministic-canonicalization")
            self.assertIsNotNone(receipt["canonicalization_policy_sha256"])
            self.assertIsNotNone(receipt["canonicalization_report_sha256"])
            self.assertTrue((out_dir / "canonicalization-report.json").exists())
            canonical_bytes = (out_dir / "canonical-raw-input.bin").read_bytes()
            self.assertNotIn(b"1784136905453", canonical_bytes)
            self.assertIn(b"<TIMESTAMP>", canonical_bytes)
            self.assertIn(b"<ELAPSED>", canonical_bytes)
            # raw.stdout must remain the real, untouched raw evidence.
            self.assertEqual((out_dir / "raw.stdout").read_bytes(), stdout)
            self.assertIn(b"1784136905453", (out_dir / "raw.stdout").read_bytes())

    def test_capture_a_and_capture_b_canonicalize_to_identical_bytes(self):
        import tempfile

        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            stdout_a = self._maven_stdout(
                buildnumber_ts="1784136905453", compile_s=["11.4", "3.3"], elapsed_s="0.14",
                total_s="18.865", finished_at="2026-07-15T17:35:22Z",
            )
            stdout_b = self._maven_stdout(
                buildnumber_ts="1784136890043", compile_s=["10.5", "3.1"], elapsed_s="0.134",
                total_s="17.597", finished_at="2026-07-15T17:35:06Z",
            )
            self.assertNotEqual(stdout_a, stdout_b)
            _, out_dir_a = self._run(tmp_path, job_name="capture-a", stdout=stdout_a)
            _, out_dir_b = self._run(tmp_path, job_name="capture-b", stdout=stdout_b)
            canonical_a = (out_dir_a / "canonical-raw-input.bin").read_bytes()
            canonical_b = (out_dir_b / "canonical-raw-input.bin").read_bytes()
            self.assertEqual(canonical_a, canonical_b)

    def test_non_maven_case_is_entirely_unaffected(self):
        import tempfile
        import unittest.mock as mock

        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            source_artifact_dir = self._make_source_artifact_dir(tmp_path)
            work_dir = tmp_path / "work"
            out_dir = tmp_path / "out"
            raw = b"running 3 tests\ntest result: ok. 3 passed; 0 failed; 0 ignored\n"
            fake_result = {"raw_stdout": raw, "raw_stderr": b"", "exit_code": 0, "wall_time_s": 1.0, "peak_rss_kb": 1024}
            with mock.patch.object(gc.capture_build, "run_real_build", return_value=fake_result):
                receipt = gc.run_one_capture(
                    case_id="repo-rustlings", ecosystem="rust", job_name="capture-a",
                    source_artifact_dir=source_artifact_dir, work_dir=work_dir, out_dir=out_dir,
                    frozen_argv=["cargo", "test"], errata_path=REAL_ERRATA_PATH,
                    sandboy_bin=Path("/nonexistent/sandboy"), sandboy_commit_sha="e" * 40,
                    toolchain_capture_fn=lambda source_root: {
                        "resolved_version": "1.97.0", "runtime_identifier": "x86_64-unknown-linux-gnu",
                        "rustc_binary_path": "/usr/bin/true", "rustc_binary_sha256": "a" * 64,
                    },
                    toolchain_env_values={"CARGO_HOME": str(tmp_path / "cargo-home")},
                    canonical_stream="stdout", primary_stream_rationale="test",
                    project_writable_dirs_relative=[],
                    requested_version_or_range="stable", resolver_mechanism="test",
                )
            self.assertEqual(receipt["canonical_input_derivation"], "raw-capped-stream")
            self.assertIsNone(receipt["canonicalization_policy_sha256"])
            self.assertIsNone(receipt["canonicalization_report_sha256"])
            self.assertEqual(receipt["canonicalization_transformations"], [])
            self.assertFalse((out_dir / "canonicalization-report.json").exists())
            self.assertEqual((out_dir / "canonical-raw-input.bin").read_bytes(), raw)


class TestVstestCanonicalizationWiring(unittest.TestCase):
    """repo-kubeops-generator's canonical benchmark input must be the
    VSTEST-canonicalized stream, dispatched to vstest_canonicalizer.py (NOT
    maven_canonicalizer.py) -- while raw.stdout/raw.stderr and content
    acceptance must still see the real, untouched raw bytes. D1b
    authorization (2026-07-16), evidence: CI run 29466573023."""

    def _vstest_stdout(self, *, duration: str) -> bytes:
        lines = [
            "Test run for /work/KubeOps.Generator.Test.dll (net10.0)",
            "",
            (
                f"Passed!  - Failed:     0, Passed:    61, Skipped:     0, Total:    61, "
                f"Duration: {duration} s - KubeOps.Generator.Test.dll (net10.0)"
            ),
        ]
        return ("\n".join(lines) + "\n").encode("utf-8")

    def _make_source_artifact_dir(self, tmp_path):
        import hashlib
        import tarfile

        source_artifact_dir = tmp_path / "source-artifact"
        source_artifact_dir.mkdir(exist_ok=True)
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

    def _run(self, tmp_path, *, job_name: str, stdout: bytes, source_artifact_dir=None):
        probe_report = {
            "report_type": "n2d1b-network-enforcement-probe-v1",
            "authorized_case_id": "repo-kubeops-generator",
            "network_enforcement_mode": "outer-netns-loopback-only",
            "negative_external_connectivity_probe": {"exit_code": 0},
            "positive_loopback_bind_connect_probe": {"exit_code": 0},
            "external_connectivity_confirmed_blocked": True,
            "loopback_bind_connect_confirmed_allowed": True,
            "enforcement_exception_verified": True,
        }
        if source_artifact_dir is None:
            source_artifact_dir = self._make_source_artifact_dir(tmp_path)
        work_dir = tmp_path / f"work-{job_name}"
        out_dir = tmp_path / f"out-{job_name}"
        fake_result = {"raw_stdout": stdout, "raw_stderr": b"", "exit_code": 0, "wall_time_s": 1.0, "peak_rss_kb": 1024}
        with mock.patch.object(gc, "network_enforcement_probe") as fake_probe_module, \
                mock.patch.object(gc.capture_build, "run_real_build", return_value=fake_result):
            fake_probe_module.run_network_enforcement_checks.return_value = probe_report
            receipt = gc.run_one_capture(
                case_id="repo-kubeops-generator", ecosystem="dotnet", job_name=job_name,
                source_artifact_dir=source_artifact_dir, work_dir=work_dir, out_dir=out_dir,
                frozen_argv=["dotnet", "test", "test/KubeOps.Generator.Test/KubeOps.Generator.Test.csproj"],
                errata_path=REAL_ERRATA_PATH,
                sandboy_bin=Path("/nonexistent/sandboy"), sandboy_commit_sha="e" * 40,
                toolchain_capture_fn=lambda source_root: {
                    "resolved_version": "10.0.100", "runtime_identifier": "linux-x64",
                    "rustc_binary_path": "/usr/bin/true", "rustc_binary_sha256": "a" * 64,
                },
                toolchain_env_values={"DOTNET_ROOT": "/usr/share/dotnet"},
                canonical_stream="stdout", primary_stream_rationale="test",
                project_writable_dirs_relative=[],
                requested_version_or_range="10.0.x", resolver_mechanism="test",
            )
        return receipt, out_dir

    def test_canonical_benchmark_input_is_the_vstest_canonicalized_stream(self):
        import tempfile

        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            stdout = self._vstest_stdout(duration="2")
            receipt, out_dir = self._run(tmp_path, job_name="capture-a", stdout=stdout)
            self.assertEqual(receipt["canonical_input_derivation"], "case-specific-deterministic-canonicalization")
            self.assertIsNotNone(receipt["canonicalization_policy_sha256"])
            self.assertIsNotNone(receipt["canonicalization_report_sha256"])
            self.assertTrue((out_dir / "canonicalization-report.json").exists())
            canonical_bytes = (out_dir / "canonical-raw-input.bin").read_bytes()
            self.assertNotIn(b"Duration: 2 s", canonical_bytes)
            self.assertIn(b"<ELAPSED>", canonical_bytes)
            # raw.stdout must remain the real, untouched raw evidence.
            self.assertEqual((out_dir / "raw.stdout").read_bytes(), stdout)
            self.assertIn(b"Duration: 2 s", (out_dir / "raw.stdout").read_bytes())

    def test_capture_a_and_capture_b_canonicalize_to_identical_bytes(self):
        import tempfile

        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            stdout_a = self._vstest_stdout(duration="2")
            stdout_b = self._vstest_stdout(duration="1")
            self.assertNotEqual(stdout_a, stdout_b)
            shared_source = self._make_source_artifact_dir(tmp_path)
            _, out_dir_a = self._run(tmp_path, job_name="capture-a", stdout=stdout_a,
                                      source_artifact_dir=shared_source)
            _, out_dir_b = self._run(tmp_path, job_name="capture-b", stdout=stdout_b,
                                      source_artifact_dir=shared_source)
            canonical_a = (out_dir_a / "canonical-raw-input.bin").read_bytes()
            canonical_b = (out_dir_b / "canonical-raw-input.bin").read_bytes()
            self.assertEqual(canonical_a, canonical_b)

    def test_uses_vstest_module_not_maven_module(self):
        self.assertIs(gc.CANONICALIZATION_MODULE_BY_CASE_ID["repo-kubeops-generator"], gc.vstest_canonicalizer)
        self.assertIs(
            gc.CANONICALIZATION_MODULE_BY_CASE_ID["repo-docker-java-parser"], gc.maven_canonicalizer
        )

    def test_a_case_id_cannot_appear_in_more_than_one_profile(self):
        all_ids = [
            cid for policy, _module in gc._CANONICALIZATION_PROFILES for cid in policy["applicable_case_ids"]
        ]
        self.assertEqual(len(all_ids), len(set(all_ids)))


if __name__ == "__main__":
    unittest.main()
