"""N2-B generic Rust ToolAdapter — inspection + planning only."""
from __future__ import annotations

import re
from pathlib import Path

from . import base

ECOSYSTEM = "rust"

_WORKSPACE_MEMBERS_RE = re.compile(r"\[workspace\][^\[]*members\s*=\s*\[([^\]]*)\]", re.DOTALL)
_GIT_DEP_RE = re.compile(r'git\s*=\s*"([^"]+)"')
_PATH_DEP_RE = re.compile(r'path\s*=\s*"([^"]+)"')


def detect(source_root: Path) -> dict:
    source_root = Path(source_root)
    files = base.walk_files(source_root)
    rel = lambda p: str(p.relative_to(source_root))  # noqa: E731

    manifests = [f for f in files if f.name == "Cargo.toml"]
    lockfiles = [f for f in files if f.name == "Cargo.lock"]
    build_rs = [f for f in files if f.name == "build.rs"]
    cargo_config = [f for f in files if f.name in ("config.toml", "config") and f.parent.name == ".cargo"]

    root_manifest = next((m for m in manifests if m.parent == source_root), None)
    workspace_members = []
    if root_manifest is not None:
        text = base.read_text(root_manifest)
        m = _WORKSPACE_MEMBERS_RE.search(text)
        if m:
            workspace_members = re.findall(r'"([^"]+)"', m.group(1))

    git_deps, path_deps = [], []
    for manifest in manifests:
        text = base.read_text(manifest)
        git_deps.extend(_GIT_DEP_RE.findall(text))
        path_deps.extend(_PATH_DEP_RE.findall(text))

    ambiguities = []
    confidence = 1.0
    if len(manifests) == 0:
        confidence = 0.0
    elif root_manifest is None and len(manifests) > 1:
        confidence = 1.0 / len(manifests)
        ambiguities.append("multiple Cargo.toml files and none at the source root; no automatic selection permitted")

    return {
        "ecosystem": ECOSYSTEM,
        "detected_project_roots": sorted({rel(p.parent) for p in manifests}),
        "detected_build_systems": ["cargo"] if manifests else [],
        "candidate_entry_points": [rel(p) for p in manifests],
        "confidence": confidence,
        "ambiguous": len(manifests) != 1 and root_manifest is None,
        "ambiguities": ambiguities,
        "required_toolchain": None,
        "dependency_files": [rel(p) for p in cargo_config],
        "lockfiles": [rel(p) for p in lockfiles],
        "custom_scripts": [rel(p) for p in build_rs],
        "custom_imports": [],
        "network_risk_indicators": [f"git dependency: {d}" for d in git_deps],
        "container_or_external_service_indicators": [],
        "workspace_members": workspace_members,
        "path_dependencies": path_deps,
        "offline_mode_feasible": bool(lockfiles) and not git_deps,
    }


def inspect(source_root: Path, entry_point: str) -> dict:
    manifest_path = Path(source_root) / entry_point
    text = base.read_text(manifest_path)
    has_build_rs = (manifest_path.parent / "build.rs").is_file()
    return {
        "entry_point": entry_point,
        "git_dependencies": _GIT_DEP_RE.findall(text),
        "path_dependencies": _PATH_DEP_RE.findall(text),
        "has_build_rs": has_build_rs,
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
                "argv": ["cargo", "fetch", "--locked", "--manifest-path", entry_point],
                "rationale": "populate the local cargo registry cache (incl. any git dependencies) "
                             "before network isolation closes, so the captured build can run --offline.",
            },
        ],
    }


def plan_untrusted_execution(manifest: dict) -> dict:
    entry_point = manifest["project"]["entry_point"]
    return {
        "argv": ["cargo", "build", "--release", "--locked", "--offline", "--manifest-path", entry_point],
        "network_during_execution": "denied",
    }


def toolchain_identity_contract() -> dict:
    return {
        "requested_source": "rust-toolchain.toml, or the workflow's rustup toolchain input",
        "resolver_mechanism": "rustup toolchain resolution",
        "resolved_fields": ["resolved_version", "runtime_identifier", "resolved_executable_path"],
        "executed_fields": ["executed_argv0", "executed_binary_absolute_path", "executed_binary_sha256"],
        "classification_rule": (
            "resolved_version (rustc --version) must equal the requested channel/pin to classify "
            "exact-match/compatible-resolution; a resolved toolchain from a different channel or "
            "major version than requested is unexpected-resolution."
        ),
    }


def filesystem_policy_hints(manifest: dict) -> dict:
    entry_point = Path(manifest["project"]["entry_point"])
    crate_dir = entry_point.parent
    runtime_knowledge = base.generic_filesystem_runtime_knowledge()
    return {
        "read_only": {"/proc": runtime_knowledge["/proc"] + " (cargo/rustc build-parallelism detection)"},
        "writable": {
            "/tmp": "cargo may use temp files during compilation",
            str(crate_dir / "target"): "build output; must be pre-created before Landlock policy construction",
        },
        "must_pre_create": [str(crate_dir / "target")],
    }


def environment_allowlist(manifest: dict) -> list[str]:
    return ["CARGO_HOME", "RUSTUP_HOME", "PATH", "HOME", "TMPDIR", "CARGO_NET_OFFLINE"]


def network_requirements(manifest: dict) -> dict:
    return {"required_during_trusted_setup": True, "required_during_untrusted_execution": False}


def resource_limit_hints(manifest: dict) -> dict:
    hints = base.generic_resource_limit_hints()
    hints["notes"] = ["rustc/LLVM codegen can be memory- and CPU-heavy; no known incompatibility with "
                       "any specific ulimit has been evidenced yet for this ecosystem."]
    return hints


def receipt_fields(manifest: dict) -> tuple:
    return base.COMMON_RECEIPT_FIELDS


def sanitizer_profile() -> dict:
    profile = base.generic_sanitizer_profile()
    profile["transformations"] = profile["transformations"] + [
        "cargo_compiling_progress_line", "cargo_finished_line",
    ]
    return profile
