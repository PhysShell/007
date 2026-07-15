#!/usr/bin/env python3
"""N2-C archive security checks (section 15). Applied to every downloaded
non-repository artifact (and, defensively, to any tar/zip we build
ourselves) before it is trusted as N2-C evidence. Static inspection of
archive member metadata only — never extracts-and-executes, never opens an
encrypted archive with a guessed/embedded password.
"""
from __future__ import annotations

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
            if target.startswith("/") or _is_traversal_or_absolute(target):
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
