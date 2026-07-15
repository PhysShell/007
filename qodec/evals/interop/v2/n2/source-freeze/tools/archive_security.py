#!/usr/bin/env python3
"""N2-C archive security checks (section 15). Applied to every downloaded
non-repository artifact (and, defensively, to any tar/zip we build
ourselves) before it is trusted as N2-C evidence. Static inspection of
archive member metadata only — never extracts-and-executes, never opens an
encrypted archive with a guessed/embedded password.
"""
from __future__ import annotations

import posixpath
import tarfile
import zipfile
from pathlib import Path


class RejectedArchive(Exception):
    """Raised for any failed archive-security check; message is the reason."""


def _is_traversal_or_absolute(member_name: str) -> bool:
    if member_name.startswith("/") or member_name.startswith("\\"):
        return True
    if ":" in member_name[:3]:  # e.g. "C:\..." Windows drive-absolute
        return True
    parts = member_name.replace("\\", "/").split("/")
    return ".." in parts


def _symlink_escapes_root(member_name: str, target: str) -> bool:
    """A relative symlink target containing '..' does not necessarily escape
    the archive root — it only escapes if, resolved against the symlink's
    OWN parent directory (not the archive root), it lands above the root.
    Many legitimate same-repository symlinks (test fixtures, shared assets)
    use '../' to reach a sibling directory a few levels up without ever
    leaving the archive."""
    target_posix = target.replace("\\", "/")
    if target_posix.startswith("/"):
        return True
    if ":" in target_posix[:3]:  # Windows drive-absolute target
        return True
    member_posix = member_name.replace("\\", "/")
    parent = posixpath.dirname(member_posix)
    resolved = posixpath.normpath(posixpath.join(parent, target_posix))
    return resolved.startswith("../") or resolved == ".." or resolved.startswith("/")


def inspect_tar(path: Path) -> dict:
    findings = {"absolute_or_traversal_paths": [], "device_files": [], "unsafe_symlinks": [], "member_count": 0}
    try:
        with tarfile.open(path, mode="r:*") as tar:
            members = tar.getmembers()
    except tarfile.ReadError as e:
        raise RejectedArchive(f"cannot read tar archive {path}: {e}") from e
    findings["member_count"] = len(members)
    for m in members:
        if _is_traversal_or_absolute(m.name):
            findings["absolute_or_traversal_paths"].append(m.name)
        if m.isdev() or m.ischr() or m.isblk() or m.isfifo():
            findings["device_files"].append(m.name)
        if m.issym() or m.islnk():
            target = m.linkname
            if _symlink_escapes_root(m.name, target):
                findings["unsafe_symlinks"].append({"name": m.name, "target": target})
    return findings


def inspect_zip(path: Path) -> dict:
    findings = {"absolute_or_traversal_paths": [], "device_files": [], "unsafe_symlinks": [],
                "encrypted_members": [], "member_count": 0}
    try:
        with zipfile.ZipFile(path) as zf:
            infos = zf.infolist()
    except zipfile.BadZipFile as e:
        raise RejectedArchive(f"cannot read zip archive {path}: {e}") from e
    findings["member_count"] = len(infos)
    for info in infos:
        if _is_traversal_or_absolute(info.filename):
            findings["absolute_or_traversal_paths"].append(info.filename)
        # bit 0x1 of the general-purpose flag = encrypted member
        if info.flag_bits & 0x1:
            findings["encrypted_members"].append(info.filename)
    return findings


def assert_safe(path: Path) -> dict:
    """Raises RejectedArchive if any hard-reject condition (section 15) is
    found; otherwise returns the findings dict (always empty lists)."""
    suffix = path.suffix.lower()
    if suffix in (".tar", ".gz", ".tgz", ".bz2") or path.name.endswith(".tar.gz") or path.name.endswith(".tar.bz2"):
        findings = inspect_tar(path)
    elif suffix == ".zip":
        findings = inspect_zip(path)
    else:
        raise RejectedArchive(f"unrecognized archive type for {path} (suffix {suffix!r})")

    if findings.get("absolute_or_traversal_paths"):
        raise RejectedArchive(f"{path}: absolute or path-traversal archive members: {findings['absolute_or_traversal_paths']}")
    if findings.get("device_files"):
        raise RejectedArchive(f"{path}: device/fifo/block-special archive members: {findings['device_files']}")
    if findings.get("unsafe_symlinks"):
        raise RejectedArchive(f"{path}: symlinks escaping archive root: {findings['unsafe_symlinks']}")
    if findings.get("encrypted_members"):
        raise RejectedArchive(f"{path}: password-protected/encrypted archive members: {findings['encrypted_members']}")
    return findings


# Minimum plausible size for a "real" acquired source file — well below any
# genuine log/dataset/repo-tar content, but enough to reject a truncated or
# accidentally-empty placeholder.
_MIN_REAL_CONTENT_BYTES = 1


def find_content_only_acquisitions(acquisition_root: Path) -> list[str]:
    """Section 11: 'The artifact-contract job must fail if the acquisition
    artifact contains only a receipt where actual source bytes are
    required.' Returns the list of candidate_ids under acquisition_root
    whose directory holds ONLY a receipt/manifest (acquisition-receipt.json,
    source-file-manifest.json, <id>.acquisition.json) with no real content
    file (source.tar for repository-execution, or a non-empty source/
    directory for every other source kind)."""
    offenders = []
    for case_dir in sorted(acquisition_root.iterdir()):
        if not case_dir.is_dir():
            continue
        has_repo_tar = (case_dir / "source.tar").is_file() and (case_dir / "source.tar").stat().st_size >= _MIN_REAL_CONTENT_BYTES
        source_dir = case_dir / "source"
        has_source_dir_content = source_dir.is_dir() and any(
            f.is_file() and f.stat().st_size >= _MIN_REAL_CONTENT_BYTES for f in source_dir.rglob("*")
        )
        if not has_repo_tar and not has_source_dir_content:
            offenders.append(case_dir.name)
    return offenders
