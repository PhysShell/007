#!/usr/bin/env python3
"""N2-D1b: CLI wrapper dispatching one repository-miner case's capture-a/
capture-b job to the generic capture engine, with the exact per-case
frozen argv, canonical-stream decision (locked from the tool's own
documented output convention, never from inspecting which stream
compresses better), and ecosystem-specific env/writable-dirs.
Trusted dependency realization already happened as separate, earlier CI
steps (network allowed there); this script's own trusted_setup_fn is a
no-op that just records that fact.
"""
from __future__ import annotations

import hashlib
import json
import sys
from pathlib import Path

TOOLS_DIR = Path(__file__).resolve().parent
CANARY_TOOLS = TOOLS_DIR.parents[1] / "canary" / "tools"
for p in (CANARY_TOOLS, TOOLS_DIR):
    sys.path.insert(0, str(p))

import dotnet_adapter  # noqa: E402
import ecosystem_toolchain as et  # noqa: E402
import generic_capture as gc  # noqa: E402

CASES = {
    "repo-hyperfine": {
        "ecosystem": "rust",
        "frozen_argv": ["cargo", "run", "--", "--version"],
        "canonical_stream": "stdout",
        "canonical_stream_rationale": (
            "cargo's own build-status preamble (Compiling/Finished) goes to stderr; "
            "the executed binary's --version output (the intended payload) goes to "
            "stdout, per standard CLI convention."
        ),
        "project_writable_dirs_relative": ["target"],
        "requested_version_or_range": "stable",
        "resolver_mechanism": "rustup toolchain resolution (rust-toolchain.toml / --default-toolchain stable)",
    },
    "repo-docker-java-parser": {
        "ecosystem": "jvm-maven",
        "frozen_argv": ["mvn", "test"],
        "canonical_stream": "stdout",
        "canonical_stream_rationale": "Maven's console reporter (including surefire test results) writes to stdout by documented convention; stderr carries JVM/process-level errors only.",
        "project_writable_dirs_relative": ["target"],
        "requested_version_or_range": "11",
        "resolver_mechanism": "explicit JDK pin -- JDK 21 fails with a real 'bad constant pool index' error in the Scala 2.13.6 compiler-bridge (classfile-version incompatibility), verified by dry run",
    },
    "repo-kubeops-generator": {
        "ecosystem": "dotnet",
        "frozen_argv": ["dotnet", "test", "test/KubeOps.Generator.Test/KubeOps.Generator.Test.csproj"],
        "canonical_stream": "stdout",
        "canonical_stream_rationale": "dotnet test's console logger (including VSTest results) writes to stdout by documented convention, matching N2-A's own established canonical_stream_decision.",
        "project_writable_dirs_relative": [
            "src/KubeOps.Generator/bin", "src/KubeOps.Generator/obj",
            "src/KubeOps.Abstractions/bin", "src/KubeOps.Abstractions/obj",
            "test/KubeOps.Generator.Test/bin", "test/KubeOps.Generator.Test/obj",
            "test/KubeOps.Generator.Test.Entities/bin", "test/KubeOps.Generator.Test.Entities/obj",
        ],
        "requested_version_or_range": "10.0.x",
        "resolver_mechanism": "actions/setup-dotnet dotnet-version 10.0.x -- explicit pin, verified by dry run (project targets net10.0; N2-A's own .NET 8 SDK does not support it)",
    },
    "repo-pyflakes": {
        "ecosystem": "python",
        "frozen_argv": ["python", "-m", "pyflakes", "src/"],
        "canonical_stream": "stdout",
        "canonical_stream_rationale": "pyflakes writes every violation it finds directly to stdout by documented convention; exit code alone signals pass/fail.",
        "project_writable_dirs_relative": [],
        "requested_version_or_range": "3.x",
        "resolver_mechanism": "runner-preinstalled python3, installed into a dedicated venv via trusted `pip install .`",
    },
    "repo-spotless": {
        "ecosystem": "jvm-gradle",
        "frozen_argv": ["./gradlew", "spotlessCheck"],
        "canonical_stream": "stdout",
        "canonical_stream_rationale": "Gradle's console output (task execution, spotlessCheck's own diagnostics) writes to stdout by documented convention.",
        "project_writable_dirs_relative": ["plugin-gradle/build", ".gradle"],
        "requested_version_or_range": "9.4.1",
        "resolver_mechanism": "gradle wrapper (gradlew) self-managed distribution fetch, JDK 21",
    },
    "repo-rustlings": {
        "ecosystem": "rust",
        "frozen_argv": ["cargo", "test"],
        "canonical_stream": "stdout",
        "canonical_stream_rationale": (
            "cargo test's own build-status preamble (Compiling/Finished) goes to stderr; "
            "the test harness's own pass/fail report (the intended payload) goes to "
            "stdout, per standard cargo-test convention -- same rationale as repo-hyperfine."
        ),
        "project_writable_dirs_relative": ["target"],
        "requested_version_or_range": "stable",
        "resolver_mechanism": "rustup toolchain resolution (no rust-toolchain.toml present; --default-toolchain stable)",
    },
    "repo-dockerfile-parser-rs": {
        "ecosystem": "rust",
        "frozen_argv": ["cargo", "test"],
        "canonical_stream": "stdout",
        "canonical_stream_rationale": "Same cargo-test convention as repo-hyperfine/repo-rustlings: build-status preamble to stderr, test harness report to stdout.",
        "project_writable_dirs_relative": ["target"],
        "requested_version_or_range": "stable",
        "resolver_mechanism": "rustup toolchain resolution (no rust-toolchain.toml present; --default-toolchain stable)",
        # This case's frozen acquisition has no committed Cargo.lock (a real,
        # load-bearing dependency-resolution finding, not a source defect).
        # Per explicit N2-D1b instruction: generate Cargo.lock exactly ONCE
        # in a dedicated trusted job, publish it as an immutable D1b-owned
        # dependency artifact, and make capture-a and capture-b consume the
        # EXACT SAME published lockfile -- never generate separate lockfiles
        # inside each capture.
        "requires_published_lockfile": True,
    },
    "repo-requests": {
        "ecosystem": "python",
        "frozen_argv": ["pytest"],
        "canonical_stream": "stdout",
        "canonical_stream_rationale": "pytest's console report (collection, pass/fail summary) writes to stdout by documented convention.",
        # pytest writes its cache (.pytest_cache/) directly into the
        # rootdir by default (confirmed via a real local dry-run) -- the
        # source tree itself is read-only under this policy, so this one
        # directory must be pre-created and writable.
        "project_writable_dirs_relative": [".pytest_cache"],
        "requested_version_or_range": "3.x",
        "resolver_mechanism": (
            "runner-preinstalled python3, installed into a dedicated venv via the "
            "repo's own documented `pip install -r requirements-dev.txt` (installs "
            "requests[socks] editable plus pytest/pytest-cov/pytest-httpbin/httpbin/trustme)"
        ),
    },
    "repo-moshi": {
        "ecosystem": "jvm-gradle",
        "frozen_argv": ["./gradlew", "test"],
        "canonical_stream": "stdout",
        "canonical_stream_rationale": "Gradle's console output (per-module test task execution and JUnit summary) writes to stdout by documented convention -- same as repo-spotless.",
        # Multi-module Gradle project (see settings.gradle.kts); an
        # unqualified `./gradlew test` runs the test task in every
        # subproject that defines one. Every module's own build dir must be
        # pre-created and writable, plus the root build/.gradle dirs.
        "project_writable_dirs_relative": [
            "build", ".gradle",
            "moshi/build", "moshi/japicmp/build", "moshi/records-tests/build",
            "moshi-adapters/build", "moshi-adapters/japicmp/build",
            "moshi-kotlin/build",
            "moshi-kotlin-codegen/build",
            "moshi-kotlin-tests/build",
            "moshi-kotlin-tests/codegen-only/build",
            "moshi-kotlin-tests/extra-moshi-test-module/build",
            "examples/build",
        ],
        "requested_version_or_range": "9.5.1",
        "resolver_mechanism": "gradle wrapper (gradlew) self-managed distribution fetch, JDK 21",
    },
}


def build_toolchain_fn_and_env(ecosystem: str, args) -> tuple:
    """Every returned closure takes one argument, `source_root` (the
    extracted case source tree) -- only jvm-gradle's actually needs it
    (gradlew is a project-relative wrapper; a real capture failed with
    FileNotFoundError('./gradlew') when the toolchain probe ran with no
    cwd), but a uniform one-arg calling convention keeps generic_capture.py
    ecosystem-agnostic."""
    if ecosystem == "rust":
        cargo_home = str(Path.home() / ".cargo")
        rustup_home = str(Path.home() / ".rustup")
        return (
            lambda source_root: et.capture_rust_toolchain_identity(),
            {"CARGO_HOME": cargo_home, "RUSTUP_HOME": rustup_home, "CARGO_NET_OFFLINE": "true"},
        )
    if ecosystem == "jvm-maven":
        java_home = args.java_home_11
        return (
            lambda source_root: et.capture_maven_toolchain_identity(java_home=java_home),
            {"JAVA_HOME": java_home},
        )
    if ecosystem == "jvm-gradle":
        java_home = args.java_home_21
        gradle_user_home = str(Path.home() / ".gradle")
        return (
            lambda source_root: et.capture_gradle_toolchain_identity(
                gradle_bin="./gradlew", java_home=java_home, cwd=source_root
            ),
            {"JAVA_HOME": java_home, "GRADLE_USER_HOME": gradle_user_home},
        )
    if ecosystem == "python":
        python_bin = args.venv_python
        return (
            lambda source_root: et.capture_python_toolchain_identity(python_bin),
            {},
        )
    if ecosystem == "dotnet":
        import os

        dotnet_root = os.environ.get("DOTNET_ROOT") or None
        dotnet_bin = dotnet_adapter.resolve_dotnet_bin(dotnet_root, "dotnet")

        def _capture(source_root):
            raw = dotnet_adapter.capture_toolchain_identity(dotnet_bin)
            return {
                **raw,
                "resolved_version": raw["sdk_version"],
                "runtime_identifier": raw["runtime_identifier"],
                "rustc_binary_path": dotnet_bin,
                "rustc_binary_sha256": raw["dotnet_binary_sha256"],
            }

        env_values = {"DOTNET_CLI_TELEMETRY_OPTOUT": "1", "DOTNET_NOLOGO": "1"}
        if dotnet_root:
            env_values["DOTNET_ROOT"] = dotnet_root
        return _capture, env_values
    raise ValueError(f"unknown ecosystem {ecosystem!r}")


def main() -> int:
    import argparse

    ap = argparse.ArgumentParser()
    ap.add_argument("--case-id", required=True, choices=list(CASES))
    ap.add_argument("--ecosystem", required=True)
    ap.add_argument("--job-name", required=True, choices=["capture-a", "capture-b"])
    ap.add_argument("--source-artifact-dir", required=True)
    ap.add_argument("--work-dir", required=True)
    ap.add_argument("--out-dir", required=True)
    ap.add_argument("--sandboy-bin", required=True)
    ap.add_argument("--sandboy-commit-sha", required=True)
    ap.add_argument("--errata-path", required=True)
    ap.add_argument("--venv-python", default="")
    ap.add_argument("--java-home-11", default="")
    ap.add_argument("--java-home-21", default="")
    ap.add_argument(
        "--published-lockfile", default="",
        help="Path to a pre-generated, immutable Cargo.lock artifact (repo-dockerfile-parser-rs only). "
             "Copied into the case's extracted source tree, then `cargo fetch --locked` runs against it -- "
             "never regenerated per-capture, so capture-a and capture-b consume the exact same lockfile.",
    )
    args = ap.parse_args()

    case = CASES[args.case_id]
    assert case["ecosystem"] == args.ecosystem

    toolchain_fn, env_values = build_toolchain_fn_and_env(args.ecosystem, args)
    # The erratum resolver inside gc.run_one_capture compares frozen_argv
    # against the erratum's original_frozen_argv VERBATIM -- passing the
    # already-venv-substituted argv here (as an earlier version of this
    # script did) made that comparison fail for repo-pyflakes, since the
    # substituted argv0 never matches the pure recorded original. The pure,
    # untouched frozen argv goes in; any interpreter-path substitution is
    # applied via argv0_override, AFTER erratum resolution, inside
    # run_one_capture itself.
    argv0_override = args.venv_python if args.ecosystem == "python" and args.venv_python else None

    if case.get("requires_published_lockfile"):
        if not args.published_lockfile:
            print(f"run_pilot_case: FAILED: {args.case_id} requires --published-lockfile, none given", file=sys.stderr)
            return 1
        published_lockfile_path = Path(args.published_lockfile)
        published_lockfile_sha256 = hashlib.sha256(published_lockfile_path.read_bytes()).hexdigest()

        def trusted_setup_fn(source_root):
            import shutil
            import subprocess

            dest = source_root / "Cargo.lock"
            shutil.copyfile(published_lockfile_path, dest)
            r = subprocess.run(["cargo", "fetch", "--locked"], cwd=source_root, capture_output=True, text=True)
            if r.returncode != 0:
                raise gc.GenericCaptureFailure(
                    f"cargo fetch --locked against published lockfile failed (exit {r.returncode}): {r.stderr[-2000:]}"
                )
            return {
                "note": "trusted dependency realization: published Cargo.lock copied in, then `cargo fetch --locked` (network allowed here; capture itself never contacts the network)",
                "published_lockfile_path": str(published_lockfile_path),
                "published_lockfile_sha256": published_lockfile_sha256,
                "cargo_fetch_exit_code": r.returncode,
            }
    else:
        def trusted_setup_fn(source_root):
            return {"note": "trusted dependency realization already performed as an earlier, separate CI step (network allowed there); this capture step never contacts the network."}

    try:
        receipt = gc.run_one_capture(
            case_id=args.case_id, ecosystem=args.ecosystem, job_name=args.job_name,
            source_artifact_dir=Path(args.source_artifact_dir),
            work_dir=Path(args.work_dir), out_dir=Path(args.out_dir),
            frozen_argv=case["frozen_argv"],
            errata_path=Path(args.errata_path),
            sandboy_bin=Path(args.sandboy_bin), sandboy_commit_sha=args.sandboy_commit_sha,
            toolchain_capture_fn=toolchain_fn, toolchain_env_values=env_values,
            canonical_stream=case["canonical_stream"],
            primary_stream_rationale=case["canonical_stream_rationale"],
            project_writable_dirs_relative=case["project_writable_dirs_relative"],
            requested_version_or_range=case["requested_version_or_range"],
            resolver_mechanism=case["resolver_mechanism"],
            trusted_setup_fn=trusted_setup_fn,
            argv0_override=argv0_override,
        )
    except gc.GenericCaptureFailure as e:
        Path(args.out_dir).mkdir(parents=True, exist_ok=True)
        (Path(args.out_dir) / "capture-failure.json").write_text(json.dumps({"error": str(e)}, indent=2))
        print(f"run_pilot_case: FAILED: {e}", file=sys.stderr)
        return 1

    print(f"run_pilot_case[{args.case_id}/{args.job_name}]: exit_code={receipt['termination']['exit_code']} "
          f"canonical_raw_input_sha256={receipt['canonical_raw_input_sha256'][:16]}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
