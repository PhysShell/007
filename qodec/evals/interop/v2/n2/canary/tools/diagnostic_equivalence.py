#!/usr/bin/env python3
"""N2-A.1 diagnostic-equivalence check (addendum section 5).

Proves the new deterministic build argv preserves the COMPLETE diagnostic
multiset — same warnings, same file/line/column/code/message/project,
same occurrence counts — relative to the pre-hotfix argv. Never touches
the third-party source; never suppresses, filters, or reclassifies a
diagnostic. The only thing allowed to differ between "pre" and "post" is
which build argv produced the (structurally identical) output.
"""
from __future__ import annotations

import re
import subprocess
import sys
from collections import Counter
from pathlib import Path

TOOLS_DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(TOOLS_DIR))
import dotnet_adapter as adapter  # noqa: E402

WARNING_LINE_RE = re.compile(
    r"^\s*(?:\d+>)?(?P<path>\S.*?)\((?P<line>\d+),(?P<col>\d+)\): warning (?P<code>\w+): "
    r"(?P<message>.+?) \[(?P<project>[^\]]+)\]\s*$",
    re.MULTILINE,
)


def _relativize(path_text: str, source_root: Path) -> str:
    try:
        return str(Path(path_text).resolve().relative_to(source_root.resolve()))
    except ValueError:
        return path_text


def parse_warning_multiset(raw_stdout: str, source_root: Path) -> Counter:
    """Returns a Counter keyed by (file, line, column, code, message, project),
    each relativized to source_root so it's stable across different fresh
    checkout paths (pre/post runs use independent temp checkouts)."""
    counter: Counter = Counter()
    for m in WARNING_LINE_RE.finditer(raw_stdout):
        key = (
            _relativize(m.group("path"), source_root),
            int(m.group("line")),
            int(m.group("col")),
            m.group("code"),
            m.group("message"),
            _relativize(m.group("project"), source_root),
        )
        counter[key] += 1
    return counter


def _key_to_dict(key: tuple, count: int) -> dict:
    file_, line, col, code, message, project = key
    return {"file": file_, "line": line, "column": col, "code": code,
            "message": message, "project": project, "occurrence_count": count}


def compare_multisets(pre: Counter, post: Counter) -> dict:
    all_keys = set(pre) | set(post)
    added, removed, changed = [], [], []
    for key in sorted(all_keys):
        pre_count, post_count = pre.get(key, 0), post.get(key, 0)
        if pre_count and not post_count:
            removed.append(_key_to_dict(key, pre_count))
        elif post_count and not pre_count:
            added.append(_key_to_dict(key, post_count))
        elif pre_count != post_count:
            changed.append({"key": _key_to_dict(key, pre_count), "pre_count": pre_count, "post_count": post_count})
    return {
        "added": added,
        "removed": removed,
        "changed": changed,
        "equivalent": not added and not removed and not changed,
    }


def _clean_project_outputs(source_root: Path, project_rel: str) -> None:
    import shutil

    project_dir = (source_root / project_rel).parent
    for sub in ("obj", "bin"):
        shutil.rmtree(project_dir / sub, ignore_errors=True)


def run_build(source_root: Path, dotnet_bin: str, project_rel: str, extra_argv: list[str]) -> tuple[int, str]:
    _clean_project_outputs(source_root, project_rel)
    subprocess.run([dotnet_bin, "restore", project_rel], cwd=str(source_root), capture_output=True, timeout=300)
    project_dir = (source_root / project_rel).parent
    (project_dir / "bin").mkdir(parents=True, exist_ok=True)
    (project_dir / "obj").mkdir(parents=True, exist_ok=True)
    argv = [dotnet_bin, "build", project_rel, "--configuration", "Release", "--no-restore",
            "--nologo", "--verbosity", "normal", *extra_argv]
    r = subprocess.run(argv, cwd=str(source_root), capture_output=True, timeout=600)
    return r.returncode, r.stdout.decode("utf-8", errors="replace")


def main() -> int:
    import argparse
    import json

    ap = argparse.ArgumentParser()
    ap.add_argument("--source-root", required=True)
    ap.add_argument("--dotnet-root", required=True)
    ap.add_argument("--project-rel", required=True)
    ap.add_argument("--pre-argv-json", default="[]", help="JSON list of extra pre-hotfix build argv flags")
    ap.add_argument("--post-argv-json", default="[]", help="JSON list of extra post-hotfix build argv flags")
    ap.add_argument("--out", required=True)
    args = ap.parse_args()
    args.pre_argv = json.loads(args.pre_argv_json)
    args.post_argv = json.loads(args.post_argv_json)

    dotnet_bin = adapter.resolve_dotnet_bin(args.dotnet_root)
    source_root = Path(args.source_root)

    pre_exit, pre_stdout = run_build(source_root, dotnet_bin, args.project_rel, args.pre_argv)
    post_exit, post_stdout = run_build(source_root, dotnet_bin, args.project_rel, args.post_argv)

    pre_multiset = parse_warning_multiset(pre_stdout, source_root)
    post_multiset = parse_warning_multiset(post_stdout, source_root)
    comparison = compare_multisets(pre_multiset, post_multiset)

    report = {
        "report_version": "n2a1-diagnostic-equivalence-v1",
        "pre_hotfix_argv": args.pre_argv,
        "post_hotfix_argv": args.post_argv,
        "pre_exit_code": pre_exit,
        "post_exit_code": post_exit,
        "pre_hotfix_warning_multiset": [_key_to_dict(k, c) for k, c in sorted(pre_multiset.items())],
        "post_hotfix_warning_multiset": [_key_to_dict(k, c) for k, c in sorted(post_multiset.items())],
        "pre_hotfix_distinct_warning_count": len(pre_multiset),
        "post_hotfix_distinct_warning_count": len(post_multiset),
        **comparison,
    }
    Path(args.out).write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    print(f"diagnostic_equivalence: equivalent={comparison['equivalent']} "
          f"pre_distinct={len(pre_multiset)} post_distinct={len(post_multiset)} "
          f"pre_exit={pre_exit} post_exit={post_exit}", file=sys.stderr)
    return 0 if comparison["equivalent"] and pre_exit == 0 and post_exit == 0 else 1


if __name__ == "__main__":
    raise SystemExit(main())
