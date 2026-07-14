"""Tests for the Scope M1 additions: RTK pin, comparison contract, Nix flake,
GitHub Actions workflow, and the non-scoring smoke suite.

Pure-Python and runnable without Nix. Tests that need the qodec binary or
actionlint locate them and skip cleanly when absent (in CI the flake check
provides both). Designed to be driven via Nix (`nix build
.#checks.x86_64-linux.qodec-v2-contract`) but also runnable directly.
"""

import json
import os
import re
import subprocess
import sys
import unittest
from pathlib import Path

V2_DIR = Path(__file__).resolve().parents[1]
REPO_ROOT = Path(os.environ["V2_REPO_ROOT"]) if os.environ.get("V2_REPO_ROOT") else V2_DIR.parents[3]
sys.path.insert(0, str(V2_DIR))
sys.path.insert(0, str(V2_DIR / "smoke"))
sys.path.insert(0, str(Path(__file__).resolve().parent))

import validate_contract as VC  # noqa: E402

FLAKE = REPO_ROOT / "flake.nix"
WORKFLOW = REPO_ROOT / ".github" / "workflows" / "qodec-v2.yml"
GITIGNORE = REPO_ROOT / ".gitignore"
COMPARISON = V2_DIR / "rtk-comparison-contract.json"
RTK_PIN = "5d32d0736f686b69d1e8b9dc45c007d4eb77a0a2"

SHA40 = re.compile(r"^[0-9a-f]{40}$")


def is_pinned_commit(ref: str) -> bool:
    """A pin is valid only as a full 40-hex commit SHA — never a branch/tag/short sha."""
    return bool(SHA40.match(ref))


def find_qodec() -> str | None:
    for cand in (os.environ.get("QODEC_BIN"),
                 str(REPO_ROOT / "qodec" / "target" / "release" / "qodec"),
                 str(V2_DIR.parents[2] / "target" / "release" / "qodec")):
        if cand and Path(cand).exists():
            return cand
    from shutil import which
    return which("qodec")


class TestRtkPin(unittest.TestCase):
    def test_rtk_source_uses_exact_commit_sha(self):
        text = FLAKE.read_text()
        m = re.search(r"github:rtk-ai/rtk/([^\"\s]+)", text)
        self.assertIsNotNone(m, "rtk-src input not found in flake.nix")
        self.assertTrue(is_pinned_commit(m.group(1)), f"rtk-src ref {m.group(1)!r} is not a 40-hex commit")
        self.assertEqual(m.group(1), RTK_PIN)
        self.assertIn("flake = false", text)

    def test_rtk_moving_ref_is_rejected(self):
        for bad in ("main", "master", "v1.0.0", "latest", "5d32d07", RTK_PIN.upper()):
            self.assertFalse(is_pinned_commit(bad), f"{bad!r} must be rejected as a pin")
        self.assertTrue(is_pinned_commit(RTK_PIN))

    def test_comparison_contract_pin_matches_flake(self):
        c = json.loads(COMPARISON.read_text())
        self.assertEqual(c["rtk_source_sha"], RTK_PIN)


class TestComparisonContract(unittest.TestCase):
    def setUp(self):
        self.c = json.loads(COMPARISON.read_text())

    def test_comparison_contract_is_non_gating(self):
        self.assertEqual(self.c["status"], "observational-before-data")
        self.assertIs(self.c["promotion_gate"], False)

    def test_comparison_contract_inherits_interop_v2(self):
        self.assertEqual(self.c["inherits_interop_contract"], "interop-benchmark-v2")
        self.assertEqual(self.c["contract_version"], "rtk-qodec-comparison-v1")

    def test_all_four_logical_arms_exist(self):
        self.assertEqual(set(self.c["logical_arms"]), {"RAW", "QODEC", "RTK", "RTK+QODEC"})

    def test_transparency_leaderboard_excludes_rtk(self):
        arms = self.c["leaderboards"]["transparency"]["arms"]
        self.assertEqual(arms, ["RAW", "QODEC"])
        self.assertNotIn("RTK", arms)
        self.assertNotIn("RTK+QODEC", arms)

    def test_utility_leaderboard_contains_all_four(self):
        arms = self.c["leaderboards"]["end_to_end_utility"]["arms"]
        self.assertEqual(set(arms), {"RAW", "QODEC", "RTK", "RTK+QODEC"})

    def test_token_metric_forbids_chars_over_4_as_canonical(self):
        ta = self.c["token_accounting"]
        self.assertIn("chars/4", ta["forbidden_as_canonical"])
        # chars/4 must not appear as a leaderboard metric
        for lb in self.c["leaderboards"].values():
            for metric in lb["metrics"]:
                self.assertNotIn("chars", metric.lower())

    def test_required_savings_columns_present(self):
        self.assertEqual(
            set(self.c["required_savings_columns"]),
            {"qodec_vs_raw", "rtk_vs_raw", "qodec_incremental_after_rtk", "hybrid_vs_raw"},
        )


class TestSmokeSuite(unittest.TestCase):
    def test_smoke_fixtures_marked_non_benchmark(self):
        readme = (V2_DIR / "smoke" / "README.md").read_text()
        for marker in ("NON-BENCHMARK", "NON-GATING", "NOT PART OF THE 48 BASE CASES", "NOT PART OF HELD-OUT"):
            self.assertIn(marker, readme)

    def test_smoke_fixtures_cannot_appear_in_coverage_manifest(self):
        from test_contract import build_manifest
        import tempfile
        with tempfile.TemporaryDirectory() as d:
            base = Path(d)
            m = build_manifest(base)
            smoke_case = dict(m["cases"][0])
            smoke_case["case_id"] = "smoke-build-log"
            smoke_case["tags"] = ["non-benchmark"]
            m["cases"].append(smoke_case)
            cov = VC.load_json(V2_DIR / "coverage-matrix.json")
            violations = VC.validate(m, cov, base)
            self.assertTrue(any(v.startswith("[smoke-leak]") for v in violations),
                            "coverage manifest must reject non-benchmark/smoke cases")

    @unittest.skipUnless(find_qodec(), "qodec binary not available")
    def test_qodec_roundtrips(self):
        import run_smoke
        qbin = find_qodec()
        raw = (V2_DIR / "smoke" / "fixtures" / "build-log.txt").read_bytes()
        arm = run_smoke.qodec_arm(qbin, raw, "o200k")
        self.assertTrue(arm["roundtrip_ok"])
        self.assertLessEqual(arm["tokens_out"], arm["tokens_in"])

    @unittest.skipUnless(find_qodec(), "qodec binary not available")
    def test_qodec_roundtrips_rtk_output(self):
        # losslessness holds for ANY input, including RTK-shaped reduced text
        import run_smoke
        qbin = find_qodec()
        reduced = b"error[E0308]: mismatched types src/core/parse.rs:120:17\n[see remaining: 3 more]\n"
        arm = run_smoke.qodec_arm(qbin, reduced, "o200k")
        self.assertTrue(arm["roundtrip_ok"])
        self.assertLessEqual(arm["tokens_out"], arm["tokens_in"])


class TestWorkflow(unittest.TestCase):
    def setUp(self):
        self.text = WORKFLOW.read_text()

    def test_workflow_third_party_actions_are_sha_pinned(self):
        uses = re.findall(r"uses:\s*([^\s@]+)@([^\s]+)(?:\s*#\s*(\S+))?", self.text)
        self.assertTrue(uses, "no `uses:` entries found")
        for action, ref, tag in uses:
            self.assertTrue(SHA40.match(ref), f"{action}@{ref} is not pinned to a 40-hex SHA")
            self.assertTrue(tag, f"{action}@{ref} missing a human-readable tag comment")

    def test_workflow_has_no_floating_refs(self):
        # inspect only `uses:` action refs, not prose/comments or branch filters
        for ref in re.findall(r"uses:\s*\S+@(\S+)", self.text):
            self.assertNotIn(ref, ("main", "master", "latest"))
            self.assertTrue(SHA40.match(ref), f"floating/unpinned ref {ref!r}")

    def test_workflow_permissions_contents_read_only(self):
        import yaml
        doc = yaml.safe_load(self.text)
        self.assertEqual(doc["permissions"], {"contents": "read"})

    def test_workflow_has_cancel_concurrency(self):
        import yaml
        doc = yaml.safe_load(self.text)
        self.assertTrue(doc["concurrency"]["cancel-in-progress"])

    def test_workflow_contains_no_model_calls(self):
        low = self.text.lower()
        forbidden = ["openai", "anthropic", "api_key", "api-key", "apikey",
                     "/v1/chat", "/v1/completions", "vllm", "huggingface", "x-api-key"]
        for tok in forbidden:
            self.assertNotIn(tok, low, f"workflow references forbidden model-call token {tok!r}")

    def test_workflow_uses_official_nix_installer(self):
        self.assertIn("install-nix-action", self.text)

    def test_actionlint_passes(self):
        from shutil import which
        al = which("actionlint")
        if not al:
            self.skipTest("actionlint not available")
        r = subprocess.run([al, str(WORKFLOW)], capture_output=True, text=True)
        self.assertEqual(r.returncode, 0, f"actionlint failed:\n{r.stdout}\n{r.stderr}")


class TestFlakeOutputs(unittest.TestCase):
    def setUp(self):
        self.text = FLAKE.read_text()

    def test_existing_outputs_preserved(self):
        for token in ("default = o7", "o7 = o7", "flake-utils.lib.mkApp",
                      "devShells", "default = pkgs.mkShell", "inherit o7",
                      "clippy = craneLib", "fmt = craneLib"):
            self.assertIn(token, self.text)

    def test_new_packages_present(self):
        for token in ("qodec = qodec", "rtk-pinned = rtk-pinned",
                      "qodec-bench", "qodec-v2-contract-test", "qodec-rtk-smoke",
                      "qodec-build", "rtk-pinned-build", "qodec-v2-contract",
                      "github-actions-lint"):
            self.assertIn(token, self.text)


class TestGitignore(unittest.TestCase):
    def test_private_path_remains_ignored(self):
        self.assertIn("qodec/evals/interop/v2/private/", GITIGNORE.read_text())


if __name__ == "__main__":
    unittest.main(verbosity=2)
