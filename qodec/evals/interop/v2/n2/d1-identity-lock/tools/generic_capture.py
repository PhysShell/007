#!/usr/bin/env python3
"""N2-D1b generic capture engine: materializes one repository-miner primary
case's raw benchmark input by actually running its frozen (or, for
repo-pyflakes, authorized-erratum-corrected) execution_expectation.argv
against its exact frozen, pinned-commit source tree, under Sandboy
confinement with network denied, for capture-a and capture-b independently.

Reuses, unmodified: canary/tools/capture_build.run_real_build (the actual
confined-execution+capture primitive -- ecosystem-agnostic in practice),
canary/tools/run_confined_build.sh (the outer netns+resource wrapper),
canary/tools/sanitizer.sanitize (base rules), miner/tools/toolchain_identity.
build_toolchain_identity, miner/tools/receipt_contract (schema + validate).

New here (D1b-owned): per-ecosystem toolchain identity capture
(ecosystem_toolchain.py), the generalized sandbox policy
(generic_sandbox_policy.py), execution-plan errata resolution
(execution-plan-errata.json), the durable-input 4 MiB byte-range cap
(shared rule with derive_raw_input.py's non-repository cases), and this
orchestration itself.
"""
from __future__ import annotations

import hashlib
import json
import os
import sys
import tarfile
from pathlib import Path

TOOLS_DIR = Path(__file__).resolve().parent
CANARY_TOOLS = TOOLS_DIR.parents[1] / "canary" / "tools"
MINER_TOOLS = TOOLS_DIR.parents[1] / "miner" / "tools"
CORPUS_TOOLS = TOOLS_DIR.parents[1] / "corpus" / "tools"
for p in (CANARY_TOOLS, MINER_TOOLS, CORPUS_TOOLS, TOOLS_DIR):
    sys.path.insert(0, str(p))

import capture_build  # noqa: E402
import content_acceptance  # noqa: E402
import generic_sandbox_policy as gsp  # noqa: E402
import maven_canonicalizer  # noqa: E402
import network_enforcement_probe  # noqa: E402
import receipt_contract  # noqa: E402
import toolchain_identity  # noqa: E402
import vstest_canonicalizer  # noqa: E402
from sanitizer import sanitize  # noqa: E402

MAXIMUM_EXTRACTED_SOURCE_BYTES = 4194304  # 4 MiB -- same durable-input cap as derive_raw_input.py

CANONICALIZATION_POLICY_PATH = TOOLS_DIR.parent / "capture-canonicalization-policy.json"
VSTEST_CANONICALIZATION_POLICY_PATH = TOOLS_DIR.parent / "vstest-capture-canonicalization-policy.json"

# D1b decision (2026-07-16): for the case_id(s) each policy below names, the
# canonical benchmark input is a deterministic derivation of the raw,
# selected stream through that narrowly scoped canonicalization profile --
# exact raw capture-a/capture-b byte equality is no longer required for
# those cases, only exact CANONICAL equality. Every other case is entirely
# unaffected: its canonical benchmark input remains the raw, capped,
# selected stream verbatim, exactly as before this decision.
#
# Each profile is independently self-hash-locked and verified against its
# OWN canonicalizer module's RULES (fail closed at import time if either
# policy has been tampered with or has drifted from its module) -- Maven's
# and VSTest's profiles are never merged, and a case_id must never appear in
# more than one profile's applicable_case_ids ("do not add the rule to the
# Maven-specific canonicalizer or silently broaden either profile", D1b
# 2026-07-16).
_CANONICALIZATION_POLICY = maven_canonicalizer.load_and_verify_policy(CANONICALIZATION_POLICY_PATH)
_VSTEST_CANONICALIZATION_POLICY = vstest_canonicalizer.load_and_verify_policy(VSTEST_CANONICALIZATION_POLICY_PATH)
_CANONICALIZATION_PROFILES = [
    (_CANONICALIZATION_POLICY, maven_canonicalizer),
    (_VSTEST_CANONICALIZATION_POLICY, vstest_canonicalizer),
]
_all_canonicalized_case_ids = [
    cid for policy, _module in _CANONICALIZATION_PROFILES for cid in policy["applicable_case_ids"]
]
if len(_all_canonicalized_case_ids) != len(set(_all_canonicalized_case_ids)):
    raise ValueError(
        f"a case_id appears in more than one canonicalization profile's applicable_case_ids: "
        f"{_all_canonicalized_case_ids}"
    )
CANONICALIZATION_MODULE_BY_CASE_ID = {
    cid: module for policy, module in _CANONICALIZATION_PROFILES for cid in policy["applicable_case_ids"]
}
CANONICALIZATION_POLICY_SHA256_BY_CASE_ID = {
    cid: policy["policy_sha256"] for policy, _module in _CANONICALIZATION_PROFILES for cid in policy["applicable_case_ids"]
}
CANONICALIZED_CASE_IDS = frozenset(CANONICALIZATION_MODULE_BY_CASE_ID)


class GenericCaptureFailure(Exception):
    pass


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def sha256_file(path: Path) -> str:
    return sha256_bytes(Path(path).read_bytes())


def resolve_effective_argv(case_id: str, frozen_argv: list[str], errata_path: Path) -> tuple[list[str], str, str | None]:
    """Returns (effective_argv, resolution, erratum_sha256). resolution is
    "frozen" (no erratum applies) or "authorized-n2d1b-erratum"."""
    errata = json.loads(errata_path.read_text())
    entry = next((e for e in errata["entries"] if e["case_id"] == case_id
                  and e["status"] == "AUTHORIZED_ERRATUM"), None)
    if entry is None:
        return frozen_argv, "frozen", None
    if entry["original_frozen_argv"] != frozen_argv:
        raise GenericCaptureFailure(
            f"erratum for {case_id} was authorized against a different frozen argv "
            f"({entry['original_frozen_argv']!r}) than what is currently frozen ({frozen_argv!r}) "
            "-- refusing to apply a stale erratum"
        )
    return entry["corrected_effective_argv"], "authorized-n2d1b-erratum", errata["errata_sha256"]


def verify_and_extract_source(source_tar: Path, expected_archive_sha256: str, dest: Path) -> None:
    actual = sha256_file(source_tar)
    if actual != expected_archive_sha256:
        raise GenericCaptureFailure(
            f"source.tar sha256 {actual} != acquisition-recorded {expected_archive_sha256} "
            "-- refusing to build from a mismatched source artifact"
        )
    dest.mkdir(parents=True, exist_ok=True)
    with tarfile.open(source_tar, "r") as tar:
        tar.extractall(dest, filter="data")


def apply_durable_byte_cap(data: bytes) -> bytes:
    return data[:MAXIMUM_EXTRACTED_SOURCE_BYTES]


def verify_relative_argv0_exists(argv0: str, cwd: Path) -> None:
    """The confined build always runs with cwd=source_root (the repo root of
    the case's own fresh extraction) -- never a subdirectory named after a
    manifest's `project.entry_point` (which can name a specific module
    without meaning "run from there"; repo-moshi's manifest says entry_point
    "moshi" but its gradlew wrapper lives at the repository root, and the
    frozen argv is the wrapper's own relative path, not entry_point). For a
    relative wrapper argv0 (e.g. "./gradlew"), fail loudly and specifically
    here if it doesn't actually exist at that cwd, rather than letting a
    wrong assumption surface later as an opaque sandbox/ENOENT error."""
    if not argv0.startswith(("./", "../")):
        return
    resolved = (cwd / argv0).resolve()
    if not resolved.is_file():
        raise GenericCaptureFailure(
            f"relative argv0 {argv0!r} does not exist at the confined build's cwd "
            f"({cwd}) -- resolved path {resolved} is not a file. Do not assume a "
            f"manifest's project.entry_point names the command's cwd; verify where "
            f"the wrapper actually lives in the real extracted source tree."
        )


def dedupe_sanitizer_rule_names(*reports: dict) -> list[str]:
    """sanitizer.sanitize's rules_applied is a list of
    {"rule": name, "replacements": count} dicts, not rule-name strings --
    set()-ing the dicts directly raises "unhashable type: 'dict'" the moment
    any rule actually matches (a real repo-kubeops-generator/dotnet capture
    hit this: dotnet test's real "Time Elapsed HH:MM:SS.ffffff" line matched
    the dotnet_time_elapsed_line rule)."""
    names: set[str] = set()
    for report in reports:
        names.update(entry["rule"] for entry in report.get("rules_applied", []))
    return sorted(names)


def run_one_capture(*, case_id: str, ecosystem: str, job_name: str,
                     source_artifact_dir: Path, work_dir: Path, out_dir: Path,
                     frozen_argv: list[str], errata_path: Path,
                     sandboy_bin: Path, sandboy_commit_sha: str,
                     toolchain_capture_fn, toolchain_env_values: dict[str, str],
                     canonical_stream: str, primary_stream_rationale: str,
                     project_writable_dirs_relative: list[str],
                     requested_version_or_range: str, resolver_mechanism: str,
                     trusted_setup_fn=None, argv0_override: str | None = None,
                     project_writable_files_relative: list[str] = ()) -> dict:
    """Runs exactly one capture (capture-a or capture-b) for one
    repository-miner case. `trusted_setup_fn(source_root) -> dict | None` is
    called with network allowed, before the confined run; its return value
    (if any) is recorded as `trusted_dependency_realization`. `frozen_argv`
    must be the case's untouched, pure frozen (or erratum-corrected) argv --
    `argv0_override`, if given, replaces argv[0] (e.g. an interpreter's
    venv-absolute path) AFTER erratum resolution, never before it, so the
    erratum's exact-match comparison is never broken by a caller-side
    substitution (a real capture showed this fail for repo-pyflakes:
    resolve_effective_argv compared an already-venv-substituted argv against
    the erratum's pure original_frozen_argv and refused it as stale)."""
    work_dir.mkdir(parents=True, exist_ok=True)
    out_dir.mkdir(parents=True, exist_ok=True)

    receipt_json = json.loads((source_artifact_dir / "acquisition-receipt.json").read_text())
    source_tar = source_artifact_dir / "source.tar"
    source_root = work_dir / "source"
    verify_and_extract_source(source_tar, receipt_json["normalized_archive_sha256"], source_root)

    effective_argv, resolution, erratum_sha256 = resolve_effective_argv(case_id, frozen_argv, errata_path)
    if argv0_override:
        effective_argv = [argv0_override, *effective_argv[1:]]

    toolchain_identity_raw = toolchain_capture_fn(source_root)
    identity = toolchain_identity.build_toolchain_identity(
        requested_version_or_range=requested_version_or_range,
        resolver_mechanism=resolver_mechanism,
        resolved_version=toolchain_identity_raw.get("resolved_version"),
        runtime_identifier=toolchain_identity_raw.get("runtime_identifier"),
        resolved_executable_path=(
            toolchain_identity_raw.get("rustc_binary_path")
            or toolchain_identity_raw.get("python_binary_path")
            or toolchain_identity_raw.get("mvn_binary_path")
            or toolchain_identity_raw.get("gradle_binary_path")
        ),
        executed_binary_absolute_path=(
            toolchain_identity_raw.get("rustc_binary_path")
            or toolchain_identity_raw.get("python_binary_path")
            or toolchain_identity_raw.get("mvn_binary_path")
            or toolchain_identity_raw.get("gradle_binary_path")
        ),
        executed_binary_sha256=(
            toolchain_identity_raw.get("rustc_binary_sha256")
            or toolchain_identity_raw.get("python_binary_sha256")
            or toolchain_identity_raw.get("mvn_binary_sha256")
            or toolchain_identity_raw.get("gradle_binary_sha256")
        ),
        executed_argv0=effective_argv[0],
    )
    if toolchain_identity.is_hard_failure(identity["toolchain_executed"]["classification"]):
        raise GenericCaptureFailure(f"{case_id}/{job_name}: toolchain identity is incomplete: {toolchain_identity_raw}")

    trusted_setup_result = trusted_setup_fn(source_root) if trusted_setup_fn else None

    home_dir = work_dir / "home"
    tmp_dir = work_dir / "tmp"
    capture_out_dir = work_dir / "capture-out"
    for d in (home_dir, tmp_dir, capture_out_dir):
        d.mkdir(parents=True, exist_ok=True)
    writable_dirs = [source_root / rel for rel in project_writable_dirs_relative]
    for d in writable_dirs:
        d.mkdir(parents=True, exist_ok=True)
    # Some projects' own pom.xml/build.gradle write specific FILES directly
    # into the (otherwise read-only) source root rather than into a known
    # build-output directory -- a real capture showed repo-docker-java-
    # parser's pom.xml bind maven-dependency-plugin:tree to the default
    # execution, serializing to "dependency.tree" at the project root, which
    # failed with Permission denied under a read-only source_root. Pre-touch
    # (never truncate an already-populated file) and grant fs_rw to the
    # EXACT file, not the whole source root.
    writable_files = [source_root / rel for rel in project_writable_files_relative]
    for f in writable_files:
        f.parent.mkdir(parents=True, exist_ok=True)
        f.touch(exist_ok=True)

    policy_path = work_dir / "policy.toml"
    policy_sha256, canonical_policy_sha256 = gsp.write_policy(
        policy_path, work_dir=work_dir, ecosystem=ecosystem, case_id=case_id,
        source_root=source_root, home_dir=home_dir, tmp_dir=tmp_dir,
        capture_out_dir=capture_out_dir, project_writable_dirs=writable_dirs + writable_files,
        env_values=toolchain_env_values,
    )

    launcher_env = {"PATH": os.environ.get("PATH", "/usr/bin:/bin"), "HOME": str(home_dir), "TMPDIR": str(tmp_dir)}
    launcher_env.update(toolchain_env_values)

    # D1b-authorized, per exact case_id (never a whole ecosystem --
    # gsp.NETWORK_ENFORCEMENT_AUTHORIZED_CASES is the single source of
    # truth): this case's policy disables Sandboy's own Landlock TCP
    # mediation (network_enforcement_mode = "outer-netns-loopback-only") --
    # verify, in the EXACT same envelope (same sandboy_bin, same
    # policy_path, same source_root, same env) the capture that follows
    # uses, that this narrow exception is genuinely what it claims: real
    # external connectivity still blocked, real dynamic loopback
    # bind+connect genuinely allowed. Never asserted, always independently
    # re-proven on every authorized job. The workload never starts if
    # either probe fails.
    network_enforcement_report = None
    if case_id in gsp.NETWORK_ENFORCEMENT_AUTHORIZED_CASES:
        network_enforcement_report = network_enforcement_probe.run_network_enforcement_checks(
            case_id=case_id, sandboy_bin=sandboy_bin, policy_path=policy_path, cwd=source_root, env=launcher_env,
        )
        (out_dir / "network-enforcement-probe-report.json").write_text(
            json.dumps(network_enforcement_report, indent=2, sort_keys=True) + "\n"
        )
        if not network_enforcement_report["enforcement_exception_verified"]:
            raise GenericCaptureFailure(
                f"{case_id}/{job_name}: network_enforcement_mode=outer-netns-loopback-only failed live "
                f"verification: external_connectivity_confirmed_blocked="
                f"{network_enforcement_report['external_connectivity_confirmed_blocked']} "
                f"loopback_bind_connect_confirmed_allowed="
                f"{network_enforcement_report['loopback_bind_connect_confirmed_allowed']}"
            )

    verify_relative_argv0_exists(effective_argv[0], source_root)
    result = capture_build.run_real_build(sandboy_bin, policy_path, source_root, effective_argv, launcher_env)

    (out_dir / "raw.stdout").write_bytes(result["raw_stdout"])
    (out_dir / "raw.stderr").write_bytes(result["raw_stderr"])

    sanitized_stdout, stdout_report = sanitize(result["raw_stdout"], tmp_root=str(work_dir))
    sanitized_stderr, stderr_report = sanitize(result["raw_stderr"], tmp_root=str(work_dir))

    # `canonical_bytes` is the FULL (uncapped) selected stream -- immutable
    # raw execution evidence, never itself mutated below.
    canonical_bytes = result["raw_stdout"] if canonical_stream == "stdout" else result["raw_stderr"]
    raw_selected_capped = apply_durable_byte_cap(canonical_bytes)

    # Fail-closed content-acceptance gate: a schema-valid receipt is NOT a
    # valid capture. Real inspection of CI run #6's 18 "successful" captures
    # found every one was an infrastructure/sandbox failure (empty or
    # non-workload stdout) that nothing here had ever validated against.
    # This report is written for BOTH accepted and rejected captures. Always
    # run against the REAL raw (capped) selected stream -- never the
    # canonicalized derivation below -- so a canonicalization rule can never
    # mask a genuine content failure.
    content_report = content_acceptance.validate_capture_content(
        case_id=case_id, canonical_stream_bytes=raw_selected_capped,
        raw_stdout=result["raw_stdout"], raw_stderr=result["raw_stderr"],
        exit_code=result["exit_code"], execution_argv_resolution=resolution,
    )
    (out_dir / "content-validation-report.json").write_text(
        json.dumps(content_report, indent=2, sort_keys=True) + "\n"
    )
    if not content_report["accepted"]:
        raise GenericCaptureFailure(
            f"{case_id}/{job_name}: capture content rejected: {'; '.join(content_report['rejection_reasons'])}"
        )

    # D1b decision (2026-07-16): for case_ids named in
    # capture-canonicalization-policy.json, the canonical benchmark input is
    # a deterministic derivation of the full raw selected stream through a
    # narrowly scoped, self-hash-locked canonicalization profile -- never a
    # byte-prefix of raw stdout, always a documented transformation. Every
    # other case is untouched: its canonical benchmark input remains the
    # raw, capped, selected stream verbatim, exactly as before this
    # decision.
    canonicalization_report = None
    if case_id in CANONICALIZED_CASE_IDS:
        canon_module = CANONICALIZATION_MODULE_BY_CASE_ID[case_id]
        try:
            canonical_pre_cap_bytes, canonicalization_report = canon_module.canonicalize_stream(canonical_bytes)
        except canon_module.CanonicalizerError as e:
            raise GenericCaptureFailure(f"{case_id}/{job_name}: canonicalization failed: {e}") from e
        canonical_input_derivation = "case-specific-deterministic-canonicalization"
        (out_dir / "canonicalization-report.json").write_text(
            json.dumps(canonicalization_report, indent=2, sort_keys=True) + "\n"
        )
    else:
        canonical_input_derivation = "raw-capped-stream"
        canonical_pre_cap_bytes = canonical_bytes

    canonical_capped = apply_durable_byte_cap(canonical_pre_cap_bytes)
    try:
        canonical_capped.decode("utf-8", errors="strict")
        canonical_benchmark_input_is_valid_utf8 = True
    except UnicodeDecodeError:
        canonical_benchmark_input_is_valid_utf8 = False

    receipt = {
        "receipt_contract_version": "n2b-receipt-contract-v1",
        "case_id": case_id,
        "job": job_name,
        "source_identity": {
            "commit_sha": receipt_json["actual_head_sha"],
            "archive_sha256": receipt_json["normalized_archive_sha256"],
        },
        "license_identity": {"spdx": None, "sha256": receipt_json["license_sha256"]},
        "acquisition_identity": {
            "checkout_action_identity": "n2c-durable-release:n2d0-durable-evidence-v1",
            "persist_credentials": False,
        },
        "adapter_identity": {"name": f"n2d1b-generic-capture-{ecosystem}", "version": "1"},
        **identity,
        # Provenance-only fields, present for every ecosystem's receipt but
        # only ever populated (non-None) for python: platform.platform()
        # (host_runtime_identifier) embeds the runner's own kernel release,
        # not Python toolchain identity, so it is recorded here for evidence
        # but deliberately EXCLUDED from toolchain_resolved.runtime_identifier
        # (which is instead a stable Python ABI identity for python --
        # see ecosystem_toolchain.capture_python_toolchain_identity) and
        # never compared across independent runners in a pairwise
        # reproducibility check.
        "host_runtime_identifier": toolchain_identity_raw.get("host_runtime_identifier"),
        "toolchain_identity_provenance": {
            "python_version_raw": toolchain_identity_raw.get("python_version_raw"),
            "sys_implementation_name": toolchain_identity_raw.get("sys_implementation_name"),
            "sys_implementation_cache_tag": toolchain_identity_raw.get("sys_implementation_cache_tag"),
            "sysconfig_soabi": toolchain_identity_raw.get("sysconfig_soabi"),
            "sysconfig_platform": toolchain_identity_raw.get("sysconfig_platform"),
            "resolved_base_interpreter_path": toolchain_identity_raw.get("resolved_base_interpreter_path"),
            "resolved_base_interpreter_sha256": toolchain_identity_raw.get("resolved_base_interpreter_sha256"),
            "executed_venv_interpreter_path": toolchain_identity_raw.get("executed_venv_interpreter_path"),
            "executed_venv_interpreter_sha256": toolchain_identity_raw.get("executed_venv_interpreter_sha256"),
            "setup_python_action_commit": toolchain_identity_raw.get("setup_python_action_commit"),
        },
        "sandbox_identity": {"sandboy_commit_sha": sandboy_commit_sha, "policy_sha256": policy_sha256},
        # The raw policy_sha256 above embeds this job's own absolute
        # work_dir path (e.g. capture-a vs capture-b's separate runner
        # temp dirs) and so is EXPECTED to differ between capture-a and
        # capture-b even when every semantic policy field is identical --
        # canonical_policy_sha256 (gsp.canonical_policy_text's <WORKDIR>
        # substitution) is the field a pairwise reproducibility check must
        # compare instead.
        "canonical_policy_sha256": canonical_policy_sha256,
        "outer_isolation": {"network_isolation": True, "wall_clock_timeout_s": 1200},
        "resource_limits": {
            "cpu_time_limit_s": 600, "process_count_limit": 512,
            "memory_enforcement_mechanism": "outer-runner-enforced",
        },
        "frozen_execution_argv": frozen_argv,
        "effective_execution_argv": effective_argv,
        "execution_argv_resolution": resolution,
        "execution_argv_erratum_sha256": erratum_sha256,
        "command_argv": effective_argv,
        "environment_variable_names": sorted(launcher_env.keys()),
        "stdout_identity": {"sha256": sha256_bytes(result["raw_stdout"])},
        "stderr_identity": {"sha256": sha256_bytes(result["raw_stderr"])},
        "termination": {"exit_code": result["exit_code"]},
        "sanitization": {
            "profile_version": "n2a-canary-sanitizer-v1",
            "transformations": dedupe_sanitizer_rule_names(stdout_report, stderr_report),
        },
        "reproducibility": {"class": "expected-semantically-reproducible"},
        "trusted_dependency_realization": trusted_setup_result,
        "canonical_stream": canonical_stream,
        "canonical_stream_rationale": primary_stream_rationale,
        "canonical_input_derivation": canonical_input_derivation,
        "raw_selected_stream_identity": {
            "sha256": sha256_bytes(canonical_bytes),
            "byte_size": len(canonical_bytes),
        },
        "canonicalization_policy_sha256": (
            CANONICALIZATION_POLICY_SHA256_BY_CASE_ID.get(case_id) if canonicalization_report is not None else None
        ),
        "canonicalization_report_sha256": (
            sha256_bytes((json.dumps(canonicalization_report, indent=2, sort_keys=True) + "\n").encode())
            if canonicalization_report is not None else None
        ),
        "canonicalization_transformations": (
            [
                {"rule_name": name, "match_count": count}
                for name, count in canonicalization_report["rule_match_counts"].items()
            ] if canonicalization_report is not None else []
        ),
        "canonical_pre_cap_identity": {
            "sha256": sha256_bytes(canonical_pre_cap_bytes),
            "byte_size": len(canonical_pre_cap_bytes),
        },
        "canonical_raw_input_sha256": sha256_bytes(canonical_capped),
        "canonical_raw_input_byte_size": len(canonical_capped),
        "canonical_benchmark_input_is_valid_utf8": canonical_benchmark_input_is_valid_utf8,
        "raw_capture_content_classification": content_report["content_classification"],
        "wall_time_s": result["wall_time_s"],
        "peak_rss_kb": result["peak_rss_kb"],
        "sanitized_stdout_sha256": stdout_report["sanitized_sha256"],
        "sanitized_stderr_sha256": stderr_report["sanitized_sha256"],
        "content_validation_report_sha256": sha256_bytes(
            (json.dumps(content_report, indent=2, sort_keys=True) + "\n").encode()
        ),
        "network_enforcement_exception": network_enforcement_report,
    }
    schema_errors = receipt_contract.validate_receipt(receipt)
    if schema_errors:
        raise GenericCaptureFailure(f"{case_id}/{job_name}: receipt failed schema validation: {schema_errors}")

    (out_dir / "canonical-raw-input.bin").write_bytes(canonical_capped)
    (out_dir / "receipt.json").write_text(json.dumps(receipt, indent=2, sort_keys=True) + "\n")
    (out_dir / "sanitization-report.json").write_text(
        json.dumps({"stdout": stdout_report, "stderr": stderr_report}, indent=2, sort_keys=True) + "\n"
    )
    return receipt
