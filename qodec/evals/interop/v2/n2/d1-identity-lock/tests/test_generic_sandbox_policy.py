"""Unit tests for generic_sandbox_policy.py."""
import sys
import tempfile
import unittest
from pathlib import Path

TOOLS = Path(__file__).resolve().parents[1] / "tools"
sys.path.insert(0, str(TOOLS))
import generic_sandbox_policy as gsp  # noqa: E402


class TestBuildPolicy(unittest.TestCase):
    def test_unknown_ecosystem_raises(self):
        with self.assertRaises(ValueError):
            gsp.build_policy(
                ecosystem="cobol", case_id="repo-test-case", source_root=Path("/x"), home_dir=Path("/h"),
                tmp_dir=Path("/t"), capture_out_dir=Path("/o"),
                project_writable_dirs=[], env_values={},
            )

    def test_rust_policy_includes_cargo_home_and_rustup_home(self):
        text = gsp.build_policy(
            ecosystem="rust", case_id="repo-hyperfine", source_root=Path("/src"), home_dir=Path("/h"),
            tmp_dir=Path("/t"), capture_out_dir=Path("/o"),
            project_writable_dirs=[Path("/src/target")],
            env_values={"CARGO_HOME": "/h/.cargo", "RUSTUP_HOME": "/h/.rustup"},
        )
        self.assertIn("/h/.cargo", text)  # fs_rw
        self.assertIn("/h/.rustup", text)  # fs_ro
        self.assertIn('"CARGO_HOME"', text)
        self.assertIn("tcp_connect = []", text)
        self.assertIn("tcp_bind = []", text)

    def test_network_always_denied_regardless_of_ecosystem(self):
        for ecosystem in gsp.ECOSYSTEM_POLICY_HINTS:
            text = gsp.build_policy(
                ecosystem=ecosystem, case_id="repo-test-case", source_root=Path("/src"), home_dir=Path("/h"),
                tmp_dir=Path("/t"), capture_out_dir=Path("/o"),
                project_writable_dirs=[], env_values={},
            )
            self.assertIn("tcp_connect = []", text)
            self.assertIn("tcp_bind = []", text)

    def test_missing_env_value_is_simply_omitted_not_an_error(self):
        text = gsp.build_policy(
            ecosystem="rust", case_id="repo-test-case", source_root=Path("/src"), home_dir=Path("/h"),
            tmp_dir=Path("/t"), capture_out_dir=Path("/o"),
            project_writable_dirs=[], env_values={},
        )
        self.assertIn("fs_ro", text)

    def test_source_root_is_read_only(self):
        text = gsp.build_policy(
            ecosystem="python", case_id="repo-pyflakes", source_root=Path("/my/source"), home_dir=Path("/h"),
            tmp_dir=Path("/t"), capture_out_dir=Path("/o"),
            project_writable_dirs=[], env_values={},
        )
        fs_ro_line = next(line for line in text.splitlines() if line.startswith("fs_ro"))
        self.assertIn("/my/source", fs_ro_line)

    def test_dotnet_ecosystem_has_policy_hints(self):
        # A real capture (pilot-dotnet-capture-a/b) failed with ValueError:
        # "no policy hints for ecosystem 'dotnet'" -- dotnet was missing from
        # ECOSYSTEM_POLICY_HINTS even though generic_capture.py calls
        # write_policy unconditionally for all 5 ecosystems.
        text = gsp.build_policy(
            ecosystem="dotnet", case_id="repo-test-dotnet-case", source_root=Path("/src"), home_dir=Path("/h"),
            tmp_dir=Path("/t"), capture_out_dir=Path("/o"),
            project_writable_dirs=[], env_values={"DOTNET_ROOT": "/usr/share/dotnet"},
        )
        self.assertIn('"DOTNET_ROOT"', text)
        fs_ro_line = next(line for line in text.splitlines() if line.startswith("fs_ro"))
        self.assertIn("/usr/share/dotnet", fs_ro_line)

    def test_dev_null_is_present_and_writable_for_every_ecosystem(self):
        # A real capture (mvn/gradlew launcher scripts) failed with
        # "cannot create /dev/null: Permission denied" -- /dev/null was
        # never in the policy at all, only /dev/urandom and /dev/random.
        for ecosystem in gsp.ECOSYSTEM_POLICY_HINTS:
            text = gsp.build_policy(
                ecosystem=ecosystem, case_id="repo-test-case", source_root=Path("/src"), home_dir=Path("/h"),
                tmp_dir=Path("/t"), capture_out_dir=Path("/o"),
                project_writable_dirs=[], env_values={},
            )
            fs_rw_line = next(line for line in text.splitlines() if line.startswith("fs_rw"))
            self.assertIn('"/dev/null"', fs_rw_line)

    def test_dev_urandom_and_dev_random_remain_present(self):
        for ecosystem in gsp.ECOSYSTEM_POLICY_HINTS:
            text = gsp.build_policy(
                ecosystem=ecosystem, case_id="repo-test-case", source_root=Path("/src"), home_dir=Path("/h"),
                tmp_dir=Path("/t"), capture_out_dir=Path("/o"),
                project_writable_dirs=[], env_values={},
            )
            fs_ro_line = next(line for line in text.splitlines() if line.startswith("fs_ro"))
            self.assertIn('"/dev/urandom"', fs_ro_line)
            self.assertIn('"/dev/random"', fs_ro_line)

    def test_no_broad_dev_tree_rule_is_introduced(self):
        # Exactly the three named device nodes -- never a bare "/dev" entry
        # that would open the whole device tree.
        for ecosystem in gsp.ECOSYSTEM_POLICY_HINTS:
            text = gsp.build_policy(
                ecosystem=ecosystem, case_id="repo-test-case", source_root=Path("/src"), home_dir=Path("/h"),
                tmp_dir=Path("/t"), capture_out_dir=Path("/o"),
                project_writable_dirs=[], env_values={},
            )
            for line in text.splitlines():
                if line.startswith("fs_ro") or line.startswith("fs_rw"):
                    self.assertNotIn('"/dev"', line)
                    self.assertNotIn('"/dev/"', line)

    def test_python_policy_exposes_exact_venv_root_read_only(self):
        text = gsp.build_policy(
            ecosystem="python", case_id="repo-pyflakes", source_root=Path("/src"), home_dir=Path("/h"),
            tmp_dir=Path("/t"), capture_out_dir=Path("/o"),
            project_writable_dirs=[],
            env_values={"VIRTUAL_ENV": "/runner-temp/venv-repo-pyflakes"},
        )
        fs_ro_line = next(line for line in text.splitlines() if line.startswith("fs_ro"))
        fs_rw_line = next(line for line in text.splitlines() if line.startswith("fs_rw"))
        self.assertIn('"/runner-temp/venv-repo-pyflakes"', fs_ro_line)
        # Read-only: the interpreter never needs to write into its own venv
        # during a capture -- the venv is not made writable.
        self.assertNotIn('"/runner-temp/venv-repo-pyflakes"', fs_rw_line)
        self.assertIn('"VIRTUAL_ENV"', text)

    def test_python_policy_does_not_expose_unrelated_sibling_paths(self):
        # Only the exact venv root for THIS job -- not the whole
        # $RUNNER_TEMP directory tree that houses sibling jobs' venvs too.
        text = gsp.build_policy(
            ecosystem="python", case_id="repo-pyflakes", source_root=Path("/src"), home_dir=Path("/h"),
            tmp_dir=Path("/t"), capture_out_dir=Path("/o"),
            project_writable_dirs=[],
            env_values={"VIRTUAL_ENV": "/runner-temp/venv-repo-pyflakes"},
        )
        fs_ro_line = next(line for line in text.splitlines() if line.startswith("fs_ro"))
        self.assertNotIn('"/runner-temp"]', fs_ro_line)
        self.assertNotIn('"/runner-temp/venv-repo-requests"', fs_ro_line)

    def test_rust_env_allow_includes_rustup_toolchain(self):
        text = gsp.build_policy(
            ecosystem="rust", case_id="repo-hyperfine", source_root=Path("/src"), home_dir=Path("/h"),
            tmp_dir=Path("/t"), capture_out_dir=Path("/o"),
            project_writable_dirs=[], env_values={},
        )
        self.assertIn('"RUSTUP_TOOLCHAIN"', text)

    def test_rust_policy_makes_real_tmp_writable(self):
        # A real capture (CI run #8, once RUSTUP_TOOLCHAIN actually reached
        # the confined process) showed rustc's linker fail with "Cannot
        # create temporary file in /tmp/: Permission denied" -- the system
        # linker hardcodes /tmp regardless of TMPDIR, same class of gap as
        # dotnet's real /tmp/.dotnet/shm finding.
        text = gsp.build_policy(
            ecosystem="rust", case_id="repo-hyperfine", source_root=Path("/src"), home_dir=Path("/h"),
            tmp_dir=Path("/job-tmp"), capture_out_dir=Path("/o"),
            project_writable_dirs=[], env_values={},
        )
        fs_rw_line = next(line for line in text.splitlines() if line.startswith("fs_rw"))
        self.assertIn('"/tmp"', fs_rw_line)

    def test_maven_policy_makes_real_tmp_writable(self):
        # A real capture (CI run #10) showed scala-maven-plugin unpack its
        # compiler-bridge sources jar via a hardcoded /tmp path --
        # "AccessDeniedException: /tmp/scala-maven-plugin-compiler-bridge-
        # sources...", ignoring TMPDIR -- same class of gap as rust's
        # linker and dotnet's /tmp/.dotnet/shm findings.
        text = gsp.build_policy(
            ecosystem="jvm-maven", case_id="repo-docker-java-parser", source_root=Path("/src"), home_dir=Path("/h"),
            tmp_dir=Path("/job-tmp"), capture_out_dir=Path("/o"),
            project_writable_dirs=[], env_values={},
        )
        fs_rw_line = next(line for line in text.splitlines() if line.startswith("fs_rw"))
        self.assertIn('"/tmp"', fs_rw_line)

    def test_dotnet_policy_makes_real_tmp_writable(self):
        # The dotnet CLI's first-run NuGet-migrations named mutex hardcodes
        # /tmp/.dotnet/shm regardless of TMPDIR/HOME -- a real N2-A canary run
        # showed EACCES there until the real system /tmp (not just the job's
        # own dedicated tmp_dir) was made fs_rw.
        text = gsp.build_policy(
            ecosystem="dotnet", case_id="repo-test-dotnet-case", source_root=Path("/src"), home_dir=Path("/h"),
            tmp_dir=Path("/job-tmp"), capture_out_dir=Path("/o"),
            project_writable_dirs=[], env_values={},
        )
        fs_rw_line = next(line for line in text.splitlines() if line.startswith("fs_rw"))
        self.assertIn('"/tmp"', fs_rw_line)


class TestNetworkEnforcementModeIsCaseIdScoped(unittest.TestCase):
    """D1b authorization (2026-07-16): network_enforcement_mode is selected
    by explicit case_id (gsp.NETWORK_ENFORCEMENT_AUTHORIZED_CASES), never by
    ecosystem alone -- a case's ecosystem sharing an authorized sibling case
    (e.g. another jvm-gradle case, or another dotnet case) must NOT itself
    receive the exception."""

    def _build(self, *, ecosystem: str, case_id: str) -> str:
        return gsp.build_policy(
            ecosystem=ecosystem, case_id=case_id, source_root=Path("/src"), home_dir=Path("/h"),
            tmp_dir=Path("/t"), capture_out_dir=Path("/o"),
            project_writable_dirs=[], env_values={},
        )

    def test_repo_spotless_receives_the_exception(self):
        text = self._build(ecosystem="jvm-gradle", case_id="repo-spotless")
        self.assertIn('network_enforcement_mode = "outer-netns-loopback-only"', text)
        self.assertIn("tcp_connect = []", text)
        self.assertIn("tcp_bind = []", text)

    def test_repo_kubeops_generator_receives_the_exception(self):
        text = self._build(ecosystem="dotnet", case_id="repo-kubeops-generator")
        self.assertIn('network_enforcement_mode = "outer-netns-loopback-only"', text)
        self.assertIn("tcp_connect = []", text)
        self.assertIn("tcp_bind = []", text)

    def test_another_dotnet_case_does_not_receive_it(self):
        # Same ecosystem as repo-kubeops-generator, different case_id --
        # "Do not grant it to the entire dotnet ecosystem."
        text = self._build(ecosystem="dotnet", case_id="repo-some-other-dotnet-case")
        self.assertNotIn("network_enforcement_mode", text)

    def test_another_gradle_case_does_not_receive_it(self):
        # Same ecosystem as repo-spotless (a later Stage-2 gradle case,
        # e.g. repo-moshi) -- not authorized just because its ecosystem
        # sibling is; a later case needing this must stop for separate
        # D1b review, per the policy's own narrow-scope discipline.
        text = self._build(ecosystem="jvm-gradle", case_id="repo-moshi")
        self.assertNotIn("network_enforcement_mode", text)

    def test_an_unauthorized_case_cannot_request_it_via_any_ecosystem(self):
        for ecosystem in gsp.ECOSYSTEM_POLICY_HINTS:
            text = self._build(ecosystem=ecosystem, case_id="repo-totally-unauthorized-case")
            self.assertNotIn("network_enforcement_mode", text, f"ecosystem={ecosystem}")

    def test_only_the_two_authorized_cases_exist(self):
        self.assertEqual(
            set(gsp.NETWORK_ENFORCEMENT_AUTHORIZED_CASES),
            {"repo-spotless", "repo-kubeops-generator"},
        )

    def test_both_authorized_cases_grant_read_access_to_probe_script_directories(self):
        # Real evidence (CI run #14): the network-enforcement probes
        # (canary/tools/network_probe.py, d1-identity-lock/tools/
        # loopback_bind_probe.py) run through the SAME policy as the real
        # capture, but that policy never granted read access to the 007
        # checkout's own tools directories -- confined python3 failed with
        # "can't open file '.../network_probe.py': [Errno 13] Permission
        # denied" (exit code 2, Python's own convention for a script it
        # can't open -- not Sandboy's).
        for ecosystem, case_id in (("jvm-gradle", "repo-spotless"), ("dotnet", "repo-kubeops-generator")):
            text = self._build(ecosystem=ecosystem, case_id=case_id)
            fs_ro_line = next(line for line in text.splitlines() if line.startswith("fs_ro"))
            self.assertIn(str(gsp.CANARY_TOOLS_DIR), fs_ro_line)
            self.assertIn(str(gsp.TOOLS_DIR), fs_ro_line)

    def test_unauthorized_case_does_not_get_probe_script_read_access(self):
        text = self._build(ecosystem="dotnet", case_id="repo-some-other-dotnet-case")
        fs_ro_line = next(line for line in text.splitlines() if line.startswith("fs_ro"))
        self.assertNotIn(str(gsp.CANARY_TOOLS_DIR), fs_ro_line)


class TestWritePolicy(unittest.TestCase):
    def test_write_policy_produces_stable_canonical_hash_independent_of_workdir(self):
        with tempfile.TemporaryDirectory() as tmp1, tempfile.TemporaryDirectory() as tmp2:
            work_dir1, work_dir2 = Path(tmp1), Path(tmp2)
            raw1, canon1 = gsp.write_policy(
                work_dir1 / "policy.toml", work_dir=work_dir1, ecosystem="python", case_id="repo-pyflakes",
                source_root=work_dir1 / "src", home_dir=work_dir1 / "home",
                tmp_dir=work_dir1 / "tmp", capture_out_dir=work_dir1 / "out",
                project_writable_dirs=[], env_values={},
            )
            raw2, canon2 = gsp.write_policy(
                work_dir2 / "policy.toml", work_dir=work_dir2, ecosystem="python", case_id="repo-pyflakes",
                source_root=work_dir2 / "src", home_dir=work_dir2 / "home",
                tmp_dir=work_dir2 / "tmp", capture_out_dir=work_dir2 / "out",
                project_writable_dirs=[], env_values={},
            )
            self.assertNotEqual(raw1, raw2)  # different absolute paths
            self.assertEqual(canon1, canon2)  # same structural policy


if __name__ == "__main__":
    unittest.main()
