#!/usr/bin/env python3
"""N2-B SanitizerContract (section 15).

A generic sanitizer *profile* interface — deliberately not a universal
log-rewriting language. Every transformation must be deterministic, minimal,
test-covered, and justified by an actually-observed volatile field (never
speculative). Structurally forbids the operations the addendum explicitly
names as never acceptable: deleting diagnostics/warnings/errors, deduping or
reordering lines, truncating failure causes, or normalizing for better token
reduction.
"""
from __future__ import annotations

FORBIDDEN_TRANSFORMATION_MARKERS = (
    "dedup", "deduplicate", "reorder", "sort_lines", "truncate",
    "token_reduction", "token_saving", "strip_warning", "strip_error",
    "remove_diagnostic", "remove_error", "remove_warning", "drop_failure_cause",
)

REQUIRED_PROFILE_FIELDS = ("profile_version", "transformations")


def validate_profile(profile: dict) -> list[str]:
    errors = []
    for field in REQUIRED_PROFILE_FIELDS:
        if field not in profile:
            errors.append(f"sanitizer profile missing required field {field!r}")
    transformations = profile.get("transformations", [])
    if not isinstance(transformations, list) or not transformations:
        errors.append("transformations must be a non-empty ordered list")
        return errors
    for name in transformations:
        low = name.lower()
        for marker in FORBIDDEN_TRANSFORMATION_MARKERS:
            if marker in low:
                errors.append(f"transformation {name!r} matches forbidden marker {marker!r} "
                               "(dedup/reorder/truncate/token-reduction transforms are never acceptable)")
    return errors


def transformation_receipt(profile: dict, applied: list[str]) -> dict:
    """A minimal, checkable record of which transformations actually ran,
    for inclusion in a build receipt's `sanitization` section."""
    unknown = [t for t in applied if t not in profile.get("transformations", [])]
    return {
        "profile_version": profile.get("profile_version"),
        "transformations_applied": applied,
        "unknown_transformations": unknown,
        "consistent_with_profile": not unknown,
    }
