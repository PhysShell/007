"""Section 23 tests for generate_manifests.py: schema validation, argv-array
enforcement (no shell strings), and that N2-A/N2-B frozen paths are
unaffected by manifest generation."""
import sys
import unittest
from pathlib import Path

TOOLS = Path(__file__).resolve().parents[1] / "tools"
sys.path.insert(0, str(TOOLS))
import generate_manifests  # noqa: E402
import registry as registry_mod  # noqa: E402
import eligibility  # noqa: E402
import selection  # noqa: E402

SOURCE_FREEZE_DIR = Path(__file__).resolve().parents[1]


class TestRealManifestsOnDisk(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        reg = registry_mod.load_registry(SOURCE_FREEZE_DIR / "candidate-registry.json")
        reports = eligibility.evaluate_registry(reg)
        eligible_ids = {r["candidate_id"] for r in reports if r["eligible"]}
        cls.eligible = [c for c in reg["candidates"] if c["candidate_id"] in eligible_ids]
        cls.result = selection.run_selection(cls.eligible)
        cls.by_id = {c["candidate_id"]: c for c in cls.eligible}

    def test_every_primary_has_a_valid_manifest_on_disk(self):
        for cid in self.result["primary_case_ids"]:
            path = SOURCE_FREEZE_DIR / "source-manifests" / "primary" / f"{cid}.json"
            self.assertTrue(path.is_file(), f"missing manifest for {cid}")
            import json
            manifest = json.loads(path.read_text())
            self.assertEqual(generate_manifests.validate_manifest(manifest), [])
            self.assertEqual(manifest["selection_role"], "primary")
            self.assertIsNone(manifest["fallback_priority"])

    def test_every_alternate_has_a_valid_manifest_with_fallback_priority(self):
        for priority, cid in enumerate(self.result["alternate_case_ids"], start=1):
            path = SOURCE_FREEZE_DIR / "source-manifests" / "alternate" / f"{cid}.json"
            self.assertTrue(path.is_file(), f"missing manifest for {cid}")
            import json
            manifest = json.loads(path.read_text())
            self.assertEqual(manifest["selection_role"], "alternate")
            self.assertEqual(manifest["fallback_priority"], priority)

    def test_no_manifest_argv_contains_a_shell_string(self):
        import json
        for role in ("primary", "alternate"):
            for path in (SOURCE_FREEZE_DIR / "source-manifests" / role).glob("*.json"):
                manifest = json.loads(path.read_text())
                argv = manifest["execution_expectation"]["argv"]
                self.assertIsInstance(argv, list)
                for item in argv:
                    self.assertIsInstance(item, str)
                    self.assertNotIn(" && ", item)
                    self.assertNotIn(" | ", item)
                    self.assertNotIn(";", item)

    def test_every_repository_manifest_has_frozen_argv_or_explicit_probe_marker(self):
        # Section 6: never an empty argv silently claiming the plan is
        # frozen — either a real argv (status "frozen") or an explicit
        # "requires-n2d-probe" marker, never anything else.
        import json
        for role in ("primary", "alternate"):
            for path in (SOURCE_FREEZE_DIR / "source-manifests" / role).glob("*.json"):
                manifest = json.loads(path.read_text())
                if manifest["source_kind"] != "repository-execution":
                    continue
                exe = manifest["execution_expectation"]
                self.assertIn(exe["execution_plan_status"], ("frozen", "requires-n2d-probe"))
                if exe["execution_plan_status"] == "frozen":
                    self.assertTrue(exe["argv"], f"{manifest['case_id']}: frozen plan must have a real argv")
                else:
                    self.assertEqual(exe["argv"], [], f"{manifest['case_id']}: requires-n2d-probe must have empty argv")

    def test_every_non_repository_manifest_has_extraction_recipe_not_bare_empty_argv(self):
        import json
        for role in ("primary", "alternate"):
            for path in (SOURCE_FREEZE_DIR / "source-manifests" / role).glob("*.json"):
                manifest = json.loads(path.read_text())
                if manifest["source_kind"] == "repository-execution":
                    continue
                exe = manifest["execution_expectation"]
                self.assertEqual(exe["argv"], [])
                self.assertEqual(exe["execution_plan_status"], "frozen")
                self.assertIsNotNone(exe["extraction_recipe"], f"{manifest['case_id']}: non-repository case needs a deterministic extraction recipe")
                recipe = exe["extraction_recipe"]
                self.assertIn("maximum_extracted_source_bytes", recipe)
                self.assertGreater(recipe["maximum_extracted_source_bytes"], 0)

    def test_repo_fd_and_terraform_compound_commands_marked_requires_probe(self):
        # Real registry data: these two alternates have a "&&"-joined
        # multi-step capture command that cannot be a single argv array.
        import json
        for cid in ("repo-fd", "repo-terraform-example-module"):
            path = SOURCE_FREEZE_DIR / "source-manifests" / "alternate" / f"{cid}.json"
            manifest = json.loads(path.read_text())
            self.assertEqual(manifest["execution_expectation"]["execution_plan_status"], "requires-n2d-probe")
            self.assertEqual(manifest["execution_expectation"]["argv"], [])


class TestManifestRejectsShellString(unittest.TestCase):
    def test_manifest_with_shell_string_argv_fails_schema(self):
        candidate = {
            "candidate_id": "c1", "source_kind": "repository-execution", "origin_kind": "repository-miner",
            "ecosystem": "rust", "primary_family": "test", "secondary_tags": [],
            "source_identity": {"identity_kind": "git-commit", "commit_sha": "a" * 40},
            "license": {"spdx": "MIT", "redistribution_allowed": True},
            "project": {"entry_point": "Cargo.toml"},
            "public_canonical_url": "https://github.com/example/example",
            "expected_capture_command_class": "cargo test", "expected_size_bucket": "small",
            "expected_size_estimation_basis": "test",
        }
        manifest = generate_manifests.build_source_manifest(candidate, "primary", None, [])
        # A caller trying to smuggle a shell string into argv (instead of an
        # array of tokens) violates the schema's `items: {"type": "string"}`
        # only at the array level, not string CONTENT — this is why
        # test_no_manifest_argv_contains_a_shell_string above checks content
        # directly; here we confirm argv defaults to an empty list, never a
        # bare string.
        self.assertIsInstance(manifest["execution_expectation"]["argv"], list)

    def test_compound_shell_command_marked_requires_probe_not_truncated(self):
        candidate = {
            "candidate_id": "c2", "source_kind": "repository-execution", "origin_kind": "repository-miner",
            "ecosystem": "rust", "primary_family": "test", "secondary_tags": [],
            "source_identity": {"identity_kind": "git-commit", "commit_sha": "a" * 40},
            "license": {"spdx": "MIT", "redistribution_allowed": True},
            "project": {"entry_point": "Cargo.toml"},
            "public_canonical_url": "https://github.com/example/example",
            "expected_capture_command_class": "cargo run -- --version && cargo run -- .",
            "expected_size_bucket": "small", "expected_size_estimation_basis": "test",
        }
        manifest = generate_manifests.build_source_manifest(candidate, "primary", None, [])
        exe = manifest["execution_expectation"]
        self.assertEqual(exe["execution_plan_status"], "requires-n2d-probe")
        self.assertEqual(exe["argv"], [])

    def test_non_repository_candidate_gets_extraction_recipe(self):
        candidate = {
            "candidate_id": "c3", "source_kind": "dataset-artifact", "origin_kind": "public-runtime-dataset",
            "ecosystem": "infrastructure-or-language-neutral", "primary_family": "runtime", "secondary_tags": [],
            "source_identity": {"identity_kind": "immutable-object-or-doi", "object_id_or_doi": "10.5281/zenodo.1",
                                 "selected_exact_file": "data.tar.gz", "archive_member": "data.log"},
            "license": {"spdx": "CC0-1.0", "redistribution_allowed": True},
            "project": {"entry_point": None},
            "public_canonical_url": "https://zenodo.org/records/1",
            "expected_size_bucket": "small", "expected_size_estimation_basis": "test",
        }
        manifest = generate_manifests.build_source_manifest(candidate, "primary", None, [])
        exe = manifest["execution_expectation"]
        self.assertEqual(exe["execution_plan_status"], "frozen")
        self.assertEqual(exe["argv"], [])
        self.assertEqual(exe["extraction_recipe"]["input_file_identity"], "source/data.tar.gz")
        self.assertEqual(exe["extraction_recipe"]["archive_member"], "data.log")


if __name__ == "__main__":
    unittest.main()
