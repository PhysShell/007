#!/usr/bin/env python3
"""N2-B SandboxExecutionPlanner (section 12).

Plans confinement using the ACCEPTED Sandboy identity only — never modifies
Sandboy, and never widens the S0/N2-A-proven capability envelope silently.
If an adapter's filesystem_policy_hints() requests something outside that
envelope, this planner reports a capability_gap and stops instead of
"fixing" Own.NET from within N2-B (an explicit stop condition, section 25).
"""
from __future__ import annotations

import fnmatch

# Accepted Sandboy S0 implementation — must not change in N2-B.
ACCEPTED_SANDBOY_COMMIT_SHA = "e925058ddea405b5821fc0aed4882c76650dcbe9"

# Filesystem capability envelope proven feasible by Sandboy S0 + the N2-A
# canary. Anything outside this set is a capability gap, not something this
# planner may silently grant.
PROVEN_READ_ONLY_PATTERNS = ("/proc", "/proc/*", "/sys", "/sys/*", "/dev/urandom", "/dev/random")
PROVEN_WRITABLE_PATTERNS = ("/tmp", "/tmp/*",
                            "bin", "bin/*", "*/bin", "*/bin/*",
                            "obj", "obj/*", "*/obj", "*/obj/*",
                            "target", "target/*", "*/target", "*/target/*",
                            "build", "build/*", "*/build", "*/build/*",
                            "~/.m2", "~/.m2/*", "~/.gradle", "~/.gradle/*")

MANDATORY_DEFAULT_DENY = (
    "credentials", "ssh_agent", "host_home", "docker_socket",
    "unrelated_workspace_paths", "external_network_during_untrusted_execution",
    "unbounded_process_creation", "unbounded_wall_time",
)


def _matches_any(path: str, patterns: tuple) -> bool:
    return any(fnmatch.fnmatch(path, pattern) for pattern in patterns)


def _check_capability_gaps(read_only: dict, writable: dict) -> list[str]:
    gaps = []
    for path in read_only:
        if not _matches_any(path, PROVEN_READ_ONLY_PATTERNS):
            gaps.append(f"read-only path {path!r} is outside the S0/N2-A-proven capability envelope")
    for path in writable:
        if not _matches_any(path, PROVEN_WRITABLE_PATTERNS):
            gaps.append(f"writable path {path!r} is outside the S0/N2-A-proven capability envelope")
    return gaps


def plan_sandbox_execution(manifest: dict, adapter) -> dict:
    fs_hints = adapter.filesystem_policy_hints(manifest)
    env_allowlist = adapter.environment_allowlist(manifest)
    resource_hints = adapter.resource_limit_hints(manifest)

    capability_gaps = _check_capability_gaps(fs_hints.get("read_only", {}), fs_hints.get("writable", {}))

    plan = {
        "sandboy_commit_sha": ACCEPTED_SANDBOY_COMMIT_SHA,
        "outer_disposable_vm_required": True,
        "outer_network_isolation_required": True,
        "outer_resource_limits": resource_hints,
        "filesystem_read_allowlist": fs_hints.get("read_only", {}),
        "filesystem_write_allowlist": fs_hints.get("writable", {}),
        "filesystem_must_pre_create": fs_hints.get("must_pre_create", []),
        "environment_allowlist": env_allowlist,
        "seccomp_deny_requirements": "inherit accepted Sandboy S0 syscall denylist; no relaxation",
        "child_process_inheritance_requirement": "policy applies to the full process tree, not just argv[0]",
        "capture_paths": ["raw.stdout", "raw.stderr", "sandboy-execution-receipt.json"],
        "receipt_paths": ["sandboy-execution-receipt.json", "snapshot-manifest.json"],
        "mandatory_default_deny": list(MANDATORY_DEFAULT_DENY),
        "capability_gaps": capability_gaps,
    }
    plan["status"] = "CAPABILITY_GAP" if capability_gaps else "PLANNED"
    return plan
