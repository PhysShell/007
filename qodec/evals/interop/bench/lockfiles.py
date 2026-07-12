"""Parse tools.lock.toml and repos.lock.toml — the pinned identities a run
claims to reproduce. Kept tiny and dependency-free (tomllib is stdlib on 3.11).
"""

from __future__ import annotations

import os
import shutil
import tomllib
from dataclasses import dataclass, field
from pathlib import Path

HERE = Path(__file__).resolve().parent
INTEROP = HERE.parent
CRATE_ROOT = INTEROP.parents[1]


def _load(name: str) -> dict:
    return tomllib.loads((INTEROP / name).read_text())


@dataclass
class Tool:
    name: str
    kind: str  # "built" | "cli" | "unsupported"
    raw: dict = field(default_factory=dict)

    @property
    def pinned_version(self) -> str | None:
        return self.raw.get("version")

    @property
    def reason(self) -> str | None:
        return self.raw.get("reason")

    @property
    def stdin_filters(self) -> list[str]:
        return list(self.raw.get("stdin_filters", []))

    def resolve_bin(self) -> str | None:
        """Locate the executable: <NAME>_BIN env override, then PATH."""
        if self.kind != "cli":
            return None
        env = os.environ.get(f"{self.name.upper()}_BIN")
        if env and Path(env).exists():
            return env
        return shutil.which(self.raw.get("bin", self.name))

    def detected_version(self) -> str | None:
        """Run the tool's version command and extract the version string."""
        import re
        import subprocess

        b = self.resolve_bin()
        if not b:
            return None
        cmd = [b, *self.raw.get("version_cmd", ["--version"])]
        try:
            out = subprocess.run(cmd, capture_output=True, text=True, check=False)
        except OSError:
            return None
        text = (out.stdout + out.stderr).strip()
        if not text:
            return None
        m = re.search(self.raw.get("version_re", r"(\S+)"), text)
        return m.group(1) if m else text.splitlines()[0]


@dataclass
class Repo:
    id: str
    url: str
    rev: str
    raw: dict = field(default_factory=dict)

    def clone_dir(self) -> Path:
        return INTEROP / ".cache" / "repos" / self.id


def tools() -> dict[str, Tool]:
    data = _load("tools.lock.toml").get("tools", {})
    return {name: Tool(name=name, kind=t.get("kind", "cli"), raw=t) for name, t in data.items()}


def repos() -> dict[str, Repo]:
    data = _load("repos.lock.toml").get("repos", {})
    out = {}
    for rid, r in data.items():
        rev = r.get("rev", "")
        if not rev or rev == "unset":
            # A reproducible run refuses an unpinned repo — surface it loudly
            # rather than silently benchmark whatever HEAD happened to be.
            raise ValueError(f"repo {rid!r} has no pinned rev in repos.lock.toml")
        out[rid] = Repo(id=rid, url=r["url"], rev=rev, raw=r)
    return out
