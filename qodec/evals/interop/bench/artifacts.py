"""Persist every source and intermediate artifact of a case/arm, hashed.

The interop bench claims reproducibility, so it must keep the evidence, not
only the aggregates. For each case/arm we write:

    cases/<case>/<arm>/
      producer.txt        raw producer output
      transformed.txt     output after the non-qodec transforms (qodec's input)
      qodec-envelope.json the adapter envelope
      qodec-content.txt   what the reader receives (artifact or passthrough)
      decoded.txt         qodec-content.txt decoded back
      meta.json           argv, cwd, versions, SHA, exit codes, timings,
                          sha256 + bytes of every file, tokens, codec, roundtrip
"""

from __future__ import annotations

import hashlib
import json
from pathlib import Path


def sha256(text: str) -> str:
    return "sha256:" + hashlib.sha256(text.encode("utf-8")).hexdigest()


class ArtifactDir:
    """Writes files under cases/<case>/<arm>/ and records each one's digest."""

    def __init__(self, root: Path, case_id: str, arm: str):
        self.dir = root / "cases" / case_id / arm
        self.dir.mkdir(parents=True, exist_ok=True)
        self.files: dict[str, dict] = {}

    def write(self, name: str, text: str) -> str:
        (self.dir / name).write_text(text)
        digest = sha256(text)
        self.files[name] = {"sha256": digest, "bytes": len(text.encode("utf-8"))}
        return digest

    def write_meta(self, meta: dict) -> None:
        meta = {**meta, "artifacts": self.files}
        (self.dir / "meta.json").write_text(json.dumps(meta, indent=2) + "\n")
