#!/usr/bin/env python3
"""N2-A capture-job orchestration (runs as `capture-a` or `capture-b`).

Extracts the already-verified, already-packaged source artifact from the
trusted-source-acquisition job, re-verifies it against its own file manifest
(never re-clones or re-fetches the third-party repository), captures trusted
toolchain identity, realizes dependencies (trusted setup — see
dotnet_adapter.py's module docstring for why this is not a network violation),
then runs the actual `dotnet build` through the outer network-isolation +
resource-limit wrapper + Sandboy, and writes every required N2-A capture
artifact.
"""
from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import sys
import tarfile
import time
from pathlib import Path

TOOLS_DIR = Path(__file__).resolve().parent
CORPUS_TOOLS = TOOLS_DIR.parents[2] / "corpus" / "tools"
sys.path.insert(0, str(CORPUS_TOOLS))
sys.path.insert(0, str(TOOLS_DIR))
from hashing import sha256_bytes, sha256_file  # noqa: E402

import dotnet_adapter as adapter  # noqa: E402
import sandboy_policy  # noqa: E402
from sanitizer import sanitize  # noqa: E402

RUN_CONFINED_BUILD = TOOLS_DIR / "run_confined_build.sh"
NETWORK_PROBE = TOOLS_DIR / "network_probe.py"


class CaptureFailure(Exception):
    pass


def verify_and_extract_source(source_tar: Path, source_manifest: dict, dest: Path) -> None:
    expected_sha = source_manifest["resolved"]["archive_sha256"]
    actual_sha = sha256_file(source_tar)
    if actual_sha != expected_sha:
        raise CaptureFailure(
            f"source.tar sha256 {actual_sha} != acquisition-recorded {expected_sha} — "
            "refusing to build from a tampered or mismatched source artifact"
        )
    dest.mkdir(parents=True, exist_ok=True)
    with tarfile.open(source_tar, "r") as tar:
        tar.extractall(dest, filter="data")


def verify_extracted_tree(dest: Path, file_manifest: list[dict]) -> None:
    for entry in file_manifest:
        p = dest / entry["path"]
        if not p.is_file():
            raise CaptureFailure(f"extracted tree missing manifest file: {entry['path']}")
        actual = sha256_file(p)
        if actual != entry["sha256"]:
            raise CaptureFailure(
                f"extracted file {entry['path']} sha256 {actual} != manifest {entry['sha256']}"
            )


def run_network_probe(sandboy_bin: Path, policy_path: Path, source_root: Path, env: dict, python_bin: str) -> dict:
    argv = [
        str(RUN_CONFINED_BUILD),
        str(sandboy_bin),
        str(policy_path),
        str(source_root),
        "--",
        python_bin,
        str(NETWORK_PROBE),
    ]
    r = subprocess.run(argv, capture_output=True, text=True, env=env, timeout=60)
    try:
        parsed = json.loads(r.stdout.strip().splitlines()[-1]) if r.stdout.strip() else {"probe_results": []}
    except (json.JSONDecodeError, IndexError):
        parsed = {"probe_results": [], "parse_error": True, "raw_stdout": r.stdout, "raw_stderr": r.stderr}
    return {
        "wrapper_exit_code": r.returncode,
        "all_targets_unreachable": r.returncode == 0,
        **parsed,
    }


def run_real_build(sandboy_bin: Path, policy_path: Path, source_root: Path, argv: list[str], env: dict) -> dict:
    wrapper_argv = [str(RUN_CONFINED_BUILD), str(sandboy_bin), str(policy_path), str(source_root), "--", *argv]
    time_bin = shutil.which("time") or "/usr/bin/time"
    use_gnu_time = Path(time_bin).exists()
    if use_gnu_time:
        wrapper_argv = [time_bin, "-v"] + wrapper_argv
    start = time.time()
    r = subprocess.run(wrapper_argv, capture_output=True, env=env, timeout=1200)
    wall_time_s = time.time() - start
    peak_rss_kb = None
    time_v_tail = ""
    if use_gnu_time:
        # `time -v` writes its report to stderr, appended after the wrapped
        # command's own stderr; split it back off before treating the rest as
        # the build's actual stderr.
        stderr_text = r.stderr.decode("utf-8", errors="replace")
        marker = "\tCommand being timed:"
        idx = stderr_text.rfind(marker)
        if idx != -1:
            time_v_tail = stderr_text[idx:]
            r_stderr = stderr_text[:idx].encode("utf-8")
        else:
            r_stderr = r.stderr
        for line in time_v_tail.splitlines():
            if "Maximum resident set size" in line:
                try:
                    peak_rss_kb = int(line.strip().split()[-1])
                except ValueError:
                    pass
    else:
        r_stderr = r.stderr
    return {
        "argv": argv,
        "wrapper_argv": wrapper_argv,
        "exit_code": r.returncode,
        "raw_stdout": r.stdout,
        "raw_stderr": r_stderr,
        "wall_time_s": wall_time_s,
        "peak_rss_kb": peak_rss_kb,
        "gnu_time_report": time_v_tail or None,
    }


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--source-artifact-dir", required=True, help="downloaded miner-canary-source dir")
    ap.add_argument("--sandboy-bin", required=True)
    ap.add_argument("--sandboy-commit-sha", required=True)
    ap.add_argument("--work-dir", required=True, help="fresh isolated workspace for this job")
    ap.add_argument("--out-dir", required=True)
    ap.add_argument("--job-name", required=True, choices=["capture-a", "capture-b"])
    ap.add_argument("--dotnet-bin", default="dotnet")
    ap.add_argument("--dotnet-root", required=True)
    ap.add_argument("--python-bin", default=sys.executable)
    args = ap.parse_args()

    src_artifact_dir = Path(args.source_artifact_dir)
    work_dir = Path(args.work_dir)
    out_dir = Path(args.out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)
    work_dir.mkdir(parents=True, exist_ok=True)

    source_manifest = json.loads((src_artifact_dir / "source-manifest.json").read_text())
    file_manifest = json.loads((src_artifact_dir / "source-file-manifest.json").read_text())
    license_record = json.loads((src_artifact_dir / "license-record.json").read_text())

    source_root = work_dir / "source"
    try:
        verify_and_extract_source(src_artifact_dir / "source.tar", source_manifest, source_root)
        verify_extracted_tree(source_root, file_manifest)

        adapter.validate_project_before_execution(source_root, source_manifest)
        toolchain_identity = adapter.capture_toolchain_identity(args.dotnet_bin)
        restore_result = adapter.realize_dependencies_trusted(source_root, source_manifest, args.dotnet_bin)
    except CaptureFailure as e:
        (out_dir / "capture-failure.json").write_text(json.dumps({"error": str(e)}, indent=2))
        print(f"capture_build: FAILED (pre-execution): {e}", file=sys.stderr)
        return 1

    home_dir = work_dir / "home"
    tmp_dir = work_dir / "tmp"
    capture_out_dir = work_dir / "capture-out"
    for d in (home_dir, tmp_dir, capture_out_dir):
        d.mkdir(parents=True, exist_ok=True)

    project_dir = (source_root / source_manifest["project"]["path"]).parent
    # Sandboy's add_fs() silently SKIPS a fs_rw rule whose path doesn't exist
    # yet at policy-application time (it warns and continues rather than
    # failing the whole ruleset — reasonable for portability, but it means an
    # about-to-be-created build output directory gets NO Landlock rule at
    # all, which under a default-deny-all-not-explicitly-allowed ruleset
    # means "completely inaccessible", not "writable once created". obj/
    # already exists by this point (trusted `dotnet restore` created it) but
    # bin/ does not — an earlier N2-A run showed the build fail at the final
    # copy-to-output step (MSB3021: Access to the path '.../bin' is denied)
    # for exactly this reason. Pre-create both so Sandboy's rule actually
    # applies to them.
    (project_dir / "bin").mkdir(parents=True, exist_ok=True)
    (project_dir / "obj").mkdir(parents=True, exist_ok=True)
    policy_path = work_dir / "policy.toml"
    policy_sha256, canonical_policy_sha256 = sandboy_policy.write_policy(
        policy_path,
        work_dir=work_dir,
        source_root=source_root,
        project_dir=project_dir,
        sdk_root=Path(args.dotnet_root),
        home_dir=home_dir,
        tmp_dir=tmp_dir,
        capture_out_dir=capture_out_dir,
        repo_tools_dir=TOOLS_DIR,
    )

    sandboy_bin = Path(args.sandboy_bin)
    launcher_env = {
        "PATH": os.environ.get("PATH", "/usr/bin:/bin"),
        "HOME": str(home_dir),
        "TMPDIR": str(tmp_dir),
        "DOTNET_ROOT": args.dotnet_root,
        "DOTNET_CLI_TELEMETRY_OPTOUT": "1",
        "DOTNET_NOLOGO": "1",
        "DOTNET_SKIP_FIRST_TIME_EXPERIENCE": "1",
        "DOTNET_MULTILEVEL_LOOKUP": "0",
        "DOTNET_GENERATE_ASPNET_CERTIFICATE": "false",
    }

    network_report = run_network_probe(sandboy_bin, policy_path, source_root, launcher_env, args.python_bin)
    (out_dir / "network-isolation-report.json").write_text(
        json.dumps(
            {
                "job": args.job_name,
                "isolation_mechanism": "sudo unshare --net (fresh network namespace; loopback only)",
                **network_report,
            },
            indent=2,
            sort_keys=True,
        )
        + "\n"
    )
    if not network_report["all_targets_unreachable"]:
        print("capture_build: WARNING: network probe found a reachable target inside the isolated namespace", file=sys.stderr)

    build_argv = adapter.build_argv(source_manifest)
    result = run_real_build(sandboy_bin, policy_path, source_root, build_argv, launcher_env)

    (out_dir / "raw.stdout").write_bytes(result["raw_stdout"])
    (out_dir / "raw.stderr").write_bytes(result["raw_stderr"])

    # The build's cwd lives entirely under work_dir (a fresh per-job temp
    # extraction) — that's the volatile path that can appear in dotnet's
    # output. GITHUB_WORKSPACE (the 007 checkout) is a second, independent
    # volatile root: the build never touches it, but sanitize it too in case
    # any tool error ever echoes $PWD-adjacent paths.
    workspace_root = os.environ.get("GITHUB_WORKSPACE", "")
    sanitized_stdout, stdout_report = sanitize(
        result["raw_stdout"], tmp_root=str(work_dir), workspace_root=workspace_root
    )
    sanitized_stderr, stderr_report = sanitize(
        result["raw_stderr"], tmp_root=str(work_dir), workspace_root=workspace_root
    )
    (out_dir / "sanitization-report.json").write_text(
        json.dumps({"stdout": stdout_report, "stderr": stderr_report}, indent=2, sort_keys=True) + "\n"
    )

    (out_dir / "resource-limit-report.json").write_text(
        json.dumps(
            {
                "job": args.job_name,
                "requested_limits": {
                    "address_space_kib": None,
                    "cpu_time_s": 600,
                    "max_processes": 512,
                    "wall_clock_timeout_s": 900,
                },
                "address_space_limit_finding": (
                    "RLIMIT_AS (`ulimit -v`) is deliberately NOT applied: an earlier N2-A run "
                    "confirmed it makes CoreCLR fail immediately with HRESULT 0x8007000E "
                    "(E_OUTOFMEMORY) — CoreCLR reserves virtual address space far beyond what "
                    "it commits/uses at startup. This is a genuine dotnet/RLIMIT_AS interop "
                    "constraint, recorded as a finding rather than forced through."
                ),
                "enforced_by": "outer job shell (ulimit -t/-u) + timeout; NOT enforced by Sandboy itself (documented S0 gap)",
                "observed_peak_rss_kib": result["peak_rss_kb"],
                "observed_wall_time_s": result["wall_time_s"],
                "exit_code": result["exit_code"],
            },
            indent=2,
            sort_keys=True,
        )
        + "\n"
    )

    receipt = {
        "receipt_version": "n2a-capture-receipt-v1",
        "job": args.job_name,
        "case_id": source_manifest["case_id"],
        "source_repository": source_manifest["repository"]["url"],
        "source_commit_sha": source_manifest["resolved"]["actual_head_sha"],
        "source_archive_sha256": source_manifest["resolved"]["archive_sha256"],
        "license_spdx": license_record["spdx"],
        "license_sha256": license_record["sha256"],
        "sandboy_commit_sha": args.sandboy_commit_sha,
        # Informational only: two independent `cargo build --release` runs
        # are not guaranteed byte-reproducible (embedded paths/timestamps in
        # debug info), so this is NOT part of the hard reproducibility gate —
        # sandboy_commit_sha (identical by construction, same pinned ref
        # checked out in both jobs) is the semantic identity field instead.
        "sandboy_binary_sha256": sha256_file(sandboy_bin),
        "policy_sha256": policy_sha256,
        "canonical_policy_sha256": canonical_policy_sha256,
        "argv": build_argv,
        "cwd_relative_to_source_root": ".",
        "environment_allowlist": sorted(launcher_env.keys()),
        "restore_trusted_setup": restore_result,
        "dotnet_sdk_version": toolchain_identity["sdk_version"],
        "dotnet_runtime_identifier": toolchain_identity["runtime_identifier"],
        "dotnet_binary_sha256": toolchain_identity["dotnet_binary_sha256"],
        "exit_code": result["exit_code"],
        "wall_time_s": result["wall_time_s"],
        "stdout_sha256": sha256_bytes(result["raw_stdout"]),
        "stderr_sha256": sha256_bytes(result["raw_stderr"]),
        "sanitized_stdout_sha256": stdout_report["sanitized_sha256"],
        "sanitized_stderr_sha256": stderr_report["sanitized_sha256"],
        "network_isolation": "sudo unshare --net, loopback-only namespace; see network-isolation-report.json",
        "capture_timestamp": os.environ.get("SOURCE_DATE_EPOCH_ISO") or None,
    }
    (out_dir / "sandboy-execution-receipt.json").write_text(json.dumps(receipt, indent=2, sort_keys=True) + "\n")

    # Semantic fields compared for reproducibility (capture_timestamp and
    # wall_time_s are the only allowed-to-vary metadata).
    semantic_fields = [
        "case_id", "source_repository", "source_commit_sha", "source_archive_sha256",
        "license_spdx", "license_sha256", "sandboy_commit_sha",
        "canonical_policy_sha256", "argv", "environment_allowlist", "dotnet_sdk_version",
        "dotnet_runtime_identifier", "dotnet_binary_sha256", "exit_code",
        "sanitized_stdout_sha256", "sanitized_stderr_sha256",
    ]
    snapshot_manifest = {
        "job": args.job_name,
        "semantic_receipt_fields": semantic_fields,
        "semantic_view": {k: receipt[k] for k in semantic_fields},
        "raw_stdout_sha256": sha256_bytes(result["raw_stdout"]),
        "raw_stderr_sha256": sha256_bytes(result["raw_stderr"]),
    }
    (out_dir / "snapshot-manifest.json").write_text(json.dumps(snapshot_manifest, indent=2, sort_keys=True) + "\n")

    print(f"capture_build[{args.job_name}]: build exit_code={result['exit_code']} wall_time_s={result['wall_time_s']:.2f}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
