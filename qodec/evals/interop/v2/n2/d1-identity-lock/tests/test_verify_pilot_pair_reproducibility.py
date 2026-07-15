"""Unit tests for verify_pilot_pair_reproducibility.py."""
import json
import sys
import tempfile
import unittest
from pathlib import Path

TOOLS = Path(__file__).resolve().parents[1] / "tools"
sys.path.insert(0, str(TOOLS))
import maven_canonicalizer as mc  # noqa: E402
import verify_pilot_pair_reproducibility as verifier  # noqa: E402

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


def _write_maven_capture(out_dir: Path, *, raw_stdout: bytes, extra_receipt_fields=None):
    out_dir.mkdir(parents=True, exist_ok=True)
    canonical_bytes, canon_report = mc.canonicalize_stream(raw_stdout)
    (out_dir / "raw.stdout").write_bytes(raw_stdout)
    (out_dir / "canonical-raw-input.bin").write_bytes(canonical_bytes)
    (out_dir / "canonicalization-report.json").write_text(json.dumps(canon_report, indent=2, sort_keys=True))
    receipt = {
        "case_id": "repo-docker-java-parser",
        "source_identity": {"commit_sha": "a" * 40, "archive_sha256": "b" * 64},
        "toolchain_resolved": {"resolved_version": "3.8.7"},
        "toolchain_executed": {"executed_binary_sha256": "c" * 64},
        "effective_execution_argv": ["mvn", "test"],
        "sandbox_identity": {"sandboy_commit_sha": "d" * 40, "policy_sha256": "e" * 64},
        "canonical_stream": "stdout",
        "canonicalization_policy_sha256": "f" * 64,
        "canonicalization_report_sha256": "1" * 64,
        "canonical_input_derivation": "case-specific-deterministic-canonicalization",
    }
    if extra_receipt_fields:
        receipt.update(extra_receipt_fields)
    (out_dir / "receipt.json").write_text(json.dumps(receipt, indent=2, sort_keys=True))


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
            dir_a, dir_b = tmp_path / "a", tmp_path / "b"
            _write_maven_capture(dir_a, raw_stdout=stdout_a)
            _write_maven_capture(dir_b, raw_stdout=stdout_b)
            report = verifier.verify_pair(dir_a, dir_b)
            self.assertTrue(report["passed"], report)
            self.assertTrue(report["canonical_bytes_equal"])
            self.assertTrue(report["idempotent_a"])
            self.assertTrue(report["idempotent_b"])
            self.assertEqual(report["unmatched_raw_diff_lines"], [])
            self.assertEqual(report["canonical_bounded_diff"], "")
            self.assertEqual(report["identity_mismatches"], [])


class TestVerifyPairFailureModes(unittest.TestCase):
    def _base_pair(self, tmp_path):
        stdout_a = _maven_stdout(
            buildnumber_ts="1784136905453", compile_s=["11.4"], elapsed_s="0.14",
            total_s="18.865", finished_at="2026-07-15T17:35:22Z",
        )
        stdout_b = _maven_stdout(
            buildnumber_ts="1784136890043", compile_s=["10.5"], elapsed_s="0.134",
            total_s="17.597", finished_at="2026-07-15T17:35:06Z",
        )
        dir_a, dir_b = tmp_path / "a", tmp_path / "b"
        return dir_a, dir_b, stdout_a, stdout_b

    def test_identity_mismatch_fails(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            dir_a, dir_b, stdout_a, stdout_b = self._base_pair(tmp_path)
            _write_maven_capture(dir_a, raw_stdout=stdout_a)
            _write_maven_capture(dir_b, raw_stdout=stdout_b, extra_receipt_fields={
                "effective_execution_argv": ["mvn", "test", "-o"],
            })
            report = verifier.verify_pair(dir_a, dir_b)
            self.assertFalse(report["passed"])
            self.assertIn("effective_execution_argv", report["identity_mismatches"])

    def test_non_canonical_bytes_equal_fails(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            dir_a, dir_b, stdout_a, stdout_b = self._base_pair(tmp_path)
            _write_maven_capture(dir_a, raw_stdout=stdout_a)
            _write_maven_capture(dir_b, raw_stdout=stdout_b)
            # Corrupt capture-b's canonical bytes directly (simulating a
            # canonicalizer bug that failed to fully converge).
            (dir_b / "canonical-raw-input.bin").write_bytes(
                (dir_b / "canonical-raw-input.bin").read_bytes() + b"EXTRA"
            )
            report = verifier.verify_pair(dir_a, dir_b)
            self.assertFalse(report["passed"])
            self.assertFalse(report["canonical_bytes_equal"])

    def test_unmatched_raw_difference_fails(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            dir_a, dir_b, stdout_a, stdout_b = self._base_pair(tmp_path)
            # Inject an extra, uncovered raw difference: a comment line with
            # different content that no canonicalization rule touches.
            stdout_a_extra = stdout_a.replace(b"BUILD SUCCESS", b"BUILD SUCCESS extra-a")
            stdout_b_extra = stdout_b.replace(b"BUILD SUCCESS", b"BUILD SUCCESS extra-b")
            _write_maven_capture(dir_a, raw_stdout=stdout_a_extra)
            _write_maven_capture(dir_b, raw_stdout=stdout_b_extra)
            report = verifier.verify_pair(dir_a, dir_b)
            self.assertFalse(report["passed"])
            self.assertNotEqual(report["unmatched_raw_diff_lines"], [])

    def test_non_idempotent_canonicalizer_output_fails(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            dir_a, dir_b, stdout_a, stdout_b = self._base_pair(tmp_path)
            _write_maven_capture(dir_a, raw_stdout=stdout_a)
            _write_maven_capture(dir_b, raw_stdout=stdout_b)
            # Corrupt capture-a's canonical-raw-input.bin to contain a raw,
            # un-canonicalized volatile value -- re-canonicalizing it would
            # change bytes, proving it was never truly canonical.
            tampered = (dir_a / "canonical-raw-input.bin").read_text().replace(
                "<TIMESTAMP>", "1784136905453", 1
            )
            (dir_a / "canonical-raw-input.bin").write_text(tampered)
            report = verifier.verify_pair(dir_a, dir_b)
            self.assertFalse(report["passed"])
            self.assertFalse(report["idempotent_a"])


class TestNonCanonicalizedCaseIsVacuouslyIdempotent(unittest.TestCase):
    def test_raw_capped_stream_case_requires_exact_raw_equality(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            dir_a, dir_b = tmp_path / "a", tmp_path / "b"
            for d in (dir_a, dir_b):
                d.mkdir(parents=True)
                raw = b"running 3 tests\ntest result: ok. 3 passed; 0 failed; 0 ignored\n"
                (d / "raw.stdout").write_bytes(raw)
                (d / "canonical-raw-input.bin").write_bytes(raw)
                (d / "receipt.json").write_text(json.dumps({
                    "case_id": "repo-rustlings",
                    "source_identity": {"commit_sha": "a" * 40, "archive_sha256": "b" * 64},
                    "toolchain_resolved": {"resolved_version": "1.97.0"},
                    "toolchain_executed": {"executed_binary_sha256": "c" * 64},
                    "effective_execution_argv": ["cargo", "test"],
                    "sandbox_identity": {"sandboy_commit_sha": "d" * 40, "policy_sha256": "e" * 64},
                    "canonical_stream": "stdout",
                    "canonicalization_policy_sha256": None,
                    "canonical_input_derivation": "raw-capped-stream",
                }))
            report = verifier.verify_pair(dir_a, dir_b)
            self.assertTrue(report["passed"], report)
            self.assertTrue(report["idempotent_a"])
            self.assertTrue(report["idempotent_b"])


if __name__ == "__main__":
    unittest.main()
