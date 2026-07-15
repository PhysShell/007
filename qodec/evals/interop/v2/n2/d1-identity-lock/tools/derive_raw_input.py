#!/usr/bin/env python3
"""N2-D1b: derive the canonical TEXT input to QODEC/RTK for a non-repository
primary case from its already-frozen N2-C acquisition + source manifest.

`normalized-source.tar` (durable-input-manifest.json's canonical_benchmark_
input_path) is a durable PACKAGING object, not the benchmark's text input.
The real input is defined by the frozen N2-C source manifest's
execution_expectation.extraction_recipe:

  - input_file_identity: path (relative to the acquisition case root) of the
    file to read. When null, the case has exactly one file under source/ in
    its source-file-manifest.json and that file is used (an explicit,
    checkable resolution rule, not a guess -- more than one candidate with a
    null identity is reported as an ambiguity, never picked arbitrarily).
  - archive_member: when non-null, input_file_identity itself is a nested
    archive (tar.gz / zip) and this names the one member to extract from it.
    Verified safe (archive_security.assert_safe) before extraction, exactly
    like the outer acquisition archive.
  - starting_line_or_byte_offset / maximum_extracted_source_bytes: applied
    as bytes[offset : min(EOF, offset + maximum_extracted_source_bytes)],
    verbatim, on whatever bytes step 1-2 selected. No normalization, no
    trimming, no line selection, no offset adjustment.

Every step here is read-only over already-downloaded/verified acquisition
trees; it does not reacquire anything from upstream and does not modify
N2-C's or N2-D0's frozen files.
"""
from __future__ import annotations

import hashlib
import json
import tarfile
import zipfile
from pathlib import Path


class DerivationError(Exception):
    """A real, reportable failure -- never silently worked around."""


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def sha256_file(path: Path) -> str:
    return sha256_bytes(path.read_bytes())


def verify_source_file_manifest(acquisition_root: Path, source_file_manifest: list[dict]) -> list[dict]:
    """Recomputes every listed file's hash and compares. Returns problems
    (empty list means the extracted tree matches what N2-C recorded)."""
    problems = []
    for entry in source_file_manifest:
        path = acquisition_root / entry["path"]
        if not path.is_file():
            problems.append({"path": entry["path"], "problem": "file missing from extracted acquisition tree"})
            continue
        actual = sha256_file(path)
        if actual != entry["sha256"]:
            problems.append({"path": entry["path"], "problem": "sha256 mismatch",
                              "expected": entry["sha256"], "actual": actual})
    return problems


def resolve_input_file_identity(extraction_recipe: dict, source_file_manifest: list[dict]) -> str:
    """input_file_identity if set; otherwise requires exactly one file listed
    in source-file-manifest.json and uses it. Ambiguity (more than one file,
    identity unset) is a DerivationError, never an arbitrary pick."""
    identity = extraction_recipe.get("input_file_identity")
    if identity is not None:
        return identity
    paths = [e["path"] for e in source_file_manifest]
    if len(paths) != 1:
        raise DerivationError(
            f"input_file_identity is null and source-file-manifest.json lists "
            f"{len(paths)} files (expected exactly 1 for an unambiguous null-identity "
            f"resolution): {paths}"
        )
    return paths[0]


def extract_archive_member_bytes(archive_path: Path, member_name: str, archive_security_module) -> bytes:
    """Verifies the nested archive is safe, then extracts exactly one named
    member's raw bytes. Supports .tar / .tar.gz / .tgz and .zip by suffix,
    matching archive_security's own dispatch convention."""
    archive_security_module.assert_safe(archive_path)
    suffixes = "".join(archive_path.suffixes).lower()
    if suffixes.endswith(".zip"):
        with zipfile.ZipFile(archive_path) as zf:
            try:
                return zf.read(member_name)
            except KeyError as e:
                raise DerivationError(f"member {member_name!r} not found in {archive_path}") from e
    # tarfile handles .tar / .tar.gz / .tgz / .tar.bz2 transparently via mode="r:*"
    with tarfile.open(archive_path, mode="r:*") as tf:
        try:
            member = tf.getmember(member_name)
        except KeyError as e:
            raise DerivationError(f"member {member_name!r} not found in {archive_path}") from e
        extracted = tf.extractfile(member)
        if extracted is None:
            raise DerivationError(f"member {member_name!r} in {archive_path} is not a regular file")
        return extracted.read()


def apply_byte_range(data: bytes, offset: int, maximum_extracted_source_bytes: int) -> bytes:
    """bytes[offset : min(EOF, offset + maximum_extracted_source_bytes)],
    verbatim -- Python slicing already clamps to EOF on its own."""
    return data[offset:offset + maximum_extracted_source_bytes]


def derive_raw_input(acquisition_root: Path, source_manifest: dict, archive_security_module) -> dict:
    """Full per-case derivation. `source_manifest` is the frozen N2-C source
    manifest dict (source-manifests/primary/<case>.json content) for one case.
    Returns a result dict with either a derived byte range + hashes, or a
    `problems` list explaining why derivation failed for this case."""
    case_id = source_manifest["case_id"]
    recipe = source_manifest["execution_expectation"]["extraction_recipe"]

    sfm_path = acquisition_root / "source-file-manifest.json"
    source_file_manifest = json.loads(sfm_path.read_text())
    sfm_problems = verify_source_file_manifest(acquisition_root, source_file_manifest)
    if sfm_problems:
        return {"case_id": case_id, "problems": sfm_problems}

    try:
        identity = resolve_input_file_identity(recipe, source_file_manifest)
        source_path = acquisition_root / identity
        if not source_path.is_file():
            raise DerivationError(f"resolved input_file_identity {identity!r} is not a file under {acquisition_root}")

        archive_member = recipe.get("archive_member")
        if archive_member is not None:
            raw_bytes = extract_archive_member_bytes(source_path, archive_member, archive_security_module)
        else:
            raw_bytes = source_path.read_bytes()

        offset = recipe["starting_line_or_byte_offset"]
        max_bytes = recipe["maximum_extracted_source_bytes"]
        selected = apply_byte_range(raw_bytes, offset, max_bytes)
    except DerivationError as e:
        return {"case_id": case_id, "problems": [{"problem": str(e)}]}

    try:
        selected.decode("utf-8")
        utf8_valid = True
    except UnicodeDecodeError as e:
        utf8_valid = False
        utf8_error = str(e)

    result = {
        "case_id": case_id,
        "input_file_identity": identity,
        "archive_member": archive_member,
        "byte_offset": offset,
        "maximum_extracted_source_bytes": max_bytes,
        "derived_byte_size": len(selected),
        "derived_raw_input_sha256": sha256_bytes(selected),
        "utf8_valid": utf8_valid,
        "problems": [],
    }
    if not utf8_valid:
        result["problems"].append({"problem": f"selected bytes are not valid UTF-8: {utf8_error}"})
    return result


def main() -> int:
    import argparse
    import sys

    ap = argparse.ArgumentParser()
    ap.add_argument("--acquisition-root", required=True, help="extracted acquisition case directory (e.g. .../dataset-rtn-traffic-ids)")
    ap.add_argument("--source-manifest", required=True, help="frozen N2-C source-manifests/primary/<case>.json path")
    ap.add_argument("--archive-security-path", required=True)
    ap.add_argument("--out", required=True)
    args = ap.parse_args()

    sys.path.insert(0, str(Path(args.archive_security_path).parent))
    import archive_security  # noqa: E402

    source_manifest = json.loads(Path(args.source_manifest).read_text())
    result = derive_raw_input(Path(args.acquisition_root), source_manifest, archive_security)
    Path(args.out).write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
    print(json.dumps({"case_id": result["case_id"], "problems": result["problems"]}, indent=2))
    return 0 if not result["problems"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
