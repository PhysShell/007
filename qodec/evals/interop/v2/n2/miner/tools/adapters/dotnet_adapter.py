"""N2-B generic dotnet ToolAdapter — inspection + planning only.

Generalizes the N2-A dotnet_adapter.py (frozen, untouched) into the section-9
ToolAdapter contract. Never restores, builds, tests, or runs any repository
script — detect()/inspect() only read filenames and manifest text.
"""
from __future__ import annotations

import re
from pathlib import Path

from . import base

ECOSYSTEM = "dotnet"

_SDK_IMPORT_RE = re.compile(r'<Project\s+Sdk="Microsoft\.NET\.Sdk[^"]*"', re.IGNORECASE)
_IMPORT_RE = re.compile(r'<Import\s+Project="([^"]+)"', re.IGNORECASE)
_PACKAGE_REF_RE = re.compile(r"<PackageReference\b")
_PROJECT_REF_RE = re.compile(r"<ProjectReference\b")
_TARGET_FRAMEWORK_RE = re.compile(r"<TargetFramework>([^<]+)</TargetFramework>")
_TARGET_FRAMEWORKS_RE = re.compile(r"<TargetFrameworks>([^<]+)</TargetFrameworks>")


def detect(source_root: Path) -> dict:
    source_root = Path(source_root)
    files = base.walk_files(source_root)
    rel = lambda p: str(p.relative_to(source_root))  # noqa: E731

    csproj = [f for f in files if f.suffix == ".csproj"]
    sln = [f for f in files if f.suffix == ".sln"]
    global_json = [f for f in files if f.name == "global.json"]
    nuget_config = [f for f in files if f.name.lower() == "nuget.config"]
    directory_build = [f for f in files if f.name in ("Directory.Build.props", "Directory.Build.targets")]
    directory_packages = [f for f in files if f.name == "Directory.Packages.props"]
    lockfiles = [f for f in files if f.name == "packages.lock.json"]

    custom_imports = []
    for proj in csproj:
        text = base.read_text(proj)
        for m in _IMPORT_RE.finditer(text):
            target = m.group(1)
            if "$(MSBuildSDKsPath)" not in target and "Sdk.props" not in target and "Sdk.targets" not in target:
                custom_imports.append({"project": rel(proj), "import": target})

    ambiguities = []
    confidence = 1.0
    if len(csproj) == 0:
        confidence = 0.0
    elif len(csproj) > 1:
        confidence = 1.0 / len(csproj)
        ambiguities.append(f"{len(csproj)} .csproj files found; no automatic entry-point selection permitted")

    required_toolchain = None
    if global_json:
        text = base.read_text(global_json[0])
        m = re.search(r'"version"\s*:\s*"([^"]+)"', text)
        if m:
            required_toolchain = m.group(1)

    return {
        "ecosystem": ECOSYSTEM,
        "detected_project_roots": sorted({rel(p.parent) for p in csproj}),
        "detected_build_systems": ["msbuild", "dotnet-cli"] if csproj or sln else [],
        "candidate_entry_points": [rel(p) for p in csproj],
        "confidence": confidence,
        "ambiguous": len(csproj) != 1,
        "ambiguities": ambiguities,
        "required_toolchain": required_toolchain,
        "dependency_files": [rel(p) for p in nuget_config + directory_packages],
        "lockfiles": [rel(p) for p in lockfiles],
        "custom_scripts": [],
        "custom_imports": custom_imports,
        "network_risk_indicators": [rel(p) for p in nuget_config],
        "container_or_external_service_indicators": [],
        "directory_build_files": [rel(p) for p in directory_build],
    }


def inspect(source_root: Path, entry_point: str) -> dict:
    proj_path = Path(source_root) / entry_point
    text = base.read_text(proj_path)
    tf = _TARGET_FRAMEWORK_RE.search(text)
    tfs = _TARGET_FRAMEWORKS_RE.search(text)
    return {
        "entry_point": entry_point,
        "package_reference_count": len(_PACKAGE_REF_RE.findall(text)),
        "project_reference_count": len(_PROJECT_REF_RE.findall(text)),
        "target_framework": tf.group(1) if tf else None,
        "target_frameworks": tfs.group(1).split(";") if tfs else None,
        "uses_sdk_style_project": bool(_SDK_IMPORT_RE.search(text)),
    }


def validate_manifest(manifest: dict) -> list[str]:
    errors = []
    project = manifest.get("project", {})
    if manifest.get("ecosystem") != ECOSYSTEM:
        errors.append(f"ecosystem must be {ECOSYSTEM!r}")
    if not project.get("entry_point"):
        errors.append("project.entry_point is required")
    if project.get("ambiguous"):
        errors.append("ambiguous entry point must be resolved by explicit manifest selection before planning")
    return errors


def plan_trusted_setup(manifest: dict) -> dict:
    entry_point = manifest["project"]["entry_point"]
    return {
        "steps": [
            {
                "operation": "dependency_realization",
                "argv": ["dotnet", "restore", entry_point],
                "rationale": (
                    "obj/project.assets.json is an SDK-internal restore output that "
                    "--no-restore cannot skip on a first build, even with zero external "
                    "packages (N2-A finding) — this runs once as trusted setup, before "
                    "network isolation closes."
                ),
            },
        ],
    }


def plan_untrusted_execution(manifest: dict) -> dict:
    entry_point = manifest["project"]["entry_point"]
    return {
        "argv": ["dotnet", "build", entry_point, "--configuration", "Release",
                 "--no-restore", "--nologo", "--verbosity", "normal",
                 "-p:UseSharedCompilation=false", "--disable-build-servers", "-m:1",
                 "-p:BuildInParallel=false", "-p:RunAnalyzersInParallel=false"],
        "network_during_execution": "denied",
        "determinism_note": (
            "Scope N2-A.1: the shared Roslyn/VBCSCompiler compilation server can "
            "interleave CoreCompile diagnostics in different orders across "
            "independent builds even with identical source/toolchain/environment "
            "(observed in workflow run 29371996936). --disable-build-servers alone "
            "was proven insufficient under real Sandboy-confined, separately-"
            "provisioned capture (workflow run 29374881325); this full combination "
            "was proven deterministic across three independent real capture-a/"
            "capture-b comparisons (runs 29375074566, 29383202537, 29383433153). "
            "Every ecosystem plan produced by this adapter carries these controls, "
            "not just the N2-A reference case."
        ),
    }


def toolchain_identity_contract() -> dict:
    return {
        "requested_source": "global.json sdk.version, or the workflow's setup-dotnet version input range",
        "resolver_mechanism": "actions/setup-dotnet (or local SDK resolution honoring global.json rollForward)",
        "resolved_fields": ["resolved_version", "runtime_identifier", "resolved_executable_path"],
        "executed_fields": ["executed_argv0", "executed_binary_absolute_path", "executed_binary_sha256"],
        "classification_rule": (
            "resolved_version must equal the requested version, or fall within the requested "
            "range under a documented rollForward policy, to classify exact-match/"
            "compatible-resolution; any other resolved_version is unexpected-resolution — this is "
            "exactly the N2-A finding where a workflow requested 8.0.x but 10.0.301 executed."
        ),
    }


def filesystem_policy_hints(manifest: dict) -> dict:
    entry_point = Path(manifest["project"]["entry_point"])
    proj_dir = entry_point.parent
    runtime_knowledge = base.generic_filesystem_runtime_knowledge()
    return {
        "read_only": {"/proc": runtime_knowledge["/proc"], "/sys": runtime_knowledge["/sys"],
                      "/dev/urandom": runtime_knowledge["/dev/urandom"], "/dev/random": runtime_knowledge["/dev/random"]},
        "writable": {
            "/tmp": runtime_knowledge["/tmp"] + " (dotnet: NuGet-migrations mutex at /tmp/.dotnet/shm)",
            str(proj_dir / "bin"): "build output; must be pre-created before Landlock policy construction",
            str(proj_dir / "obj"): "restore/build intermediate output; must be pre-created before Landlock policy construction",
        },
        "must_pre_create": [str(proj_dir / "bin"), str(proj_dir / "obj")],
    }


def environment_allowlist(manifest: dict) -> list[str]:
    return [
        "DOTNET_CLI_TELEMETRY_OPTOUT", "DOTNET_GENERATE_ASPNET_CERTIFICATE",
        "DOTNET_MULTILEVEL_LOOKUP", "DOTNET_NOLOGO", "DOTNET_ROOT",
        "DOTNET_SKIP_FIRST_TIME_EXPERIENCE", "HOME", "PATH", "TMPDIR",
    ]


def network_requirements(manifest: dict) -> dict:
    return {"required_during_trusted_setup": False, "required_during_untrusted_execution": False}


def resource_limit_hints(manifest: dict) -> dict:
    hints = base.generic_resource_limit_hints()
    hints["notes"] = ["CoreCLR reserves virtual address space far beyond what it commits; RLIMIT_AS "
                       "made a real N2-A build fail in ~40ms with a misleading E_OUTOFMEMORY."]
    return hints


def receipt_fields(manifest: dict) -> tuple:
    return base.COMMON_RECEIPT_FIELDS


def sanitizer_profile() -> dict:
    profile = base.generic_sanitizer_profile()
    profile["transformations"] = profile["transformations"] + [
        "msbuild_build_started_banner", "msbuild_process_id", "dotnet_time_elapsed_line",
    ]
    return profile
