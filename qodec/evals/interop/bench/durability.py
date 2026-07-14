"""Crash-durable journaling for the Level-2 matrix.

A 7B CPU run is long; a crash in pass 2 must not lose pass 1. So records are an
append-only journal, flushed after every request and fsync'd periodically, and
`--resume` skips already-completed keys. State files are written atomically
(tmp + fsync + os.replace + parent-dir fsync) so a crash — process *or* power
loss — never leaves a half-written meta.json and a committed record survives.

`atomic_write(..., fsync=True)` is durable against machine crash / power loss:
the temp file's data and the parent directory's rename are both fsync'd before
the call returns. `fsync=False` still gives an atomic replace (no partial
readers, safe against a process crash) but not power-loss persistence — used for
the run-state progress receipt, whose truth is the journal anyway.
"""

from __future__ import annotations

import json
import os
from pathlib import Path


class JournalCorruption(RuntimeError):
    """A completed (newline-terminated) journal line is unreadable — malformed
    JSON, invalid UTF-8, a missing key field, or a duplicate key. Only a single
    torn *trailing* fragment (a crash mid-write) is tolerated; anything else
    means the journal is not the append-only log we wrote, so we refuse it
    rather than silently drop or double-count records."""


class DuplicateKey(RuntimeError):
    """append() was handed a key already in the journal."""


def _fsync_dir(directory: Path) -> None:
    """fsync a directory so a rename into it survives power loss. Not every
    filesystem supports it; a failure here is not fatal."""
    try:
        fd = os.open(str(directory), os.O_RDONLY)
    except OSError:
        return
    try:
        os.fsync(fd)
    except OSError:
        pass
    finally:
        os.close(fd)


def atomic_write(path: Path, text: str, *, fsync: bool = True) -> None:
    """Write via a temp file + atomic rename, so readers never see a partial
    file and a crash mid-write leaves the previous version intact. With
    `fsync=True` the temp file's bytes and the parent-directory rename are
    flushed to disk, making the write durable against power loss too."""
    tmp = path.with_name(path.name + ".tmp")
    with tmp.open("w", encoding="utf-8", newline="\n") as fh:
        fh.write(text)
        fh.flush()
        if fsync:
            os.fsync(fh.fileno())
    os.replace(tmp, path)
    if fsync:
        _fsync_dir(path.parent)


class RecordLog:
    """Append-only `records.jsonl`.

    - `append` writes one record, flushes immediately, fsyncs every N, and
      refuses a key already present (a caller that forgot `has()` cannot
      double-count).
    - `load_existing` (resume) validates every completed line strictly and
      TRUNCATES a single torn final line (a partial write from a crash) so
      appends stay well-formed. It raises `JournalCorruption` on any malformed
      *complete* line rather than skip it silently.
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
        cut = raw.rfind(b"\n") + 1          # everything up to here is whole, \n-terminated
        # The completed prefix must be valid UTF-8; a bad byte there is real
        # corruption (the torn tail, which may be cut mid-character, is exempt).
        try:
            good = raw[:cut].decode("utf-8")
        except UnicodeDecodeError as exc:
            raise JournalCorruption(f"invalid UTF-8 in completed journal region: {exc}") from exc
        for line in good.splitlines():
            line = line.strip()
            if not line:
                continue
            try:
                r = json.loads(line)
            except json.JSONDecodeError as exc:
                raise JournalCorruption(f"malformed complete line: {exc}: {line[:120]!r}") from exc
            try:
                key = self._key(r)
            except (KeyError, TypeError) as exc:
                raise JournalCorruption(f"record missing key field {exc}: {line[:120]!r}") from exc
            if key in self.completed:
                raise JournalCorruption(f"duplicate key in journal: {key}")
            self.completed.add(key)
            self.records.append(r)
        if cut != len(raw):                 # drop the single torn trailing partial line
            with self.path.open("r+b") as fh:
                fh.truncate(cut)

    def open(self) -> None:
        self._fh = self.path.open("a", encoding="utf-8", newline="\n")

    def has(self, key: tuple) -> bool:
        return key in self.completed

    def sync(self) -> None:
        """Force flush + fsync now, regardless of `sync_every`, so the journal on
        disk is a known-durable prefix. Called before a pass-1 receipt pins the
        prefix bytes + hash, so the receipt describes bytes that survive power
        loss — not just OS buffers."""
        if self._fh is None:
            raise RuntimeError("RecordLog.open() not called")
        self._fh.flush()
        os.fsync(self._fh.fileno())
        self._since = 0

    def append(self, r: dict) -> None:
        if self._fh is None:
            raise RuntimeError("RecordLog.open() not called")
        key = self._key(r)
        if key in self.completed:
            raise DuplicateKey(f"refusing duplicate key {key}")
        self._fh.write(json.dumps(r, ensure_ascii=False) + "\n")
        self._fh.flush()
        self.completed.add(key)
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
