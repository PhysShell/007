"""Tests that the top-level contract documents (miner-framework-contract.json,
toolchain-identity-contract.json) are well-formed and stay consistent with
the code and with the frozen-base guard's recorded SHAs."""
import json
import sys
import unittest
from pathlib import Path

MINER_DIR = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(MINER_DIR / "tests"))
sys.path.insert(0, str(MINER_DIR / "tools"))
import test_frozen_n2b as frozen  # noqa: E402
import sandbox_planner  # noqa: E402


class TestMinerFrameworkContract(unittest.TestCase):
    def setUp(self):
        self.doc = json.loads((MINER_DIR / "miner-framework-contract.json").read_text())

    def test_accepted_sandboy_sha_matches_code(self):
        self.assertEqual(self.doc["accepted_sandboy_commit_sha"], sandbox_planner.ACCEPTED_SANDBOY_COMMIT_SHA)

    def test_base_commit_matches_frozen_base_guard(self):
        self.assertEqual(self.doc["base_commit"], frozen.BASE)

    def test_lists_all_five_adapters(self):
        adapter_component = next(c for c in self.doc["components"] if c["name"] == "ToolAdapterRegistry")
        self.assertEqual(sorted(adapter_component["adapters"]), ["dotnet", "jvm-gradle", "jvm-maven", "python", "rust"])

    def test_every_component_has_a_module_reference(self):
        for component in self.doc["components"]:
            self.assertIn("module", component)
            self.assertTrue(component["module"])


class TestToolchainIdentityContractDoc(unittest.TestCase):
    def setUp(self):
        self.doc = json.loads((MINER_DIR / "toolchain-identity-contract.json").read_text())

    def test_four_classifications_present(self):
        self.assertEqual(
            sorted(self.doc["classifications"]),
            ["compatible-resolution", "exact-match", "identity-missing", "unexpected-resolution"],
        )

    def test_identity_missing_is_the_only_hard_failure(self):
        self.assertEqual(self.doc["hard_failure_classifications"], ["identity-missing"])


if __name__ == "__main__":
    unittest.main()
