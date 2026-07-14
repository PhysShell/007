"""Tests for sandbox_planner.py — SandboxExecutionPlanner (section 12)."""
import sys
import unittest
from pathlib import Path

MINER_DIR = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(MINER_DIR / "tools"))
import sandbox_planner as sp  # noqa: E402
from adapters import dotnet_adapter, rust_adapter, python_adapter, maven_adapter, gradle_adapter  # noqa: E402


class TestAcceptedSandboyPinUnchanged(unittest.TestCase):
    def test_pin_matches_accepted_s0_commit(self):
        self.assertEqual(sp.ACCEPTED_SANDBOY_COMMIT_SHA, "e925058ddea405b5821fc0aed4882c76650dcbe9")


class TestPlanningWithinCapabilityEnvelope(unittest.TestCase):
    def test_dotnet_plan_has_no_capability_gaps(self):
        manifest = {"project": {"entry_point": "Foo/Foo.csproj"}}
        plan = sp.plan_sandbox_execution(manifest, dotnet_adapter)
        self.assertEqual(plan["status"], "PLANNED")
        self.assertEqual(plan["capability_gaps"], [])

    def test_all_adapters_produce_capability_gap_free_plans(self):
        manifests = {
            dotnet_adapter: {"project": {"entry_point": "Foo/Foo.csproj"}},
            rust_adapter: {"project": {"entry_point": "Cargo.toml"}},
            python_adapter: {"project": {"entry_point": "pyproject.toml"}},
            maven_adapter: {"project": {"entry_point": "pom.xml"}},
            gradle_adapter: {"project": {"entry_point": "build.gradle.kts"}},
        }
        for adapter, manifest in manifests.items():
            plan = sp.plan_sandbox_execution(manifest, adapter)
            self.assertEqual(plan["status"], "PLANNED", f"{adapter.__name__} produced a capability gap")


class TestMandatoryDefaultDeny(unittest.TestCase):
    def test_all_mandatory_deny_elements_present(self):
        manifest = {"project": {"entry_point": "Foo/Foo.csproj"}}
        plan = sp.plan_sandbox_execution(manifest, dotnet_adapter)
        for element in ("credentials", "ssh_agent", "host_home", "docker_socket",
                        "unrelated_workspace_paths", "external_network_during_untrusted_execution",
                        "unbounded_process_creation", "unbounded_wall_time"):
            self.assertIn(element, plan["mandatory_default_deny"])


class TestUnjustifiedFilesystemPermissionRejected(unittest.TestCase):
    def test_arbitrary_broad_path_is_flagged_as_capability_gap(self):
        class _FakeAdapter:
            @staticmethod
            def filesystem_policy_hints(manifest):
                return {"read_only": {}, "writable": {"/home/runner": "unjustified broad access"}, "must_pre_create": []}

            @staticmethod
            def environment_allowlist(manifest):
                return []

            @staticmethod
            def resource_limit_hints(manifest):
                from adapters import base
                return base.generic_resource_limit_hints()

        plan = sp.plan_sandbox_execution({}, _FakeAdapter)
        self.assertEqual(plan["status"], "CAPABILITY_GAP")
        self.assertTrue(plan["capability_gaps"])

    def test_docker_socket_path_is_flagged_as_capability_gap(self):
        class _FakeAdapter:
            @staticmethod
            def filesystem_policy_hints(manifest):
                return {"read_only": {}, "writable": {"/var/run/docker.sock": "unjustified"}, "must_pre_create": []}

            @staticmethod
            def environment_allowlist(manifest):
                return []

            @staticmethod
            def resource_limit_hints(manifest):
                from adapters import base
                return base.generic_resource_limit_hints()

        plan = sp.plan_sandbox_execution({}, _FakeAdapter)
        self.assertEqual(plan["status"], "CAPABILITY_GAP")


class TestRlimitAsNeverClaimedSafe(unittest.TestCase):
    def test_rejected_mechanisms_lists_rlimit_as(self):
        manifest = {"project": {"entry_point": "Foo/Foo.csproj"}}
        plan = sp.plan_sandbox_execution(manifest, dotnet_adapter)
        self.assertIn("RLIMIT_AS", plan["outer_resource_limits"]["rejected_mechanisms"])

    def test_memory_enforcement_mechanism_is_never_rlimit_as(self):
        manifest = {"project": {"entry_point": "Foo/Foo.csproj"}}
        plan = sp.plan_sandbox_execution(manifest, dotnet_adapter)
        self.assertNotEqual(plan["outer_resource_limits"]["memory_enforcement_mechanism"], "RLIMIT_AS")
        self.assertIn(
            plan["outer_resource_limits"]["memory_enforcement_mechanism"],
            ("cgroup-enforced", "outer-runner-enforced", "unavailable"),
        )


if __name__ == "__main__":
    unittest.main()
