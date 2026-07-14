"""Deterministic hashing helpers for corpus bundles.

All hashes are SHA256. JSON is hashed in a canonical form (sorted keys, tight
separators, UTF-8) so semantically equal documents hash equally. Directory
("tree") hashes are computed over a sorted list of (relative-path, mode, blob)
entries so a fixture tree has one stable identity independent of filesystem
iteration order.
"""
from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def sha256_file(path: str | os.PathLike) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as fh:
        for chunk in iter(lambda: fh.read(65536), b""):
            h.update(chunk)
    return h.hexdigest()


def canonical_json_bytes(obj) -> bytes:
    return json.dumps(obj, sort_keys=True, separators=(",", ":"), ensure_ascii=True).encode("utf-8")


def sha256_json(obj) -> str:
    return sha256_bytes(canonical_json_bytes(obj))


def sha256_json_file(path: str | os.PathLike) -> str:
    with open(path, "r", encoding="utf-8") as fh:
        return sha256_json(json.load(fh))


def tree_sha256(root: str | os.PathLike) -> str:
    """Stable SHA256 of a directory tree (regular files + symlink targets).

    Entries are sorted by relative POSIX path. Each entry contributes its path,
    an executable-bit flag, and its content hash. An empty (or absent) tree
    hashes to the digest of the fixed header only, which is still stable.
    """
    root = Path(root)
    h = hashlib.sha256()
    h.update(b"corpus-tree-v1\n")
    if not root.exists():
        return h.hexdigest()
    entries = []
    for dirpath, dirnames, filenames in os.walk(root):
        dirnames.sort()
        for name in sorted(filenames):
            p = Path(dirpath) / name
            rel = p.relative_to(root).as_posix()
            entries.append((rel, p))
    for rel, p in sorted(entries, key=lambda e: e[0]):
        if p.is_symlink():
            kind = b"L"
            content = os.readlink(p).encode("utf-8")
            payload = sha256_bytes(content)
        else:
            kind = b"F"
            payload = sha256_file(p)
        execbit = b"1" if (not p.is_symlink() and os.access(p, os.X_OK)) else b"0"
        h.update(kind + b" " + execbit + b" " + rel.encode("utf-8") + b" " + payload.encode("ascii") + b"\n")
    return h.hexdigest()
