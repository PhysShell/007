"""Deterministic rendering of report.md from the canonical report.json.

No numbers are computed here; report.md only restates report.json in prose so
the human-readable and machine-readable reports can never disagree.
"""
from __future__ import annotations


def _pct(x: float) -> str:
    return "%.1f%%" % (x * 100.0)


def build_report_md(rep: dict) -> str:
    L: list[str] = []
    ev = rep["evaluation"]["arms"]
    C = ev["projection"]
    A = ev["full_derived"]
    B = ev["negative_control"]
    st = rep["status"]

    L.append("# B1 development result — case-0001 (v0)")
    L.append("")
    L.append("> Golden **development** fixture. One fixture can show the pipeline runs and "
             "detects known losses; it cannot prove the pipeline works on new cases.")
    L.append("")
    L.append("## Verdict")
    L.append("")
    L.append("| field | value |")
    L.append("|---|---|")
    L.append("| development_result | **%s** |" % st["development_result"])
    L.append("| generalization | %s |" % st["generalization"])
    L.append("| source_set_complete | %s |" % str(st["source_set_complete"]).lower())
    L.append("| holdout_evaluated | %s |" % str(st["holdout_evaluated"]).lower())
    L.append("| authoritative_for_a_series | %s |" % str(st["authoritative_for_a_series"]).lower())
    L.append("")

    L.append("## What worked")
    L.append("")
    L.append("- All %d fixture input blobs verified by digest and size (fail-closed check)."
             % len(rep["input_digests"]))
    L.append("- Both extractors ran deterministically over the local CAS and produced "
             "%d derived transcript records (%d bytes), user-visible only, no chain-of-thought."
             % (rep["derived"]["total_records"], rep["derived"]["total_bytes"]))
    L.append("- The compiled projection (arm C) carried **%s** of required observations "
             "with **%s** question support coverage, at **%s** of the full-derived byte size "
             "(reduction ratio %.4f)."
             % (_pct(C["required_observation_recall"]),
                _pct(C["question_support_coverage"]),
                _pct(C["reduction_ratio_vs_full_derived"]),
                C["reduction_ratio_vs_full_derived"]))
    L.append("- Projection made **%d** unsupported claims, **%d** contradictions, "
             "**%d** supersession errors; provenance coverage %s."
             % (C["unsupported_claim_count"], C["contradiction_count"],
                C["supersession_error_count"], _pct(C["provenance_coverage"])))
    L.append("")

    L.append("## What did not work / limits of this run")
    L.append("")
    L.append("- Arm A (full derived transcripts) carries the raw bytes but **zero** typed, "
             "provenance-bound observations (required-observation recall %s): a dump does not "
             "answer typed questions without the compilation step. Its raw material directly "
             "evidences only %s of required observations (evidence_source_presence)."
             % (_pct(A["required_observation_recall"]), _pct(A["evidence_source_presence"])))
    L.append("- This fixture is a development fixture: its questions were shaped on its own "
             "material, so arm C's high recall debugs the representation and does **not** "
             "demonstrate generalization.")
    L.append("- A ChatGPT account export is still pending; the source set remains "
             "INTERIM_SOURCE_SET_CAPTURED (not complete).")
    L.append("")

    L.append("## Where the negative control lost state")
    L.append("")
    L.append("The sealed negative-control reconstruction (arm B) does not merely omit — it "
             "**contradicts** the captured topology on **%d** required observation(s):"
             % B["contradiction_count"])
    for oid in B["contradicted_required_observation_ids"]:
        L.append("- `%s`" % oid)
    L.append("")
    L.append("Confirmation is digest-bound, not hardcoded:")
    for claim_id, info in sorted(rep["evaluation"]["negative_control_divergence"].items()):
        L.append("- `%s`: asserted_by_negative_control=%s — %s"
                 % (claim_id, str(info["asserted_by_negative_control"]).lower(),
                    info["corroboration"]))
    L.append("")

    L.append("## Context reduction")
    L.append("")
    L.append("| arm | input bytes | records | required recall | reduction vs full |")
    L.append("|---|---|---|---|---|")
    for name, arm in (("full_derived", A), ("negative_control", B), ("projection", C)):
        L.append("| %s | %d | %d | %s | %.4f |"
                 % (name, arm["input_bytes"], arm["record_count"],
                    _pct(arm["required_observation_recall"]),
                    arm["reduction_ratio_vs_full_derived"]))
    L.append("")

    L.append("## What still cannot be claimed")
    L.append("")
    L.append("- B1 is **not** proven; context preservation is **not** solved.")
    L.append("- The schema is **not** universal and this is **not** production ready.")
    L.append("- Nothing here **generalizes**; the source set is **not** SOURCE_SET_COMPLETE.")
    L.append("- B1 remains read-only and non-authoritative over the A-series.")
    L.append("")

    L.append("## Reproduce")
    L.append("")
    L.append("```")
    L.append(rep["reproduce_command"])
    L.append("```")
    L.append("")
    return "\n".join(L)
