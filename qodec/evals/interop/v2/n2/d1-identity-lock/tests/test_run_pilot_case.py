"""Unit tests for run_pilot_case.py's build_toolchain_fn_and_env -- targeted
at the real, byte-level-inspection-confirmed bugs found in CI run #7:

  - Python: venv_root was computed with Path(python_bin).resolve(), which
    follows the venv's own bin/python symlink to the real base interpreter
    (e.g. /usr/bin/python3.x) and silently derives the WRONG root (e.g.
    "/usr" instead of the venv directory) -- VIRTUAL_ENV then pointed
    nowhere useful and the sandbox never actually exposed the real venv,
    reproducing the identical PermissionError as before the fix existed.
  - Maven: no fix at all previously existed for ~/.m2/repository -- trusted
    setup populates the REAL, unconfined ~/.m2, but the confined process's
    HOME is an isolated fresh directory with no relationship to it.
  - Gradle: no fix at all previously existed for the daemon's own client<->
    daemon TCP loopback bind, which Sandboy's (correctly) fully-closed
    tcp_bind policy rejects.
"""
import argparse
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

TOOLS = Path(__file__).resolve().parents[1] / "tools"
sys.path.insert(0, str(TOOLS))
import run_pilot_case as rpc  # noqa: E402


def _args(**overrides):
    base = {
        "java_home_11": "/usr/lib/jvm/java-11-openjdk-amd64",
        "java_home_21": "/usr/lib/jvm/java-21-openjdk-amd64",
        "venv_python": "",
    }
    base.update(overrides)
    return argparse.Namespace(**base)


class TestPythonVenvRootComputation(unittest.TestCase):
    def test_symlinked_venv_python_does_not_collapse_to_base_interpreter_root(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            fake_base_python = tmp_path / "usr-bin" / "python3.11"
            fake_base_python.parent.mkdir(parents=True)
            fake_base_python.write_text("#!/bin/sh\n")
            fake_base_python.chmod(0o755)

            venv_root = tmp_path / "venv-repo-pyflakes"
            (venv_root / "bin").mkdir(parents=True)
            venv_python = venv_root / "bin" / "python"
            venv_python.symlink_to(fake_base_python)

            _toolchain_fn, env_values = rpc.build_toolchain_fn_and_env(
                "python", _args(venv_python=str(venv_python))
            )
            self.assertEqual(env_values["VIRTUAL_ENV"], str(venv_root))
            # The bug this guards against: .resolve() following the symlink
            # would have produced the base interpreter's grandparent instead.
            self.assertNotEqual(env_values["VIRTUAL_ENV"], str(fake_base_python.parent.parent))


class TestMavenLocalRepoExposure(unittest.TestCase):
    def test_maven_opts_points_at_real_m2_repository_and_policy_key_matches(self):
        _toolchain_fn, env_values = rpc.build_toolchain_fn_and_env("jvm-maven", _args())
        real_m2_repo = str(Path.home() / ".m2" / "repository")
        self.assertIn(f"-Dmaven.repo.local={real_m2_repo}", env_values["MAVEN_OPTS"])
        self.assertEqual(env_values["MAVEN_LOCAL_REPO_PATH"], real_m2_repo)


class TestGradleDaemonDisabled(unittest.TestCase):
    def test_gradle_properties_under_gradle_user_home_disables_the_daemon(self):
        # First attempt (CI run #7/#8) was GRADLE_OPTS=-Dorg.gradle.daemon=
        # false -- real evidence showed Gradle instead forked a fresh
        # "single-use" daemon (still a daemon, still the same TCP bind)
        # because the injected JVM arg didn't match any idle default
        # daemon. org.gradle.daemon=false only reliably disables the daemon
        # from a gradle.properties file under GRADLE_USER_HOME.
        with tempfile.TemporaryDirectory() as tmp:
            fake_home = Path(tmp)
            with mock.patch.object(rpc.Path, "home", return_value=fake_home):
                _toolchain_fn, env_values = rpc.build_toolchain_fn_and_env("jvm-gradle", _args())
            gradle_user_home = fake_home / ".gradle"
            self.assertEqual(env_values["GRADLE_USER_HOME"], str(gradle_user_home))
            self.assertNotIn("GRADLE_OPTS", env_values)
            props = (gradle_user_home / "gradle.properties").read_text()
            self.assertIn("org.gradle.daemon=false", props)


if __name__ == "__main__":
    unittest.main()
