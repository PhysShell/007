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

TOOLS_DIR = Path(__file__).resolve().parent
CANARY_TOOLS_DIR = TOOLS_DIR.parents[1] / "canary" / "tools"

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
        # A real capture (CI run #8, once RUSTUP_TOOLCHAIN actually started
        # reaching the confined process) showed rustc's linker step fail
        # with "Cannot create temporary file in /tmp/: Permission denied" --
        # the system linker (cc/ld) hardcodes /tmp for its own temp object
        # files regardless of TMPDIR. Same class of gap as dotnet's real
        # /tmp/.dotnet/shm finding (N2-A): the job's own dedicated tmp_dir
        # isn't enough, the real system /tmp itself must be fs_rw too.
        "extra_fs_rw_fixed": [Path("/tmp")],
    },
    "python": {
        "env_allow": ["PATH", "HOME", "TMPDIR", "PYTHONDONTWRITEBYTECODE", "PIP_NO_INDEX", "VIRTUAL_ENV"],
        # The venv root (pyvenv.cfg, site-packages, the interpreter binary
        # itself) lives under $RUNNER_TEMP, entirely outside source_root --
        # a real capture showed Python fail to even start with
        # "PermissionError: .../pyvenv.cfg" until this exact directory was
        # made fs_ro-visible. Read+execute only; the interpreter never needs
        # to write into its own venv during a capture.
        # PYTHON_BASE_INTERPRETER_ROOT is a synthetic, policy-only key (never
        # itself forwarded to the confined child -- nothing reads this env
        # var at runtime) pointing at the pinned actions/setup-python install
        # root (e.g. /opt/hostedtoolcache/Python/3.12.3/x64), which the venv's
        # bin/python is a symlink INTO. Real evidence (repo-pyflakes, CI run
        # 29466573023, after the setup-python pin let the venv's interpreter
        # identity resolve correctly): "sandboy: exec .../venv-repo-pyflakes/
        # bin/python: Permission denied (os error 13)" -- Sandboy's own
        # Landlock denial on the symlink's REAL TARGET, which BASELINE_FS_RO
        # never covers since hostedtoolcache is outside /usr,/bin,/lib. Same
        # class of gap as JAVA_HOME/DOTNET_ROOT: any toolchain installed
        # outside the baseline system paths needs its own explicit grant.
        "extra_fs_ro_from_env": ["VIRTUAL_ENV", "PYTHON_BASE_INTERPRETER_ROOT"],
        "extra_fs_rw_from_env": [],
    },
    "jvm-maven": {
        "env_allow": ["PATH", "HOME", "TMPDIR", "JAVA_HOME", "MAVEN_OPTS"],
        "extra_fs_ro_from_env": ["JAVA_HOME"],
        # MAVEN_LOCAL_REPO_PATH is a synthetic, policy-only key (never itself
        # forwarded to the confined child -- MAVEN_OPTS's own
        # -Dmaven.repo.local value is what Maven actually reads) pointing at
        # the REAL ~/.m2/repository populated during trusted setup. fs_rw,
        # not fs_ro: Maven writes _remote.repositories/resolver-status.
        # markers into its local repo even when every artifact it needs is
        # already cached.
        # SBT_GLOBAL_BASE_PATH is likewise synthetic/policy-only -- MAVEN_
        # OPTS's own -Dsbt.global.base value is what zinc actually reads --
        # pointing at the REAL ~/.sbt populated during trusted setup (a
        # cache root entirely separate from ~/.m2, used by scala-maven-
        # plugin's embedded zinc compiler for its compiler-bridge cache; a
        # real capture, CI run #11, showed this hit the exact same
        # never-exposed-to-confined-HOME gap as ~/.m2 did).
        "extra_fs_rw_from_env": ["MAVEN_LOCAL_REPO_PATH", "SBT_GLOBAL_BASE_PATH"],
        # A real capture (CI run #10, once the compiler-bridge was actually
        # cached in ~/.m2) showed scala-maven-plugin unpack its
        # compiler-bridge sources jar via a hardcoded /tmp path --
        # "AccessDeniedException: /tmp/scala-maven-plugin-compiler-bridge-
        # sources...", ignoring TMPDIR. Same class of gap as rust's linker
        # and dotnet's /tmp/.dotnet/shm findings.
        "extra_fs_rw_fixed": [Path("/tmp")],
    },
    "jvm-gradle": {
        "env_allow": ["PATH", "HOME", "TMPDIR", "JAVA_HOME", "GRADLE_USER_HOME", "GRADLE_OPTS"],
        # GRADLE_M2_REPO_PATH is a synthetic, policy-only key (never itself
        # forwarded to the confined child -- nothing reads this env var at
        # runtime), pointing at the real ~/.m2/repository. Real evidence
        # (repo-spotless, CI run 29467180079): a p2/equo-based subproject
        # plugin hardcodes the ambient ~/.m2/repository for its own Maven-
        # style artifact cache (not GRADLE_USER_HOME), independent of which
        # Gradle ecosystem is in play -- "java.io.UncheckedIOException:
        # Failed to create MD5 hash for file: /home/runner/.m2/repository/
        # dev/equo/p2-data/queries/version (Permission denied)". Read-only:
        # the failing operation only computes a hash of an already-cached
        # file (the jvm-maven ecosystem's own MAVEN_LOCAL_REPO_PATH grants
        # fs_rw for its own case; nothing here writes to this path).
        "extra_fs_ro_from_env": ["JAVA_HOME", "GRADLE_M2_REPO_PATH"],
        "extra_fs_rw_from_env": ["GRADLE_USER_HOME"],
        # Real evidence (CI runs #9-#11): two independent, argv/env-only
        # attempts (GRADLE_OPTS, then gradle.properties) to make Gradle skip
        # its daemon both failed identically -- "java.net.BindException:
        # Permission denied" trying to bind its own client<->daemon loopback
        # IPC port. Gradle's daemon architecture always needs SOME loopback
        # TCP port, chosen by the OS, for this; Sandboy's Landlock tcp_bind
        # is a fixed port list (Landlock scopes ports, not addresses -- see
        # sandboy's own README) with no way to express "any port, loopback
        # only". The resulting network_enforcement_mode escape hatch is now
        # authorized per case_id (NETWORK_ENFORCEMENT_AUTHORIZED_CASES
        # below), not per ecosystem -- see build_policy().
    },
    "dotnet": {
        # Mirrors canary/tools/sandboy_policy.py's proven N2-A dotnet policy
        # verbatim (env_allow list, and the real /tmp fs_rw finding below) --
        # not reinvented independently of it.
        "env_allow": [
            "PATH", "HOME", "TMPDIR", "DOTNET_ROOT", "DOTNET_CLI_TELEMETRY_OPTOUT",
            "DOTNET_NOLOGO", "DOTNET_SKIP_FIRST_TIME_EXPERIENCE",
            "DOTNET_MULTILEVEL_LOOKUP", "DOTNET_GENERATE_ASPNET_CERTIFICATE",
            "NUGET_PACKAGES",
        ],
        # A real capture (repo-kubeops-generator, run #13, after the
        # --no-restore erratum let the process get past the frozen argv's
        # own implicit-restore/NU1301 failure) showed MSBuild fail with
        # "MSB4024: The imported project file
        # '.../microsoft.testing.platform/1.9.1/buildTransitive/net9.0/
        # Microsoft.Testing.Platform.props' could not be loaded. Access to
        # the path ... is denied." -- the trusted `dotnet restore` step ran
        # unconfined against the real, ambient $HOME, so NuGet's global
        # packages folder (the default $HOME/.nuget/packages, unless
        # NUGET_PACKAGES overrides it) landed there; the confined process's
        # own isolated HOME has no relationship to that real path, and
        # MSBuild still needs read access to already-restored packages'
        # .props/.targets even with --no-restore (it only skips re-running
        # the restore operation itself, not every file NuGet restored).
        # NUGET_PACKAGES is forwarded verbatim (not a MAVEN_OPTS-style
        # indirection -- NuGet reads this env var directly) so the confined
        # child resolves the exact same real path trusted setup populated.
        "extra_fs_ro_from_env": ["DOTNET_ROOT", "NUGET_PACKAGES"],
        "extra_fs_rw_from_env": [],
        # The dotnet CLI's first-run NuGet-migrations named mutex hardcodes
        # /tmp/.dotnet/shm (ignoring TMPDIR/HOME) -- a real N2-A canary run
        # showed EACCES here until the real system /tmp itself (not just the
        # job's own tmp_dir) was made fs_rw.
        "extra_fs_rw_fixed": [Path("/tmp")],
    },
}

# D1b authorization (2026-07-16): network_enforcement_mode is selected by
# explicit case_id, never by ecosystem alone -- "Do not grant it to the
# entire dotnet ecosystem" applies equally to jvm-gradle (a case needing this
# must stop for separate D1b review, per this policy's own narrow-scope
# discipline -- it is never inherited merely because a case shares an
# ecosystem with an already-authorized one). Real evidence for each:
#   - repo-kubeops-generator: VSTest's own SocketServer.Start (client<->
#     test-host communication) hit "System.Net.Sockets.SocketException (13):
#     Permission denied" binding its own loopback TCP port -- confirmed via
#     real CI run 29465040390 evidence AFTER the NuGet-packages fix let
#     trusted restore + compile genuinely succeed (D1b, 2026-07-16).
#   - repo-moshi: after repo-spotless's own separate rejection
#     (REJECTED_ACQUISITION_MODEL_INCOMPATIBLE -- see
#     repo-spotless-rejection-record.json) and its substitution into the
#     pilot's jvm-gradle slot, repo-moshi hit the identical class of Gradle-
#     daemon loopback-bind failure repo-spotless originally did --
#     "java.net.BindException: Permission denied" -- confirmed via real CI
#     run 29469116485 (D1b, 2026-07-16), separately authorized here (not
#     inherited from repo-spotless's now-revoked entry).
#
# REVOKED: repo-spotless's own entry (originally authorized 2026-07-15 on
# the identical daemon-bind evidence) is intentionally NOT present below --
# repo-spotless itself is rejected for an unrelated reason (its upstream
# build's unconditional git-ratchet dependency) and must not be silently
# restored to this authorized set.
NETWORK_ENFORCEMENT_AUTHORIZED_CASES = {
    "repo-kubeops-generator": "outer-netns-loopback-only",
    "repo-moshi": "outer-netns-loopback-only",
}


def _toml_str_list(paths: list[Path]) -> str:
    return "[" + ", ".join(f'"{p}"' for p in paths) + "]"


def build_policy(*, ecosystem: str, case_id: str, source_root: Path, home_dir: Path, tmp_dir: Path,
                  capture_out_dir: Path, project_writable_dirs: list[Path],
                  env_values: dict[str, str]) -> str:
    """`env_values` maps every name in this ecosystem's env_allow to its
    concrete, dedicated (non-host) value for THIS job -- used both to render
    fs_ro/fs_rw entries for env-pointed directories and (by the caller) as
    the actual child-process environment. `case_id` is required (not merely
    ecosystem) because network_enforcement_mode is authorized per exact
    case_id -- see NETWORK_ENFORCEMENT_AUTHORIZED_CASES."""
    if ecosystem not in ECOSYSTEM_POLICY_HINTS:
        raise ValueError(f"no policy hints for ecosystem {ecosystem!r}")
    hints = ECOSYSTEM_POLICY_HINTS[ecosystem]
    network_enforcement_mode = NETWORK_ENFORCEMENT_AUTHORIZED_CASES.get(case_id)

    fs_ro = list(BASELINE_FS_RO) + [source_root]
    for env_name in hints["extra_fs_ro_from_env"]:
        value = env_values.get(env_name)
        if value:
            fs_ro.append(Path(value))
    fs_ro.extend(hints.get("extra_fs_ro_fixed", []))
    if network_enforcement_mode:
        # The network-enforcement probes (canary/tools/network_probe.py,
        # d1-identity-lock/tools/loopback_bind_probe.py) run in the EXACT
        # same envelope/policy as the real capture that follows -- but no
        # ecosystem's own real argv ever needs to read a file from the 007
        # checkout itself (only from source_root), so fs_ro never granted
        # this. Real evidence (CI run #14): confined python3 failed with
        # "can't open file '.../network_probe.py': [Errno 13] Permission
        # denied" -- exit code 2 is Python's OWN "couldn't open the script"
        # convention, not Sandboy's config-error exit code as first
        # hypothesized (Sandboy itself started fine). Mirrors N2-A's own
        # proven sandboy_policy.py repo_tools_dir grant for the identical
        # reason.
        fs_ro.extend([CANARY_TOOLS_DIR, TOOLS_DIR])

    fs_rw = list(BASELINE_FS_RW) + [home_dir, tmp_dir, capture_out_dir, *project_writable_dirs]
    for env_name in hints["extra_fs_rw_from_env"]:
        value = env_values.get(env_name)
        if value:
            fs_rw.append(Path(value))
    fs_rw.extend(hints.get("extra_fs_rw_fixed", []))

    lines = [
        f"# Generated by generic_sandbox_policy.py for N2-D1b ({ecosystem}/{case_id}). Do not edit by hand.",
        f"fs_ro = {_toml_str_list(fs_ro)}",
        f"fs_rw = {_toml_str_list(fs_rw)}",
        "tcp_connect = []",
        "tcp_bind = []",
        f"env_allow = {_toml_str_list([Path(n) for n in hints['env_allow']])}",
    ]
    if network_enforcement_mode:
        lines.append(f'network_enforcement_mode = "{network_enforcement_mode}"')
    return "\n".join(lines) + "\n"


def canonical_policy_text(text: str, work_dir: Path) -> str:
    return text.replace(str(work_dir), "<WORKDIR>")


def write_policy(path: Path, *, work_dir: Path, **kwargs) -> tuple[str, str]:
    text = build_policy(**kwargs)
    path.write_text(text)
    raw_sha256 = hashlib.sha256(text.encode()).hexdigest()
    canonical_sha256 = hashlib.sha256(canonical_policy_text(text, work_dir).encode()).hexdigest()
    return raw_sha256, canonical_sha256
