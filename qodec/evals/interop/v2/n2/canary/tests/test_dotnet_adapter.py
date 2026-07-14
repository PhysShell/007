"""Tests for dotnet_adapter.py's toolchain-identity parsing and absolute-path
resolution.

Regression coverage for a real N2-A bug: dotnet_sdk_version and
dotnet_runtime_identifier came back None from an actual `dotnet --info` run,
because the parsing regexes were anchored at line-start with no tolerance for
the leading whitespace real `dotnet --info` output indents every field with.
"""
import stat
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

TOOLS = Path(__file__).resolve().parents[1] / "tools"
sys.path.insert(0, str(TOOLS))
import dotnet_adapter as adapter  # noqa: E402

# A realistic `dotnet --info` transcript (trimmed), with the real leading
# whitespace indentation dotnet actually emits.
REAL_DOTNET_INFO_OUTPUT = """.NET SDK:
 Version:            8.0.404
 Commit:             abcdef1234
 Workload version:   8.0.404-manifests.abc123

Runtime Environment:
 OS Name:     ubuntu
 OS Version:  24.04
 OS Platform: Linux
 RID:         linux-x64
 Base Path:   /usr/share/dotnet/sdk/8.0.404/

.NET workloads installed:
 Workload version: 8.0.404-manifests.abc123
 There are no installed workloads to display.

Host:
  Version:      8.0.11
  Architecture: x64
  Commit:       fedcba4321

.NET SDKs installed:
  8.0.404 [/usr/share/dotnet/sdk]

.NET runtimes installed:
  Microsoft.AspNetCore.App 8.0.11 [/usr/share/dotnet/shared/Microsoft.AspNetCore.App]
  Microsoft.NETCore.App 8.0.11 [/usr/share/dotnet/shared/Microsoft.NETCore.App]

Environment variables:
  Not set

global.json file:
  Not found

Learn more:
  https://aka.ms/dotnet/info
"""


class TestCaptureToolchainIdentity(unittest.TestCase):
    def _run_with_fake_info(self, stdout_text, returncode=0):
        fake_result = mock.Mock(stdout=stdout_text, returncode=returncode)
        with mock.patch.object(adapter.subprocess, "run", return_value=fake_result):
            with mock.patch.object(adapter, "_sha256_file", return_value="deadbeef"):
                return adapter.capture_toolchain_identity("/usr/share/dotnet/dotnet")

    def test_parses_sdk_version_despite_leading_whitespace(self):
        identity = self._run_with_fake_info(REAL_DOTNET_INFO_OUTPUT)
        self.assertEqual(identity["sdk_version"], "8.0.404")

    def test_parses_rid_despite_leading_whitespace(self):
        identity = self._run_with_fake_info(REAL_DOTNET_INFO_OUTPUT)
        self.assertEqual(identity["runtime_identifier"], "linux-x64")

    def test_sdk_version_is_the_sdk_section_not_the_host_section(self):
        # The "Host:" section also has a "Version:" field (8.0.11 above);
        # first-match must be the .NET SDK section's version (8.0.404).
        identity = self._run_with_fake_info(REAL_DOTNET_INFO_OUTPUT)
        self.assertNotEqual(identity["sdk_version"], "8.0.11")

    def test_parses_base_path(self):
        identity = self._run_with_fake_info(REAL_DOTNET_INFO_OUTPUT)
        self.assertEqual(identity["sdk_base_path"], "/usr/share/dotnet/sdk/8.0.404/")

    def test_missing_fields_come_back_none_not_empty_string(self):
        identity = self._run_with_fake_info("garbage output with no recognizable fields\n")
        self.assertIsNone(identity["sdk_version"])
        self.assertIsNone(identity["runtime_identifier"])

    def test_dotnet_binary_path_is_the_passed_absolute_path(self):
        identity = self._run_with_fake_info(REAL_DOTNET_INFO_OUTPUT)
        self.assertEqual(identity["dotnet_binary_path"], "/usr/share/dotnet/dotnet")


class TestResolveDotnetBin(unittest.TestCase):
    def test_prefers_dotnet_root_over_path_lookup(self):
        with tempfile.TemporaryDirectory() as tmp:
            fake_dotnet = Path(tmp) / "dotnet"
            fake_dotnet.write_text("#!/bin/sh\necho fake\n")
            fake_dotnet.chmod(fake_dotnet.stat().st_mode | stat.S_IEXEC)
            resolved = adapter.resolve_dotnet_bin(tmp)
            self.assertEqual(resolved, str(fake_dotnet))

    def test_falls_back_to_path_when_dotnet_root_missing_binary(self):
        with tempfile.TemporaryDirectory() as tmp:
            # tmp has no "dotnet" file in it.
            with mock.patch.object(adapter.shutil, "which", return_value="/usr/bin/dotnet"):
                resolved = adapter.resolve_dotnet_bin(tmp)
            self.assertEqual(resolved, "/usr/bin/dotnet")

    def test_falls_back_to_bare_name_when_nothing_resolves(self):
        with mock.patch.object(adapter.shutil, "which", return_value=None):
            resolved = adapter.resolve_dotnet_bin(None)
        self.assertEqual(resolved, "dotnet")

    def test_none_dotnet_root_skips_straight_to_path_lookup(self):
        with mock.patch.object(adapter.shutil, "which", return_value="/opt/dotnet/dotnet"):
            resolved = adapter.resolve_dotnet_bin(None)
        self.assertEqual(resolved, "/opt/dotnet/dotnet")


class TestBuildArgv(unittest.TestCase):
    def test_replaces_bare_dotnet_with_resolved_absolute_path(self):
        manifest = {"build": {"argv": ["dotnet", "build", "Foo.csproj", "--no-restore"]}}
        argv = adapter.build_argv(manifest, "/usr/share/dotnet/dotnet")
        self.assertEqual(argv, ["/usr/share/dotnet/dotnet", "build", "Foo.csproj", "--no-restore"])

    def test_leaves_non_dotnet_argv0_untouched(self):
        # Defensive: only ever substitutes the literal "dotnet" name, never
        # silently rewrites some other tool's argv[0].
        manifest = {"build": {"argv": ["some-other-tool", "build"]}}
        argv = adapter.build_argv(manifest, "/usr/share/dotnet/dotnet")
        self.assertEqual(argv, ["some-other-tool", "build"])

    def test_does_not_mutate_the_source_manifest(self):
        manifest = {"build": {"argv": ["dotnet", "build"]}}
        adapter.build_argv(manifest, "/abs/dotnet")
        self.assertEqual(manifest["build"]["argv"], ["dotnet", "build"])


if __name__ == "__main__":
    unittest.main()
