"""Producers — create the raw/tool-only artifact a pipeline starts from.

Returns (produced, baseline): `produced` is the text the transforms will act on
(for an rtk-command that is already the RTK-filtered output); `baseline` is the
raw reference when the producer is itself a tool (rtk-command), else None.
"""

from __future__ import annotations

from pathlib import Path

from . import execution, lockfiles
from .manifest import Case, Producer

CRATE_ROOT = lockfiles.CRATE_ROOT


def _repo_dir(repo_id: str, repos: dict[str, lockfiles.Repo]) -> Path:
    if repo_id not in repos:
        raise execution.ExecutionError(f"repo {repo_id!r} not pinned in repos.lock.toml")
    d = repos[repo_id].clone_dir()
    if not d.exists():
        raise execution.ExecutionError(
            f"repo {repo_id!r} not cloned at {d} — run `python3 manage.py sync`"
        )
    return d


def produce(case: Case, tools: dict[str, lockfiles.Tool],
            repos: dict[str, lockfiles.Repo]) -> tuple[execution.Executed, execution.Executed | None]:
    p: Producer = case.producer
    if p.type == "fixture":
        path = Path(p.raw["path"])
        full = path if path.is_absolute() else CRATE_ROOT / path
        text = full.read_text()
        ex = execution.Executed(
            text=text, tool="fixture", argv=["cat", str(full)], cwd=".",
            exit_code=0, wall_ms=0.0, tool_version=None,
        )
        return ex, None

    if p.type == "command":
        cwd = _repo_dir(p["repo"], repos) if "repo" in p.raw else CRATE_ROOT
        sha = execution.repo_head(repos[p.raw["repo"]]) if "repo" in p.raw else None
        ex = execution.run(p.raw["argv"], cwd=cwd, tool="command", repo_sha=sha)
        return ex, None

    if p.type == "rtk-command":
        rtk = tools["rtk"]
        b = rtk.resolve_bin()
        if not b:
            raise execution.ExecutionError("rtk not resolvable (RTK_BIN / PATH)")
        cwd = _repo_dir(p.raw["repo"], repos)
        sha = execution.repo_head(repos[p.raw["repo"]])
        ver = rtk.detected_version()
        argv = p.raw["argv"]  # e.g. ["rg", "-n", "derive", "src"]
        produced = execution.run([b, *argv], cwd=cwd, tool="rtk", version=ver, repo_sha=sha)
        # Raw baseline: the same native command without rtk, for RTK's own
        # reduction figure. argv[0] is the native tool (rg, git, ...).
        baseline = execution.run(argv, cwd=cwd, tool=argv[0], repo_sha=sha)
        return produced, baseline

    if p.type == "codegraph":
        cg = tools["codegraph"]
        b = cg.resolve_bin()
        if not b:
            raise execution.ExecutionError("codegraph not resolvable (CODEGRAPH_BIN / PATH)")
        repo_dir = _repo_dir(p.raw["repo"], repos)
        sha = execution.repo_head(repos[p.raw["repo"]])
        ver = cg.detected_version()
        argv = [b, "explore", p.raw["query"], "-p", str(repo_dir)]
        if "max_files" in p.raw:
            argv += ["--max-files", str(p.raw["max_files"])]
        produced = execution.run(argv, tool="codegraph", version=ver, repo_sha=sha)
        return produced, None

    raise execution.ExecutionError(f"unknown producer type {p.type!r}")
