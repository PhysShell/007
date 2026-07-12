"""Run a producer or transform and capture full provenance.

Every artifact the bench keeps is paired with exactly how it was made: argv,
cwd, exit code, wall time, tool version, repo SHA. That is what turns a number
into a reproducible measurement rather than a claim.
"""

from __future__ import annotations

import subprocess
import time
from dataclasses import dataclass, field
from pathlib import Path

from . import lockfiles


@dataclass
class Executed:
    text: str
    tool: str
    argv: list[str]
    cwd: str
    exit_code: int
    wall_ms: float
    tool_version: str | None = None
    repo_sha: str | None = None
    extra: dict = field(default_factory=dict)

    def provenance(self) -> dict:
        return {
            "tool": self.tool,
            "argv": self.argv,
            "cwd": self.cwd,
            "exit_code": self.exit_code,
            "wall_ms": round(self.wall_ms, 2),
            "tool_version": self.tool_version,
            "repo_sha": self.repo_sha,
            **self.extra,
        }


class ExecutionError(RuntimeError):
    pass


def run(argv: list[str], *, cwd: Path | None = None, stdin: str | None = None,
        tool: str = "cmd", version: str | None = None, repo_sha: str | None = None) -> Executed:
    started = time.perf_counter()
    try:
        proc = subprocess.run(
            argv, cwd=str(cwd) if cwd else None, input=stdin,
            capture_output=True, text=True, check=False,
        )
    except OSError as exc:
        raise ExecutionError(f"could not run {argv!r}: {exc}") from exc
    ms = (time.perf_counter() - started) * 1000.0
    if proc.returncode != 0:
        raise ExecutionError(
            f"{tool} exited {proc.returncode}: {' '.join(argv)}\n{proc.stderr.strip()[:500]}"
        )
    return Executed(
        text=proc.stdout, tool=tool, argv=argv,
        cwd=str(cwd) if cwd else ".", exit_code=proc.returncode,
        wall_ms=ms, tool_version=version, repo_sha=repo_sha,
    )


def repo_head(repo: lockfiles.Repo) -> str | None:
    d = repo.clone_dir()
    if not (d / ".git").exists():
        return None
    try:
        out = subprocess.run(
            ["git", "rev-parse", "HEAD"], cwd=str(d),
            capture_output=True, text=True, check=False,
        )
    except OSError:
        return None
    return out.stdout.strip() or None
