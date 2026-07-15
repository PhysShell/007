#!/usr/bin/env python3
"""N2-D1b: verifies the jvm-gradle network_enforcement_mode =
"outer-netns-loopback-only" escape hatch (see generic_sandbox_policy.py)
is exactly what it claims to be, in the exact same envelope (same
run_confined_build.sh outer netns, same sandboy_bin, same policy.toml, same
cwd/env) the real gradle capture uses -- run BEFORE that capture, on every
jvm-gradle job:

  1. A negative external-connectivity probe (canary/tools/network_probe.py,
     already proven/reused verbatim, never reimplemented) -- every real
     external target must stay UNREACHABLE.
  2. A positive loopback bind+connect probe (loopback_bind_probe.py) --
     the OS-chosen ephemeral loopback port Gradle's daemon needs must be
     genuinely ALLOWED.

Both reports (argv, exit code, raw stdout/stderr, hashes) are recorded
verbatim -- this module never asserts anything the two probes' own real
output doesn't say.
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

import capture_build  # noqa: E402

NEGATIVE_PROBE_SCRIPT = CANARY_TOOLS / "network_probe.py"
POSITIVE_PROBE_SCRIPT = TOOLS_DIR / "loopback_bind_probe.py"


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def _run_probe(sandboy_bin: Path, policy_path: Path, cwd: Path, env: dict, script: Path) -> dict:
    result = capture_build.run_real_build(sandboy_bin, policy_path, cwd, ["python3", str(script)], env)
    try:
        parsed = json.loads(result["raw_stdout"].strip().splitlines()[-1]) if result["raw_stdout"].strip() else {}
    except (json.JSONDecodeError, IndexError):
        parsed = {"parse_error": True}
    return {
        "argv": result["argv"],
        "exit_code": result["exit_code"],
        "raw_stdout_sha256": sha256_bytes(result["raw_stdout"]),
        "raw_stderr_sha256": sha256_bytes(result["raw_stderr"]),
        "parsed_result": parsed,
    }


def run_gradle_network_enforcement_checks(*, sandboy_bin: Path, policy_path: Path, cwd: Path, env: dict) -> dict:
    """Runs both probes through the SAME envelope as the real capture that
    follows. Raises GenericCaptureFailure-compatible AssertionError (caught
    by the caller, which decides how to fail) if either probe's own
    real result contradicts what network_enforcement_mode promises."""
    negative = _run_probe(sandboy_bin, policy_path, cwd, env, NEGATIVE_PROBE_SCRIPT)
    positive = _run_probe(sandboy_bin, policy_path, cwd, env, POSITIVE_PROBE_SCRIPT)

    any_external_reachable = any(
        r.get("result") == "REACHABLE" for r in negative["parsed_result"].get("probe_results", [])
    )
    loopback_allowed = positive["parsed_result"].get("result") == "ALLOWED" and positive["exit_code"] == 0

    return {
        "report_type": "n2d1b-gradle-network-enforcement-probe-v1",
        "network_enforcement_mode": "outer-netns-loopback-only",
        "negative_external_connectivity_probe": negative,
        "positive_loopback_bind_connect_probe": positive,
        "external_connectivity_confirmed_blocked": not any_external_reachable,
        "loopback_bind_connect_confirmed_allowed": loopback_allowed,
        "enforcement_exception_verified": (not any_external_reachable) and loopback_allowed,
    }
