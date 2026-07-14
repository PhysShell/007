"""N2-B generic Maven (jvm-maven) ToolAdapter — inspection + planning only."""
from __future__ import annotations

import re
from pathlib import Path

from . import base

ECOSYSTEM = "jvm-maven"

_MODULE_RE = re.compile(r"<module>([^<]+)</module>")
_REPOSITORY_URL_RE = re.compile(r"<repository>.*?<url>([^<]+)</url>.*?</repository>", re.DOTALL)
_PLUGIN_REPOSITORY_URL_RE = re.compile(r"<pluginRepository>.*?<url>([^<]+)</url>.*?</pluginRepository>", re.DOTALL)


def detect(source_root: Path) -> dict:
    source_root = Path(source_root)
    files = base.walk_files(source_root)
    rel = lambda p: str(p.relative_to(source_root))  # noqa: E731

    poms = [f for f in files if f.name == "pom.xml"]
    wrapper = [f for f in files if f.name in ("mvnw", "mvnw.cmd", "maven-wrapper.properties")]

    root_pom = next((p for p in poms if p.parent == source_root), None)
    modules = []
    extra_repos = []
    if root_pom is not None:
        text = base.read_text(root_pom)
        modules = _MODULE_RE.findall(text)
        extra_repos.extend(_REPOSITORY_URL_RE.findall(text))
        extra_repos.extend(_PLUGIN_REPOSITORY_URL_RE.findall(text))

    ambiguities = []
    confidence = 1.0
    if len(poms) == 0:
        confidence = 0.0
    elif root_pom is None and len(poms) > 1:
        confidence = 1.0 / len(poms)
        ambiguities.append("multiple pom.xml files and none at the source root; no automatic selection permitted")

    return {
        "ecosystem": ECOSYSTEM,
        "detected_project_roots": sorted({rel(p.parent) for p in poms}),
        "detected_build_systems": ["maven"] if poms else [],
        "candidate_entry_points": [rel(p) for p in poms],
        "confidence": confidence,
        "ambiguous": len(poms) != 1 and root_pom is None,
        "ambiguities": ambiguities,
        "required_toolchain": None,
        "dependency_files": [rel(root_pom)] if root_pom is not None else [],
        "lockfiles": [],
        "custom_scripts": [rel(p) for p in wrapper],
        "custom_imports": [],
        "network_risk_indicators": [f"non-central repository: {u}" for u in extra_repos],
        "container_or_external_service_indicators": [],
        "modules": modules,
        "has_wrapper": bool(wrapper),
        "offline_mode_feasible": not extra_repos,
    }


def inspect(source_root: Path, entry_point: str) -> dict:
    pom_path = Path(source_root) / entry_point
    text = base.read_text(pom_path)
    return {
        "entry_point": entry_point,
        "modules": _MODULE_RE.findall(text),
        "extra_repositories": _REPOSITORY_URL_RE.findall(text) + _PLUGIN_REPOSITORY_URL_RE.findall(text),
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
                "argv": ["mvn", "-f", entry_point, "dependency:go-offline", "-B"],
                "rationale": "populate the local Maven repository cache before network isolation closes, "
                             "so the captured build can run with --offline.",
            },
        ],
    }


def plan_untrusted_execution(manifest: dict) -> dict:
    entry_point = manifest["project"]["entry_point"]
    return {"argv": ["mvn", "-f", entry_point, "--offline", "-B", "package"], "network_during_execution": "denied"}


def toolchain_identity_contract() -> dict:
    return {
        "requested_source": "pom.xml maven.compiler.release/source, or the workflow's setup-java version input",
        "resolver_mechanism": "actions/setup-java (or local JDK/Maven resolution, including Maven Wrapper pin)",
        "resolved_fields": ["resolved_version", "runtime_identifier", "resolved_executable_path"],
        "executed_fields": ["executed_argv0", "executed_binary_absolute_path", "executed_binary_sha256"],
        "classification_rule": (
            "resolved JDK version must match the requested release/source version to classify "
            "exact-match/compatible-resolution; a different major JDK version is unexpected-resolution."
        ),
    }


def filesystem_policy_hints(manifest: dict) -> dict:
    entry_point = Path(manifest["project"]["entry_point"])
    proj_dir = entry_point.parent
    return {
        "read_only": {},
        "writable": {
            "~/.m2/repository": "local Maven repository cache",
            str(proj_dir / "target"): "build output; must be pre-created before Landlock policy construction",
        },
        "must_pre_create": [str(proj_dir / "target")],
    }


def environment_allowlist(manifest: dict) -> list[str]:
    return ["JAVA_HOME", "M2_HOME", "MAVEN_OPTS", "PATH", "HOME", "TMPDIR"]


def network_requirements(manifest: dict) -> dict:
    return {"required_during_trusted_setup": True, "required_during_untrusted_execution": False}


def resource_limit_hints(manifest: dict) -> dict:
    hints = base.generic_resource_limit_hints()
    hints["notes"] = ["JVM startup (-Xmx heap reservation) can interact badly with strict virtual-memory "
                       "limits similarly to CoreCLR; treat RLIMIT_AS with the same suspicion until proven safe."]
    return hints


def receipt_fields(manifest: dict) -> tuple:
    return base.COMMON_RECEIPT_FIELDS


def sanitizer_profile() -> dict:
    profile = base.generic_sanitizer_profile()
    profile["transformations"] = profile["transformations"] + [
        "maven_build_timestamp_line", "maven_total_time_line",
    ]
    return profile
