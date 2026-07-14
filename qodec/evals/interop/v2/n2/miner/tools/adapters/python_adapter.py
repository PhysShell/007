"""N2-B generic Python ToolAdapter — inspection + planning only."""
from __future__ import annotations

import re
from pathlib import Path

from . import base

ECOSYSTEM = "python"

_EXT_MODULES_RE = re.compile(r"ext_modules\s*=", re.IGNORECASE)
_CFFI_RE = re.compile(r"\bcffi\b", re.IGNORECASE)


def detect(source_root: Path) -> dict:
    source_root = Path(source_root)
    files = base.walk_files(source_root)
    rel = lambda p: str(p.relative_to(source_root))  # noqa: E731

    pyproject = [f for f in files if f.name == "pyproject.toml"]
    setup_py = [f for f in files if f.name == "setup.py"]
    requirements = [f for f in files if f.name.startswith("requirements") and f.suffix == ".txt"]
    poetry_lock = [f for f in files if f.name == "poetry.lock"]
    uv_lock = [f for f in files if f.name == "uv.lock"]
    pipfile_lock = [f for f in files if f.name == "Pipfile.lock"]
    tox_ini = [f for f in files if f.name == "tox.ini"]
    pytest_ini = [f for f in files if f.name in ("pytest.ini", "conftest.py")]
    pyx_files = [f for f in files if f.suffix == ".pyx"]

    lockfiles = poetry_lock + uv_lock + pipfile_lock
    entry_points = pyproject + setup_py

    native_ext_indicators = [rel(p) for p in pyx_files]
    for sp in setup_py:
        text = base.read_text(sp)
        if _EXT_MODULES_RE.search(text):
            native_ext_indicators.append(f"{rel(sp)}: ext_modules")
        if _CFFI_RE.search(text):
            native_ext_indicators.append(f"{rel(sp)}: cffi")

    ambiguities = []
    confidence = 1.0
    if len(entry_points) == 0:
        confidence = 0.0
    elif len(entry_points) > 1:
        confidence = 1.0 / len(entry_points)
        ambiguities.append(f"{len(entry_points)} candidate entry points (pyproject.toml/setup.py); "
                            "no automatic selection permitted")

    return {
        "ecosystem": ECOSYSTEM,
        "detected_project_roots": sorted({rel(p.parent) for p in entry_points}),
        "detected_build_systems": (["pep517"] if pyproject else []) + (["setuptools"] if setup_py else []),
        "candidate_entry_points": [rel(p) for p in entry_points],
        "confidence": confidence,
        "ambiguous": len(entry_points) != 1,
        "ambiguities": ambiguities,
        "required_toolchain": None,
        "dependency_files": [rel(p) for p in requirements],
        "lockfiles": [rel(p) for p in lockfiles],
        "custom_scripts": [rel(p) for p in setup_py],
        "custom_imports": [],
        "network_risk_indicators": [] if lockfiles else [rel(p) for p in requirements] or ["no lockfile present"],
        "container_or_external_service_indicators": [],
        "test_entry_points": [rel(p) for p in tox_ini + pytest_ini],
        "native_extension_indicators": native_ext_indicators,
        "offline_mode_feasible": bool(lockfiles),
    }


def inspect(source_root: Path, entry_point: str) -> dict:
    entry_path = Path(source_root) / entry_point
    text = base.read_text(entry_path)
    return {
        "entry_point": entry_point,
        "has_ext_modules": bool(_EXT_MODULES_RE.search(text)),
        "has_cffi": bool(_CFFI_RE.search(text)),
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


def _dependency_realization_step(manifest: dict) -> dict:
    lockfiles = manifest.get("dependency_lock", {}).get("files", [])
    if any(f.endswith("poetry.lock") for f in lockfiles):
        return {"operation": "dependency_realization", "argv": ["poetry", "install", "--no-interaction", "--no-root"],
                "rationale": "poetry.lock present; install pinned dependencies before network isolation closes."}
    if any(f.endswith("uv.lock") for f in lockfiles):
        return {"operation": "dependency_realization", "argv": ["uv", "sync", "--locked"],
                "rationale": "uv.lock present; sync pinned dependencies before network isolation closes."}
    if any(f.endswith("Pipfile.lock") for f in lockfiles):
        return {"operation": "dependency_realization", "argv": ["pipenv", "sync"],
                "rationale": "Pipfile.lock present; sync pinned dependencies before network isolation closes."}
    return {"operation": "dependency_realization", "argv": ["pip", "install", "-r", "requirements.txt"],
            "rationale": "no lockfile present; this is an unpinned-dependency network-risk indicator, "
                         "surfaced to eligibility/scoring, not silently accepted."}


def plan_trusted_setup(manifest: dict) -> dict:
    return {"steps": [_dependency_realization_step(manifest)]}


def plan_untrusted_execution(manifest: dict) -> dict:
    return {"argv": ["python", "-m", "pytest", "-q"], "network_during_execution": "denied"}


def toolchain_identity_contract() -> dict:
    return {
        "requested_source": ".python-version file, or the workflow's setup-python version input",
        "resolver_mechanism": "actions/setup-python (or pyenv/local interpreter resolution)",
        "resolved_fields": ["resolved_version", "runtime_identifier", "resolved_executable_path"],
        "executed_fields": ["executed_argv0", "executed_binary_absolute_path", "executed_binary_sha256"],
        "classification_rule": (
            "resolved_version must match the requested version (or the requested minor-version range) "
            "to classify exact-match/compatible-resolution; any other resolved interpreter is "
            "unexpected-resolution."
        ),
    }


def filesystem_policy_hints(manifest: dict) -> dict:
    runtime_knowledge = base.generic_filesystem_runtime_knowledge()
    return {
        "read_only": {"/dev/urandom": runtime_knowledge["/dev/urandom"]},
        "writable": {
            "/tmp": "pip/venv may use temp files during install",
        },
        "must_pre_create": [],
    }


def environment_allowlist(manifest: dict) -> list[str]:
    return ["PATH", "HOME", "TMPDIR", "PYTHONDONTWRITEBYTECODE", "PIP_NO_INPUT", "VIRTUAL_ENV"]


def network_requirements(manifest: dict) -> dict:
    return {"required_during_trusted_setup": True, "required_during_untrusted_execution": False}


def resource_limit_hints(manifest: dict) -> dict:
    hints = base.generic_resource_limit_hints()
    hints["notes"] = ["CPython startup has no known RLIMIT_AS incompatibility; native-extension builds "
                       "(ext_modules/cffi) may need more headroom than pure-Python projects."]
    return hints


def receipt_fields(manifest: dict) -> tuple:
    return base.COMMON_RECEIPT_FIELDS


def sanitizer_profile() -> dict:
    profile = base.generic_sanitizer_profile()
    profile["transformations"] = profile["transformations"] + [
        "pytest_duration_line", "pytest_tmp_path_line",
    ]
    return profile
