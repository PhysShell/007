"""RTK adapter — command-output lane.

RTK filters stdout by command type: it groups, deduplicates, drops noise and
keeps errors. In the bench it sits *before* qodec on the command-output lane
(command -> RTK -> qodec), and the question the lane answers is whether qodec
finds residual token redundancy after RTK has already eaten the obvious
repeats.

Contract caveat the design doc is explicit about: RTK's hook does NOT intercept
Claude Code's built-in Read/Grep/Glob, so the bench must invoke `rtk` filters
explicitly (`rtk read`, `rtk grep`, `rtk cargo test`) rather than trust hook
magic. Here we treat RTK as a stdin filter keyed by the producing command's
`kind` (the field carried on each Level-1 case).
"""

from __future__ import annotations

import shutil
import subprocess

from . import Availability, ToolUnavailable

NAME = "rtk"


def available() -> Availability:
    path = shutil.which("rtk")
    if path is None:
        return Availability(False, "`rtk` not on PATH")
    try:
        out = subprocess.run(
            ["rtk", "--version"], capture_output=True, text=True, check=False
        )
    except OSError as exc:  # pragma: no cover - environment dependent
        return Availability(False, f"`rtk --version` failed: {exc}")
    return Availability(out.returncode == 0, out.stdout.strip() or path)


def optimize(text: str, *, kind: str = "log", **_: object) -> str:
    """Filter raw command output through RTK for its command `kind`.

    We pipe the raw stdout to `rtk <kind> --stdin` (the explicit-filter form).
    When RTK exposes a different stdin contract in your install, adjust here —
    the bench pins the exact invocation in tools.lock.toml so a run is
    reproducible."""
    if not available().ok:
        raise ToolUnavailable("rtk not installed")
    proc = subprocess.run(
        ["rtk", kind, "--stdin"],
        input=text,
        capture_output=True,
        text=True,
        check=False,
    )
    if proc.returncode != 0:
        raise ToolUnavailable(f"rtk {kind} failed: {proc.stderr.strip()}")
    return proc.stdout
