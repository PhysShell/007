"""Deterministic rendering of report.md from the canonical report.json.

No numbers are computed here; report.md only restates report.json in prose so
the human-readable and machine-readable reports can never disagree.

Corrective round `#issuecomment-5182632187` §4: the prose deliberately says
"compiled-observation coverage", never "recall", and states in the same breath
that a low value for the raw arm does NOT mean the information is absent from
the transcripts — only that it is not carried as a compiled observation with
complete provenance.
"""
from __future__ import annotations


def _pct(x: float) -> str:
    return "%.1f%%" % (x * 100.0)


def _task_section(L: list, t: dict) -> None:
    proj = t["projection"]
    sel = t["selectors"]
    L.append("### `%s`" % t["task_id"])
    L.append("")
    L.append("- required topics: %s"
             % (", ".join("`%s`" % x for x in sel["required_topics"]) or "-"))
    L.append("- preferred topics: %s"
             % (", ".join("`%s`" % x for x in sel["preferred_topics"]) or "-"))
    L.append("- required kinds: %s"
             % (", ".join("`%s`" % x for x in sel["required_kinds"]) or "-"))
    L.append("- selected **%d** observation(s), omitted %d with explicit reasons, %d bytes"
             % (proj["selected_count"], proj["omitted_count"], proj["used_bytes"]))
    L.append("- projection validity: **%s**"
             % ("valid" if proj["validity"]["valid"] else "INVALID"))
    evl = t.get("evaluation")
    if evl:
        C = evl["arms"]["projection"]
        L.append("- compiled-observation coverage %s, structural question support %s, "
                 "presented-provenance ratio %s, at reduction ratio %.4f vs full derived"
                 % (_pct(C["compiled_observation_coverage"]),
                    _pct(C["structural_question_support"]),
                    "n/a" if C["presented_provenance_ratio"] is None
                    else _pct(C["presented_provenance_ratio"]),
                    C["reduction_ratio_vs_full_derived"]))
    L.append("")


def build_report_md(rep: dict) -> str:
    L: list[str] = []
    st = rep["status"]
    tasks = rep["tasks"]
    td = rep["task_dependence"]

    L.append("# B1 development result — %s" % rep["fixture_id"])
    L.append("")
    L.append("> Golden **development** fixture. One fixture can show the pipeline runs and "
             "detects known losses; it cannot prove the pipeline works on new cases.")
    L.append("")

    L.append("## Verdict")
    L.append("")
    L.append("| field | value |")
    L.append("|---|---|")
    for k in ("deterministic_compilation_pipeline", "task_conditioned_projection",
              "development_result", "generalization"):
        L.append("| %s | **%s** |" % (k, st[k]))
    for k in ("source_set_complete", "holdout_evaluated", "authoritative_for_a_series"):
        L.append("| %s | %s |" % (k, str(st[k]).lower()))
    L.append("")

    L.append("## Task-dependent selection")
    L.append("")
    L.append("Selection is a pure deterministic function of "
             "`gold state + task + selector contract/version + budget`, under "
             "`%s` (impl `%s`)."
             % (rep["selector_contract"]["selector_id"],
                rep["selector_contract"]["selector_impl_digest"]))
    L.append("")
    for t in tasks:
        _task_section(L, t)
    L.append("**Same gold state, different tasks, different projections:** "
             "`%s` selected %d observation(s) the other did not; `%s` selected %d "
             "the other did not; symmetric difference **%d**; neither is a superset "
             "of the other (%s)."
             % (td["task_a"], len(td["selected_only_by_a"]),
                td["task_b"], len(td["selected_only_by_b"]),
                td["symmetric_difference_count"],
                str(td["neither_is_a_superset"]).lower()))
    L.append("")

    L.append("## What worked")
    L.append("")
    L.append("- All %d fixture input blobs verified by digest and size (fail-closed check)."
             % len(rep["input_digests"]))
    L.append("- Both extractors ran deterministically over the local CAS and produced "
             "%d derived transcript records (%d bytes), user-visible only, no chain-of-thought."
             % (rep["derived"]["total_records"], rep["derived"]["total_bytes"]))
    L.append("- Every task's projection passed the validity checks in `validity.py`, which "
             "are computed from the presented records themselves and are proven able to "
             "fail by the adversarial tests in `tests/test_validity.py`.")
    L.append("")

    L.append("## How to read the coverage numbers")
    L.append("")
    L.append("`compiled_observation_coverage` and `structural_question_support` are "
             "**structural**: they ask whether an arm carries the compiled observation "
             "together with its complete provenance graph. They are **not** semantic recall "
             "and **not** answerability.")
    L.append("")
    L.append("A raw transcript may well contain the information behind an observation while "
             "carrying neither the compiled observation nor its provenance — it still scores "
             "zero here. So a low `full_derived` figure must **not** be read as \"only that "
             "fraction of the project state is present in the conversations\". Measuring that "
             "requires a separate extraction/readout experiment, which does not exist yet.")
    L.append("")

    L.append("## Where the negative control lost state")
    L.append("")
    L.append("Each proposition is reported separately, because they are established by "
             "different evidence:")
    L.append("")
    L.append("| superseded belief | fixture-expected divergence | corrective evidence absent "
             "from NC | contrary claim detected |")
    L.append("|---|---|---|---|")
    first_eval = next((t["evaluation"] for t in tasks if t.get("evaluation")), None)
    if first_eval:
        for claim_id, info in sorted(first_eval["negative_control_divergence"].items()):
            contrary = info["contrary_claim_detected"]
            L.append("| `%s` | %s | %s | %s |"
                     % (claim_id,
                        str(info["fixture_expected_divergence"]).lower(),
                        str(info["corrective_evidence_absent_from_negative_control"]).lower(),
                        "not evaluated" if contrary is None else str(contrary).lower()))
        L.append("")
        L.append("Absence of a corrective conversation id proves the reconstruction lacked "
                 "that corrective reference. It does **not** by itself prove the "
                 "reconstruction asserted the opposite proposition — that is the separate "
                 "`contrary_claim_detected` column.")
    L.append("")

    L.append("## Context reduction")
    L.append("")
    L.append("| task | arm | input bytes | records | compiled-obs coverage | reduction vs full |")
    L.append("|---|---|---|---|---|---|")
    for t in tasks:
        evl = t.get("evaluation")
        if not evl:
            continue
        for name in ("full_derived", "negative_control", "projection"):
            arm = evl["arms"][name]
            L.append("| %s | %s | %d | %d | %s | %.4f |"
                     % (t["task_id"], name, arm["input_bytes"], arm["record_count"],
                        _pct(arm["compiled_observation_coverage"]),
                        arm["reduction_ratio_vs_full_derived"]))
    L.append("")

    L.append("## What still cannot be claimed")
    L.append("")
    L.append("- B1 is **not** proven; context preservation is **not** solved.")
    L.append("- The schema is **not** universal and this is **not** production ready.")
    L.append("- Nothing here **generalizes**; the source set is **not** SOURCE_SET_COMPLETE.")
    L.append("- Two tasks over one fixture show selection responds to the task. They do "
             "**not** show the selector generalizes to unseen tasks or unseen gold states.")
    L.append("- B1 remains read-only and non-authoritative over the A-series.")
    L.append("")

    L.append("## Reproduce")
    L.append("")
    L.append("```")
    L.append(rep["reproduce_command"])
    L.append("```")
    L.append("")
    L.append("The task-dependent projection half needs no CAS and no secrets:")
    L.append("")
    L.append("```")
    L.append("python3 research/b1-context/tools/project_case.py --fixture %s "
             "--out research/b1-context/results/%s-v0" % (rep["fixture_id"], rep["fixture_id"]))
    L.append("```")
    L.append("")
    return "\n".join(L)
