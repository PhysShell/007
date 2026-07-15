"""Section 23 tests for archive_security.py: traversal, absolute-path,
device-file, unsafe-symlink, and encrypted-member rejection."""
import io
import sys
import tarfile
import unittest
import zipfile
from pathlib import Path

TOOLS = Path(__file__).resolve().parents[1] / "tools"
sys.path.insert(0, str(TOOLS))
import archive_security  # noqa: E402


def _make_tar(tmp_path: Path, members: list) -> Path:
    path = tmp_path / "test.tar"
    with tarfile.open(path, mode="w") as tar:
        for name, data, kwargs in members:
            info = tarfile.TarInfo(name)
            info.size = len(data)
            for k, v in kwargs.items():
                setattr(info, k, v)
            tar.addfile(info, io.BytesIO(data))
    return path


class TestTarSecurity(unittest.TestCase):
    def setUp(self):
        import tempfile
        self._tmpdir = tempfile.TemporaryDirectory()
        self.tmp_path = Path(self._tmpdir.name)

    def tearDown(self):
        self._tmpdir.cleanup()

    def test_safe_tar_passes(self):
        path = _make_tar(self.tmp_path, [("foo/bar.txt", b"hello", {})])
        findings = archive_security.assert_safe(path)
        self.assertEqual(findings["absolute_or_traversal_paths"], [])

    def test_path_traversal_rejected(self):
        path = _make_tar(self.tmp_path, [("../../etc/passwd", b"evil", {})])
        with self.assertRaises(archive_security.RejectedArchive):
            archive_security.assert_safe(path)

    def test_absolute_path_rejected(self):
        path = _make_tar(self.tmp_path, [("/etc/passwd", b"evil", {})])
        with self.assertRaises(archive_security.RejectedArchive):
            archive_security.assert_safe(path)

    def test_windows_drive_absolute_path_rejected(self):
        path = _make_tar(self.tmp_path, [("C:\\Windows\\System32\\evil.dll", b"evil", {})])
        with self.assertRaises(archive_security.RejectedArchive):
            archive_security.assert_safe(path)

    def test_symlink_escaping_root_rejected(self):
        path = _make_tar(self.tmp_path, [
            ("link", b"", {"type": tarfile.SYMTYPE, "linkname": "/etc/passwd"}),
        ])
        with self.assertRaises(archive_security.RejectedArchive):
            archive_security.assert_safe(path)

    def test_safe_relative_symlink_passes(self):
        path = _make_tar(self.tmp_path, [
            ("foo.txt", b"hi", {}),
            ("link", b"", {"type": tarfile.SYMTYPE, "linkname": "foo.txt"}),
        ])
        findings = archive_security.assert_safe(path)
        self.assertEqual(findings["unsafe_symlinks"], [])

    def test_relative_symlink_with_dotdot_that_stays_inside_root_passes(self):
        # Regression: a symlink target containing ".." is not automatically
        # an escape — it must be resolved against the symlink's OWN parent
        # directory. "docs/static/img/logo.png" -> "../../../res/logo.png"
        # resolves to "res/logo.png", still inside the archive root.
        path = _make_tar(self.tmp_path, [
            ("res/logo.png", b"png", {}),
            ("docs/static/img/logo.png", b"", {"type": tarfile.SYMTYPE, "linkname": "../../../res/logo.png"}),
        ])
        findings = archive_security.assert_safe(path)
        self.assertEqual(findings["unsafe_symlinks"], [])

    def test_relative_symlink_that_actually_escapes_root_rejected(self):
        # One more ".." than needed to reach the root escapes above it.
        path = _make_tar(self.tmp_path, [
            ("docs/static/img/logo.png", b"", {"type": tarfile.SYMTYPE, "linkname": "../../../../res/logo.png"}),
        ])
        with self.assertRaises(archive_security.RejectedArchive):
            archive_security.assert_safe(path)


class TestZipSecurity(unittest.TestCase):
    def setUp(self):
        import tempfile
        self._tmpdir = tempfile.TemporaryDirectory()
        self.tmp_path = Path(self._tmpdir.name)

    def tearDown(self):
        self._tmpdir.cleanup()

    def test_safe_zip_passes(self):
        path = self.tmp_path / "test.zip"
        with zipfile.ZipFile(path, "w") as zf:
            zf.writestr("foo.txt", "hello")
        findings = archive_security.assert_safe(path)
        self.assertEqual(findings["absolute_or_traversal_paths"], [])

    def test_zip_traversal_rejected(self):
        path = self.tmp_path / "evil.zip"
        with zipfile.ZipFile(path, "w") as zf:
            zf.writestr("../../etc/passwd", "evil")
        with self.assertRaises(archive_security.RejectedArchive):
            archive_security.assert_safe(path)

    def test_encrypted_zip_member_rejected(self):
        # zipfile's writer can't natively produce an AES/ZipCrypto-encrypted
        # member, so this tests inspect_zip's encrypted-flag check directly
        # against a real ZipInfo with the general-purpose bit-0 (encrypted)
        # flag set, rather than fabricating fragile raw bytes.
        from unittest import mock

        path = self.tmp_path / "plain.zip"
        with zipfile.ZipFile(path, "w") as zf:
            zf.writestr("secret.txt", "hidden")

        encrypted_info = zipfile.ZipInfo("secret.txt")
        encrypted_info.flag_bits |= 0x1
        with mock.patch.object(zipfile.ZipFile, "infolist", return_value=[encrypted_info]):
            findings = archive_security.inspect_zip(path)
        self.assertEqual(findings["encrypted_members"], ["secret.txt"])
        with self.assertRaises(archive_security.RejectedArchive):
            with mock.patch.object(zipfile.ZipFile, "infolist", return_value=[encrypted_info]):
                archive_security.assert_safe(path)


if __name__ == "__main__":
    unittest.main()
