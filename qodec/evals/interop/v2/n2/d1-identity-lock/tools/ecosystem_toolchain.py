#!/usr/bin/env python3
"""N2-D1b: per-ecosystem toolchain identity capture (trusted setup, never
inside the Sandboy/network-isolated boundary), for the 4 ecosystems N2-A's
dotnet_adapter.py does not cover. Mirrors dotnet_adapter.capture_toolchain_
identity's shape and discipline: resolve the binary to an absolute path
ONCE, probe real `--version`-style output, hash the binary, and never let
the identity probe and the actual executed binary diverge.
"""
from __future__ import annotations

import hashlib
import json
import re
import shutil
import subprocess
from pathlib import Path


def _sha256_file(path: Path) -> str | None:
    if not path or not Path(path).is_file():
        return None
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(65536), b""):
            h.update(chunk)
    return h.hexdigest()


def _resolve(bin_hint: str) -> str | None:
    which = shutil.which(bin_hint)
    return which or (bin_hint if Path(bin_hint).is_file() else None)


_ANSI_ESCAPE_RE = re.compile(r"\x1b\[[0-9;]*[a-zA-Z]")


def _strip_ansi(text: str) -> str:
    """Some installed `mvn`/`gradle` builds emit ANSI color codes on
    --version even with stdout piped (not a real terminal) -- a real capture
    showed 'mvn --version' returning '\\x1b[1mApache Maven 3.8.7\\x1b[m\\n...',
    which silently broke the '^Apache Maven' anchor and produced a None
    resolved_version (classified as an identity-missing hard failure). Strip
    escapes only for parsing; the raw captured text (with escapes) is still
    recorded as evidence."""
    return _ANSI_ESCAPE_RE.sub("", text)


def capture_rust_toolchain_identity(
    cargo_bin: str = "cargo", rustc_bin: str = "rustc", cwd: str | Path | None = None
) -> dict:
    import os

    cargo_path = _resolve(cargo_bin)
    rustc_path = _resolve(rustc_bin)
    rustc_r = subprocess.run([rustc_path or rustc_bin, "--version", "--verbose"],
                              capture_output=True, text=True, check=False, cwd=cwd)
    cargo_r = subprocess.run([cargo_path or cargo_bin, "--version"],
                              capture_output=True, text=True, check=False, cwd=cwd)
    resolved_version = _first_match(r"^release:\s*(\S+)\s*$", rustc_r.stdout, re.MULTILINE)
    commit_hash = _first_match(r"^commit-hash:\s*(\S+)\s*$", rustc_r.stdout, re.MULTILINE)
    host = _first_match(r"^host:\s*(\S+)\s*$", rustc_r.stdout, re.MULTILINE)

    # Real evidence for the "rustup could not choose a version of cargo to
    # run, because one wasn't specified explicitly, and no default is
    # configured" failure seen inside Sandboy confinement: capture rustup's
    # own view of toolchain resolution (outside confinement, where it always
    # worked) so the confined failure can be diagnosed and compared against
    # a known-good baseline, rather than guessed at.
    rustup_path = _resolve("rustup")
    active_toolchain_r = which_cargo_r = which_rustc_r = None
    if rustup_path:
        active_toolchain_r = subprocess.run([rustup_path, "show", "active-toolchain"],
                                             capture_output=True, text=True, check=False, cwd=cwd)
        which_cargo_r = subprocess.run([rustup_path, "which", "cargo"],
                                        capture_output=True, text=True, check=False, cwd=cwd)
        which_rustc_r = subprocess.run([rustup_path, "which", "rustc"],
                                        capture_output=True, text=True, check=False, cwd=cwd)

    return {
        "ecosystem": "rust",
        "rustc_binary_path": rustc_path,
        "rustc_binary_sha256": _sha256_file(rustc_path) if rustc_path else None,
        "cargo_binary_path": cargo_path,
        "cargo_binary_sha256": _sha256_file(cargo_path) if cargo_path else None,
        "rustc_version_verbose_exit_code": rustc_r.returncode,
        "rustc_version_verbose_raw": rustc_r.stdout,
        "cargo_version_raw": cargo_r.stdout.strip(),
        "resolved_version": resolved_version,
        "runtime_identifier": host,
        "commit_hash": commit_hash,
        "rustup_binary_path": rustup_path,
        "rustup_show_active_toolchain_raw": active_toolchain_r.stdout.strip() if active_toolchain_r else None,
        "rustup_show_active_toolchain_exit_code": active_toolchain_r.returncode if active_toolchain_r else None,
        "rustup_which_cargo_raw": which_cargo_r.stdout.strip() if which_cargo_r else None,
        "rustup_which_rustc_raw": which_rustc_r.stdout.strip() if which_rustc_r else None,
        "rustup_home": os.environ.get("RUSTUP_HOME"),
        "cargo_home": os.environ.get("CARGO_HOME"),
    }


def resolve_rustup_active_toolchain_name(cwd: str | Path | None = None) -> str | None:
    """Parses the toolchain name (e.g. "stable-x86_64-unknown-linux-gnu")
    out of `rustup show active-toolchain`'s first line, for use as an
    explicit RUSTUP_TOOLCHAIN override -- set because a real capture showed
    rustup fail to resolve ANY default toolchain purely from RUSTUP_HOME/
    settings.toml once inside Sandboy confinement (the shim's read of
    settings.toml apparently doesn't survive Landlock's partial enforcement
    intact); RUSTUP_TOOLCHAIN, if set, is documented to take precedence over
    settings.toml and needs no file read to resolve."""
    rustup_path = _resolve("rustup")
    if not rustup_path:
        return None
    r = subprocess.run([rustup_path, "show", "active-toolchain"], capture_output=True, text=True, check=False, cwd=cwd)
    if r.returncode != 0 or not r.stdout.strip():
        return None
    # First line looks like "stable-x86_64-unknown-linux-gnu (default)"
    return r.stdout.strip().splitlines()[0].split()[0]


def capture_python_toolchain_identity(python_bin: str = "python3", *, base_interpreter_path: str | None = None,
                                       setup_python_action_commit: str | None = None) -> dict:
    """`python_bin` is the executed venv interpreter (e.g. `$RUNNER_TEMP/
    venv-repo-pyflakes/bin/python`). `base_interpreter_path`, when given, is
    the pinned interpreter `actions/setup-python` resolved BEFORE the venv
    was created from it (D1b authorization 2026-07-16: pyflakes' python3 is
    no longer the GH runner's ambient, unpinned system interpreter -- two
    separate ephemeral runners were shown, by real CI evidence, to
    genuinely ship a different python3 binary).

    `runtime_identifier` here is a STABLE Python ABI identity (implementation
    + version + cache tag + SOABI + platform ABI) -- comparable across
    independent runners by construction. `platform.platform()` (which
    embeds the runner's own KERNEL release, not Python toolchain identity)
    is recorded separately as `host_runtime_identifier`, informational-only,
    never part of the strict pairwise-identity comparison."""
    python_path = _resolve(python_bin)
    r = subprocess.run([python_path or python_bin, "--version"], capture_output=True, text=True, check=False)
    # `python --version` writes to stdout on 3.4+; some very old builds wrote
    # to stderr -- check both rather than assume.
    version_text = (r.stdout or r.stderr).strip()
    resolved_version = _first_match(r"Python\s+(\S+)", version_text)

    abi_r = subprocess.run(
        [python_path or python_bin, "-c",
         "import json, sys, sysconfig; "
         "print(json.dumps({"
         "'implementation_name': sys.implementation.name, "
         "'cache_tag': sys.implementation.cache_tag, "
         "'soabi': sysconfig.get_config_var('SOABI'), "
         "'platform': sysconfig.get_platform(), "
         "}))"],
        capture_output=True, text=True, check=False,
    )
    try:
        abi = json.loads(abi_r.stdout.strip()) if abi_r.stdout.strip() else {}
    except json.JSONDecodeError:
        abi = {}

    platform_r = subprocess.run(
        [python_path or python_bin, "-c", "import platform; print(platform.platform())"],
        capture_output=True, text=True, check=False,
    )

    stable_abi_runtime_identifier = None
    if resolved_version and abi.get("implementation_name") and abi.get("cache_tag") and abi.get("platform"):
        # SOABI is legitimately None on some platforms (e.g. Windows) --
        # included when present, omitted (not a hyphen to a literal "None"
        # string) otherwise, so the identifier stays meaningful either way.
        parts = [abi["implementation_name"], resolved_version, abi["cache_tag"]]
        if abi.get("soabi"):
            parts.append(abi["soabi"])
        parts.append(abi["platform"])
        stable_abi_runtime_identifier = "-".join(parts)

    resolved_base_path = base_interpreter_path or python_path
    return {
        "ecosystem": "python",
        "python_binary_path": python_path,
        "python_binary_sha256": _sha256_file(python_path) if python_path else None,
        "python_version_raw": version_text,
        "resolved_version": resolved_version,
        "runtime_identifier": stable_abi_runtime_identifier,
        # Provenance-only, NOT compared across independent runners (embeds
        # the runner's own kernel release, not Python toolchain identity).
        "host_runtime_identifier": platform_r.stdout.strip() or None,
        "sys_implementation_name": abi.get("implementation_name"),
        "sys_implementation_cache_tag": abi.get("cache_tag"),
        "sysconfig_soabi": abi.get("soabi"),
        "sysconfig_platform": abi.get("platform"),
        "resolved_base_interpreter_path": resolved_base_path,
        "resolved_base_interpreter_sha256": _sha256_file(resolved_base_path) if resolved_base_path else None,
        "executed_venv_interpreter_path": python_path,
        "executed_venv_interpreter_sha256": _sha256_file(python_path) if python_path else None,
        "setup_python_action_commit": setup_python_action_commit,
    }


def capture_maven_toolchain_identity(mvn_bin: str = "mvn", java_home: str | None = None) -> dict:
    mvn_path = _resolve(mvn_bin)
    env = None
    if java_home:
        import os
        env = dict(os.environ)
        env["JAVA_HOME"] = java_home
        env["PATH"] = f"{java_home}/bin:{env.get('PATH', '')}"
    r = subprocess.run([mvn_path or mvn_bin, "--version"], capture_output=True, text=True, check=False, env=env)
    text = r.stdout
    clean_text = _strip_ansi(text)
    maven_version = _first_match(r"^Apache Maven\s+(\S+)", clean_text, re.MULTILINE)
    java_version = _first_match(r"^Java version:\s*([^,]+),", clean_text, re.MULTILINE)
    java_home_reported = _first_match(r"^Java version:.*runtime:\s*(\S+)\s*$", clean_text, re.MULTILINE)
    java_bin = None
    if java_home:
        candidate = Path(java_home) / "bin" / "java"
        java_bin = str(candidate) if candidate.is_file() else None
    return {
        "ecosystem": "jvm-maven",
        "mvn_binary_path": mvn_path,
        "mvn_binary_sha256": _sha256_file(mvn_path) if mvn_path else None,
        "mvn_version_raw": text,
        "resolved_version": maven_version,
        "runtime_identifier": (java_version or "").strip() or None,
        "java_home": java_home,
        "java_home_reported_by_mvn": java_home_reported,
        "java_binary_path": java_bin,
        "java_binary_sha256": _sha256_file(java_bin) if java_bin else None,
    }


def capture_gradle_toolchain_identity(
    gradle_bin: str = "gradle", java_home: str | None = None, cwd: str | Path | None = None
) -> dict:
    """`gradle_bin` is commonly a project-relative wrapper ("./gradlew") --
    `cwd` must be the extracted source tree root, or a relative wrapper path
    resolves against the calling process's own CWD instead (a real capture
    showed this fail with FileNotFoundError: './gradlew')."""
    gradle_path = _resolve(gradle_bin) if not str(gradle_bin).startswith(("./", "../")) else gradle_bin
    env = None
    if java_home:
        import os
        env = dict(os.environ)
        env["JAVA_HOME"] = java_home
        env["PATH"] = f"{java_home}/bin:{env.get('PATH', '')}"
    r = subprocess.run(
        [gradle_path or gradle_bin, "--version"], capture_output=True, text=True, check=False, env=env, cwd=cwd
    )
    text = r.stdout
    clean_text = _strip_ansi(text)
    gradle_version = _first_match(r"^Gradle\s+(\S+)\s*$", clean_text, re.MULTILINE)
    # Gradle 9.x's real --version output dropped the old "JVM:" line for
    # separate "Launcher JVM:"/"Daemon JVM:" lines -- a real capture against
    # this runner's installed Gradle 9.4.1 showed the old '^JVM:' anchor
    # never match at all, producing a None runtime_identifier (another
    # identity-missing hard failure). "Launcher JVM" is the JVM Gradle
    # itself actually launched under; check it first, falling back to the
    # older "JVM:" line for pre-9.x Gradle.
    jvm_version = _first_match(r"^(?:Launcher JVM|JVM):\s*(\S+)", clean_text, re.MULTILINE)
    gradle_binary_abs = str((Path(cwd) / gradle_path).resolve()) if cwd and gradle_path and not Path(gradle_path).is_absolute() else gradle_path
    return {
        "ecosystem": "jvm-gradle",
        "gradle_binary_path": gradle_binary_abs,
        "gradle_binary_sha256": _sha256_file(gradle_binary_abs) if gradle_binary_abs else None,
        "gradle_version_raw": text,
        "resolved_version": gradle_version,
        "runtime_identifier": jvm_version,
        "java_home": java_home,
    }


def _first_match(pattern: str, text: str, flags: int = 0) -> str | None:
    m = re.search(pattern, text, flags)
    return m.group(1) if m else None


CAPTURE_FUNCTIONS = {
    "rust": capture_rust_toolchain_identity,
    "python": capture_python_toolchain_identity,
    "jvm-maven": capture_maven_toolchain_identity,
    "jvm-gradle": capture_gradle_toolchain_identity,
}
