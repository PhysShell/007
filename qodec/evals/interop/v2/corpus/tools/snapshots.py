"""Snapshot-manifest construction, verification and bundle path safety.

Canonical corpus evidence is raw.* and rtk.* only. Qodec/VG/hybrid outputs are
derived run artifacts and are rejected inside a case bundle.
"""
from __future__ import annotations

from pathlib import Path

from hashing import sha256_file, tree_sha256

SNAPSHOT_VERSION = "corpus-snapshot-v1"

# Canonical byte layout inside a case bundle.
RAW_STDOUT = "snapshots/raw.stdout"
RAW_STDERR = "snapshots/raw.stderr"
RTK_STDOUT = "snapshots/rtk.stdout"
RTK_STDERR = "snapshots/rtk.stderr"
NATIVE_RECEIPT = "receipts/native.json"
RTK_RECEIPT = "receipts/rtk.json"
FIXTURE_DIR = "fixture"

# Derived artifacts that must never live in a canonical bundle.
FORBIDDEN_DERIVED = {
    "qodec.stdout", "qodec.stderr", "vg.stdout", "hybrid.stdout", "qodec-envelope.json",
}


def _h(path: Path):
    return sha256_file(path) if path.exists() else None


def build_snapshot_manifest(bundle: Path, case: dict) -> dict:
    return {
        "case_id": case["case_id"],
        "snapshot_version": SNAPSHOT_VERSION,
        "raw_stdout_sha256": _h(bundle / RAW_STDOUT),
        "raw_stderr_sha256": _h(bundle / RAW_STDERR),
        "rtk_stdout_sha256": _h(bundle / RTK_STDOUT),
        "rtk_stderr_sha256": _h(bundle / RTK_STDERR),
        "native_receipt_sha256": _h(bundle / NATIVE_RECEIPT),
        "rtk_receipt_sha256": _h(bundle / RTK_RECEIPT),
        "fixture_tree_sha256": tree_sha256(bundle / FIXTURE_DIR),
        "capture_recipe_sha256": _h(bundle / case["capture_recipe_path"]),
        "provenance_sha256": _h(bundle / case["provenance_path"]),
        "evidence_map_sha256": _h(bundle / case["evidence_map_path"]),
    }


def verify_hashes(bundle: Path, case: dict, manifest: dict) -> list[str]:
    """Recompute every pinned hash from committed files and compare."""
    errs = []
    computed = build_snapshot_manifest(bundle, case)
    for key, want in manifest.items():
        if key in ("case_id", "snapshot_version"):
            if want != computed.get(key):
                errs.append(f"snapshot-manifest {key}: {want!r} != computed {computed.get(key)!r}")
            continue
        got = computed.get(key)
        if got is None:
            errs.append(f"snapshot file missing for {key}")
        elif got != want:
            errs.append(f"snapshot hash mismatch for {key}: manifest {want} != file {got}")
    return errs


def check_derived_leakage(bundle: Path) -> list[str]:
    errs = []
    for p in bundle.rglob("*"):
        if p.is_file() and p.name in FORBIDDEN_DERIVED:
            errs.append(f"derived (qodec/VG/hybrid) artifact not allowed in bundle: {p.relative_to(bundle).as_posix()}")
    return errs


def check_path_safety(bundle: Path) -> list[str]:
    """Reject symlinks that escape the bundle and any resolved path outside it."""
    errs = []
    base = bundle.resolve()
    for p in bundle.rglob("*"):
        try:
            resolved = p.resolve()
        except Exception as e:
            errs.append(f"unresolvable path {p}: {e}")
            continue
        if base != resolved and base not in resolved.parents:
            errs.append(f"path escapes bundle root: {p.relative_to(bundle).as_posix()}")
        if p.is_symlink():
            errs.append(f"symlink not allowed in bundle: {p.relative_to(bundle).as_posix()}")
    return errs
