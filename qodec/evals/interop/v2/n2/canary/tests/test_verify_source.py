"""Tests for verify_source.py's trusted-acquisition validation, against a
synthetic fixture repository — never the real approved canary repository
(this session cannot and must not fetch it; see qodec-n2-miner-canary.yml's
trusted-source-acquisition job, which is the only place that ever touches it).
"""
import json
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

TOOLS = Path(__file__).resolve().parents[1] / "tools"
sys.path.insert(0, str(TOOLS))
import verify_source as vs  # noqa: E402

GOOD_CSPROJ = """<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup>
    <OutputType>Exe</OutputType>
    <TargetFramework>net8.0</TargetFramework>
    <ImplicitUsings>enable</ImplicitUsings>
    <Nullable>enable</Nullable>
  </PropertyGroup>
</Project>
"""

MIT_LICENSE = """MIT License

Copyright (c) 2024 Example Author

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction.
"""

BASE_MANIFEST = {
    "manifest_version": "n2a-source-manifest-v1",
    "case_id": "test-fixture-001",
    "origin_kind": "generated-first-party-from-public-source",
    "repository": {
        "url": "https://github.com/example/fixture-repo",
        "owner": "example",
        "name": "fixture-repo",
        "approved_commit_sha": "",  # filled in per-test
    },
    "license": {"spdx": "MIT", "file": "LICENSE"},
    "project": {
        "path": "EncryptAesApp/EncryptAesApp.csproj",
        "ecosystem": "dotnet",
        "expected_target_framework": "net8.0",
        "expected_output_type": "Exe",
        "expected_package_reference_count": 0,
        "expected_project_reference_count": 0,
    },
    "build": {
        "argv": ["dotnet", "build", "EncryptAesApp/EncryptAesApp.csproj", "--no-restore"],
        "network_during_execution": "denied",
        "restore": "not-performed",
    },
}


def _git(repo: Path, *args: str) -> str:
    r = subprocess.run(["git", "-C", str(repo), *args], capture_output=True, text=True, check=True)
    return r.stdout.strip()


class FixtureRepoTestCase(unittest.TestCase):
    """Builds a fresh synthetic git repo matching the approved csproj shape,
    with a hook to mutate it before the final commit."""

    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp(prefix="n2a-fixture-"))
        self.repo = self.tmp / "repo"
        self.repo.mkdir()
        _git(self.repo, "init", "-q", "-b", "main")
        _git(self.repo, "config", "user.email", "test@example.com")
        _git(self.repo, "config", "user.name", "Test")

    def tearDown(self):
        shutil.rmtree(self.tmp, ignore_errors=True)

    def commit_fixture(self, *, csproj=GOOD_CSPROJ, license_text=MIT_LICENSE, extra_files=None):
        (self.repo / "EncryptAesApp").mkdir(exist_ok=True)
        (self.repo / "EncryptAesApp" / "EncryptAesApp.csproj").write_text(csproj)
        (self.repo / "EncryptAesApp" / "Program.cs").write_text("Console.WriteLine(\"hi\");\n")
        (self.repo / "LICENSE").write_text(license_text)
        for rel, content in (extra_files or {}).items():
            p = self.repo / rel
            p.parent.mkdir(parents=True, exist_ok=True)
            p.write_text(content)
        subprocess.run(["git", "-C", str(self.repo), "add", "-A"], check=True)
        subprocess.run(["git", "-C", str(self.repo), "commit", "-q", "-m", "fixture"], check=True)
        return _git(self.repo, "rev-parse", "HEAD")

    def manifest_for(self, sha: str) -> dict:
        m = json.loads(json.dumps(BASE_MANIFEST))
        m["repository"]["approved_commit_sha"] = sha
        return m

    def run_verify(self, manifest: dict, out_dir: Path | None = None):
        out_dir = out_dir or (self.tmp / "out")
        manifest_path = self.tmp / "manifest.json"
        manifest_path.write_text(json.dumps(manifest))
        import io
        import contextlib

        stderr = io.StringIO()
        argv_backup = sys.argv
        sys.argv = [
            "verify_source.py",
            "--source-dir", str(self.repo),
            "--manifest", str(manifest_path),
            "--out-dir", str(out_dir),
        ]
        try:
            with contextlib.redirect_stderr(stderr):
                exit_code = vs.main()
        finally:
            sys.argv = argv_backup
        return exit_code, stderr.getvalue(), out_dir


class TestAcceptedCandidate(FixtureRepoTestCase):
    def test_accepts_matching_fixture(self):
        sha = self.commit_fixture()
        exit_code, _, out_dir = self.run_verify(self.manifest_for(sha))
        self.assertEqual(exit_code, 0)
        self.assertTrue((out_dir / "source.tar").exists())
        self.assertTrue((out_dir / "source-manifest.json").exists())
        self.assertTrue((out_dir / "license-record.json").exists())
        self.assertTrue((out_dir / "source-file-manifest.json").exists())

    def test_archive_excludes_git_dir(self):
        import tarfile

        sha = self.commit_fixture()
        _, _, out_dir = self.run_verify(self.manifest_for(sha))
        with tarfile.open(out_dir / "source.tar") as tar:
            names = tar.getnames()
        self.assertTrue(all(not n.startswith(".git") for n in names))
        self.assertIn("LICENSE", names)
        self.assertIn("EncryptAesApp/EncryptAesApp.csproj", names)

    def test_archive_is_deterministic_across_two_runs(self):
        sha = self.commit_fixture()
        _, _, out_a = self.run_verify(self.manifest_for(sha), self.tmp / "out_a")
        _, _, out_b = self.run_verify(self.manifest_for(sha), self.tmp / "out_b")
        self.assertEqual(
            (out_a / "source.tar").read_bytes(),
            (out_b / "source.tar").read_bytes(),
        )


class TestRejectedCandidates(FixtureRepoTestCase):
    def test_rejects_wrong_commit_sha(self):
        self.commit_fixture()
        manifest = self.manifest_for("f" * 40)
        exit_code, stderr, _ = self.run_verify(manifest)
        self.assertEqual(exit_code, 1)
        self.assertIn("REJECTED", stderr)
        self.assertIn("substitute a different revision", stderr)

    def test_rejects_package_reference(self):
        csproj = GOOD_CSPROJ.replace(
            "<Nullable>enable</Nullable>",
            '<Nullable>enable</Nullable>\n    <PackageReference Include="Newtonsoft.Json" Version="13.0.1" />',
        )
        sha = self.commit_fixture(csproj=csproj)
        exit_code, stderr, _ = self.run_verify(self.manifest_for(sha))
        self.assertEqual(exit_code, 1)
        self.assertIn("PackageReference", stderr)

    def test_rejects_project_reference(self):
        csproj = GOOD_CSPROJ.replace(
            "<Nullable>enable</Nullable>",
            '<Nullable>enable</Nullable>\n    <ProjectReference Include="../Other/Other.csproj" />',
        )
        sha = self.commit_fixture(csproj=csproj)
        exit_code, stderr, _ = self.run_verify(self.manifest_for(sha))
        self.assertEqual(exit_code, 1)
        self.assertIn("ProjectReference", stderr)

    def test_rejects_custom_import(self):
        csproj = GOOD_CSPROJ.replace(
            "</Project>", '  <Import Project="custom.props" />\n</Project>'
        )
        sha = self.commit_fixture(csproj=csproj)
        exit_code, stderr, _ = self.run_verify(self.manifest_for(sha))
        self.assertEqual(exit_code, 1)
        self.assertIn("Import", stderr)

    def test_rejects_wrong_target_framework(self):
        csproj = GOOD_CSPROJ.replace("net8.0", "net6.0")
        sha = self.commit_fixture(csproj=csproj)
        exit_code, stderr, _ = self.run_verify(self.manifest_for(sha))
        self.assertEqual(exit_code, 1)
        self.assertIn("TargetFramework", stderr)

    def test_rejects_missing_license(self):
        self.commit_fixture()
        (self.repo / "LICENSE").unlink()
        subprocess.run(["git", "-C", str(self.repo), "add", "-A"], check=True)
        subprocess.run(["git", "-C", str(self.repo), "commit", "-q", "-m", "remove license"], check=True)
        sha2 = _git(self.repo, "rev-parse", "HEAD")
        exit_code, stderr, _ = self.run_verify(self.manifest_for(sha2))
        self.assertEqual(exit_code, 1)
        self.assertIn("license file", stderr)

    def test_rejects_non_mit_license_text(self):
        sha = self.commit_fixture(license_text="All rights reserved. No permission granted.\n")
        exit_code, stderr, _ = self.run_verify(self.manifest_for(sha))
        self.assertEqual(exit_code, 1)
        self.assertIn("MIT", stderr)

    def test_rejects_gitmodules(self):
        sha = self.commit_fixture(extra_files={".gitmodules": "[submodule \"x\"]\n"})
        exit_code, stderr, _ = self.run_verify(self.manifest_for(sha))
        self.assertEqual(exit_code, 1)
        self.assertIn("submodule", stderr)

    def test_rejects_stray_targets_file(self):
        sha = self.commit_fixture(extra_files={"custom.targets": "<Project></Project>\n"})
        exit_code, stderr, _ = self.run_verify(self.manifest_for(sha))
        self.assertEqual(exit_code, 1)
        self.assertIn("MSBuild import file", stderr)

    def test_rejects_stray_props_file(self):
        sha = self.commit_fixture(extra_files={"Directory.Build.props": "<Project></Project>\n"})
        exit_code, stderr, _ = self.run_verify(self.manifest_for(sha))
        self.assertEqual(exit_code, 1)
        self.assertIn("MSBuild", stderr)

    def test_rejects_nuget_config(self):
        sha = self.commit_fixture(extra_files={"NuGet.config": "<configuration></configuration>\n"})
        exit_code, stderr, _ = self.run_verify(self.manifest_for(sha))
        self.assertEqual(exit_code, 1)
        self.assertIn("NuGet.config", stderr)

    def test_rejects_global_json(self):
        sha = self.commit_fixture(extra_files={"global.json": "{}\n"})
        exit_code, stderr, _ = self.run_verify(self.manifest_for(sha))
        self.assertEqual(exit_code, 1)
        self.assertIn("global.json", stderr)

    def test_rejects_git_lfs_pointer(self):
        lfs_pointer = "version https://git-lfs.github.com/spec/v1\noid sha256:0000\nsize 1\n"
        sha = self.commit_fixture(extra_files={"asset.bin": lfs_pointer})
        exit_code, stderr, _ = self.run_verify(self.manifest_for(sha))
        self.assertEqual(exit_code, 1)
        self.assertIn("LFS", stderr)

    def test_rejects_executable_git_hook(self):
        sha = self.commit_fixture()
        hook = self.repo / ".git" / "hooks" / "pre-commit"
        hook.write_text("#!/bin/sh\necho pwned\n")
        hook.chmod(0o755)
        exit_code, stderr, _ = self.run_verify(self.manifest_for(sha))
        self.assertEqual(exit_code, 1)
        self.assertIn("hook", stderr)

    def test_missing_project_path_is_rejected(self):
        sha = self.commit_fixture()
        manifest = self.manifest_for(sha)
        manifest["project"]["path"] = "DoesNotExist/DoesNotExist.csproj"
        exit_code, stderr, _ = self.run_verify(manifest)
        self.assertEqual(exit_code, 1)
        self.assertIn("project file not found", stderr)


if __name__ == "__main__":
    unittest.main()
