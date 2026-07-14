"""Tests for capture_build.py's pure, offline-testable pieces: source
extraction + tree-manifest verification. Does not invoke Sandboy or dotnet —
those require the real workflow environment (own-net/sandboy, actions/setup-
dotnet) and are exercised end-to-end in CI, not here.
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
import capture_build as cb  # noqa: E402
import verify_source as vs  # noqa: E402

GOOD_CSPROJ = """<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup>
    <OutputType>Exe</OutputType>
    <TargetFramework>net8.0</TargetFramework>
  </PropertyGroup>
</Project>
"""
MIT_LICENSE = "MIT License\n\nPermission is hereby granted, free of charge, to any person obtaining a copy\n"


class TestExtractAndVerify(unittest.TestCase):
    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp(prefix="n2a-capture-test-"))
        repo = self.tmp / "repo"
        repo.mkdir()
        subprocess.run(["git", "init", "-q", "-b", "main", str(repo)], check=True)
        subprocess.run(["git", "-C", str(repo), "config", "user.email", "t@example.com"], check=True)
        subprocess.run(["git", "-C", str(repo), "config", "user.name", "T"], check=True)
        (repo / "EncryptAesApp").mkdir()
        (repo / "EncryptAesApp" / "EncryptAesApp.csproj").write_text(GOOD_CSPROJ)
        (repo / "EncryptAesApp" / "Program.cs").write_text("Console.WriteLine(1);\n")
        (repo / "LICENSE").write_text(MIT_LICENSE)
        subprocess.run(["git", "-C", str(repo), "add", "-A"], check=True)
        subprocess.run(["git", "-C", str(repo), "commit", "-q", "-m", "x"], check=True)
        sha = subprocess.run(
            ["git", "-C", str(repo), "rev-parse", "HEAD"], capture_output=True, text=True, check=True
        ).stdout.strip()

        manifest = {
            "manifest_version": "n2a-source-manifest-v1",
            "case_id": "test",
            "origin_kind": "generated-first-party-from-public-source",
            "repository": {"url": "https://github.com/example/x", "owner": "example", "name": "x",
                            "approved_commit_sha": sha},
            "license": {"spdx": "MIT", "file": "LICENSE"},
            "project": {"path": "EncryptAesApp/EncryptAesApp.csproj", "ecosystem": "dotnet",
                        "expected_target_framework": "net8.0", "expected_output_type": "Exe",
                        "expected_package_reference_count": 0, "expected_project_reference_count": 0},
            "build": {"argv": ["dotnet", "build"], "network_during_execution": "denied", "restore": "not-performed"},
        }
        manifest_path = self.tmp / "manifest.json"
        manifest_path.write_text(json.dumps(manifest))

        self.source_artifact_dir = self.tmp / "source-artifact"
        argv_backup = sys.argv
        sys.argv = ["verify_source.py", "--source-dir", str(repo), "--manifest", str(manifest_path),
                    "--out-dir", str(self.source_artifact_dir)]
        try:
            exit_code = vs.main()
        finally:
            sys.argv = argv_backup
        assert exit_code == 0, "fixture acquisition step itself must succeed"

        self.source_manifest = json.loads((self.source_artifact_dir / "source-manifest.json").read_text())
        self.file_manifest = json.loads((self.source_artifact_dir / "source-file-manifest.json").read_text())

    def tearDown(self):
        shutil.rmtree(self.tmp, ignore_errors=True)

    def test_extract_and_verify_round_trip(self):
        dest = self.tmp / "extracted"
        cb.verify_and_extract_source(self.source_artifact_dir / "source.tar", self.source_manifest, dest)
        cb.verify_extracted_tree(dest, self.file_manifest)  # must not raise
        self.assertTrue((dest / "EncryptAesApp" / "EncryptAesApp.csproj").is_file())
        self.assertTrue((dest / "LICENSE").is_file())

    def test_rejects_tampered_archive_hash(self):
        tampered_manifest = dict(self.source_manifest)
        tampered_manifest["resolved"] = dict(self.source_manifest["resolved"])
        tampered_manifest["resolved"]["archive_sha256"] = "0" * 64
        dest = self.tmp / "extracted-tampered"
        with self.assertRaises(cb.CaptureFailure):
            cb.verify_and_extract_source(self.source_artifact_dir / "source.tar", tampered_manifest, dest)

    def test_rejects_tampered_extracted_file(self):
        dest = self.tmp / "extracted2"
        cb.verify_and_extract_source(self.source_artifact_dir / "source.tar", self.source_manifest, dest)
        (dest / "LICENSE").write_text("tampered\n")
        with self.assertRaises(cb.CaptureFailure):
            cb.verify_extracted_tree(dest, self.file_manifest)

    def test_rejects_missing_manifest_file(self):
        dest = self.tmp / "extracted3"
        cb.verify_and_extract_source(self.source_artifact_dir / "source.tar", self.source_manifest, dest)
        (dest / "LICENSE").unlink()
        with self.assertRaises(cb.CaptureFailure):
            cb.verify_extracted_tree(dest, self.file_manifest)

    def test_capture_time_project_validation_matches_manifest(self):
        import dotnet_adapter as adapter

        dest = self.tmp / "extracted4"
        cb.verify_and_extract_source(self.source_artifact_dir / "source.tar", self.source_manifest, dest)
        result = adapter.validate_project_before_execution(dest, self.source_manifest)
        self.assertEqual(result["path"], "EncryptAesApp/EncryptAesApp.csproj")

    def test_capture_time_validation_rejects_injected_package_reference(self):
        import dotnet_adapter as adapter

        dest = self.tmp / "extracted5"
        cb.verify_and_extract_source(self.source_artifact_dir / "source.tar", self.source_manifest, dest)
        # Simulate a tampered extraction (e.g. a compromised artifact store)
        # slipping a dependency in between acquisition and capture.
        proj = dest / "EncryptAesApp" / "EncryptAesApp.csproj"
        proj.write_text(GOOD_CSPROJ.replace(
            "</PropertyGroup>",
            '</PropertyGroup>\n  <ItemGroup><PackageReference Include="X" Version="1.0.0" /></ItemGroup>',
        ))
        with self.assertRaises(ValueError):
            adapter.validate_project_before_execution(dest, self.source_manifest)


if __name__ == "__main__":
    unittest.main()
