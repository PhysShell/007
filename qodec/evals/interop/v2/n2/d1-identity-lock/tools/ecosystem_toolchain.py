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


def capture_rust_toolchain_identity(cargo_bin: str = "cargo", rustc_bin: str = "rustc") -> dict:
    cargo_path = _resolve(cargo_bin)
    rustc_path = _resolve(rustc_bin)
    rustc_r = subprocess.run([rustc_path or rustc_bin, "--version", "--verbose"],
                              capture_output=True, text=True, check=False)
    cargo_r = subprocess.run([cargo_path or cargo_bin, "--version"],
                              capture_output=True, text=True, check=False)
    resolved_version = _first_match(r"^release:\s*(\S+)\s*$", rustc_r.stdout, re.MULTILINE)
    commit_hash = _first_match(r"^commit-hash:\s*(\S+)\s*$", rustc_r.stdout, re.MULTILINE)
    host = _first_match(r"^host:\s*(\S+)\s*$", rustc_r.stdout, re.MULTILINE)
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
    }


def capture_python_toolchain_identity(python_bin: str = "python3") -> dict:
    python_path = _resolve(python_bin)
    r = subprocess.run([python_path or python_bin, "--version"], capture_output=True, text=True, check=False)
    # `python --version` writes to stdout on 3.4+; some very old builds wrote
    # to stderr -- check both rather than assume.
    version_text = (r.stdout or r.stderr).strip()
    resolved_version = _first_match(r"Python\s+(\S+)", version_text)
    platform_r = subprocess.run(
        [python_path or python_bin, "-c", "import platform; print(platform.platform())"],
        capture_output=True, text=True, check=False,
    )
    return {
        "ecosystem": "python",
        "python_binary_path": python_path,
        "python_binary_sha256": _sha256_file(python_path) if python_path else None,
        "python_version_raw": version_text,
        "resolved_version": resolved_version,
        "runtime_identifier": platform_r.stdout.strip() or None,
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
    maven_version = _first_match(r"^Apache Maven\s+(\S+)", text, re.MULTILINE)
    java_version = _first_match(r"^Java version:\s*([^,]+),", text, re.MULTILINE)
    java_home_reported = _first_match(r"^Java version:.*runtime:\s*(\S+)\s*$", text, re.MULTILINE)
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


def capture_gradle_toolchain_identity(gradle_bin: str = "gradle", java_home: str | None = None) -> dict:
    gradle_path = _resolve(gradle_bin)
    env = None
    if java_home:
        import os
        env = dict(os.environ)
        env["JAVA_HOME"] = java_home
        env["PATH"] = f"{java_home}/bin:{env.get('PATH', '')}"
    r = subprocess.run([gradle_path or gradle_bin, "--version"], capture_output=True, text=True, check=False, env=env)
    text = r.stdout
    gradle_version = _first_match(r"^Gradle\s+(\S+)\s*$", text, re.MULTILINE)
    jvm_version = _first_match(r"^JVM:\s*(\S+)", text, re.MULTILINE)
    return {
        "ecosystem": "jvm-gradle",
        "gradle_binary_path": gradle_path,
        "gradle_binary_sha256": _sha256_file(gradle_path) if gradle_path else None,
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
