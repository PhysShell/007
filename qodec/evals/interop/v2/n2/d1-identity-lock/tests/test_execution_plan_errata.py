"""N2-D1b: contract tests for execution-plan-errata.json.

Proves the exactly-two-authorized-errata invariant: (1) repo-pyflakes,
whose correction is minimal (differs only in the final path argument,
src/ -> pyflakes/); (2) repo-kubeops-generator, whose correction appends
only --no-restore after a real, evidenced trusted restore. Also proves
that no frozen N2-C file was modified by either correction.
"""
import hashlib
import json
import subprocess
import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[7]
ERRATA_PATH = Path(__file__).resolve().parents[1] / "execution-plan-errata.json"
N2C_MANIFEST_PATH = REPO_ROOT / "qodec/evals/interop/v2/n2/source-freeze/source-manifests/primary/repo-pyflakes.json"
KUBEOPS_N2C_MANIFEST_PATH = (
    REPO_ROOT / "qodec/evals/interop/v2/n2/source-freeze/source-manifests/primary/repo-kubeops-generator.json"
)


def load_errata() -> dict:
    return json.loads(ERRATA_PATH.read_text())


class TestExecutionPlanErrata(unittest.TestCase):
    def test_errata_self_hash_is_stable(self):
        body = load_errata()
        recorded = body.pop("errata_sha256")
        canonical = json.dumps(body, indent=2, sort_keys=True) + "\n"
        recomputed = hashlib.sha256(canonical.encode("utf-8")).hexdigest()
        self.assertEqual(recorded, recomputed)

    def test_exactly_two_authorized_errata(self):
        body = load_errata()
        authorized = [e for e in body["entries"] if e["status"] == "AUTHORIZED_ERRATUM"]
        self.assertEqual(len(authorized), 2)

    def test_the_two_authorized_errata_are_exactly_pyflakes_and_kubeops(self):
        body = load_errata()
        authorized = {e["case_id"] for e in body["entries"] if e["status"] == "AUTHORIZED_ERRATUM"}
        self.assertEqual(authorized, {"repo-pyflakes", "repo-kubeops-generator"})

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


class TestKubeOpsGeneratorErratum(unittest.TestCase):
    """The second authorized erratum: appending only --no-restore, after a
    real, evidenced trusted restore, to suppress the frozen argv's own
    implicit re-restore under permanent network denial."""

    def _entry(self) -> dict:
        body = load_errata()
        return next(e for e in body["entries"] if e["case_id"] == "repo-kubeops-generator")

    def test_correction_appends_only_no_restore(self):
        entry = self._entry()
        original = entry["original_frozen_argv"]
        corrected = entry["corrected_effective_argv"]
        self.assertEqual(corrected[:len(original)], original)
        self.assertEqual(corrected[len(original):], ["--no-restore"])

    def test_no_no_build_flag_no_framework_retarget(self):
        entry = self._entry()
        corrected = entry["corrected_effective_argv"]
        self.assertNotIn("--no-build", corrected)
        self.assertNotIn("--framework", corrected)
        # the exact same project target, untouched
        self.assertEqual(corrected[2], "test/KubeOps.Generator.Test/KubeOps.Generator.Test.csproj")

    def test_no_frozen_n2c_file_was_modified_by_this_correction(self):
        entry = self._entry()
        actual_sha256 = hashlib.sha256(KUBEOPS_N2C_MANIFEST_PATH.read_bytes()).hexdigest()
        self.assertEqual(entry["frozen_n2c_manifest_sha256"], actual_sha256)
        manifest = json.loads(KUBEOPS_N2C_MANIFEST_PATH.read_text())
        self.assertEqual(manifest["execution_expectation"]["argv"], entry["original_frozen_argv"])

    def test_trusted_restore_evidence_present_and_exit_zero(self):
        entry = self._entry()
        restore = entry["trusted_restore_evidence"]
        self.assertEqual(restore["restore_exit_code"], 0)
        self.assertTrue(restore["generated_assets"])
        evidence_path = REPO_ROOT / restore["restore_stdout_evidence_path"]
        self.assertEqual(hashlib.sha256(evidence_path.read_bytes()).hexdigest(), restore["restore_stdout_sha256"])

    def test_original_confined_run_evidence_shows_nu1301_under_network_denial(self):
        entry = self._entry()
        original_run = entry["original_confined_run_evidence"]
        self.assertEqual(original_run["detected_infrastructure_failure"], "nuget-restore-failure-nu1301")
        evidence_path = REPO_ROOT / original_run["raw_stdout_evidence_path"]
        actual = hashlib.sha256(evidence_path.read_bytes()).hexdigest()
        self.assertEqual(original_run["raw_stdout_sha256"], actual)
        self.assertIn(b"NU1301", evidence_path.read_bytes())

    def test_corrected_argv_validation_evidence_shows_real_vstest_summary(self):
        entry = self._entry()
        validation = entry["corrected_argv_validation_evidence"]
        self.assertEqual(validation["exit_code"], 0)
        self.assertIn("Passed!", validation["observed_vstest_summary"])
        evidence_path = REPO_ROOT / validation["stdout_evidence_path"]
        actual = hashlib.sha256(evidence_path.read_bytes()).hexdigest()
        self.assertEqual(validation["stdout_sha256"], actual)
        self.assertIn(b"Passed!", evidence_path.read_bytes())

    def test_no_frozen_n2c_path_was_touched_by_this_scope_git_diff(self):
        result = subprocess.run(
            ["git", "diff", "--name-only", "4e40b6f393cbdaf1bfcc36b8c422f7e17ae41dee", "HEAD",
             "--", "qodec/evals/interop/v2/n2/source-freeze", "qodec/evals/interop/v2/n2/d0-durable-evidence"],
            cwd=REPO_ROOT, capture_output=True, text=True, check=False,
        )
        changed = [line for line in result.stdout.splitlines() if line.strip()]
        self.assertEqual(changed, [], f"N2-C/N2-D0 frozen paths changed: {changed}")


if __name__ == "__main__":
    unittest.main()
