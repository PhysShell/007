"""CodeGraph adapter — retrieval lane.

CodeGraph builds `.codegraph/` via `codegraph init` (which builds the full
graph; a separate `codegraph index` is no longer needed) and answers
`codegraph_explore`. qodec sits *after* an explore call. The design doc's
hypothesis: explore output repeats paths and code snippets qodec can mine — but
this is the lane that most needs PROTECTED mode, because the model must copy
symbol names and paths verbatim, and blind mining aliases exactly those.

`optimize()` runs an explore and returns its output.
"""

from __future__ import annotations

import shutil
import subprocess

from . import Availability, ToolUnavailable

NAME = "codegraph"


def available() -> Availability:
    path = shutil.which("codegraph")
    if path is None:
        return Availability(False, "`codegraph` not on PATH")
    out = subprocess.run(
        ["codegraph", "--version"], capture_output=True, text=True, check=False
    )
    return Availability(out.returncode == 0, out.stdout.strip() or path)


def optimize(text: str, *, repo: str | None = None, target: str | None = None, **_: object) -> str:
    """Return `codegraph explore` output for `target` in `repo`.

    Requires `.codegraph/` present with no pending sync (doctor.py checks
    `codegraph status`)."""
    if not available().ok:
        raise ToolUnavailable("codegraph not installed")
    if repo is None or target is None:
        raise ToolUnavailable("codegraph lane needs a repo and an explore target")
    proc = subprocess.run(
        ["codegraph", "explore", target],
        cwd=repo,
        capture_output=True,
        text=True,
        check=False,
    )
    if proc.returncode != 0:
        raise ToolUnavailable(f"codegraph explore failed: {proc.stderr.strip()}")
    return proc.stdout
