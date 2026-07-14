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


def resolve_dotnet_bin(dotnet_root: str | None, dotnet_bin_hint: str = "dotnet") -> str:
    """Prefer the explicit, absolute `<DOTNET_ROOT>/dotnet` (the exact SDK
    `actions/setup-dotnet` installed) over PATH lookup. This matters beyond
    tidiness: the real build later runs through `sudo` (see
    run_confined_build.sh), and sudo's `secure_path` policy can silently
    replace PATH regardless of --preserve-env, which would make an
    unqualified "dotnet" resolve to whatever system-wide SDK the runner image
    ships (a real N2-A run showed this: the build actually executed under
    `/usr/share/dotnet/sdk/10.0.301/...`, not the requested 8.0.x). Resolving
    to an absolute path once, here, and using that SAME path for both the
    identity probe and the actual build argv, guarantees the two can never
    silently diverge."""
    if dotnet_root:
        candidate = Path(dotnet_root) / "dotnet"
        if candidate.is_file():
            return str(candidate)
    which = shutil.which(dotnet_bin_hint)
    if which:
        return which
    return dotnet_bin_hint


def capture_toolchain_identity(dotnet_bin: str) -> dict:
    """Trusted-setup step: `dotnet --info` identifies the exact SDK/runtime the
    canonical build ran against. Never run inside the Sandboy/network-isolated
    boundary — this is infrastructure identity, not repository-controlled
    output. `dotnet_bin` must be the same absolute path used for the actual
    build argv (see resolve_dotnet_bin) so the recorded identity can't drift
    from what actually executed."""
    r = subprocess.run([dotnet_bin, "--info"], capture_output=True, text=True, check=False)
    text = r.stdout
    # Real `dotnet --info` output indents every field line (e.g.
    # " Version:            8.0.404"), which a `^Version:` anchor (no
    # leading-whitespace tolerance) never matches — an earlier N2-A run
    # showed both sdk_version and runtime_identifier come back None for
    # exactly this reason, despite `dotnet --info` succeeding and printing
    # the real values. ".NET SDK:" is the first "Version:" field in the
    # output, ahead of the "Host:" section's own "Version:", so first-match
    # is the SDK version, not the shared host version.
    sdk_version = _first_match(r"^\s*Version:\s*(\S+)\s*$", text, flags=re.MULTILINE)
    rid = _first_match(r"^\s*RID:\s*(\S+)\s*$", text, flags=re.MULTILINE)
    base_path = _first_match(r"^\s*Base Path:\s*(\S+)\s*$", text, flags=re.MULTILINE)
    return {
        "dotnet_binary_path": dotnet_bin,
        "dotnet_binary_sha256": _sha256_file(Path(dotnet_bin)),
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


def realize_dependencies_trusted(source_root: Path, manifest: dict, dotnet_bin: str) -> dict:
    """TRUSTED SETUP ONLY — run before the outer network namespace closes and
    before Sandboy. Not part of the captured canonical evidence. `dotnet_bin`
    must be the same resolved absolute path used everywhere else (see
    resolve_dotnet_bin)."""
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


def build_argv(manifest: dict, dotnet_bin: str) -> list[str]:
    """The manifest's argv with argv[0] ("dotnet") replaced by the resolved
    absolute binary path, so the actual build can't silently run a different
    SDK than the one identified in the receipt (see resolve_dotnet_bin)."""
    argv = list(manifest["build"]["argv"])
    if argv and argv[0] == "dotnet":
        argv[0] = dotnet_bin
    return argv


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
    import os

    resolved = resolve_dotnet_bin(os.environ.get("DOTNET_ROOT"))
    print(json.dumps(capture_toolchain_identity(resolved), indent=2, default=str), file=sys.stderr)
