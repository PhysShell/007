#!/usr/bin/env python3
"""N2-A SingleToolAdapter for the pinned dotnet ecosystem (thin slice: exactly
one adapter, one project, one command — no generic adapter registry).

Responsibilities, per the N2-A addendum:
  - project validation (re-checked at capture time, independent of the
    acquisition-time check — each capture job gets its own fresh extraction)
  - toolchain identity (captured as TRUSTED SETUP, before network isolation)
  - offline build argv (fixed, from the reviewed source manifest)
  - expected writable directories / output locations (for the Sandboy policy)

Dependency-realization note (see also qodec-n2-miner-canary.yml comments):
`dotnet build --no-restore` on a project that has never been restored fails
with NETSDK1004 ("assets file not found") even with zero PackageReferences —
`obj/project.assets.json` is an SDK-internal restore output, not something
`--no-restore` can skip on a first build. So `dotnet restore` runs once, here,
as TRUSTED SETUP (before the outer network namespace closes and before
Sandboy) — this is exactly the addendum's preferred-order item 2, "dependencies
realized before untrusted execution". The manifest already establishes there
are zero PackageReference/ProjectReference entries, so this restore resolves
nothing external; it only ever touches the local SDK-provided reference-pack
metadata. The CAPTURED, canonical build (`--no-restore`, network-isolated,
Sandboy-confined) never talks to a package source at all.
"""
from __future__ import annotations

import hashlib
import re
import shutil
import subprocess
import sys
from pathlib import Path


def _sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def _sha256_file(path: Path) -> str | None:
    if not path.is_file():
        return None
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(65536), b""):
            h.update(chunk)
    return h.hexdigest()


def capture_toolchain_identity(dotnet_bin: str = "dotnet") -> dict:
    """Trusted-setup step: `dotnet --info` identifies the exact SDK/runtime the
    canonical build ran against. Never run inside the Sandboy/network-isolated
    boundary — this is infrastructure identity, not repository-controlled
    output."""
    resolved = shutil.which(dotnet_bin) or dotnet_bin
    r = subprocess.run([resolved, "--info"], capture_output=True, text=True, check=False)
    text = r.stdout
    sdk_version = _first_match(r"^Version:\s*(\S+)", text, flags=re.MULTILINE)
    rid = _first_match(r"^RID:\s*(\S+)", text, flags=re.MULTILINE)
    base_path = _first_match(r"^Base Path:\s*(\S+)", text, flags=re.MULTILINE)
    return {
        "dotnet_binary_path": resolved,
        "dotnet_binary_sha256": _sha256_file(Path(resolved)),
        "dotnet_info_exit_code": r.returncode,
        "dotnet_info_stdout_sha256": _sha256_bytes(r.stdout.encode()),
        "dotnet_info_raw": r.stdout,
        "sdk_version": sdk_version,
        "runtime_identifier": rid,
        "sdk_base_path": base_path,
    }


def _first_match(pattern: str, text: str, flags: int = 0) -> str | None:
    m = re.search(pattern, text, flags)
    return m.group(1) if m else None


def validate_project_before_execution(source_root: Path, manifest: dict) -> dict:
    proj_rel = manifest["project"]["path"]
    proj_path = source_root / proj_rel
    if not proj_path.is_file():
        raise FileNotFoundError(f"project file not found at capture time: {proj_path}")
    text = proj_path.read_text(errors="replace")
    pkg_refs = len(re.findall(r"<PackageReference\b", text))
    if pkg_refs != manifest["project"]["expected_package_reference_count"]:
        raise ValueError(
            f"capture-time PackageReference count {pkg_refs} != manifest expectation "
            f"{manifest['project']['expected_package_reference_count']} — reject, do not build"
        )
    return {"path": proj_rel, "sha256": _sha256_bytes(text.encode())}


def realize_dependencies_trusted(source_root: Path, manifest: dict, dotnet_bin: str = "dotnet") -> dict:
    """TRUSTED SETUP ONLY — run before the outer network namespace closes and
    before Sandboy. Not part of the captured canonical evidence."""
    proj_rel = manifest["project"]["path"]
    argv = [dotnet_bin, "restore", proj_rel]
    r = subprocess.run(argv, cwd=str(source_root), capture_output=True, text=True, check=False)
    return {
        "argv": argv,
        "exit_code": r.returncode,
        "stdout_sha256": _sha256_bytes(r.stdout.encode()),
        "stderr_sha256": _sha256_bytes(r.stderr.encode()),
        "note": "trusted-setup dependency realization; zero external packages per manifest; not captured evidence",
    }


def build_argv(manifest: dict) -> list[str]:
    return list(manifest["build"]["argv"])


def expected_writable_dirs(source_root: Path, manifest: dict) -> list[Path]:
    proj_dir = (source_root / manifest["project"]["path"]).parent
    return [proj_dir / "bin", proj_dir / "obj"]


def expected_output_locations(source_root: Path, manifest: dict) -> list[Path]:
    proj_dir = (source_root / manifest["project"]["path"]).parent
    return [proj_dir / "bin" / "Release" / manifest["project"]["expected_target_framework"]]


if __name__ == "__main__":
    # Minimal manual smoke: print toolchain identity so a workflow log shows
    # what actually ran, even before any capture step.
    import json

    print(json.dumps(capture_toolchain_identity(), indent=2, default=str), file=sys.stderr)
