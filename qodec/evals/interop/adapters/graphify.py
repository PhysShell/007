"""Graphify adapter — retrieval lane.

Graphify builds a local knowledge graph and answers query/path/explain against
it, emitting graph.json + GRAPH_REPORT.md and the query results. qodec sits
*after* a query/path/explain, on the hypothesis that large subgraph/path
outputs still carry repeated node names and path prefixes worth mining.

`optimize()` runs a graph query and returns its textual output. The bench pins
the repo + rev in repos.lock.toml and the graphify invocation in
tools.lock.toml, so a lane is reproducible; `doctor.py` checks that a build
produced graph.json + GRAPH_REPORT.md before any lane runs.
"""

from __future__ import annotations

import shutil
import subprocess

from . import Availability, ToolUnavailable

NAME = "graphify"


def available() -> Availability:
    path = shutil.which("graphify")
    if path is None:
        return Availability(False, "`graphify` not on PATH")
    out = subprocess.run(
        ["graphify", "--version"], capture_output=True, text=True, check=False
    )
    return Availability(out.returncode == 0, out.stdout.strip() or path)


def optimize(text: str, *, repo: str | None = None, query: str | None = None, **_: object) -> str:
    """Return Graphify query/path/explain output for `query` over `repo`.

    Level-1 feeds the query text as the "producer input" identity; the real
    optimized artifact is Graphify's own answer, which the bench then hands to
    qodec. Requires a built graph in `repo` (see doctor.py)."""
    if not available().ok:
        raise ToolUnavailable("graphify not installed")
    if repo is None or query is None:
        raise ToolUnavailable("graphify lane needs a repo and a query")
    proc = subprocess.run(
        ["graphify", "query", query],
        cwd=repo,
        capture_output=True,
        text=True,
        check=False,
    )
    if proc.returncode != 0:
        raise ToolUnavailable(f"graphify query failed: {proc.stderr.strip()}")
    return proc.stdout
