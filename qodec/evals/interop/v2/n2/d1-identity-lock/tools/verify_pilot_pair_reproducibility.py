#!/usr/bin/env python3
"""N2-D1b: verifies capture-a/capture-b pairwise reproducibility for one
pilot case's output directories.

For a case with no canonicalization profile applied (canonical_input_
derivation == "raw-capped-stream"), the canonical benchmark input IS the raw
(capped) selected stream, so "canonical bytes equal" is the same requirement
as exact raw-byte equality.

For repo-docker-java-parser (canonical_input_derivation ==
"case-specific-deterministic-canonicalization"), exact RAW byte equality is
no longer required (see capture-canonicalization-policy.json) -- instead
this verifies:
  - both receipts agree on source/toolchain/argv/sandbox/canonicalization
    identity;
  - canonical-raw-input.bin is byte-identical between capture-a/capture-b;
  - re-running the canonicalizer on each capture's own canonical-raw-
    input.bin changes zero bytes (idempotence);
  - every raw-stream line where capture-a and capture-b actually differ is
    covered by at least one capture's own canonicalization-report.json
    replacement record -- i.e. no unmatched raw difference remains.

Produces a bounded line-level diff at both the raw and canonical level (the
canonical diff must be empty for the pair to pass).
"""
from __future__ import annotations

import difflib
import json
import sys
from pathlib import Path

TOOLS_DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(TOOLS_DIR))
import maven_canonicalizer  # noqa: E402

IDENTITY_FIELDS = [
    "case_id", "source_identity", "toolchain_resolved", "toolchain_executed",
    "effective_execution_argv", "sandbox_identity", "canonical_stream",
    "canonicalization_policy_sha256", "canonical_input_derivation",
]


def _decode(data: bytes) -> str:
    return data.decode("utf-8", errors="replace")


def bounded_line_diff(a_text: str, b_text: str, *, context: int = 1) -> str:
    a_lines = a_text.splitlines(keepends=True)
    b_lines = b_text.splitlines(keepends=True)
    return "".join(difflib.unified_diff(a_lines, b_lines, fromfile="capture-a", tofile="capture-b", n=context))


def differing_line_numbers(a_text: str, b_text: str) -> set[int]:
    """1-indexed line numbers (within `a_text`) that differ from `b_text`,
    per difflib's own opcode-based alignment."""
    a_lines = a_text.splitlines()
    b_lines = b_text.splitlines()
    sm = difflib.SequenceMatcher(a=a_lines, b=b_lines, autojunk=False)
    differing: set[int] = set()
    for tag, i1, i2, _j1, _j2 in sm.get_opcodes():
        if tag != "equal":
            differing.update(range(i1 + 1, i2 + 1))
    return differing


def _raw_selected_stream_path(out_dir: Path, receipt: dict) -> Path:
    return out_dir / ("raw.stdout" if receipt["canonical_stream"] == "stdout" else "raw.stderr")


def verify_pair(dir_a: Path, dir_b: Path) -> dict:
    receipt_a = json.loads((dir_a / "receipt.json").read_text())
    receipt_b = json.loads((dir_b / "receipt.json").read_text())

    identity_mismatches = [f for f in IDENTITY_FIELDS if receipt_a.get(f) != receipt_b.get(f)]

    raw_a = _raw_selected_stream_path(dir_a, receipt_a).read_bytes()
    raw_b = _raw_selected_stream_path(dir_b, receipt_b).read_bytes()
    canonical_a = (dir_a / "canonical-raw-input.bin").read_bytes()
    canonical_b = (dir_b / "canonical-raw-input.bin").read_bytes()

    raw_text_a, raw_text_b = _decode(raw_a), _decode(raw_b)
    canonical_text_a, canonical_text_b = _decode(canonical_a), _decode(canonical_b)

    raw_bounded_diff = bounded_line_diff(raw_text_a, raw_text_b)
    canonical_bounded_diff = bounded_line_diff(canonical_text_a, canonical_text_b)
    canonical_bytes_equal = canonical_a == canonical_b

    def _idempotent(out_dir: Path, receipt: dict, canonical_bytes: bytes) -> bool:
        if not receipt.get("canonicalization_report_sha256"):
            return True  # no canonicalization applied to this case -- vacuously idempotent
        reencoded, _ = maven_canonicalizer.canonicalize_stream(canonical_bytes)
        return reencoded == canonical_bytes

    idempotent_a = _idempotent(dir_a, receipt_a, canonical_a)
    idempotent_b = _idempotent(dir_b, receipt_b, canonical_b)

    raw_diff_lines = differing_line_numbers(raw_text_a, raw_text_b)
    covered_lines: set[int] = set()
    for out_dir in (dir_a, dir_b):
        report_path = out_dir / "canonicalization-report.json"
        if report_path.is_file():
            report = json.loads(report_path.read_text())
            covered_lines.update(r["line_number"] for r in report["replacements"])
    unmatched_raw_diff_lines = sorted(raw_diff_lines - covered_lines)

    passed = (
        not identity_mismatches
        and canonical_bytes_equal
        and idempotent_a
        and idempotent_b
        and not unmatched_raw_diff_lines
    )

    return {
        "report_type": "n2d1b-pilot-pair-reproducibility-report-v1",
        "case_id": receipt_a.get("case_id"),
        "identity_mismatches": identity_mismatches,
        "canonical_bytes_equal": canonical_bytes_equal,
        "idempotent_a": idempotent_a,
        "idempotent_b": idempotent_b,
        "raw_diff_line_count": len(raw_diff_lines),
        "unmatched_raw_diff_lines": unmatched_raw_diff_lines,
        "raw_bounded_diff": raw_bounded_diff,
        "canonical_bounded_diff": canonical_bounded_diff,
        "passed": passed,
    }


def main() -> int:
    if len(sys.argv) != 3:
        print("usage: verify_pilot_pair_reproducibility.py <capture-a-out-dir> <capture-b-out-dir>", file=sys.stderr)
        return 2
    report = verify_pair(Path(sys.argv[1]), Path(sys.argv[2]))
    print(json.dumps(report, indent=2, sort_keys=True))
    return 0 if report["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
