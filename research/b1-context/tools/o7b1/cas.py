"""Read-only accessor for the owner's external local content-addressed store.

Layout mirrors the `o7-cas` helper: objects live at
`<root>/sha256/<aa>/<rest>` and are addressed by the sha256 of their bytes.

This module only ever READS. It never writes to the store, and it refuses to
hand back bytes whose recomputed digest does not match the requested address —
the CAS guarantee is "these bytes match this address", enforced here on every
access rather than trusted.
"""
from __future__ import annotations

import os
from dataclasses import dataclass

from .canonical import sha256_of_file


class BlobUnavailable(Exception):
    """Requested digest is not present in the store (fixture UNAVAILABLE)."""


class BlobDigestMismatch(Exception):
    """Stored object's bytes do not match the requested address (fail closed)."""


def _norm(digest: str) -> str:
    d = digest
    if d.startswith("cas:"):
        d = d[len("cas:"):]
    if d.startswith("sha256:"):
        d = d[len("sha256:"):]
    return d


@dataclass(frozen=True)
class Cas:
    root: str  # e.g. ~/.local/share/o7-research/cas

    def path_for(self, digest: str) -> str:
        d = _norm(digest)
        return os.path.join(self.root, "sha256", d[:2], d[2:])

    def has(self, digest: str) -> bool:
        return os.path.isfile(self.path_for(digest))

    def resolve(self, digest: str, *, expected_size: int | None = None) -> str:
        """Return the on-disk path for a digest, verifying digest (+ optional size).

        Raises BlobUnavailable if missing, BlobDigestMismatch if the bytes do not
        hash to the requested address or the size is wrong.
        """
        d = _norm(digest)
        p = self.path_for(d)
        if not os.path.isfile(p):
            raise BlobUnavailable(d)
        if expected_size is not None:
            actual = os.path.getsize(p)
            if actual != expected_size:
                raise BlobDigestMismatch(
                    f"{d}: size {actual} != expected {expected_size}"
                )
        actual_digest = sha256_of_file(p)
        if actual_digest != d:
            raise BlobDigestMismatch(f"{d}: stored bytes hash to {actual_digest}")
        return p

    def read_bytes(self, digest: str, *, expected_size: int | None = None) -> bytes:
        with open(self.resolve(digest, expected_size=expected_size), "rb") as fh:
            return fh.read()
