#!/usr/bin/env python3
"""N2-D1b: generalized Sandboy confinement policy generator, across the 5
ecosystems (dotnet, rust, jvm-maven, jvm-gradle, python).

Reuses canary/sandboy_policy.py's PROVEN mechanism (fs_ro/fs_rw/tcp_connect/
tcp_bind/env_allow TOML, the exact same accepted Sandboy commit, the same
canonicalization for reproducibility comparison) verbatim -- it is not
reimplemented here. This module only supplies the baseline system paths
every ecosystem needs (the same /proc,/sys,/dev/urandom items the N2-A
canary already had to discover the hard way) plus each ecosystem's own
toolchain root, dependency-cache directories, and environment variable
names -- rendered from N2-B's sandbox_planner.py planning hints, never
invented independently of them.
"""
from __future__ import annotations

import hashlib
from pathlib import Path

BASELINE_FS_RO = [
    Path("/usr"), Path("/bin"), Path("/lib"), Path("/lib64"), Path("/etc"),
    Path("/proc"), Path("/sys"), Path("/dev/urandom"), Path("/dev/random"),
]

# /dev/null needs read+write (shell redirection like `> /dev/null 2>&1`
# opens it for writing; some tools also read from it) -- a real capture
# showed mvn's and gradlew's own launcher scripts fail outright with
# "cannot create /dev/null: Permission denied" without this. Exactly this
# one device node, not the whole /dev tree.
BASELINE_FS_RW = [Path("/dev/null")]

# Per-ecosystem env_allow + extra fs_rw (dependency cache dirs that must be
# WRITABLE even under network-denied execution, since a case's dependencies
# were already realized there during trusted setup) + extra fs_ro (toolchain
# install roots beyond the project/source tree itself).
ECOSYSTEM_POLICY_HINTS = {
    "rust": {
        "env_allow": [
            "PATH", "HOME", "TMPDIR", "CARGO_HOME", "RUSTUP_HOME", "CARGO_NET_OFFLINE",
            "RUSTUP_TOOLCHAIN",
        ],
        "extra_fs_ro_from_env": ["RUSTUP_HOME"],
        "extra_fs_rw_from_env": ["CARGO_HOME"],
    },
    "python": {
        "env_allow": ["PATH", "HOME", "TMPDIR", "PYTHONDONTWRITEBYTECODE", "PIP_NO_INDEX", "VIRTUAL_ENV"],
        # The venv root (pyvenv.cfg, site-packages, the interpreter binary
        # itself) lives under $RUNNER_TEMP, entirely outside source_root --
        # a real capture showed Python fail to even start with
        # "PermissionError: .../pyvenv.cfg" until this exact directory was
        # made fs_ro-visible. Read+execute only; the interpreter never needs
        # to write into its own venv during a capture.
        "extra_fs_ro_from_env": ["VIRTUAL_ENV"],
        "extra_fs_rw_from_env": [],
    },
    "jvm-maven": {
        "env_allow": ["PATH", "HOME", "TMPDIR", "JAVA_HOME", "MAVEN_OPTS"],
        "extra_fs_ro_from_env": ["JAVA_HOME"],
        "extra_fs_rw_from_env": [],  # ~/.m2 lives under HOME, already fs_rw
    },
    "jvm-gradle": {
        "env_allow": ["PATH", "HOME", "TMPDIR", "JAVA_HOME", "GRADLE_USER_HOME"],
        "extra_fs_ro_from_env": ["JAVA_HOME"],
        "extra_fs_rw_from_env": ["GRADLE_USER_HOME"],
    },
    "dotnet": {
        # Mirrors canary/tools/sandboy_policy.py's proven N2-A dotnet policy
        # verbatim (env_allow list, and the real /tmp fs_rw finding below) --
        # not reinvented independently of it.
        "env_allow": [
            "PATH", "HOME", "TMPDIR", "DOTNET_ROOT", "DOTNET_CLI_TELEMETRY_OPTOUT",
            "DOTNET_NOLOGO", "DOTNET_SKIP_FIRST_TIME_EXPERIENCE",
            "DOTNET_MULTILEVEL_LOOKUP", "DOTNET_GENERATE_ASPNET_CERTIFICATE",
        ],
        "extra_fs_ro_from_env": ["DOTNET_ROOT"],
        "extra_fs_rw_from_env": [],
        # The dotnet CLI's first-run NuGet-migrations named mutex hardcodes
        # /tmp/.dotnet/shm (ignoring TMPDIR/HOME) -- a real N2-A canary run
        # showed EACCES here until the real system /tmp itself (not just the
        # job's own tmp_dir) was made fs_rw.
        "extra_fs_rw_fixed": [Path("/tmp")],
    },
}


def _toml_str_list(paths: list[Path]) -> str:
    return "[" + ", ".join(f'"{p}"' for p in paths) + "]"


def build_policy(*, ecosystem: str, source_root: Path, home_dir: Path, tmp_dir: Path,
                  capture_out_dir: Path, project_writable_dirs: list[Path],
                  env_values: dict[str, str]) -> str:
    """`env_values` maps every name in this ecosystem's env_allow to its
    concrete, dedicated (non-host) value for THIS job -- used both to render
    fs_ro/fs_rw entries for env-pointed directories and (by the caller) as
    the actual child-process environment."""
    if ecosystem not in ECOSYSTEM_POLICY_HINTS:
        raise ValueError(f"no policy hints for ecosystem {ecosystem!r}")
    hints = ECOSYSTEM_POLICY_HINTS[ecosystem]

    fs_ro = list(BASELINE_FS_RO) + [source_root]
    for env_name in hints["extra_fs_ro_from_env"]:
        value = env_values.get(env_name)
        if value:
            fs_ro.append(Path(value))

    fs_rw = list(BASELINE_FS_RW) + [home_dir, tmp_dir, capture_out_dir, *project_writable_dirs]
    for env_name in hints["extra_fs_rw_from_env"]:
        value = env_values.get(env_name)
        if value:
            fs_rw.append(Path(value))
    fs_rw.extend(hints.get("extra_fs_rw_fixed", []))

    lines = [
        f"# Generated by generic_sandbox_policy.py for N2-D1b ({ecosystem}). Do not edit by hand.",
        f"fs_ro = {_toml_str_list(fs_ro)}",
        f"fs_rw = {_toml_str_list(fs_rw)}",
        "tcp_connect = []",
        "tcp_bind = []",
        f"env_allow = {_toml_str_list([Path(n) for n in hints['env_allow']])}",
    ]
    return "\n".join(lines) + "\n"


def canonical_policy_text(text: str, work_dir: Path) -> str:
    return text.replace(str(work_dir), "<WORKDIR>")


def write_policy(path: Path, *, work_dir: Path, **kwargs) -> tuple[str, str]:
    text = build_policy(**kwargs)
    path.write_text(text)
    raw_sha256 = hashlib.sha256(text.encode()).hexdigest()
    canonical_sha256 = hashlib.sha256(canonical_policy_text(text, work_dir).encode()).hexdigest()
    return raw_sha256, canonical_sha256
