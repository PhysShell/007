#!/usr/bin/env python3
"""N2-D1b: verifies capture-a/capture-b pairwise reproducibility for one
pilot case's output directories.

Every capture is independently re-derived FROM its own raw execution
evidence before the two captures are ever compared to each other -- this
tool never merely trusts a receipt's own recorded hashes, or compares two
already-produced "canonical" files without proving where their bytes
actually came from. Concretely, for EACH capture this:

  1. validates the receipt against the shared receipt-contract schema;
  2. re-hashes the real raw.stdout/raw.stderr files and checks them against
     the receipt's own stdout_identity/stderr_identity;
  3. selects the raw stream the receipt names as canonical, and checks its
     hash/size against the receipt's raw_selected_stream_identity;
  4. re-derives the expected canonical benchmark input from that raw
     stream -- via `load_and_verify_policy` + `canonicalize_stream` for a
     case in the canonicalization policy's applicable_case_ids, or the raw
     stream itself (capped) for every other case -- and checks the
     REDERIVED bytes against the actual committed canonical-raw-input.bin,
     never the other way around;
  5. checks the receipt's canonicalization_policy_sha256/report_sha256/
     canonical_pre_cap_identity/canonical_raw_input_sha256/byte_size all
     match what was independently recomputed here.

Only once both captures pass their OWN independent verification does this
compare capture-a against capture-b: receipt identity-field agreement,
canonical-byte equality, canonicalizer idempotence, and -- coordinate-safe,
matched by each differing line's own content hash rather than by raw line
number, since a plain line-number union across two independently-diffed
sides is not safe under insertions/deletions/shifts -- confirmation that
every raw difference is explained by that side's own canonicalization
report.
"""
from __future__ import annotations

import difflib
import hashlib
import json
import sys
from pathlib import Path

TOOLS_DIR = Path(__file__).resolve().parent
MINER_TOOLS = TOOLS_DIR.parents[1] / "miner" / "tools"
for p in (MINER_TOOLS, TOOLS_DIR):
    sys.path.insert(0, str(p))

import gradle_canonicalizer  # noqa: E402
import maven_canonicalizer  # noqa: E402
import receipt_contract  # noqa: E402
import vstest_canonicalizer  # noqa: E402

POLICY_PATH = TOOLS_DIR.parent / "capture-canonicalization-policy.json"
VSTEST_POLICY_PATH = TOOLS_DIR.parent / "vstest-capture-canonicalization-policy.json"
GRADLE_POLICY_PATH = TOOLS_DIR.parent / "gradle-capture-canonicalization-policy.json"

# Case-id-scoped dispatch -- mirrors generic_capture.py's own
# CANONICALIZATION_MODULE_BY_CASE_ID single source of truth. Each profile is
# independently self-hash-locked and verified against its OWN canonicalizer
# module's RULES; Maven's, VSTest's, and Gradle's profiles are never
# merged. Reads POLICY_PATH/VSTEST_POLICY_PATH/GRADLE_POLICY_PATH fresh
# (module globals, not baked into a dict at import time) so tests can
# mock.patch.object any path to exercise tamper-detection.
_CANONICALIZER_MODULES_BY_CASE_ID = {
    "repo-docker-java-parser": maven_canonicalizer,
    "repo-kubeops-generator": vstest_canonicalizer,
    "repo-moshi": gradle_canonicalizer,
}


def _canonicalizer_for_case_id(case_id: str):
    module = _CANONICALIZER_MODULES_BY_CASE_ID.get(case_id)
    if module is maven_canonicalizer:
        return module, POLICY_PATH
    if module is vstest_canonicalizer:
        return module, VSTEST_POLICY_PATH
    if module is gradle_canonicalizer:
        return module, GRADLE_POLICY_PATH
    return None, None

# Dotted paths, not whole nested dicts -- sandbox_identity.policy_sha256 in
# particular embeds this job's own absolute work_dir path (capture-a and
# capture-b each get their own runner temp dir) and so is EXPECTED to
# differ even when every semantic field agrees; the receipt's own
# top-level canonical_policy_sha256 (the <WORKDIR>-substituted hash) is
# the field that must actually match across a pair.
IDENTITY_FIELD_PATHS = [
    "case_id",
    "source_identity.commit_sha", "source_identity.archive_sha256",
    "toolchain_resolved.resolved_version", "toolchain_resolved.runtime_identifier",
    "toolchain_executed.executed_binary_sha256",
    "effective_execution_argv",
    "sandbox_identity.sandboy_commit_sha",
    "canonical_policy_sha256",
    "canonical_stream", "canonicalization_policy_sha256", "canonical_input_derivation",
    "raw_capture_content_classification",
    # D1b authorization (2026-07-16): repo-pyflakes' python3 is now a
    # pinned actions/setup-python interpreter, not the runner-ambient one --
    # base AND executed venv interpreter identity must both match exactly
    # across capture-a/capture-b (this is NOT informational-only). Deliberately
    # excludes host_runtime_identifier (platform.platform(), which embeds the
    # runner's own kernel release) -- see toolchain_resolved.runtime_identifier
    # above for the field that IS the strict, comparable Python ABI identity.
    "toolchain_identity_provenance.resolved_base_interpreter_sha256",
    "toolchain_identity_provenance.executed_venv_interpreter_sha256",
]


def _get_path(d: dict, path: str):
    cur = d
    for part in path.split("."):
        if not isinstance(cur, dict) or part not in cur:
            return None
        cur = cur[part]
    return cur

MAX_DIFF_LINES = 200  # bounded diff cap -- see bounded_line_diff


def _sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def _decode(data: bytes) -> str:
    return data.decode("utf-8", errors="replace")


def bounded_line_diff(a_text: str, b_text: str, *, context: int = 1,
                       max_diff_lines: int = MAX_DIFF_LINES) -> tuple[str, bool]:
    """Returns (diff_text, truncated). `max_diff_lines` bounds the TOTAL
    rendered diff (not merely per-hunk context) -- an unbounded diff over a
    massively-diverged pair would otherwise make this report itself
    unbounded."""
    a_lines = a_text.splitlines(keepends=True)
    b_lines = b_text.splitlines(keepends=True)
    diff_lines = list(difflib.unified_diff(a_lines, b_lines, fromfile="capture-a", tofile="capture-b", n=context))
    truncated = len(diff_lines) > max_diff_lines
    if truncated:
        diff_lines = diff_lines[:max_diff_lines]
    return "".join(diff_lines), truncated


def apply_durable_byte_cap(data: bytes) -> bytes:
    return data[:4194304]  # 4 MiB -- same cap as generic_capture.py


class CaptureVerificationFailure(Exception):
    pass


def _raw_selected_stream_path(out_dir: Path, receipt: dict) -> Path:
    return out_dir / ("raw.stdout" if receipt["canonical_stream"] == "stdout" else "raw.stderr")


def verify_one_capture(out_dir: Path) -> dict:
    """Independently re-derives and checks everything about ONE capture
    from its own raw execution evidence. Returns a dict with `valid: bool`,
    the list of `checks` performed (each {name, passed, detail}), and the
    receipt + re-derived canonical bytes for the caller's own pairwise
    comparison (so the pairwise step never has to re-read files itself and
    risk drifting from what was actually verified here)."""
    checks: list[dict] = []

    def _check(name: str, passed: bool, detail: str = "") -> None:
        # `detail` documents the FAILURE explanation -- never shown when the
        # check actually passed, so a passing report never reads as if it
        # were describing a failure.
        checks.append({"name": name, "passed": bool(passed), "detail": "" if passed else detail})

    receipt_path = out_dir / "receipt.json"
    if not receipt_path.is_file():
        _check("receipt_exists", False, f"no receipt.json in {out_dir}")
        return {
            "valid": False, "checks": checks, "receipt": None, "canonical_bytes": None,
            "raw_selected_text": "", "canonicalization_report": None,
        }
    receipt = json.loads(receipt_path.read_text())

    schema_errors = receipt_contract.validate_receipt(receipt)
    _check("receipt_schema_valid", not schema_errors, "; ".join(schema_errors))

    raw_stdout_path, raw_stderr_path = out_dir / "raw.stdout", out_dir / "raw.stderr"
    raw_stdout = raw_stdout_path.read_bytes() if raw_stdout_path.is_file() else b""
    raw_stderr = raw_stderr_path.read_bytes() if raw_stderr_path.is_file() else b""
    _check(
        "raw_stdout_hash_matches_receipt",
        receipt.get("stdout_identity", {}).get("sha256") == _sha256(raw_stdout),
        f"receipt={receipt.get('stdout_identity', {}).get('sha256')} actual={_sha256(raw_stdout)}",
    )
    _check(
        "raw_stderr_hash_matches_receipt",
        receipt.get("stderr_identity", {}).get("sha256") == _sha256(raw_stderr),
        f"receipt={receipt.get('stderr_identity', {}).get('sha256')} actual={_sha256(raw_stderr)}",
    )

    raw_selected_path = _raw_selected_stream_path(out_dir, receipt)
    raw_selected = raw_selected_path.read_bytes() if raw_selected_path.is_file() else b""
    raw_selected_identity = receipt.get("raw_selected_stream_identity", {})
    _check(
        "raw_selected_stream_identity_matches",
        raw_selected_identity.get("sha256") == _sha256(raw_selected)
        and raw_selected_identity.get("byte_size") == len(raw_selected),
        f"receipt={raw_selected_identity} actual_sha256={_sha256(raw_selected)} actual_size={len(raw_selected)}",
    )

    derivation = receipt.get("canonical_input_derivation")
    case_id = receipt.get("case_id")
    expected_pre_cap = raw_selected
    expected_report: dict | None = None

    if derivation == "case-specific-deterministic-canonicalization":
        canon_module, canon_policy_path = _canonicalizer_for_case_id(case_id)
        _check(
            "case_recognized_for_canonicalization",
            canon_module is not None,
            f"case_id={case_id!r} is not registered in any canonicalization profile's dispatch",
        )

        policy = None
        if canon_module is not None:
            try:
                policy = canon_module.load_and_verify_policy(canon_policy_path)
                _check("canonicalization_policy_integrity", True)
            except canon_module.PolicyIntegrityError as e:
                _check("canonicalization_policy_integrity", False, str(e))
                policy = None

        if policy is not None:
            _check(
                "canonicalization_policy_sha256_matches_receipt",
                receipt.get("canonicalization_policy_sha256") == policy["policy_sha256"],
                f"receipt={receipt.get('canonicalization_policy_sha256')} policy={policy['policy_sha256']}",
            )
            _check(
                "case_is_authorized_for_canonicalization",
                case_id in policy["applicable_case_ids"],
                f"case_id={case_id!r} applicable_case_ids={policy['applicable_case_ids']}",
            )
        if canon_module is not None:
            try:
                expected_pre_cap, expected_report = canon_module.canonicalize_stream(raw_selected)
                _check("canonicalization_reproduces_without_error", True)
            except canon_module.CanonicalizerError as e:
                _check("canonicalization_reproduces_without_error", False, str(e))
                expected_pre_cap, expected_report = raw_selected, None

        report_path = out_dir / "canonicalization-report.json"
        if report_path.is_file() and expected_report is not None:
            committed_report = json.loads(report_path.read_text())
            _check(
                "committed_canonicalization_report_matches_rederived",
                committed_report == expected_report,
                "committed canonicalization-report.json differs from a fresh canonicalize_stream(raw) call",
            )
            committed_report_sha256 = _sha256((json.dumps(committed_report, indent=2, sort_keys=True) + "\n").encode())
            _check(
                "canonicalization_report_sha256_matches_receipt",
                receipt.get("canonicalization_report_sha256") == committed_report_sha256,
                f"receipt={receipt.get('canonicalization_report_sha256')} actual={committed_report_sha256}",
            )
        else:
            _check("committed_canonicalization_report_present", False, f"missing {report_path}")
    elif derivation == "raw-capped-stream":
        _check(
            "no_canonicalization_metadata_for_uncanonicalized_case",
            receipt.get("canonicalization_policy_sha256") is None
            and receipt.get("canonicalization_report_sha256") is None,
            "raw-capped-stream case must not carry canonicalization policy/report hashes",
        )
    else:
        _check("canonical_input_derivation_recognized", False, f"unrecognized derivation {derivation!r}")

    expected_canonical = apply_durable_byte_cap(expected_pre_cap)
    canonical_path = out_dir / "canonical-raw-input.bin"
    actual_canonical = canonical_path.read_bytes() if canonical_path.is_file() else b""
    _check(
        "canonical_bytes_are_rederived_from_raw",
        expected_canonical == actual_canonical,
        "the committed canonical-raw-input.bin does not match canonicalize(raw_selected_stream) -- "
        "its bytes cannot be shown to have actually derived from the real raw evidence",
    )
    _check(
        "canonical_pre_cap_identity_matches_receipt",
        receipt.get("canonical_pre_cap_identity", {}).get("sha256") == _sha256(expected_pre_cap)
        and receipt.get("canonical_pre_cap_identity", {}).get("byte_size") == len(expected_pre_cap),
        f"receipt={receipt.get('canonical_pre_cap_identity')}",
    )
    _check(
        "canonical_raw_input_identity_matches_receipt",
        receipt.get("canonical_raw_input_sha256") == _sha256(expected_canonical)
        and receipt.get("canonical_raw_input_byte_size") == len(expected_canonical),
        f"receipt_sha256={receipt.get('canonical_raw_input_sha256')} receipt_size="
        f"{receipt.get('canonical_raw_input_byte_size')} actual_sha256={_sha256(expected_canonical)} "
        f"actual_size={len(expected_canonical)}",
    )
    _check(
        "idempotent_on_its_own_rederived_bytes",
        _is_idempotent(derivation, case_id, expected_canonical),
        "re-canonicalizing the rederived canonical bytes changed them -- not idempotent",
    )

    valid = all(c["passed"] for c in checks)
    return {
        "valid": valid,
        "checks": checks,
        "receipt": receipt,
        "raw_selected_text": _decode(raw_selected),
        "canonical_bytes": actual_canonical,
        "canonicalization_report": expected_report,
    }


def _is_idempotent(derivation: str, case_id: str, canonical_bytes: bytes) -> bool:
    if derivation != "case-specific-deterministic-canonicalization":
        return True  # no canonicalization applied -- vacuously idempotent
    canon_module, _policy_path = _canonicalizer_for_case_id(case_id)
    if canon_module is None:
        return False
    try:
        reencoded, _ = canon_module.canonicalize_stream(canonical_bytes)
    except canon_module.CanonicalizerError:
        return False
    return reencoded == canonical_bytes


def _line_hashes_from_report(report: dict | None) -> set[str]:
    if not report:
        return set()
    return {r["before_line_sha256"] for r in report.get("replacements", [])}


def _uncovered_lines(text_a: str, text_b: str, covered_a: set[str], covered_b: set[str]) -> list[dict]:
    """Every line where capture-a and capture-b's raw text genuinely differ
    (per ONE shared difflib alignment -- never two independently-run,
    swapped-argument SequenceMatchers, which would silently mismatch
    `a`'s opcode indices against `b`'s covered-hash set or vice versa),
    whose own content hash is NOT present in THAT SIDE's own
    canonicalization report. Matched by CONTENT HASH, never by raw line
    number across the two sides, since insertions/deletions/shifts make a
    plain line-number correspondence between two independently-diffed
    sides coordinate-unsafe."""
    a_lines = text_a.splitlines()
    b_lines = text_b.splitlines()
    sm = difflib.SequenceMatcher(a=a_lines, b=b_lines, autojunk=False)
    uncovered = []
    for tag, i1, i2, j1, j2 in sm.get_opcodes():
        if tag == "equal":
            continue
        for idx in range(i1, i2):
            h = _sha256(a_lines[idx].encode("utf-8"))
            if h not in covered_a:
                uncovered.append({"side": "a", "line_number": idx + 1, "sha256": h})
        for idx in range(j1, j2):
            h = _sha256(b_lines[idx].encode("utf-8"))
            if h not in covered_b:
                uncovered.append({"side": "b", "line_number": idx + 1, "sha256": h})
    return uncovered


def verify_pair(dir_a: Path, dir_b: Path) -> dict:
    result_a = verify_one_capture(dir_a)
    result_b = verify_one_capture(dir_b)

    receipt_a, receipt_b = result_a["receipt"], result_b["receipt"]
    identity_mismatches = []
    if receipt_a is not None and receipt_b is not None:
        identity_mismatches = [
            f for f in IDENTITY_FIELD_PATHS if _get_path(receipt_a, f) != _get_path(receipt_b, f)
        ]

    raw_text_a = result_a["raw_selected_text"] if result_a["receipt"] is not None else ""
    raw_text_b = result_b["raw_selected_text"] if result_b["receipt"] is not None else ""
    canonical_a = result_a["canonical_bytes"] or b""
    canonical_b = result_b["canonical_bytes"] or b""
    canonical_text_a, canonical_text_b = _decode(canonical_a), _decode(canonical_b)

    raw_bounded_diff, raw_diff_truncated = bounded_line_diff(raw_text_a, raw_text_b)
    canonical_bounded_diff, canonical_diff_truncated = bounded_line_diff(canonical_text_a, canonical_text_b)
    canonical_bytes_equal = canonical_a == canonical_b

    covered_a = _line_hashes_from_report(result_a["canonicalization_report"])
    covered_b = _line_hashes_from_report(result_b["canonicalization_report"])
    unmatched_raw_diff_lines = _uncovered_lines(raw_text_a, raw_text_b, covered_a, covered_b)

    passed = (
        result_a["valid"]
        and result_b["valid"]
        and not identity_mismatches
        and canonical_bytes_equal
        and not unmatched_raw_diff_lines
    )

    return {
        "report_type": "n2d1b-pilot-pair-reproducibility-report-v2",
        "case_id": (receipt_a or {}).get("case_id") or (receipt_b or {}).get("case_id"),
        "capture_a_verification": {"valid": result_a["valid"], "checks": result_a["checks"]},
        "capture_b_verification": {"valid": result_b["valid"], "checks": result_b["checks"]},
        "identity_mismatches": identity_mismatches,
        "canonical_bytes_equal": canonical_bytes_equal,
        "unmatched_raw_diff_lines": unmatched_raw_diff_lines,
        "raw_bounded_diff": raw_bounded_diff,
        "raw_bounded_diff_truncated": raw_diff_truncated,
        "canonical_bounded_diff": canonical_bounded_diff,
        "canonical_bounded_diff_truncated": canonical_diff_truncated,
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
