"""Unit tests for verify_pilot_pair_reproducibility.py.

Uses REAL gc.run_one_capture() calls (mocking only capture_build.
run_real_build, same discipline as test_generic_capture.py) to produce
genuinely schema-valid, fully-populated receipts and canonical files --
then tampers with specific files/fields to exercise each failure mode
against that real baseline, rather than hand-rolling ad hoc receipt dicts
that would trivially fail schema validation for unrelated reasons.
"""
import json
import sys
import tempfile
import unittest
import unittest.mock as mock
from pathlib import Path

TOOLS = Path(__file__).resolve().parents[1] / "tools"
REPO_ROOT = Path(__file__).resolve().parents[7]
sys.path.insert(0, str(TOOLS))
import generic_capture as gc  # noqa: E402
import maven_canonicalizer as mc  # noqa: E402
import verify_pilot_pair_reproducibility as verifier  # noqa: E402

REAL_ERRATA_PATH = REPO_ROOT / "qodec/evals/interop/v2/n2/d1-identity-lock/execution-plan-errata.json"
ESC = "\x1b"


def _maven_stdout(*, buildnumber_ts, compile_s, elapsed_s, total_s, finished_at) -> bytes:
    lines = [
        f"[{ESC}[1;34mINFO{ESC}[m] BUILD SUCCESS",
        f"[{ESC}[1;34mINFO{ESC}[m] Storing buildNumber: null at timestamp: {buildnumber_ts}",
    ]
    for s in compile_s:
        lines.append(f"[{ESC}[1;34mINFO{ESC}[m] compile in {s} s")
    lines.append(
        f"[{ESC}[1;34mINFO{ESC}[m] {ESC}[1;32mTests run: {ESC}[0;1;32m3{ESC}[m, Failures: 0, "
        f"Errors: 0, Skipped: 0, Time elapsed: {elapsed_s} s - in com.example.FooTest"
    )
    lines.append(f"[{ESC}[1;34mINFO{ESC}[m] Total time:  {total_s} s")
    lines.append(f"[{ESC}[1;34mINFO{ESC}[m] Finished at: {finished_at}")
    return ("\n".join(lines) + "\n").encode("utf-8")


def _make_source_artifact_dir(tmp_path):
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


def _run_real_maven_capture(tmp_path, *, job_name: str, stdout: bytes, source_artifact_dir=None) -> Path:
    # Real capture-a/capture-b jobs each download the SAME published
    # release-asset bytes (source_identity is guaranteed identical by
    # construction) -- share one source_artifact_dir across a test's pair
    # rather than regenerating source.tar per call, which would otherwise
    # embed a real, differing tar mtime and make archive_sha256 spuriously
    # differ for reasons that have nothing to do with the code under test.
    if source_artifact_dir is None:
        source_artifact_dir = _make_source_artifact_dir(tmp_path)
    work_dir = tmp_path / f"work-{job_name}"
    out_dir = tmp_path / f"out-{job_name}"
    fake_result = {"raw_stdout": stdout, "raw_stderr": b"", "exit_code": 0, "wall_time_s": 1.0, "peak_rss_kb": 1024}
    with mock.patch.object(gc.capture_build, "run_real_build", return_value=fake_result):
        gc.run_one_capture(
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
    return out_dir


def _run_real_rust_capture(tmp_path, *, job_name: str, stdout: bytes, source_artifact_dir=None) -> Path:
    if source_artifact_dir is None:
        source_artifact_dir = _make_source_artifact_dir(tmp_path)
    work_dir = tmp_path / f"work-{job_name}"
    out_dir = tmp_path / f"out-{job_name}"
    fake_result = {"raw_stdout": stdout, "raw_stderr": b"", "exit_code": 0, "wall_time_s": 1.0, "peak_rss_kb": 1024}
    with mock.patch.object(gc.capture_build, "run_real_build", return_value=fake_result):
        gc.run_one_capture(
            case_id="repo-rustlings", ecosystem="rust", job_name=job_name,
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
    return out_dir


class TestVerifyOneCaptureHappyPath(unittest.TestCase):
    def test_a_genuine_capture_passes_every_check(self):
        with tempfile.TemporaryDirectory() as tmp:
            stdout = _maven_stdout(
                buildnumber_ts="1784136905453", compile_s=["11.4", "3.3"], elapsed_s="0.14",
                total_s="18.865", finished_at="2026-07-15T17:35:22Z",
            )
            out_dir = _run_real_maven_capture(Path(tmp), job_name="capture-a", stdout=stdout)
            result = verifier.verify_one_capture(out_dir)
            self.assertTrue(result["valid"], result["checks"])
            self.assertTrue(all(c["passed"] for c in result["checks"]))


class TestVerifyPairHappyPath(unittest.TestCase):
    def test_real_evidence_shaped_pair_passes(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            stdout_a = _maven_stdout(
                buildnumber_ts="1784136905453", compile_s=["11.4", "3.3"], elapsed_s="0.14",
                total_s="18.865", finished_at="2026-07-15T17:35:22Z",
            )
            stdout_b = _maven_stdout(
                buildnumber_ts="1784136890043", compile_s=["10.5", "3.1"], elapsed_s="0.134",
                total_s="17.597", finished_at="2026-07-15T17:35:06Z",
            )
            shared_source = _make_source_artifact_dir(tmp_path)
            dir_a = _run_real_maven_capture(tmp_path, job_name="capture-a", stdout=stdout_a,
                                             source_artifact_dir=shared_source)
            dir_b = _run_real_maven_capture(tmp_path, job_name="capture-b", stdout=stdout_b,
                                             source_artifact_dir=shared_source)
            report = verifier.verify_pair(dir_a, dir_b)
            self.assertTrue(report["passed"], report)
            self.assertTrue(report["capture_a_verification"]["valid"])
            self.assertTrue(report["capture_b_verification"]["valid"])
            self.assertTrue(report["canonical_bytes_equal"])
            self.assertEqual(report["unmatched_raw_diff_lines"], [])
            self.assertEqual(report["canonical_bounded_diff"], "")
            self.assertEqual(report["identity_mismatches"], [])

    def test_non_canonicalized_case_requires_exact_raw_equality(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            raw = b"running 3 tests\ntest result: ok. 3 passed; 0 failed; 0 ignored\n"
            shared_source = _make_source_artifact_dir(tmp_path)
            dir_a = _run_real_rust_capture(tmp_path, job_name="capture-a", stdout=raw,
                                            source_artifact_dir=shared_source)
            dir_b = _run_real_rust_capture(tmp_path, job_name="capture-b", stdout=raw,
                                            source_artifact_dir=shared_source)
            report = verifier.verify_pair(dir_a, dir_b)
            self.assertTrue(report["passed"], report)


class TestVerifyPairFailureModes(unittest.TestCase):
    def _pair(self, tmp_path):
        stdout_a = _maven_stdout(
            buildnumber_ts="1784136905453", compile_s=["11.4"], elapsed_s="0.14",
            total_s="18.865", finished_at="2026-07-15T17:35:22Z",
        )
        stdout_b = _maven_stdout(
            buildnumber_ts="1784136890043", compile_s=["10.5"], elapsed_s="0.134",
            total_s="17.597", finished_at="2026-07-15T17:35:06Z",
        )
        shared_source = _make_source_artifact_dir(tmp_path)
        dir_a = _run_real_maven_capture(tmp_path, job_name="capture-a", stdout=stdout_a,
                                         source_artifact_dir=shared_source)
        dir_b = _run_real_maven_capture(tmp_path, job_name="capture-b", stdout=stdout_b,
                                         source_artifact_dir=shared_source)
        return dir_a, dir_b

    def test_missing_receipt_fails_closed(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            dir_a, dir_b = self._pair(tmp_path)
            (dir_a / "receipt.json").unlink()
            report = verifier.verify_pair(dir_a, dir_b)
            self.assertFalse(report["passed"])
            self.assertFalse(report["capture_a_verification"]["valid"])

    def test_tampered_canonical_bytes_that_no_longer_derive_from_raw_fails(self):
        # The exact hole flagged in review: two identical-looking canonical
        # stub files that were never actually produced from the real raw
        # bytes must NOT pass just because they equal each other.
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            dir_a, dir_b = self._pair(tmp_path)
            fake_stub = b"totally fabricated canonical content, not derived from raw\n"
            (dir_a / "canonical-raw-input.bin").write_bytes(fake_stub)
            (dir_b / "canonical-raw-input.bin").write_bytes(fake_stub)
            report = verifier.verify_pair(dir_a, dir_b)
            self.assertFalse(report["passed"])
            self.assertFalse(report["capture_a_verification"]["valid"])
            self.assertFalse(report["capture_b_verification"]["valid"])
            names_a = {c["name"]: c["passed"] for c in report["capture_a_verification"]["checks"]}
            self.assertFalse(names_a["canonical_bytes_are_rederived_from_raw"])

    def test_tampered_receipt_canonical_hash_fails(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            dir_a, dir_b = self._pair(tmp_path)
            receipt = json.loads((dir_a / "receipt.json").read_text())
            receipt["canonical_raw_input_sha256"] = "0" * 64
            (dir_a / "receipt.json").write_text(json.dumps(receipt))
            report = verifier.verify_pair(dir_a, dir_b)
            self.assertFalse(report["passed"])
            self.assertFalse(report["capture_a_verification"]["valid"])

    def test_tampered_policy_file_fails_closed(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            dir_a, dir_b = self._pair(tmp_path)
            tampered = json.loads(verifier.POLICY_PATH.read_text())
            tampered["rules"][0]["anchored_regex"] = "garbage"
            tampered_path = tmp_path / "tampered-policy.json"
            tampered_path.write_text(json.dumps(tampered))
            with mock.patch.object(verifier, "POLICY_PATH", tampered_path):
                report = verifier.verify_pair(dir_a, dir_b)
            self.assertFalse(report["passed"])
            names_a = {c["name"]: c["passed"] for c in report["capture_a_verification"]["checks"]}
            self.assertFalse(names_a["canonicalization_policy_integrity"])

    def test_identity_mismatch_fails(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            dir_a, dir_b = self._pair(tmp_path)
            receipt = json.loads((dir_b / "receipt.json").read_text())
            receipt["effective_execution_argv"] = ["mvn", "test", "-o"]
            (dir_b / "receipt.json").write_text(json.dumps(receipt))
            report = verifier.verify_pair(dir_a, dir_b)
            self.assertFalse(report["passed"])
            self.assertIn("effective_execution_argv", report["identity_mismatches"])

    def test_unmatched_raw_difference_not_covered_by_either_report_fails(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            stdout_a = _maven_stdout(
                buildnumber_ts="1784136905453", compile_s=["11.4"], elapsed_s="0.14",
                total_s="18.865", finished_at="2026-07-15T17:35:22Z",
            ).replace(b"BUILD SUCCESS", b"BUILD SUCCESS extra-a")
            stdout_b = _maven_stdout(
                buildnumber_ts="1784136890043", compile_s=["10.5"], elapsed_s="0.134",
                total_s="17.597", finished_at="2026-07-15T17:35:06Z",
            ).replace(b"BUILD SUCCESS", b"BUILD SUCCESS extra-b")
            shared_source = _make_source_artifact_dir(tmp_path)
            dir_a = _run_real_maven_capture(tmp_path, job_name="capture-a", stdout=stdout_a,
                                             source_artifact_dir=shared_source)
            dir_b = _run_real_maven_capture(tmp_path, job_name="capture-b", stdout=stdout_b,
                                             source_artifact_dir=shared_source)
            report = verifier.verify_pair(dir_a, dir_b)
            self.assertFalse(report["passed"])
            self.assertNotEqual(report["unmatched_raw_diff_lines"], [])


class TestBoundedLineDiff(unittest.TestCase):
    def test_small_diff_is_not_truncated(self):
        diff, truncated = verifier.bounded_line_diff("a\nb\n", "a\nc\n")
        self.assertFalse(truncated)
        self.assertIn("-b", diff)
        self.assertIn("+c", diff)

    def test_large_diff_is_truncated_with_flag_set(self):
        a_text = "\n".join(f"line{i}" for i in range(1000)) + "\n"
        b_text = "\n".join(f"different{i}" for i in range(1000)) + "\n"
        diff, truncated = verifier.bounded_line_diff(a_text, b_text, max_diff_lines=50)
        self.assertTrue(truncated)
        self.assertLessEqual(len(diff.splitlines()), 50)


class TestUncoveredLines(unittest.TestCase):
    def test_covered_lines_on_both_sides_are_not_reported(self):
        text_a = "x\nStoring buildNumber: null at timestamp: 111\ny\n"
        text_b = "x\nStoring buildNumber: null at timestamp: 222\ny\n"
        report_a = {"replacements": [{"before_line_sha256": verifier._sha256(
            "Storing buildNumber: null at timestamp: 111".encode())}]}
        report_b = {"replacements": [{"before_line_sha256": verifier._sha256(
            "Storing buildNumber: null at timestamp: 222".encode())}]}
        covered_a = verifier._line_hashes_from_report(report_a)
        covered_b = verifier._line_hashes_from_report(report_b)
        uncovered = verifier._uncovered_lines(text_a, text_b, covered_a, covered_b)
        self.assertEqual(uncovered, [])

    def test_a_side_covered_but_b_side_not_still_reports_b(self):
        text_a = "x\nStoring buildNumber: null at timestamp: 111\ny\n"
        text_b = "x\nStoring buildNumber: null at timestamp: 222\ny\n"
        report_a = {"replacements": [{"before_line_sha256": verifier._sha256(
            "Storing buildNumber: null at timestamp: 111".encode())}]}
        covered_a = verifier._line_hashes_from_report(report_a)
        uncovered = verifier._uncovered_lines(text_a, text_b, covered_a, set())
        self.assertEqual(len(uncovered), 1)
        self.assertEqual(uncovered[0]["side"], "b")

    def test_uncovered_lines_on_both_sides_are_reported(self):
        text_a = "x\nunexplained-a\ny\n"
        text_b = "x\nunexplained-b\ny\n"
        uncovered = verifier._uncovered_lines(text_a, text_b, set(), set())
        self.assertEqual(len(uncovered), 2)
        self.assertEqual({u["side"] for u in uncovered}, {"a", "b"})


class TestIdempotence(unittest.TestCase):
    def test_non_canonicalized_derivation_is_vacuously_idempotent(self):
        self.assertTrue(verifier._is_idempotent("raw-capped-stream", b"anything"))

    def test_canonicalized_derivation_checks_real_idempotence(self):
        raw = _maven_stdout(
            buildnumber_ts="123", compile_s=["1.0"], elapsed_s="0.1", total_s="1.0",
            finished_at="2026-07-15T17:35:22Z",
        )
        canonical, _ = mc.canonicalize_stream(raw)
        self.assertTrue(verifier._is_idempotent("case-specific-deterministic-canonicalization", canonical))


if __name__ == "__main__":
    unittest.main()
