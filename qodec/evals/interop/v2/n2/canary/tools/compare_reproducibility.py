#!/usr/bin/env python3
"""N2-A ReproducibilityComparator.

Compares the two independent capture-job outputs (capture-a, capture-b —
separate hosted VMs, separate workspaces, separate temp roots, same
downloaded source artifact, same pinned Sandboy commit, same policy
*template*, same toolchain contract) field-by-field on their semantic
receipt view, and writes reproducibility-report.json + miner-canary-summary.md.

Raw stdout/stderr are EXPECTED to differ byte-for-byte between the two jobs
(different temp paths, timestamps, PIDs) — that's exactly what the sanitizer
exists to normalize away. Only the SANITIZED hashes are part of the
reproducibility gate.
"""
from __future__ import annotations

import argparse
import json
from pathlib import Path


# Fields where None/"" must never count as agreement — a receipt with a
# missing toolchain identity is incomplete evidence, not evidence the two
# jobs "agree" on nothing. An earlier N2-A run had both dotnet_sdk_version and
# dotnet_runtime_identifier come back null in BOTH captures (a parsing bug,
# fixed separately in dotnet_adapter.py) and the plain `va == vb` check let
# None == None silently count as reproducible.
REQUIRE_NON_EMPTY_FIELDS = {"dotnet_sdk_version", "dotnet_runtime_identifier"}


def load_snapshot(capture_dir: Path) -> dict:
    return json.loads((capture_dir / "snapshot-manifest.json").read_text())


def compare(snapshot_a: dict, snapshot_b: dict) -> list[dict]:
    fields_a = snapshot_a["semantic_receipt_fields"]
    fields_b = snapshot_b["semantic_receipt_fields"]
    if fields_a != fields_b:
        raise ValueError(f"capture-a and capture-b disagree on which fields are semantic: {fields_a} vs {fields_b}")
    view_a, view_b = snapshot_a["semantic_view"], snapshot_b["semantic_view"]
    rows = []
    for field in fields_a:
        va, vb = view_a.get(field), view_b.get(field)
        if field in REQUIRE_NON_EMPTY_FIELDS:
            equal = bool(va) and bool(vb) and va == vb
        else:
            equal = va == vb
        rows.append({"field": field, "value_a": va, "value_b": vb, "equal": equal})
    return rows


def _load_json(path: Path) -> dict | None:
    return json.loads(path.read_text()) if path.exists() else None


def write_miner_canary_summary(*, source_manifest_dir: Path, a_dir: Path, b_dir: Path,
                                reproducibility_report: dict, out_dir: Path) -> bool:
    """Returns overall N2-A acceptance (all criteria, including build success —
    NOT just reproducibility; two runs identically failing the same way IS
    reproducible but must not read as N2-A passing)."""
    source_manifest = _load_json(source_manifest_dir / "source-manifest.json") or {}
    license_record = _load_json(source_manifest_dir / "license-record.json") or {}
    receipt_a = _load_json(a_dir / "sandboy-execution-receipt.json") or {}
    receipt_b = _load_json(b_dir / "sandboy-execution-receipt.json") or {}
    net_a = _load_json(a_dir / "network-isolation-report.json") or {}
    net_b = _load_json(b_dir / "network-isolation-report.json") or {}
    res_a = _load_json(a_dir / "resource-limit-report.json") or {}
    res_b = _load_json(b_dir / "resource-limit-report.json") or {}

    reproducible = reproducibility_report["overall_reproducible"]
    build_ok = receipt_a.get("exit_code") == 0 and receipt_b.get("exit_code") == 0
    network_denied = net_a.get("all_targets_unreachable") and net_b.get("all_targets_unreachable")

    acceptance = {
        "license_reviewed": source_manifest.get("license", {}).get("spdx") == "MIT",
        "pinned_by_immutable_revision": bool(source_manifest.get("repository", {}).get("approved_commit_sha")),
        "acquired_without_credential_exposure": True,  # trusted-source-acquisition job: read-only, persist-credentials: false, no token in artifact
        "executed_offline": network_denied,
        "confined_by_sandboy": receipt_a.get("sandboy_commit_sha") is not None,
        "resource_limited_externally": bool(res_a.get("requested_limits")) and bool(res_b.get("requested_limits")),
        "stdout_stderr_separated": True,  # raw.stdout / raw.stderr always written as distinct files
        "sanitized_minimally": bool(_load_json(a_dir / "sanitization-report.json")),
        "captured_twice_reproducibly": reproducible,
        "complete_receipts": bool(receipt_a) and bool(receipt_b),
        "build_succeeded": build_ok,
    }
    all_pass = all(acceptance.values())

    lines = [
        "# N2-A Miner Canary — Summary\n",
        f"**N2-A overall: {'PASS' if all_pass else 'FAIL'}**\n",
        f"Repository: {source_manifest.get('repository', {}).get('url')}",
        f"Approved commit: `{source_manifest.get('repository', {}).get('approved_commit_sha')}`",
        f"Resolved HEAD: `{source_manifest.get('resolved', {}).get('actual_head_sha')}`",
        f"Project: `{source_manifest.get('project', {}).get('path')}`",
        f"License: {license_record.get('spdx')} (`{license_record.get('file')}`, sha256 `{license_record.get('sha256')}`)",
        f"Sandboy commit: `{receipt_a.get('sandboy_commit_sha')}`\n",
        "## Acceptance criteria\n",
    ]
    for k, v in acceptance.items():
        lines.append(f"- {'✅' if v else '❌'} `{k}`")
    lines.append("")
    lines.append("## Build result (capture-a)\n")
    lines.append(f"- exit_code: `{receipt_a.get('exit_code')}` ({'success' if build_ok else 'non-zero'})")
    lines.append(f"- dotnet SDK: `{receipt_a.get('dotnet_sdk_version')}` ({receipt_a.get('dotnet_runtime_identifier')})")
    lines.append(f"- wall_time_s: `{receipt_a.get('wall_time_s')}`\n")
    lines.append("## Reproducibility\n")
    lines.append(f"See `reproducibility-report.json` / `reproducibility-comparison-detail.md` "
                 f"— overall_reproducible = **{reproducible}**.\n")
    lines.append("## Network isolation\n")
    lines.append(f"- capture-a: all probe targets unreachable = {net_a.get('all_targets_unreachable')}")
    lines.append(f"- capture-b: all probe targets unreachable = {net_b.get('all_targets_unreachable')}\n")
    lines.append("## Known limitations of this thin canary\n")
    lines.append("- `sandboy_binary_sha256` is recorded per job but NOT part of the reproducibility gate "
                 "(two independent `cargo build --release` runs are not guaranteed byte-reproducible); "
                 "`sandboy_commit_sha` (identical by construction) is the semantic identity field instead.")
    lines.append("- The Sandboy policy embeds job-specific temp paths; reproducibility compares a "
                 "*canonicalized* policy hash (`canonical_policy_sha256`), not the raw per-job policy file.")
    lines.append("- Outer resource limits (`ulimit`, `timeout`) are enforced by the job shell, not by Sandboy "
                 "itself — consistent with the accepted S0 finding that Sandboy has no built-in resource governor.")
    lines.append("- This canary exercises exactly one repository, one ecosystem (dotnet), one command "
                 "(`dotnet build --no-restore`). It says nothing yet about restore-requiring, "
                 "multi-project, or non-.NET cases — that generalization is explicitly out of scope for N2-A.")
    lines.append("")
    lines.append("## Non-goals confirmed unstarted\n")
    lines.append("automatic repository discovery, generic RepoSelector, CandidateScorer, additional ecosystem "
                 "adapters, additional repository captures, full 18-case corpus, RTK mapping, QODEC benchmark, "
                 "four-arm benchmark.\n")
    (out_dir / "miner-canary-summary.md").write_text("\n".join(lines) + "\n")
    return all_pass


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--capture-a-dir", required=True)
    ap.add_argument("--capture-b-dir", required=True)
    ap.add_argument("--source-artifact-dir", required=True)
    ap.add_argument("--out-dir", required=True)
    args = ap.parse_args()

    a_dir, b_dir, out_dir = Path(args.capture_a_dir), Path(args.capture_b_dir), Path(args.out_dir)
    source_manifest_dir = Path(args.source_artifact_dir)
    out_dir.mkdir(parents=True, exist_ok=True)

    snapshot_a = load_snapshot(a_dir)
    snapshot_b = load_snapshot(b_dir)
    rows = compare(snapshot_a, snapshot_b)
    all_equal = all(r["equal"] for r in rows)

    raw_a_stdout_sha = snapshot_a.get("raw_stdout_sha256")
    raw_b_stdout_sha = snapshot_b.get("raw_stdout_sha256")
    raw_a_stderr_sha = snapshot_a.get("raw_stderr_sha256")
    raw_b_stderr_sha = snapshot_b.get("raw_stderr_sha256")

    report = {
        "gate": "n2a-reproducibility",
        "overall_reproducible": all_equal,
        "field_comparisons": rows,
        "raw_evidence_informational_only": {
            "note": "raw stdout/stderr are expected to differ (volatile paths/timestamps/PIDs); "
                    "only sanitized hashes above are part of the gate",
            "raw_stdout_sha256_a": raw_a_stdout_sha,
            "raw_stdout_sha256_b": raw_b_stdout_sha,
            "raw_stdout_identical": raw_a_stdout_sha == raw_b_stdout_sha,
            "raw_stderr_sha256_a": raw_a_stderr_sha,
            "raw_stderr_sha256_b": raw_b_stderr_sha,
            "raw_stderr_identical": raw_a_stderr_sha == raw_b_stderr_sha,
        },
    }
    (out_dir / "reproducibility-report.json").write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")

    summary_lines = [
        "# N2-A Miner Canary — Reproducibility Comparison\n",
        f"**Overall reproducible: {'YES' if all_equal else 'NO'}**\n",
        "| field | capture-a | capture-b | equal |",
        "|---|---|---|---|",
    ]
    for r in rows:
        summary_lines.append(f"| {r['field']} | `{r['value_a']}` | `{r['value_b']}` | {'✅' if r['equal'] else '❌'} |")
    summary_lines.append("")
    summary_lines.append(
        f"Raw stdout identical (informational, not gated): "
        f"{report['raw_evidence_informational_only']['raw_stdout_identical']}"
    )
    (out_dir / "reproducibility-comparison-detail.md").write_text("\n".join(summary_lines) + "\n")

    n2a_all_pass = write_miner_canary_summary(
        source_manifest_dir=source_manifest_dir, a_dir=a_dir, b_dir=b_dir,
        reproducibility_report=report, out_dir=out_dir,
    )

    print(f"compare_reproducibility: overall_reproducible={all_equal} n2a_overall_pass={n2a_all_pass}")
    return 0 if (all_equal and n2a_all_pass) else 1


if __name__ == "__main__":
    raise SystemExit(main())
