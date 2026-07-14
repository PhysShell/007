#!/usr/bin/env python3
"""manage.py — materialize the pinned corpus repos and their CodeGraph indexes.

Idempotent: clones each repo in repos.lock.toml at its exact rev into
.cache/repos/<id> (gitignored), verifies HEAD == rev, and builds the CodeGraph
index if missing. `doctor.py` then checks the same invariants before a run.

    python3 manage.py sync            # clone/checkout + index all pinned repos
    python3 manage.py sync --no-index # clone/checkout only
"""

from __future__ import annotations

import argparse
import subprocess
import sys

from bench import lockfiles


def _run(argv: list[str], cwd=None) -> int:
    print("  $", " ".join(argv))
    return subprocess.run(argv, cwd=cwd, check=False).returncode


def sync_repo(repo: lockfiles.Repo, *, index: bool) -> bool:
    d = repo.clone_dir()
    d.parent.mkdir(parents=True, exist_ok=True)
    if not (d / ".git").exists():
        tag = repo.raw.get("tag")
        print(f"[{repo.id}] cloning {repo.url} @ {tag or repo.rev}")
        if tag:
            if _run(["git", "clone", "--depth", "1", "--branch", tag, repo.url, str(d)]) != 0:
                return False
        else:
            d.mkdir(parents=True, exist_ok=True)
            _run(["git", "init", str(d)])
            _run(["git", "-C", str(d), "fetch", "--depth", "1", repo.url, repo.rev])
            _run(["git", "-C", str(d), "checkout", "FETCH_HEAD"])
    head = subprocess.run(["git", "-C", str(d), "rev-parse", "HEAD"],
                          capture_output=True, text=True, check=False).stdout.strip()
    if head != repo.rev:
        print(f"[{repo.id}] HEAD {head} != pinned {repo.rev} — refusing")
        return False
    print(f"[{repo.id}] HEAD == pinned rev {repo.rev}")
    if index:
        cg = lockfiles.tools().get("codegraph")
        b = cg.resolve_bin() if cg else None
        if not b:
            print(f"[{repo.id}] codegraph not resolvable — skipping index")
        elif (d / ".codegraph").exists():
            print(f"[{repo.id}] .codegraph/ present — leaving index as-is")
        else:
            print(f"[{repo.id}] building CodeGraph index")
            if _run([b, "init", str(d)]) != 0:
                return False
    return True


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("cmd", choices=["sync"])
    ap.add_argument("--no-index", action="store_true", help="skip codegraph init")
    args = ap.parse_args()
    repos = lockfiles.repos()
    ok = all(sync_repo(r, index=not args.no_index) for r in repos.values())
    print("\nall repos synced" if ok else "\nSOME REPOS FAILED")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
