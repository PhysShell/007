"""N2-B generic Gradle (jvm-gradle) ToolAdapter — inspection + planning only.

Never invokes the Gradle Wrapper script itself (section 9: "не выполнять
wrapper scripts в N2-B") — detect()/inspect() only read text.
"""
from __future__ import annotations

import re
from pathlib import Path

from . import base

ECOSYSTEM = "jvm-gradle"

_INCLUDE_BUILD_RE = re.compile(r"includeBuild\s*\(\s*['\"]([^'\"]+)['\"]\s*\)")
_MAVEN_URL_RE = re.compile(r"maven\s*\{\s*url\s*[=(]?\s*['\"]([^'\"]+)['\"]")
_PLUGIN_ID_RE = re.compile(r'id\s*\(?\s*["\']([\w.\-]+)["\']')


def detect(source_root: Path) -> dict:
    source_root = Path(source_root)
    files = base.walk_files(source_root)
    rel = lambda p: str(p.relative_to(source_root))  # noqa: E731

    settings = [f for f in files if f.name in ("settings.gradle", "settings.gradle.kts")]
    build_files = [f for f in files if f.name in ("build.gradle", "build.gradle.kts")]
    wrapper = [f for f in files if f.name in ("gradlew", "gradlew.bat", "gradle-wrapper.properties")]
    init_scripts = [f for f in files if f.name.startswith("init") and f.suffix in (".gradle",) or f.name == "init.gradle.kts"]

    root_settings = next((s for s in settings if s.parent == source_root), None)
    included_builds, extra_repos, plugins = [], [], []
    for f in settings + build_files:
        text = base.read_text(f)
        included_builds.extend(_INCLUDE_BUILD_RE.findall(text))
        for url in _MAVEN_URL_RE.findall(text):
            if "mavenCentral" not in url and "google" not in url:
                extra_repos.append(url)
        plugins.extend(_PLUGIN_ID_RE.findall(text))

    ambiguities = []
    confidence = 1.0
    if len(build_files) == 0:
        confidence = 0.0
    elif root_settings is None and len(build_files) > 1:
        confidence = 1.0 / len(build_files)
        ambiguities.append("multiple build.gradle[.kts] files and no root settings.gradle[.kts]; "
                            "no automatic selection permitted")

    return {
        "ecosystem": ECOSYSTEM,
        "detected_project_roots": sorted({rel(p.parent) for p in build_files}),
        "detected_build_systems": ["gradle"] if build_files or settings else [],
        "candidate_entry_points": [rel(p) for p in build_files],
        "confidence": confidence,
        "ambiguous": len(build_files) != 1 and root_settings is None,
        "ambiguities": ambiguities,
        "required_toolchain": None,
        "dependency_files": [rel(p) for p in settings],
        "lockfiles": [],
        "custom_scripts": [rel(p) for p in init_scripts],
        "custom_imports": [],
        "network_risk_indicators": [f"non-default repository: {u}" for u in extra_repos],
        "container_or_external_service_indicators": [],
        "included_builds": included_builds,
        "plugins": sorted(set(plugins)),
        "has_wrapper": bool(wrapper),
        "offline_mode_feasible": not extra_repos,
    }


def inspect(source_root: Path, entry_point: str) -> dict:
    build_path = Path(source_root) / entry_point
    text = base.read_text(build_path)
    return {
        "entry_point": entry_point,
        "extra_repositories": [u for u in _MAVEN_URL_RE.findall(text)
                                if "mavenCentral" not in u and "google" not in u],
        "plugins": sorted(set(_PLUGIN_ID_RE.findall(text))),
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
    return {
        "steps": [
            {
                "operation": "dependency_realization",
                "argv": ["gradle", "--offline=false", "dependencies", "--refresh-dependencies"],
                "rationale": "populate the local Gradle dependency cache before network isolation closes, "
                             "so the captured build can run with --offline. Invoked via the plain 'gradle' "
                             "binary during planning discussion, never via the repository's own wrapper script.",
            },
        ],
    }


def plan_untrusted_execution(manifest: dict) -> dict:
    return {"argv": ["gradle", "--offline", "build"], "network_during_execution": "denied"}


def toolchain_identity_contract() -> dict:
    return {
        "requested_source": "gradle-wrapper.properties distributionUrl, or the workflow's setup-java/gradle version input",
        "resolver_mechanism": "actions/setup-java + gradle/actions/setup-gradle (or local Gradle resolution)",
        "resolved_fields": ["resolved_version", "runtime_identifier", "resolved_executable_path"],
        "executed_fields": ["executed_argv0", "executed_binary_absolute_path", "executed_binary_sha256"],
        "classification_rule": (
            "resolved Gradle/JDK version must match the requested distributionUrl/version to classify "
            "exact-match/compatible-resolution; a different resolved distribution is unexpected-resolution."
        ),
    }


def filesystem_policy_hints(manifest: dict) -> dict:
    entry_point = Path(manifest["project"]["entry_point"])
    proj_dir = entry_point.parent
    return {
        "read_only": {},
        "writable": {
            "~/.gradle": "Gradle dependency and build cache",
            str(proj_dir / "build"): "build output; must be pre-created before Landlock policy construction",
        },
        "must_pre_create": [str(proj_dir / "build")],
    }


def environment_allowlist(manifest: dict) -> list[str]:
    return ["JAVA_HOME", "GRADLE_OPTS", "GRADLE_USER_HOME", "PATH", "HOME", "TMPDIR"]


def network_requirements(manifest: dict) -> dict:
    return {"required_during_trusted_setup": True, "required_during_untrusted_execution": False}


def resource_limit_hints(manifest: dict) -> dict:
    hints = base.generic_resource_limit_hints()
    hints["notes"] = ["Gradle daemon + JVM heap reservation shares the same RLIMIT_AS suspicion as Maven/JDK."]
    return hints


def receipt_fields(manifest: dict) -> tuple:
    return base.COMMON_RECEIPT_FIELDS


def sanitizer_profile() -> dict:
    profile = base.generic_sanitizer_profile()
    profile["transformations"] = profile["transformations"] + [
        "gradle_build_duration_line", "gradle_progress_line",
    ]
    return profile
