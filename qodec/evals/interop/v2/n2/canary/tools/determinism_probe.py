#!/usr/bin/env python3
"""N2-A.1 determinism probe.

Runs the approved third-party build repeatedly, under a set of named
candidate MSBuild/Roslyn producer-scheduling flag variants, and records
whether independent fresh builds agree on sanitized stdout/stderr — the
actual empirical test the N2-A.1 addendum requires ("test deterministic
producer controls rather than normalizing their output afterward... proven
on real hosted runners rather than accepted because it sounds plausible").

This never touches the sanitizer's rule set and never sorts, groups, or
deduplicates diagnostics — it only varies the build's own argv and observes
the resulting sanitized-stdout hash across repeated, independent, from-fresh
(restore + build) invocations of the same pinned commit.
"""
from __future__ import annotations

import json
import shutil
import subprocess
import sys
import time
from pathlib import Path

TOOLS_DIR = Path(__file__).resolve().parent
CORPUS_TOOLS = TOOLS_DIR.parents[2] / "corpus" / "tools"
sys.path.insert(0, str(CORPUS_TOOLS))
sys.path.insert(0, str(TOOLS_DIR))
from hashing import sha256_bytes  # noqa: E402

import dotnet_adapter as adapter  # noqa: E402
from sanitizer import sanitize  # noqa: E402

COMPILER_SERVER_MARKERS = ("VBCSCompiler", "CompilerServer: server")


def _detect_compiler_server(raw_stdout: bytes, raw_stderr: bytes) -> dict:
    """Best-effort detection of shared-compiler-server involvement: a
    substring search over the build's own raw output, plus a live-process
    snapshot taken immediately after the build (racy by nature — a VBCSCompiler
    process can exit on an idle timeout — so both signals are recorded rather
    than collapsed into a single boolean)."""
    text = (raw_stdout + b"\n" + raw_stderr).decode("utf-8", errors="replace")
    string_hits = {marker: (marker in text) for marker in COMPILER_SERVER_MARKERS}
    try:
        ps = subprocess.run(["ps", "-eo", "comm"], capture_output=True, text=True, timeout=10)
        process_seen = "VBCSCompiler" in ps.stdout
    except (OSError, subprocess.SubprocessError):
        process_seen = None
    return {"string_markers_seen": string_hits, "vbcscompiler_process_seen": process_seen}


def _clean_project_outputs(source_root: Path, project_rel: str) -> None:
    project_dir = (source_root / project_rel).parent
    for sub in ("obj", "bin"):
        shutil.rmtree(project_dir / sub, ignore_errors=True)


def run_one_capture(*, source_root: Path, dotnet_bin: str, project_rel: str,
                     extra_argv: list[str], base_argv_tail: list[str]) -> dict:
    """One fresh (restore + build) cycle — matches what a real capture job
    does, minus Sandboy/network-isolation (see determinism_probe module
    docstring: the probe's job is to find the flag set, not to re-prove
    Sandboy confinement, which the final chosen argv is proven under
    separately, for real, in the full capture pipeline)."""
    _clean_project_outputs(source_root, project_rel)
    restore_argv = [dotnet_bin, "restore", project_rel]
    r_restore = subprocess.run(restore_argv, cwd=str(source_root), capture_output=True, timeout=300)
    if r_restore.returncode != 0:
        return {
            "restore_exit_code": r_restore.returncode,
            "build_exit_code": None,
            "error": "restore failed",
            "restore_stderr_tail": r_restore.stderr.decode("utf-8", errors="replace")[-2000:],
        }

    project_dir = (source_root / project_rel).parent
    (project_dir / "bin").mkdir(parents=True, exist_ok=True)
    (project_dir / "obj").mkdir(parents=True, exist_ok=True)

    build_argv = [dotnet_bin, "build", project_rel, *base_argv_tail, *extra_argv]
    start = time.time()
    r_build = subprocess.run(build_argv, cwd=str(source_root), capture_output=True, timeout=600)
    wall_time_s = time.time() - start

    sanitized_stdout, stdout_report = sanitize(r_build.stdout, tmp_root=str(source_root))
    sanitized_stderr, stderr_report = sanitize(r_build.stderr, tmp_root=str(source_root))
    compiler_server = _detect_compiler_server(r_build.stdout, r_build.stderr)
    warning_lines = [
        line for line in r_build.stdout.decode("utf-8", errors="replace").splitlines()
        if ": warning " in line
    ]

    return {
        "restore_exit_code": r_restore.returncode,
        "build_argv": build_argv,
        "build_exit_code": r_build.returncode,
        "wall_time_s": wall_time_s,
        "raw_stdout_sha256": sha256_bytes(r_build.stdout),
        "raw_stderr_sha256": sha256_bytes(r_build.stderr),
        "sanitized_stdout_sha256": stdout_report["sanitized_sha256"],
        "sanitized_stderr_sha256": stderr_report["sanitized_sha256"],
        "warning_count": len(warning_lines),
        "compiler_server": compiler_server,
    }


def run_variant(*, source_root: Path, dotnet_bin: str, dotnet_root: str, project_rel: str,
                variant_name: str, extra_argv: list[str], repeats: int,
                base_argv_tail: list[str]) -> dict:
    toolchain_identity = adapter.capture_toolchain_identity(dotnet_bin)
    captures = [
        run_one_capture(
            source_root=source_root, dotnet_bin=dotnet_bin, project_rel=project_rel,
            extra_argv=extra_argv, base_argv_tail=base_argv_tail,
        )
        for _ in range(repeats)
    ]
    sanitized_stdout_hashes = {c.get("sanitized_stdout_sha256") for c in captures if c.get("sanitized_stdout_sha256")}
    sanitized_stderr_hashes = {c.get("sanitized_stderr_sha256") for c in captures if c.get("sanitized_stderr_sha256")}
    exit_codes = [c.get("build_exit_code") for c in captures]
    all_succeeded = all(code == 0 for code in exit_codes)
    return {
        "variant_name": variant_name,
        "extra_argv": extra_argv,
        "resolved_sdk_version": toolchain_identity["sdk_version"],
        "runtime_identifier": toolchain_identity["runtime_identifier"],
        "dotnet_binary_sha256": toolchain_identity["dotnet_binary_sha256"],
        "repeats": repeats,
        "captures": captures,
        "all_exit_codes_zero": all_succeeded,
        "distinct_sanitized_stdout_hashes": sorted(sanitized_stdout_hashes),
        "distinct_sanitized_stderr_hashes": sorted(sanitized_stderr_hashes),
        "all_sanitized_stdout_matched": len(sanitized_stdout_hashes) == 1,
        "all_sanitized_stderr_matched": len(sanitized_stderr_hashes) == 1,
        "deterministic": all_succeeded and len(sanitized_stdout_hashes) == 1 and len(sanitized_stderr_hashes) == 1,
    }


def main() -> int:
    import argparse

    ap = argparse.ArgumentParser()
    ap.add_argument("--source-root", required=True)
    ap.add_argument("--dotnet-root", required=True)
    ap.add_argument("--project-rel", required=True)
    ap.add_argument("--repeats", type=int, default=6)
    ap.add_argument("--variants-json", required=True, help="path to a JSON list of {name, argv} variant objects")
    ap.add_argument("--out", required=True)
    args = ap.parse_args()

    dotnet_bin = adapter.resolve_dotnet_bin(args.dotnet_root)
    variants = json.loads(Path(args.variants_json).read_text())
    base_argv_tail = ["--configuration", "Release", "--no-restore", "--nologo", "--verbosity", "normal"]

    results = []
    for variant in variants:
        print(f"determinism_probe: running variant {variant['name']!r} argv={variant['argv']}", file=sys.stderr)
        result = run_variant(
            source_root=Path(args.source_root), dotnet_bin=dotnet_bin, dotnet_root=args.dotnet_root,
            project_rel=args.project_rel, variant_name=variant["name"], extra_argv=variant["argv"],
            repeats=args.repeats, base_argv_tail=base_argv_tail,
        )
        results.append(result)
        print(f"determinism_probe: variant {variant['name']!r} deterministic={result['deterministic']}", file=sys.stderr)

    report = {
        "probe_version": "n2a1-determinism-probe-v1",
        "project_rel": args.project_rel,
        "dotnet_binary_sha256": adapter._sha256_file(Path(dotnet_bin)),
        "base_argv_tail": base_argv_tail,
        "variants": results,
    }
    Path(args.out).write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
