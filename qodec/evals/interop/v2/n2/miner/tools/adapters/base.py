"""Shared ToolAdapter contract (section 9).

Every ecosystem adapter module (dotnet_adapter.py, rust_adapter.py,
python_adapter.py, maven_adapter.py, gradle_adapter.py) must expose exactly
these module-level functions — this is a duck-typed interface, not an ABC,
because N2-B adapters are pure inspection/planning modules with no per-
instance state. `test_adapters.py` asserts every adapter in the registry
implements `REQUIRED_ADAPTER_FUNCTIONS`.

`detect`/`inspect` may only read files (names, manifests, lockfiles, config)
— they must never invoke a package manager, build tool, wrapper script, or
any repository-provided script. There is no `execute` function in this
contract at all; N2-B never runs third-party code.
"""
from __future__ import annotations

from pathlib import Path

REQUIRED_ADAPTER_FUNCTIONS = (
    "detect",
    "inspect",
    "validate_manifest",
    "plan_trusted_setup",
    "plan_untrusted_execution",
    "toolchain_identity_contract",
    "filesystem_policy_hints",
    "environment_allowlist",
    "network_requirements",
    "resource_limit_hints",
    "receipt_fields",
    "sanitizer_profile",
)

# Fields every receipt must carry regardless of ecosystem (section 16); each
# adapter's receipt_fields() must be a superset of this.
COMMON_RECEIPT_FIELDS = (
    "source_identity", "license_identity", "acquisition_identity", "adapter_identity",
    "toolchain_requested", "toolchain_resolved", "toolchain_executed", "sandbox_identity",
    "outer_isolation", "resource_limits", "command_argv", "environment_variable_names",
    "stdout_identity", "stderr_identity", "termination", "sanitization", "reproducibility",
)


def walk_files(root: Path, max_depth: int = 6) -> list[Path]:
    """Read-only directory walk (no execution, no symlink following) used by
    every adapter's detect(). Bounded depth guards against pathological
    fixture/candidate trees."""
    root = Path(root)
    out = []
    stack = [(root, 0)]
    while stack:
        current, depth = stack.pop()
        if depth > max_depth:
            continue
        try:
            entries = sorted(current.iterdir())
        except (FileNotFoundError, NotADirectoryError, PermissionError):
            continue
        for entry in entries:
            if entry.is_symlink():
                continue
            if entry.is_dir():
                if entry.name in (".git", "node_modules", "bin", "obj", "target", ".gradle"):
                    continue
                stack.append((entry, depth + 1))
            else:
                out.append(entry)
    return sorted(out)


def read_text(path: Path) -> str:
    return Path(path).read_text(errors="replace")


def generic_sanitizer_profile() -> dict:
    """Shared baseline transformation set; adapters may extend but never
    remove entries, and never add dedup/reorder/truncation transforms
    (forbidden by section 15)."""
    return {
        "profile_version": "n2b-sanitizer-profile-v1",
        "transformations": [
            "iso_timestamp",
            "workspace_root_path",
            "tmp_root_path",
            "pid_bracket",
            "ansi_csi",
            "ansi_osc",
            "crlf_normalize",
        ],
    }


def generic_resource_limit_hints() -> dict:
    return {
        "wall_clock_timeout_s": 900,
        "cpu_time_limit_s": 600,
        "process_count_limit": 512,
        "memory_enforcement_mechanism": "outer-runner-enforced",
        "rejected_mechanisms": ["RLIMIT_AS"],
        "rejected_mechanisms_reason": (
            "N2-A found RLIMIT_AS (ulimit -v) makes CoreCLR fail startup with a "
            "misleading E_OUTOFMEMORY at ~40ms; treated as a cross-ecosystem hazard, "
            "not just a dotnet-specific one, until proven safe per-runtime."
        ),
    }


def generic_filesystem_runtime_knowledge() -> dict:
    """Explicit runtime-path knowledge carried over from N2-A findings
    (section 13) — never applied unconditionally; each adapter decides which
    of these its own ecosystem's runtime actually needs, and states why."""
    return {
        "/proc": "read-only: cgroup/CPU/memory detection many managed runtimes perform at startup",
        "/sys": "read-only: cgroup/CPU/memory detection many managed runtimes perform at startup",
        "/tmp": "read-write: some runtimes hardcode named mutexes/shared-memory segments under /tmp",
        "/dev/urandom": "read-only: CSPRNG-backed GUID/random generation at startup",
        "/dev/random": "read-only: CSPRNG-backed GUID/random generation at startup",
    }
