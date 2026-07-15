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

# D1b decision identity for the one, narrow authorization under which an
# empty canonical stream is not an automatic rejection: repo-pyflakes may
# legitimately execute successfully and find zero violations. Authorized
# 2026-07-15, in response to real CI evidence (post venv/PRESERVE_ENV fix)
# showing repo-pyflakes run cleanly (exit 0, no infrastructure-failure
# signature, harmless-Landlock-warning-only stderr) against the authorized
# "pyflakes/" erratum with genuinely zero stdout bytes. Every other case
# keeps the unconditional nonempty-canonical-stream requirement.
PYFLAKES_EMPTY_OUTPUT_AUTHORIZATION_ID = "n2d1b-pyflakes-empty-output-authorization-2026-07-15"

_RUNTIME_FAILURE_STDERR_RE = re.compile(r"(Traceback \(most recent call last\)|^Error:|Exception:)", re.MULTILINE)


def _stderr_free_of_pyflakes_runtime_failure(stderr: bytes) -> bool:
    """True if stderr shows no Python traceback/exception/error -- after
    stripping the expected, harmless "Landlock only PARTIALLY enforced"
    warning line present on every real run (including genuinely successful
    ones), which must never itself count as a failure signal."""
    text = stderr.decode("utf-8", errors="replace")
    text_without_harmless_warning = "\n".join(
        line for line in text.splitlines() if "Landlock only PARTIALLY enforced" not in line
    )
    return not _RUNTIME_FAILURE_STDERR_RE.search(text_without_harmless_warning)

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
                              raw_stdout: bytes, raw_stderr: bytes, exit_code: int,
                              execution_argv_resolution: str | None = None) -> dict:
    """Returns a content-validation-report dict. Never raises itself --
    the caller (generic_capture.run_one_capture) decides whether to raise
    based on report['accepted'], so the report can always be written to
    disk first, including for a rejected capture.

    `execution_argv_resolution` is generic_capture.resolve_effective_argv's
    own resolution string ("frozen" or "authorized-n2d1b-erratum") -- it
    gates the one narrow repo-pyflakes empty-output authorization below;
    every other case's acceptance is unaffected by it."""
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

    # The ONE narrow, explicitly authorized exception to the global
    # nonempty-canonical-stream requirement: repo-pyflakes may legitimately
    # execute successfully and find zero violations. Every condition below
    # must hold -- this never applies to any other case_id, and a missing/
    # wrong execution_argv_resolution (e.g. an un-updated caller) fails
    # closed into the ordinary nonempty-required path.
    is_authorized_pyflakes_empty_result = (
        case_id == "repo-pyflakes"
        and len(canonical_stream_bytes) == 0
        and len(raw_stdout) == 0
        and exit_code == 0
        and infra_failure is None
        and execution_argv_resolution == "authorized-n2d1b-erratum"
        and _stderr_free_of_pyflakes_runtime_failure(raw_stderr)
    )

    if is_authorized_pyflakes_empty_result:
        accepted = is_valid_utf8 and term_allowed and infra_failure is None
        rejection_reasons = []
        content_classification = "successful-empty-domain-result"
        empty_output_authorized = True
        approving_decision_identity = PYFLAKES_EMPTY_OUTPUT_AUTHORIZATION_ID
    else:
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
        content_classification = "genuine-workload-output" if accepted else "rejected"
        empty_output_authorized = False
        approving_decision_identity = None

    return {
        "report_type": "n2d1b-content-validation-report-v1",
        "case_id": case_id,
        "exit_code": exit_code,
        "checks": checks,
        "infrastructure_failure_detected": infra_failure,
        "case_semantic_marker_description": semantic_description,
        "accepted": accepted,
        "rejection_reasons": rejection_reasons,
        "content_classification": content_classification,
        "empty_output_authorized": empty_output_authorized,
        "approving_decision_identity": approving_decision_identity,
    }
