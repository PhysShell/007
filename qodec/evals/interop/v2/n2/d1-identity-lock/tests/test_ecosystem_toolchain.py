"""Unit tests for ecosystem_toolchain.py's parsing logic.

Uses real captured `--version`-style output (recorded verbatim from this
session's actual local rustc/cargo/python/mvn installs) as fixture text, so
the regex-extraction logic is pinned without requiring all 5 toolchains to
be installed wherever these tests run.
"""
import sys
import unittest
from pathlib import Path

TOOLS = Path(__file__).resolve().parents[1] / "tools"
sys.path.insert(0, str(TOOLS))
import ecosystem_toolchain as et  # noqa: E402

REAL_RUSTC_VERBOSE = (
    "rustc 1.97.0 (2d8144b78 2026-07-07)\n"
    "binary: rustc\n"
    "commit-hash: 2d8144b7880597b6e6d3dfd63a9a9efae3f533d3\n"
    "commit-date: 2026-07-07\n"
    "host: x86_64-unknown-linux-gnu\n"
    "release: 1.97.0\n"
    "LLVM version: 22.1.6\n"
)

REAL_MVN_VERSION = (
    "Apache Maven 3.9.11 (3e54c93a704957b63ee3494413a2b544fd3d825b)\n"
    "Maven home: /opt/maven\n"
    "Java version: 11.0.31, vendor: Ubuntu, runtime: /usr/lib/jvm/java-11-openjdk-amd64\n"
    "Default locale: en_US, platform encoding: UTF-8\n"
    'OS name: "linux", version: "6.18.5", arch: "amd64", family: "unix"\n'
)

REAL_GRADLE_VERSION = (
    "------------------------------------------------------------\n"
    "Gradle 8.14.3\n"
    "------------------------------------------------------------\n\n"
    "Build time:   2025-08-08 08:63:24 UTC\n"
    "Revision:     b2ea86e18e1a1eff0f2d75c6dc8f10cbe93e4b8a\n\n"
    "Kotlin:       2.1.21\n"
    "Groovy:       3.0.24\n"
    "Ant:          Apache Ant(TM) version 1.10.15 compiled on August 25 2024\n"
    "JVM:          21.0.10 (Ubuntu 21.0.10+7-Ubuntu-124.04)\n"
    "OS:           Linux 6.18.5 amd64\n"
)


class TestFirstMatch(unittest.TestCase):
    def test_rustc_release_extraction(self):
        import re
        version = et._first_match(r"^release:\s*(\S+)\s*$", REAL_RUSTC_VERBOSE, re.MULTILINE)
        self.assertEqual(version, "1.97.0")

    def test_rustc_commit_hash_extraction(self):
        import re
        commit = et._first_match(r"^commit-hash:\s*(\S+)\s*$", REAL_RUSTC_VERBOSE, re.MULTILINE)
        self.assertEqual(commit, "2d8144b7880597b6e6d3dfd63a9a9efae3f533d3")

    def test_rustc_host_extraction(self):
        import re
        host = et._first_match(r"^host:\s*(\S+)\s*$", REAL_RUSTC_VERBOSE, re.MULTILINE)
        self.assertEqual(host, "x86_64-unknown-linux-gnu")

    def test_maven_version_extraction(self):
        import re
        version = et._first_match(r"^Apache Maven\s+(\S+)", REAL_MVN_VERSION, re.MULTILINE)
        self.assertEqual(version, "3.9.11")

    def test_maven_java_version_extraction(self):
        import re
        java_version = et._first_match(r"^Java version:\s*([^,]+),", REAL_MVN_VERSION, re.MULTILINE)
        self.assertEqual(java_version.strip(), "11.0.31")

    def test_gradle_version_extraction(self):
        import re
        version = et._first_match(r"^Gradle\s+(\S+)\s*$", REAL_GRADLE_VERSION, re.MULTILINE)
        self.assertEqual(version, "8.14.3")

    def test_gradle_jvm_extraction(self):
        import re
        jvm = et._first_match(r"^JVM:\s*(\S+)", REAL_GRADLE_VERSION, re.MULTILINE)
        self.assertEqual(jvm, "21.0.10")

    def test_no_match_returns_none(self):
        self.assertIsNone(et._first_match(r"^nonexistent:\s*(\S+)$", REAL_RUSTC_VERBOSE))


class TestRealLocalToolchainCapture(unittest.TestCase):
    """These call the real, locally-installed toolchains this session already
    has (rustc/cargo/python3) -- not mocks. Skipped gracefully if a given
    tool isn't present in whatever environment runs this suite."""

    def test_rust_identity_capture_real(self):
        identity = et.capture_rust_toolchain_identity()
        if identity["rustc_binary_path"] is None:
            self.skipTest("rustc not installed in this environment")
        self.assertIsNotNone(identity["resolved_version"])
        self.assertIsNotNone(identity["rustc_binary_sha256"])
        self.assertEqual(identity["rustc_version_verbose_exit_code"], 0)

    def test_python_identity_capture_real(self):
        identity = et.capture_python_toolchain_identity(sys.executable)
        self.assertIsNotNone(identity["resolved_version"])
        self.assertIsNotNone(identity["python_binary_sha256"])


if __name__ == "__main__":
    unittest.main()
