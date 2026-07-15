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
                ecosystem="cobol", source_root=Path("/x"), home_dir=Path("/h"),
                tmp_dir=Path("/t"), capture_out_dir=Path("/o"),
                project_writable_dirs=[], env_values={},
            )

    def test_rust_policy_includes_cargo_home_and_rustup_home(self):
        text = gsp.build_policy(
            ecosystem="rust", source_root=Path("/src"), home_dir=Path("/h"),
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
                ecosystem=ecosystem, source_root=Path("/src"), home_dir=Path("/h"),
                tmp_dir=Path("/t"), capture_out_dir=Path("/o"),
                project_writable_dirs=[], env_values={},
            )
            self.assertIn("tcp_connect = []", text)
            self.assertIn("tcp_bind = []", text)

    def test_missing_env_value_is_simply_omitted_not_an_error(self):
        text = gsp.build_policy(
            ecosystem="rust", source_root=Path("/src"), home_dir=Path("/h"),
            tmp_dir=Path("/t"), capture_out_dir=Path("/o"),
            project_writable_dirs=[], env_values={},
        )
        self.assertIn("fs_ro", text)

    def test_source_root_is_read_only(self):
        text = gsp.build_policy(
            ecosystem="python", source_root=Path("/my/source"), home_dir=Path("/h"),
            tmp_dir=Path("/t"), capture_out_dir=Path("/o"),
            project_writable_dirs=[], env_values={},
        )
        fs_ro_line = next(line for line in text.splitlines() if line.startswith("fs_ro"))
        self.assertIn("/my/source", fs_ro_line)


class TestWritePolicy(unittest.TestCase):
    def test_write_policy_produces_stable_canonical_hash_independent_of_workdir(self):
        with tempfile.TemporaryDirectory() as tmp1, tempfile.TemporaryDirectory() as tmp2:
            work_dir1, work_dir2 = Path(tmp1), Path(tmp2)
            raw1, canon1 = gsp.write_policy(
                work_dir1 / "policy.toml", work_dir=work_dir1, ecosystem="python",
                source_root=work_dir1 / "src", home_dir=work_dir1 / "home",
                tmp_dir=work_dir1 / "tmp", capture_out_dir=work_dir1 / "out",
                project_writable_dirs=[], env_values={},
            )
            raw2, canon2 = gsp.write_policy(
                work_dir2 / "policy.toml", work_dir=work_dir2, ecosystem="python",
                source_root=work_dir2 / "src", home_dir=work_dir2 / "home",
                tmp_dir=work_dir2 / "tmp", capture_out_dir=work_dir2 / "out",
                project_writable_dirs=[], env_values={},
            )
            self.assertNotEqual(raw1, raw2)  # different absolute paths
            self.assertEqual(canon1, canon2)  # same structural policy


if __name__ == "__main__":
    unittest.main()
