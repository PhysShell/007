#!/usr/bin/env python3
"""N2-D1b: fail-closed content-acceptance gate for the D1b generic capture
engine. A schema-valid receipt is not a valid capture -- real inspection
of CI run #6's 18 "successful" captures found every single one was an
infrastructure/sandbox failure (empty or non-workload stdout), never
caught because nothing validated the actual captured bytes or workload
semantics. This module is that validation, run BEFORE a capture may report
success.
"""
from __future__ import annotations

import re

# Real infrastructure-failure signatures found by inspecting run #6's actual
# captured stderr bytes -- never invented, never guessed. Each is specific
# enough not to false-positive on legitimate workload output (e.g. NOT the
# generic "sandboy: warning: Landlock only PARTIALLY enforced" line, which
# is expected and harmless on every real run, including eventually-valid
# ones).
INFRASTRUCTURE_FAILURE_SIGNATURES: list[tuple[str, re.Pattern]] = [
    ("rustup-no-default-toolchain",
     re.compile(r"rustup could not choose a version of cargo to run.*no default is configured", re.DOTALL)),
    ("dev-null-permission-denied",
     re.compile(r"cannot create /dev/null: Permission denied")),
    ("python-venv-permission-denied",
     re.compile(r"PermissionError.*pyvenv\.cfg")),
    ("nuget-restore-failure-nu1301",
     re.compile(r"\bNU1301\b")),
    ("command-or-wrapper-not-found",
     re.compile(r"(command not found|No such file or directory.*\b(cargo|mvn|gradlew|dotnet|python3?)\b)")),
    ("sandbox-refused-to-run",
     re.compile(r"(Refusing to run unconfined|Landlock NOT enforced)")),
]


def detect_infrastructure_failure(stdout: bytes, stderr: bytes) -> str | None:
    """Returns the matched signature name, or None if no known
    infrastructure failure is present in either stream."""
    combined = (stdout + b"\n" + stderr).decode("utf-8", errors="replace")
    for name, pattern in INFRASTRUCTURE_FAILURE_SIGNATURES:
        if pattern.search(combined):
            return name
    return None


# Exit codes that are never a legitimate workload outcome for any case
# (shell "command not found" / "permission denied to exec" conventions) --
# everything else is case-specific and gated by the semantic marker instead
# of a blanket "must be zero" rule (repo-requests must accept real pytest
# test-failure exit codes; repo-pyflakes must accept its own violation-found
# exit code).
ABNORMAL_EXIT_CODES = {126, 127}


def termination_allowed(exit_code: int) -> bool:
    return exit_code not in ABNORMAL_EXIT_CODES and exit_code >= 0


def _cargo_test_semantic_marker(text: str) -> tuple[bool, str]:
    ok = bool(re.search(r"^test result: (ok|FAILED)\.", text, re.MULTILINE))
    return ok, "cargo test harness completion line ('test result: ...')"


def _hyperfine_semantic_marker(text: str) -> tuple[bool, str]:
    ok = bool(re.search(r"^hyperfine \d+\.\d+", text, re.MULTILINE))
    return ok, "hyperfine --version payload ('hyperfine <version>')"


def _maven_semantic_marker(text: str) -> tuple[bool, str]:
    ok = "BUILD SUCCESS" in text or bool(re.search(r"Tests run:\s*\d+", text))
    return ok, "Maven BUILD SUCCESS banner or 'Tests run: N' summary"


def _dotnet_test_semantic_marker(text: str) -> tuple[bool, str]:
    ok = bool(re.search(r"(Passed!|Failed!|Total tests:)", text))
    return ok, "VSTest completion summary (Passed!/Failed!/Total tests:)"


def _pytest_semantic_marker(text: str) -> tuple[bool, str]:
    ok = bool(re.search(r"=+ .*(passed|failed|error).* in [\d.]+s.*=+", text))
    return ok, "pytest final summary line ('=== N passed/failed in Ts ===')"


def _gradle_semantic_marker(text: str) -> tuple[bool, str]:
    ok = bool(re.search(r"(BUILD SUCCESSFUL|BUILD FAILED)", text))
    return ok, "Gradle build completion banner (BUILD SUCCESSFUL/BUILD FAILED)"


def _pyflakes_semantic_marker(text: str) -> tuple[bool, str]:
    # Redundant with the general nonempty-canonical-stream gate, but kept
    # explicit per case-specific acceptance requirements: pyflakes must
    # genuinely execute and produce nonempty stdout. A genuinely empty
    # result after successful execution is not silently accepted here or
    # anywhere else -- it fails the nonempty gate upstream in
    # validate_capture_content, which is the correct "stop and report".
    ok = len(text.strip()) > 0
    return ok, "nonempty pyflakes violation output"


CASE_SEMANTIC_VALIDATORS = {
    "repo-hyperfine": _hyperfine_semantic_marker,
    "repo-rustlings": _cargo_test_semantic_marker,
    "repo-dockerfile-parser-rs": _cargo_test_semantic_marker,
    "repo-docker-java-parser": _maven_semantic_marker,
    "repo-kubeops-generator": _dotnet_test_semantic_marker,
    "repo-requests": _pytest_semantic_marker,
    "repo-spotless": _gradle_semantic_marker,
    "repo-moshi": _gradle_semantic_marker,
    "repo-pyflakes": _pyflakes_semantic_marker,
}


class ContentAcceptanceFailure(Exception):
    pass


def validate_capture_content(*, case_id: str, canonical_stream_bytes: bytes,
                              raw_stdout: bytes, raw_stderr: bytes, exit_code: int) -> dict:
    """Returns a content-validation-report dict. Never raises itself --
    the caller (generic_capture.run_one_capture) decides whether to raise
    based on report['accepted'], so the report can always be written to
    disk first, including for a rejected capture."""
    try:
        canonical_text = canonical_stream_bytes.decode("utf-8", errors="strict")
        is_valid_utf8 = True
    except UnicodeDecodeError:
        canonical_text = canonical_stream_bytes.decode("utf-8", errors="replace")
        is_valid_utf8 = False

    is_nonempty = len(canonical_stream_bytes) > 0
    infra_failure = detect_infrastructure_failure(raw_stdout, raw_stderr)
    term_allowed = termination_allowed(exit_code)

    validator = CASE_SEMANTIC_VALIDATORS.get(case_id)
    if validator is None:
        semantic_ok, semantic_description = False, f"no case-specific semantic validator registered for {case_id!r}"
    else:
        semantic_ok, semantic_description = validator(canonical_text)

    checks = {
        "canonical_is_valid_utf8": is_valid_utf8,
        "canonical_is_nonempty": is_nonempty,
        "termination_allowed": term_allowed,
        "no_infrastructure_failure_detected": infra_failure is None,
        "case_semantic_marker_found": semantic_ok,
    }
    accepted = all(checks.values())

    rejection_reasons = []
    if not is_valid_utf8:
        rejection_reasons.append("canonical stream is not valid UTF-8")
    if not is_nonempty:
        rejection_reasons.append("canonical stream is empty")
    if not term_allowed:
        rejection_reasons.append(f"exit code {exit_code} is an abnormal/non-workload termination")
    if infra_failure is not None:
        rejection_reasons.append(f"detected known infrastructure failure: {infra_failure}")
    if not semantic_ok:
        rejection_reasons.append(f"case-specific semantic marker not found: {semantic_description}")

    return {
        "report_type": "n2d1b-content-validation-report-v1",
        "case_id": case_id,
        "exit_code": exit_code,
        "checks": checks,
        "infrastructure_failure_detected": infra_failure,
        "case_semantic_marker_description": semantic_description,
        "accepted": accepted,
        "rejection_reasons": rejection_reasons,
    }
