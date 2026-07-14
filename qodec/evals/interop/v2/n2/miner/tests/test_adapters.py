"""Tests for the ToolAdapter registry (section 9) — dotnet/rust/python/maven/gradle."""
import sys
import unittest
from pathlib import Path

MINER_DIR = Path(__file__).resolve().parents[1]
FIXTURES = MINER_DIR / "fixtures"
sys.path.insert(0, str(MINER_DIR / "tools"))
from adapters import ADAPTERS, base, registered_ecosystems, get_adapter  # noqa: E402
from adapters import dotnet_adapter, rust_adapter, python_adapter, maven_adapter, gradle_adapter  # noqa: E402


class TestAdapterRegistry(unittest.TestCase):
    def test_all_five_adapters_registered(self):
        self.assertEqual(
            registered_ecosystems(),
            ["dotnet", "jvm-gradle", "jvm-maven", "python", "rust"],
        )

    def test_get_adapter_returns_module(self):
        self.assertIs(get_adapter("dotnet"), dotnet_adapter)

    def test_unknown_ecosystem_raises(self):
        with self.assertRaises(ValueError):
            get_adapter("cobol")

    def test_every_adapter_implements_the_full_contract(self):
        for ecosystem, adapter in ADAPTERS.items():
            for fn_name in base.REQUIRED_ADAPTER_FUNCTIONS:
                self.assertTrue(
                    hasattr(adapter, fn_name),
                    f"{ecosystem} adapter missing required function {fn_name!r}",
                )

    def test_every_adapter_receipt_fields_is_superset_of_common(self):
        manifest = {"project": {"entry_point": "x"}}
        for ecosystem, adapter in ADAPTERS.items():
            fields = set(adapter.receipt_fields(manifest))
            self.assertTrue(
                set(base.COMMON_RECEIPT_FIELDS).issubset(fields),
                f"{ecosystem} adapter's receipt_fields() is missing common fields",
            )


class TestArgvArraysNeverShellStrings(unittest.TestCase):
    def _assert_argv_is_list_of_strings(self, argv):
        self.assertIsInstance(argv, list)
        for item in argv:
            self.assertIsInstance(item, str)
            self.assertNotIn(" && ", item)
            self.assertNotIn(" | ", item)
            self.assertNotIn(";", item)

    def test_dotnet_argv_arrays(self):
        manifest = {"project": {"entry_point": "Foo/Foo.csproj"}}
        self._assert_argv_is_list_of_strings(dotnet_adapter.plan_trusted_setup(manifest)["steps"][0]["argv"])
        self._assert_argv_is_list_of_strings(dotnet_adapter.plan_untrusted_execution(manifest)["argv"])

    def test_rust_argv_arrays(self):
        manifest = {"project": {"entry_point": "Cargo.toml"}}
        self._assert_argv_is_list_of_strings(rust_adapter.plan_trusted_setup(manifest)["steps"][0]["argv"])
        self._assert_argv_is_list_of_strings(rust_adapter.plan_untrusted_execution(manifest)["argv"])

    def test_python_argv_arrays(self):
        manifest = {"project": {"entry_point": "pyproject.toml"}, "dependency_lock": {"files": []}}
        self._assert_argv_is_list_of_strings(python_adapter.plan_trusted_setup(manifest)["steps"][0]["argv"])
        self._assert_argv_is_list_of_strings(python_adapter.plan_untrusted_execution(manifest)["argv"])

    def test_maven_argv_arrays(self):
        manifest = {"project": {"entry_point": "pom.xml"}}
        self._assert_argv_is_list_of_strings(maven_adapter.plan_trusted_setup(manifest)["steps"][0]["argv"])
        self._assert_argv_is_list_of_strings(maven_adapter.plan_untrusted_execution(manifest)["argv"])

    def test_gradle_argv_arrays(self):
        manifest = {"project": {"entry_point": "build.gradle.kts"}}
        self._assert_argv_is_list_of_strings(gradle_adapter.plan_trusted_setup(manifest)["steps"][0]["argv"])
        self._assert_argv_is_list_of_strings(gradle_adapter.plan_untrusted_execution(manifest)["argv"])


class TestDotnetAdapterFixtures(unittest.TestCase):
    def test_simple_project_detected_unambiguous(self):
        result = dotnet_adapter.detect(FIXTURES / "dotnet_simple")
        self.assertEqual(result["candidate_entry_points"], ["Foo/Foo.csproj"])
        self.assertFalse(result["ambiguous"])
        self.assertEqual(result["confidence"], 1.0)

    def test_ambiguous_project_not_auto_selected(self):
        result = dotnet_adapter.detect(FIXTURES / "dotnet_ambiguous")
        self.assertTrue(result["ambiguous"])
        self.assertEqual(len(result["candidate_entry_points"]), 2)
        self.assertTrue(result["ambiguities"])

        manifest = {"ecosystem": "dotnet", "project": {"entry_point": None, "ambiguous": True}}
        errors = dotnet_adapter.validate_manifest(manifest)
        self.assertTrue(any("ambiguous" in e for e in errors))

    def test_package_reference_detected(self):
        inspected = dotnet_adapter.inspect(FIXTURES / "dotnet_packageref", "Bar/Bar.csproj")
        self.assertEqual(inspected["package_reference_count"], 1)

    def test_custom_msbuild_import_detected(self):
        result = dotnet_adapter.detect(FIXTURES / "dotnet_custom_msbuild")
        self.assertTrue(result["custom_imports"])
        self.assertEqual(result["custom_imports"][0]["project"], "Baz/Baz.csproj")


class TestRustAdapterFixtures(unittest.TestCase):
    def test_workspace_members_detected(self):
        result = rust_adapter.detect(FIXTURES / "rust_workspace")
        self.assertEqual(sorted(result["workspace_members"]), ["crate-a", "crate-b"])
        self.assertTrue(result["lockfiles"])
        self.assertTrue(result["offline_mode_feasible"])

    def test_build_rs_detected_as_custom_script(self):
        result = rust_adapter.detect(FIXTURES / "rust_buildrs")
        self.assertIn("build.rs", result["custom_scripts"])


class TestPythonAdapterFixtures(unittest.TestCase):
    def test_locked_project_offline_feasible(self):
        result = python_adapter.detect(FIXTURES / "python_pytest_lock")
        self.assertTrue(result["lockfiles"])
        self.assertTrue(result["offline_mode_feasible"])
        self.assertTrue(result["test_entry_points"])

    def test_unlocked_project_flagged_as_network_risk(self):
        result = python_adapter.detect(FIXTURES / "python_no_lock")
        self.assertFalse(result["lockfiles"])
        self.assertFalse(result["offline_mode_feasible"])
        self.assertTrue(result["network_risk_indicators"])


class TestMavenAdapterFixtures(unittest.TestCase):
    def test_multimodule_detected(self):
        result = maven_adapter.detect(FIXTURES / "maven_multimodule")
        self.assertEqual(sorted(result["modules"]), ["module-a", "module-b"])


class TestGradleAdapterFixtures(unittest.TestCase):
    def test_custom_repository_flagged_as_network_risk(self):
        result = gradle_adapter.detect(FIXTURES / "gradle_wrapper_custom_repo")
        self.assertTrue(result["network_risk_indicators"])
        self.assertTrue(result["has_wrapper"])
        self.assertFalse(result["offline_mode_feasible"])


if __name__ == "__main__":
    unittest.main()
