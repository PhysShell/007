"""Crash-durable journaling for the Level-2 matrix.

A 7B CPU run is long; a crash in pass 2 must not lose pass 1. So records are an
append-only journal, flushed after every request and fsync'd periodically, and
`--resume` skips already-completed keys. State files are written atomically
(tmp + os.replace) so a crash never leaves a half-written meta.json.
"""

from __future__ import annotations

import json
import os
from pathlib import Path


def atomic_write(path: Path, text: str) -> None:
    """Write via a temp file + atomic rename, so readers never see a partial
    file and a crash mid-write leaves the previous version intact."""
    tmp = path.with_name(path.name + ".tmp")
    tmp.write_text(text)
    os.replace(tmp, path)


class RecordLog:
    """Append-only `records.jsonl`.

    - `append` writes one record, flushes immediately, fsyncs every N.
    - `load_existing` (resume) reads completed keys and TRUNCATES a torn final
      line (a partial write from a crash) so appends stay well-formed.
    - `has(key)` lets the runner skip completed work, so a resumed run never
      duplicates a request.
    """

    def __init__(self, path: Path, key_fields=("case", "question", "arm", "repeat"),
                 sync_every: int = 5):
        self.path = path
        self.key_fields = key_fields
        self.sync_every = sync_every
        self.completed: set[tuple] = set()
        self.records: list[dict] = []
        self._since = 0
        self._fh = None

    def _key(self, r: dict) -> tuple:
        return tuple(r[f] for f in self.key_fields)

    def load_existing(self) -> None:
        if not self.path.exists():
            return
        raw = self.path.read_bytes()
        cut = raw.rfind(b"\n") + 1          # keep only whole, newline-terminated lines
        good = raw[:cut].decode("utf-8", "replace")
        for line in good.splitlines():
            line = line.strip()
            if not line:
                continue
            try:
                r = json.loads(line)
            except json.JSONDecodeError:
                continue
            self.completed.add(self._key(r))
            self.records.append(r)
        if cut != len(raw):                 # drop a torn trailing partial line
            with self.path.open("r+b") as fh:
                fh.truncate(cut)

    def open(self) -> None:
        self._fh = self.path.open("a")

    def has(self, key: tuple) -> bool:
        return key in self.completed

    def append(self, r: dict) -> None:
        if self._fh is None:
            raise RuntimeError("RecordLog.open() not called")
        self._fh.write(json.dumps(r, ensure_ascii=False) + "\n")
        self._fh.flush()
        self.completed.add(self._key(r))
        self.records.append(r)
        self._since += 1
        if self._since >= self.sync_every:
            os.fsync(self._fh.fileno())
            self._since = 0

    def close(self) -> None:
        if self._fh is not None:
            os.fsync(self._fh.fileno())
            self._fh.close()
            self._fh = None
