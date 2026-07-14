"""Tests for compare_reproducibility.py against synthetic capture-a/capture-b
snapshot manifests (never the real captures — those only exist after a real
CI run)."""
import json
import shutil
import sys
import tempfile
import unittest
from pathlib import Path

TOOLS = Path(__file__).resolve().parents[1] / "tools"
sys.path.insert(0, str(TOOLS))
import compare_reproducibility as cr  # noqa: E402

FIELDS = [
    "case_id", "source_repository", "source_commit_sha", "source_archive_sha256",
    "license_spdx", "license_sha256", "sandboy_commit_sha", "canonical_policy_sha256",
    "argv", "environment_allowlist", "dotnet_sdk_version", "dotnet_runtime_identifier",
    "dotnet_binary_sha256", "exit_code", "sanitized_stdout_sha256", "sanitized_stderr_sha256",
]


def make_semantic_view(**overrides) -> dict:
    base = {
        "case_id": "miner-canary-dotnet-001",
        "source_repository": "https://github.com/example/x",
        "source_commit_sha": "a" * 40,
        "source_archive_sha256": "b" * 64,
        "license_spdx": "MIT",
        "license_sha256": "c" * 64,
        "sandboy_commit_sha": "e925058ddea405b5821fc0aed4882c76650dcbe9",
        "canonical_policy_sha256": "d" * 64,
        "argv": ["dotnet", "build"],
        "environment_allowlist": ["PATH", "HOME"],
        "dotnet_sdk_version": "8.0.100",
        "dotnet_runtime_identifier": "linux-x64",
        "dotnet_binary_sha256": "e" * 64,
        "exit_code": 0,
        "sanitized_stdout_sha256": "f" * 64,
        "sanitized_stderr_sha256": "0" * 64,
    }
    base.update(overrides)
    return base


def make_snapshot(job: str, raw_stdout="raw1", raw_stderr="raw1err", **overrides) -> dict:
    return {
        "job": job,
        "semantic_receipt_fields": FIELDS,
        "semantic_view": make_semantic_view(**overrides),
        "raw_stdout_sha256": raw_stdout,
        "raw_stderr_sha256": raw_stderr,
    }


class TestCompareReproducibility(unittest.TestCase):
    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp(prefix="n2a-repro-test-"))
        self.a_dir = self.tmp / "a"
        self.b_dir = self.tmp / "b"
        self.a_dir.mkdir()
        self.b_dir.mkdir()

    def tearDown(self):
        shutil.rmtree(self.tmp, ignore_errors=True)

    def write(self, d: Path, snapshot: dict):
        (d / "snapshot-manifest.json").write_text(json.dumps(snapshot))

    def test_identical_semantic_views_are_reproducible(self):
        self.write(self.a_dir, make_snapshot("capture-a"))
        self.write(self.b_dir, make_snapshot("capture-b", raw_stdout="raw2", raw_stderr="raw2err"))
        snap_a = cr.load_snapshot(self.a_dir)
        snap_b = cr.load_snapshot(self.b_dir)
        rows = cr.compare(snap_a, snap_b)
        self.assertTrue(all(r["equal"] for r in rows))

    def test_differing_exit_code_is_detected(self):
        self.write(self.a_dir, make_snapshot("capture-a", exit_code=0))
        self.write(self.b_dir, make_snapshot("capture-b", exit_code=1))
        rows = cr.compare(cr.load_snapshot(self.a_dir), cr.load_snapshot(self.b_dir))
        exit_row = next(r for r in rows if r["field"] == "exit_code")
        self.assertFalse(exit_row["equal"])

    def test_differing_sanitized_stdout_is_detected(self):
        self.write(self.a_dir, make_snapshot("capture-a", sanitized_stdout_sha256="x" * 64))
        self.write(self.b_dir, make_snapshot("capture-b", sanitized_stdout_sha256="y" * 64))
        rows = cr.compare(cr.load_snapshot(self.a_dir), cr.load_snapshot(self.b_dir))
        row = next(r for r in rows if r["field"] == "sanitized_stdout_sha256")
        self.assertFalse(row["equal"])

    def test_raw_stdout_difference_does_not_fail_the_gate(self):
        # Raw output is EXPECTED to differ (temp paths, timestamps, PIDs);
        # only sanitized hashes are part of the reproducibility gate.
        self.write(self.a_dir, make_snapshot("capture-a", raw_stdout="raw-unique-a"))
        self.write(self.b_dir, make_snapshot("capture-b", raw_stdout="raw-unique-b"))
        rows = cr.compare(cr.load_snapshot(self.a_dir), cr.load_snapshot(self.b_dir))
        self.assertTrue(all(r["equal"] for r in rows))

    def test_mismatched_semantic_field_set_raises(self):
        snap_a = make_snapshot("capture-a")
        snap_b = make_snapshot("capture-b")
        snap_b["semantic_receipt_fields"] = FIELDS[:-1]  # drop a field
        self.write(self.a_dir, snap_a)
        self.write(self.b_dir, snap_b)
        with self.assertRaises(ValueError):
            cr.compare(cr.load_snapshot(self.a_dir), cr.load_snapshot(self.b_dir))

    def test_full_main_writes_reports_and_exits_nonzero_on_mismatch(self):
        import io
        import contextlib

        self.write(self.a_dir, make_snapshot("capture-a", exit_code=0))
        self.write(self.b_dir, make_snapshot("capture-b", exit_code=1))
        (self.a_dir / "sandboy-execution-receipt.json").write_text(json.dumps({"exit_code": 0, "sandboy_commit_sha": "x"}))
        for d in (self.a_dir, self.b_dir):
            (d / "network-isolation-report.json").write_text(json.dumps({"all_targets_unreachable": True}))
            (d / "resource-limit-report.json").write_text(json.dumps({"requested_limits": {"cpu_time_s": 1}}))
        (self.a_dir / "sanitization-report.json").write_text("{}")

        src_dir = self.tmp / "src"
        src_dir.mkdir()
        (src_dir / "source-manifest.json").write_text(json.dumps({
            "license": {"spdx": "MIT"}, "repository": {"approved_commit_sha": "a" * 40},
        }))
        (src_dir / "license-record.json").write_text(json.dumps({"spdx": "MIT", "file": "LICENSE", "sha256": "x"}))

        out_dir = self.tmp / "out"
        argv_backup = sys.argv
        sys.argv = [
            "compare_reproducibility.py",
            "--capture-a-dir", str(self.a_dir),
            "--capture-b-dir", str(self.b_dir),
            "--source-artifact-dir", str(src_dir),
            "--out-dir", str(out_dir),
        ]
        stdout = io.StringIO()
        try:
            with contextlib.redirect_stdout(stdout):
                exit_code = cr.main()
        finally:
            sys.argv = argv_backup

        self.assertEqual(exit_code, 1)
        self.assertTrue((out_dir / "reproducibility-report.json").exists())
        self.assertTrue((out_dir / "miner-canary-summary.md").exists())
        report = json.loads((out_dir / "reproducibility-report.json").read_text())
        self.assertFalse(report["overall_reproducible"])
        summary = (out_dir / "miner-canary-summary.md").read_text()
        self.assertIn("FAIL", summary)


if __name__ == "__main__":
    unittest.main()
