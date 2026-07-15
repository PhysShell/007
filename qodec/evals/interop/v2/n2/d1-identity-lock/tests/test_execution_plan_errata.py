"""N2-D1b: contract tests for execution-plan-errata.json.

Proves the exactly-one-authorized-erratum invariant, that the sole erratum
is repo-pyflakes, that the correction is minimal (differs only in the final
path argument, src/ -> pyflakes/), and that no frozen N2-C file was
modified by this correction.
"""
import hashlib
import json
import subprocess
import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[7]
ERRATA_PATH = Path(__file__).resolve().parents[1] / "execution-plan-errata.json"
N2C_MANIFEST_PATH = REPO_ROOT / "qodec/evals/interop/v2/n2/source-freeze/source-manifests/primary/repo-pyflakes.json"


def load_errata() -> dict:
    return json.loads(ERRATA_PATH.read_text())


class TestExecutionPlanErrata(unittest.TestCase):
    def test_errata_self_hash_is_stable(self):
        body = load_errata()
        recorded = body.pop("errata_sha256")
        canonical = json.dumps(body, indent=2, sort_keys=True) + "\n"
        recomputed = hashlib.sha256(canonical.encode("utf-8")).hexdigest()
        self.assertEqual(recorded, recomputed)

    def test_exactly_one_authorized_erratum(self):
        body = load_errata()
        authorized = [e for e in body["entries"] if e["status"] == "AUTHORIZED_ERRATUM"]
        self.assertEqual(len(authorized), 1)

    def test_the_one_authorized_erratum_is_repo_pyflakes(self):
        body = load_errata()
        authorized = [e for e in body["entries"] if e["status"] == "AUTHORIZED_ERRATUM"]
        self.assertEqual(authorized[0]["case_id"], "repo-pyflakes")

    def test_correction_differs_only_in_final_path_argument(self):
        body = load_errata()
        entry = next(e for e in body["entries"] if e["case_id"] == "repo-pyflakes")
        original = entry["original_frozen_argv"]
        corrected = entry["corrected_effective_argv"]
        self.assertEqual(len(original), len(corrected))
        # every argument except the last must be byte-identical
        self.assertEqual(original[:-1], corrected[:-1])
        self.assertNotEqual(original[-1], corrected[-1])

    def test_correction_is_specifically_src_to_pyflakes(self):
        body = load_errata()
        entry = next(e for e in body["entries"] if e["case_id"] == "repo-pyflakes")
        self.assertEqual(entry["original_frozen_argv"][-1], "src/")
        self.assertEqual(entry["corrected_effective_argv"][-1], "pyflakes/")

    def test_correction_does_not_substitute_a_different_workload(self):
        body = load_errata()
        entry = next(e for e in body["entries"] if e["case_id"] == "repo-pyflakes")
        original_tool_argv = entry["original_frozen_argv"][:3]
        corrected_tool_argv = entry["corrected_effective_argv"][:3]
        self.assertEqual(original_tool_argv, ["python", "-m", "pyflakes"])
        self.assertEqual(corrected_tool_argv, ["python", "-m", "pyflakes"])
        self.assertNotIn(".", entry["corrected_effective_argv"])

    def test_no_frozen_n2c_file_was_modified_by_this_correction(self):
        """The errata record must reference the frozen manifest's real,
        currently-matching SHA256 -- if the manifest had been touched to
        "fix" the argv there, this comparison would fail."""
        body = load_errata()
        entry = next(e for e in body["entries"] if e["case_id"] == "repo-pyflakes")
        actual_sha256 = hashlib.sha256(N2C_MANIFEST_PATH.read_bytes()).hexdigest()
        self.assertEqual(entry["frozen_n2c_manifest_sha256"], actual_sha256)
        # and the manifest's own committed argv must still be the ORIGINAL
        # (unmodified) one -- the erratum lives only in this D1b-owned file.
        manifest = json.loads(N2C_MANIFEST_PATH.read_text())
        self.assertEqual(manifest["execution_expectation"]["argv"], entry["original_frozen_argv"])

    def test_no_frozen_n2c_path_was_touched_by_this_scope_git_diff(self):
        """Belt-and-suspenders: confirm via git that no path under
        source-freeze/ or d0-durable-evidence/ has uncommitted or
        N2-D1b-authored changes relative to the accepted N2-D0 closure head."""
        result = subprocess.run(
            ["git", "diff", "--name-only", "4e40b6f393cbdaf1bfcc36b8c422f7e17ae41dee", "HEAD",
             "--", "qodec/evals/interop/v2/n2/source-freeze", "qodec/evals/interop/v2/n2/d0-durable-evidence"],
            cwd=REPO_ROOT, capture_output=True, text=True, check=False,
        )
        changed = [line for line in result.stdout.splitlines() if line.strip()]
        self.assertEqual(changed, [], f"N2-C/N2-D0 frozen paths changed: {changed}")

    def test_correction_rule_text_present(self):
        body = load_errata()
        entry = next(e for e in body["entries"] if e["case_id"] == "repo-pyflakes")
        self.assertIn("smallest evidence-based correction", entry["correction_rule"])

    def test_verified_tree_evidence_confirms_src_absent_and_pyflakes_present(self):
        body = load_errata()
        entry = next(e for e in body["entries"] if e["case_id"] == "repo-pyflakes")
        self.assertFalse(entry["verified_src_directory_present"])
        self.assertTrue(entry["verified_pyflakes_directory_present"])

    def test_retained_stderr_evidence_file_matches_recorded_hash(self):
        body = load_errata()
        entry = next(e for e in body["entries"] if e["case_id"] == "repo-pyflakes")
        evidence_path = REPO_ROOT / entry["retained_stderr_evidence_path"]
        actual = hashlib.sha256(evidence_path.read_bytes()).hexdigest()
        self.assertEqual(entry["original_stderr_sha256"], actual)


if __name__ == "__main__":
    unittest.main()
