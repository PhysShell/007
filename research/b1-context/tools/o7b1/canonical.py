"""Canonical serialization + hashing.

One and only one serialization policy is used for every digest-bound artifact in
this vertical, so that "same input -> byte-identical output" is a property of the
code rather than a hope:

  - UTF-8, no BOM;
  - object keys sorted (stable regardless of construction order);
  - compact separators (no incidental whitespace);
  - ensure_ascii=False so human-readable text stays human-readable and the byte
    stream is stable (ASCII-escaping and non-escaping are both deterministic, we
    pick non-escaping once and keep it);
  - LF newlines only; exactly one trailing LF on whole-file artifacts;
  - JSONL: one canonical object per line, each terminated by a single LF.

Floats are re-emitted via Python's shortest round-trip repr, which is
deterministic within an interpreter; native source timestamps are preserved
as-is rather than reformatted, so we never invent precision.
"""
from __future__ import annotations

import hashlib
import json
from typing import Any, Iterable


def canonical_bytes(obj: Any) -> bytes:
    """Canonical JSON for a single object, WITHOUT a trailing newline."""
    text = json.dumps(obj, ensure_ascii=False, sort_keys=True, separators=(",", ":"))
    return text.encode("utf-8")


def canonical_file_bytes(obj: Any) -> bytes:
    """Canonical JSON for a whole-file artifact, with exactly one trailing LF."""
    return canonical_bytes(obj) + b"\n"


def jsonl_bytes(records: Iterable[Any]) -> bytes:
    """Canonical JSONL: one object per line, single LF after each (incl. last)."""
    out = bytearray()
    for rec in records:
        out += canonical_bytes(rec)
        out += b"\n"
    return bytes(out)


def sha256_hex(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def sha256_of_file(path: str) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as fh:
        for chunk in iter(lambda: fh.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()
