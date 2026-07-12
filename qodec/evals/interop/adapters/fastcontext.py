"""FastContext adapter — brief + evidence lane.

FastContext turns selected data into a dense prose brief. The design doc's
expectation: packing the whole brief rarely wins (a good brief is already
dense, near-repeat-free — qodec will mostly pass it through), but the attached
*evidence* (findings, traces, paths, snippets) is exactly qodec's home turf.

So the useful composition the bench compares is:
    brief only
    brief + raw evidence
    brief + qodec(evidence)
    qodec(brief + evidence)          # usually worse than the third

`optimize()` here produces the brief for a payload via the FastContext library
(served locally). Evidence is handled by the runner, which pairs a plain brief
with a qodec-encoded evidence appendix rather than encoding the brief itself.
"""

from __future__ import annotations

import importlib.util
import subprocess
import sys

from . import Availability, ToolUnavailable

NAME = "fastcontext"

_DRIVER = r"""
import sys
try:
    from fastcontext import brief
except Exception as exc:
    sys.stderr.write("fastcontext import failed: %s" % exc); sys.exit(3)
sys.stdout.write(brief(sys.stdin.read()))
"""


def available() -> Availability:
    if importlib.util.find_spec("fastcontext") is None:
        return Availability(False, "python package `fastcontext` not importable")
    return Availability(True, "fastcontext library API")


def optimize(text: str, **_: object) -> str:
    if not available().ok:
        raise ToolUnavailable("fastcontext library not installed")
    proc = subprocess.run(
        [sys.executable, "-c", _DRIVER],
        input=text,
        capture_output=True,
        text=True,
        check=False,
    )
    if proc.returncode != 0:
        raise ToolUnavailable(f"fastcontext brief failed: {proc.stderr.strip()}")
    return proc.stdout
